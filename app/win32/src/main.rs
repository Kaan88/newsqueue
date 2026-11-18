//! NewsQueue native Win32 front end (GUI_PLAN Phase 2, first slice).
//!
//! Real common controls (comctl32 v6) over the same engine as the egui app:
//! the engine runs on its own thread (`EngineHost`), this window only sends
//! `Command`s and redraws from the `Snapshot` on a timer. Because the engine
//! no longer depends on the window ticking, the window minimizes normally.
//!
//! ```text
//! newsqueue-win32 [--config DIR] [--capture out.png] [file.nzb ...]
//! ```
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(clippy::all, non_snake_case)]
mod util;

use newsqueue_core::command::{Command, MoveDir};
use newsqueue_core::engine::{Engine, Event};
use newsqueue_core::host::{EngineHost, TICK_INTERVAL};
use newsqueue_core::model::{format_size_short, ItemStatus, Settings, VERSION};
use newsqueue_core::snapshot::{EngineHandle, Snapshot};
use std::cell::RefCell;
use std::ffi::c_void;
use util::*;
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::Dialogs::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetFocus, SetFocus};
use windows::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::*;

const CHILD: u32 = WS_CHILD.0 | WS_VISIBLE.0;
const TIMER_ID: usize = 1;

// Toolbar / menu command ids.
const ID_CONNECT: i32 = 100;
const ID_PAUSE: i32 = 101;
const ID_ADD: i32 = 102;
const ID_CLEANUP: i32 = 103;
const ID_TOP: i32 = 104;
const ID_UP: i32 = 105;
const ID_DOWN: i32 = 106;
const ID_BOTTOM: i32 = 107;
const ID_UNPACK: i32 = 108;
const ID_DELETE: i32 = 109;
const ID_ITEM_PAUSE: i32 = 110;
const ID_ITEM_RESUME: i32 = 111;
const ID_PASSWORD: i32 = 112;

/// Queue columns: title, width (96 dpi), right-aligned.
const COLS: [(&str, i32, bool); 7] = [
    ("Subject", 360, false),
    ("Size", 80, true),
    ("Left", 80, true),
    ("Status", 170, false),
    ("Files", 50, true),
    ("Group", 170, false),
    ("Folder", 320, false),
];
const COL_STATUS: i32 = 3;

#[derive(Default)]
struct Ui {
    main: HWND,
    tb: HWND,
    tab: HWND,
    queue: HWND,
    log: HWND,
    sb: HWND,
    snap: Snapshot,
    handle: Option<EngineHandle>,
}
thread_local! {
    static UI: RefCell<Ui> = RefCell::new(Ui::default());
    static HOST: RefCell<Option<EngineHost>> = RefCell::new(None);
}
fn ui<R>(f: impl FnOnce(&mut Ui) -> R) -> R {
    UI.with(|u| f(&mut u.borrow_mut()))
}
fn send_cmd(c: Command) {
    if let Some(h) = ui(|u| u.handle.clone()) {
        h.send(c);
    }
}

fn row_status(r: &newsqueue_core::snapshot::QueueRow) -> String {
    let st = r.status.unwrap_or(ItemStatus::Queued);
    match st {
        ItemStatus::Downloading | ItemStatus::Paused | ItemStatus::Queued if r.progress > 0.0 => {
            format!("{} {:.0}%", st.label(), r.progress * 100.0)
        }
        _ => st.label().to_string(),
    }
}

fn cell(r: &newsqueue_core::snapshot::QueueRow, col: usize) -> String {
    match col {
        0 => r.subject.clone(),
        1 => format_size_short(r.total_bytes),
        2 => format_size_short(r.remaining_bytes.max(0.0) as u64),
        3 => row_status(r),
        4 => r.files.to_string(),
        5 => r.group.clone(),
        6 => r.folder.clone(),
        _ => String::new(),
    }
}

// ------------------------------------------------------------- startup ----

