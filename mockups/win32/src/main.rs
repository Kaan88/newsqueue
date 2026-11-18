#![allow(clippy::all, non_snake_case)]
mod util;
use std::cell::RefCell;
use std::ffi::c_void;
use util::*;
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};

#[derive(Clone, Default)]
struct Row {
    indent: i32,
    img: i32,
    cols: Vec<String>,
    pct: u32,
    kind: u8, // 0 downloading, 1 paused, 2 completed
}

#[derive(Clone, Default)]
struct St {
    dark: bool,
    rows: Vec<Row>,
    logs: Vec<[String; 3]>,
    tb: HWND,
    qtb: HWND,
    tab: HWND,
    sb: HWND,
    pq: HWND,
    pl: HWND,
    queue: HWND,
    thr: HWND,
    log: HWND,
    qlabel: HWND,
    qedit: HWND,
    lbtn1: HWND,
    lbtn2: HWND,
    llabel: HWND,
    ledit: HWND,
    lcount: HWND,
    grays: Vec<HWND>,
    orange: Vec<HWND>,
}
thread_local! { static ST: RefCell<St> = RefCell::new(St::default()); }
fn st() -> St {
    ST.with(|s| s.borrow().clone())
}

const COL_STATUS: i32 = 6;

fn sample_rows() -> Vec<Row> {
    let r = |indent, img, c: [&str; 12], pct, kind| Row { indent, img, cols: c.iter().map(|x| x.to_string()).collect(), pct, kind };
    let dl = r"C:\NewsQueue\download\";
    let mut v = vec![r(
        0,
        1,
        ["Aurora.Field.Recordings.2025", "12.00 GB", "8", "12.4 days", "alt.binaries.movies", "northstar", "Completed", &format!("{dl}Aurora.Field.Recordings.2025"), "Yes", "", "No", "2026-10-21 21:14"],
        100,
        2,
    )];
    for i in 1..=4 {
        v.push(r(
            1,
            2,
            [&format!("Aurora.Field.Recordings.2025.part{i:02}.rar"), "3.00 GB", "2", "", "", "", "Completed", "", "", "", "", ""],
            100,
            2,
        ));
    }
    v.push(r(
        0,
        0,
        ["Harbor.Lights.1080p", "10.00 GB", "5", "3.1 days", "alt.binaries.tv", "lumen", "Downloading 63%", &format!("{dl}Harbor.Lights.1080p"), "Yes", "", "No", "2026-10-31 08:02"],
        63,
        0,
    ));
    v.push(r(
        0,
        0,
        ["Workshop.Tools.Archive.2024", "11.04 GB", "12", "60.0 days", "alt.binaries.apps", "archivebot", "Paused 38%", &format!("{dl}Workshop.Tools.Archive.2024"), "Yes", "", "Yes", "2026-09-04 17:40"],
        38,
        1,
    ));
    v
}

fn sample_logs() -> Vec<[String; 3]> {
    let l = [
        ("21:02:11", "Main", "NewsQueue 1.0.5 (2026-11-18) started."),
        ("21:02:11", "Main", "OS Windows 11"),
        ("21:02:11", "Main", "0 items loaded into download history list in 0ms."),
        ("21:02:12", "Main", "3 collections loaded into the queue."),
        ("21:02:14", "Servers", "Connecting to news.example.net:563 (SSL)."),
        ("21:02:15", "Servers", "Thread 1: connected, TLS 1.3, authenticated."),
        ("21:02:15", "Servers", "Thread 2: connected, TLS 1.3, authenticated."),
        ("21:02:15", "Servers", "Thread 3: connected, TLS 1.3, authenticated."),
        ("21:02:16", "Download", "Harbor.Lights.1080p.part01.rar: started (1/5)."),
        ("21:02:31", "Download", "Harbor.Lights.1080p.part01.rar: 2048 articles, 0 missing."),
        ("21:02:31", "Par2", "Harbor.Lights.par2: verifying 3 files."),
        ("21:02:44", "Par2", "Harbor.Lights.par2: all files OK."),
        ("21:03:02", "Unrar", "Aurora.Field.Recordings.2025: extracting 4 volumes."),
        ("21:03:40", "Unrar", "Aurora.Field.Recordings.2025: extracted without errors."),
        ("21:04:01", "Download", "Workshop.Tools.Archive.2024 paused by user."),
        ("21:04:10", "Servers", "Thread 3: idle, connection kept open."),
        ("21:04:22", "RSS", "Daily Releases: 14 items, 2 matched the filters."),
        ("21:04:22", "RSS", "Daily Releases: 2 NZB files queued."),
    ];
    l.iter().map(|(a, b, c)| [a.to_string(), b.to_string(), c.to_string()]).collect()
}

// ---------------------------------------------------------------- helpers ---

fn lv_cols(lv: HWND, cols: &[(&str, i32, bool)]) {
    for (i, (t, wd, right)) in cols.iter().enumerate() {
        let tz = wz(t);
        let mut c = LVCOLUMNW::default();
        c.mask = LVCF_TEXT | LVCF_WIDTH | LVCF_FMT;
        c.fmt = if *right { LVCFMT_RIGHT } else { LVCFMT_LEFT };
        c.cx = s(*wd);
        c.pszText = PWSTR(tz.as_ptr() as *mut u16);
        send(lv, 0x1061 /*LVM_INSERTCOLUMNW*/, i, &c as *const _ as isize);
    }
}
fn lv_item(lv: HWND, row: i32, texts: &[&str]) {
    for (i, t) in texts.iter().enumerate() {
        let tz = wz(t);
        let mut it = LVITEMW::default();
        it.mask = LVIF_TEXT;
        it.iItem = row;
        it.iSubItem = i as i32;
        it.pszText = PWSTR(tz.as_ptr() as *mut u16);
        send(lv, if i == 0 { 0x104D } else { 0x1074 }, if i == 0 { 0 } else { row as usize }, &it as *const _ as isize); // INSERTITEMW / SETITEMTEXTW
    }
}
fn lv_select(lv: HWND, row: i32) {
    let mut it = LVITEMW::default();
    it.stateMask = LIST_VIEW_ITEM_STATE_FLAGS(3);
    it.state = LIST_VIEW_ITEM_STATE_FLAGS(3);
    send(lv, 0x102B, row as usize, &it as *const _ as isize);
}
fn lv_check(lv: HWND, row: i32, on: bool) {
    let mut it = LVITEMW::default();
    it.stateMask = LIST_VIEW_ITEM_STATE_FLAGS(0xF000);
    it.state = LIST_VIEW_ITEM_STATE_FLAGS(if on { 2 << 12 } else { 1 << 12 });
    send(lv, 0x102B, row as usize, &it as *const _ as isize);
}
fn set_theme(h: HWND, name: PCWSTR) {
    unsafe {
        let _ = SetWindowTheme(h, name, PCWSTR::null());
    }
}
fn wnd_h(h: HWND) -> i32 {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(h, &mut r);
    }
    r.bottom - r.top
}
fn mv(h: HWND, x: i32, y: i32, w: i32, hh: i32) {
    unsafe {
        let _ = MoveWindow(h, x, y, w, hh, true);
    }
}
fn show(h: HWND, on: bool) {
    unsafe {
        let _ = ShowWindow(h, if on { SW_SHOW } else { SW_HIDE });
    }
}
fn cue(edit: HWND, t: &str) {
    let z = wz(t);
    send(edit, 0x1501, 1, z.as_ptr() as isize);
}

