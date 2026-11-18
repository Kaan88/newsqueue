//! Small dialog framework over plain windows: a top-level "NQDlg" window,
//! controls placed in 96-dpi units, and one handler closure that sees every
//! command, notification and timer. Modal dialogs run their own message loop
//! (`run_modal`); modeless ones (`show_modeless`) ride on the main loop,
//! which passes their keys through `is_dialog_message`.

use crate::util::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::rc::Rc;
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;

pub const CHILD: u32 = WS_CHILD.0 | WS_VISIBLE.0;
pub const SS_ICON: u32 = 0x03;
pub const SS_ETCHEDHORZ: u32 = 0x10;
pub const IDOK_: i32 = 1;
pub const IDCANCEL_: i32 = 2;
const WM_END: u32 = WM_APP + 40;

/// What the handler is told about.
pub enum Ev {
    /// WM_COMMAND: control id and notification code.
    Cmd(i32, u32),
    /// WM_NOTIFY: the control id and the raw NMHDR pointer.
    Notify(i32, LPARAM),
    #[allow(dead_code)]
    Timer(usize),
    /// The close box (the default handler treats it as Cancel).
    Close,
}

pub type Handler = Box<dyn FnMut(HWND, Ev) -> isize>;

struct Data {
    handler: RefCell<Option<Handler>>,
    result: Cell<Option<i32>>,
    modeless: bool,
}

thread_local! {
    static CTRLS: RefCell<HashMap<(isize, i32), HWND>> = RefCell::new(HashMap::new());
    static GRAY: RefCell<HashSet<isize>> = RefCell::new(HashSet::new());
    static ACCENT: RefCell<HashSet<isize>> = RefCell::new(HashSet::new());
    static MODELESS: RefCell<Vec<HWND>> = const { RefCell::new(Vec::new()) };
}

pub fn register_classes() {
    unsafe {
        for (name, proc_, bg) in [
            (w!("NQDlg"), Some(dlg_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT), GetSysColorBrush(COLOR_BTNFACE)),
            (w!("NQPanel"), Some(panel_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT), GetSysColorBrush(COLOR_BTNFACE)),
        ] {
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: proc_,
                hInstance: hinst(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: bg,
                lpszClassName: name,
                hIcon: LoadIconW(Some(hinst()), PCWSTR(1 as *const u16)).unwrap_or_default(),
                ..Default::default()
            };
            RegisterClassExW(&wc);
        }
    }
}

/// A hidden dialog window of client size `cw`×`ch` (96-dpi units), centred
/// on `owner`.
pub fn create(owner: Option<HWND>, title: &str, cw: i32, ch: i32, resizable: bool) -> HWND {
    // No WS_CLIPCHILDREN: group boxes are transparent and rely on the
    // dialog painting the face colour behind them.
    let mut style = WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0;
    if resizable {
        style |= WS_THICKFRAME.0 | WS_MAXIMIZEBOX.0;
    }
    let ex = WS_EX_DLGMODALFRAME.0 | WS_EX_CONTROLPARENT.0;
    let (ow, oh) = outer(style, ex, cw, ch);
    let (mut x, mut y);
    unsafe {
        let mut r = RECT::default();
        let ok = match owner {
            Some(o) if IsWindowVisible(o).as_bool() && !IsIconic(o).as_bool() => GetWindowRect(o, &mut r).is_ok(),
            _ => false,
        };
        if ok {
            x = r.left + ((r.right - r.left) - ow) / 2;
            y = r.top + ((r.bottom - r.top) - oh) / 2;
        } else {
            let mut wa = RECT::default();
            let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut wa as *mut _ as *mut c_void), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
            x = wa.left + ((wa.right - wa.left) - ow) / 2;
            y = wa.top + ((wa.bottom - wa.top) - oh) / 3;
        }
        x = x.max(0);
        y = y.max(0);
    }
    let c = wz("NQDlg");
    let t = wz(title);
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(ex),
            PCWSTR(c.as_ptr()),
            PCWSTR(t.as_ptr()),
            WINDOW_STYLE(style),
            x,
            y,
            ow,
            oh,
            owner,
            None,
            Some(hinst()),
            None,
        )
        .expect("dialog window")
    }
}

