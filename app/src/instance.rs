//! Per-profile desktop instance lock and bounded NZB handoff. No engine starts
//! until the lock is acquired; diagnostic/demo windows bypass it.
use std::{fs::{File, OpenOptions}, io, os::windows::fs::OpenOptionsExt, path::Path};
use sha2::{Digest, Sha256};
use windows::{core::PCWSTR, Win32::{Foundation::*, System::DataExchange::COPYDATASTRUCT, UI::WindowsAndMessaging::*}};

const HANDOFF: usize = 0x4e514e5a;
pub const OPEN_PENDING: u32 = WM_APP + 2;
thread_local! {
    static PENDING: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub fn lock(base: &Path) -> io::Result<File> {
    std::fs::create_dir_all(base)?;
    OpenOptions::new().read(true).write(true).create(true).truncate(false)
        .share_mode(0).open(base.join("newsqueue-instance.lock"))
}

fn property(base: &Path) -> Vec<u16> {
    let base = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
    let digest = Sha256::digest(base.to_string_lossy().to_lowercase().as_bytes());
    format!("NewsQueue.Profile.{digest:x}").encode_utf16().chain(Some(0)).collect()
}

pub unsafe fn register(hwnd: HWND, base: &Path) {
    let key = property(base);
    let _ = SetPropW(hwnd, PCWSTR(key.as_ptr()), Some(HANDLE(1usize as _)));
}

pub fn forward(base: &Path, paths: &[String]) -> bool {
    let paths: Vec<String> = paths.iter().map(|p| {
        std::path::absolute(p).unwrap_or_else(|_| p.into()).to_string_lossy().into_owned()
    }).collect();
    let data = serde_json::to_vec(&paths).unwrap_or_default();
    if data.len() > 1024 * 1024 { return false; }
    let key = property(base);
    // A concurrent first launch may still be constructing its main window.
    for _ in 0..50 {
        unsafe {
            let mut after = None;
            while let Ok(hwnd) = FindWindowExW(None, after, windows::core::w!("NewsQueueMain"), None) {
                after = Some(hwnd);
                if GetPropW(hwnd, PCWSTR(key.as_ptr())).0.is_null() { continue; }
                let cds = COPYDATASTRUCT { dwData: HANDOFF, cbData: data.len() as u32, lpData: data.as_ptr() as _ };
                let mut result = 0;
                SendMessageTimeoutW(hwnd, WM_COPYDATA, WPARAM(0), LPARAM(&cds as *const _ as isize),
                    SMTO_ABORTIFHUNG | SMTO_BLOCK, 3000, Some(&mut result));
                return result == 1;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}

pub unsafe fn receive(hwnd: HWND, lp: LPARAM) -> LRESULT {
    if lp.0 == 0 { return LRESULT(0); }
    let cds = &*(lp.0 as *const COPYDATASTRUCT);
    if cds.dwData != HANDOFF || cds.cbData > 1024 * 1024 || cds.lpData.is_null() { return LRESULT(0); }
    let bytes = std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize);
    let Ok(paths) = serde_json::from_slice::<Vec<String>>(bytes) else { return LRESULT(0); };
    PENDING.with(|p| p.borrow_mut().extend(paths));
    let _ = PostMessageW(Some(hwnd), OPEN_PENDING, WPARAM(0), LPARAM(0));
    LRESULT(1)
}

pub fn take_pending() -> Vec<String> { PENDING.with(|p| std::mem::take(&mut *p.borrow_mut())) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handoff_child() {
        let Some(base) = std::env::var_os("NQ_INSTANCE_TEST_PROFILE") else { return; };
        let base = std::path::PathBuf::from(base);
        assert_eq!(lock(&base).unwrap_err().raw_os_error(), Some(32));
        assert!(forward(&base, &[base.join("Unicode ü sample.nzb").to_string_lossy().into_owned()]));
    }

    #[test]
    fn second_process_forwards_files_to_hidden_window() {
        unsafe extern "system" fn proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
            if m == WM_COPYDATA { receive(h, l) } else { DefWindowProcW(h, m, w, l) }
        }
        let root = std::env::temp_dir().join(format!("nq-handoff-test-{}", std::process::id()));
        let guard = lock(&root).unwrap();
        unsafe {
            let name = windows::core::w!("NewsQueueMain");
            let wc = WNDCLASSW { lpfnWndProc: Some(proc), lpszClassName: name, ..Default::default() };
            assert_ne!(RegisterClassW(&wc), 0);
            let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), name, name, WS_OVERLAPPED,
                0, 0, 100, 100, None, None, None, None).unwrap();
            register(hwnd, &root);
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "instance::tests::handoff_child", "--nocapture"])
                .env("NQ_INSTANCE_TEST_PROFILE", &root).spawn().unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() { DispatchMessageW(&msg); }
                if let Some(status) = child.try_wait().unwrap() { assert!(status.success()); break; }
                if std::time::Instant::now() > deadline { child.kill().ok(); panic!("handoff timed out"); }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(!IsWindowVisible(hwnd).as_bool());
            assert_eq!(take_pending(), vec![root.join("Unicode ü sample.nzb").to_string_lossy().into_owned()]);
            DestroyWindow(hwnd).unwrap();
            UnregisterClassW(name, None).unwrap();
        }
        drop(guard);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn lock_excludes_same_profile_and_releases_on_drop() {
        let root = std::env::temp_dir().join(format!("nq-instance-test-{}", std::process::id()));
        let first = lock(&root).unwrap();
        assert_eq!(lock(&root).unwrap_err().raw_os_error(), Some(32));
        let other = lock(&root.join("other")).unwrap();
        drop(first);
        let again = lock(&root).unwrap();
        drop((again, other));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn older_settings_enable_single_instance_and_explicit_false_survives() {
        let defaults: newsqueue_core::model::Settings = serde_json::from_str("{}").unwrap();
        assert!(defaults.single_instance);
        let disabled: newsqueue_core::model::Settings = serde_json::from_str(r#"{"single_instance":false}"#).unwrap();
        assert!(!disabled.single_instance);
    }
}