const SS_LEFT: u32 = 0;
const SS_RIGHT: u32 = 2;
const SS_NOPREFIX: u32 = 0x80;
const SS_ETCHEDHORZ: u32 = 0x10;
const CHILD: u32 = WS_CHILD.0 | WS_VISIBLE.0;

struct TbItem(&'static str, &'static str, u8, u8); // icon, text, state, style(0 button,1 sep,2 check)
fn toolbar(parent: HWND, size: i32, items: &[TbItem], text: bool, dark: bool) -> HWND {
    let tb = mkpx(
        "ToolbarWindow32",
        "",
        sty!(CHILD, WS_CLIPSIBLINGS, TBSTYLE_FLAT, TBSTYLE_TOOLTIPS, CCS_NODIVIDER, CCS_NOPARENTALIGN, CCS_NORESIZE),
        0,
        0,
        0,
        100,
        30,
        Some(parent),
        0,
    );
    send(tb, TB_BUTTONSTRUCTSIZE, std::mem::size_of::<TBBUTTON>(), 0);
    let mut names: Vec<&str> = vec![];
    for it in items {
        if it.3 != 1 && !names.contains(&it.0) {
            names.push(it.0);
        }
    }
    let il = imagelist(s(size), &names);
    send(tb, TB_SETIMAGELIST, 0, il);
    send(tb, TB_SETMAXTEXTROWS, if text { 1 } else { 0 }, 0);
    send(tb, TB_SETEXTENDEDSTYLE, 0, 0x80 /*TBSTYLE_EX_DOUBLEBUFFER*/);
    let mut btns = vec![];
    for (i, it) in items.iter().enumerate() {
        let mut b = TBBUTTON::default();
        if it.3 == 1 {
            b.fsStyle = 1;
            b.iBitmap = s(8);
        } else {
            b.iBitmap = names.iter().position(|n| *n == it.0).unwrap() as i32;
            b.idCommand = 100 + i as i32;
            b.fsState = it.2 | 4;
            b.fsStyle = it.3;
            if text {
                let z: &'static mut Vec<u16> = Box::leak(Box::new(wz(it.1)));
                b.iString = z.as_ptr() as isize;
            }
        }
        btns.push(b);
    }
    send(tb, TB_ADDBUTTONSW, btns.len(), btns.as_ptr() as isize);
    let _ = dark;
    tb
}

// -------------------------------------------------------------- main window -

unsafe extern "system" fn main_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_SIZE => {
            layout(h);
            LRESULT(0)
        }
        WM_ERASEBKGND => {
            let s = st();
            if s.dark {
                let mut rc = RECT::default();
                let _ = GetClientRect(h, &mut rc);
                FillRect(HDC(wp.0 as *mut c_void), &rc, dark_brush());
                return LRESULT(1);
            }
            DefWindowProcW(h, m, wp, lp)
        }
        WM_NOTIFY => notify(lp),
        WM_DRAWITEM => {
            let d = &*(lp.0 as *const DRAWITEMSTRUCT);
            draw_graph(d.hDC, d.rcItem, st().dark);
            LRESULT(1)
        }
        WM_CTLCOLORSTATIC => ctl_static(wp, lp, h, m),
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

fn dark_brush() -> HBRUSH {
    thread_local! { static B: HBRUSH = unsafe { CreateSolidBrush(rgb(32, 32, 32)) }; }
    B.with(|b| *b)
}

unsafe fn ctl_static(wp: WPARAM, lp: LPARAM, h: HWND, m: u32) -> LRESULT {
    let s = st();
    let dc = HDC(wp.0 as *mut c_void);
    let c = HWND(lp.0 as *mut c_void);
    if s.dark {
        SetTextColor(dc, rgb(230, 230, 230));
        SetBkMode(dc, TRANSPARENT);
        return LRESULT(GetStockObject(NULL_BRUSH).0 as isize);
    }
    if s.orange.contains(&c) {
        SetTextColor(dc, rgb(0xC0, 0x6A, 0x00));
    } else if s.grays.contains(&c) {
        SetTextColor(dc, COLORREF(GetSysColor(COLOR_GRAYTEXT)));
    } else {
        return DefWindowProcW(h, m, wp, lp);
    }
    SetBkMode(dc, TRANSPARENT);
    LRESULT(GetSysColorBrush(COLOR_BTNFACE).0 as isize)
}

unsafe fn draw_graph(dc: HDC, rc: RECT, dark: bool) {
    let bg = if dark { rgb(24, 24, 24) } else { rgb(255, 255, 255) };
    let b = CreateSolidBrush(bg);
    let inner = RECT { left: rc.left + 2, top: rc.top + 2, right: rc.right - 2, bottom: rc.bottom - 2 };
    FillRect(dc, &inner, b);
    let _ = DeleteObject(b.into());
    let pen = CreatePen(PS_SOLID, 1, rgb(46, 155, 62));
    let op = SelectObject(dc, pen.into());
    let (w, h) = (inner.right - inner.left, inner.bottom - inner.top);
    let data = [3, 5, 6, 8, 9, 9, 8, 9, 10, 9, 7, 8, 9, 10, 10, 9, 8, 9, 10, 9, 9, 8, 7, 8, 9, 9, 10, 9];
    for (i, v) in data.iter().enumerate() {
        let x = inner.left + (i as i32 * w) / (data.len() as i32 - 1);
        let y = inner.bottom - 1 - (v * (h - 2)) / 11;
        if i == 0 {
            let _ = MoveToEx(dc, x, y, None);
        } else {
            let _ = LineTo(dc, x, y);
        }
    }
    SelectObject(dc, op);
    let _ = DeleteObject(pen.into());
}

