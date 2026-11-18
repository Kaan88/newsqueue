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
//! Drawing done inside user32/win32k itself (`DrawEdge`, frames) still uses
//! the system colours; the menu bar gets the face colour through
//! `face_menus`, and title bars and scroll bars are painted again here
//! (see the caption and scroll bar sections).

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
        let mut pairs = vec![(color as usize, ours[0]), (brush as usize, ours[1]), (fill as usize, ours[2])];
        let mut names = vec![("GetSysColor", ours[0]), ("GetSysColorBrush", ours[1]), ("FillRect", ours[2])];
        for (name, from, to) in scroll_imports(user32) {
            pairs.push((from, to));
            names.push((name, to));
        }
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
        owner_draw_bar(menu);
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
        maybe_classic_scroll(HWND(wp.0 as *mut _), style);
        if cs.dwExStyle.0 & windows::Win32::UI::WindowsAndMessaging::WS_EX_CLIENTEDGE.0 != 0 {
            let _ = windows::Win32::UI::Shell::SetWindowSubclass(HWND(wp.0 as *mut _), Some(edge_proc), EDGE_SUBCLASS_ID, 0);
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
    if let Some(r) = menu_bar_message(h, m, wp, lp) {
        return r;
    }
    if m == WM_NCLBUTTONDOWN && track_caption_button(h, wp.0 as u32) {
        return LRESULT(0);
    }
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
        paint_frame(dc, wr.right - wr.left, wr.bottom - wr.top, wi.cxWindowBorders as i32, wi.cyWindowBorders as i32);
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
    paint_caption_buttons(h, dc, &info, &to_window);
}

// --------------------------------------------------------- scroll bars ---
//
// Window scroll bars are drawn inside user32 with the system's face colour
// (F0F0F0 on Windows 11), which the import patch cannot reach. Controls of
// the classes in `SCROLL_CLASSES` are subclassed when they are created and
// their bars painted again, Windows 2000 style, after every non-client paint
// and every change of a scroll position (`SetScrollInfo` and friends are
// redirected in the import tables like the colour functions).

const SCROLL_SUBCLASS_ID: usize = 0x4E51_5342; // "NQSB"
const SCROLL_CLASSES: [&str; 4] = ["SysListView32", "ListBox", "Edit", "ComboLBox"];
const STATE_SYSTEM_UNAVAILABLE: u32 = 0x1;
const STATE_SYSTEM_PRESSED: u32 = 0x8;
const STATE_SYSTEM_OFFSCREEN: u32 = 0x10000;

static ORIG_SET_INFO: AtomicUsize = AtomicUsize::new(0);
static ORIG_SET_POS: AtomicUsize = AtomicUsize::new(0);
static ORIG_SHOW: AtomicUsize = AtomicUsize::new(0);
static ORIG_ENABLE: AtomicUsize = AtomicUsize::new(0);

type SetInfoFn = unsafe extern "system" fn(isize, i32, *const std::ffi::c_void, i32) -> i32;
type SetPosFn = unsafe extern "system" fn(isize, i32, i32, i32) -> i32;
type ShowFn = unsafe extern "system" fn(isize, i32, i32) -> i32;
type EnableFn = unsafe extern "system" fn(isize, u32, u32) -> i32;

unsafe extern "system" fn set_scroll_info(h: isize, bar: i32, si: *const std::ffi::c_void, redraw: i32) -> i32 {
    let f: SetInfoFn = std::mem::transmute(ORIG_SET_INFO.load(Ordering::Relaxed));
    let r = f(h, bar, si, redraw);
    if redraw != 0 && bar != 2 {
        repaint_bars(HWND(h as *mut _));
    }
    r
}

unsafe extern "system" fn set_scroll_pos(h: isize, bar: i32, pos: i32, redraw: i32) -> i32 {
    let f: SetPosFn = std::mem::transmute(ORIG_SET_POS.load(Ordering::Relaxed));
    let r = f(h, bar, pos, redraw);
    if redraw != 0 && bar != 2 {
        repaint_bars(HWND(h as *mut _));
    }
    r
}

unsafe extern "system" fn show_scroll_bar(h: isize, bar: i32, show: i32) -> i32 {
    let f: ShowFn = std::mem::transmute(ORIG_SHOW.load(Ordering::Relaxed));
    let r = f(h, bar, show);
    if bar != 2 {
        repaint_bars(HWND(h as *mut _));
    }
    r
}

unsafe extern "system" fn enable_scroll_bar(h: isize, flags: u32, arrows: u32) -> i32 {
    let f: EnableFn = std::mem::transmute(ORIG_ENABLE.load(Ordering::Relaxed));
    let r = f(h, flags, arrows);
    if flags != 2 {
        repaint_bars(HWND(h as *mut _));
    }
    r
}

/// The redirections `install` adds for the scroll functions.
unsafe fn scroll_imports(user32: HMODULE) -> Vec<(&'static str, usize, usize)> {
    let mut out = Vec::new();
    for (name, cname, orig, ours) in [
        ("SetScrollInfo", s!("SetScrollInfo"), &ORIG_SET_INFO, set_scroll_info as *const () as usize),
        ("SetScrollPos", s!("SetScrollPos"), &ORIG_SET_POS, set_scroll_pos as *const () as usize),
        ("ShowScrollBar", s!("ShowScrollBar"), &ORIG_SHOW, show_scroll_bar as *const () as usize),
        ("EnableScrollBar", s!("EnableScrollBar"), &ORIG_ENABLE, enable_scroll_bar as *const () as usize),
        ("DrawEdge", s!("DrawEdge"), &ORIG_DRAW_EDGE, draw_edge as *const () as usize),
        ("DrawFrameControl", s!("DrawFrameControl"), &ORIG_DRAW_FRAME, draw_frame_control as *const () as usize),
    ] {
        if let Some(f) = GetProcAddress(user32, cname) {
            orig.store(f as usize, Ordering::Relaxed);
            out.push((name, f as usize, ours));
        }
    }
    out
}

