//! File and folder pickers through the common item dialogs.

use crate::util::wz;
use std::{collections::BTreeMap, path::Path};
use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
use windows::Win32::System::Com::*;
use windows::Win32::UI::Controls::{
    GetThemeAppProperties, SetThemeAppProperties, SET_THEME_APP_PROPERTIES_FLAGS,
    ALLOW_CONTROLS, ALLOW_NONCLIENT,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR;

/// Common item dialogs cannot be activated with the desktop's theme flags set
/// to zero (CLASS_E_CLASSNOTAVAILABLE). Keep themes enabled for their complete
/// lifetime, then restore the desktop policy on success, cancellation or error.
struct PickerThemes(SET_THEME_APP_PROPERTIES_FLAGS);

impl PickerThemes {
    fn enable() -> Self {
        unsafe {
            let previous = GetThemeAppProperties();
            SetThemeAppProperties(previous | ALLOW_CONTROLS | ALLOW_NONCLIENT);
            Self(previous)
        }
    }
}

impl Drop for PickerThemes {
    fn drop(&mut self) {
        unsafe { SetThemeAppProperties(self.0) }
    }
}

#[derive(Debug)]
struct PickerError {
    operation: &'static str,
    source: windows::core::Error,
}

type PickResult<T> = Result<T, PickerError>;

fn step<T>(operation: &'static str, result: windows::core::Result<T>) -> PickResult<T> {
    result.map_err(|source| PickerError { operation, source })
}

/// Only Show's explicit cancellation HRESULT is a normal dismissal.
fn shown(result: windows::core::Result<()>) -> PickResult<bool> {
    match result {
        Ok(()) => Ok(true),
        Err(error) if error.code() == windows::core::HRESULT::from_win32(ERROR_CANCELLED.0) => Ok(false),
        other => step("Show dialog", other).map(|()| true),
    }
}

fn report<T>(owner: HWND, title: &str, result: PickResult<T>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            let message = format!("{} failed: {}\n\nPlease try again. If this persists, include this error when reporting the problem.", error.operation, error.source);
            crate::with_engine(|e| e.log("Picker", &message));
            crate::form::message(Some(owner), title, &message, MB_ICONERROR);
            None
        }
    }
}

fn path_of(item: &IShellItem) -> PickResult<String> {
    unsafe {
        let p = step("Read selected path", item.GetDisplayName(SIGDN_FILESYSPATH))?;
        let text = step("Decode selected path", p.to_string().map_err(|error| windows::core::Error::new(
            windows::Win32::Foundation::E_INVALIDARG, error.to_string()
        )));
        CoTaskMemFree(Some(p.0 as *const _));
        text
    }
}

fn set_folder(dlg: &IFileDialog, start: &str) {
    if start.is_empty() {
        return;
    }
    let z = wz(start);
    unsafe {
        // A stale initial directory is only a hint; let Windows choose another.
        if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(z.as_ptr()), None) {
            let _ = dlg.SetFolder(&item);
        }
    }
}

const FOLDERS_FILE: &str = "newsqueue-picker-folders.json";

fn folders_at(file: &Path) -> BTreeMap<String,String> {
    std::fs::read(file).ok().and_then(|bytes|serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

fn last_folder(file: &Path, key: &str) -> Option<String> {
    folders_at(file).remove(key).filter(|folder|Path::new(folder).is_dir())
}

fn remember_folder(file: &Path,key: &str,selection: &[String]) -> std::io::Result<()> {
    let Some(folder)=selection.first().and_then(|path|Path::new(path).parent()).filter(|folder|folder.is_dir()) else {return Ok(())};
    let mut folders=folders_at(file);
    folders.insert(key.into(),folder.to_string_lossy().into_owned());
    if let Some(parent)=file.parent(){std::fs::create_dir_all(parent)?;}
    let temporary=file.with_extension("json.tmp");
    std::fs::write(&temporary,serde_json::to_vec_pretty(&folders)?)?;
    std::fs::rename(temporary,file)
}

fn set_filters(dlg: &IFileDialog, filters: &[(&str, &str)]) -> PickResult<()> {
    let names: Vec<_> = filters.iter().map(|(name, pattern)| (wz(name), wz(pattern))).collect();
    let specs: Vec<_> = names.iter().map(|(name, pattern)| COMDLG_FILTERSPEC {
        pszName: PCWSTR(name.as_ptr()), pszSpec: PCWSTR(pattern.as_ptr()),
    }).collect();
    if !specs.is_empty() {
        unsafe { step("Set file filters", dlg.SetFileTypes(&specs))?; }
    }
    Ok(())
}

/// Folder picker starting in `start`.
pub fn folder(owner: HWND, title: &str, start: &str) -> Option<String> {
    let run = || -> PickResult<Option<String>> {
        let _themes = PickerThemes::enable();
        unsafe {
            let dlg: IFileOpenDialog = step("Create folder dialog", CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER))?;
            let opts = step("Read folder options", dlg.GetOptions())?;
            step("Set folder options", dlg.SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM))?;
            let t = wz(title);
            step("Set dialog title", dlg.SetTitle(PCWSTR(t.as_ptr())))?;
            set_folder(&step("Access folder dialog", dlg.cast())?, start);
            if !shown(dlg.Show(Some(owner)))? { return Ok(None); }
            path_of(&step("Read selected folder", dlg.GetResult())?).map(Some)
        }
    };
    report(owner, title, run()).flatten()
}