unsafe fn notify(lp: LPARAM) -> LRESULT {
    let nm = &*(lp.0 as *const NMHDR);
    let s = st();
    if nm.code == LVN_GETDISPINFOW {
        let di = &mut *(lp.0 as *mut NMLVDISPINFOW);
        let it = &mut di.item;
        let (row, sub) = (it.iItem as usize, it.iSubItem as usize);
        if nm.hwndFrom == s.queue {
            if let Some(r) = s.rows.get(row) {
                if it.mask.0 & LVIF_TEXT.0 != 0 {
                    put(it.pszText, it.cchTextMax, r.cols.get(sub).map(|x| x.as_str()).unwrap_or(""));
                }
                if it.mask.0 & LVIF_IMAGE.0 != 0 && sub == 0 {
                    it.iImage = r.img;
                }
                if it.mask.0 & LVIF_INDENT.0 != 0 {
                    it.iIndent = r.indent;
                }
            }
        } else if nm.hwndFrom == s.log {
            if let Some(r) = s.logs.get(row) {
                if it.mask.0 & LVIF_TEXT.0 != 0 {
                    put(it.pszText, it.cchTextMax, &r[sub.min(2)]);
                }
            }
        }
        return LRESULT(0);
    }
    if nm.code == NM_CUSTOMDRAW {
        if nm.hwndFrom == s.queue {
            return queue_draw(lp, &s);
        }
        if s.dark && nm.hwndFrom == s.tb || s.dark && nm.hwndFrom == s.qtb {
            let cd = &mut *(lp.0 as *mut NMTBCUSTOMDRAW);
            return match cd.nmcd.dwDrawStage {
                CDDS_PREPAINT => LRESULT(CDRF_NOTIFYITEMDRAW as isize),
                CDDS_ITEMPREPAINT => {
                    cd.clrText = rgb(235, 235, 235);
                    LRESULT(TBCDRF_USECDCOLORS as isize)
                }
                _ => LRESULT(0),
            };
        }
    }
    LRESULT(0)
}

unsafe fn put(dst: PWSTR, max: i32, t: &str) {
    let z = wz(t);
    let n = z.len().min(max.max(1) as usize);
    std::ptr::copy_nonoverlapping(z.as_ptr(), dst.0, n);
    *dst.0.add(n - 1) = 0;
}

