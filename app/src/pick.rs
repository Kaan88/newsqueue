//! File and folder pickers through the common item dialogs
//! (`IFileOpenDialog` / `IFileSaveDialog`).

use crate::util::wz;
use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::*;

fn path_of(item: &IShellItem) -> Option<String> {
    unsafe {
        let p = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }
}

fn set_folder(dlg: &IFileDialog, start: &str) {
    if start.is_empty() {
        return;
    }
    let z = wz(start);
    unsafe {
        if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(z.as_ptr()), None) {
            let _ = dlg.SetFolder(&item);
        }
    }
}

/// Folder picker starting in `start`.
pub fn folder(owner: HWND, title: &str, start: &str) -> Option<String> {
    unsafe {
        let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let opts = dlg.GetOptions().ok()?;
        dlg.SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
        let t = wz(title);
        let _ = dlg.SetTitle(PCWSTR(t.as_ptr()));
        set_folder(&dlg.cast().ok()?, start);
        dlg.Show(Some(owner)).ok()?;
        path_of(&dlg.GetResult().ok()?)
    }
}

/// Open dialog; `filters` are (name, "*.nzb;*.NZB") pairs.
pub fn open_files(owner: HWND, title: &str, filters: &[(&str, &str)], multi: bool) -> Vec<String> {
    let run = || -> Option<Vec<String>> {
        unsafe {
            let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
            let opts = dlg.GetOptions().ok()?;
            let mut o = opts | FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM;
            if multi {
                o |= FOS_ALLOWMULTISELECT;
            }
            dlg.SetOptions(o).ok()?;
            let t = wz(title);
            let _ = dlg.SetTitle(PCWSTR(t.as_ptr()));
            let names: Vec<(Vec<u16>, Vec<u16>)> = filters.iter().map(|(n, p)| (wz(n), wz(p))).collect();
            let specs: Vec<COMDLG_FILTERSPEC> =
                names.iter().map(|(n, p)| COMDLG_FILTERSPEC { pszName: PCWSTR(n.as_ptr()), pszSpec: PCWSTR(p.as_ptr()) }).collect();
            if !specs.is_empty() {
                let _ = dlg.SetFileTypes(&specs);
            }
            dlg.Show(Some(owner)).ok()?;
            let items = dlg.GetResults().ok()?;
            let n = items.GetCount().ok()?;
            Some((0..n).filter_map(|i| items.GetItemAt(i).ok().and_then(|it| path_of(&it))).collect())
        }
    };
    run().unwrap_or_default()
}

/// Save dialog with a suggested file name.
pub fn save_file(owner: HWND, title: &str, name: &str, filters: &[(&str, &str)], ext: &str) -> Option<String> {
    unsafe {
        let dlg: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let t = wz(title);
        let _ = dlg.SetTitle(PCWSTR(t.as_ptr()));
        let n = wz(name);
        let _ = dlg.SetFileName(PCWSTR(n.as_ptr()));
        let e = wz(ext);
        let _ = dlg.SetDefaultExtension(PCWSTR(e.as_ptr()));
        let names: Vec<(Vec<u16>, Vec<u16>)> = filters.iter().map(|(n, p)| (wz(n), wz(p))).collect();
        let specs: Vec<COMDLG_FILTERSPEC> =
            names.iter().map(|(n, p)| COMDLG_FILTERSPEC { pszName: PCWSTR(n.as_ptr()), pszSpec: PCWSTR(p.as_ptr()) }).collect();
        if !specs.is_empty() {
            let _ = dlg.SetFileTypes(&specs);
        }
        dlg.Show(Some(owner)).ok()?;
        path_of(&dlg.GetResult().ok()?)
    }
}
