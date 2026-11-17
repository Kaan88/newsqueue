#![allow(unsafe_op_in_unsafe_fn)]
//! Native Windows notification-area icon and minimize-to-tray integration.
//!
//! Kept behind `cfg(windows)`; the rest of the app remains usable without it.

#[cfg(any(target_os = "windows", test))]
use crate::model::{format_speed, APP_NAME, VERSION};
#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, AtomicU8, Ordering};
#[cfg(target_os = "windows")]
use std::sync::Mutex;

#[cfg(target_os = "windows")]
static EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
static TRAY_READY: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
static SPEED_IN_TRAY: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
static TRAY_BALLOON: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
static TRAY_SPEED_BPS_BITS: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "windows")]
static TRAY_SPEED_MODE: AtomicU8 = AtomicU8::new(1);
#[cfg(target_os = "windows")]
static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
#[cfg(target_os = "windows")]
static SAVED_PLACEMENT: Mutex<Option<WINDOWPLACEMENT>> = Mutex::new(None);

/// What the app should do when the main window asks to close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseAction {
    Exit,
    HideToTray,
}

/// Decide between a real exit and hiding to the notification area.
///
/// Hiding is only safe when the tray icon exists and the user did not pick
/// Exit from the tray menu, otherwise the app would become unreachable.
pub fn close_action(
    exit_requested: bool,
    minimize_on_close: bool,
    tray_ready: bool,
) -> CloseAction {
    if !exit_requested && minimize_on_close && tray_ready {
        CloseAction::HideToTray
    } else {
        CloseAction::Exit
    }
}

/// Show the current transfer speed in the notification-area tooltip when the
/// user enabled it in Setup.
#[cfg(target_os = "windows")]
pub fn set_options(speed_in_tray: bool, tray_balloon: bool) {
    SPEED_IN_TRAY.store(speed_in_tray, Ordering::Relaxed);
    TRAY_BALLOON.store(tray_balloon, Ordering::Relaxed);
}

#[cfg(not(target_os = "windows"))]
pub fn set_options(_speed_in_tray: bool, _tray_balloon: bool) {}

/// Publish the latest measured speed for the tray timer.
#[cfg(target_os = "windows")]
pub fn set_speed(bps: f64, mode: u8) {
    TRAY_SPEED_BPS_BITS.store(bps.max(0.0).to_bits(), Ordering::Relaxed);
    TRAY_SPEED_MODE.store(mode, Ordering::Relaxed);
}

#[cfg(not(target_os = "windows"))]
pub fn set_speed(_bps: f64, _mode: u8) {}

#[cfg(any(target_os = "windows", test))]
fn should_balloon(enabled: bool, tray_ready: bool) -> bool {
    enabled && tray_ready
}

#[cfg(any(target_os = "windows", test))]
fn tray_tip(speed_bps: f64, speed_mode: u8, show_speed: bool) -> String {
    let base = format!("{APP_NAME} {VERSION}");
    if show_speed && speed_bps > 0.0 {
        format!("{base} - {}", format_speed(speed_bps, speed_mode))
    } else {
        base
    }
}