unsafe fn queue_draw(lp: LPARAM, s: &St) -> LRESULT {
    let cd = &mut *(lp.0 as *mut NMLVCUSTOMDRAW);
    let stage = cd.nmcd.dwDrawStage.0;
    if stage == CDDS_PREPAINT.0 {
        return LRESULT(CDRF_NOTIFYITEMDRAW as isize);
    }
    if stage == CDDS_ITEMPREPAINT.0 {
        return LRESULT(CDRF_NOTIFYSUBITEMDRAW as isize);
    }
    if stage == (CDDS_ITEMPREPAINT.0 | CDDS_SUBITEM.0) {
        let row = cd.nmcd.dwItemSpec;
        let Some(r) = s.rows.get(row) else { return LRESULT(0) };
        if cd.iSubItem != COL_STATUS {
            if r.kind == 2 && !s.dark {
                cd.clrText = rgb(110, 110, 110); // completed rows are muted
            }
            return LRESULT(CDRF_NEWFONT as isize);
        }
        let dc = cd.nmcd.hdc;
        let mut rc = RECT { left: LVIR_BOUNDS as i32, top: COL_STATUS, right: 0, bottom: 0 };
        send(s.queue, 0x1038, row, &mut rc as *mut _ as isize); // LVM_GETSUBITEMRECT
        // cell background (selection)
        let sel = cd.nmcd.uItemState.0 & CDIS_SELECTED.0 != 0;
        let bgc = if s.dark { rgb(32, 32, 32) } else { rgb(255, 255, 255) };
        let b = CreateSolidBrush(bgc);
        FillRect(dc, &rc, b);
        let _ = DeleteObject(b.into());
        if sel {
            let th = OpenThemeData(Some(s.queue), w!("Explorer::ListView"));
            if th.0 != 0 {
                let _ = DrawThemeBackground(th, dc, 1, if GetFocus() == s.queue { 3 } else { 5 }, &rc, None);
                let _ = CloseThemeData(th);
            }
        }
        let bar = RECT { left: rc.left + s_(3), top: rc.top + s_(3), right: rc.right - s_(3), bottom: rc.bottom - s_(3) };
        let th = OpenThemeData(Some(s.queue), w!("PROGRESS"));
        if th.0 != 0 {
            let _ = DrawThemeBackground(th, dc, 1, 0, &bar, None); // PP_BAR
            let inner = RECT { left: bar.left + 1, top: bar.top + 1, right: bar.left + 1 + ((bar.right - bar.left - 2) * r.pct as i32) / 100, bottom: bar.bottom - 1 };
            if r.kind == 2 {
                let g = CreateSolidBrush(rgb(176, 180, 186));
                FillRect(dc, &inner, g);
                let _ = DeleteObject(g.into());
            } else {
                // PP_FILL, PBFS_NORMAL(1) / PBFS_PAUSED(3)
                let _ = DrawThemeBackground(th, dc, 5, if r.kind == 1 { 3 } else { 1 }, &inner, None);
            }
            let _ = CloseThemeData(th);
        }
        SetBkMode(dc, TRANSPARENT);
        let mut t = wz(&r.cols[COL_STATUS as usize]);
        t.pop();
        let fill_right = bar.left + 1 + ((bar.right - bar.left - 2) * r.pct as i32) / 100;
        let dark = s.dark;
        let base = if dark { rgb(255, 255, 255) } else { rgb(0, 0, 0) };
        for (l, rr, col) in [(bar.left, fill_right, if r.kind == 2 { rgb(0, 0, 0) } else { rgb(255, 255, 255) }), (fill_right, bar.right, if r.kind == 2 { rgb(0, 0, 0) } else { base })] {
            let saved = SaveDC(dc);
            IntersectClipRect(dc, l, bar.top, rr, bar.bottom);
            SetTextColor(dc, col);
            let mut tr = bar;
            let _ = DrawTextW(dc, &mut t, &mut tr, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
            RestoreDC(dc, saved);
        }
        return LRESULT(CDRF_SKIPDEFAULT as isize);
    }
    LRESULT(0)
}
fn s_(v: i32) -> i32 {
    s(v)
}

unsafe extern "system" fn page_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_ERASEBKGND => {
            let sx = st();
            let dc = HDC(wp.0 as *mut c_void);
            let mut rc = RECT::default();
            let _ = GetClientRect(h, &mut rc);
            if sx.dark {
                FillRect(dc, &rc, dark_brush());
            } else {
                // paint the same texture the tab control uses for its pane, aligned to it
                let mut tr = RECT::default();
                let _ = GetWindowRect(sx.tab, &mut tr);
                let mut pts = [POINT { x: tr.left, y: tr.top }, POINT { x: tr.right, y: tr.bottom }];
                MapWindowPoints(None, Some(h), &mut pts);
                tr = RECT { left: pts[0].x, top: pts[0].y, right: pts[1].x, bottom: pts[1].y };
                let th = OpenThemeData(Some(h), w!("TAB"));
                if th.0 != 0 {
                    let _ = DrawThemeBackground(th, dc, 10, 0, &tr, Some(&rc));
                    let _ = CloseThemeData(th);
                } else {
                    FillRect(dc, &rc, GetSysColorBrush(COLOR_WINDOW));
                }
            }
            LRESULT(1)
        }
        WM_CTLCOLORSTATIC => {
            let sx = st();
            let dc = HDC(wp.0 as *mut c_void);
            if !sx.dark {
                let _ = DrawThemeParentBackground(HWND(lp.0 as *mut c_void), dc, None);
            }
            SetBkMode(dc, TRANSPARENT);
            if sx.dark {
                SetTextColor(dc, rgb(230, 230, 230));
            }
            LRESULT(GetStockObject(NULL_BRUSH).0 as isize)
        }
        WM_NOTIFY | WM_COMMAND | WM_DRAWITEM => SendMessageW(GetParent(h).unwrap(), m, Some(wp), Some(lp)),
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

fn layout(h: HWND) {
    let s_ = st();
    if s_.tb.0.is_null() || s_.pl.0.is_null() {
        return;
    }
    let mut rc = RECT::default();
    unsafe {
        let _ = GetClientRect(h, &mut rc);
    }
    let (w, hh) = (rc.right, rc.bottom);
    send(s_.sb, WM_SIZE, 0, 0);
    let sbh = wnd_h(s_.sb);
    let tbh = hi(send(s_.tb, TB_GETBUTTONSIZE, 0, 0)) + s(6);
    mv(s_.tb, 0, 0, w, tbh);
    let th = hh - tbh - sbh;
    mv(s_.tab, 0, tbh, w, th);
    let mut r = RECT { left: 0, top: 0, right: w, bottom: th };
    send(s_.tab, TCM_ADJUSTRECT, 0, &mut r as *mut _ as isize);
    let (px, py, pw, ph) = (r.left, tbh + r.top, r.right - r.left, r.bottom - r.top);
    mv(s_.pq, px, py, pw, ph);
    mv(s_.pl, px, py, pw, ph);
    // queue page
    let qh = hi(send(s_.qtb, TB_GETBUTTONSIZE, 0, 0)) + s(6);
    let eh = s(23);
    let ew = s(170);
    mv(s_.qtb, 0, 0, pw - ew - s(70), qh);
    mv(s_.qedit, pw - ew - s(8), (qh - eh) / 2, ew, eh);
    mv(s_.qlabel, pw - ew - s(52), (qh - s(16)) / 2, s(40), s(16));
    let thr_h = s(150);
    mv(s_.queue, 0, qh, pw, ph - qh - thr_h - s(6));
    mv(s_.thr, 0, ph - thr_h, pw, thr_h);
    // log page
    let bh = s(26);
    let by = (qh - bh) / 2;
    mv(s_.lbtn1, s(8), by, s(75), bh);
    mv(s_.lbtn2, s(91), by, s(75), bh);
    mv(s_.llabel, s(182), (qh - s(16)) / 2, s(40), s(16));
    mv(s_.ledit, s(230), (qh - eh) / 2, s(200), eh);
    mv(s_.lcount, pw - s(110), (qh - s(16)) / 2, s(100), s(16));
    mv(s_.log, 0, qh, pw, ph - qh);
}

fn register(name: PCWSTR, proc: WNDPROC, bg: HBRUSH) {
    unsafe {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: proc,
            hInstance: hinst(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            hbrBackground: bg,
            lpszClassName: name,
            ..Default::default()
        };
        RegisterClassExW(&wc);
    }
}

fn work_area() -> (i32, i32) {
    unsafe {
        let mut r = RECT::default();
        let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut r as *mut _ as *mut c_void), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
        (r.right - r.left, r.bottom - r.top)
    }
}

fn dark_title(h: HWND) {
    unsafe {
        let v: i32 = 1;
        let _ = DwmSetWindowAttribute(h, DWMWA_USE_IMMERSIVE_DARK_MODE, &v as *const _ as *const c_void, 4);
    }
}

fn dark_lv(lv: HWND) {
    set_theme(lv, w!("DarkMode_Explorer"));
    send(lv, 0x1001, 0, rgb(32, 32, 32).0 as isize);
    send(lv, 0x1026, 0, rgb(32, 32, 32).0 as isize);
    send(lv, 0x1024, 0, rgb(235, 235, 235).0 as isize);
    let hd = HWND(send(lv, 0x101F, 0, 0) as *mut c_void);
    set_theme(hd, w!("DarkMode_ItemsView"));
}

