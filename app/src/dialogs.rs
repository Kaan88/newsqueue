//! The smaller dialogs: version/update, about, notes, statistics, columns,
//! pause for, speed limit, shutdown, rename, execute command, archive
//! password, RSS feeds and filter help. Setup lives in `setup.rs`.

use crate::form::{self, Ev, IDCANCEL_, IDOK_};
use crate::util::*;
use crate::{send_cmd, setting, setting_bool, ui, update, with_engine};
use newsqueue_core::command::Command;
use newsqueue_core::model::*;
use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// ------------------------------------------------------------- updater ----

pub struct UpdateState {
    rx: Option<std::sync::mpsc::Receiver<update::UpdateEvent>>,
    pub status: String,
    pub available: Option<update::Release>,
    notified: String,
    installing: bool,
    next_check: Option<Instant>,
}

impl Default for UpdateState {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdateState {
    pub fn new() -> Self {
        Self { rx: None, status: String::new(), available: None, notified: String::new(), installing: false, next_check: Some(Instant::now() + Duration::from_secs(15)) }
    }
}

fn check_now() {
    if ui(|u| u.demo || u.update.rx.is_some()) {
        return;
    }
    ui(|u| {
        u.update.status = "Checking for a new version...".into();
        u.update.rx = Some(update::spawn_check());
    });
}

fn update_now() {
    let Some(release) = ui(|u| u.update.available.clone()) else { return };
    if ui(|u| u.update.rx.is_some() || u.update.installing) {
        return;
    }
    if update::install_dir().is_none() {
        ui(|u| u.update.status = format!("{APP_NAME} {} is available. This is a development build; get it from the release page.", release.version));
        return;
    }
    with_engine(|e| e.log("Update", &format!("Downloading version {}.", release.version)));
    ui(|u| {
        u.update.installing = true;
        u.update.status = format!("Downloading {APP_NAME} {}...", release.version);
        u.update.rx = Some(update::spawn_download(release));
    });
}

/// Periodic check, auto-install when idle, and worker results.
pub fn poll_update() {
    let minutes = setting("check_version_minutes").as_u64().unwrap_or(0);
    let due = ui(|u| u.update.next_check.is_some_and(|at| Instant::now() >= at));
    if due {
        ui(|u| u.update.next_check = (minutes > 0).then(|| Instant::now() + Duration::from_secs(minutes * 60)));
        if minutes > 0 {
            check_now();
        }
    }
    let idle = ui(|u| u.snap.all_done);
    let auto = setting_bool("auto_update")
        && idle
        && ui(|u| !u.update.installing && u.update.rx.is_none() && u.update.available.is_some())
        && update::install_dir().is_some();
    if auto {
        update_now();
    }
    let Some(rx) = ui(|u| u.update.rx.take()) else { return };
    let event = match rx.try_recv() {
        Ok(e) => e,
        Err(std::sync::mpsc::TryRecvError::Empty) => {
            ui(|u| u.update.rx = Some(rx));
            return;
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => update::UpdateEvent::Failed("The update worker stopped unexpectedly".into()),
    };
    match event {
        update::UpdateEvent::UpToDate(latest) => ui(|u| {
            u.update.available = None;
            u.update.status = format!("{APP_NAME} {VERSION} is up to date (latest release: {latest}).");
        }),
        update::UpdateEvent::Available(release) => {
            let new = ui(|u| {
                u.update.status = format!("{APP_NAME} {} is available (you have {VERSION}).", release.version);
                let new = u.update.notified != release.version;
                u.update.notified = release.version.clone();
                u.update.available = Some(release.clone());
                new
            });
            if new {
                with_engine(|e| e.log("Update", &format!("Version {} is available.", release.version)));
                crate::toast(format!("NewsQueue {} is available", release.version));
                crate::tray::notify("Update available", &format!("NewsQueue {} is available", release.version));
            }
        }
        update::UpdateEvent::Downloaded(release, setup) => {
            let Some(dir) = update::install_dir() else {
                ui(|u| u.update.installing = false);
                return;
            };
            match update::launch_installer(&setup, &dir) {
                Ok(()) => {
                    with_engine(|e| e.log("Update", &format!("Installing version {}; restarting.", release.version)));
                    ui(|u| u.update.status = format!("Installing {APP_NAME} {}...", release.version));
                    crate::request_exit();
                }
                Err(err) => {
                    with_engine(|e| e.log("Error", &err));
                    ui(|u| {
                        u.update.installing = false;
                        u.update.status = err;
                    });
                }
            }
        }
        update::UpdateEvent::Failed(err) => {
            with_engine(|e| e.log("Error", &err));
            ui(|u| {
                u.update.installing = false;
                u.update.status = err;
            });
        }
    }
}

const ID_STATUS_TEXT: i32 = 10;
const ID_UPDATE_NOW: i32 = 11;
const ID_CHECK: i32 = 12;
const ID_RELEASES: i32 = 13;

fn build_version(owner: HWND) -> HWND {
    let d = form::create(Some(owner), "Version", 440, 190, false);
    form::heading(d, &format!("{APP_NAME} {VERSION}"), 16, 14, 400);
    form::note(d, &format!("Build date {BUILD_DATE}"), 16, 36, 400, 18);
    form::label_id(d, ID_STATUS_TEXT, "", 16, 66, 408, 40);
    form::hline(d, 16, 132, 408);
    form::button(d, ID_RELEASES, "Release page", 16, 148, 100);
    form::button(d, ID_CHECK, "Check again", 166, 148, 90);
    form::button(d, ID_UPDATE_NOW, "Update now", 262, 148, 90);
    form::button(d, IDOK_, "Close", 358, 148, 66);
    sync_version(d);
    d
}

fn sync_version(d: HWND) {
    let (status, avail) = ui(|u| (u.update.status.clone(), u.update.available.is_some()));
    let t = form::item(d, ID_STATUS_TEXT);
    if form::text(t) != status {
        form::set_text(t, &status);
    }
    form::enable(form::item(d, ID_UPDATE_NOW), avail);
}

pub fn version(owner: HWND) {
    check_now();
    let d = build_version(owner);
    unsafe {
        SetTimer(Some(d), 1, 300, None);
    }
    form::run_modal(
        d,
        Some(owner),
        Box::new(|d, ev| {
            match ev {
                Ev::Timer(_) => sync_version(d),
                Ev::Cmd(ID_CHECK, _) => check_now(),
                Ev::Cmd(ID_UPDATE_NOW, _) => update_now(),
                Ev::Cmd(ID_RELEASES, _) => {
                    let _ = std::process::Command::new("explorer").arg(format!("{HOME_URL}/releases")).spawn();
                }
                Ev::Cmd(IDCANCEL_ | IDOK_, _) | Ev::Close => form::end(d, IDCANCEL_),
                _ => {}
            }
            0
        }),
    );
}

// --------------------------------------------------------------- about ----

fn build_about(owner: HWND) -> HWND {
    let d = form::create(Some(owner), "About", 420, 236, false);
    let icon = mk("STATIC", "", form::CHILD | form::SS_ICON, 0, 16, 16, 48, 48, d, 0);
    unsafe {
        if let Ok(h) = LoadImageW(Some(hinst()), windows::core::PCWSTR(1 as *const u16), IMAGE_ICON, s(48), s(48), LR_DEFAULTCOLOR) {
            send(icon, STM_SETICON, h.0 as usize, 0);
        }
    }
    form::heading(d, APP_NAME, 80, 18, 320);
    form::label(d, &format!("Version {VERSION}  ({BUILD_DATE})"), 80, 38, 320);
    form::label(d, "A desktop Usenet download queue.", 80, 74, 320);
    form::note(d, "Independent project, not affiliated with the application whose layout it follows. UnRAR and par2cmdline are used under their own licenses (see the tools folder).", 80, 100, 324, 64);
    form::hline(d, 16, 182, 388);
    form::button(d, 20, "Home page", 16, 198, 100);
    form::button(d, 21, "Check for updates", 124, 198, 160);
    form::button(d, IDOK_, "OK", 324, 198, 80);
    d
}

pub fn about(owner: HWND) {
    let d = build_about(owner);
    form::run_modal(
        d,
        Some(owner),
        Box::new(|d, ev| {
            match ev {
                Ev::Cmd(21, _) => version(d),
                Ev::Cmd(20, _) => {
                    let _ = std::process::Command::new("explorer").arg(HOME_URL).spawn();
                }
                Ev::Cmd(IDOK_ | IDCANCEL_, _) | Ev::Close => form::end(d, IDOK_),
                _ => {}
            }
            0
        }),
    );
}

// --------------------------------------------------------------- notes ----

fn build_notes(owner: HWND) -> HWND {
    let d = form::create(Some(owner), "Notes", 480, 420, false);
    let text = with_engine(|e| e.settings.notes.clone());
    let m = form::memo(d, 10, &text, 12, 12, 456, 342);
    send(m, WM_SETFONT, mono().0 as usize, 1);
    form::hline(d, 12, 366, 456);
    form::button(d, IDOK_, "Save", 302, 378, 80);
    form::button(d, IDCANCEL_, "Cancel", 388, 378, 80);
    d
}

pub fn notes(owner: HWND) {
    let d = build_notes(owner);
    form::run_modal(
        d,
        Some(owner),
        Box::new(|d, ev| {
            match ev {
                Ev::Cmd(IDOK_, _) => {
                    let text = form::memo_text(form::item(d, 10));
                    send_cmd(Command::MergeSettings(serde_json::json!({ "notes": text })));
                    crate::toast("Notes saved");
                    form::end(d, IDOK_);
                }
                Ev::Cmd(IDCANCEL_, _) | Ev::Close => form::end(d, IDCANCEL_),
                _ => {}
            }
            0
        }),
    );
}

fn mono() -> HFONT {
    thread_local! { static F: HFONT = unsafe {
        let mut lf = LOGFONTW::default();
        lf.lfHeight = -s(13);
        lf.lfWeight = 400;
        let name: Vec<u16> = "Consolas".encode_utf16().collect();
        lf.lfFaceName[..name.len()].copy_from_slice(&name);
        CreateFontIndirectW(&lf)
    }; }
    F.with(|f| *f)
}

// --------------------------------------------------------------- stats ----

const ID_STATS_TAB: i32 = 30;
const ID_STATS_CHART: i32 = 31;
const ID_STATS_HIST: i32 = 32;
const ID_STATS_RETRY: i32 = 33;
const ID_STATS_MISC: i32 = 34;
const ID_STATS_SEARCH: i32 = 35;

thread_local! { static STATS_TAB: RefCell<usize> = const { RefCell::new(0) }; }

fn build_stats(owner: HWND, tab_index: usize) -> HWND {
    let d = form::create(Some(owner), "Statistics & history", 760, 500, false);
    let tab = form::reg(d, ID_STATS_TAB, mk("SysTabControl32", "", form::CHILD | WS_CLIPSIBLINGS.0, 0, 10, 10, 740, 440, d, ID_STATS_TAB));
    for (i, t) in ["Daily transfer", "Monthly transfer", "Speed graph", "Download history", "Misc. stats"].iter().enumerate() {
        let z = wz(t);
        let mut it = TCITEMW::default();
        it.mask = TCIF_TEXT;
        it.pszText = windows::core::PWSTR(z.as_ptr() as *mut u16);
        send(tab, TCM_INSERTITEMW, i, &it as *const _ as isize);
    }
    register_chart();
    let pane=form::tab_panel(tab,12,34,716,394);
    let chart = form::reg(pane, ID_STATS_CHART, mk("NQChart", "", form::CHILD, WS_EX_CLIENTEDGE.0, 0, 0, 716, 394, pane, ID_STATS_CHART));
    let _ = chart;
    form::label(pane, "Search", 0, 0, 50);
    form::edit(pane, ID_STATS_SEARCH, "", 52, 0, 240, 0);
    form::button(pane, ID_STATS_RETRY, "Retry", 636, 0, 80);
    let lv = form::listview(pane, ID_STATS_HIST, &[("Name", 330, false), ("Date", 130, false), ("Size", 90, true), ("Status", 140, false)], 0, 32, 716, 362, 0);
    let _ = lv;
    form::label_id(pane, ID_STATS_MISC, "", 8, 8, 700, 300);
    form::button(d, IDOK_, "Close", 670, 462, 80);
    // Siblings above the tab control.
    for id in [ID_STATS_CHART, ID_STATS_HIST, ID_STATS_MISC, ID_STATS_SEARCH, ID_STATS_RETRY] {
        unsafe {
            let _ = SetWindowPos(form::item(d, id), Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
    unsafe {
        let _ = SetWindowLongW(tab, GWL_STYLE, GetWindowLongW(tab, GWL_STYLE) | WS_CLIPSIBLINGS.0 as i32);
    }
    stats_show(d, tab_index);
    d
}

fn stats_show(d: HWND, i: usize) {
    STATS_TAB.with(|t| *t.borrow_mut() = i);
    send(form::item(d, ID_STATS_TAB), TCM_SETCURSEL, i, 0);
    form::show(form::item(d, ID_STATS_CHART), i <= 2);
    for id in [ID_STATS_HIST, ID_STATS_RETRY, ID_STATS_SEARCH] {
        form::show(form::item(d, id), i == 3);
    }
    unsafe {
        // The "Search" label has no id; find it by position instead.
        let _ = EnumChildWindows(Some(d), Some(search_label), LPARAM((i == 3) as isize));
    }
    form::show(form::item(d, ID_STATS_MISC), i == 4);
    stats_fill(d);
    unsafe {
        let _ = InvalidateRect(Some(form::item(d, ID_STATS_CHART)), None, true);
    }
}

unsafe extern "system" fn search_label(c: HWND, lp: LPARAM) -> windows::core::BOOL {
    if crate::form::text(c) == "Search" && GetDlgCtrlID(c) == 0 {
        crate::form::show(c, lp.0 != 0);
    }
    TRUE
}

fn stats_fill(d: HWND) {
    let filter = form::text(form::item(d, ID_STATS_SEARCH)).to_lowercase();
    let (history, stats) = ui(|u| (u.snap.history.clone(), u.snap.stats.clone()));
    let rows: Vec<Vec<String>> = history
        .iter()
        .filter(|h| filter.is_empty() || h.name.to_lowercase().contains(&filter))
        .map(|h| vec![h.name.clone(), h.date.clone(), h.size.clone(), h.status.clone()])
        .collect();
    let lv = form::item(d, ID_STATS_HIST);
    if send(lv, LVM_GETITEMCOUNT, 0, 0) as usize != rows.len() {
        form::lv_fill(lv, &rows);
    }
    let hm = |secs: u64| format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60);
    let misc = format!(
        "Current session uptime:\t{}\r\nCurrent session transfer:\t{}\r\nCollections this session:\t{}\r\nTotal collections downloaded:\t{}\r\nTotal startups:\t{}\r\nLongest uptime:\t{}",
        hm(stats.session_secs),
        format_bytes_auto(stats.session_bytes as f64),
        stats.session_collections,
        stats.total_collections,
        stats.startups,
        hm(stats.longest_uptime_secs.max(stats.session_secs)),
    );
    let m = form::item(d, ID_STATS_MISC);
    // Tab stops are not available in a STATIC; lay the pairs out as lines.
    let misc = misc.replace('\t', "  ");
    if form::text(m) != misc {
        form::set_text(m, &misc);
    }
}

pub fn stats(owner: HWND) {
    let d = build_stats(owner, 0);
    unsafe {
        SetTimer(Some(d), 1, 1000, None);
    }
    form::run_modal(
        d,
        Some(owner),
        Box::new(|d, ev| {
            match ev {
                Ev::Notify(ID_STATS_TAB, lp) => {
                    let nm = unsafe { &*(lp.0 as *const windows::Win32::UI::Controls::NMHDR) };
                    if nm.code == TCN_SELCHANGE {
                        stats_show(d, send(form::item(d, ID_STATS_TAB), TCM_GETCURSEL, 0, 0).max(0) as usize);
                    }
                }
                Ev::Timer(_) => {
                    stats_fill(d);
                    if STATS_TAB.with(|t| *t.borrow()) == 2 {
                        unsafe {
                            let _ = InvalidateRect(Some(form::item(d, ID_STATS_CHART)), None, false);
                        }
                    }
                }
                Ev::Cmd(ID_STATS_SEARCH, code) if code == EN_CHANGE => {
                    form::lv_fill(form::item(d, ID_STATS_HIST), &[]);
                    stats_fill(d);
                }
                Ev::Cmd(ID_STATS_RETRY, _) => {
                    // Rows are filtered; map back to the history index by name.
                    let lv = form::item(d, ID_STATS_HIST);
                    if let Some(row) = form::lv_selected(lv) {
                        let filter = form::text(form::item(d, ID_STATS_SEARCH)).to_lowercase();
                        let idx = ui(|u| {
                            u.snap.history.iter().enumerate().filter(|(_, h)| filter.is_empty() || h.name.to_lowercase().contains(&filter)).nth(row).map(|(i, h)| (i, h.source.is_empty()))
                        });
                        match idx {
                            Some((i, false)) => send_cmd(Command::RetryHistory(i)),
                            Some((_, true)) => form::message(Some(d), "Retry", "This entry has no saved NZB to queue again.", MB_ICONINFORMATION),
                            None => {}
                        }
                    }
                }
                Ev::Cmd(IDOK_ | IDCANCEL_, _) | Ev::Close => form::end(d, IDOK_),
                _ => {}
            }
            0
        }),
    );
}

fn register_chart() {
    thread_local! { static DONE: RefCell<bool> = const { RefCell::new(false) }; }
    if DONE.with(|d| d.replace(true)) {
        return;
    }
    unsafe {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(chart_proc),
            hInstance: hinst(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: GetSysColorBrush(COLOR_WINDOW),
            lpszClassName: windows::core::w!("NQChart"),
            ..Default::default()
        };
        RegisterClassExW(&wc);
    }
}

unsafe extern "system" fn chart_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(h, &mut ps);
            paint_chart(h, dc);
            let _ = EndPaint(h, &ps);
            LRESULT(0)
        }
        WM_PRINTCLIENT => {
            paint_chart(h, HDC(wp.0 as *mut c_void));
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

unsafe fn paint_chart(h: HWND, dc: HDC) {
    let mut rc = RECT::default();
    let _ = GetClientRect(h, &mut rc);
    // Double-buffered.
    let mem = CreateCompatibleDC(Some(dc));
    let bmp = CreateCompatibleBitmap(dc, rc.right, rc.bottom);
    let old = SelectObject(mem, bmp.into());
    FillRect(mem, &rc, GetSysColorBrush(COLOR_WINDOW));
    SelectObject(mem, font().into());
    SetBkMode(mem, TRANSPARENT);
    let tab = STATS_TAB.with(|t| *t.borrow());
    let (stats, mode) = ui(|u| (u.snap.stats.clone(), u.snap.settings.get("speed_bytes_mode").and_then(|v| v.as_u64()).unwrap_or(3) as u8));
    let area = RECT { left: rc.left + s(48), top: rc.top + s(28), right: rc.right - s(14), bottom: rc.bottom - s(28) };
    let text = |t: &str, x: i32, y: i32, flags: DRAW_TEXT_FORMAT| {
        let mut z = wz(t);
        z.pop();
        let mut r = RECT { left: x - s(60), top: y, right: x + s(60), bottom: y + s(16) };
        if flags.0 & DT_CENTER.0 == 0 {
            r.left = x;
            r.right = x + s(400);
        }
        SetTextColor(mem, COLORREF(GetSysColor(COLOR_GRAYTEXT)));
        DrawTextW(mem, &mut z, &mut r, flags | DT_SINGLELINE | DT_NOPREFIX);
    };
    let grid = CreatePen(PS_SOLID, 1, rgb(0xEC, 0xEA, 0xE6));
    let op = SelectObject(mem, grid.into());
    for k in 0..=4 {
        let y = area.top + (area.bottom - area.top) * k / 4;
        let _ = MoveToEx(mem, area.left, y, None);
        let _ = LineTo(mem, area.right, y);
    }
    SelectObject(mem, op);
    let _ = DeleteObject(grid.into());
    match tab {
        0 | 1 => {
            let (values, labels): (Vec<f64>, Vec<String>) = if tab == 0 {
                (stats.daily.clone(), (0..24).map(|h| format!("{h:02}")).collect())
            } else {
                (
                    stats.monthly.clone(),
                    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"].iter().map(|s| s.to_string()).collect(),
                )
            };
            let n = values.len().max(1) as i32;
            let max = values.iter().cloned().fold(1.0f64, f64::max);
            let bw = (area.right - area.left) / n;
            // Classic bars: selection navy with a light left and a dark right edge.
            let fill = CreateSolidBrush(rgb(0x0A, 0x24, 0x6A));
            let lit = CreateSolidBrush(rgb(0x6A, 0x84, 0xC8));
            let dark = CreateSolidBrush(rgb(0x04, 0x10, 0x30));
            for (i, v) in values.iter().enumerate() {
                let x = area.left + i as i32 * bw;
                let hgt = ((v / max) * (area.bottom - area.top) as f64) as i32;
                let bar = RECT { left: x + s(3), top: area.bottom - hgt, right: x + bw - s(3), bottom: area.bottom };
                FillRect(mem, &bar, fill);
                if hgt > 1 && bar.right - bar.left > 3 {
                    FillRect(mem, &RECT { right: bar.left + 1, ..bar }, lit);
                    FillRect(mem, &RECT { top: bar.top, left: bar.left, right: bar.right, bottom: bar.top + 1 }, lit);
                    FillRect(mem, &RECT { left: bar.right - 1, ..bar }, dark);
                }
                if tab == 1 || i % 2 == 0 {
                    text(&labels[i], x + bw / 2, area.bottom + s(6), DT_CENTER);
                }
            }
            let _ = DeleteObject(fill.into());
            let _ = DeleteObject(lit.into());
            let _ = DeleteObject(dark.into());
            text(&format!("{} per {} (max {:.0} MB)", "MB", if tab == 0 { "hour, today" } else { "month, this year" }, max), area.left, rc.top + s(6), DT_LEFT);
        }
        _ => {
            let data = &stats.speed_history;
            let max = data.iter().cloned().fold(1.0f32, f32::max);
            if data.len() > 1 {
                let pen = CreatePen(PS_SOLID, s(2), rgb(0x00, 0x80, 0x00));
                let op = SelectObject(mem, pen.into());
                let n = data.len() as i32;
                for (i, v) in data.iter().enumerate() {
                    let x = area.left + (area.right - area.left) * i as i32 / (n - 1);
                    let y = area.bottom - ((v / max) * (area.bottom - area.top) as f32) as i32;
                    if i == 0 {
                        let _ = MoveToEx(mem, x, y, None);
                    } else {
                        let _ = LineTo(mem, x, y);
                    }
                }
                SelectObject(mem, op);
                let _ = DeleteObject(pen.into());
            }
            text(&format!("Speed over the last {} seconds (max {})", data.len(), format_speed(max as f64, mode)), area.left, rc.top + s(6), DT_LEFT);
            text("now", area.right - s(30), area.bottom + s(6), DT_CENTER);
        }
    }
    let axis = CreatePen(PS_SOLID, 1, rgb(0x80, 0x80, 0x80));
    let op = SelectObject(mem, axis.into());
    let _ = MoveToEx(mem, area.left, area.top, None);
    let _ = LineTo(mem, area.left, area.bottom);
    let _ = LineTo(mem, area.right, area.bottom);
    SelectObject(mem, op);
    let _ = DeleteObject(axis.into());
    let _ = BitBlt(dc, 0, 0, rc.right, rc.bottom, Some(mem), 0, 0, SRCCOPY);
    SelectObject(mem, old);
    let _ = DeleteObject(bmp.into());
    let _ = DeleteDC(mem);
}

// ------------------------------------------------------------- columns ----

fn build_columns(owner: HWND) -> HWND {
    let d = form::create(Some(owner), "Columns", 360, 430, false);
    form::note(d, "Tick the queue columns to show and set their width in pixels.", 14, 10, 332, 32);
    let (vis, widths) = with_engine(|e| (e.settings.column_visible, e.settings.column_widths));
    for (i, name) in QUEUE_COLUMN_NAMES.iter().enumerate() {
        let y = 46 + i as i32 * 28;
        form::check(d, 100 + i as i32, name, 20, y, 200, vis[i]);
        form::spin(d, 200 + i as i32, widths[i] as i64, 24, 800, 236, y - 2, 70);
        form::label(d, "px", 312, y - 2, 30);
    }
    form::hline(d, 14, 384, 332);
    form::button(d, 20, "Defaults", 14, 396, 80);
    form::button(d, IDOK_, "OK", 180, 396, 80);
    form::button(d, IDCANCEL_, "Cancel", 266, 396, 80);
    d
}

pub fn columns(owner: HWND) {
    let d = build_columns(owner);
    form::run_modal(
        d,
        Some(owner),
        Box::new(|d, ev| {
            match ev {
                Ev::Cmd(20, _) => {
                    for i in 0..12 {
                        form::set_check(form::item(d, 100 + i as i32), i < 10);
                        form::set_text(form::item(d, 200 + i as i32), &(QUEUE_COLUMN_WIDTHS[i] as i64).to_string());
                    }
                }
                Ev::Cmd(IDOK_, _) => {
                    let mut vis: Vec<bool> = (0..12).map(|i| form::checked(form::item(d, 100 + i))).collect();
                    if !vis.iter().any(|v| *v) {
                        vis[0] = true;
                    }
                    let widths: Vec<f32> = (0..12).map(|i| form::num(form::item(d, 200 + i)).clamp(24, 800) as f32).collect();
                    send_cmd(Command::MergeSettings(serde_json::json!({ "column_visible": vis, "column_widths": widths })));
                    form::end(d, IDOK_);
                }
                Ev::Cmd(IDCANCEL_, _) | Ev::Close => form::end(d, IDCANCEL_),
                _ => {}
            }
            0
        }),
    );
}

// ----------------------------------------------------------- pause for ----

const PAUSE_PRESETS: [u32; 8] = [5, 15, 30, 60, 120, 180, 240, 300];

fn build_pause_for(owner: HWND) -> HWND {
    let d = form::create(Some(owner), "Pause for", 330, 214, false);
    form::label(d, "Pause downloading for:", 14, 8, 300);
    for (i, m) in PAUSE_PRESETS.iter().enumerate() {
        let t = if *m < 60 { format!("{m} min") } else { format!("{} h", m / 60) };
        form::button(d, 100 + i as i32, &t, 14 + (i as i32 % 4) * 77, 34 + (i as i32 / 4) * 32, 71);
    }
    form::label(d, "Custom", 14, 108, 60);
    form::spin(d, 10, 10, 1, 1440, 76, 108, 70);
    form::label(d, "min", 152, 108, 40);
    form::set_default(form::button(d, 11, "Pause", 238, 107, 78));
    form::label_id(d, 12, "", 14, 144, 300, 18);
    form::hline(d, 14, 168, 302);
    form::button(d, 13, "Resume now", 14, 178, 100);
    form::button(d, IDCANCEL_, "Close", 236, 178, 80);
    sync_pause(d);
    d
}

fn sync_pause(d: HWND) {
    let until = with_engine(|e| e.pause_until);
    let t = match until {
        Some(at) => {
            let secs = at.saturating_duration_since(Instant::now()).as_secs();
            format!("Paused, resuming in {}m {:02}s", secs / 60, secs % 60)
        }
        None => String::new(),
    };
    let c = form::item(d, 12);
    if form::text(c) != t {
        form::set_text(c, &t);
    }
}

pub fn pause_for(owner: HWND) {
    let d = build_pause_for(owner);
    form::accent(form::item(d, 12));
    unsafe {
        SetTimer(Some(d), 1, 500, None);
    }
    form::run_modal(
        d,
        Some(owner),
        Box::new(|d, ev| {
            match ev {
                Ev::Cmd(id @ 100..=107, _) => {
                    send_cmd(Command::PauseFor(PAUSE_PRESETS[(id - 100) as usize]));
                    form::end(d, IDOK_);
                }
                Ev::Cmd(11 | IDOK_, _) => {
                    let m = form::num(form::item(d, 10)).clamp(1, 1440) as u32;
                    send_cmd(Command::PauseFor(m));
                    form::end(d, IDOK_);
                }
                Ev::Cmd(13, _) => {
                    send_cmd(Command::PauseFor(0));
                    form::end(d, IDOK_);
                }
                Ev::Timer(_) => sync_pause(d),
                Ev::Cmd(IDCANCEL_, _) | Ev::Close => form::end(d, IDCANCEL_),
                _ => {}
            }
            0
        }),
    );
}

// ---------------------------------------------------------- speed limit ---

fn build_speed_limit(owner: HWND) -> HWND {
    let d = form::create(Some(owner), "Speed limit", 370, 302, false);
    let (tag, custom) = with_engine(|e| (e.speed_limit_tag, e.custom_speed_limit_mbs));
    form::radio(d, 100, "No limit", 20, 16, 330, true);
    form::radio(d, 110, "Custom limit", 20, 48, 330, false);
    form::edit(d, 112, &format!("{:.2}", custom.unwrap_or(15.0)), 40, 78, 130, 0);
    form::label(d, "MB/s", 182, 81, 75);
    form::radio(d, 111, "Percentage of configured line speed", 20, 118, 330, false);
    form::combo(d, 113, &["90%", "80%", "70%", "60%", "50%", "40%", "30%", "20%", "10%"], (9-tag.unwrap_or(9).clamp(1,9)) as usize, 40, 148, 110);
    form::set_check(form::item(d, if custom.is_some() { 110 } else if tag.is_some() { 111 } else { 100 }), true);
    form::note(d, "Total across all servers and connections.\r\n1 MB/s = 1,000,000 bytes per second.", 20, 192, 330, 46);
    form::button(d, IDCANCEL_, "Cancel", 176, 264, 80);
    form::button(d, IDOK_, "Apply", 266, 264, 84);
    d
}

fn speed_limit_command(d: HWND) -> Result<Command, ()> {
    if form::checked(form::item(d,110)) {
        let mbs = form::text(form::item(d,112)).trim().replace(',', ".").parse::<f64>().map_err(|_| ())?;
        if !valid_custom_speed(mbs) { return Err(()); }
        Ok(Command::SetCustomSpeedLimit(mbs))
    } else {
        Ok(Command::SetSpeedLimit(if form::checked(form::item(d,111)) { Some(9-form::combo_sel(form::item(d,113)) as u8) } else { None }))
    }
}

pub fn speed_limit(owner: HWND) {
    let d = build_speed_limit(owner);
    form::run_modal(d, Some(owner), Box::new(|d, ev| {
        match ev {
            Ev::Cmd(112, code) if code == EN_CHANGE => {
                for id in [100,110,111] { form::set_check(form::item(d,id), id == 110); }
            }
            Ev::Cmd(113, code) if code == CBN_SELCHANGE => {
                for id in [100,110,111] { form::set_check(form::item(d,id), id == 111); }
            }
            Ev::Cmd(IDOK_, _) => {
                match speed_limit_command(d) {
                    Ok(command) => send_cmd(command),
                    Err(()) => { form::message(Some(d), "Speed limit", "Enter a positive number in MB/s.", MB_ICONWARNING); return 0; }
                }
                form::end(d, IDOK_);
            }
            Ev::Cmd(IDCANCEL_, _) | Ev::Close => form::end(d, IDCANCEL_),
            _ => {}
        }
        0
    }));
}

// ------------------------------------------------------------ shutdown ----

pub const SHUTDOWN_METHODS: [&str; 4] = ["Shutdown Windows", "Hibernate", "Sleep", "Exit"];

fn build_shutdown(owner: HWND) -> HWND {
    let d = form::create(Some(owner), "Shutdown", 420, 236, false);
    form::label(d, "Shutdown method", 14, 10, 200);
    let method = setting("shutdown_method").as_str().unwrap_or("Exit").to_string();
    let sel = SHUTDOWN_METHODS.iter().position(|m| m.eq_ignore_ascii_case(&method)).unwrap_or(3);
    form::combo(d, 10, &SHUTDOWN_METHODS, sel, 14, 34, 220);
    form::check(d, 11, "Run this action when all tasks are done", 14, 72, 360, ui(|u| u.countdown.armed));
    let c = form::label_id(d, 12, "", 14, 106, 280, 18);
    form::accent(c);
    form::button(d, 13, "Abort", 322, 100, 84);
    form::note(d, "After a 15 second countdown NewsQueue runs the selected action. Shutdown, Hibernate and Sleep affect Windows; Exit only closes NewsQueue.", 14, 134, 392, 48);
    form::hline(d, 14, 186, 392);
    form::button(d, IDOK_, "OK", 326, 198, 80);
    sync_shutdown(d);
    d
}

fn sync_shutdown(d: HWND) {
    let (left, armed) = ui(|u| (u.countdown.remaining(Instant::now()), u.countdown.armed));
    let method = SHUTDOWN_METHODS[form::combo_sel(form::item(d, 10)).min(3)];
    let t = match left {
        Some(secs) => format!("{method} in {secs}s"),
        None if armed => "Waiting for all tasks to finish".into(),
        None => String::new(),
    };
    let c = form::item(d, 12);
    if form::text(c) != t {
        form::set_text(c, &t);
    }
    form::enable(form::item(d, 13), armed);
    if form::checked(form::item(d, 11)) != armed {
        form::set_check(form::item(d, 11), armed);
    }
}

pub fn shutdown(owner: HWND) {
    let d = build_shutdown(owner);
    unsafe {
        SetTimer(Some(d), 1, 250, None);
    }
    form::run_modal(
        d,
        Some(owner),
        Box::new(|d, ev| {
            match ev {
                Ev::Cmd(10, code) if code == CBN_SELCHANGE => {
                    let m = SHUTDOWN_METHODS[form::combo_sel(form::item(d, 10)).min(3)];
                    send_cmd(Command::MergeSettings(serde_json::json!({ "shutdown_method": m })));
                }
                Ev::Cmd(11, _) => {
                    let on = form::checked(form::item(d, 11));
                    ui(|u| {
                        if on {
                            u.countdown.armed = true;
                        } else {
                            u.countdown.abort();
                        }
                    });
                    with_engine(|e| e.log("Shutdown", if on { "Shutdown armed." } else { "Shutdown disarmed." }));
                    sync_shutdown(d);
                }
                Ev::Cmd(13, _) => {
                    ui(|u| u.countdown.abort());
                    with_engine(|e| e.log("Shutdown", "Shutdown aborted."));
                    sync_shutdown(d);
                }
                Ev::Timer(_) => sync_shutdown(d),
                Ev::Cmd(IDOK_ | IDCANCEL_, _) | Ev::Close => form::end(d, IDOK_),
                _ => {}
            }
            0
        }),
    );
}

// ------------------------------------------------- one-line text prompts --

/// A modal prompt with one edit; `None` on Cancel.
fn prompt(owner: HWND, title: &str, intro: &str, value: &str, hint: &str, password: bool) -> Option<String> {
    let d = build_prompt(owner, title, intro, value, hint, password);
    let out = Rc::new(RefCell::new(None));
    let o2 = Rc::clone(&out);
    form::run_modal(
        d,
        Some(owner),
        Box::new(move |d, ev| {
            match ev {
                Ev::Cmd(IDOK_, _) => {
                    *o2.borrow_mut() = Some(form::text(form::item(d, 10)));
                    form::end(d, IDOK_);
                }
                Ev::Cmd(IDCANCEL_, _) | Ev::Close => form::end(d, IDCANCEL_),
                _ => {}
            }
            0
        }),
    );
    out.take()
}

fn build_prompt(owner: HWND, title: &str, intro: &str, value: &str, hint: &str, password: bool) -> HWND {
    let has_hint = !hint.is_empty();
    let intro_height=form::text_height(intro,392);
    let edit_y=18+intro_height;
    let hint_height=form::text_height(hint,392).max(18);
    let y=if has_hint {edit_y+28+hint_height+8} else {edit_y+32};
    let d = form::create(Some(owner), title, 420, (y+54).max(if has_hint {150} else {128}), false);
    form::label_id(d, 20, intro, 14, 12, 392, intro_height);
    let e = form::edit(d, 10, value, 14, edit_y, 392, if password { ES_PASSWORD as u32 } else { 0 });
    send(e, EM_SETSEL, 0, -1);
    if has_hint {
        form::note(d, hint, 14, edit_y+28, 392, hint_height);
    }
    form::hline(d, 14, y, 392);
    form::button(d, IDOK_, "OK", 240, y + 12, 80);
    form::button(d, IDCANCEL_, "Cancel", 326, y + 12, 80);
    d
}

pub fn rename(owner: HWND, subject: &str) -> Option<String> {
    prompt(owner, "Rename collection", "Collection name", subject, "", false)
}

pub fn execute(owner: HWND, current: &str) -> Option<String> {
    prompt(
        owner,
        "Execute command",
        "Command to run after download, PAR2 and unpack",
        current,
        "Placeholders: {folder}, {name}, {file}. Example: player.exe {file}",
        false,
    )
}

/// The archive password prompt; `asked` = the engine needs it to continue.
pub fn password(owner: HWND, subject: &str, asked: bool) -> Option<String> {
    let intro = if asked { format!("{subject} is password protected.") } else { format!("Archive password for {subject}") };
    prompt(owner, "Enter password", &intro, "", "", true)
}

// ------------------------------------------------------------ RSS feeds ---

fn build_rss_feeds(owner: HWND) -> HWND {
    let d = form::create(Some(owner), "RSS feeds", 560, 470, true);
    let (urls, filters) = with_engine(|e| (e.settings.rss_urls.join("\n"), e.settings.rss_filters.clone()));
    form::label_id(d, 20, "Feed URLs, one per line. API keys stay in the local settings file.", 14, 12, 532, 16);
    form::memo(d, 10, &urls, 14, 32, 532, 130);
    form::label_id(d, 21, "Autodownload filters (empty: only list the items)", 14, 178, 532, 16);
    form::note_id(d, 22, "pattern | A[P]  or  pattern | D.  The last matching line wins; P adds the item paused.", 14, 196, 532, 18);
    form::memo(d, 11, &filters, 14, 218, 532, 190);
    form::label_id(d, 23, "", 14, 420, 532, 2);
    unsafe { SetWindowLongW(form::item(d,23),GWL_STYLE,(form::CHILD|form::SS_ETCHEDHORZ) as i32); }
    form::button(d, 12, "Filter help", 14, 432, 90);
    form::button(d, IDOK_, "Save", 380, 432, 80);
    form::button(d, IDCANCEL_, "Cancel", 466, 432, 80);
    form::resize_layout(d, 560, 470, layout_rss_feeds);
    d
}

fn layout_rss_feeds(d: HWND, w: i32, h: i32) {
    let extra=(h-s(470)).max(0);
    let upper=extra/3;
    let place=|id,x,y,width: i32,height: i32| unsafe {
        let _=MoveWindow(form::item(d,id),x,y,width.max(1),height.max(1),true);
    };
    place(20,s(14),s(12),w-s(28),s(16));
    place(10,s(14),s(32),w-s(28),s(130)+upper);
    place(21,s(14),s(178)+upper,w-s(28),s(16));
    place(22,s(14),s(196)+upper,w-s(28),s(18));
    place(11,s(14),s(218)+upper,w-s(28),s(190)+extra-upper);
    place(23,s(14),h-s(50),w-s(28),s(2));
    place(12,s(14),h-s(38),s(90),s(26));
    place(IDOK_,w-s(180),h-s(38),s(80),s(26));
    place(IDCANCEL_,w-s(94),h-s(38),s(80),s(26));
}

pub fn rss_feeds(owner: HWND) {
    let d = build_rss_feeds(owner);
    form::run_modal(
        d,
        Some(owner),
        Box::new(|d, ev| {
            match ev {
                Ev::Cmd(12, _) => filter_help(d),
                Ev::Cmd(IDOK_, _) => {
                    let urls: Vec<String> = form::memo_text(form::item(d, 10)).lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
                    let filters = form::memo_text(form::item(d, 11));
                    send_cmd(Command::SetRssUrls(urls));
                    send_cmd(Command::MergeSettings(serde_json::json!({ "rss_filters": filters })));
                    form::end(d, IDOK_);
                }
                Ev::Cmd(IDCANCEL_, _) | Ev::Close => form::end(d, IDCANCEL_),
                _ => {}
            }
            0
        }),
    );
}

pub fn filter_help(owner: HWND) {
    form::message(
        Some(owner),
        "Filter help",
        "Each filter compares the item name with a wildcard pattern.\n\n\
         pattern | A    adds a match to the queue\n\
         pattern | D    denies a match\n\n\
         Optional letters after A: F (add as first), P (add paused), R (use the release name).\n\n\
         The last matching filter wins. Lines starting with ; are comments.",
        MB_ICONINFORMATION,
    );
}

// ------------------------------------------------------- screenshot mode --

pub(crate) fn audit_smoke(main: HWND) {
    let speed = build_speed_limit(main);
    form::set_check(form::item(speed,100), false);
    form::set_check(form::item(speed,110), true);
    form::set_text(form::item(speed,112), "22,50");
    assert_eq!(speed_limit_command(speed), Ok(Command::SetCustomSpeedLimit(22.5)));
    form::set_text(form::item(speed,112), "NaN");
    assert!(speed_limit_command(speed).is_err());
    form::set_check(form::item(speed,110), false);
    form::set_check(form::item(speed,111), true);
    send(form::item(speed,113), CB_SETCURSEL, 4, 0);
    assert_eq!(speed_limit_command(speed), Ok(Command::SetSpeedLimit(Some(5))));
    form::set_check(form::item(speed,111), false);
    assert_eq!(speed_limit_command(speed), Ok(Command::SetSpeedLimit(None)));
    unsafe { DestroyWindow(speed).unwrap(); }
    let about = build_about(main);
    assert_eq!(form::text(form::item(about,21)), "Check for updates");
    unsafe { DestroyWindow(about).unwrap(); }

    let d=build_stats(main,0);
    show_offscreen(d);
    for index in 0..5 {
        stats_show(d,index);
        assert_eq!(send(form::item(d,ID_STATS_TAB),TCM_GETCURSEL,0,0),index as isize);
        for (id,visible) in [(ID_STATS_CHART,index<=2),(ID_STATS_HIST,index==3),(ID_STATS_SEARCH,index==3),(ID_STATS_MISC,index==4)] {
            let control=form::item(d,id);
            assert!(!control.is_invalid());
            assert_eq!(unsafe {IsWindowVisible(control).as_bool()},visible);
        }
    }
    unsafe {DestroyWindow(d).unwrap();}
    let d=build_rss_feeds(main);
    show_offscreen(d);
    unsafe {
        SetWindowPos(d,None,0,0,s(800),s(650),SWP_NOMOVE|SWP_NOZORDER|SWP_NOACTIVATE).unwrap();
        let mut client=RECT::default();let mut edit=RECT::default();
        GetClientRect(d,&mut client).unwrap();GetWindowRect(form::item(d,10),&mut edit).unwrap();
        assert_eq!(edit.right-edit.left,client.right-s(28));
        let mut minimum=MINMAXINFO::default();
        send(d,WM_GETMINMAXINFO,0,&mut minimum as *mut _ as isize);
        assert!(minimum.ptMinTrackSize.x>=s(560));
        DestroyWindow(d).unwrap();
    }
}

/// Additional state variants for the isolated visual audit. Never applies actions.
pub(crate) fn audit_variant(main: HWND, index: usize) -> Option<(String, HWND)> {
    Some(match index {
        0..=3 => (format!("stats-{}", index+1), build_stats(main,index+1)),
        4..=13 => {
            let tag=(index-4) as u8;
            with_engine(|e| e.speed_limit_tag=(tag>0).then_some(tag));
            (format!("speed-{tag}"),build_speed_limit(main))
        }
        14..=17 => {
            let i=index-14;
            ui(|u| {u.snap.settings["shutdown_method"]=SHUTDOWN_METHODS[i].into();u.countdown.armed=false;u.countdown.until=None;});
            (format!("shutdown-idle-{i}"),build_shutdown(main))
        }
        18 => {
            with_engine(|e| e.pause_until=Some(Instant::now()+Duration::from_secs(615)));
            ("pause-active".into(),build_pause_for(main))
        }
        19 => ("password-long".into(),build_prompt(main,"Enter password","Very.Long.Collection.Name.İstanbul.日本語.with.many.parts.and.a.long.archive.name is password protected.","","",true)),
        20 => {
            let d=build_rss_feeds(main);
            unsafe {let _=SetWindowPos(d,None,0,0,s(800),s(650),SWP_NOMOVE|SWP_NOZORDER|SWP_NOACTIVATE);}
            ("rss-feeds-large".into(),d)
        }
        _ => return None,
    })
}
/// Opens a dialog without its loop for `--screenshot` (NQ_DEMO_DIALOG).
pub fn demo(name: &str, main: HWND) -> Option<HWND> {
    let d = match name {
        "setup" => crate::setup::build(main, std::env::var("NQ_DEMO_SETUP_PAGE").ok().and_then(|p| p.parse().ok()).unwrap_or(0)),
        "server" => crate::setup::build_server(main, &with_engine(|e| e.settings.servers[0].clone()), true),
        "stats" => build_stats(main, std::env::var("NQ_DEMO_STATS_TAB").ok().and_then(|v|v.parse::<usize>().ok()).unwrap_or(0).min(4)),
        "notes" => build_notes(main),
        "about" => build_about(main),
        "columns" => build_columns(main),
        "pause" => build_pause_for(main),
        "speed" => build_speed_limit(main),
        "version" => {
            ui(|u| u.update.status = format!("{APP_NAME} {VERSION} is up to date (latest release: {VERSION})."));
            build_version(main)
        }
        "shutdown" => {
            ui(|u| {
                u.countdown.armed = true;
                u.countdown.until = Some(Instant::now() + Duration::from_millis(12_400));
            });
            let d = build_shutdown(main);
            form::set_check(form::item(d, 11), true);
            d
        }
        "unrar-password" => build_prompt(main, "Enter password", "Aurora.Field.Recordings.2025 is password protected.", "", "", true),
        "rename" => build_prompt(main, "Rename collection", "Collection name", "Harbor.Lights.1080p", "", false),
        "execute" => build_prompt(main, "Execute command", "Command to run after download, PAR2 and unpack", "player.exe {file}", "Placeholders: {folder}, {name}, {file}. Example: player.exe {file}", false),
        "rss-feeds" => build_rss_feeds(main),
        _ => return None,
    };
    show_offscreen(d);
    unsafe { let _ = UpdateWindow(d); }
    Some(d)
}