fn main() {
    let mut args = std::env::args().skip(1);
    let mut capture: Option<String> = None;
    let mut nzbs: Vec<String> = vec![];
    while let Some(a) = args.next() {
        match a.as_str() {
            "--config" => {
                if let Some(d) = args.next() {
                    std::fs::create_dir_all(&d).ok();
                    std::env::set_var("NQ_CONFIG", d);
                }
            }
            "--capture" => capture = args.next(),
            other => nzbs.push(other.to_string()),
        }
    }
    unsafe {
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_WIN95_CLASSES | ICC_BAR_CLASSES | ICC_TAB_CLASSES | ICC_LISTVIEW_CLASSES | ICC_PROGRESS_CLASS,
        };
        let _ = InitCommonControlsEx(&icc);
    }
    init_fonts();

    let mut host = EngineHost::new(Engine::new(Settings::load(), false));
    host.log("Main", &format!("NewsQueue {VERSION} (native Win32 front end)"));
    host.start(TICK_INTERVAL);
    host.checkin();
    let handle = host.handle();
    HOST.with(|h| *h.borrow_mut() = Some(host));
    ui(|u| {
        u.snap = handle.snapshot();
        u.handle = Some(handle);
    });

    nzbs.into_iter().for_each(add_nzb);
    let main = create_main();
    unsafe {
        let _ = ShowWindow(main, SW_SHOW);
        SetTimer(Some(main), TIMER_ID, 250, None);
    }
    refresh();
    if let Some(path) = capture {
        pump(600);
        refresh();
        util::capture(main, &path);
        unsafe {
            let _ = DestroyWindow(main);
        }
    }
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    // Stop the engine thread and save, as the egui app does on exit.
    HOST.with(|h| {
        if let Some(mut host) = h.borrow_mut().take() {
            host.checkout();
            host.save_all();
        }
    });
}