/// Subclasses `h` for classic scroll bars when its class has bars.
unsafe fn maybe_classic_scroll(h: HWND, style: u32) {
    use windows::Win32::UI::Shell::SetWindowSubclass;
    use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;
    let mut buf = [0u16; 64];
    let n = GetClassNameW(h, &mut buf) as usize;
    let class = String::from_utf16_lossy(&buf[..n]);
    if SCROLL_CLASSES.iter().any(|c| c.eq_ignore_ascii_case(&class)) {
        let _ = SetWindowSubclass(h, Some(scroll_proc), SCROLL_SUBCLASS_ID, 0);
    }
    maybe_classic_button(h, &class, style);
}

fn is_classic_scroll(h: HWND) -> bool {
    use windows::Win32::UI::Shell::GetWindowSubclass;
    let mut data = 0usize;
    unsafe { GetWindowSubclass(h, Some(scroll_proc), SCROLL_SUBCLASS_ID, Some(&mut data)).as_bool() }
}

unsafe fn repaint_bars(h: HWND) {
    use windows::Win32::Graphics::Gdi::{GetWindowDC, ReleaseDC};
    if h.is_invalid() || !is_classic_scroll(h) {
        return;
    }
    let dc = GetWindowDC(Some(h));
    if !dc.is_invalid() {
        paint_bars(h, dc);
        ReleaseDC(Some(h), dc);
    }
}

unsafe extern "system" fn scroll_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    use windows::Win32::Graphics::Gdi::HDC;
    use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::*;
    match m {
        WM_NCPAINT | WM_VSCROLL | WM_HSCROLL | WM_MOUSEWHEEL | WM_MOUSEHWHEEL | WM_SIZE | WM_NCLBUTTONDOWN | WM_NCLBUTTONUP
        | WM_NCLBUTTONDBLCLK | WM_NCMOUSEMOVE | WM_NCMOUSELEAVE | WM_ENABLE | WM_STYLECHANGED | WM_KEYDOWN | WM_CAPTURECHANGED => {
            let r = DefSubclassProc(h, m, wp, lp);
            repaint_bars(h);
            r
        }
        WM_PRINT => {
            let r = DefSubclassProc(h, m, wp, lp);
            if lp.0 as u32 & PRF_NONCLIENT as u32 != 0 {
                paint_bars(h, HDC(wp.0 as *mut _));
            }
            r
        }
        WM_NCDESTROY => {
            let _ = RemoveWindowSubclass(h, Some(scroll_proc), SCROLL_SUBCLASS_ID);
            DefSubclassProc(h, m, wp, lp)
        }
        _ => DefSubclassProc(h, m, wp, lp),
    }
}

/// Paints the visible scroll bars of `h` into its window DC `dc`.
unsafe fn paint_bars(h: HWND, dc: HDC) {
    use windows::Win32::UI::WindowsAndMessaging::{GetScrollBarInfo, GetWindowRect, OBJID_HSCROLL, OBJID_VSCROLL, SCROLLBARINFO};
    let mut wr = RECT::default();
    if GetWindowRect(h, &mut wr).is_err() {
        return;
    }
    let mut shown = [None, None];
    for (i, obj) in [OBJID_VSCROLL, OBJID_HSCROLL].into_iter().enumerate() {
        let mut sbi = SCROLLBARINFO { cbSize: std::mem::size_of::<SCROLLBARINFO>() as u32, ..Default::default() };
        if GetScrollBarInfo(h, obj, &mut sbi).is_err() || sbi.rgstate[0] & (STATE_SYSTEM_INVISIBLE | STATE_SYSTEM_OFFSCREEN) != 0 {
            continue;
        }
        let r = sbi.rcScrollBar;
        let bar = RECT { left: r.left - wr.left, top: r.top - wr.top, right: r.right - wr.left, bottom: r.bottom - wr.top };
        if bar.right <= bar.left || bar.bottom <= bar.top {
            continue;
        }
        paint_bar(dc, bar, i == 0, &sbi);
        shown[i] = Some(bar);
    }
    // The size box between two bars.
    if let (Some(v), Some(hz)) = (shown[0], shown[1]) {
        fill(dc, RECT { left: v.left, top: hz.top, right: v.right, bottom: hz.bottom }, FACE);
    }
}

const FACE: u32 = rgb(0xD4, 0xD0, 0xC8);
const WHITE: u32 = rgb(0xFF, 0xFF, 0xFF);
const SHADOW: u32 = rgb(0x80, 0x80, 0x80);
const DKSHADOW: u32 = rgb(0x40, 0x40, 0x40);

unsafe fn fill(dc: HDC, r: RECT, color: u32) {
    use windows::Win32::Graphics::Gdi::{DeleteObject, FillRect};
    let b = CreateSolidBrush(COLORREF(color));
    FillRect(dc, &r, b);
    let _ = DeleteObject(b.into());
}

/// A raised Windows 2000 button: face, light/white top-left and
/// shadow/dark shadow bottom-right.
unsafe fn raised(dc: HDC, r: RECT) {
    let (l, t, rt, b) = (r.left, r.top, r.right, r.bottom);
    fill(dc, r, FACE);
    fill(dc, RECT { left: l, top: b - 1, right: rt, bottom: b }, DKSHADOW);
    fill(dc, RECT { left: rt - 1, top: t, right: rt, bottom: b }, DKSHADOW);
    fill(dc, RECT { left: l + 1, top: t + 1, right: rt - 2, bottom: t + 2 }, WHITE);
    fill(dc, RECT { left: l + 1, top: t + 1, right: l + 2, bottom: b - 2 }, WHITE);
    fill(dc, RECT { left: l + 1, top: b - 2, right: rt - 1, bottom: b - 1 }, SHADOW);
    fill(dc, RECT { left: rt - 2, top: t + 1, right: rt - 1, bottom: b - 1 }, SHADOW);
}

/// The dither of a scroll bar track: white and face pixels in turn.
unsafe fn dither(dc: HDC, r: RECT, dark: bool) {
    use windows::Win32::Graphics::Gdi::{CreateBitmap, CreatePatternBrush, DeleteObject, FillRect, SetBkColor, SetBrushOrgEx, SetTextColor};
    if dark {
        fill(dc, r, DKSHADOW);
        return;
    }
    let bits: [u16; 8] = [0xAAAA, 0x5555, 0xAAAA, 0x5555, 0xAAAA, 0x5555, 0xAAAA, 0x5555];
    let bmp = CreateBitmap(8, 8, 1, 1, Some(bits.as_ptr() as *const _));
    let brush = CreatePatternBrush(bmp);
    let old_text = SetTextColor(dc, COLORREF(FACE));
    let old_bk = SetBkColor(dc, COLORREF(WHITE));
    let _ = SetBrushOrgEx(dc, 0, 0, None);
    FillRect(dc, &r, brush);
    SetTextColor(dc, old_text);
    SetBkColor(dc, old_bk);
    let _ = DeleteObject(brush.into());
    let _ = DeleteObject(bmp.into());
}