/// Show a notification-area balloon/toast. No-op when the tray icon is not
/// ready or the user disabled tray balloon hints.
pub fn notify(title: &str, message: &str) {
    #[cfg(target_os = "windows")]
    {
        let force = std::env::var_os("NQ_TRAY_FORCE_BALLOON").is_some();
        if !should_balloon(TRAY_BALLOON.load(Ordering::Relaxed) || force, ready()) {
            return;
        }
        let hwnd = TRAY_HWND.load(Ordering::Relaxed);
        if hwnd == 0 {
            return;
        }
        let result = unsafe {
            let mut data: NOTIFYICONDATAW = std::mem::zeroed();
            data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = hwnd;
            data.uID = 1;
            data.uFlags = NIF_INFO;
            data.dwInfoFlags = NIIF_INFO;
            data.uTimeoutOrVersion = 5_000;
            copy_wide(&mut data.szInfoTitle, title);
            copy_wide(&mut data.szInfo, message);
            Shell_NotifyIconW(NIM_MODIFY, &data)
        };
        // Diagnostic hook used by tray-verify.ps1 to confirm the shell accepted
        // the balloon request.
        if let Ok(path) = std::env::var("NQ_TRAY_BALLOON_FILE") {
            let _ = std::fs::write(path, format!("{result}\n{title}\n{message}"));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (title, message);
    }
}

#[cfg(target_os = "windows")]
pub fn ready() -> bool {
    TRAY_READY.load(Ordering::Relaxed)
}

#[cfg(not(target_os = "windows"))]
pub fn ready() -> bool {
    false
}

#[cfg(target_os = "windows")]
pub fn is_visible(handle: isize) -> bool {
    unsafe { IsWindowVisible(handle) != 0 }
}

#[cfg(not(target_os = "windows"))]
pub fn is_visible(_handle: isize) -> bool {
    false
}

#[cfg(target_os = "windows")]
pub fn exit_requested() -> bool {
    EXIT_REQUESTED.load(Ordering::Relaxed)
}

#[cfg(target_os = "windows")]
pub fn main_window_handle() -> Option<isize> {
    let title = wide(&format!("{APP_NAME} {VERSION}"));
    let handle = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if handle == 0 {
        return None;
    }
    // A second instance must not attach its tray icon to the first instance's
    // window; only accept a window owned by this process.
    let mut owner = 0u32;
    unsafe {
        GetWindowThreadProcessId(handle, &mut owner);
    }
    (owner == unsafe { GetCurrentProcessId() }).then_some(handle)
}

#[cfg(target_os = "windows")]
pub fn show_main_window() {
    if let Some(handle) = main_window_handle() {
        let saved = SAVED_PLACEMENT
            .lock()
            .ok()
            .and_then(|mut guard| guard.take());
        unsafe {
            taskbar_add_tab(handle);
            if let Some(saved) = saved {
                let maximized = saved.showCmd == SW_SHOWMAXIMIZED as u32;
                let mut restore = saved;
                restore.showCmd = if maximized {
                    SW_SHOWMAXIMIZED as u32
                } else {
                    SW_SHOWNORMAL as u32
                };
                SetWindowPlacement(handle, &restore);
                ShowWindow(handle, if maximized { SW_SHOWMAXIMIZED } else { SW_RESTORE });
            } else {
                ShowWindow(handle, SW_RESTORE);
            }
            SetForegroundWindow(handle);
        }
    }
}

#[cfg(target_os = "windows")]
pub fn hide_main_window(handle: isize) {
    unsafe {
        // Already moved off-screen; do not overwrite the real placement.
        if SAVED_PLACEMENT
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
        {
            taskbar_delete_tab(handle);
            return;
        }

        let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
        placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        if GetWindowPlacement(handle, &mut placement) != 0 {
            if placement.showCmd == SW_SHOWMINIMIZED as u32 {
                placement.showCmd = SW_SHOWNORMAL as u32;
            }
            let width = (placement.rcNormalPosition.right - placement.rcNormalPosition.left).max(200);
            let height =
                (placement.rcNormalPosition.bottom - placement.rcNormalPosition.top).max(200);
            if let Ok(mut guard) = SAVED_PLACEMENT.lock() {
                *guard = Some(placement);
            }
            let was_minimized = IsIconic(handle) != 0;
            if was_minimized {
                ShowWindow(handle, SW_RESTORE);
            }
            let moved = SetWindowPos(
                handle,
                0,
                -32_000,
                -32_000,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            debug_log(&format!(
                "hide moved={moved} minimized={was_minimized} width={width} height={height}"
            ));
        } else {
            let mut rect: RECT = std::mem::zeroed();
            GetWindowRect(handle, &mut rect);
            let width = (rect.right - rect.left).max(200);
            let height = (rect.bottom - rect.top).max(200);
            let moved = SetWindowPos(
                handle,
                0,
                -32_000,
                -32_000,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            debug_log(&format!("hide fallback moved={moved}"));
        }
        taskbar_delete_tab(handle);
    }
}

#[cfg(target_os = "windows")]
pub fn is_minimized(handle: isize) -> bool {
    unsafe { IsIconic(handle) != 0 }
}

#[cfg(target_os = "windows")]
pub fn spawn() {
    std::thread::spawn(|| unsafe {
        run_tray();
    });
}

#[cfg(not(target_os = "windows"))]
pub fn spawn() {}

#[cfg(not(target_os = "windows"))]
pub fn exit_requested() -> bool {
    true
}

#[cfg(not(target_os = "windows"))]
pub fn main_window_handle() -> Option<isize> {
    None
}

#[cfg(not(target_os = "windows"))]
pub fn show_main_window() {}

#[cfg(not(target_os = "windows"))]
pub fn hide_main_window(_handle: isize) {}

#[cfg(not(target_os = "windows"))]
pub fn is_minimized(_handle: isize) -> bool {
    false
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrayCommand {
    Show,
    Exit,
    None,
}

#[cfg(target_os = "windows")]
fn command_for_id(id: u32) -> TrayCommand {
    match id {
        1 => TrayCommand::Show,
        2 => TrayCommand::Exit,
        _ => TrayCommand::None,
    }
}

#[cfg(target_os = "windows")]
unsafe fn run_tray() {
    // The eframe window is created after this thread starts, so wait for it.
    let mut main = 0;
    for _ in 0..100 {
        if let Some(handle) = main_window_handle() {
            main = handle;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    if main == 0 {
        return;
    }

    let class_name = wide("NewsQueueTrayWindow");
    let window_title = wide("NewsQueue Tray");
    let instance = GetModuleHandleW(std::ptr::null());
    let window_class = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(tray_window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: 0,
        hCursor: 0,
        hbrBackground: 0,
        lpszMenuName: std::ptr::null(),
        lpszClassName: class_name.as_ptr(),
    };
    RegisterClassW(&window_class);
    let hwnd = CreateWindowExW(
        0,
        class_name.as_ptr(),
        window_title.as_ptr(),
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        instance,
        std::ptr::null_mut(),
    );
    if hwnd == 0 {
        return;
    }

    let mut icon = LoadIconW(instance, 1 as *const u16);
    if icon == 0 {
        icon = LoadIconW(0, 32512 as *const u16); // IDI_APPLICATION
    }
    let mut data: NOTIFYICONDATAW = std::mem::zeroed();
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = 1;
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = WM_TRAY;
    data.hIcon = icon;
    copy_wide(&mut data.szTip, &tray_tip(0.0, 1, false));
    if Shell_NotifyIconW(NIM_ADD, &data) == 0 {
        let _ = DestroyWindow(hwnd);
        return;
    }
    TRAY_HWND.store(hwnd, Ordering::Relaxed);
    TRAY_READY.store(true, Ordering::Relaxed);
    let _ = SetTimer(hwnd, TRAY_TIMER_ID, 1_000, None);

    let mut message: MSG = std::mem::zeroed();
    while GetMessageW(&mut message, 0, 0, 0) > 0 {
        let _ = TranslateMessage(&message);
        DispatchMessageW(&message);
    }
    let _ = KillTimer(hwnd, TRAY_TIMER_ID);
    let _ = Shell_NotifyIconW(NIM_DELETE, &data);
    TRAY_READY.store(false, Ordering::Relaxed);
    TRAY_HWND.store(0, Ordering::Relaxed);
    let _ = DestroyWindow(hwnd);
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn tray_window_proc(
    hwnd: isize,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    if message == WM_TIMER && wparam == TRAY_TIMER_ID {
        update_tray_tip(hwnd);
        return 0;
    }
    if message == WM_TRAY_TEST_BALLOON {
        notify("NewsQueue", "Tray balloon test");
        return 0;
    }
    if message == WM_TRAY {
        match lparam as u32 {
            WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                show_main_window();
                return 0;
            }
            WM_RBUTTONUP => {
                show_tray_menu(hwnd);
                return 0;
            }
            _ => {}
        }
    }
    if message == WM_TRAY_EXIT {
        request_exit();
        return 0;
    }
    DefWindowProcW(hwnd, message, wparam, lparam)
}

#[cfg(target_os = "windows")]
fn request_exit() {
    EXIT_REQUESTED.store(true, Ordering::Relaxed);
    if let Some(main) = main_window_handle() {
        unsafe {
            // A hidden window does not paint, so eframe would never run the
            // close frame. Show it without stealing focus first; the app then
            // sees WM_CLOSE, skips CancelClose and shuts down normally (which
            // saves settings and queue state).
            ShowWindow(main, SW_SHOWNOACTIVATE);
            PostMessageW(main, WM_CLOSE, 0, 0);
        }
    }
}

#[cfg(target_os = "windows")]
unsafe fn show_tray_menu(hwnd: isize) {
    let menu = CreatePopupMenu();
    if menu == 0 {
        return;
    }
    let show = wide("Show NewsQueue");
    let exit = wide("Exit");
    AppendMenuW(menu, MF_STRING, 1, show.as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
    AppendMenuW(menu, MF_STRING, 2, exit.as_ptr());
    let mut point = POINT { x: 0, y: 0 };
    GetCursorPos(&mut point);
    SetForegroundWindow(hwnd);
    let command = command_for_id(TrackPopupMenu(
        menu,
        TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
        point.x,
        point.y,
        0,
        hwnd,
        std::ptr::null(),
    ) as u32);
    DestroyMenu(menu);
    let _ = PostMessageW(hwnd, 0, 0, 0);
    match command {
        TrayCommand::Show => show_main_window(),
        TrayCommand::Exit => {
            let _ = PostMessageW(hwnd, WM_TRAY_EXIT, 0, 0);
        }
        TrayCommand::None => {}
    }
}

#[cfg(target_os = "windows")]
type ComRelease = unsafe extern "system" fn(*mut std::ffi::c_void) -> u32;
#[cfg(target_os = "windows")]
type ComHrInit = unsafe extern "system" fn(*mut std::ffi::c_void) -> i32;
#[cfg(target_os = "windows")]
type ComTab = unsafe extern "system" fn(*mut std::ffi::c_void, isize) -> i32;

#[cfg(target_os = "windows")]
const CLSID_TASKBAR_LIST: GUID = GUID {
    data1: 0x56FD_F344,
    data2: 0xFD6D,
    data3: 0x11D0,
    data4: [0x95, 0x8A, 0x00, 0x60, 0x97, 0xC9, 0xA0, 0x90],
};
#[cfg(target_os = "windows")]
const IID_ITASKBAR_LIST3: GUID = GUID {
    data1: 0xEA1A_FB91,
    data2: 0x9E28,
    data3: 0x4B86,
    data4: [0x90, 0xE9, 0x9E, 0x9F, 0x8A, 0x5E, 0xEF, 0xAF],
};

#[cfg(target_os = "windows")]
unsafe fn taskbar_add_tab(hwnd: isize) {
    taskbar_tab(hwnd, true);
}

#[cfg(target_os = "windows")]
unsafe fn taskbar_delete_tab(hwnd: isize) {
    taskbar_tab(hwnd, false);
}

#[cfg(target_os = "windows")]
unsafe fn taskbar_tab(hwnd: isize, add: bool) {
    let init_result = CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED);
    let initialized = init_result >= 0;
    let mut object: *mut std::ffi::c_void = std::ptr::null_mut();
    let created = CoCreateInstance(
        &CLSID_TASKBAR_LIST,
        std::ptr::null_mut(),
        CLSCTX_INPROC_SERVER,
        &IID_ITASKBAR_LIST3,
        &mut object,
    );
    debug_log(&format!(
        "taskbar init={init_result:#x} create={created:#x} add={add} object={object:p}"
    ));
    if created >= 0 && !object.is_null() {
        let vtable = *(object as *mut *const usize);
        let release: ComRelease = std::mem::transmute(*vtable.add(2));
        let hr_init: ComHrInit = std::mem::transmute(*vtable.add(3));
        let add_tab: ComTab = std::mem::transmute(*vtable.add(4));
        let delete_tab: ComTab = std::mem::transmute(*vtable.add(5));
        let hr = hr_init(object);
        if hr >= 0 {
            let result = if add {
                add_tab(object, hwnd)
            } else {
                delete_tab(object, hwnd)
            };
            debug_log(&format!("taskbar hr={hr:#x} tab={result:#x}"));
        } else {
            debug_log(&format!("taskbar hr_init={hr:#x}"));
        }
        release(object);
    }
    if initialized {
        CoUninitialize();
    }
}

#[cfg(target_os = "windows")]
unsafe fn update_tray_tip(hwnd: isize) {
    let force_speed = std::env::var_os("NQ_TRAY_FORCE_SPEED").is_some();
    let show_speed = SPEED_IN_TRAY.load(Ordering::Relaxed) || force_speed;
    let bps = std::env::var("NQ_TRAY_TEST_SPEED_BPS")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or_else(|| f64::from_bits(TRAY_SPEED_BPS_BITS.load(Ordering::Relaxed)));
    let mode = if force_speed {
        1
    } else {
        TRAY_SPEED_MODE.load(Ordering::Relaxed)
    };
    let tip = tray_tip(bps, mode, show_speed);
    let mut data: NOTIFYICONDATAW = std::mem::zeroed();
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = 1;
    data.uFlags = NIF_TIP;
    copy_wide(&mut data.szTip, &tip);
    let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
    // Diagnostic hook used by tray-verify.ps1 to inspect the rendered tooltip.
    if let Ok(path) = std::env::var("NQ_TRAY_TIP_FILE") {
        let _ = std::fs::write(path, tip);
    }
}

#[cfg(any(target_os = "windows", test))]
fn copy_wide(destination: &mut [u16], value: &str) {
    let encoded: Vec<u16> = value.encode_utf16().collect();
    let count = encoded.len().min(destination.len().saturating_sub(1));
    destination[..count].copy_from_slice(&encoded[..count]);
    for unit in &mut destination[count..] {
        *unit = 0;
    }
}

#[cfg(target_os = "windows")]
fn debug_log(message: &str) {
    if let Ok(path) = std::env::var("NQ_TRAY_DEBUG") {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{message}");
        }
    }
}

#[cfg(target_os = "windows")]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "windows")]
const WM_CLOSE: u32 = 0x0010;
#[cfg(target_os = "windows")]
const WM_LBUTTONUP: u32 = 0x0202;
#[cfg(target_os = "windows")]
const WM_LBUTTONDBLCLK: u32 = 0x0203;
#[cfg(target_os = "windows")]
const WM_RBUTTONUP: u32 = 0x0205;
#[cfg(target_os = "windows")]
const WM_TIMER: u32 = 0x0113;
#[cfg(target_os = "windows")]
const TRAY_TIMER_ID: usize = 1;
#[cfg(target_os = "windows")]
const WM_APP: u32 = 0x8000;
#[cfg(target_os = "windows")]
const WM_TRAY: u32 = WM_APP + 1;
#[cfg(target_os = "windows")]
const WM_TRAY_EXIT: u32 = WM_APP + 2;
#[cfg(target_os = "windows")]
const WM_TRAY_TEST_BALLOON: u32 = WM_APP + 3;
#[cfg(target_os = "windows")]
const NIM_ADD: u32 = 0;
#[cfg(target_os = "windows")]
const NIM_MODIFY: u32 = 1;
#[cfg(target_os = "windows")]
const NIM_DELETE: u32 = 2;
#[cfg(target_os = "windows")]
const NIF_MESSAGE: u32 = 0x0000_0001;
#[cfg(target_os = "windows")]
const NIF_ICON: u32 = 0x0000_0002;
#[cfg(target_os = "windows")]
const NIF_TIP: u32 = 0x0000_0004;
#[cfg(target_os = "windows")]
const NIF_INFO: u32 = 0x0000_0010;
#[cfg(target_os = "windows")]
const NIIF_INFO: u32 = 0x0000_0001;
#[cfg(target_os = "windows")]
const SW_SHOWNORMAL: i32 = 1;
#[cfg(target_os = "windows")]
const SW_SHOWMINIMIZED: i32 = 2;
#[cfg(target_os = "windows")]
const SW_SHOWMAXIMIZED: i32 = 3;
#[cfg(target_os = "windows")]
const SW_SHOWNOACTIVATE: i32 = 4;
#[cfg(target_os = "windows")]
const SW_RESTORE: i32 = 9;
#[cfg(target_os = "windows")]
const SWP_NOZORDER: u32 = 0x0004;
#[cfg(target_os = "windows")]
const SWP_NOACTIVATE: u32 = 0x0010;
#[cfg(target_os = "windows")]
const SWP_SHOWWINDOW: u32 = 0x0040;
#[cfg(target_os = "windows")]
const COINIT_APARTMENTTHREADED: u32 = 0x2;
#[cfg(target_os = "windows")]
const CLSCTX_INPROC_SERVER: u32 = 0x1;
#[cfg(target_os = "windows")]
const MF_STRING: u32 = 0x0000_0000;
#[cfg(target_os = "windows")]
const MF_SEPARATOR: u32 = 0x0000_0800;
#[cfg(target_os = "windows")]
const TPM_RIGHTBUTTON: u32 = 0x0002;
#[cfg(target_os = "windows")]
const TPM_RETURNCMD: u32 = 0x0100;
#[cfg(target_os = "windows")]
const TPM_NONOTIFY: u32 = 0x0080;

#[cfg(target_os = "windows")]
type WndProc = unsafe extern "system" fn(isize, u32, usize, isize) -> isize;

#[cfg(target_os = "windows")]
#[allow(non_snake_case)]
#[repr(C)]
struct WNDCLASSW {
    style: u32,
    lpfnWndProc: Option<WndProc>,
    cbClsExtra: i32,
    cbWndExtra: i32,
    hInstance: isize,
    hIcon: isize,
    hCursor: isize,
    hbrBackground: isize,
    lpszMenuName: *const u16,
    lpszClassName: *const u16,
}

#[cfg(target_os = "windows")]
#[allow(non_snake_case)]
#[repr(C)]
struct NOTIFYICONDATAW {
    cbSize: u32,
    hWnd: isize,
    uID: u32,
    uFlags: u32,
    uCallbackMessage: u32,
    hIcon: isize,
    szTip: [u16; 128],
    dwState: u32,
    dwStateMask: u32,
    szInfo: [u16; 256],
    uTimeoutOrVersion: u32,
    szInfoTitle: [u16; 64],
    dwInfoFlags: u32,
    guidItem: GUID,
    hBalloonIcon: isize,
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct GUID {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct POINT {
    x: i32,
    y: i32,
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RECT {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(target_os = "windows")]
#[allow(non_snake_case)]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct WINDOWPLACEMENT {
    length: u32,
    flags: u32,
    showCmd: u32,
    ptMinPosition: POINT,
    ptMaxPosition: POINT,
    rcNormalPosition: RECT,
}

#[cfg(target_os = "windows")]
#[allow(non_snake_case)]
#[repr(C)]
struct MSG {
    hwnd: isize,
    message: u32,
    wParam: usize,
    lParam: isize,
    time: u32,
    pt: POINT,
    lPrivate: u32,
}

#[cfg(target_os = "windows")]
#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassW(class: *const WNDCLASSW) -> u16;
    fn CreateWindowExW(
        ex_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: isize,
        menu: isize,
        instance: isize,
        param: *mut std::ffi::c_void,
    ) -> isize;
    fn DefWindowProcW(hwnd: isize, message: u32, wparam: usize, lparam: isize) -> isize;
    fn GetMessageW(message: *mut MSG, hwnd: isize, min: u32, max: u32) -> i32;
    fn TranslateMessage(message: *const MSG) -> i32;
    fn DispatchMessageW(message: *const MSG) -> isize;
    fn FindWindowW(class_name: *const u16, window_name: *const u16) -> isize;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    fn ShowWindow(hwnd: isize, command: i32) -> i32;
    fn IsWindowVisible(hwnd: isize) -> i32;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn IsIconic(hwnd: isize) -> i32;
    fn PostMessageW(hwnd: isize, message: u32, wparam: usize, lparam: isize) -> i32;
    fn DestroyWindow(hwnd: isize) -> i32;
    fn SetTimer(
        hwnd: isize,
        id: usize,
        elapse: u32,
        timer_proc: Option<unsafe extern "system" fn(isize, u32, usize, u32)>,
    ) -> usize;
    fn KillTimer(hwnd: isize, id: usize) -> i32;
    fn CreatePopupMenu() -> isize;
    fn AppendMenuW(menu: isize, flags: u32, id: usize, item: *const u16) -> i32;
    fn TrackPopupMenu(
        menu: isize,
        flags: u32,
        x: i32,
        y: i32,
        reserved: i32,
        hwnd: isize,
        rect: *const std::ffi::c_void,
    ) -> i32;
    fn DestroyMenu(menu: isize) -> i32;
    fn GetCursorPos(point: *mut POINT) -> i32;
    fn LoadIconW(instance: isize, name: *const u16) -> isize;
    fn GetModuleHandleW(name: *const u16) -> isize;
    fn GetWindowPlacement(hwnd: isize, placement: *mut WINDOWPLACEMENT) -> i32;
    fn SetWindowPlacement(hwnd: isize, placement: *const WINDOWPLACEMENT) -> i32;
    fn GetWindowRect(hwnd: isize, rect: *mut RECT) -> i32;
    fn SetWindowPos(
        hwnd: isize,
        insert_after: isize,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "ole32")]
unsafe extern "system" {
    fn CoInitializeEx(reserved: *mut std::ffi::c_void, coinit: u32) -> i32;
    fn CoUninitialize();
    fn CoCreateInstance(
        clsid: *const GUID,
        outer: *mut std::ffi::c_void,
        context: u32,
        iid: *const GUID,
        object: *mut *mut std::ffi::c_void,
    ) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "shell32")]
unsafe extern "system" {
    fn Shell_NotifyIconW(message: u32, data: *const NOTIFYICONDATAW) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcessId() -> u32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_action_hides_only_when_the_tray_can_restore_it() {
        assert_eq!(close_action(false, true, true), CloseAction::HideToTray);
        assert_eq!(close_action(true, true, true), CloseAction::Exit);
        assert_eq!(close_action(false, false, true), CloseAction::Exit);
        assert_eq!(close_action(false, true, false), CloseAction::Exit);
    }

    #[test]
    fn tray_tip_shows_speed_only_when_enabled_and_active() {
        let base = format!("{APP_NAME} {VERSION}");
        assert_eq!(tray_tip(0.0, 1, true), base);
        assert_eq!(tray_tip(0.0, 1, false), base);
        let tip = tray_tip(12.5 * 1024.0 * 1024.0, 1, true);
        assert!(tip.starts_with(&base), "{tip}");
        assert!(tip.contains("MiB/s"), "{tip}");
        assert_eq!(tray_tip(12.5 * 1024.0 * 1024.0, 1, false), base);
        assert!(tray_tip(12.5 * 1024.0 * 1024.0, 3, true).contains("Mbit/s"));
    }

    #[test]
    fn balloon_hints_need_both_the_setting_and_a_ready_tray() {
        assert!(should_balloon(true, true));
        assert!(!should_balloon(true, false));
        assert!(!should_balloon(false, true));
    }

    #[test]
    fn copy_wide_truncates_and_terminates() {
        let mut buffer = [0xFFFFu16; 4];
        copy_wide(&mut buffer, "abcdef");
        assert_eq!(buffer, [b'a' as u16, b'b' as u16, b'c' as u16, 0]);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn tray_menu_ids_map_to_actions() {
        assert_eq!(command_for_id(1), TrayCommand::Show);
        assert_eq!(command_for_id(2), TrayCommand::Exit);
        assert_eq!(command_for_id(3), TrayCommand::None);
    }
}