fn create_main() -> HWND {
    unsafe {
        let cls = w!("NewsQueueNative");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(main_proc),
            hInstance: hinst(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: GetSysColorBrush(COLOR_BTNFACE),
            lpszClassName: cls,
            hIcon: LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let style = WS_OVERLAPPEDWINDOW.0 | WS_CLIPCHILDREN.0;
        let (w, h) = outer(style, 0, 1285, 700);
        let title = wz(&format!("NewsQueue {VERSION}"));
        let main = CreateWindowExW(
            WS_EX_ACCEPTFILES,
            cls,
            PCWSTR(title.as_ptr()),
            WINDOW_STYLE(style),
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            w,
            h,
            None,
            None,
            Some(hinst()),
            None,
        )
        .expect("main window");
        DragAcceptFiles(main, true);

        let tb = toolbar(main);
        let tab = mkpx("SysTabControl32", "", sty!(CHILD, WS_CLIPSIBLINGS), 0, 0, 0, 10, 10, Some(main), 0);
        for (i, t) in ["Queue", "Log"].iter().enumerate() {
            let z = wz(t);
            let mut it = TCITEMW::default();
            it.mask = TCIF_TEXT;
            it.pszText = PWSTR(z.as_ptr() as *mut u16);
            send(tab, TCM_INSERTITEMW, i, &it as *const _ as isize);
        }
        let lvs = sty!(CHILD, WS_CLIPSIBLINGS, LVS_REPORT, LVS_OWNERDATA, LVS_SHOWSELALWAYS);
        let queue = mkpx("SysListView32", "", lvs, WS_EX_CLIENTEDGE.0, 0, 0, 10, 10, Some(main), 0);
        let log = mkpx("SysListView32", "", lvs | LVS_NOSORTHEADER, WS_EX_CLIENTEDGE.0, 0, 0, 10, 10, Some(main), 0);
        for lv in [queue, log] {
            let _ = SetWindowTheme(lv, w!("Explorer"), PCWSTR::null());
            let ex = (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER | LVS_EX_LABELTIP) as isize;
            send(lv, LVM_SETEXTENDEDLISTVIEWSTYLE, ex as usize, ex);
        }
        lv_cols(queue, &COLS);
        lv_cols(log, &[("Time", 70, false), ("Area", 80, false), ("Message", 900, false)]);
        let _ = ShowWindow(log, SW_HIDE);
        let sb = mkpx("msctls_statusbar32", "", sty!(CHILD, SBARS_SIZEGRIP), 0, 0, 0, 0, 0, Some(main), 0);
        let parts = [s(160), s(330), s(520), -1];
        send(sb, SB_SETPARTS, parts.len(), parts.as_ptr() as isize);

        ui(|u| {
            u.main = main;
            u.tb = tb;
            u.tab = tab;
            u.queue = queue;
            u.log = log;
            u.sb = sb;
        });
        layout(main);
        main
    }
}

fn toolbar(parent: HWND) -> HWND {
    let items: [(i32, &str, &str); 10] = [
        (ID_CONNECT, "connect", "Connect"),
        (ID_PAUSE, "pause", "Pause"),
        (ID_ADD, "nzb", "Add NZB"),
        (ID_CLEANUP, "cleanup", "Clean up"),
        (ID_TOP, "top", "Top"),
        (ID_UP, "up", "Up"),
        (ID_DOWN, "down", "Down"),
        (ID_BOTTOM, "bottom", "Bottom"),
        (ID_UNPACK, "unrar", "Unpack"),
        (ID_DELETE, "delete", "Delete"),
    ];
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
    let names: Vec<&str> = items.iter().map(|i| i.1).collect();
    send(tb, TB_SETIMAGELIST, 0, imagelist(s(24), &names));
    send(tb, TB_SETEXTENDEDSTYLE, 0, 0x80 /* TBSTYLE_EX_DOUBLEBUFFER */);
    let mut btns = vec![];
    for (i, (id, _, text)) in items.iter().enumerate() {
        if *id == ID_TOP || *id == ID_UNPACK {
            let mut sep = TBBUTTON::default();
            sep.fsStyle = 1;
            sep.iBitmap = s(8);
            btns.push(sep);
        }
        let mut b = TBBUTTON::default();
        b.iBitmap = i as i32;
        b.idCommand = *id;
        b.fsState = TBSTATE_ENABLED as u8;
        let z: &'static mut Vec<u16> = Box::leak(Box::new(wz(text)));
        b.iString = z.as_ptr() as isize;
        btns.push(b);
    }
    send(tb, TB_ADDBUTTONSW, btns.len(), btns.as_ptr() as isize);
    send(tb, TB_AUTOSIZE, 0, 0);
    tb
}

fn lv_cols(lv: HWND, cols: &[(&str, i32, bool)]) {
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

fn wnd_h(h: HWND) -> i32 {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(h, &mut r);
    }
    r.bottom - r.top
}

fn layout(main: HWND) {
    let u = ui(|u| (u.tb, u.tab, u.queue, u.log, u.sb));
    let (tb, tab, queue, log, sb) = u;
    if tb.is_invalid() {
        return;
    }
    unsafe {
        let mut rc = RECT::default();
        let _ = GetClientRect(main, &mut rc);
        let wdt = rc.right;
        send(sb, WM_SIZE, 0, 0);
        let sbh = wnd_h(sb);
        let tbh = (send(tb, TB_GETBUTTONSIZE, 0, 0) >> 16) as i32 + s(4);
        let _ = MoveWindow(tb, 0, 0, wdt, tbh, true);
        let tab_h = s(26);
        let _ = MoveWindow(tab, 0, tbh, wdt, tab_h, true);
        let top = tbh + tab_h;
        let h = (rc.bottom - sbh - top).max(0);
        let _ = MoveWindow(queue, 0, top, wdt, h, true);
        let _ = MoveWindow(log, 0, top, wdt, h, true);
    }
}

// -------------------------------------------------------------- refresh ---

/// Pulls the newest snapshot; updates only what changed.
fn refresh() {
    let Some(handle) = ui(|u| u.handle.clone()) else { return };
    let snap = handle.snapshot();
    let (queue, log, sb, tb, old_queue, old_log_len, old_log_last) = ui(|u| {
        (u.queue, u.log, u.sb, u.tb, u.snap.queue.clone(), u.snap.log_tail.len(), u.snap.log_tail.last().cloned())
    });
    let queue_changed = old_queue != snap.queue;
    let log_changed = old_log_len != snap.log_tail.len() || old_log_last != snap.log_tail.last().cloned();
    let (n, nlog) = (snap.queue.len(), snap.log_tail.len());
    let status = [
        if snap.connected { "Connected" } else { "Not connected" }.to_string(),
        format!("{}/s{}", format_size_short(snap.speed_bps as u64), if snap.paused { " (paused)" } else { "" }),
        format!("{} left in {} item(s)", format_size_short(snap.queue_remaining_bytes.max(0.0) as u64), n),
    ];
    let (connected, paused) = (snap.connected, snap.paused);
    ui(|u| u.snap = snap);
    if queue_changed {
        // Keep the selection by id across reorders.
        let sel_id = selected_id_from(&old_queue, queue);
        send(queue, LVM_SETITEMCOUNT, n, (LVSICF_NOINVALIDATEALL | LVSICF_NOSCROLL) as isize);
        if let Some(id) = sel_id {
            if let Some(row) = ui(|u| u.snap.queue.iter().position(|r| r.id == id)) {
                select_row(queue, row);
            }
        }
        unsafe {
            let _ = InvalidateRect(Some(queue), None, false);
        }
    }
    if log_changed {
        send(log, LVM_SETITEMCOUNT, nlog, LVSICF_NOSCROLL as isize);
        if nlog > 0 {
            send(log, LVM_ENSUREVISIBLE, nlog - 1, 0);
        }
        unsafe {
            let _ = InvalidateRect(Some(log), None, false);
        }
    }
    for (i, t) in status.iter().enumerate() {
        let z = wz(t);
        send(sb, SB_SETTEXTW, i, z.as_ptr() as isize);
    }
    set_button_text(tb, ID_CONNECT, if connected { "Disconnect" } else { "Connect" });
    set_button_text(tb, ID_PAUSE, if paused { "Resume" } else { "Pause" });
}

fn set_button_text(tb: HWND, id: i32, text: &str) {
    thread_local! { static TEXTS: RefCell<Vec<(i32, Vec<u16>)>> = RefCell::new(vec![]); }
    TEXTS.with(|t| {
        let mut t = t.borrow_mut();
        let z = wz(text);
        if t.iter().any(|(i, v)| *i == id && *v == z) {
            return;
        }
        t.retain(|(i, _)| *i != id);
        t.push((id, z));
        let z = &t.last().unwrap().1;
        let mut bi = TBBUTTONINFOW::default();
        bi.cbSize = std::mem::size_of::<TBBUTTONINFOW>() as u32;
        bi.dwMask = TBIF_TEXT;
        bi.pszText = PWSTR(z.as_ptr() as *mut u16);
        send(tb, TB_SETBUTTONINFOW, id as usize, &bi as *const _ as isize);
    });
}

fn selected_row(lv: HWND) -> Option<usize> {
    let r = send(lv, LVM_GETNEXTITEM, usize::MAX, LVNI_SELECTED as isize);
    (r >= 0).then_some(r as usize)
}
fn selected_id_from(rows: &[newsqueue_core::snapshot::QueueRow], lv: HWND) -> Option<u64> {
    selected_row(lv).and_then(|i| rows.get(i)).map(|r| r.id)
}
fn selected_id() -> Option<u64> {
    let (lv, rows) = ui(|u| (u.queue, u.snap.queue.clone()));
    selected_id_from(&rows, lv)
}
fn select_row(lv: HWND, row: usize) {
    let mut clear = LVITEMW::default();
    clear.stateMask = LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED.0 | LVIS_FOCUSED.0);
    send(lv, LVM_SETITEMSTATE, usize::MAX, &clear as *const _ as isize);
    let mut it = LVITEMW::default();
    it.stateMask = LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED.0 | LVIS_FOCUSED.0);
    it.state = LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED.0 | LVIS_FOCUSED.0);
    send(lv, LVM_SETITEMSTATE, row, &it as *const _ as isize);
}

