//! Windows 2000 colours for this process only.
//!
//! With visual styles off for the client area the common controls draw
//! their 3D look from `GetSysColor`/`GetSysColorBrush`, which on Windows
//! 10/11 return a pale grey face and a bright blue selection. `install`
//! points the import-table entries of those two functions, in this exe and
//! every module loaded so far (comctl32 above all), at lookups that answer
//! the "Windows Standard" scheme of Windows 2000 (face D4D0C8, selection
//! 0A246A) and fall back to the system for anything else. Nothing outside
//! the process changes: `SetSysColors` is never called.
//!
//! Drawing done inside user32/win32k itself (scroll bars, `DrawEdge`, the
//! title bar) still uses the system colours; the menu bar gets the face
//! colour through `face_menus`.

use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use windows::Win32::Foundation::{COLORREF, HANDLE, HMODULE};
use windows::Win32::Graphics::Gdi::{CreateSolidBrush, HBRUSH};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Memory::{PAGE_PROTECTION_FLAGS, PAGE_READWRITE, VirtualProtect};
use windows::Win32::System::ProcessStatus::EnumProcessModules;
use windows::Win32::UI::WindowsAndMessaging::{HMENU, MENUINFO, MIM_APPLYTOSUBMENUS, MIM_BACKGROUND, SetMenuInfo};
use windows::core::{s, w};

const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

/// The Windows 2000 "Windows Standard" colour for a `COLOR_*` index.
pub fn classic(index: i32) -> Option<u32> {
    Some(match index {
        0 => rgb(0xD4, 0xD0, 0xC8),         // SCROLLBAR
        1 => rgb(0x3A, 0x6E, 0xA5),         // BACKGROUND (desktop)
        4 => rgb(0xD4, 0xD0, 0xC8),         // MENU
        5 => rgb(0xFF, 0xFF, 0xFF),         // WINDOW
        6 | 7 | 8 | 18 | 23 => 0,           // frame, menu/window/button/info text
        13 | 30 => rgb(0x0A, 0x24, 0x6A),   // HIGHLIGHT, MENUHILIGHT
        14 => rgb(0xFF, 0xFF, 0xFF),        // HIGHLIGHTTEXT
        15 | 22 => rgb(0xD4, 0xD0, 0xC8),   // BTNFACE, 3DLIGHT
        16 | 17 => rgb(0x80, 0x80, 0x80),   // BTNSHADOW, GRAYTEXT
        20 => rgb(0xFF, 0xFF, 0xFF),        // BTNHIGHLIGHT
        21 => rgb(0x40, 0x40, 0x40),        // 3DDKSHADOW
        24 => rgb(0xFF, 0xFF, 0xE1),        // INFOBK
        26 => rgb(0x00, 0x00, 0x80),        // HOTLIGHT
        _ => return None,
    })
}

static ORIG_COLOR: AtomicUsize = AtomicUsize::new(0);
static ORIG_BRUSH: AtomicUsize = AtomicUsize::new(0);
static ORIG_FILL: AtomicUsize = AtomicUsize::new(0);
static BRUSHES: [AtomicIsize; 32] = [const { AtomicIsize::new(0) }; 32];

type ColorFn = unsafe extern "system" fn(i32) -> u32;
type BrushFn = unsafe extern "system" fn(i32) -> isize;

unsafe extern "system" fn get_sys_color(index: i32) -> u32 {
    if let Some(c) = classic(index) {
        return c;
    }
    let f: ColorFn = std::mem::transmute(ORIG_COLOR.load(Ordering::Relaxed));
    f(index)
}

unsafe extern "system" fn get_sys_color_brush(index: i32) -> isize {
    if let (Some(c), Some(slot)) = (classic(index), BRUSHES.get(index as usize)) {
        let b = slot.load(Ordering::Relaxed);
        if b != 0 {
            return b;
        }
        // System colour brushes are never freed; neither are these.
        let b = CreateSolidBrush(COLORREF(c)).0 as isize;
        return match slot.compare_exchange(0, b, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => b,
            Err(existing) => existing,
        };
    }
    let f: BrushFn = std::mem::transmute(ORIG_BRUSH.load(Ordering::Relaxed));
    f(index)
}

