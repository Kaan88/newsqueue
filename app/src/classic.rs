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
use windows::Win32::Foundation::{COLORREF, HANDLE, HMODULE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateSolidBrush, HBRUSH, HDC};
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

// ------------------------------------------------------------ captions ---
//
// Windows 10/11 draw every title bar through DWM in the modern style, even
// with visual styles off. `hook_captions` turns DWM's non-client rendering
// off for each top-level window with a caption (so user32 draws the classic
// frame and caption buttons) and paints the caption bar itself the way
// Windows 2000 did: a navy-to-light-blue gradient (grey when inactive), the
// small icon and the title in bold white.


const SUBCLASS_ID: usize = 0x4E51_4341; // "NQCA"
/// Accessibility state bit of a title-bar part that is not shown.
const STATE_SYSTEM_INVISIBLE: u32 = 0x8000;

/// Screenshot mode: draw every caption as active (the captured windows are
/// not in the foreground).
pub static FORCE_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Starts giving classic captions to the top-level windows this thread creates.
pub fn hook_captions() {
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowsHookExW, WH_CBT};
    unsafe {
        let _ = SetWindowsHookExW(WH_CBT, Some(cbt_proc), None, GetCurrentThreadId());
    }
}

unsafe extern "system" fn cbt_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{CBT_CREATEWNDW, CallNextHookEx, HCBT_CREATEWND, HHOOK, WS_CAPTION, WS_CHILD};
    if code == HCBT_CREATEWND as i32 {
        let cs = &*(*(lp.0 as *const CBT_CREATEWNDW)).lpcs;
        let style = cs.style as u32;
        if style & WS_CHILD.0 == 0 && style & WS_CAPTION.0 == WS_CAPTION.0 {
            classic_caption(HWND(wp.0 as *mut _));
        }
    }
    CallNextHookEx(Some(HHOOK::default()), code, wp, lp)
}

/// Gives one window the classic caption.
pub fn classic_caption(h: HWND) {
    use windows::Win32::Graphics::Dwm::{DWMNCRP_DISABLED, DWMWA_NCRENDERING_POLICY, DwmSetWindowAttribute};
    use windows::Win32::UI::Shell::SetWindowSubclass;
    unsafe {
        let policy = DWMNCRP_DISABLED;
        let _ = DwmSetWindowAttribute(h, DWMWA_NCRENDERING_POLICY, &policy as *const _ as *const _, std::mem::size_of_val(&policy) as u32);
        let _ = SetWindowSubclass(h, Some(caption_proc), SUBCLASS_ID, 0);
    }
}

unsafe extern "system" fn caption_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    use windows::Win32::Graphics::Gdi::{GetWindowDC, ReleaseDC};
    use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::*;
    match m {
        WM_NCPAINT | WM_SETTEXT | WM_NCACTIVATE | WM_SETICON | WM_NCLBUTTONDOWN | WM_NCLBUTTONUP | WM_NCMOUSEMOVE | WM_NCMOUSELEAVE => {
            let r = DefSubclassProc(h, m, wp, lp);
            // WM_NCACTIVATE says which state to draw before the window has it.
            let active = FORCE_ACTIVE.load(Ordering::Relaxed) || if m == WM_NCACTIVATE { wp.0 != 0 } else { windows::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow() == h || GetForegroundWindow() == h };
            let dc = GetWindowDC(Some(h));
            if !dc.is_invalid() {
                paint_caption(h, dc, active);
                ReleaseDC(Some(h), dc);
            }
            r
        }
        WM_PRINT => {
            let r = DefSubclassProc(h, m, wp, lp);
            if lp.0 as u32 & PRF_NONCLIENT as u32 != 0 {
                paint_caption(h, HDC(wp.0 as *mut _), true);
            }
            r
        }
        WM_NCDESTROY => {
            let _ = RemoveWindowSubclass(h, Some(caption_proc), SUBCLASS_ID);
            DefSubclassProc(h, m, wp, lp)
        }
        _ => DefSubclassProc(h, m, wp, lp),
    }
}