// ------------------------------------------------------------- messages ---

unsafe extern "system" fn main_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_SIZE => {
            layout(h);
            LRESULT(0)
        }
        WM_TIMER if wp.0 == TIMER_ID => {
            refresh();
            handle_events(h);
            LRESULT(0)
        }
        WM_COMMAND => {
            command((wp.0 & 0xffff) as i32, h);
            LRESULT(0)
        }
        WM_NOTIFY => notify(h, lp),
        WM_CONTEXTMENU => {
            let queue = ui(|u| u.queue);
            if HWND(wp.0 as *mut c_void) == queue {
                context_menu(h, lp);
            }
            LRESULT(0)
        }
        WM_DROPFILES => {
            let drop = HDROP(wp.0 as *mut c_void);
            let n = DragQueryFileW(drop, u32::MAX, None);
            for i in 0..n {
                let mut buf = [0u16; 1024];
                let len = DragQueryFileW(drop, i, Some(&mut buf)) as usize;
                add_nzb(String::from_utf16_lossy(&buf[..len]));
            }
            DragFinish(drop);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

fn add_nzb(path: String) {
    if path.to_ascii_lowercase().ends_with(".nzb") {
        send_cmd(Command::AddNzbFile(path, false));
    }
}

fn command(id: i32, h: HWND) {
    let snap_flags = ui(|u| (u.snap.connected, u.snap.paused));
    match id {
        ID_CONNECT => send_cmd(if snap_flags.0 { Command::Disconnect } else { Command::Connect }),
        ID_PAUSE => send_cmd(Command::TogglePause),
        ID_ADD => open_nzbs(h).into_iter().for_each(add_nzb),
        ID_CLEANUP => send_cmd(Command::Cleanup),
        _ => {
            let Some(id_sel) = selected_id() else { return };
            let c = match id {
                ID_TOP => Command::Move(id_sel, MoveDir::Top),
                ID_UP => Command::Move(id_sel, MoveDir::Up),
                ID_DOWN => Command::Move(id_sel, MoveDir::Down),
                ID_BOTTOM => Command::Move(id_sel, MoveDir::Bottom),
                ID_UNPACK => Command::Unpack(id_sel),
                ID_ITEM_PAUSE => Command::Pause(id_sel, true),
                ID_ITEM_RESUME => Command::Pause(id_sel, false),
                ID_DELETE => {
                    if !confirm(h, "Remove the selected item from the queue?") {
                        return;
                    }
                    Command::Delete(id_sel)
                }
                ID_PASSWORD => match prompt(h, "Archive password", "Password for the archive:") {
                    Some(p) if !p.is_empty() => Command::SubmitPassword(id_sel, p),
                    _ => return,
                },
                _ => return,
            };
            send_cmd(c);
        }
    }
    // Show the result without waiting for the next timer tick.
    std::thread::sleep(TICK_INTERVAL * 2);
    refresh();
}

fn context_menu(h: HWND, lp: LPARAM) {
    let Some(_) = selected_id() else { return };
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let items: [(i32, &str); 10] = [
            (ID_ITEM_RESUME, "Resume"),
            (ID_ITEM_PAUSE, "Pause"),
            (0, ""),
            (ID_TOP, "Move to top"),
            (ID_UP, "Move up"),
            (ID_DOWN, "Move down"),
            (ID_BOTTOM, "Move to bottom"),
            (0, ""),
            (ID_UNPACK, "Unpack now"),
            (ID_PASSWORD, "Set archive password…"),
        ];
        for (id, t) in items.iter().chain([(0, ""), (ID_DELETE, "Delete")].iter()) {
            let z = wz(t);
            if *id == 0 {
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            } else {
                let _ = AppendMenuW(menu, MF_STRING, *id as usize, PCWSTR(z.as_ptr()));
            }
        }
        let (mut x, mut y) = (lo(lp.0) as i16 as i32, hi(lp.0) as i16 as i32);
        if x == -1 && y == -1 {
            let mut p = POINT::default();
            let _ = GetCursorPos(&mut p);
            (x, y) = (p.x, p.y);
        }
        let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, x, y, None, h, None);
        let _ = DestroyMenu(menu);
    }
}