fn attach(dlg: HWND, handler: Handler, modeless: bool) {
    let data = Rc::new(Data { handler: RefCell::new(Some(handler)), result: Cell::new(None), modeless });
    unsafe {
        SetWindowLongPtrW(dlg, GWLP_USERDATA, Rc::into_raw(data) as isize);
    }
}

fn data(h: HWND) -> Option<Rc<Data>> {
    unsafe {
        let p = GetWindowLongPtrW(h, GWLP_USERDATA) as *const Data;
        if p.is_null() {
            return None;
        }
        Rc::increment_strong_count(p);
        Some(Rc::from_raw(p))
    }
}

/// Runs the handler; while it runs (e.g. it opened a nested modal dialog)
/// further events for this dialog get the default answer.
fn dispatch(h: HWND, ev: Ev) -> Option<isize> {
    let d = data(h)?;
    let mut f = d.handler.borrow_mut().take()?;
    let r = f(h, ev);
    if d.handler.borrow().is_none() {
        *d.handler.borrow_mut() = Some(f);
    }
    Some(r)
}

/// Shows `dlg` modally over `owner` and returns the code given to [`end`].
pub fn run_modal(dlg: HWND, owner: Option<HWND>, handler: Handler) -> i32 {
    attach(dlg, handler, false);
    unsafe {
        if let Some(o) = owner {
            let _ = EnableWindow(o, false);
        }
        let _ = ShowWindow(dlg, SW_SHOW);
        focus_first(dlg);
        let d = data(dlg).expect("dialog data");
        let mut msg = MSG::default();
        while d.result.get().is_none() {
            if !GetMessageW(&mut msg, None, 0, 0).as_bool() {
                // WM_QUIT: hand it back to the outer loop.
                PostQuitMessage(msg.wParam.0 as i32);
                d.result.set(Some(IDCANCEL_));
                break;
            }
            if !is_dialog_message(&msg) && !IsDialogMessageW(dlg, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        if let Some(o) = owner {
            let _ = EnableWindow(o, true);
        }
        let _ = DestroyWindow(dlg);
        if let Some(o) = owner {
            if IsWindowVisible(o).as_bool() {
                let _ = SetForegroundWindow(o);
            }
        }
        d.result.get().unwrap_or(IDCANCEL_)
    }
}

/// Shows `dlg` without blocking; [`end`] closes it.
pub fn show_modeless(dlg: HWND, handler: Handler) {
    attach(dlg, handler, true);
    MODELESS.with(|m| m.borrow_mut().push(dlg));
    unsafe {
        let _ = ShowWindow(dlg, SW_SHOW);
    }
    focus_first(dlg);
}

/// Lets the main loop route Tab/Enter/Esc to open modeless dialogs.
pub fn is_dialog_message(msg: &MSG) -> bool {
    let list = MODELESS.with(|m| m.borrow().clone());
    list.into_iter().any(|d| unsafe { IsDialogMessageW(d, msg).as_bool() })
}

/// The open modeless dialogs (e.g. to bring Setup to the front).
pub fn modeless() -> Vec<HWND> {
    MODELESS.with(|m| m.borrow().clone())
}

pub fn end(dlg: HWND, code: i32) {
    if let Some(d) = data(dlg) {
        d.result.set(Some(code));
        if d.modeless {
            unsafe {
                let _ = PostMessageW(Some(dlg), WM_END, WPARAM(0), LPARAM(0));
            }
        }
    }
}

fn focus_first(dlg: HWND) {
    unsafe {
        if let Ok(first) = GetNextDlgTabItem(dlg, None, false) {
            let _ = SetFocus(Some(first));
        }
    }
}

unsafe extern "system" fn dlg_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_COMMAND => {
            let id = (wp.0 & 0xffff) as i32;
            let code = ((wp.0 >> 16) & 0xffff) as u32;
            if dispatch(h, Ev::Cmd(id, code)).is_none() && id == IDCANCEL_ {
                end(h, IDCANCEL_);
            }
            LRESULT(0)
        }
        WM_NOTIFY => {
            let nm = &*(lp.0 as *const NMHDR);
            LRESULT(dispatch(h, Ev::Notify(nm.idFrom as i32, lp)).unwrap_or(0))
        }
        WM_TIMER => {
            dispatch(h, Ev::Timer(wp.0));
            LRESULT(0)
        }
        WM_CLOSE => {
            if dispatch(h, Ev::Close).is_none() {
                end(h, IDCANCEL_);
            }
            LRESULT(0)
        }
        WM_END => {
            let _ = DestroyWindow(h);
            LRESULT(0)
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => ctl_color(h, m, wp, lp),
        WM_PRINTCLIENT => print_face(h, wp),
        WM_DESTROY => {
            MODELESS.with(|l| l.borrow_mut().retain(|x| *x != h));
            CTRLS.with(|c| c.borrow_mut().retain(|k, _| k.0 != h.0 as isize));
            let p = GetWindowLongPtrW(h, GWLP_USERDATA) as *const Data;
            if !p.is_null() {
                SetWindowLongPtrW(h, GWLP_USERDATA, 0);
                drop(Rc::from_raw(p));
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

/// Gray notes and the orange countdown line.
pub unsafe fn ctl_color(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let dc = HDC(wp.0 as *mut c_void);
    let c = lp.0;
    if m == WM_CTLCOLORSTATIC {
        if GRAY.with(|g| g.borrow().contains(&c)) {
            SetTextColor(dc, COLORREF(GetSysColor(COLOR_GRAYTEXT)));
            SetBkMode(dc, TRANSPARENT);
            return LRESULT(GetSysColorBrush(COLOR_BTNFACE).0 as isize);
        }
        if ACCENT.with(|g| g.borrow().contains(&c)) {
            SetTextColor(dc, rgb(0xC0, 0x5A, 0x00));
            SetBkMode(dc, TRANSPARENT);
            return LRESULT(GetSysColorBrush(COLOR_BTNFACE).0 as isize);
        }
    }
    if m == WM_CTLCOLORSTATIC || m == WM_CTLCOLORBTN {
        // Labels, checkboxes and group boxes all sit on the face colour
        // (DefWindowProc leaves a group box's inside white).
        SetTextColor(dc, COLORREF(GetSysColor(COLOR_BTNTEXT)));
        SetBkColor(dc, COLORREF(GetSysColor(COLOR_BTNFACE)));
        return LRESULT(GetSysColorBrush(COLOR_BTNFACE).0 as isize);
    }
    let _ = h;
    DefWindowProcW(h, m, wp, lp)
}

/// WM_PRINT (screenshots) skips the class background; fill it here.
pub unsafe fn print_face(h: HWND, wp: WPARAM) -> LRESULT {
    let mut rc = RECT::default();
    let _ = GetClientRect(h, &mut rc);
    FillRect(HDC(wp.0 as *mut c_void), &rc, GetSysColorBrush(COLOR_BTNFACE));
    LRESULT(0)
}

/// Container for a group of controls (a Setup page); forwards everything
/// the controls send to the dialog.
unsafe extern "system" fn panel_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_COMMAND | WM_NOTIFY | WM_DRAWITEM => {
            let root = GetAncestor(h, GA_ROOT);
            SendMessageW(root, m, Some(wp), Some(lp))
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => ctl_color(h, m, wp, lp),
        WM_PRINTCLIENT => print_face(h, wp),
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

// ------------------------------------------------------------ controls ----

fn reg(parent: HWND, id: i32, c: HWND) -> HWND {
    if id > 2 || id == IDOK_ || id == IDCANCEL_ {
        let root = unsafe { GetAncestor(parent, GA_ROOT) };
        CTRLS.with(|m| m.borrow_mut().insert((root.0 as isize, id), c));
    }
    c
}

/// The control `id` anywhere inside the dialog (pages included).
pub fn item(dlg: HWND, id: i32) -> HWND {
    let root = unsafe { GetAncestor(dlg, GA_ROOT) };
    CTRLS.with(|m| m.borrow().get(&(root.0 as isize, id)).copied()).unwrap_or_default()
}

pub fn panel(p: HWND, x: i32, y: i32, wd: i32, ht: i32) -> HWND {
    mk("NQPanel", "", sty!(WS_CHILD, WS_CLIPSIBLINGS), WS_EX_CONTROLPARENT.0, x, y, wd, ht, p, 0)
}

pub fn label(p: HWND, t: &str, x: i32, y: i32, wd: i32) -> HWND {
    mk("STATIC", t, CHILD | 0x80 /* SS_NOPREFIX */, 0, x, y + 4, wd, 16, p, 0)
}
pub fn label_id(p: HWND, id: i32, t: &str, x: i32, y: i32, wd: i32, ht: i32) -> HWND {
    reg(p, id, mk("STATIC", t, CHILD | 0x80, 0, x, y, wd, ht, p, id))
}
pub fn heading(p: HWND, t: &str, x: i32, y: i32, wd: i32) -> HWND {
    let h = mk("STATIC", t, CHILD | 0x80, 0, x, y, wd, 20, p, 0);
    send(h, WM_SETFONT, bold().0 as usize, 1);
    h
}
pub fn note(p: HWND, t: &str, x: i32, y: i32, wd: i32, ht: i32) -> HWND {
    let n = mk("STATIC", t, CHILD | 0x80, 0, x, y, wd, ht, p, 0);
    GRAY.with(|g| g.borrow_mut().insert(n.0 as isize));
    n
}
pub fn accent(c: HWND) {
    ACCENT.with(|g| g.borrow_mut().insert(c.0 as isize));
}
pub fn edit(p: HWND, id: i32, t: &str, x: i32, y: i32, wd: i32, extra: u32) -> HWND {
    reg(p, id, mk("EDIT", t, sty!(CHILD, WS_TABSTOP, ES_AUTOHSCROLL) | extra, WS_EX_CLIENTEDGE.0, x, y, wd, 23, p, id))
}
pub fn memo(p: HWND, id: i32, t: &str, x: i32, y: i32, wd: i32, ht: i32) -> HWND {
    let st = sty!(CHILD, WS_TABSTOP, WS_VSCROLL, ES_MULTILINE, ES_AUTOVSCROLL, ES_WANTRETURN);
    let e = reg(p, id, mk("EDIT", "", st, WS_EX_CLIENTEDGE.0, x, y, wd, ht, p, id));
    set_text(e, &t.replace("\r\n", "\n").replace('\n', "\r\n"));
    e
}
/// Number edit with an up-down buddy; returns the edit.
pub fn spin(p: HWND, id: i32, val: i64, lo_: i32, hi_: i32, x: i32, y: i32, wd: i32) -> HWND {
    let e = edit(p, id, &val.to_string(), x, y, wd, sty!(ES_NUMBER));
    let u = mk("msctls_updown32", "", sty!(CHILD, UDS_SETBUDDYINT, UDS_ALIGNRIGHT, UDS_ARROWKEYS, UDS_NOTHOUSANDS), 0, 0, 0, 10, 10, p, 0);
    send(u, UDM_SETBUDDY, e.0 as usize, 0);
    send(u, UDM_SETRANGE32, lo_ as usize, hi_ as isize);
    send(u, UDM_SETPOS32, 0, val as isize);
    e
}
pub fn button(p: HWND, id: i32, t: &str, x: i32, y: i32, wd: i32) -> HWND {
    let st = if id == IDOK_ { BS_DEFPUSHBUTTON } else { BS_PUSHBUTTON };
    reg(p, id, mk("BUTTON", t, sty!(CHILD, WS_TABSTOP, st), 0, x, y, wd, 26, p, id))
}
pub fn check(p: HWND, id: i32, t: &str, x: i32, y: i32, wd: i32, on: bool) -> HWND {
    let c = reg(p, id, mk("BUTTON", t, sty!(CHILD, WS_TABSTOP, BS_AUTOCHECKBOX), 0, x, y, wd, 20, p, id));
    set_check(c, on);
    c
}
pub fn radio(p: HWND, id: i32, t: &str, x: i32, y: i32, wd: i32, first: bool) -> HWND {
    let st = sty!(CHILD, WS_TABSTOP, BS_AUTORADIOBUTTON) | if first { WS_GROUP.0 } else { 0 };
    reg(p, id, mk("BUTTON", t, st, 0, x, y, wd, 20, p, id))
}
pub fn group(p: HWND, t: &str, x: i32, y: i32, wd: i32, ht: i32) -> HWND {
    let g = mk("BUTTON", t, sty!(CHILD, BS_GROUPBOX), 0, x, y, wd, ht, p, 0);
    // Below the controls it frames, so it never paints over them.
    unsafe {
        let _ = SetWindowPos(g, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
    g
}
pub fn combo(p: HWND, id: i32, items: &[&str], sel: usize, x: i32, y: i32, wd: i32) -> HWND {
    let cb = reg(p, id, mk("COMBOBOX", "", sty!(CHILD, WS_TABSTOP, WS_VSCROLL, CBS_DROPDOWNLIST), 0, x, y, wd, 200, p, id));
    for t in items {
        let z = wz(t);
        send(cb, CB_ADDSTRING, 0, z.as_ptr() as isize);
    }
    send(cb, CB_SETCURSEL, sel, 0);
    cb
}
pub fn hline(p: HWND, x: i32, y: i32, wd: i32) {
    mk("STATIC", "", sty!(CHILD, SS_ETCHEDHORZ), 0, x, y, wd, 2, p, 0);
}
pub fn listbox(p: HWND, id: i32, items: &[&str], x: i32, y: i32, wd: i32, ht: i32) -> HWND {
    let lb = reg(
        p,
        id,
        mk("LISTBOX", "", sty!(CHILD, WS_TABSTOP, WS_VSCROLL, LBS_NOTIFY, LBS_NOINTEGRALHEIGHT), WS_EX_CLIENTEDGE.0, x, y, wd, ht, p, id),
    );
    send(lb, LB_SETITEMHEIGHT, 0, s(22) as isize);
    for t in items {
        let z = wz(t);
        send(lb, LB_ADDSTRING, 0, z.as_ptr() as isize);
    }
    lb
}
/// Report-style ListView with the Explorer look.
pub fn listview(p: HWND, id: i32, cols: &[(&str, i32, bool)], x: i32, y: i32, wd: i32, ht: i32, extra_ex: u32) -> HWND {
    let lv = reg(
        p,
        id,
        mk("SysListView32", "", sty!(CHILD, WS_TABSTOP, LVS_REPORT, LVS_SHOWSELALWAYS, LVS_SINGLESEL), WS_EX_CLIENTEDGE.0, x, y, wd, ht, p, id),
    );
    lv_setup(lv, extra_ex);
    lv_cols(lv, cols);
    lv
}

pub fn lv_setup(lv: HWND, extra_ex: u32) {
    unsafe {
        let _ = windows::Win32::UI::Controls::SetWindowTheme(lv, w!("Explorer"), PCWSTR::null());
    }
    let ex = (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER | LVS_EX_LABELTIP) | extra_ex;
    send(lv, LVM_SETEXTENDEDLISTVIEWSTYLE, ex as usize, ex as isize);
}

pub fn lv_cols(lv: HWND, cols: &[(&str, i32, bool)]) {
    for (i, (t, wd, right)) in cols.iter().enumerate() {
        let tz = wz(t);
        let mut c = LVCOLUMNW::default();
        c.mask = LVCF_TEXT | LVCF_WIDTH | LVCF_FMT;
        c.fmt = if *right { LVCFMT_RIGHT } else { LVCFMT_LEFT };
        c.cx = s(*wd);
        c.pszText = PWSTR(tz.as_ptr() as *mut u16);
        send(lv, LVM_INSERTCOLUMNW, i, &c as *const _ as isize);
    }
}

/// Replaces all rows of a normal (not owner-data) ListView.
pub fn lv_fill(lv: HWND, rows: &[Vec<String>]) {
    send(lv, WM_SETREDRAW, 0, 0);
    send(lv, LVM_DELETEALLITEMS, 0, 0);
    for (r, cells) in rows.iter().enumerate() {
        for (i, t) in cells.iter().enumerate() {
            let tz = wz(t);
            let mut it = LVITEMW::default();
            it.mask = LVIF_TEXT;
            it.iItem = r as i32;
            it.iSubItem = i as i32;
            it.pszText = PWSTR(tz.as_ptr() as *mut u16);
            if i == 0 {
                send(lv, LVM_INSERTITEMW, 0, &it as *const _ as isize);
            } else {
                send(lv, LVM_SETITEMTEXTW, r, &it as *const _ as isize);
            }
        }
    }
    send(lv, WM_SETREDRAW, 1, 0);
}

pub fn lv_selected(lv: HWND) -> Option<usize> {
    let r = send(lv, LVM_GETNEXTITEM, usize::MAX, LVNI_SELECTED as isize);
    (r >= 0).then_some(r as usize)
}

pub fn lv_select(lv: HWND, row: usize) {
    let mut clear = LVITEMW::default();
    clear.stateMask = LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED.0 | LVIS_FOCUSED.0);
    send(lv, LVM_SETITEMSTATE, usize::MAX, &clear as *const _ as isize);
    let mut it = LVITEMW::default();
    it.stateMask = LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED.0 | LVIS_FOCUSED.0);
    it.state = LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED.0 | LVIS_FOCUSED.0);
    send(lv, LVM_SETITEMSTATE, row, &it as *const _ as isize);
    send(lv, LVM_ENSUREVISIBLE, row, 0);
}

pub fn lv_set_checked(lv: HWND, row: usize, on: bool) {
    let mut it = LVITEMW::default();
    it.stateMask = LVIS_STATEIMAGEMASK;
    it.state = LIST_VIEW_ITEM_STATE_FLAGS(if on { 2 << 12 } else { 1 << 12 });
    send(lv, LVM_SETITEMSTATE, row, &it as *const _ as isize);
}

// -------------------------------------------------------------- values ----

pub fn text(c: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(c).max(0) as usize;
        let mut buf = vec![0u16; n + 1];
        let got = GetWindowTextW(c, &mut buf) as usize;
        String::from_utf16_lossy(&buf[..got])
    }
}
pub fn set_text(c: HWND, t: &str) {
    let z = wz(t);
    unsafe {
        let _ = SetWindowTextW(c, PCWSTR(z.as_ptr()));
    }
}
/// Multi-line edit text with plain `\n` line ends.
pub fn memo_text(c: HWND) -> String {
    text(c).replace("\r\n", "\n")
}
pub fn checked(c: HWND) -> bool {
    send(c, BM_GETCHECK, 0, 0) == 1
}
pub fn set_check(c: HWND, on: bool) {
    send(c, BM_SETCHECK, on as usize, 0);
}
pub fn num(c: HWND) -> i64 {
    text(c).trim().parse().unwrap_or(0)
}
pub fn combo_sel(c: HWND) -> usize {
    send(c, CB_GETCURSEL, 0, 0).max(0) as usize
}
pub fn enable(c: HWND, on: bool) {
    unsafe {
        let _ = EnableWindow(c, on);
    }
}
pub fn show(c: HWND, on: bool) {
    unsafe {
        let _ = ShowWindow(c, if on { SW_SHOW } else { SW_HIDE });
    }
}

/// Message box owned by `owner`.
pub fn message(owner: Option<HWND>, title: &str, body: &str, icon: MESSAGEBOX_STYLE) {
    let (t, b) = (wz(title), wz(body));
    unsafe {
        MessageBoxW(owner, PCWSTR(b.as_ptr()), PCWSTR(t.as_ptr()), MB_OK | icon);
    }
}

pub fn confirm(owner: Option<HWND>, title: &str, body: &str) -> bool {
    let (t, b) = (wz(title), wz(body));
    unsafe { MessageBoxW(owner, PCWSTR(b.as_ptr()), PCWSTR(t.as_ptr()), MB_YESNO | MB_ICONQUESTION) == IDYES }
}