/// A scroll arrow's triangle pointing `dir` (0 up, 1 down, 2 left, 3 right).
unsafe fn arrow_glyph(dc: HDC, r: RECT, dir: u8, enabled: bool, pressed: bool) {
    let size = ((r.right - r.left).min(r.bottom - r.top) * 7 / 16).max(3) | 1; // odd width
    let half = size / 2;
    let (mut cx, mut cy) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
    if pressed {
        cx += 1;
        cy += 1;
    }
    let draw = |dx: i32, dy: i32, color: u32| {
        // Rows of the triangle, from its point to its base.
        for k in 0..=half {
            let (x0, y0, x1, y1) = match dir {
                0 => (cx - k, cy - half / 2 + k, cx + k + 1, cy - half / 2 + k + 1),
                1 => (cx - k, cy + half / 2 - k, cx + k + 1, cy + half / 2 - k + 1),
                2 => (cx - half / 2 + k, cy - k, cx - half / 2 + k + 1, cy + k + 1),
                _ => (cx + half / 2 - k, cy - k, cx + half / 2 - k + 1, cy + k + 1),
            };
            fill(dc, RECT { left: x0 + dx, top: y0 + dy, right: x1 + dx, bottom: y1 + dy }, color);
        }
    };
    if enabled {
        draw(0, 0, 0);
    } else {
        draw(1, 1, WHITE);
        draw(0, 0, SHADOW);
    }
}

unsafe fn paint_bar(dc: HDC, bar: RECT, vertical: bool, sbi: &windows::Win32::UI::WindowsAndMessaging::SCROLLBARINFO) {
    let enabled = sbi.rgstate[0] & STATE_SYSTEM_UNAVAILABLE == 0;
    let len = if vertical { bar.bottom - bar.top } else { bar.right - bar.left };
    let arrow = sbi.dxyLineButton.min(len / 2).max(0);
    let (a, b) = if vertical {
        (RECT { bottom: bar.top + arrow, ..bar }, RECT { top: bar.bottom - arrow, ..bar })
    } else {
        (RECT { right: bar.left + arrow, ..bar }, RECT { left: bar.right - arrow, ..bar })
    };
    let track = if vertical { RECT { top: a.bottom, bottom: b.top, ..bar } } else { RECT { left: a.right, right: b.left, ..bar } };
    let has_thumb = enabled && sbi.xyThumbBottom > sbi.xyThumbTop && sbi.rgstate[3] & STATE_SYSTEM_INVISIBLE == 0;
    let thumb = if vertical {
        RECT { top: bar.top + sbi.xyThumbTop, bottom: bar.top + sbi.xyThumbBottom, ..bar }
    } else {
        RECT { left: bar.left + sbi.xyThumbTop, right: bar.left + sbi.xyThumbBottom, ..bar }
    };
    // Track, darker where it is held down.
    if has_thumb {
        let (before, after) = if vertical {
            (RECT { bottom: thumb.top, ..track }, RECT { top: thumb.bottom, ..track })
        } else {
            (RECT { right: thumb.left, ..track }, RECT { left: thumb.right, ..track })
        };
        dither(dc, before, sbi.rgstate[2] & STATE_SYSTEM_PRESSED != 0);
        dither(dc, after, sbi.rgstate[4] & STATE_SYSTEM_PRESSED != 0);
        raised(dc, thumb);
    } else {
        dither(dc, track, false);
    }
    for (r, state, dir) in [(a, sbi.rgstate[1], if vertical { 0 } else { 2 }), (b, sbi.rgstate[5], if vertical { 1 } else { 3 })] {
        let on = enabled && state & STATE_SYSTEM_UNAVAILABLE == 0;
        let pressed = on && state & STATE_SYSTEM_PRESSED != 0;
        if pressed {
            fill(dc, r, SHADOW);
            fill(dc, RECT { left: r.left + 1, top: r.top + 1, right: r.right - 1, bottom: r.bottom - 1 }, FACE);
        } else {
            raised(dc, r);
        }
        arrow_glyph(dc, r, dir, on, pressed);
    }
}

// ----------------------------------------------------- caption buttons ---
//
// user32 draws the minimize/maximize/close buttons with the system face
// colour, so `paint_caption` paints them again as raised Windows 2000
// buttons with Marlett glyphs, and `track_caption_button` runs the press
// itself so the held-down state is classic too.

/// The window and `TITLEBARINFOEX` button index (2 min, 3 max, 4 help,
/// 5 close) being held down, if any.
static PRESSED_WINDOW: AtomicIsize = AtomicIsize::new(0);
static PRESSED_BUTTON: AtomicUsize = AtomicUsize::new(0);