unsafe fn notify(h: HWND, lp: LPARAM) -> LRESULT {
    let nm = &*(lp.0 as *const NMHDR);
    let (queue, log, tab) = ui(|u| (u.queue, u.log, u.tab));
    if nm.code == LVN_GETDISPINFOW {
        let di = &mut *(lp.0 as *mut NMLVDISPINFOW);
        let it = &mut di.item;
        let (row, sub) = (it.iItem as usize, it.iSubItem as usize);
        if it.mask.0 & LVIF_TEXT.0 != 0 {
            let text = ui(|u| {
                if nm.hwndFrom == queue {
                    u.snap.queue.get(row).map(|r| cell(r, sub))
                } else if nm.hwndFrom == log {
                    u.snap.log_tail.get(row).map(|l| match sub {
                        0 => l.time.clone(),
                        1 => l.level.clone(),
                        _ => l.message.clone(),
                    })
                } else {
                    None
                }
            });
            put(it.pszText, it.cchTextMax, text.as_deref().unwrap_or(""));
        }
        return LRESULT(0);
    }
    if nm.code == TCN_SELCHANGE && nm.hwndFrom == tab {
        let sel = send(tab, TCM_GETCURSEL, 0, 0);
        let _ = ShowWindow(queue, if sel == 0 { SW_SHOW } else { SW_HIDE });
        let _ = ShowWindow(log, if sel == 1 { SW_SHOW } else { SW_HIDE });
        return LRESULT(0);
    }
    if nm.code == NM_CUSTOMDRAW && nm.hwndFrom == queue {
        return queue_draw(lp, queue);
    }
    if nm.code == LVN_KEYDOWN && nm.hwndFrom == queue {
        let kd = &*(lp.0 as *const NMLVKEYDOWN);
        if kd.wVKey == 0x2E {
            command(ID_DELETE, h);
        }
    }
    LRESULT(0)
}