type FillFn = unsafe extern "system" fn(isize, *const std::ffi::c_void, isize) -> i32;

/// `FillRect` with `(HBRUSH)(COLOR_x + 1)`, which user32 resolves itself.
unsafe extern "system" fn fill_rect(dc: isize, rect: *const std::ffi::c_void, brush: isize) -> i32 {
    let brush = if (1..=31).contains(&brush) && classic((brush - 1) as i32).is_some() { get_sys_color_brush((brush - 1) as i32) } else { brush };
    let f: FillFn = std::mem::transmute(ORIG_FILL.load(Ordering::Relaxed));
    f(dc, rect, brush)
}

/// The brush `GetSysColorBrush` now answers for `index`.
pub fn brush(index: i32) -> HBRUSH {
    HBRUSH(unsafe { get_sys_color_brush(index) } as *mut _)
}

/// Redirects every import-table slot holding `from` to `to` in `module`.
unsafe fn patch_module(module: HMODULE, pairs: &[(usize, usize)], names: &[(&str, usize)]) -> usize {
    let base = module.0 as usize;
    let rd32 = |off: usize| (base + off as usize) as *const u32;
    if *(base as *const u16) != 0x5A4D {
        return 0; // "MZ"
    }
    let nt = base + *rd32(0x3C) as usize;
    if *(nt as *const u32) != 0x0000_4550 {
        return 0; // "PE\0\0"
    }
    let optional = nt + 24;
    if *(optional as *const u16) != 0x20B {
        return 0; // PE32+ only
    }
    // Data directory 1 (imports) of a PE32+ optional header.
    let import_rva = *((optional + 112 + 8) as *const u32) as usize;
    if import_rva == 0 {
        return 0;
    }
    let mut patched = 0;
    let mut desc = base + import_rva;
    loop {
        let name = *((desc + 12) as *const u32);
        let first_thunk = *((desc + 16) as *const u32) as usize;
        if name == 0 && first_thunk == 0 {
            break;
        }
        if first_thunk != 0 {
            let mut slot = (base + first_thunk) as *mut usize;
            while *slot != 0 {
                if let Some(&(_, to)) = pairs.iter().find(|(from, _)| *from == *slot) {
                    let mut old = PAGE_PROTECTION_FLAGS(0);
                    if VirtualProtect(slot as *const _, 8, PAGE_READWRITE, &mut old).is_ok() {
                        *slot = to;
                        let _ = VirtualProtect(slot as *const _, 8, old, &mut old);
                        patched += 1;
                    }
                }
                slot = slot.add(1);
            }
        }
        desc += 20;
    }
    // Delay-loaded imports (directory 13): the slots may still hold the
    // loader's thunks, so match them by name.
    let delay_rva = *((optional + 112 + 13 * 8) as *const u32) as usize;
    if delay_rva != 0 {
        let mut d = base + delay_rva;
        loop {
            let dll = *((d + 4) as *const u32);
            let iat = *((d + 12) as *const u32) as usize;
            let int = *((d + 16) as *const u32) as usize;
            if dll == 0 {
                break;
            }
            if iat != 0 && int != 0 {
                let mut k = 0;
                loop {
                    let entry = *((base + int + k * 8) as *const u64);
                    if entry == 0 {
                        break;
                    }
                    if entry & (1 << 63) == 0 {
                        let name = std::ffi::CStr::from_ptr((base + entry as usize + 2) as *const std::ffi::c_char);
                        if let Some(&(_, to)) = names.iter().find(|(n, _)| name.to_bytes() == n.as_bytes()) {
                            let slot = (base + iat + k * 8) as *mut usize;
                            let mut old = PAGE_PROTECTION_FLAGS(0);
                            if VirtualProtect(slot as *const _, 8, PAGE_READWRITE, &mut old).is_ok() {
                                *slot = to;
                                let _ = VirtualProtect(slot as *const _, 8, old, &mut old);
                                patched += 1;
                            }
                        }
                    }
                    k += 1;
                }
            }
            d += 32;
        }
    }
    patched
}