/// Paints the caption buttons from `info` (window coordinates via `to_window`).
unsafe fn paint_caption_buttons(h: HWND, dc: HDC, info: &windows::Win32::UI::WindowsAndMessaging::TITLEBARINFOEX, to_window: &dyn Fn(RECT) -> RECT) {
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::UI::WindowsAndMessaging::IsZoomed;
    let held = if PRESSED_WINDOW.load(Ordering::Relaxed) == h.0 as isize { PRESSED_BUTTON.load(Ordering::Relaxed) } else { 0 };
    for i in 2..=5 {
        let b = info.rgrect[i];
        if info.rgstate[i] & STATE_SYSTEM_INVISIBLE != 0 || b.right <= b.left {
            continue;
        }
        let r = to_window(b);
        let pressed = held == i;
        if pressed {
            sunken(dc, r);
        } else {
            raised(dc, r);
        }
        let glyph = match i {
            2 => '0',
            3 if IsZoomed(h).as_bool() => '2',
            3 => '1',
            4 => 's',
            _ => 'r',
        };
        let enabled = info.rgstate[i] & STATE_SYSTEM_UNAVAILABLE == 0;
        let height = ((r.bottom - r.top) * 9 / 16).max(6);
        let font = CreateFontW(height, 0, 0, 0, 400, 0, 0, 0, SYMBOL_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, DEFAULT_QUALITY, DEFAULT_PITCH.0 as u32, w!("Marlett"));
        let old = SelectObject(dc, font.into());
        SetBkMode(dc, TRANSPARENT);
        let mut text = [glyph as u16];
        let shift = if pressed { 1 } else { 0 };
        let mut draw = |dx: i32, color: u32| {
            SetTextColor(dc, COLORREF(color));
            let mut tr = RECT { left: r.left + shift + dx, top: r.top + shift + dx, right: r.right + shift + dx, bottom: r.bottom + shift + dx };
            DrawTextW(dc, &mut text, &mut tr, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        };
        if enabled {
            draw(0, 0);
        } else {
            draw(1, WHITE);
            draw(0, SHADOW);
        }
        SelectObject(dc, old);
        let _ = DeleteObject(font.into());
    }
}

/// The window frame (`bx`/`by` thick) of a `w` x `h` window: a raised
/// Windows 2000 edge around a face-coloured border.
unsafe fn paint_frame(dc: HDC, w: i32, h: i32, bx: i32, by: i32) {
    if bx < 2 || by < 2 {
        return;
    }
    fill(dc, RECT { left: 0, top: 0, right: w, bottom: by }, FACE);
    fill(dc, RECT { left: 0, top: h - by, right: w, bottom: h }, FACE);
    fill(dc, RECT { left: 0, top: 0, right: bx, bottom: h }, FACE);
    fill(dc, RECT { left: w - bx, top: 0, right: w, bottom: h }, FACE);
    fill(dc, RECT { left: 1, top: 1, right: w - 2, bottom: 2 }, WHITE);
    fill(dc, RECT { left: 1, top: 1, right: 2, bottom: h - 2 }, WHITE);
    fill(dc, RECT { left: 1, top: h - 2, right: w - 1, bottom: h - 1 }, SHADOW);
    fill(dc, RECT { left: w - 2, top: 1, right: w - 1, bottom: h - 1 }, SHADOW);
    fill(dc, RECT { left: 0, top: h - 1, right: w, bottom: h }, DKSHADOW);
    fill(dc, RECT { left: w - 1, top: 0, right: w, bottom: h }, DKSHADOW);
}

/// A pushed Windows 2000 button: the raised edge inverted.
unsafe fn sunken(dc: HDC, r: RECT) {
    let (l, t, rt, b) = (r.left, r.top, r.right, r.bottom);
    fill(dc, r, FACE);
    fill(dc, RECT { left: l, top: t, right: rt, bottom: t + 1 }, DKSHADOW);
    fill(dc, RECT { left: l, top: t, right: l + 1, bottom: b }, DKSHADOW);
    fill(dc, RECT { left: l + 1, top: t + 1, right: rt - 1, bottom: t + 2 }, SHADOW);
    fill(dc, RECT { left: l + 1, top: t + 1, right: l + 2, bottom: b - 1 }, SHADOW);
    fill(dc, RECT { left: l, top: b - 1, right: rt, bottom: b }, WHITE);
    fill(dc, RECT { left: rt - 1, top: t, right: rt, bottom: b }, WHITE);
}

/// Runs a press of the caption button under hit-test code `hit` the way
/// user32 would (pushed while the mouse is on it, the command on release
/// over it), but painted classic. Returns false for other hit codes.
unsafe fn track_caption_button(h: HWND, hit: u32) -> bool {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture};
    use windows::Win32::UI::WindowsAndMessaging::*;
    let index = match hit {
        HTMINBUTTON => 2,
        HTMAXBUTTON => 3,
        HTHELP => 4,
        HTCLOSE => 5,
        _ => return false,
    };
    let mut info = TITLEBARINFOEX { cbSize: std::mem::size_of::<TITLEBARINFOEX>() as u32, ..Default::default() };
    SendMessageW(h, WM_GETTITLEBARINFOEX, Some(WPARAM(0)), Some(LPARAM(&mut info as *mut _ as isize)));
    if info.rgstate[index] & (STATE_SYSTEM_UNAVAILABLE | STATE_SYSTEM_INVISIBLE) != 0 {
        return true;
    }
    let button = info.rgrect[index];
    let over = || {
        let mut p = POINT::default();
        let _ = GetCursorPos(&mut p);
        p.x >= button.left && p.x < button.right && p.y >= button.top && p.y < button.bottom
    };
    let show = |pressed: bool| {
        PRESSED_WINDOW.store(if pressed { h.0 as isize } else { 0 }, Ordering::Relaxed);
        PRESSED_BUTTON.store(if pressed { index } else { 0 }, Ordering::Relaxed);
        repaint_caption(h);
    };
    SetCapture(h);
    let mut inside = true;
    show(true);
    let mut msg = MSG::default();
    loop {
        if GetCapture() != h {
            inside = false;
            break;
        }
        if !GetMessageW(&mut msg, None, 0, 0).as_bool() {
            PostQuitMessage(msg.wParam.0 as i32);
            inside = false;
            break;
        }
        match msg.message {
            WM_MOUSEMOVE => {
                let now = over();
                if now != inside {
                    inside = now;
                    show(inside);
                }
            }
            WM_LBUTTONUP => {
                inside = over();
                break;
            }
            WM_KEYDOWN if msg.wParam.0 == 0x1B => {
                inside = false;
                break;
            }
            _ => {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    if GetCapture() == h {
        let _ = ReleaseCapture();
    }
    show(false);
    if inside {
        let command = match index {
            2 => SC_MINIMIZE,
            3 if IsZoomed(h).as_bool() => SC_RESTORE,
            3 => SC_MAXIMIZE,
            4 => SC_CONTEXTHELP,
            _ => SC_CLOSE,
        };
        let _ = PostMessageW(Some(h), WM_SYSCOMMAND, WPARAM(command as usize), LPARAM(0));
    }
    true
}

unsafe fn repaint_caption(h: HWND) {
    use windows::Win32::Graphics::Gdi::{GetWindowDC, ReleaseDC};
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let dc = GetWindowDC(Some(h));
    if !dc.is_invalid() {
        let active = FORCE_ACTIVE.load(Ordering::Relaxed) || windows::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow() == h || GetForegroundWindow() == h;
        paint_caption(h, dc, active);
        ReleaseDC(Some(h), dc);
    }
}

// ------------------------------------------------------------ menu bar ---
//
// With visual styles off Windows 10/11 draw the menu bar "flat": a blue
// highlight on the hot item and grey text while the window is inactive.
// `face_menus` makes the top-level items owner-drawn and the caption
// subclass draws them like Windows 2000: black text, a thin raised edge on
// the hot item and a thin sunken one on the open item.

/// Top-level menu items drawn here: their item data points into this list.
static BAR_ITEMS: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

fn is_bar_item(data: usize) -> bool {
    data != 0 && BAR_ITEMS.lock().map(|v| v.contains(&data)).unwrap_or(false)
}

/// The text of an owner-drawn bar item (with its `&` mnemonic marker).
unsafe fn bar_text(data: usize) -> &'static [u16] {
    &*(data as *const Vec<u16>)
}

unsafe fn owner_draw_bar(menu: HMENU) {
    use windows::Win32::UI::WindowsAndMessaging::*;
    let n = GetMenuItemCount(Some(menu));
    for i in 0..n.max(0) as u32 {
        let mut buf = [0u16; 128];
        let len = GetMenuStringW(menu, i, Some(&mut buf), MF_BYPOSITION) as usize;
        let text: &'static mut Vec<u16> = Box::leak(Box::new(buf[..len].to_vec()));
        let data = text as *mut Vec<u16> as usize;
        // The string stays on the item too: user32 matches Alt+letter
        // against it.
        let mut z: Vec<u16> = text.iter().copied().chain(std::iter::once(0)).collect();
        let info = MENUITEMINFOW {
            cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
            fMask: MIIM_FTYPE | MIIM_DATA | MIIM_STRING,
            fType: MFT_OWNERDRAW,
            dwItemData: data,
            dwTypeData: windows::core::PWSTR(z.as_mut_ptr()),
            cch: len as u32,
            ..Default::default()
        };
        if SetMenuItemInfoW(menu, i, true, &info).is_ok() {
            if let Ok(mut v) = BAR_ITEMS.lock() {
                v.push(data);
            }
        }
    }
}

/// The menu font, sized for the window's DPI.
unsafe fn menu_font(h: HWND) -> windows::Win32::Graphics::Gdi::HFONT {
    use windows::Win32::Graphics::Gdi::CreateFontIndirectW;
    use windows::Win32::UI::HiDpi::{GetDpiForWindow, SystemParametersInfoForDpi};
    use windows::Win32::UI::WindowsAndMessaging::{NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS};
    let mut ncm = NONCLIENTMETRICSW { cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32, ..Default::default() };
    let dpi = GetDpiForWindow(h).max(96);
    let _ = SystemParametersInfoForDpi(SPI_GETNONCLIENTMETRICS.0, ncm.cbSize, Some(&mut ncm as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0).0, dpi);
    CreateFontIndirectW(&ncm.lfMenuFont)
}

/// Handles the owner-draw messages of the menu bar; None for anything else.
unsafe fn menu_bar_message(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_DISABLED, ODS_GRAYED, ODS_HOTLIGHT, ODS_NOACCEL, ODS_SELECTED, ODT_MENU};
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    use windows::Win32::UI::WindowsAndMessaging::*;
    match m {
        WM_MEASUREITEM => {
            let mis = &mut *(lp.0 as *mut MEASUREITEMSTRUCT);
            if mis.CtlType != ODT_MENU || !is_bar_item(mis.itemData) {
                return None;
            }
            let text: Vec<u16> = bar_text(mis.itemData).iter().copied().filter(|c| *c != '&' as u16).collect();
            let dc = GetDC(Some(h));
            let font = menu_font(h);
            let old = SelectObject(dc, font.into());
            let mut size = windows::Win32::Foundation::SIZE::default();
            let _ = GetTextExtentPoint32W(dc, &text, &mut size);
            SelectObject(dc, old);
            let _ = DeleteObject(font.into());
            ReleaseDC(Some(h), dc);
            let pad = (6 * GetDpiForWindow(h).max(96) / 96) as i32;
            mis.itemWidth = (size.cx + 2 * pad) as u32;
            mis.itemHeight = GetSystemMetrics(SM_CYMENU) as u32;
            Some(LRESULT(1))
        }
        WM_DRAWITEM => {
            let dis = &*(lp.0 as *const DRAWITEMSTRUCT);
            if dis.CtlType != ODT_MENU || !is_bar_item(dis.itemData) {
                return None;
            }
            let dc = dis.hDC;
            let r = dis.rcItem;
            let state = dis.itemState;
            fill(dc, r, FACE);
            let open = state.0 & ODS_SELECTED.0 != 0;
            let hot = state.0 & ODS_HOTLIGHT.0 != 0;
            if open {
                fill(dc, RECT { bottom: r.top + 1, ..r }, SHADOW);
                fill(dc, RECT { right: r.left + 1, ..r }, SHADOW);
                fill(dc, RECT { top: r.bottom - 1, ..r }, WHITE);
                fill(dc, RECT { left: r.right - 1, ..r }, WHITE);
            } else if hot {
                fill(dc, RECT { bottom: r.top + 1, ..r }, WHITE);
                fill(dc, RECT { right: r.left + 1, ..r }, WHITE);
                fill(dc, RECT { top: r.bottom - 1, ..r }, SHADOW);
                fill(dc, RECT { left: r.right - 1, ..r }, SHADOW);
            }
            let font = menu_font(h);
            let old = SelectObject(dc, font.into());
            SetBkMode(dc, TRANSPARENT);
            let disabled = state.0 & (ODS_GRAYED.0 | ODS_DISABLED.0) != 0;
            let mut text = bar_text(dis.itemData).to_vec();
            let mut flags = DT_CENTER | DT_VCENTER | DT_SINGLELINE;
            if state.0 & ODS_NOACCEL.0 != 0 {
                flags |= DT_HIDEPREFIX;
            }
            let shift = if open { 1 } else { 0 };
            let mut tr = RECT { left: r.left + shift, top: r.top + shift, right: r.right + shift, bottom: r.bottom + shift };
            if disabled {
                SetTextColor(dc, COLORREF(WHITE));
                let mut t2 = RECT { left: tr.left + 1, top: tr.top + 1, right: tr.right + 1, bottom: tr.bottom + 1 };
                DrawTextW(dc, &mut text.clone(), &mut t2, flags);
            }
            SetTextColor(dc, COLORREF(if disabled { SHADOW } else { 0 }));
            DrawTextW(dc, &mut text, &mut tr, flags);
            SelectObject(dc, old);
            let _ = DeleteObject(font.into());
            Some(LRESULT(1))
        }
        WM_MENUCHAR => {
            // Owner-drawn items lose the automatic Alt+letter: find it here.
            let menu = HMENU(lp.0 as *mut _);
            if GetMenu(h) != menu {
                return None;
            }
            let ch = char::from_u32(wp.0 as u32 & 0xFFFF)?.to_lowercase().next()?;
            let n = GetMenuItemCount(Some(menu)).max(0) as u32;
            for i in 0..n {
                let info = &mut MENUITEMINFOW { cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32, fMask: MIIM_DATA, ..Default::default() };
                if GetMenuItemInfoW(menu, i, true, info).is_err() || !is_bar_item(info.dwItemData) {
                    continue;
                }
                let text = String::from_utf16_lossy(bar_text(info.dwItemData));
                let mnemonic = text.split('&').nth(1).and_then(|s| s.chars().next()).and_then(|c| c.to_lowercase().next());
                if mnemonic == Some(ch) {
                    return Some(LRESULT(((MNC_EXECUTE as isize) << 16) | i as isize));
                }
            }
            None
        }
        _ => None,
    }
}

// ------------------------------------------------ push and spin buttons ---
//
// comctl32 draws push buttons and up-down arrows through user32 frame
// functions with the system face colour (F0F0F0) while their text uses the
// redirected colours, so every caption sat on a darker box. Both are
// painted here in full instead, the Windows 2000 way.

const BUTTON_SUBCLASS_ID: usize = 0x4E51_4255; // "NQBU"
const MOUSE_LEAVE: u32 = 0x02A3; // WM_MOUSELEAVE
const SPIN_SUBCLASS_ID: usize = 0x4E51_5350; // "NQSP"

/// Subclasses push buttons and up-down controls for classic painting.
unsafe fn maybe_classic_button(h: HWND, class: &str, style: u32) {
    use windows::Win32::UI::Shell::SetWindowSubclass;
    if class.eq_ignore_ascii_case("Button") && matches!(style & 0xF, 0 | 1) {
        let _ = SetWindowSubclass(h, Some(button_proc), BUTTON_SUBCLASS_ID, 0);
    } else if class.eq_ignore_ascii_case("msctls_updown32") {
        let _ = SetWindowSubclass(h, Some(spin_proc), SPIN_SUBCLASS_ID, 0);
    }
}

unsafe fn paint_now(h: HWND, paint: unsafe fn(HWND, HDC)) {
    use windows::Win32::Graphics::Gdi::{GetDC, ReleaseDC};
    let dc = GetDC(Some(h));
    if !dc.is_invalid() {
        paint(h, dc);
        ReleaseDC(Some(h), dc);
    }
}

unsafe extern "system" fn button_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
    use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::*;
    match m {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(h, &mut ps);
            paint_button(h, dc);
            let _ = EndPaint(h, &ps);
            LRESULT(0)
        }
        WM_PRINTCLIENT => {
            paint_button(h, HDC(wp.0 as *mut _));
            LRESULT(0)
        }
        // The button repaints itself for these without WM_PAINT.
        WM_LBUTTONDOWN | WM_LBUTTONUP | WM_LBUTTONDBLCLK | WM_MOUSEMOVE | WM_KEYDOWN | WM_KEYUP | WM_SETFOCUS | WM_KILLFOCUS | WM_CAPTURECHANGED
        | WM_ENABLE | WM_SETTEXT | WM_UPDATEUISTATE | MOUSE_LEAVE => {
            let r = DefSubclassProc(h, m, wp, lp);
            paint_now(h, paint_button);
            r
        }
        _ if m == BM_SETSTATE || m == BM_SETSTYLE || m == BM_CLICK => {
            let r = DefSubclassProc(h, m, wp, lp);
            paint_now(h, paint_button);
            r
        }
        WM_NCDESTROY => {
            let _ = RemoveWindowSubclass(h, Some(button_proc), BUTTON_SUBCLASS_ID);
            DefSubclassProc(h, m, wp, lp)
        }
        _ => DefSubclassProc(h, m, wp, lp),
    }
}