unsafe fn put(dst: PWSTR, max: i32, t: &str) {
    if dst.is_null() || max <= 0 {
        return;
    }
    let z = wz(t);
    let n = z.len().min(max as usize);
    std::ptr::copy_nonoverlapping(z.as_ptr(), dst.0, n);
    *dst.0.add(n - 1) = 0;
}

/// Progress bar in the Status column; finished rows are drawn muted.
unsafe fn queue_draw(lp: LPARAM, queue: HWND) -> LRESULT {
    let cd = &mut *(lp.0 as *mut NMLVCUSTOMDRAW);
    let stage = cd.nmcd.dwDrawStage.0;
    if stage == CDDS_PREPAINT.0 {
        return LRESULT(CDRF_NOTIFYITEMDRAW as isize);
    }
    if stage == CDDS_ITEMPREPAINT.0 {
        return LRESULT(CDRF_NOTIFYSUBITEMDRAW as isize);
    }
    if stage != (CDDS_ITEMPREPAINT.0 | CDDS_SUBITEM.0) {
        return LRESULT(0);
    }
    let row = cd.nmcd.dwItemSpec;
    let Some(r) = ui(|u| u.snap.queue.get(row).cloned()) else { return LRESULT(0) };
    let status = r.status.unwrap_or(ItemStatus::Queued);
    let done = matches!(status, ItemStatus::Completed);
    if cd.iSubItem != COL_STATUS {
        if done {
            cd.clrText = rgb(110, 110, 110);
        }
        return LRESULT(CDRF_NEWFONT as isize);
    }
    let dc = cd.nmcd.hdc;
    let mut rc = RECT { left: LVIR_BOUNDS as i32, top: COL_STATUS, right: 0, bottom: 0 };
    send(queue, LVM_GETSUBITEMRECT, row, &mut rc as *mut _ as isize);
    let sel = send(queue, LVM_GETITEMSTATE, row, LVIS_SELECTED.0 as isize) != 0;
    FillRect(dc, &rc, GetSysColorBrush(COLOR_WINDOW));
    if sel {
        let th = OpenThemeData(Some(queue), w!("Explorer::ListView"));
        if th.0 != 0 {
            let _ = DrawThemeBackground(th, dc, 1, if GetFocus() == queue { 3 } else { 5 }, &rc, None);
            let _ = CloseThemeData(th);
        }
    }
    let bar = RECT { left: rc.left + s(3), top: rc.top + s(3), right: rc.right - s(3), bottom: rc.bottom - s(3) };
    let pct = (r.progress.clamp(0.0, 1.0) * 100.0) as i32;
    let fill_right = bar.left + 1 + ((bar.right - bar.left - 2) * pct) / 100;
    let th = OpenThemeData(Some(queue), w!("PROGRESS"));
    if th.0 != 0 {
        let _ = DrawThemeBackground(th, dc, 1, 0, &bar, None);
        let inner = RECT { left: bar.left + 1, top: bar.top + 1, right: fill_right, bottom: bar.bottom - 1 };
        if done {
            let g = CreateSolidBrush(rgb(176, 180, 186));
            FillRect(dc, &inner, g);
            let _ = DeleteObject(g.into());
        } else {
            let state = match status {
                ItemStatus::Paused => 3,
                ItemStatus::Error | ItemStatus::Incomplete => 2,
                _ => 1,
            };
            let _ = DrawThemeBackground(th, dc, 5, state, &inner, None);
        }
        let _ = CloseThemeData(th);
    }
    // Text centred; a white halo keeps it readable over the fill.
    SetBkMode(dc, TRANSPARENT);
    let old = SelectObject(dc, font().into());
    let mut t = wz(&row_status(&r));
    t.pop();
    let mut tr = bar;
    SetTextColor(dc, rgb(255, 255, 255));
    for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
        let mut h = RECT { left: tr.left + dx, top: tr.top + dy, right: tr.right + dx, bottom: tr.bottom + dy };
        DrawTextW(dc, &mut t, &mut h, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
    }
    SetTextColor(dc, rgb(0, 0, 0));
    DrawTextW(dc, &mut t, &mut tr, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
    SelectObject(dc, old);
    LRESULT(CDRF_SKIPDEFAULT as isize)
}

// --------------------------------------------------------------- events ---

fn handle_events(h: HWND) {
    let events = HOST.with(|host| host.borrow_mut().as_mut().map(|x| x.with(|e| e.take_events())).unwrap_or_default());
    for ev in events {
        match ev {
            Event::Toast(t) | Event::Notify(_, t) => {
                let sb = ui(|u| u.sb);
                let z = wz(&t);
                send(sb, SB_SETTEXTW, 3, z.as_ptr() as isize);
            }
            Event::Message(title, text) => message(h, &title, &text),
            Event::PasswordNeeded(id) => {
                let name = ui(|u| u.snap.queue.iter().find(|r| r.id == id).map(|r| r.subject.clone())).unwrap_or_default();
                match prompt(h, "Archive password", &format!("{name} is encrypted. Password:")) {
                    Some(p) if !p.is_empty() => send_cmd(Command::SubmitPassword(id, p)),
                    _ => send_cmd(Command::CancelPassword(id)),
                }
            }
            Event::RssFeedsNeeded => {}
        }
    }
}

fn message(h: HWND, title: &str, text: &str) {
    let (t, x) = (wz(title), wz(text));
    unsafe {
        MessageBoxW(Some(h), PCWSTR(x.as_ptr()), PCWSTR(t.as_ptr()), MB_OK | MB_ICONINFORMATION);
    }
}

fn confirm(h: HWND, text: &str) -> bool {
    let x = wz(text);
    unsafe { MessageBoxW(Some(h), PCWSTR(x.as_ptr()), w!("NewsQueue"), MB_YESNO | MB_ICONQUESTION) == IDYES }
}

fn open_nzbs(h: HWND) -> Vec<String> {
    let mut buf = vec![0u16; 32 * 1024];
    let filter: Vec<u16> = "NZB files (*.nzb)\0*.nzb\0All files\0*.*\0\0".encode_utf16().collect();
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: h,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(buf.as_mut_ptr()),
        nMaxFile: buf.len() as u32,
        Flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_ALLOWMULTISELECT | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    if !unsafe { GetOpenFileNameW(&mut ofn) }.as_bool() {
        return vec![];
    }
    let parts: Vec<String> = buf.split(|c| *c == 0).take_while(|p| !p.is_empty()).map(String::from_utf16_lossy).collect();
    match parts.as_slice() {
        [one] => vec![one.clone()],
        [dir, files @ ..] => files.iter().map(|f| format!("{dir}\\{f}")).collect(),
        [] => vec![],
    }
}

// ---------------------------------------------------------- input prompt --

thread_local! { static PROMPT: RefCell<(HWND, Option<String>, bool)> = RefCell::new((HWND::default(), None, false)); }

/// Small modal text prompt (masked input) built from plain controls.
fn prompt(parent: HWND, title: &str, label: &str) -> Option<String> {
    unsafe {
        let cls = w!("NewsQueuePrompt");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(prompt_proc),
            hInstance: hinst(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: GetSysColorBrush(COLOR_BTNFACE),
            lpszClassName: cls,
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let style = WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0;
        let (w, h) = outer(style, WS_EX_DLGMODALFRAME.0, 360, 110);
        let mut pr = RECT::default();
        let _ = GetWindowRect(parent, &mut pr);
        let t = wz(title);
        let dlg = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            cls,
            PCWSTR(t.as_ptr()),
            WINDOW_STYLE(style),
            pr.left + ((pr.right - pr.left) - w) / 2,
            pr.top + ((pr.bottom - pr.top) - h) / 2,
            w,
            h,
            Some(parent),
            None,
            Some(hinst()),
            None,
        )
        .ok()?;
        mk("STATIC", label, CHILD | 0x80, 0, 12, 12, 336, 18, dlg, 0);
        let edit = mk("EDIT", "", sty!(CHILD, WS_TABSTOP, ES_AUTOHSCROLL, ES_PASSWORD), WS_EX_CLIENTEDGE.0, 12, 34, 336, 22, dlg, 10);
        mk("BUTTON", "OK", sty!(CHILD, WS_TABSTOP, BS_DEFPUSHBUTTON), 0, 192, 72, 75, 25, dlg, 1);
        mk("BUTTON", "Cancel", sty!(CHILD, WS_TABSTOP, BS_PUSHBUTTON), 0, 273, 72, 75, 25, dlg, 2);
        PROMPT.with(|p| *p.borrow_mut() = (edit, None, false));
        let _ = EnableWindow(parent, false);
        let _ = ShowWindow(dlg, SW_SHOW);
        let _ = SetFocus(Some(edit));
        let mut msg = MSG::default();
        while !PROMPT.with(|p| p.borrow().2) && GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if !IsDialogMessageW(dlg, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let _ = EnableWindow(parent, true);
        let _ = DestroyWindow(dlg);
        let _ = SetForegroundWindow(parent);
        PROMPT.with(|p| p.borrow_mut().1.take())
    }
}

unsafe extern "system" fn prompt_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_COMMAND => {
            let id = (wp.0 & 0xffff) as i32;
            if id == 1 || id == 2 {
                PROMPT.with(|p| {
                    let mut p = p.borrow_mut();
                    if id == 1 {
                        let mut buf = [0u16; 512];
                        let n = GetWindowTextW(p.0, &mut buf) as usize;
                        p.1 = Some(String::from_utf16_lossy(&buf[..n]));
                    }
                    p.2 = true;
                });
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            PROMPT.with(|p| p.borrow_mut().2 = true);
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}