/// Paints the caption bar of `h` into its window DC `dc`, leaving the
/// caption buttons user32 drew.
unsafe fn paint_caption(h: HWND, dc: HDC, active: bool) {
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::UI::WindowsAndMessaging::*;
    let mut info = TITLEBARINFOEX { cbSize: std::mem::size_of::<TITLEBARINFOEX>() as u32, ..Default::default() };
    SendMessageW(h, WM_GETTITLEBARINFOEX, Some(WPARAM(0)), Some(LPARAM(&mut info as *mut _ as isize)));
    let mut wr = RECT::default();
    if GetWindowRect(h, &mut wr).is_err() || info.rcTitleBar.bottom <= info.rcTitleBar.top {
        return;
    }
    let to_window = |r: RECT| RECT { left: r.left - wr.left, top: r.top - wr.top, right: r.right - wr.left, bottom: r.bottom - wr.top };
    let mut bar = to_window(info.rcTitleBar);
    // rcTitleBar starts after the system icon; the bar runs from the frame.
    let mut wi = WINDOWINFO { cbSize: std::mem::size_of::<WINDOWINFO>() as u32, ..Default::default() };
    if GetWindowInfo(h, &mut wi).is_ok() {
        bar.left = wi.cxWindowBorders as i32;
    }
    // The bar runs up to the first caption button; buttons stay as drawn.
    let mut right = bar.right;
    for (i, b) in info.rgrect.iter().enumerate().skip(2) {
        let invisible = info.rgstate[i] & STATE_SYSTEM_INVISIBLE != 0;
        if !invisible && b.right > b.left {
            right = right.min(to_window(*b).left - 2);
        }
    }
    let saved = SaveDC(dc);
    let bar_rect = RECT { right: bar.right, ..bar };
    IntersectClipRect(dc, bar_rect.left, bar_rect.top, bar_rect.right, bar_rect.bottom);
    for (i, b) in info.rgrect.iter().enumerate().skip(2) {
        if info.rgstate[i] & STATE_SYSTEM_INVISIBLE == 0 && b.right > b.left {
            let b = to_window(*b);
            ExcludeClipRect(dc, b.left, b.top, b.right, b.bottom);
        }
    }
    let (a, b) = if active { (rgb(0x0A, 0x24, 0x6A), rgb(0xA6, 0xCA, 0xF0)) } else { (rgb(0x80, 0x80, 0x80), rgb(0xC0, 0xC0, 0xC0)) };
    // Gradient over the whole bar; the buttons are clipped out.
    let vert = |x: i32, y: i32, c: u32| TRIVERTEX {
        x,
        y,
        Red: ((c & 0xFF) << 8) as u16,
        Green: (((c >> 8) & 0xFF) << 8) as u16,
        Blue: (((c >> 16) & 0xFF) << 8) as u16,
        Alpha: 0,
    };
    let verts = [vert(bar_rect.left, bar_rect.top, a), vert(bar_rect.right, bar_rect.bottom, b)];
    let rect = GRADIENT_RECT { UpperLeft: 0, LowerRight: 1 };
    let _ = GdiGradientFill(dc, &verts, &rect as *const _ as *const _, 1, GRADIENT_FILL_RECT_H);
    // Small icon.
    let mut x = bar.left + 2;
    let icon = {
        let i = SendMessageW(h, WM_GETICON, Some(WPARAM(ICON_SMALL as usize)), Some(LPARAM(0))).0;
        let i = if i == 0 { SendMessageW(h, WM_GETICON, Some(WPARAM(ICON_SMALL2 as usize)), Some(LPARAM(0))).0 } else { i };
        if i == 0 { GetClassLongPtrW(h, GCLP_HICONSM) as isize } else { i }
    };
    let style = GetWindowLongW(h, GWL_STYLE) as u32;
    if icon != 0 && style & WS_SYSMENU.0 != 0 {
        let sz = GetSystemMetrics(SM_CXSMICON);
        let y = bar.top + (bar.bottom - bar.top - sz) / 2;
        let _ = DrawIconEx(dc, x, y, HICON(icon as *mut _), sz, sz, 0, None, DI_NORMAL);
        x += sz + 3;
    } else {
        x += 2;
    }
    // Title in the caption font, bold white.
    let len = GetWindowTextLengthW(h);
    let mut buf = vec![0u16; len as usize + 1];
    let n = GetWindowTextW(h, &mut buf) as usize;
    let mut ncm = NONCLIENTMETRICSW { cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32, ..Default::default() };
    let _ = SystemParametersInfoW(SPI_GETNONCLIENTMETRICS, ncm.cbSize, Some(&mut ncm as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    ncm.lfCaptionFont.lfWeight = 700;
    let font = CreateFontIndirectW(&ncm.lfCaptionFont);
    let old = SelectObject(dc, font.into());
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, COLORREF(if active { rgb(0xFF, 0xFF, 0xFF) } else { rgb(0xD4, 0xD0, 0xC8) }));
    let mut tr = RECT { left: x, top: bar.top, right: right - 2, bottom: bar.bottom };
    DrawTextW(dc, &mut buf[..n], &mut tr, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX);
    SelectObject(dc, old);
    let _ = DeleteObject(font.into());
    let _ = RestoreDC(dc, saved);
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