/// A Windows 2000 push button: raised; the default button (or one being
/// pushed) gets a black frame; pushed it is flat with a shadow line and the
/// caption moves down-right; disabled text is embossed.
unsafe fn paint_button(h: HWND, dc: HDC) {
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
    use windows::Win32::UI::WindowsAndMessaging::*;
    let mut r = RECT::default();
    if GetClientRect(h, &mut r).is_err() || r.right <= r.left {
        return;
    }
    let state = SendMessageW(h, BM_GETSTATE, None, None).0 as u32;
    let pushed = state & 0x4 != 0; // BST_PUSHED
    let focused = state & 0x8 != 0; // BST_FOCUS
    let style = GetWindowLongW(h, GWL_STYLE) as u32;
    let default = style & 0xF == 1 || focused;
    let enabled = IsWindowEnabled(h).as_bool();
    let ui = SendMessageW(h, WM_QUERYUISTATE, None, None).0 as u32;
    let (hide_focus, hide_accel) = (ui & 0x1 != 0, ui & 0x2 != 0); // UISF_HIDEFOCUS, UISF_HIDEACCEL
    let mut face = r;
    if default {
        // The black frame of the default button.
        fill(dc, r, 0);
        face = RECT { left: r.left + 1, top: r.top + 1, right: r.right - 1, bottom: r.bottom - 1 };
    }
    if pushed {
        fill(dc, face, SHADOW);
        fill(dc, RECT { left: face.left + 1, top: face.top + 1, right: face.right - 1, bottom: face.bottom - 1 }, FACE);
    } else {
        raised(dc, face);
    }
    // Caption.
    let len = GetWindowTextLengthW(h);
    let mut text = vec![0u16; len as usize + 1];
    let n = GetWindowTextW(h, &mut text) as usize;
    text.truncate(n);
    let font = HFONT(SendMessageW(h, WM_GETFONT, None, None).0 as *mut _);
    let old = if font.is_invalid() { None } else { Some(SelectObject(dc, font.into())) };
    SetBkMode(dc, TRANSPARENT);
    let shift = if pushed { 1 } else { 0 };
    let mut flags = DT_CENTER | DT_VCENTER | DT_SINGLELINE;
    if hide_accel {
        flags |= DT_HIDEPREFIX;
    }
    let mut tr = RECT { left: face.left + 2 + shift, top: face.top + shift, right: face.right - 2 + shift, bottom: face.bottom + shift };
    if enabled {
        SetTextColor(dc, COLORREF(0));
        DrawTextW(dc, &mut text.clone(), &mut tr, flags);
    } else {
        SetTextColor(dc, COLORREF(WHITE));
        let mut t2 = RECT { left: tr.left + 1, top: tr.top + 1, right: tr.right + 1, bottom: tr.bottom + 1 };
        DrawTextW(dc, &mut text.clone(), &mut t2, flags);
        SetTextColor(dc, COLORREF(SHADOW));
        DrawTextW(dc, &mut text, &mut tr, flags);
    }
    if let Some(old) = old {
        SelectObject(dc, old);
    }
    if focused && !hide_focus {
        let fr = RECT { left: face.left + 3, top: face.top + 3, right: face.right - 3, bottom: face.bottom - 3 };
        SetTextColor(dc, COLORREF(0));
        SetBkColor(dc, COLORREF(WHITE));
        let _ = DrawFocusRect(dc, &fr);
    }
}