fn make_main(tab: usize, dark: bool) -> HWND {
    unsafe {
        let mut s0 = St::default();
        s0.dark = dark;
        s0.rows = sample_rows();
        s0.logs = sample_logs();
        ST.with(|x| *x.borrow_mut() = s0);
        let (waw, wah) = work_area();
        let dpi = DPI.load(std::sync::atomic::Ordering::Relaxed) as i32;
        let cw = 1285.min(waw * 96 / dpi - 24);
        let ch = 840.min(wah * 96 / dpi - 70);
        let style = WS_OVERLAPPEDWINDOW.0 | WS_CLIPCHILDREN.0;
        let (ow, oh) = outer(style, 0, cw, ch);
        let main = mkpx("NQMain", "NewsQueue 1.0.5", style, 0, s(20), s(10), ow, oh, None, 0);
        if dark {
            dark_title(main);
        }
        // main toolbar (text under buttons)
        let tb = toolbar(
            main,
            32,
            &[
                TbItem("connect", "Connect", 1, 2),
                TbItem("", "", 0, 1),
                TbItem("pause", "Pause", 0, 0),
                TbItem("pausefor", "Pause for", 0, 0),
                TbItem("", "", 0, 1),
                TbItem("nzb", "NZB", 0, 0),
                TbItem("", "", 0, 1),
                TbItem("download", "Download", 0, 0),
                TbItem("", "", 0, 1),
                TbItem("cleanup", "Cleanup", 0, 0),
                TbItem("", "", 0, 1),
                TbItem("scheduler", "Scheduler", 0, 2),
                TbItem("", "", 0, 1),
                TbItem("shutdown", "Shutdown", 0, 0),
                TbItem("", "", 0, 1),
                TbItem("setup", "Setup", 0, 0),
                TbItem("", "", 0, 1),
                TbItem("home", "Home", 0, 0),
                TbItem("version", "Version?", 0, 0),
                TbItem("about", "About", 0, 0),
                TbItem("notes", "Notes", 0, 0),
                TbItem("stats", "Stats", 0, 0),
                TbItem("tray", "To tray", 0, 0),
            ],
            true,
            dark,
        );
        // tab control
        let tabc = mkpx("SysTabControl32", "", sty!(CHILD, WS_CLIPSIBLINGS, WS_CLIPCHILDREN, TCS_FOCUSNEVER), 0, 0, 0, 100, 100, Some(main), 1);
        let til = imagelist(s(16), &["queue", "shield", "page", "rss"]);
        send(tabc, TCM_SETIMAGELIST, 0, til);
        for (i, t) in ["Download queue", "PAR2", "Log", "RSS"].iter().enumerate() {
            let z = wz(t);
            let mut it = TCITEMW::default();
            it.mask = TCIF_TEXT | TCIF_IMAGE;
            it.pszText = PWSTR(z.as_ptr() as *mut u16);
            it.iImage = i as i32;
            send(tabc, TCM_INSERTITEMW, i, &it as *const _ as isize);
        }
        send(tabc, TCM_SETCURSEL, tab, 0);
        // pages
        let pq = mkpx("NQPage", "", sty!(WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_VISIBLE), 0, 0, 0, 10, 10, Some(main), 2);
        let pl = mkpx("NQPage", "", sty!(WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS), 0, 0, 0, 10, 10, Some(main), 3);
        // queue page
        let qtb = toolbar(
            pq,
            16,
            &[
                TbItem("play", "", 0, 0),
                TbItem("pause_s", "", 0, 0),
                TbItem("", "", 0, 1),
                TbItem("top", "", 0, 0),
                TbItem("up", "", 0, 0),
                TbItem("down", "", 0, 0),
                TbItem("bottom", "", 0, 0),
                TbItem("", "", 0, 1),
                TbItem("collapse", "", 0, 0),
                TbItem("expand", "", 0, 0),
                TbItem("", "", 0, 1),
                TbItem("unrar", "", 0, 0),
                TbItem("nounrar", "", 0, 0),
                TbItem("folder", "", 0, 0),
                TbItem("", "", 0, 1),
                TbItem("rename", "", 0, 0),
                TbItem("export", "", 0, 0),
                TbItem("folder", "", 0, 0),
                TbItem("", "", 0, 1),
                TbItem("key", "", 0, 0),
                TbItem("eta", "", 0, 0),
            ],
            false,
            dark,
        );
        let qlabel = mkpx("STATIC", "Filter", sty!(CHILD, SS_RIGHT), 0, 0, 0, 10, 10, Some(pq), 0);
        let qedit = mkpx("EDIT", "", sty!(CHILD, WS_TABSTOP, ES_AUTOHSCROLL), WS_EX_CLIENTEDGE.0, 0, 0, 10, 10, Some(pq), 0);
        cue(qedit, "Filter subjects");
        let lvstyle = sty!(CHILD, WS_TABSTOP, LVS_REPORT, LVS_SHOWSELALWAYS, LVS_NOSORTHEADER);
        let queue = mkpx("SysListView32", "", lvstyle | 0x1000 /*LVS_OWNERDATA*/, WS_EX_CLIENTEDGE.0, 0, 0, 10, 10, Some(pq), 10);
        send(queue, 0x1036, 0, 0x20 | 0x10000 | 0x10 | 0x4000);
        set_theme(queue, w!("Explorer"));
        let names = ["queue_cols"; 0];
        let _ = names;
        let cols: [(&str, i32, bool); 12] = [
            ("Subject", 390, false),
            ("Size", 95, true),
            ("Parts", 95, true),
            ("Age", 95, false),
            ("Group", 95, false),
            ("Poster", 151, false),
            ("Status", 95, false),
            ("Download folder", 220, false),
            ("Unrar", 38, false),
            ("Execute", 95, false),
            ("Delete archives", 50, false),
            ("Posted on", 50, false),
        ];
        // widths from QUEUE_COLUMN_WIDTHS (390,95,95,95,95,151,95,220,38,95,50,50) mapped to the
        // column names; Status is widened to hold the progress bar.
        let widths = [390, 95, 95, 95, 95, 151, 95, 220, 38, 95, 50, 50];
        let mut cols2 = cols;
        for i in 0..12 {
            cols2[i].1 = widths[i];
        }
        cols2[COL_STATUS as usize].1 = 150;
        cols2[10] = ("Delete archives", 100, false);
        cols2[11] = ("Posted on", 120, false);
        cols2[9] = ("Execute", 70, false);
        lv_cols(queue, &cols2);
        let sil = imagelist(s(16), &["plus", "minus", "file"]);
        send(queue, 0x1003, 1, sil);
        send(queue, 0x102F, st_rows_len(), 0);
        // threads list
        let thr = mkpx("SysListView32", "", lvstyle, WS_EX_CLIENTEDGE.0, 0, 0, 10, 10, Some(pq), 11);
        send(thr, 0x1036, 0, 0x20 | 0x10000 | 0x10);
        set_theme(thr, w!("Explorer"));
        lv_cols(thr, &[("#", 34, true), ("Action", 110, false), ("Description", 330, false), ("Status", 130, false), ("Server", 180, false)]);
        lv_item(thr, 0, &["1", "Download", "Harbor.Lights.1080p.part03.rar  [412/1024]", "Downloading", "news.example.net"]);
        lv_item(thr, 1, &["2", "Download", "Harbor.Lights.1080p.part03.rar  [413/1024]", "Downloading", "news.example.net"]);
        lv_item(thr, 2, &["3", "Download", "Harbor.Lights.1080p.part04.rar  [88/1024]", "Downloading", "news.example.net"]);
        lv_item(thr, 3, &["4", "Download", "Harbor.Lights.1080p.part04.rar  [89/1024]", "Downloading", "backup.example.org"]);
        lv_item(thr, 4, &["5", "Idle", "", "Connected", "backup.example.org"]);
        // log page
        let lbtn1 = mkpx("BUTTON", "Clear", sty!(CHILD, WS_TABSTOP, BS_PUSHBUTTON), 0, 0, 0, 10, 10, Some(pl), 0);
        let lbtn2 = mkpx("BUTTON", "Copy", sty!(CHILD, WS_TABSTOP, BS_PUSHBUTTON), 0, 0, 0, 10, 10, Some(pl), 0);
        let llabel = mkpx("STATIC", "Filter", sty!(CHILD, SS_RIGHT), 0, 0, 0, 10, 10, Some(pl), 0);
        let ledit = mkpx("EDIT", "", sty!(CHILD, WS_TABSTOP, ES_AUTOHSCROLL), WS_EX_CLIENTEDGE.0, 0, 0, 10, 10, Some(pl), 0);
        cue(ledit, "Filter log lines");
        let lcount = mkpx("STATIC", "18 lines", sty!(CHILD, SS_RIGHT), 0, 0, 0, 10, 10, Some(pl), 0);
        let log = mkpx("SysListView32", "", lvstyle | 0x1000, WS_EX_CLIENTEDGE.0, 0, 0, 10, 10, Some(pl), 12);
        send(log, 0x1036, 0, 0x20 | 0x10000 | 0x10);
        set_theme(log, w!("Explorer"));
        lv_cols(log, &[("Time", 80, false), ("Level", 90, false), ("Message", 900, false)]);
        send(log, 0x102F, 18, 0);
        // status bar
        let sb = mkpx("msctls_statusbar32", "", sty!(CHILD, SBARS_SIZEGRIP, WS_CLIPSIBLINGS), 0, 0, 0, 0, 0, Some(main), 4);
        let wd = [90, 90, 120, 100, 100, 100, 105, 105];
        let mut edges = vec![];
        let mut acc = 0;
        for x in wd {
            acc += s(x);
            edges.push(acc);
        }
        edges.push(-1);
        send(sb, SB_SETPARTS, edges.len(), edges.as_ptr() as isize);
        let texts = ["\tNo limit", "\t4.82 MB/s", "", "\t1.37 GB", "\tETA 1h 07m", "\t19.73 GB", "\tFree: 412.33 GB", "\tFree: 412.33 GB", ""];
        for (i, t) in texts.iter().enumerate() {
            let z: &'static mut Vec<u16> = Box::leak(Box::new(wz(t)));
            let flag = if i == 2 { 0x1000 } else { 0 };
            send(sb, SB_SETTEXTW, i | flag, z.as_ptr() as isize);
        }
        ST.with(|x| {
            let mut a = x.borrow_mut();
            a.tb = tb;
            a.qtb = qtb;
            a.tab = tabc;
            a.sb = sb;
            a.pq = pq;
            a.queue = queue;
            a.thr = thr;
            a.log = log;
            a.qlabel = qlabel;
            a.qedit = qedit;
            a.lbtn1 = lbtn1;
            a.lbtn2 = lbtn2;
            a.llabel = llabel;
            a.ledit = ledit;
            a.lcount = lcount;
            a.pl = pl;
        });
        if dark {
            for l in [queue, thr, log] {
                dark_lv(l);
            }
        }
        show(pq, tab == 0);
        show(pl, tab == 2);
        let _ = SetWindowPos(pq, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        let _ = SetWindowPos(pl, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        layout(main);
        let _ = ShowWindow(main, SW_SHOW);
        let _ = UpdateWindow(main);
        let _ = SetCursorPos(5, 500);
        pump(600);
        lv_select(if tab == 0 { queue } else { log }, if tab == 0 { 5 } else { 8 });
        let _ = SetFocus(Some(if tab == 0 { queue } else { log }));
        main
    }
}
fn st_rows_len() -> usize {
    ST.with(|s| s.borrow().rows.len())
}

// ----------------------------------------------------------------- dialogs --

unsafe extern "system" fn dlg_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_CTLCOLORSTATIC => ctl_static(wp, lp, h, m),
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

fn dialog(title: &str, cw: i32, ch: i32) -> HWND {
    let style = sty!(WS_POPUP, WS_CAPTION, WS_SYSMENU);
    let ex = sty!(WS_EX_DLGMODALFRAME, WS_EX_CONTROLPARENT);
    let (ow, oh) = outer(style, ex, cw, ch);
    ST.with(|x| *x.borrow_mut() = St::default());
    mkpx("NQDlg", title, style, ex, s(160), s(60), ow, oh, None, 0)
}
fn finish(d: HWND) {
    unsafe {
        let _ = ShowWindow(d, SW_SHOW);
        let _ = UpdateWindow(d);
    }
    pump(500);
}
fn label(p: HWND, t: &str, x: i32, y: i32, wd: i32) -> HWND {
    mk("STATIC", t, sty!(CHILD, SS_LEFT, SS_NOPREFIX), 0, x, y + 4, wd, 16, p, 0)
}
fn note(p: HWND, t: &str, x: i32, y: i32, wd: i32, ht: i32) -> HWND {
    let n = mk("STATIC", t, sty!(CHILD, SS_LEFT), 0, x, y, wd, ht, p, 0);
    ST.with(|s| s.borrow_mut().grays.push(n));
    n
}
fn edit(p: HWND, t: &str, x: i32, y: i32, wd: i32, extra: u32) -> HWND {
    mk("EDIT", t, sty!(CHILD, WS_TABSTOP, ES_AUTOHSCROLL) | extra, WS_EX_CLIENTEDGE.0, x, y, wd, 23, p, 0)
}
/// number edit + up-down buddy, returns the edit
fn spin(p: HWND, val: i32, lo_: i32, hi_: i32, x: i32, y: i32, wd: i32) -> HWND {
    let e = edit(p, &val.to_string(), x, y, wd, sty!(ES_NUMBER));
    let u = mk("msctls_updown32", "", sty!(CHILD, UDS_SETBUDDYINT, UDS_ALIGNRIGHT, UDS_ARROWKEYS, UDS_NOTHOUSANDS), 0, 0, 0, 10, 10, p, 0);
    send(u, UDM_SETBUDDY, e.0 as usize, 0);
    send(u, UDM_SETRANGE32, lo_ as usize, hi_ as isize);
    send(u, UDM_SETPOS32, 0, val as isize);
    e
}
fn button(p: HWND, t: &str, x: i32, y: i32, wd: i32, def: bool) -> HWND {
    mk("BUTTON", t, sty!(CHILD, WS_TABSTOP) | if def { BS_DEFPUSHBUTTON as u32 } else { BS_PUSHBUTTON as u32 }, 0, x, y, wd, 26, p, 0)
}
fn check(p: HWND, t: &str, x: i32, y: i32, wd: i32, on: bool) -> HWND {
    let c = mk("BUTTON", t, sty!(CHILD, WS_TABSTOP, BS_AUTOCHECKBOX), 0, x, y, wd, 20, p, 0);
    send(c, BM_SETCHECK, on as usize, 0);
    c
}
fn hline(p: HWND, x: i32, y: i32, wd: i32) {
    mk("STATIC", "", sty!(CHILD, SS_ETCHEDHORZ), 0, x, y, wd, 2, p, 0);
}

fn setup_dialog(page: usize) -> HWND {
    let d = dialog("Setup", 700, 560);
    let names = [
        "Download", "Download #2", "NZB", "NZB #2", "Servers", "Log", "Connection", "Proxy", "Scheduler", "Par2", "Unpack", "Miscellaneous", "Miscellaneous #2", "Miscellaneous #3", "User interface",
    ];
    let tree = mk("SysTreeView32", "", sty!(CHILD, WS_TABSTOP, TVS_SHOWSELALWAYS, TVS_FULLROWSELECT, TVS_NOHSCROLL, TVS_DISABLEDRAGDROP), WS_EX_CLIENTEDGE.0, 10, 10, 168, 484, d, 20);
    set_theme(tree, w!("Explorer"));
    send(tree, TVM_SETITEMHEIGHT, s(24) as usize, 0);
    let mut sel = 0isize;
    for (i, n) in names.iter().enumerate() {
        let z = wz(n);
        let mut ins = TVINSERTSTRUCTW::default();
        ins.hParent = TVI_ROOT;
        ins.hInsertAfter = TVI_LAST;
        ins.Anonymous.item.mask = TVIF_TEXT;
        ins.Anonymous.item.pszText = PWSTR(z.as_ptr() as *mut u16);
        let hi = send(tree, TVM_INSERTITEMW, 0, &ins as *const _ as isize);
        if i == page {
            sel = hi;
        }
    }
    send(tree, TVM_SELECTITEM, 9, sel);
    let hd = mk("STATIC", names[page], sty!(CHILD, SS_LEFT), 0, 192, 12, 400, 20, d, 0);
    send(hd, WM_SETFONT, bold().0 as usize, 1);
    hline(d, 192, 34, 498);
    match page {
        0 => {
            let rows = [("Default download folder", r"C:\NewsQueue\download"), ("Default NZB folder", r"C:\NewsQueue\nzb"), ("Temporary folder", r"C:\NewsQueue\temp"), ("Unrar folder", r"C:\NewsQueue\unrar")];
            for (i, (l, v)) in rows.iter().enumerate() {
                let y = 50 + i as i32 * 32;
                label(d, l, 192, y, 185);
                edit(d, v, 380, y, 242, 0);
                button(d, "Browse...", 630, y - 1, 60, false);
            }
            label(d, "If free space on destination drive gets below", 192, 190, 250);
            spin(d, 500, 0, 1_000_000, 448, 190, 70);
            label(d, "MB  - pause downloading", 526, 190, 160);
            label(d, "Autosave download queue every", 192, 222, 250);
            spin(d, 5, 1, 999, 448, 222, 70);
            label(d, "min", 526, 222, 60);
            label(d, "Cleanup", 192, 262, 185);
            check(d, "Par2 list", 380, 262, 120, true);
            label(d, "Skip if file is already downloaded", 192, 292, 250);
            check(d, "", 380, 292, 20, true);
            label(d, "Dblclick toggles expand/collapse", 192, 322, 250);
            check(d, "", 380, 322, 20, true);
            note(d, "Use the Columns button at the bottom of the Setup window to edit the queue columns.", 192, 364, 498, 32);
        }
        _ => {
            for (i, (t, wd)) in [("Add server", 90), ("Edit server", 90), ("Delete server", 90), ("Move up", 80), ("Move down", 90)].iter().enumerate() {
                let xs = [192, 290, 388, 486, 574];
                button(d, t, xs[i], 44, *wd, false);
            }
            let lv = mk("SysListView32", "", sty!(CHILD, WS_TABSTOP, LVS_REPORT, LVS_SHOWSELALWAYS, LVS_SINGLESEL), WS_EX_CLIENTEDGE.0, 192, 80, 498, 200, d, 21);
            send(lv, 0x1036, 0, 0x20 | 0x10000 | 0x4 | 0x10);
            set_theme(lv, w!("Explorer"));
            lv_cols(lv, &[("Name", 120, false), ("Address", 130, false), ("Port", 40, true), ("Conn.", 44, true), ("SSL", 36, false), ("Level", 112, false)]);
            lv_item(lv, 0, &["Primary provider", "news.example.net", "563", "20", "Yes", "Level 0 (Primary)"]);
            lv_item(lv, 1, &["Backup provider", "backup.example.org", "563", "8", "Yes", "Level 1 (Backup)"]);
            lv_item(lv, 2, &["Block account", "alt.example.com", "119", "4", "No", "Level 2 (Backup)"]);
            lv_check(lv, 0, true);
            lv_check(lv, 1, true);
            lv_check(lv, 2, false);
            lv_select(lv, 0);
            note(d, "A server needs a name, an address and a non-zero port. SSL switches the default port between 563 and 119.", 192, 290, 498, 34);
        }
    }
    hline(d, 10, 504, 680);
    button(d, "Columns...", 10, 518, 90, false);
    button(d, "OK", 434, 518, 80, true);
    button(d, "Cancel", 522, 518, 80, false);
    button(d, "Apply", 610, 518, 80, false);
    finish(d);
    d
}

fn server_dialog() -> HWND {
    let d = dialog("Edit server Primary provider", 580, 340);
    label(d, "Name", 14, 16, 80);
    edit(d, "Primary provider", 110, 16, 180, 0);
    label(d, "Nr. of connections", 312, 16, 110);
    spin(d, 20, 1, 50, 430, 16, 50);
    check(d, "Select group", 490, 17, 90, true);
    label(d, "Address", 14, 48, 80);
    edit(d, "news.example.net", 110, 48, 180, 0);
    label(d, "Port", 312, 48, 110);
    spin(d, 563, 1, 65535, 430, 48, 62);
    check(d, "SSL", 504, 49, 60, true);
    mk("BUTTON", "", sty!(CHILD, BS_GROUPBOX), 0, 14, 86, 552, 108, d, 0);
    check(d, "Server requires authentication", 24, 82, 210, true);
    label(d, "Username", 28, 110, 70);
    edit(d, "demo-user", 110, 110, 180, 0);
    label(d, "Password", 312, 110, 70);
    edit(d, "secret-pass", 386, 110, 168, sty!(ES_PASSWORD));
    check(d, "ARTICLE cmd", 28, 142, 150, false);
    check(d, "Connected while idle", 28, 164, 200, true);
    label(d, "Server purpose", 14, 208, 90);
    edit(d, "", 110, 208, 130, 0);
    label(d, "Max. age", 312, 208, 60);
    spin(d, 3000, 0, 9999, 386, 208, 66);
    label(d, "days", 458, 208, 40);
    label(d, "Network adapter", 14, 240, 94);
    edit(d, "", 110, 240, 130, 0);
    hline(d, 14, 288, 552);
    button(d, "Save", 398, 300, 80, true);
    button(d, "Cancel", 486, 300, 80, false);
    finish(d);
    d
}

fn password_dialog() -> HWND {
    let d = dialog("Enter password", 380, 150);
    note(d, "Aurora.Field.Recordings.2025 is password protected.", 14, 14, 352, 18);
    label(d, "Password", 14, 40, 80);
    let e = edit(d, "hunter2", 14, 62, 352, sty!(ES_PASSWORD));
    let _ = e;
    hline(d, 14, 100, 352);
    button(d, "OK", 200, 112, 80, true);
    button(d, "Cancel", 288, 112, 78, false);
    finish(d);
    d
}

fn shutdown_dialog() -> HWND {
    let d = dialog("Shutdown", 420, 250);
    label(d, "Shutdown method", 14, 10, 200);
    let cb = mk("COMBOBOX", "", sty!(CHILD, WS_TABSTOP, WS_VSCROLL, CBS_DROPDOWNLIST), 0, 14, 32, 220, 160, d, 30);
    for t in ["Shutdown Windows", "Hibernate", "Sleep", "Exit"] {
        let z = wz(t);
        send(cb, 0x143, 0, z.as_ptr() as isize);
    }
    send(cb, 0x14E, 0, 0);
    check(d, "Run this action when all tasks are done", 14, 70, 330, true);
    let o = mk("STATIC", "Shutdown Windows in 12s", sty!(CHILD, SS_LEFT), 0, 14, 104, 240, 18, d, 0);
    ST.with(|s| s.borrow_mut().orange.push(o));
    button(d, "Abort", 322, 98, 84, false);
    note(d, "After a 15 second countdown NewsQueue runs the selected action. Shutdown, Hibernate and Sleep affect Windows; Exit only closes NewsQueue.", 14, 134, 392, 48);
    hline(d, 14, 196, 392);
    button(d, "OK", 326, 208, 80, true);
    finish(d);
    d
}

// -------------------------------------------------------------------- main --

fn shot(name: &str, h: HWND) {
    capture(h, &format!("shots/{name}.png"));
    unsafe {
        let _ = DestroyWindow(h);
    }
    pump(150);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if !args.iter().any(|a| a == "--capture") {
        println!("usage: nqmock --capture [--dark]  (writes shots/*.png)");
        return;
    }
    unsafe {
        let _ = std::fs::create_dir_all("shots");
        init_fonts();
        let icc = INITCOMMONCONTROLSEX { dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_WIN95_CLASSES | ICC_STANDARD_CLASSES | ICC_UPDOWN_CLASS | ICC_TAB_CLASSES | ICC_LISTVIEW_CLASSES | ICC_TREEVIEW_CLASSES | ICC_BAR_CLASSES };
        let _ = InitCommonControlsEx(&icc);
        register(w!("NQMain"), Some(main_proc), GetSysColorBrush(COLOR_BTNFACE));
        register(w!("NQPage"), Some(page_proc), HBRUSH::default());
        register(w!("NQDlg"), Some(dlg_proc), GetSysColorBrush(COLOR_BTNFACE));
        // dark-mode opt-in for scrollbars/menus (uxtheme ordinal 135 = SetPreferredAppMode)
        if let Ok(ux) = LoadLibraryW(w!("uxtheme.dll")) {
            if let Some(f) = GetProcAddress(ux, windows::core::PCSTR(135 as *const u8)) {
                let f: extern "system" fn(i32) -> i32 = std::mem::transmute(f);
                f(if args.iter().any(|a| a == "--dark") { 2 } else { 0 });
            }
        }
    }
    if args.iter().any(|a| a == "--dark") {
        shot("08-main-queue-dark", make_main(0, true));
        return;
    }
    shot("01-main-queue", make_main(0, false));
    shot("02-main-log", make_main(2, false));
    shot("03-setup-download", setup_dialog(0));
    shot("04-setup-servers", setup_dialog(4));
    shot("05-server-editor", server_dialog());
    shot("06-unrar-password", password_dialog());
    shot("07-shutdown-countdown", shutdown_dialog());
}

fn dbg_rects(v: &[HWND]) {
    for h in v {
        let mut r = RECT::default();
        unsafe {
            let _ = GetWindowRect(*h, &mut r);
            println!("{:?} {:?} vis={}", h.0, (r.left, r.top, r.right, r.bottom), IsWindowVisible(*h).as_bool());
        }
    }
}