/// Installs the classic colours; returns how many import slots now point at
/// them (0: nothing found, the system colours stay).
pub fn install() -> usize {
    unsafe {
        let Ok(user32) = GetModuleHandleW(w!("user32.dll")) else { return 0 };
        let (Some(color), Some(brush)) = (GetProcAddress(user32, s!("GetSysColor")), GetProcAddress(user32, s!("GetSysColorBrush"))) else {
            return 0;
        };
        let Some(fill) = GetProcAddress(user32, s!("FillRect")) else { return 0 };
        ORIG_COLOR.store(color as usize, Ordering::Relaxed);
        ORIG_BRUSH.store(brush as usize, Ordering::Relaxed);
        ORIG_FILL.store(fill as usize, Ordering::Relaxed);
        let ours = [get_sys_color as *const () as usize, get_sys_color_brush as *const () as usize, fill_rect as *const () as usize];
        let pairs = [(color as usize, ours[0]), (brush as usize, ours[1]), (fill as usize, ours[2])];
        let names = [("GetSysColor", ours[0]), ("GetSysColorBrush", ours[1]), ("FillRect", ours[2])];
        let mut modules = [HMODULE::default(); 512];
        let mut needed = 0u32;
        let process = HANDLE(-1isize as *mut _); // GetCurrentProcess()
        if EnumProcessModules(process, modules.as_mut_ptr(), std::mem::size_of_val(&modules) as u32, &mut needed).is_err() {
            return 0;
        }
        let n = (needed as usize / std::mem::size_of::<HMODULE>()).min(modules.len());
        modules[..n].iter().filter(|m| **m != user32).map(|m| patch_module(*m, &pairs, &names)).sum()
    }
}

/// Makes comctl32 re-read its cached colours and gives class backgrounds
/// that user32 resolves itself the classic face brush (this process only).
pub fn face_classes() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, GCLP_HBRBACKGROUND, HWND_MESSAGE, SendMessageW, SetClassLongPtrW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_SYSCOLORCHANGE};
    for class in [w!("SysTabControl32")] {
        unsafe {
            if let Ok(h) = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!(""), WINDOW_STYLE(0), 0, 0, 0, 0, Some(HWND_MESSAGE), None, None, None) {
                SetClassLongPtrW(h, GCLP_HBRBACKGROUND, brush(15).0 as isize);
                // comctl32 caches its colours when it loads, before
                // `install`; this makes it read them again.
                SendMessageW(h, WM_SYSCOLORCHANGE, Some(WPARAM(0)), Some(LPARAM(0)));
                let _ = DestroyWindow(h);
            }
        }
    }
}

/// Gives a menu bar and its popups the classic face colour (the system
/// draws menus itself, so the import patch does not reach them).
pub fn face_menus(menu: HMENU) {
    let info = MENUINFO {
        cbSize: std::mem::size_of::<MENUINFO>() as u32,
        fMask: MIM_BACKGROUND | MIM_APPLYTOSUBMENUS,
        hbrBack: brush(15),
        ..Default::default()
    };
    unsafe {
        let _ = SetMenuInfo(menu, &info);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_palette_is_windows_2000_standard() {
        assert_eq!(classic(15), Some(0x00C8D0D4)); // BTNFACE D4D0C8 as 0x00BBGGRR
        assert_eq!(classic(13), Some(0x006A240A)); // HIGHLIGHT 0A246A
        assert_eq!(classic(2), None); // captions stay the system's
    }

    #[test]
    fn install_redirects_get_sys_color() {
        assert!(install() > 0);
        let c = unsafe { windows::Win32::Graphics::Gdi::GetSysColor(windows::Win32::Graphics::Gdi::COLOR_BTNFACE) };
        assert_eq!(c, 0x00C8D0D4);
    }
}