/// The half of an up-down control held down: 0 none, 1 up, 2 down.
static SPIN_HELD: AtomicUsize = AtomicUsize::new(0);
static SPIN_WINDOW: AtomicIsize = AtomicIsize::new(0);

unsafe extern "system" fn spin_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
    use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::*;
    match m {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(h, &mut ps);
            paint_spin(h, dc);
            let _ = EndPaint(h, &ps);
            LRESULT(0)
        }
        WM_PRINTCLIENT => {
            paint_spin(h, HDC(wp.0 as *mut _));
            LRESULT(0)
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            let mut r = RECT::default();
            let _ = GetClientRect(h, &mut r);
            let y = ((lp.0 >> 16) & 0xFFFF) as i16 as i32;
            SPIN_WINDOW.store(h.0 as isize, Ordering::Relaxed);
            SPIN_HELD.store(if y < (r.top + r.bottom) / 2 { 1 } else { 2 }, Ordering::Relaxed);
            let res = DefSubclassProc(h, m, wp, lp);
            paint_now(h, paint_spin);
            res
        }
        WM_LBUTTONUP | WM_CAPTURECHANGED => {
            let res = DefSubclassProc(h, m, wp, lp);
            if SPIN_WINDOW.load(Ordering::Relaxed) == h.0 as isize {
                SPIN_HELD.store(0, Ordering::Relaxed);
            }
            paint_now(h, paint_spin);
            res
        }
        WM_MOUSEMOVE | WM_TIMER | WM_ENABLE | MOUSE_LEAVE => {
            let res = DefSubclassProc(h, m, wp, lp);
            paint_now(h, paint_spin);
            res
        }
        WM_NCDESTROY => {
            let _ = RemoveWindowSubclass(h, Some(spin_proc), SPIN_SUBCLASS_ID);
            DefSubclassProc(h, m, wp, lp)
        }
        _ => DefSubclassProc(h, m, wp, lp),
    }
}