/// Open dialog; `filters` are (name, "*.nzb;*.NZB") pairs.
pub fn open_files(owner: HWND, title: &str, filters: &[(&str, &str)], multi: bool) -> Vec<String> {
    // Separate histories keep choosing an unpack tool from moving the NZB picker.
    let history=crate::model::app_base().join(FOLDERS_FILE);
    let run = || -> PickResult<Vec<String>> {
        let _themes = PickerThemes::enable();
        unsafe {
            let dlg: IFileOpenDialog = step("Create open dialog", CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER))?;
            let opts = step("Read open options", dlg.GetOptions())?;
            let mut opts = opts | FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM;
            if multi { opts |= FOS_ALLOWMULTISELECT; }
            step("Set open options", dlg.SetOptions(opts))?;
            let t = wz(title);
            step("Set dialog title", dlg.SetTitle(PCWSTR(t.as_ptr())))?;
            set_filters(&step("Access open dialog", dlg.cast())?, filters)?;
            if let Some(folder)=last_folder(&history,title) {
                set_folder(&step("Access open dialog",dlg.cast())?,&folder);
            }
            if !shown(dlg.Show(Some(owner)))? { return Ok(vec![]); }
            let items = step("Read selected files", dlg.GetResults())?;
            let count = step("Count selected files", items.GetCount())?;
            (0..count).map(|i| path_of(&step("Read selected file", items.GetItemAt(i))?)).collect()
        }
    };
    let selected=report(owner, title, run()).unwrap_or_default();
    // Cancellation and failures leave the previous folder untouched.
    if !selected.is_empty() && !crate::ui(|u|u.demo) {
        if remember_folder(&history,title,&selected).is_err() {
            crate::with_engine(|e|e.log("Picker","Could not save the last opened folder."));
        }
    }
    selected
}

/// Save dialog with a suggested file name.
pub fn save_file(owner: HWND, title: &str, name: &str, filters: &[(&str, &str)], ext: &str) -> Option<String> {
    let run = || -> PickResult<Option<String>> {
        let _themes = PickerThemes::enable();
        unsafe {
            let dlg: IFileSaveDialog = step("Create save dialog", CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER))?;
            let t = wz(title);
            step("Set dialog title", dlg.SetTitle(PCWSTR(t.as_ptr())))?;
            let n = wz(name);
            step("Set suggested name", dlg.SetFileName(PCWSTR(n.as_ptr())))?;
            let e = wz(ext);
            step("Set default extension", dlg.SetDefaultExtension(PCWSTR(e.as_ptr())))?;
            set_filters(&step("Access save dialog", dlg.cast())?, filters)?;
            if !shown(dlg.Show(Some(owner)))? { return Ok(None); }
            path_of(&step("Read saved file path", dlg.GetResult())?).map(Some)
        }
    };
    report(owner, title, run()).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_history_survives_reload_and_ignores_cancel_and_stale_paths() {
        let root=std::env::temp_dir().join(format!("nq-picker-history-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(root.join("one")).unwrap();
        std::fs::create_dir_all(root.join("two")).unwrap();
        let file=root.join(FOLDERS_FILE);
        let selected=vec![root.join("one/a.nzb").to_string_lossy().into_owned(),root.join("one/b.nzb").to_string_lossy().into_owned()];
        remember_folder(&file,"NZB",&selected).unwrap();
        assert_eq!(last_folder(&file,"NZB"),Some(root.join("one").to_string_lossy().into_owned()));
        remember_folder(&file,"NZB",&[]).unwrap();
        remember_folder(&file,"Tools",&[root.join("two/tool.exe").to_string_lossy().into_owned()]).unwrap();
        assert_eq!(last_folder(&file,"NZB"),Some(root.join("one").to_string_lossy().into_owned()));
        remember_folder(&file,"NZB",&[root.join("two/new.nzb").to_string_lossy().into_owned()]).unwrap();
        assert_eq!(last_folder(&file,"NZB"),Some(root.join("two").to_string_lossy().into_owned()));
        std::fs::remove_dir(root.join("two")).unwrap();
        assert_eq!(last_folder(&file,"NZB"),None);
        std::fs::write(&file,b"invalid json").unwrap();
        assert_eq!(last_folder(&file,"NZB"),None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancellation_is_distinct_from_picker_failure() {
        assert!(shown(Ok(())).unwrap());
        assert!(!shown(Err(windows::core::Error::from_hresult(
            windows::core::HRESULT::from_win32(ERROR_CANCELLED.0)
        ))).unwrap());
        let error = shown(Err(windows::core::Error::from_hresult(
            windows::core::HRESULT(0x80040111u32 as i32)
        ))).unwrap_err();
        assert_eq!(error.operation, "Show dialog");
    }

    #[test]
    fn real_dialog_factories_work_with_classic_theme_policy() {
        // Exercise the real failing COM boundary, not a mocked dialog. No UI is
        // shown. This test also covers the shared folder and export factories.
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
            let original = PickerThemes(GetThemeAppProperties());
            SetThemeAppProperties(SET_THEME_APP_PROPERTIES_FLAGS(0));
            {
                let _themes = PickerThemes::enable();
                let open: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).unwrap();
                open.SetOptions(open.GetOptions().unwrap() | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).unwrap();
                let folder = std::env::temp_dir();
                set_folder(&open.cast().unwrap(), &folder.to_string_lossy());
                let actual = path_of(&open.GetFolder().unwrap()).unwrap();
                assert_eq!(std::fs::canonicalize(actual).unwrap(), std::fs::canonicalize(folder).unwrap());
                let _: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).unwrap();
            }
            assert_eq!(GetThemeAppProperties().0, 0);
            let fail = || -> PickResult<()> {
                let _themes = PickerThemes::enable();
                step("Synthetic failure", Err(windows::core::Error::from_hresult(
                    windows::core::HRESULT(0x80004005u32 as i32)
                )))
            };
            assert!(fail().is_err());
            assert_eq!(GetThemeAppProperties().0, 0);
            drop(original);
            CoUninitialize();
        }
    }
}