/// Two raised arrow buttons, the held one pushed.
unsafe fn paint_spin(h: HWND, dc: HDC) {
    use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
    let mut r = RECT::default();
    if GetClientRect(h, &mut r).is_err() || r.bottom - r.top < 4 {
        return;
    }
    let enabled = IsWindowEnabled(h).as_bool();
    let held = if SPIN_WINDOW.load(Ordering::Relaxed) == h.0 as isize { SPIN_HELD.load(Ordering::Relaxed) } else { 0 };
    let mid = (r.top + r.bottom) / 2;
    for (half, part, dir) in [(1, RECT { bottom: mid, ..r }, 0u8), (2, RECT { top: mid, ..r }, 1u8)] {
        let pushed = enabled && held == half;
        if pushed {
            fill(dc, part, SHADOW);
            fill(dc, RECT { left: part.left + 1, top: part.top + 1, right: part.right - 1, bottom: part.bottom - 1 }, FACE);
        } else {
            raised(dc, part);
        }
        arrow_glyph(dc, part, dir, enabled, pushed);
    }
}

// ----------------------------------------------------------- client edge ---
//
// The sunken 3D border of fields and lists (WS_EX_CLIENTEDGE) is drawn by
// user32 in the Windows 11 greys; it is painted again in the Windows 2000
// ones: shadow over dark shadow top-left, white over face bottom-right.

const EDGE_SUBCLASS_ID: usize = 0x4E51_4345; // "NQCE"

unsafe extern "system" fn edge_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    use windows::Win32::Graphics::Gdi::{GetWindowDC, ReleaseDC};
    use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::*;
    match m {
        WM_NCPAINT | WM_ENABLE | WM_STYLECHANGED | WM_SIZE => {
            let r = DefSubclassProc(h, m, wp, lp);
            let dc = GetWindowDC(Some(h));
            if !dc.is_invalid() {
                paint_client_edge(h, dc);
                ReleaseDC(Some(h), dc);
            }
            r
        }
        WM_PRINT => {
            let r = DefSubclassProc(h, m, wp, lp);
            if lp.0 as u32 & PRF_NONCLIENT as u32 != 0 {
                paint_client_edge(h, HDC(wp.0 as *mut _));
            }
            r
        }
        WM_NCDESTROY => {
            let _ = RemoveWindowSubclass(h, Some(edge_proc), EDGE_SUBCLASS_ID);
            DefSubclassProc(h, m, wp, lp)
        }
        _ => DefSubclassProc(h, m, wp, lp),
    }
}

unsafe fn paint_client_edge(h: HWND, dc: HDC) {
    use windows::Win32::UI::WindowsAndMessaging::{GWL_EXSTYLE, GetWindowLongW, GetWindowRect, WS_EX_CLIENTEDGE};
    if GetWindowLongW(h, GWL_EXSTYLE) as u32 & WS_EX_CLIENTEDGE.0 == 0 {
        return;
    }
    let mut wr = RECT::default();
    if GetWindowRect(h, &mut wr).is_err() {
        return;
    }
    let (w, ht) = (wr.right - wr.left, wr.bottom - wr.top);
    if w < 4 || ht < 4 {
        return;
    }
    fill(dc, RECT { left: 0, top: 0, right: w - 1, bottom: 1 }, SHADOW);
    fill(dc, RECT { left: 0, top: 0, right: 1, bottom: ht - 1 }, SHADOW);
    fill(dc, RECT { left: 1, top: 1, right: w - 2, bottom: 2 }, DKSHADOW);
    fill(dc, RECT { left: 1, top: 1, right: 2, bottom: ht - 2 }, DKSHADOW);
    fill(dc, RECT { left: 0, top: ht - 1, right: w, bottom: ht }, WHITE);
    fill(dc, RECT { left: w - 1, top: 0, right: w, bottom: ht }, WHITE);
    fill(dc, RECT { left: 1, top: ht - 2, right: w - 1, bottom: ht - 1 }, FACE);
    fill(dc, RECT { left: w - 2, top: 1, right: w - 1, bottom: ht - 1 }, FACE);
}

// ------------------------------------------------- DrawEdge / frames ---
//
// comctl32 draws check boxes, radio buttons, group boxes, combo box buttons
// and other 3D parts with user32's `DrawEdge` and `DrawFrameControl`, which
// pick the system colours inside user32. Their import slots point here:
// the original draws into a 32-bit buffer holding what is under the
// rectangle, then each system colour it used is swapped for the Windows
// 2000 one and the buffer is copied back.

static ORIG_DRAW_EDGE: AtomicUsize = AtomicUsize::new(0);
static ORIG_DRAW_FRAME: AtomicUsize = AtomicUsize::new(0);
type EdgeFn = unsafe extern "system" fn(isize, *mut RECT, u32, u32) -> i32;

unsafe extern "system" fn draw_edge(dc: isize, rc: *mut RECT, edge: u32, flags: u32) -> i32 {
    let f: EdgeFn = std::mem::transmute(ORIG_DRAW_EDGE.load(Ordering::Relaxed));
    recolor(dc, rc, |d, r| f(d, r, edge, flags))
}

unsafe extern "system" fn draw_frame_control(dc: isize, rc: *mut RECT, kind: u32, state: u32) -> i32 {
    let f: EdgeFn = std::mem::transmute(ORIG_DRAW_FRAME.load(Ordering::Relaxed));
    recolor(dc, rc, |d, r| f(d, r, kind, state))
}

/// Pairs of (system colour, classic colour) as 0x00RRGGBB DIB pixels.
fn swaps() -> &'static [(u32, u32)] {
    static SWAPS: std::sync::OnceLock<Vec<(u32, u32)>> = std::sync::OnceLock::new();
    SWAPS.get_or_init(|| {
        let dib = |c: u32| ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF);
        let orig = ORIG_COLOR.load(Ordering::Relaxed);
        let mut out: Vec<(u32, u32)> = Vec::new();
        if orig == 0 {
            return out;
        }
        let f: ColorFn = unsafe { std::mem::transmute(orig) };
        // Face first: it wins when two system colours are equal.
        for index in [15, 16, 21, 22, 17, 0, 4] {
            let (Some(c), sys) = (classic(index), unsafe { f(index) }) else { continue };
            if sys != c && !out.iter().any(|(s, _)| *s == dib(sys)) {
                out.push((dib(sys), dib(c)));
            }
        }
        out
    })
}

unsafe fn recolor(dc: isize, rc: *mut RECT, draw: impl Fn(isize, *mut RECT) -> i32) -> i32 {
    use windows::Win32::Graphics::Gdi::*;
    if rc.is_null() {
        return draw(dc, rc);
    }
    let r = *rc;
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let swaps = swaps();
    if w <= 0 || h <= 0 || w > 4096 || h > 4096 || swaps.is_empty() {
        return draw(dc, rc);
    }
    let hdc = HDC(dc as *mut _);
    let mem = CreateCompatibleDC(Some(hdc));
    if mem.is_invalid() {
        return draw(dc, rc);
    }
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let Ok(bmp) = CreateDIBSection(Some(hdc), &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
        let _ = DeleteDC(mem);
        return draw(dc, rc);
    };
    let old = SelectObject(mem, bmp.into());
    let result;
    if BitBlt(mem, 0, 0, w, h, Some(hdc), r.left, r.top, SRCCOPY).is_ok() && !bits.is_null() {
        // The same font and colours the caller selected.
        let font = GetCurrentObject(hdc, OBJ_FONT);
        let old_font = SelectObject(mem, font);
        SetTextColor(mem, GetTextColor(hdc));
        SetBkColor(mem, GetBkColor(hdc));
        let mut local = RECT { left: 0, top: 0, right: w, bottom: h };
        result = draw(mem.0 as isize, &mut local);
        let _ = GdiFlush();
        let px = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
        for p in px.iter_mut() {
            let c = *p & 0x00FF_FFFF;
            if let Some((_, to)) = swaps.iter().find(|(from, _)| *from == c) {
                *p = (*p & 0xFF00_0000) | to;
            }
        }
        let _ = BitBlt(hdc, r.left, r.top, w, h, Some(mem), 0, 0, SRCCOPY);
        SelectObject(mem, old_font);
        // BF_ADJUST: hand back the shrunken rectangle.
        (*rc).left = r.left + local.left;
        (*rc).top = r.top + local.top;
        (*rc).right = r.left + local.right;
        (*rc).bottom = r.top + local.bottom;
    } else {
        result = draw(dc, rc);
    }
    SelectObject(mem, old);
    let _ = DeleteObject(bmp.into());
    let _ = DeleteDC(mem);
    result
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
