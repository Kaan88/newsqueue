//! NewsQueue desktop: native Win32 front end (common controls v6) over the
//! engine in `newsqueue-core`.
//!
//! The engine runs on its own thread (`EngineHost`); this window sends
//! `Command`s and redraws from the `Snapshot` on a timer, so downloads keep
//! running while the window is minimized or hidden in the tray.
//!
//! ```text
//! newsqueue [--config DIR] [--screenshot out.png] [file.nzb ...]
//! ```
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(clippy::all, non_snake_case)]

#[macro_use]
mod util;
mod cli;
mod dialogs;
mod form;
mod pick;
mod queue;
mod setup;
mod shutdown;
mod tray;
mod update;

pub use newsqueue_core::model;

use newsqueue_core::command::{Command, MoveDir};
use newsqueue_core::engine::{Engine, Event};
use newsqueue_core::host::{EngineHost, TICK_INTERVAL};
use newsqueue_core::model::*;
use newsqueue_core::snapshot::{EngineHandle, Snapshot};
use queue::{Bar, Row};
use std::cell::RefCell;
use std::ffi::c_void;
use std::time::{Duration, Instant};
use util::*;
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::*;

const CHILD: u32 = WS_CHILD.0 | WS_VISIBLE.0;
const TIMER_REFRESH: usize = 1;
const TIMER_SOON: usize = 2;

// Main toolbar.
const ID_CONNECT: i32 = 100;
const ID_PAUSE: i32 = 101;
const ID_PAUSEFOR: i32 = 102;
const ID_NZB: i32 = 103;
const ID_DOWNLOAD: i32 = 104;
const ID_CLEANUP: i32 = 105;
const ID_SCHEDULER: i32 = 106;
const ID_SHUTDOWN: i32 = 107;
const ID_SETUP: i32 = 108;
const ID_HOME: i32 = 109;
const ID_VERSION: i32 = 110;
const ID_ABOUT: i32 = 111;
const ID_NOTES: i32 = 112;
const ID_STATS: i32 = 113;
const ID_TRAY: i32 = 114;
// Queue toolbar and context menu.
const ID_RESUME: i32 = 200;
const ID_HOLD: i32 = 201;
const ID_TOP: i32 = 202;
const ID_UP: i32 = 203;
const ID_DOWN: i32 = 204;
const ID_BOTTOM: i32 = 205;
const ID_COLLAPSE_ALL: i32 = 206;
const ID_EXPAND_ALL: i32 = 207;
const ID_UNRAR_ON: i32 = 208;
const ID_UNRAR_OFF: i32 = 209;
const ID_UNRAR_FOLDER: i32 = 210;
const ID_RENAME: i32 = 211;
const ID_EXPORT: i32 = 212;
const ID_OPEN_FOLDER: i32 = 213;
const ID_PASSWORD: i32 = 214;
const ID_ETA: i32 = 215;
const ID_DELETE: i32 = 216;
const ID_CHANGE_FOLDER: i32 = 217;
const ID_UNPACK: i32 = 218;
const ID_EXECUTE: i32 = 219;
const ID_COLUMNS: i32 = 220;
const ID_RESUME_ALL: i32 = 221;
const ID_PAUSE_ALL: i32 = 222;
const ID_QFILTER: i32 = 230;
const ID_QUEUE: i32 = 231;
const ID_CONNS: i32 = 232;
// PAR2 page.
const ID_P2_START: i32 = 300;
const ID_P2_IMPORT: i32 = 301;
const ID_P2_REMOVE: i32 = 302;
const ID_P2_UP: i32 = 303;
const ID_P2_DOWN: i32 = 304;
const ID_P2_LIST: i32 = 305;
// Log page.
const ID_LOG_CLEAR: i32 = 320;
const ID_LOG_COPY: i32 = 321;
const ID_LOG_FILTER: i32 = 322;
const ID_LOG_LIST: i32 = 323;
// RSS page.
const ID_RSS_REFRESH: i32 = 340;
const ID_RSS_FEEDS: i32 = 341;
const ID_RSS_TIMER: i32 = 342;
const ID_RSS_HELP: i32 = 343;
const ID_RSS_FILTER: i32 = 344;
const ID_RSS_LIST: i32 = 345;
const ID_STATUS: i32 = 4;
const ID_TAB: i32 = 5;

pub const TAB_NAMES: [&str; 4] = ["Download queue", "PAR2", "Log", "RSS"];

#[derive(Default)]
pub struct Ui {
    pub main: HWND,
    tb: HWND,
    tab: HWND,
    sb: HWND,
    pages: [HWND; 4],
    qtb: HWND,
    qlabel: HWND,
    qfilter: HWND,
    pub queue: HWND,
    conns: HWND,
    p2_buttons: Vec<HWND>,
    par2: HWND,
    log_buttons: [HWND; 2],
    log_label: HWND,
    log_filter: HWND,
    log_count: HWND,
    log: HWND,
    rss_buttons: Vec<HWND>,
    rss_label: HWND,
    rss_filter: HWND,
    rss_count: HWND,
    rss: HWND,
    pub snap: Snapshot,
    handle: Option<EngineHandle>,
    pub demo: bool,
    rows: Vec<Row>,
    row_sigs: Vec<String>,
    /// Visible logical queue columns, in order.
    cols: Vec<usize>,
    col_key: String,
    log_rows: Vec<usize>,
    rss_rows: Vec<usize>,
    /// Selected queue row as (collection id, file index).
    sel: Option<(u64, Option<usize>)>,
    pub countdown: shutdown::Countdown,
    pub shutdown_mode: shutdown::ShutdownMode,
    /// Close for real on the next WM_CLOSE (tray Exit, shutdown Exit, update).
    pub exit_now: bool,
    pub update: dialogs::UpdateState,
    toast: Option<(String, Instant)>,
    /// A dialog opened from an engine event is up; don't open another.
    busy: bool,
    splitter_drag: Option<i32>,
    seq: u64,
}

thread_local! {
    static UI: RefCell<Ui> = RefCell::new(Ui::default());
    static HOST: RefCell<Option<EngineHost>> = const { RefCell::new(None) };
}

/// Short borrow of the UI state. Never send messages to controls inside it.
pub fn ui<R>(f: impl FnOnce(&mut Ui) -> R) -> R {
    UI.with(|u| f(&mut u.borrow_mut()))
}

pub fn send_cmd(c: Command) {
    if let Some(h) = ui(|u| u.handle.clone()) {
        h.send(c);
    }
    refresh_soon();
}

/// Direct engine access (full settings with secrets, logs, PAR2 list).
pub fn with_engine<R>(f: impl FnOnce(&mut Engine) -> R) -> R {
    HOST.with(|h| h.borrow_mut().as_mut().expect("engine").with(f))
}

pub fn main_hwnd() -> HWND {
    ui(|u| u.main)
}

/// A setting from the snapshot's public settings.
pub fn setting(key: &str) -> serde_json::Value {
    ui(|u| u.snap.settings.get(key).cloned().unwrap_or(serde_json::Value::Null))
}
pub fn setting_bool(key: &str) -> bool {
    setting(key).as_bool().unwrap_or(false)
}
pub fn setting_u8(key: &str) -> u8 {
    setting(key).as_u64().unwrap_or(0) as u8
}

fn refresh_soon() {
    let main = ui(|u| u.main);
    if !main.is_invalid() {
        unsafe {
            SetTimer(Some(main), TIMER_SOON, 80, None);
        }
    }
}

// ------------------------------------------------------------- startup ----

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = cli::run(&args) {
        std::process::exit(code);
    }
    let mut it = args.iter().skip(1);
    let mut screenshot: Option<String> = None;
    let mut nzbs: Vec<String> = vec![];
    while let Some(a) = it.next() {
        match a.as_str() {
            "--config" => {
                if let Some(d) = it.next() {
                    std::fs::create_dir_all(d).ok();
                    std::env::set_var("NQ_CONFIG", d);
                }
            }
            "--screenshot" => screenshot = it.next().cloned(),
            other if !other.starts_with("--") => nzbs.push(other.to_string()),
            _ => {}
        }
    }
    let demo = screenshot.is_some();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_WIN95_CLASSES | ICC_STANDARD_CLASSES | ICC_UPDOWN_CLASS | ICC_TAB_CLASSES | ICC_LISTVIEW_CLASSES | ICC_BAR_CLASSES,
        };
        let _ = InitCommonControlsEx(&icc);
    }
    if demo && std::env::var("NQ_DPI").is_err() {
        // Goldens are rendered at 100 % whatever the monitor.
        std::env::set_var("NQ_DPI", "96");
    }
    init_fonts();
    form::register_classes();

    let mut host = if demo { EngineHost::new(demo_engine()) } else { EngineHost::new(Engine::new(Settings::load(), false)) };
    if !demo {
        host.log("Main", "Native Win32 front end.");
        host.start(TICK_INTERVAL);
        host.checkin();
    } else {
        host.pump_local();
    }
    let handle = host.handle();
    HOST.with(|h| *h.borrow_mut() = Some(host));
    ui(|u| {
        u.snap = handle.snapshot();
        u.handle = Some(handle);
        u.demo = demo;
        u.shutdown_mode = shutdown::ShutdownMode::Real;
        u.update = dialogs::UpdateState::new();
    });
    if !demo {
        if setting_bool("startup_connect") {
            send_cmd(Command::Connect);
        }
        #[cfg(debug_assertions)]
        if std::env::var("NQ_TEST_ARM_SHUTDOWN").as_deref() == Ok("exit") {
            send_cmd(Command::MergeSettings(serde_json::json!({
                "shutdown_method": "Exit", "minimize_to_tray": true, "minimize_to_tray_on_close": true
            })));
            ui(|u| u.countdown.armed = true);
            with_engine(|e| e.log("Shutdown", "Test shutdown armed (Exit)."));
        }
        nzbs.into_iter().for_each(|p| add_nzb(p, false));
        let (on, bind, key) = with_engine(|e| (e.settings.web_access, e.settings.web_access_bind.clone(), e.settings.web_access_key.clone()));
        start_web_access(on, &bind, &key);
        tray::spawn();
    }

    let main = create_main();
    unsafe {
        SetTimer(Some(main), TIMER_REFRESH, 250, None);
    }
    refresh();
    if let Some(path) = screenshot {
        std::process::exit(take_screenshot(main, &path));
    }
    unsafe {
        let _ = ShowWindow(main, SW_SHOW);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if pre_translate(&msg) || form::is_dialog_message(&msg) {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    HOST.with(|h| {
        if let Some(mut host) = h.borrow_mut().take() {
            host.checkout();
            host.save_all();
        }
    });
}

static WEB_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The "Web access" setting: serves the JSON API and the egui web UI of
/// `newsqueue-server` from this process, on the engine handle the window
/// uses. Started once; turning it off takes effect after a restart.
pub fn start_web_access(on: bool, bind: &str, key: &str) {
    use std::sync::atomic::Ordering::Relaxed;
    if !on || ui(|u| u.demo) || WEB_RUNNING.load(Relaxed) {
        return;
    }
    let Some(handle) = ui(|u| u.handle.clone()) else { return };
    let log = |text: String| with_engine(|e| e.log("Web", &text));
    let addr: std::net::SocketAddr = match bind.trim().parse() {
        Ok(a) => a,
        Err(_) => return log(format!("Web access not started: \"{bind}\" is not an address:port.")),
    };
    let api_key = Some(key.to_string()).filter(|k| !k.is_empty());
    if api_key.is_none() && !addr.ip().is_loopback() {
        return log(format!("Web access not started: {addr} needs a password (Setup, Miscellaneous #3)."));
    }
    let web_dir = newsqueue_server::web_dir(None);
    match newsqueue_server::spawn(addr, newsqueue_server::Api { handle, api_key, web_dir: web_dir.clone() }) {
        Ok(()) => {
            WEB_RUNNING.store(true, Relaxed);
            let ui_note = if web_dir.is_some() { "" } else { " (no web UI files found; API only)" };
            log(format!("Web access on http://{addr}{ui_note}."));
        }
        Err(e) => {
            log(format!("Web access not started: {e}"));
            ui(|u| u.toast = Some((format!("Web access: {e}"), Instant::now())));
        }
    }
}

/// Sample queue, log, PAR2 and RSS data for screenshots, with neutral paths.
fn demo_engine() -> Engine {
    let mut settings = Settings::default();
    settings.servers[0].name = "Primary provider".into();
    settings.servers[0].address = "news.example.net".into();
    settings.servers[0].username = "demo-user".into();
    settings.servers[0].password = "secret".into();
    settings.servers[0].connections = 20;
    settings.servers[0].enabled = true;
    settings.max_speed_kbps = 12_500.0;
    settings.max_speed_mbps = 100.0;
    let mut engine = Engine::new(settings, true);
    let base = app_base().to_string_lossy().to_string();
    let fix = |p: &mut String| {
        if p.starts_with(&base) {
            *p = format!(r"C:\NewsQueue{}", &p[base.len()..]);
        }
    };
    for p in [&mut engine.settings.download_dir, &mut engine.settings.nzb_dir, &mut engine.settings.temp_dir, &mut engine.settings.unrar_dir] {
        fix(p);
    }
    for c in &mut engine.queue {
        fix(&mut c.folder);
        fix(&mut c.unrar_folder);
    }
    // One collection downloading, one paused part-way, as in real use.
    for (c, status, frac) in engine.queue.iter_mut().skip(1).zip([(ItemStatus::Downloading, 0.63), (ItemStatus::Paused, 0.38)]).map(|(c, (s, f))| (c, s, f)) {
        c.status = status;
        c.downloaded = c.total_bytes as f64 * frac;
        let n = c.files.len() as f64;
        for (i, f) in c.files.iter_mut().enumerate() {
            let share = (frac * n - i as f64).clamp(0.0, 1.0);
            f.downloaded = f.bytes as f64 * share;
            f.done = share >= 1.0;
        }
    }
    let subject = engine.queue.iter().find(|c| c.status == ItemStatus::Downloading).map(|c| c.subject.clone()).unwrap_or_default();
    engine.threads = [
        ("Download", format!("{subject}.part03.rar  [412/1024]"), "Downloading", "news.example.net"),
        ("Download", format!("{subject}.part03.rar  [413/1024]"), "Downloading", "news.example.net"),
        ("Download", format!("{subject}.part04.rar  [88/1024]"), "Downloading", "news.example.net"),
        ("Download", format!("{subject}.part04.rar  [89/1024]"), "Downloading", "backup.example.org"),
        ("Idle", String::new(), "Connected", "backup.example.org"),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (a, d, st, srv))| model::ThreadRow { number: i as u32 + 1, action: a.into(), description: d, status: st.into(), server: srv.into() })
    .collect();
    engine.speed_history = (0..120).map(|i| (4.6 + 0.35 * ((i as f32) * 0.31).sin() + 0.15 * ((i as f32) * 0.9).cos()) * 1024.0 * 1024.0).collect();
    engine.speed_bps = 4.82 * 1024.0 * 1024.0;
    engine.connected = true;
    engine.free_gb = 412.33;
    engine
}

fn take_screenshot(main: HWND, path: &str) -> i32 {
    if let Some(parent) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let tab = match std::env::var("NQ_DEMO_TAB").as_deref() {
        Ok("par2") => 1,
        Ok("log") => 2,
        Ok("rss") => 3,
        _ => 0,
    };
    unsafe {
        let _ = SetWindowPos(main, None, 0, 0, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        let _ = ShowWindow(main, SW_SHOWNOACTIVATE);
    }
    select_tab(tab);
    refresh();
    // Select the first downloading collection, as a user would have.
    let row = ui(|u| u.rows.iter().position(|r| matches!(r, Row::Coll(i) if u.snap.queue[*i].status == Some(ItemStatus::Downloading))));
    if let Some(r) = row {
        form::lv_select(ui(|u| u.queue), r);
    }
    pump(400);
    let target = match std::env::var("NQ_DEMO_DIALOG") {
        Ok(name) => match dialogs::demo(&name, main) {
            Some(d) => d,
            None => {
                eprintln!("unknown NQ_DEMO_DIALOG {name}");
                return 1;
            }
        },
        Err(_) => main,
    };
    pump(400);
    util::capture(target, path);
    if std::path::Path::new(path).exists() {
        0
    } else {
        1
    }
}

// ---------------------------------------------------------- main window ---

fn register(name: PCWSTR, proc_: WNDPROC, bg: HBRUSH) {
    unsafe {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
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

fn work_area() -> RECT {
    let mut r = RECT::default();
    unsafe {
        let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut r as *mut _ as *mut c_void), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    }
    r
}

struct Tb(i32, &'static str, &'static str, &'static str, bool); // id, icon, text, tooltip, check

fn toolbar(parent: HWND, size: i32, items: &[Option<Tb>], text: bool) -> HWND {
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
    let names: Vec<&str> = items.iter().flatten().map(|t| t.1).collect();
    send(tb, TB_SETIMAGELIST, 0, imagelist(s(size), &names));
    send(tb, TB_SETMAXTEXTROWS, text as usize, 0);
    send(tb, TB_SETEXTENDEDSTYLE, 0, (TBSTYLE_EX_DOUBLEBUFFER | TBSTYLE_EX_MIXEDBUTTONS) as isize);
    let mut btns = vec![];
    let mut img = 0;
    for it in items {
        let mut b = TBBUTTON::default();
        match it {
            None => {
                b.fsStyle = BTNS_SEP as u8;
                b.iBitmap = s(8);
            }
            Some(t) => {
                b.iBitmap = img;
                img += 1;
                b.idCommand = t.0;
                b.fsState = TBSTATE_ENABLED as u8;
                b.fsStyle = (if t.4 { BTNS_CHECK } else { BTNS_BUTTON }) as u8;
                if text {
                    // Wide labels ("Scheduler", "Shutdown") get wider buttons.
                    b.fsStyle |= BTNS_AUTOSIZE as u8;
                }
                // The text doubles as the tooltip (TBSTYLE_EX_MIXEDBUTTONS
                // hides it on icon-only bars).
                let label = if text { t.2 } else { t.3 };
                let z: &'static mut Vec<u16> = Box::leak(Box::new(wz(label)));
                b.iString = z.as_ptr() as isize;
            }
        }
        btns.push(b);
    }
    send(tb, TB_ADDBUTTONSW, btns.len(), btns.as_ptr() as isize);
    if text {
        send(tb, TB_SETBUTTONWIDTH, 0, ((s(90) as isize) << 16) | s(55) as isize);
        send(tb, TB_SETBUTTONSIZE, 0, ((s(53) as isize) << 16) | s(55) as isize);
    }
    tb
}

fn create_main() -> HWND {
    unsafe {
        register(w!("NewsQueueMain"), Some(main_proc), GetSysColorBrush(COLOR_BTNFACE));
        register(w!("NewsQueuePage"), Some(page_proc), HBRUSH::default());
        let wa = work_area();
        let cw = (SAVED_WIDTH as i32).min((wa.right - wa.left) * 96 / DPI.load(std::sync::atomic::Ordering::Relaxed) as i32 - 24);
        let ch = (SAVED_HEIGHT as i32).min((wa.bottom - wa.top) * 96 / DPI.load(std::sync::atomic::Ordering::Relaxed) as i32 - 60);
        let style = WS_OVERLAPPEDWINDOW.0 | WS_CLIPCHILDREN.0;
        let (ow, oh) = outer(style, WS_EX_ACCEPTFILES.0, cw, ch);
        let x = wa.left + ((wa.right - wa.left) - ow).max(0) / 2;
        let y = wa.top + ((wa.bottom - wa.top) - oh).max(0) / 2;
        let main = mkpx("NewsQueueMain", &format!("{APP_NAME} {VERSION}"), style, WS_EX_ACCEPTFILES.0, x, y, ow, oh, None, 0);
        DragAcceptFiles(main, true);
        let tb = toolbar(
            main,
            32,
            &[
                Some(Tb(ID_CONNECT, "connect", "Connect", "Connect to / disconnect from the Usenet servers", true)),
                None,
                Some(Tb(ID_PAUSE, "pause", "Pause", "Pause/resume downloading [F1]", true)),
                Some(Tb(ID_PAUSEFOR, "pausefor", "Pause for", "Pause for a number of minutes", false)),
                None,
                Some(Tb(ID_NZB, "nzb", "NZB", "Import NZB files [hold Shift for paused]", false)),
                None,
                Some(Tb(ID_DOWNLOAD, "download", "Download", "Open the download folder [hold Shift for the unrar folder]", false)),
                None,
                Some(Tb(ID_CLEANUP, "cleanup", "Cleanup", "Remove finished entries and completed PAR2 sets", false)),
                None,
                Some(Tb(ID_SCHEDULER, "scheduler", "Scheduler", "Enable/disable the scheduler", true)),
                None,
                Some(Tb(ID_SHUTDOWN, "shutdown", "Shutdown", "Shut down when all tasks are done", true)),
                None,
                Some(Tb(ID_SETUP, "setup", "Setup", "Setup options", false)),
                None,
                Some(Tb(ID_HOME, "home", "Home", "Project home page", false)),
                Some(Tb(ID_VERSION, "version", "Version?", "Check for a new version", false)),
                Some(Tb(ID_ABOUT, "about", "About", "About NewsQueue", false)),
                Some(Tb(ID_NOTES, "notes", "Notes", "Notes", false)),
                Some(Tb(ID_STATS, "stats", "Stats", "Statistics and download history", false)),
                Some(Tb(ID_TRAY, "tray", "To tray", "Minimize to the tray", false)),
            ],
            true,
        );
        let tab = mkpx("SysTabControl32", "", sty!(CHILD, WS_CLIPSIBLINGS, TCS_FOCUSNEVER), 0, 0, 0, 100, 100, Some(main), ID_TAB);
        send(tab, TCM_SETIMAGELIST, 0, imagelist(s(16), &["queue", "shield", "page", "rss"]));
        for (i, t) in TAB_NAMES.iter().enumerate() {
            let z = wz(t);
            let mut it = TCITEMW::default();
            it.mask = TCIF_TEXT | TCIF_IMAGE;
            it.pszText = PWSTR(z.as_ptr() as *mut u16);
            it.iImage = i as i32;
            send(tab, TCM_INSERTITEMW, i, &it as *const _ as isize);
        }
        let mut pages = [HWND::default(); 4];
        for (i, p) in pages.iter_mut().enumerate() {
            *p = mkpx("NewsQueuePage", "", sty!(WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS), WS_EX_CONTROLPARENT.0, 0, 0, 10, 10, Some(main), 10 + i as i32);
        }
        // Queue page.
        let pq = pages[0];
        let qtb = toolbar(
            pq,
            16,
            &[
                Some(Tb(ID_RESUME, "play", "", "Resume the selected collection [Ctrl+D]", false)),
                Some(Tb(ID_HOLD, "pause_s", "", "Pause the selected collection [Ctrl+P]", false)),
                None,
                Some(Tb(ID_TOP, "top", "", "Move to top", false)),
                Some(Tb(ID_UP, "up", "", "Move up [Ctrl+Up]", false)),
                Some(Tb(ID_DOWN, "down", "", "Move down [Ctrl+Down]", false)),
                Some(Tb(ID_BOTTOM, "bottom", "", "Move to bottom", false)),
                None,
                Some(Tb(ID_COLLAPSE_ALL, "collapse", "", "Collapse all", false)),
                Some(Tb(ID_EXPAND_ALL, "expand", "", "Expand all", false)),
                None,
                Some(Tb(ID_UNRAR_ON, "unrar", "", "Unpack after download", false)),
                Some(Tb(ID_UNRAR_OFF, "nounrar", "", "Do not unpack", false)),
                Some(Tb(ID_UNRAR_FOLDER, "folder", "", "Change the unrar folder", false)),
                None,
                Some(Tb(ID_RENAME, "rename", "", "Rename the collection", false)),
                Some(Tb(ID_EXPORT, "export", "", "Export as NZB [Ctrl+E]", false)),
                Some(Tb(ID_OPEN_FOLDER, "folder", "", "Open the download folder", false)),
                None,
                Some(Tb(ID_PASSWORD, "key", "", "Set the archive password", false)),
                Some(Tb(ID_ETA, "eta", "", "Estimated time left", false)),
            ],
            false,
        );
        let qlabel = mkpx("STATIC", "Filter", sty!(CHILD, 2 /* SS_RIGHT */), 0, 0, 0, 10, 10, Some(pq), 0);
        let qfilter = mkpx("EDIT", "", sty!(CHILD, WS_TABSTOP, ES_AUTOHSCROLL), WS_EX_CLIENTEDGE.0, 0, 0, 10, 10, Some(pq), ID_QFILTER);
        cue(qfilter, "Filter subjects");
        let lvs = sty!(CHILD, WS_TABSTOP, LVS_REPORT, LVS_SHOWSELALWAYS, LVS_OWNERDATA, LVS_SINGLESEL);
        let queue = mkpx("SysListView32", "", lvs | LVS_NOSORTHEADER, WS_EX_CLIENTEDGE.0, 0, 0, 10, 10, Some(pq), ID_QUEUE);
        form::lv_setup(queue, LVS_EX_HEADERDRAGDROP);
        send(queue, LVM_SETIMAGELIST, LVSIL_SMALL as usize, imagelist(s(16), &["plus", "minus", "file"]));
        let conns = mkpx("SysListView32", "", lvs | LVS_NOSORTHEADER, WS_EX_CLIENTEDGE.0, 0, 0, 10, 10, Some(pq), ID_CONNS);
        form::lv_setup(conns, 0);
        form::lv_cols(conns, &[("#", 34, true), ("Action", 110, false), ("Description", 420, false), ("Status", 140, false), ("Server", 200, false)]);
        // PAR2 page.
        let pp = pages[1];
        let mut p2_buttons = vec![];
        for (id, t) in [(ID_P2_START, "Start"), (ID_P2_IMPORT, "Import..."), (ID_P2_REMOVE, "Remove"), (ID_P2_UP, "Up"), (ID_P2_DOWN, "Down")] {
            p2_buttons.push(mkpx("BUTTON", t, sty!(CHILD, WS_TABSTOP, BS_PUSHBUTTON), 0, 0, 0, 10, 10, Some(pp), id));
        }
        let par2 = mkpx("SysListView32", "", lvs | LVS_NOSORTHEADER, WS_EX_CLIENTEDGE.0, 0, 0, 10, 10, Some(pp), ID_P2_LIST);
        form::lv_setup(par2, 0);
        form::lv_cols(par2, &[("Name", 380, false), ("Size", 90, true), ("Blocks", 70, true), ("Info", 260, false), ("Unrar", 55, false), ("Status", 220, false)]);
        // Log page.
        let pl = pages[2];
        let log_buttons = [
            mkpx("BUTTON", "Clear", sty!(CHILD, WS_TABSTOP, BS_PUSHBUTTON), 0, 0, 0, 10, 10, Some(pl), ID_LOG_CLEAR),
            mkpx("BUTTON", "Copy", sty!(CHILD, WS_TABSTOP, BS_PUSHBUTTON), 0, 0, 0, 10, 10, Some(pl), ID_LOG_COPY),
        ];
        let log_label = mkpx("STATIC", "Filter", sty!(CHILD, 2), 0, 0, 0, 10, 10, Some(pl), 0);
        let log_filter = mkpx("EDIT", "", sty!(CHILD, WS_TABSTOP, ES_AUTOHSCROLL), WS_EX_CLIENTEDGE.0, 0, 0, 10, 10, Some(pl), ID_LOG_FILTER);
        cue(log_filter, "Filter log lines");
        let log_count = mkpx("STATIC", "", sty!(CHILD, 2), 0, 0, 0, 10, 10, Some(pl), 0);
        let log = mkpx("SysListView32", "", lvs | LVS_NOSORTHEADER, WS_EX_CLIENTEDGE.0, 0, 0, 10, 10, Some(pl), ID_LOG_LIST);
        form::lv_setup(log, 0);
        form::lv_cols(log, &[("Time", 80, false), ("Area", 90, false), ("Message", 1000, false)]);
        // RSS page.
        let pr = pages[3];
        let mut rss_buttons = vec![];
        for (id, t) in [(ID_RSS_REFRESH, "Refresh"), (ID_RSS_FEEDS, "Feeds..."), (ID_RSS_HELP, "Filter help")] {
            rss_buttons.push(mkpx("BUTTON", t, sty!(CHILD, WS_TABSTOP, BS_PUSHBUTTON), 0, 0, 0, 10, 10, Some(pr), id));
        }
        rss_buttons.push(mkpx("BUTTON", "Refresh on a timer", sty!(CHILD, WS_TABSTOP, BS_AUTOCHECKBOX), 0, 0, 0, 10, 10, Some(pr), ID_RSS_TIMER));
        let rss_label = mkpx("STATIC", "Filter", sty!(CHILD, 2), 0, 0, 0, 10, 10, Some(pr), 0);
        let rss_filter = mkpx("EDIT", "", sty!(CHILD, WS_TABSTOP, ES_AUTOHSCROLL), WS_EX_CLIENTEDGE.0, 0, 0, 10, 10, Some(pr), ID_RSS_FILTER);
        cue(rss_filter, "Filter names");
        let rss_count = mkpx("STATIC", "", sty!(CHILD, 2), 0, 0, 0, 10, 10, Some(pr), 0);
        let rss = mkpx("SysListView32", "", lvs | LVS_NOSORTHEADER, WS_EX_CLIENTEDGE.0, 0, 0, 10, 10, Some(pr), ID_RSS_LIST);
        form::lv_setup(rss, 0);
        form::lv_cols(rss, &[("Name", 520, false), ("Date", 160, false), ("Description", 600, false)]);
        // Status bar.
        let sb = mkpx("msctls_statusbar32", "", sty!(CHILD, SBARS_SIZEGRIP, WS_CLIPSIBLINGS), 0, 0, 0, 0, 0, Some(main), ID_STATUS);
        let wd = [90, 95, 120, 100, 105, 100, 120, 120];
        let mut edges = vec![];
        let mut acc = 0;
        for x in wd {
            acc += s(x);
            edges.push(acc);
        }
        edges.push(-1);
        send(sb, SB_SETPARTS, edges.len(), edges.as_ptr() as isize);
        send(sb, SB_SETTEXTW, 2 | SBT_OWNERDRAW as usize, 0);

        ui(|u| {
            u.main = main;
            u.tb = tb;
            u.tab = tab;
            u.sb = sb;
            u.pages = pages;
            u.qtb = qtb;
            u.qlabel = qlabel;
            u.qfilter = qfilter;
            u.queue = queue;
            u.conns = conns;
            u.p2_buttons = p2_buttons;
            u.par2 = par2;
            u.log_buttons = log_buttons;
            u.log_label = log_label;
            u.log_filter = log_filter;
            u.log_count = log_count;
            u.log = log;
            u.rss_buttons = rss_buttons;
            u.rss_label = rss_label;
            u.rss_filter = rss_filter;
            u.rss_count = rss_count;
            u.rss = rss;
        });
        for p in pages {
            let _ = SetWindowPos(p, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        }
        select_tab(0);
        layout(main);
        main
    }
}

fn cue(edit: HWND, t: &str) {
    let z = wz(t);
    send(edit, 0x1501 /* EM_SETCUEBANNER */, 1, z.as_ptr() as isize);
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
        let _ = MoveWindow(h, x, y, w.max(0), hh.max(0), true);
    }
}

/// Height of the connections panel, or 0 when hidden.
fn conns_height(page_h: i32) -> i32 {
    if !setting_bool("thread_list_visible") {
        return 0;
    }
    let h = setting("thread_list_height").as_f64().unwrap_or(THREAD_PANEL_HEIGHT as f64) as i32;
    s(h.clamp(60, 600)).min(page_h / 2)
}

fn layout(h: HWND) {
    let u = ui(|u| {
        (
            u.tb,
            u.tab,
            u.sb,
            u.pages,
            u.qtb,
            u.qlabel,
            u.qfilter,
            u.queue,
            u.conns,
            u.p2_buttons.clone(),
            u.par2,
            u.log_buttons,
            u.log_label,
            u.log_filter,
            u.log_count,
            u.log,
            u.rss_buttons.clone(),
            u.rss_label,
            u.rss_filter,
            u.rss_count,
            u.rss,
        )
    });
    let (tb, tab, sb, pages, qtb, qlabel, qfilter, queue, conns, p2b, par2, lb, llabel, lfilter, lcount, log, rb, rlabel, rfilter, rcount, rss) = u;
    if tb.is_invalid() {
        return;
    }
    let mut rc = RECT::default();
    unsafe {
        let _ = GetClientRect(h, &mut rc);
    }
    let (w, hh) = (rc.right, rc.bottom);
    send(sb, WM_SIZE, 0, 0);
    let sbh = wnd_h(sb);
    let tbh = hi(send(tb, TB_GETBUTTONSIZE, 0, 0) as isize) + s(4);
    mv(tb, 0, 0, w, tbh);
    let th = hh - tbh - sbh;
    mv(tab, 0, tbh, w, th);
    let mut r = RECT { left: 0, top: 0, right: w, bottom: th };
    send(tab, TCM_ADJUSTRECT, 0, &mut r as *mut _ as isize);
    let (px, py, pw, ph) = (r.left, tbh + r.top, r.right - r.left, r.bottom - r.top);
    for p in pages {
        mv(p, px, py, pw, ph);
    }
    // Queue page: secondary toolbar + filter, queue, splitter, connections.
    let qh = s(30);
    let eh = s(23);
    let ew = s(190);
    mv(qtb, 0, (qh - hi(send(qtb, TB_GETBUTTONSIZE, 0, 0) as isize)) / 2, pw - ew - s(60), qh);
    mv(qfilter, pw - ew - s(4), (qh - eh) / 2, ew, eh);
    mv(qlabel, pw - ew - s(52), (qh - s(16)) / 2, s(40), s(16));
    let ch = conns_height(ph - qh);
    let gap = if ch > 0 { s(5) } else { 0 };
    mv(queue, 0, qh, pw, ph - qh - ch - gap);
    mv(conns, 0, ph - ch, pw, ch);
    form::show(conns, ch > 0);
    // Button rows of the other pages.
    let bh = s(25);
    let by = (qh - bh) / 2;
    let mut x = s(4);
    for b in &p2b {
        mv(*b, x, by, s(80), bh);
        x += s(84);
    }
    mv(par2, 0, qh, pw, ph - qh);
    mv(lb[0], s(4), by, s(75), bh);
    mv(lb[1], s(83), by, s(75), bh);
    mv(llabel, s(170), (qh - s(16)) / 2, s(40), s(16));
    mv(lfilter, s(216), (qh - eh) / 2, s(240), eh);
    mv(lcount, pw - s(150), (qh - s(16)) / 2, s(140), s(16));
    mv(log, 0, qh, pw, ph - qh);
    let mut x = s(4);
    for (i, b) in rb.iter().enumerate() {
        let bw = if i == 3 { s(140) } else { s(80) };
        mv(*b, x, by, bw, bh);
        x += bw + s(4);
    }
    mv(rlabel, x + s(10), (qh - s(16)) / 2, s(40), s(16));
    mv(rfilter, x + s(56), (qh - eh) / 2, s(220), eh);
    mv(rcount, pw - s(150), (qh - s(16)) / 2, s(140), s(16));
    mv(rss, 0, qh, pw, ph - qh);
}

fn select_tab(i: usize) {
    let (tab, pages) = ui(|u| (u.tab, u.pages));
    send(tab, TCM_SETCURSEL, i, 0);
    for (j, p) in pages.iter().enumerate() {
        form::show(*p, i == j);
    }
}

fn current_tab() -> usize {
    send(ui(|u| u.tab), TCM_GETCURSEL, 0, 0).max(0) as usize
}

// -------------------------------------------------------------- refresh ---

fn visible_cols(snap: &Snapshot) -> (Vec<usize>, Vec<i32>, String) {
    let vis: Vec<bool> = serde_json::from_value(snap.settings.get("column_visible").cloned().unwrap_or_default()).unwrap_or_else(|_| vec![true; 12]);
    let widths: Vec<f32> = serde_json::from_value(snap.settings.get("column_widths").cloned().unwrap_or_default()).unwrap_or_else(|_| QUEUE_COLUMN_WIDTHS.to_vec());
    let mut cols: Vec<usize> = (0..queue::COLUMN_COUNT).filter(|i| vis.get(*i).copied().unwrap_or(true)).collect();
    if cols.is_empty() {
        cols.push(0);
    }
    let w: Vec<i32> = cols
        .iter()
        .map(|&c| {
            let base = widths.get(c).copied().unwrap_or(QUEUE_COLUMN_WIDTHS[c]) as i32;
            // The Status column holds the progress bar.
            if c == queue::COL_STATUS { base.max(150) } else { base }
        })
        .collect();
    let key = format!("{cols:?}{w:?}");
    (cols, w, key)
}

fn rebuild_columns(lv: HWND, cols: &[usize], widths: &[i32]) {
    while send(lv, LVM_DELETECOLUMN, 0, 0) != 0 {}
    let spec: Vec<(&str, i32, bool)> = cols.iter().zip(widths).map(|(&c, &w)| (QUEUE_COLUMN_NAMES[c], w, matches!(c, 1 | 2))).collect();
    form::lv_cols(lv, &spec);
}

/// Pulls the newest snapshot and updates what changed.
fn refresh() {
    let Some(handle) = ui(|u| u.handle.clone()) else { return };
    if ui(|u| u.demo) {
        HOST.with(|h| {
            if let Some(h) = h.borrow_mut().as_mut() {
                h.pump_local();
            }
        });
    }
    let snap = handle.snapshot();
    let first = ui(|u| u.seq == 0);
    if !first && ui(|u| u.seq == snap.seq) {
        return;
    }
    let (queue, conns, log, par2, rss, sb, tb) = ui(|u| (u.queue, u.conns, u.log, u.par2, u.rss, u.sb, u.tb));
    // Columns follow the settings (Columns dialog, Setup).
    let (cols, widths, key) = visible_cols(&snap);
    if ui(|u| u.col_key != key) {
        rebuild_columns(queue, &cols, &widths);
        ui(|u| {
            u.cols = cols.clone();
            u.col_key = key;
            u.row_sigs.clear();
        });
    }
    // Queue rows.
    let filter = form::text(ui(|u| u.qfilter));
    let rows = queue::flatten(&snap.queue, &filter);
    let sigs: Vec<String> = rows.iter().map(|r| queue::signature(&snap.queue, *r, &cols)).collect();
    let (old_len, old_sigs, sel) = ui(|u| (u.rows.len(), std::mem::take(&mut u.row_sigs), u.sel));
    let log_rows = filter_lines(&snap, &form::text(ui(|u| u.log_filter)));
    let rf = form::text(ui(|u| u.rss_filter)).to_lowercase();
    let rss_rows: Vec<usize> = (0..snap.rss.len()).filter(|i| rf.is_empty() || snap.rss[*i].name.to_lowercase().contains(&rf)).collect();
    let (old_log, old_log_last) = ui(|u| (u.log_rows.len(), u.snap.log_tail.last().cloned()));
    let log_changed = old_log != log_rows.len() || old_log_last != snap.log_tail.last().cloned();
    let threads_changed = ui(|u| u.snap.threads != snap.threads);
    let par2_changed = ui(|u| u.snap.par2 != snap.par2);
    let rss_changed = ui(|u| u.snap.rss != snap.rss || u.rss_rows != rss_rows);
    let (n_rows, n_log, n_rss, n_threads, n_par2) = (rows.len(), log_rows.len(), rss_rows.len(), snap.threads.len(), snap.par2.len());
    let status = status_texts(&snap);
    let flags = (snap.connected, snap.paused, setting_in(&snap, "scheduler_enabled"), snap.rss_timer);
    let speed = (snap.speed_bps, snap.settings.get("speed_bytes_mode").and_then(|v| v.as_u64()).unwrap_or(3) as u8);
    let tray_opts = (setting_in(&snap, "speed_in_tray"), setting_in(&snap, "tray_balloon"));
    let new_sel_row = sel.and_then(|(id, file)| {
        rows.iter().position(|r| match *r {
            Row::Coll(i) => file.is_none() && snap.queue[i].id == id,
            Row::File(i, j) => file == Some(j) && snap.queue[i].id == id,
        })
    });
    ui(|u| {
        u.seq = snap.seq;
        u.snap = snap;
        u.rows = rows;
        u.row_sigs = sigs.clone();
        u.log_rows = log_rows;
        u.rss_rows = rss_rows;
    });
    tray::set_options(tray_opts.0, tray_opts.1);
    tray::set_speed(speed.0, speed.1);
    // Only the rows whose content changed are repainted.
    if n_rows != old_len || first {
        send(queue, LVM_SETITEMCOUNT, n_rows, (LVSICF_NOINVALIDATEALL | LVSICF_NOSCROLL) as isize);
        unsafe {
            let _ = InvalidateRect(Some(queue), None, false);
        }
    } else {
        for (a, b) in queue::changed_ranges(&old_sigs, &sigs) {
            send(queue, LVM_REDRAWITEMS, a, b as isize);
        }
    }
    let cur = form::lv_selected(queue);
    match (new_sel_row, cur) {
        (Some(r), c) if c != Some(r) => form::lv_select(queue, r),
        (None, Some(_)) if sel.is_some() => {
            // The selected collection is gone (deleted, cleaned up).
            send(queue, LVM_SETITEMSTATE, usize::MAX, &LVITEMW { stateMask: LVIS_SELECTED, ..Default::default() } as *const _ as isize);
            ui(|u| u.sel = None);
        }
        _ => {}
    }
    if threads_changed || first {
        send(conns, LVM_SETITEMCOUNT, n_threads, LVSICF_NOSCROLL as isize);
        unsafe {
            let _ = InvalidateRect(Some(conns), None, false);
        }
    }
    if par2_changed || first {
        send(par2, LVM_SETITEMCOUNT, n_par2, LVSICF_NOSCROLL as isize);
        unsafe {
            let _ = InvalidateRect(Some(par2), None, false);
        }
    }
    if log_changed || first {
        send(log, LVM_SETITEMCOUNT, n_log, LVSICF_NOSCROLL as isize);
        if n_log > 0 {
            send(log, LVM_ENSUREVISIBLE, n_log - 1, 0);
        }
        unsafe {
            let _ = InvalidateRect(Some(log), None, false);
        }
        form::set_text(ui(|u| u.log_count), &format!("{n_log} lines"));
    }
    if rss_changed || first {
        send(rss, LVM_SETITEMCOUNT, n_rss, LVSICF_NOSCROLL as isize);
        unsafe {
            let _ = InvalidateRect(Some(rss), None, false);
        }
        form::set_text(ui(|u| u.rss_count), &format!("{n_rss} items"));
    }
    for (i, t) in status.iter().enumerate() {
        set_status(sb, i, t);
    }
    unsafe {
        // The sparkline pane.
        let mut r = RECT::default();
        send(sb, SB_GETRECT, 2, &mut r as *mut _ as isize);
        let _ = InvalidateRect(Some(sb), Some(&r), false);
    }
    let armed = ui(|u| u.countdown.armed);
    send(tb, TB_CHECKBUTTON, ID_CONNECT as usize, flags.0 as isize);
    send(tb, TB_CHECKBUTTON, ID_PAUSE as usize, flags.1 as isize);
    send(tb, TB_CHECKBUTTON, ID_SCHEDULER as usize, flags.2 as isize);
    send(tb, TB_CHECKBUTTON, ID_SHUTDOWN as usize, armed as isize);
    let timer = ui(|u| u.rss_buttons.last().copied()).unwrap_or_default();
    if form::checked(timer) != flags.3 {
        form::set_check(timer, flags.3);
    }
}

fn setting_in(snap: &Snapshot, key: &str) -> bool {
    snap.settings.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn filter_lines(snap: &Snapshot, filter: &str) -> Vec<usize> {
    let f = filter.trim().to_lowercase();
    (0..snap.log_tail.len())
        .filter(|i| {
            let l = &snap.log_tail[*i];
            f.is_empty() || l.message.to_lowercase().contains(&f) || l.level.to_lowercase().contains(&f)
        })
        .collect()
}

fn status_texts(snap: &Snapshot) -> [String; 9] {
    let speed_mode = snap.settings.get("speed_bytes_mode").and_then(|v| v.as_u64()).unwrap_or(3) as u8;
    let queue_mode = snap.settings.get("queue_bytes_mode").and_then(|v| v.as_u64()).unwrap_or(1) as u8;
    let limit = match snap.speed_limit_tag {
        Some(t) => format!("Limit {}%", t as u32 * 10),
        None => "No limit".into(),
    };
    let eta = if snap.speed_bps > 0.0 && snap.queue_remaining_bytes > 0.0 {
        format!("ETA {}", format_eta(snap.queue_remaining_bytes / snap.speed_bps))
    } else {
        "ETA -".into()
    };
    let state = if snap.scheduler_hold {
        "Scheduler hold".to_string()
    } else if snap.paused {
        "Paused".into()
    } else if snap.connected {
        format!("Connected ({})", snap.threads.iter().filter(|t| !t.server.is_empty()).count())
    } else {
        "Not connected".into()
    };
    let toast = ui(|u| u.toast.clone()).filter(|(_, at)| at.elapsed() < Duration::from_secs(4)).map(|t| t.0).unwrap_or_default();
    [
        format!("\t{limit}"),
        format!("\t{}", format_speed(snap.speed_bps, speed_mode)),
        String::new(),
        format!("\t{}", format_queue_bytes(snap.queue_remaining_bytes.max(0.0), queue_mode)),
        format!("\t{eta}"),
        format!("\t{}", format_queue_bytes(snap.total_bytes as f64, queue_mode)),
        format!("\tFree: {:.2} GB", snap.free_gb),
        format!("\t{state}"),
        toast,
    ]
}

fn set_status(sb: HWND, i: usize, t: &str) {
    if i == 2 {
        return;
    }
    thread_local! { static LAST: RefCell<[String; 9]> = RefCell::new(Default::default()); }
    let same = LAST.with(|l| l.borrow()[i] == t);
    if same {
        return;
    }
    LAST.with(|l| l.borrow_mut()[i] = t.to_string());
    let z = wz(t);
    send(sb, SB_SETTEXTW, i, z.as_ptr() as isize);
}

pub fn toast(t: impl Into<String>) {
    ui(|u| u.toast = Some((t.into(), Instant::now())));
    let sb = ui(|u| u.sb);
    let t = ui(|u| u.toast.clone().map(|t| t.0)).unwrap_or_default();
    set_status(sb, 8, &t);
}

/// Speed sparkline in the status bar.
unsafe fn draw_sparkline(dc: HDC, rc: RECT) {
    let hist = ui(|u| u.snap.stats.speed_history.clone());
    let inner = RECT { left: rc.left + 2, top: rc.top + 2, right: rc.right - 2, bottom: rc.bottom - 2 };
    FillRect(dc, &inner, GetSysColorBrush(COLOR_WINDOW));
    let n = hist.len().min(60);
    if n < 2 {
        return;
    }
    let data = &hist[hist.len() - n..];
    let max = data.iter().cloned().fold(1.0f32, f32::max);
    let pen = CreatePen(PS_SOLID, 1, rgb(46, 155, 62));
    let op = SelectObject(dc, pen.into());
    let (w, h) = (inner.right - inner.left - 1, inner.bottom - inner.top - 2);
    for (i, v) in data.iter().enumerate() {
        let x = inner.left + (i as i32 * w) / (n as i32 - 1);
        let y = inner.bottom - 1 - ((v / max) * h as f32) as i32;
        if i == 0 {
            let _ = MoveToEx(dc, x, y, None);
        } else {
            let _ = LineTo(dc, x, y);
        }
    }
    SelectObject(dc, op);
    let _ = DeleteObject(pen.into());
}

// ------------------------------------------------------------- messages ---

/// Keyboard shortcuts of the main window.
fn pre_translate(msg: &MSG) -> bool {
    if msg.message != WM_KEYDOWN {
        return false;
    }
    let main = ui(|u| u.main);
    unsafe {
        if GetAncestor(msg.hwnd, GA_ROOT) != main {
            return false;
        }
    }
    let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
    let key = VIRTUAL_KEY(msg.wParam.0 as u16);
    let id = match (key, ctrl) {
        (VK_F1, false) => ID_PAUSE,
        (VK_F2, false) => {
            select_tab(current_tab().saturating_sub(1));
            return true;
        }
        (VK_F3, false) => {
            select_tab((current_tab() + 1).min(3));
            return true;
        }
        (VK_F9, false) => {
            send_cmd(Command::MergeSettings(serde_json::json!({ "thread_list_visible": !setting_bool("thread_list_visible") })));
            return true;
        }
        (VK_P, true) => ID_HOLD,
        (VK_D, true) => ID_RESUME,
        (VK_UP, true) => ID_UP,
        (VK_DOWN, true) => ID_DOWN,
        (VK_E, true) => ID_EXPORT,
        (VK_DELETE, false) if msg.hwnd == ui(|u| u.queue) => ID_DELETE,
        _ => return false,
    };
    command(id);
    true
}

unsafe extern "system" fn main_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_SIZE => {
            if wp.0 == SIZE_MINIMIZED as usize && !ui(|u| u.demo) && setting_bool("minimize_to_tray") && tray::ready() {
                tray::hide_main_window(h.0 as isize);
                with_engine(|e| e.log("Main", "Minimized to tray."));
                return LRESULT(0);
            }
            layout(h);
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let mi = &mut *(lp.0 as *mut MINMAXINFO);
            mi.ptMinTrackSize = POINT { x: s(760), y: s(480) };
            LRESULT(0)
        }
        WM_TIMER => {
            if wp.0 == TIMER_SOON {
                let _ = KillTimer(Some(h), TIMER_SOON);
            }
            refresh();
            if wp.0 == TIMER_REFRESH {
                tick(h);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wp.0 & 0xffff) as i32;
            let code = ((wp.0 >> 16) & 0xffff) as u32;
            if code == EN_CHANGE {
                if matches!(id, ID_QFILTER | ID_LOG_FILTER | ID_RSS_FILTER) {
                    ui(|u| u.seq = 0);
                    refresh();
                }
                return LRESULT(0);
            }
            if lp.0 == 0 || code == BN_CLICKED {
                command(id);
            }
            LRESULT(0)
        }
        WM_NOTIFY => notify(h, lp),
        WM_DRAWITEM => {
            let d = &*(lp.0 as *const DRAWITEMSTRUCT);
            if d.CtlID == ID_STATUS as u32 {
                draw_sparkline(d.hDC, d.rcItem);
            }
            LRESULT(1)
        }
        WM_CONTEXTMENU => {
            if HWND(wp.0 as *mut c_void) == ui(|u| u.queue) {
                queue_menu(h, lp);
            }
            LRESULT(0)
        }
        WM_DROPFILES => {
            let drop = HDROP(wp.0 as *mut c_void);
            let n = DragQueryFileW(drop, u32::MAX, None);
            let paused = GetKeyState(VK_SHIFT.0 as i32) < 0;
            for i in 0..n {
                let mut buf = [0u16; 1024];
                let len = DragQueryFileW(drop, i, Some(&mut buf)) as usize;
                add_nzb(String::from_utf16_lossy(&buf[..len]), paused);
            }
            DragFinish(drop);
            LRESULT(0)
        }
        WM_DPICHANGED => {
            set_dpi(GetDpiForWindow(h));
            let r = &*(lp.0 as *const RECT);
            let _ = SetWindowPos(h, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
            apply_font(h);
            layout(h);
            LRESULT(0)
        }
        WM_CLOSE => {
            let exit = ui(|u| u.exit_now || u.demo) || tray::exit_requested();
            if tray::close_action(exit, setting_bool("minimize_to_tray_on_close"), tray::ready()) == tray::CloseAction::HideToTray {
                tray::hide_main_window(h.0 as isize);
                with_engine(|e| e.log("Main", "Minimized to tray on close."));
            } else {
                for d in form::modeless() {
                    let _ = DestroyWindow(d);
                }
                let _ = DestroyWindow(h);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

fn apply_font(h: HWND) {
    unsafe extern "system" fn each(c: HWND, _: LPARAM) -> windows::core::BOOL {
        send(c, WM_SETFONT, font().0 as usize, 1);
        TRUE
    }
    unsafe {
        let _ = EnumChildWindows(Some(h), Some(each), LPARAM(0));
    }
}

/// Pages: tab-pane background, forward to the main window, splitter drag.
unsafe extern "system" fn page_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let splitter_hit = |y: i32| -> bool {
        let conns = ui(|u| u.conns);
        if GetParent(conns).ok() != Some(h) || !IsWindowVisible(conns).as_bool() {
            return false;
        }
        let mut r = RECT::default();
        let _ = GetWindowRect(conns, &mut r);
        let mut p = POINT { x: r.left, y: r.top };
        let _ = ScreenToClient(h, &mut p);
        y >= p.y - s(6) && y < p.y
    };
    match m {
        WM_ERASEBKGND => {
            let dc = HDC(wp.0 as *mut c_void);
            let mut rc = RECT::default();
            let _ = GetClientRect(h, &mut rc);
            FillRect(dc, &rc, GetSysColorBrush(COLOR_BTNFACE));
            LRESULT(1)
        }
        WM_PRINTCLIENT => form::print_face(h, wp),
        WM_SETCURSOR => {
            let mut p = POINT::default();
            let _ = GetCursorPos(&mut p);
            let _ = ScreenToClient(h, &mut p);
            if splitter_hit(p.y) {
                SetCursor(LoadCursorW(None, IDC_SIZENS).ok());
                return LRESULT(1);
            }
            DefWindowProcW(h, m, wp, lp)
        }
        WM_LBUTTONDOWN => {
            let y = hi(lp.0) as i16 as i32;
            if splitter_hit(y) {
                SetCapture(h);
                ui(|u| u.splitter_drag = Some(y));
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if ui(|u| u.splitter_drag.is_some()) {
                let y = hi(lp.0) as i16 as i32;
                let mut rc = RECT::default();
                let _ = GetClientRect(h, &mut rc);
                let ht = ((rc.bottom - y) * 96 / DPI.load(std::sync::atomic::Ordering::Relaxed) as i32).clamp(60, 600);
                with_engine(|e| e.settings.thread_list_height = ht as f32);
                ui(|u| {
                    if let Some(map) = u.snap.settings.as_object_mut() {
                        map.insert("thread_list_height".into(), serde_json::json!(ht));
                    }
                });
                layout(ui(|u| u.main));
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if ui(|u| u.splitter_drag.take()).is_some() {
                let _ = ReleaseCapture();
                with_engine(|e| e.settings.save());
            }
            LRESULT(0)
        }
        WM_CTLCOLORSTATIC => {
            let dc = HDC(wp.0 as *mut c_void);
            SetBkMode(dc, TRANSPARENT);
            LRESULT(GetSysColorBrush(COLOR_BTNFACE).0 as isize)
        }
        WM_NOTIFY | WM_COMMAND | WM_DRAWITEM | WM_CONTEXTMENU => SendMessageW(GetParent(h).unwrap_or_default(), m, Some(wp), Some(lp)),
        _ => DefWindowProcW(h, m, wp, lp),
    }
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

unsafe fn notify(_h: HWND, lp: LPARAM) -> LRESULT {
    let nm = &*(lp.0 as *const NMHDR);
    let (queue, conns, log, par2, rss, tab, sb) = ui(|u| (u.queue, u.conns, u.log, u.par2, u.rss, u.tab, u.sb));
    let from = nm.hwndFrom;
    if nm.code == LVN_GETDISPINFOW {
        let di = &mut *(lp.0 as *mut NMLVDISPINFOW);
        let it = &mut di.item;
        let (row, sub) = (it.iItem as usize, it.iSubItem as usize);
        if from == queue {
            let (text, image, indent) = ui(|u| {
                let Some(r) = u.rows.get(row).copied() else { return (String::new(), 0, 0) };
                let col = u.cols.get(sub).copied().unwrap_or(0);
                let text = if col == queue::COL_STATUS { String::new() } else { queue::cell(&u.snap.queue, r, col) };
                let image = match r {
                    Row::Coll(i) => {
                        if u.snap.queue[i].expanded {
                            1
                        } else {
                            0
                        }
                    }
                    Row::File(..) => 2,
                };
                (text, image, matches!(r, Row::File(..)) as i32)
            });
            if it.mask.0 & LVIF_TEXT.0 != 0 {
                put(it.pszText, it.cchTextMax, &text);
            }
            if it.mask.0 & LVIF_IMAGE.0 != 0 {
                it.iImage = if sub == 0 { image } else { -1 };
            }
            if it.mask.0 & LVIF_INDENT.0 != 0 {
                it.iIndent = indent;
            }
            return LRESULT(0);
        }
        if it.mask.0 & LVIF_TEXT.0 != 0 {
            let text = ui(|u| {
                if from == conns {
                    u.snap.threads.get(row).map(|t| match sub {
                        0 => t.number.to_string(),
                        1 => t.action.clone(),
                        2 => t.description.clone(),
                        3 => t.status.clone(),
                        _ => t.server.clone(),
                    })
                } else if from == log {
                    u.log_rows.get(row).and_then(|i| u.snap.log_tail.get(*i)).map(|l| match sub {
                        0 => l.time.clone(),
                        1 => l.level.clone(),
                        _ => l.message.clone(),
                    })
                } else if from == par2 {
                    u.snap.par2.get(row).map(|p| match sub {
                        0 => p.name.clone(),
                        1 => format!("{:.1} MB", p.size_mb),
                        2 => p.blocks.to_string(),
                        3 => p.info.clone(),
                        4 => if p.unrar { "Yes" } else { "No" }.into(),
                        _ => {
                            if p.progress > 0.0 && p.progress < 1.0 {
                                format!("{} ({:.0}%)", p.status, p.progress * 100.0)
                            } else {
                                p.status.clone()
                            }
                        }
                    })
                } else if from == rss {
                    u.rss_rows.get(row).and_then(|i| u.snap.rss.get(*i)).map(|r| match sub {
                        0 => r.name.clone(),
                        1 => r.date.clone(),
                        _ => r.description.clone(),
                    })
                } else {
                    None
                }
            });
            put(it.pszText, it.cchTextMax, text.as_deref().unwrap_or(""));
        }
        return LRESULT(0);
    }
    if nm.code == TCN_SELCHANGE && from == tab {
        select_tab(current_tab());
        return LRESULT(0);
    }
    if nm.code == NM_CUSTOMDRAW && from == queue {
        return queue_draw(lp, queue);
    }
    if nm.code == NM_CUSTOMDRAW && from == log {
        return log_draw(lp);
    }
    if from == queue {
        match nm.code {
            LVN_ITEMCHANGED => {
                let n = &*(lp.0 as *const NMLISTVIEW);
                if n.uChanged.0 & LVIF_STATE.0 != 0 && n.uNewState & LVIS_SELECTED.0 != 0 {
                    let sel = ui(|u| u.rows.get(n.iItem as usize).map(|r| match *r {
                        Row::Coll(i) => (u.snap.queue[i].id, None),
                        Row::File(i, j) => (u.snap.queue[i].id, Some(j)),
                    }));
                    ui(|u| u.sel = sel);
                }
            }
            NM_CLICK => {
                let a = &*(lp.0 as *const NMITEMACTIVATE);
                let mut ht = LVHITTESTINFO { pt: a.ptAction, ..Default::default() };
                send(queue, LVM_SUBITEMHITTEST, 0, &mut ht as *mut _ as isize);
                if ht.iItem >= 0 && ht.iSubItem == 0 && ht.flags.0 & LVHT_ONITEMICON.0 != 0 {
                    toggle_row(ht.iItem as usize);
                }
            }
            NM_DBLCLK => {
                let a = &*(lp.0 as *const NMITEMACTIVATE);
                if a.iItem >= 0 {
                    let is_coll = ui(|u| matches!(u.rows.get(a.iItem as usize), Some(Row::Coll(_))));
                    if is_coll && setting_bool("dblclick_expand") {
                        toggle_row(a.iItem as usize);
                    } else {
                        command(ID_OPEN_FOLDER);
                    }
                }
            }
            _ => {}
        }
    }
    if from == par2 && nm.code == NM_DBLCLK {
        command(ID_P2_START);
    }
    if from == sb && nm.code == NM_CLICK {
        let m = &*(lp.0 as *const NMMOUSE);
        if m.dwItemSpec == 0 || m.dwItemSpec == 1 {
            dialogs::speed_limit(ui(|u| u.main));
        }
    }
    LRESULT(0)
}

fn toggle_row(row: usize) {
    let target = ui(|u| match u.rows.get(row) {
        Some(Row::Coll(i)) => Some((u.snap.queue[*i].id, !u.snap.queue[*i].expanded)),
        _ => None,
    });
    if let Some((id, v)) = target {
        send_cmd(Command::SetExpanded(id, v));
    }
}

/// Status column: a themed progress bar with the status text.
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
    let sub = cd.iSubItem as usize;
    let info = ui(|u| {
        let r = u.rows.get(row).copied()?;
        let col = u.cols.get(sub).copied()?;
        let (p, kind) = queue::bar(&u.snap.queue, r);
        let text = queue::cell(&u.snap.queue, r, queue::COL_STATUS);
        let active = matches!(r, Row::Coll(i) if u.snap.queue[i].status == Some(ItemStatus::Downloading));
        Some((r, col, p, kind, text, queue::finished(&u.snap.queue, r), active))
    });
    let Some((r, col, pct, kind, text, done, active)) = info else { return LRESULT(0) };
    if col != queue::COL_STATUS {
        cd.clrText = if done { rgb(120, 120, 120) } else { COLORREF(GetSysColor(COLOR_WINDOWTEXT)) };
        if active && col == queue::COL_SUBJECT {
            SelectObject(cd.nmcd.hdc, bold().into());
            return LRESULT((CDRF_NEWFONT) as isize);
        }
        SelectObject(cd.nmcd.hdc, font().into());
        return LRESULT(CDRF_NEWFONT as isize);
    }
    let dc = cd.nmcd.hdc;
    let mut rc = RECT { left: LVIR_BOUNDS as i32, top: sub as i32, right: 0, bottom: 0 };
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
    let inset = if matches!(r, Row::File(..)) { 4 } else { 2 };
    let bar = RECT { left: rc.left + s(3), top: rc.top + s(inset), right: rc.right - s(3), bottom: rc.bottom - s(inset) };
    let fill_right = bar.left + 1 + (((bar.right - bar.left - 2) as f32) * pct) as i32;
    let th = OpenThemeData(Some(queue), w!("PROGRESS"));
    if th.0 != 0 {
        let _ = DrawThemeBackground(th, dc, 1 /* PP_BAR */, 0, &bar, None);
        let inner = RECT { left: bar.left + 1, top: bar.top + 1, right: fill_right, bottom: bar.bottom - 1 };
        if inner.right > inner.left {
            if kind == Bar::Done {
                let g = CreateSolidBrush(rgb(176, 180, 186));
                FillRect(dc, &inner, g);
                let _ = DeleteObject(g.into());
            } else {
                let state = match kind {
                    Bar::Paused => 3,
                    Bar::Error => 2,
                    _ => 1,
                };
                let _ = DrawThemeBackground(th, dc, 5 /* PP_FILL */, state, &inner, None);
            }
        }
        let _ = CloseThemeData(th);
    }
    // The text over the fill is white, the rest black.
    SetBkMode(dc, TRANSPARENT);
    let old = SelectObject(dc, font().into());
    let mut t = wz(&text);
    t.pop();
    let fill_col = if kind == Bar::Done { rgb(0, 0, 0) } else { rgb(255, 255, 255) };
    for (l, rr, col) in [(bar.left, fill_right, fill_col), (fill_right, bar.right, rgb(0, 0, 0))] {
        if rr <= l {
            continue;
        }
        let saved = SaveDC(dc);
        IntersectClipRect(dc, l, bar.top, rr, bar.bottom);
        SetTextColor(dc, col);
        let mut tr = bar;
        DrawTextW(dc, &mut t, &mut tr, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
        let _ = RestoreDC(dc, saved);
    }
    SelectObject(dc, old);
    LRESULT(CDRF_SKIPDEFAULT as isize)
}

/// Log lines coloured by area: errors red, warnings orange.
unsafe fn log_draw(lp: LPARAM) -> LRESULT {
    let cd = &mut *(lp.0 as *mut NMLVCUSTOMDRAW);
    match cd.nmcd.dwDrawStage {
        CDDS_PREPAINT => LRESULT(CDRF_NOTIFYITEMDRAW as isize),
        CDDS_ITEMPREPAINT => {
            let level = ui(|u| u.log_rows.get(cd.nmcd.dwItemSpec).and_then(|i| u.snap.log_tail.get(*i)).map(|l| l.level.clone())).unwrap_or_default();
            cd.clrText = match level.as_str() {
                "Error" => rgb(0xC0, 0x20, 0x20),
                "Warning" | "Shutdown" => rgb(0xB0, 0x60, 0x00),
                "Par2" | "Unrar" => rgb(0x20, 0x60, 0xA0),
                _ => COLORREF(GetSysColor(COLOR_WINDOWTEXT)),
            };
            LRESULT(CDRF_NEWFONT as isize)
        }
        _ => LRESULT(0),
    }
}

fn queue_menu(h: HWND, lp: LPARAM) {
    let Some((id, _)) = ui(|u| u.sel) else { return };
    let c = ui(|u| u.snap.queue.iter().find(|c| c.id == id).cloned());
    let Some(c) = c else { return };
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let paused = c.status == Some(ItemStatus::Paused);
        let items: Vec<(i32, &str, bool)> = vec![
            (ID_RESUME, "Resume\tCtrl+D", paused),
            (ID_HOLD, "Pause\tCtrl+P", !paused),
            (0, "", true),
            (ID_TOP, "Move to top", true),
            (ID_UP, "Move up\tCtrl+Up", true),
            (ID_DOWN, "Move down\tCtrl+Down", true),
            (ID_BOTTOM, "Move to bottom", true),
            (0, "", true),
            (ID_EXPAND_ALL, "Expand all", true),
            (ID_COLLAPSE_ALL, "Collapse all", true),
            (0, "", true),
            (ID_UNPACK, "Unpack now", true),
            (ID_CHANGE_FOLDER, "Change download folder...", true),
            (ID_UNRAR_FOLDER, "Change unrar folder...", true),
            (ID_PASSWORD, "Archive password...", true),
            (ID_EXECUTE, "Execute command...", true),
            (ID_RENAME, "Rename...", true),
            (ID_EXPORT, "Export NZB...\tCtrl+E", true),
            (ID_OPEN_FOLDER, "Open download folder", true),
            (ID_ETA, "Estimated time left", true),
            (0, "", true),
            (ID_RESUME_ALL, "Resume all", true),
            (ID_PAUSE_ALL, "Pause all", true),
            (ID_COLUMNS, "Columns...", true),
            (0, "", true),
            (ID_DELETE, "Delete\tDel", true),
        ];
        for (id, t, enabled) in items {
            if id == 0 {
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            } else {
                let z = wz(t);
                let flags = if enabled { MF_STRING } else { MF_STRING | MF_GRAYED };
                let _ = AppendMenuW(menu, flags, id as usize, PCWSTR(z.as_ptr()));
            }
        }
        // Unrar as a check item.
        let z = wz("Unpack after download");
        let _ = InsertMenuW(menu, ID_UNPACK as u32, MF_BYCOMMAND | MF_STRING | if c.unrar { MF_CHECKED } else { MF_UNCHECKED }, if c.unrar { ID_UNRAR_OFF } else { ID_UNRAR_ON } as usize, PCWSTR(z.as_ptr()));
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

// ------------------------------------------------------------- commands ---

pub fn add_nzb(path: String, paused: bool) {
    if path.to_ascii_lowercase().ends_with(".nzb") {
        send_cmd(Command::AddNzbFile(path, paused));
    }
}

fn selected() -> Option<newsqueue_core::snapshot::QueueRow> {
    ui(|u| u.sel.and_then(|(id, _)| u.snap.queue.iter().find(|c| c.id == id).cloned()))
}

fn open_path(p: &str) {
    let _ = std::fs::create_dir_all(p);
    let _ = std::process::Command::new("explorer").arg(p).spawn();
}

fn command(id: i32) {
    let main = ui(|u| u.main);
    let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
    match id {
        ID_CONNECT => send_cmd(if ui(|u| u.snap.connected) { Command::Disconnect } else { Command::Connect }),
        ID_PAUSE => send_cmd(Command::TogglePause),
        ID_PAUSEFOR => dialogs::pause_for(main),
        ID_NZB => {
            for p in pick::open_files(main, "Import NZB files", &[("NZB files", "*.nzb"), ("All files", "*.*")], true) {
                add_nzb(p, shift);
            }
        }
        ID_DOWNLOAD => {
            let key = if shift { "unrar_dir" } else { "download_dir" };
            open_path(setting(key).as_str().unwrap_or(""));
        }
        ID_CLEANUP => send_cmd(Command::Cleanup),
        ID_SCHEDULER => {
            let on = !setting_bool("scheduler_enabled");
            send_cmd(Command::MergeSettings(serde_json::json!({ "scheduler_enabled": on })));
        }
        ID_SHUTDOWN => dialogs::shutdown(main),
        ID_SETUP => setup::open(main, 0),
        ID_HOME => {
            let _ = std::process::Command::new("explorer").arg(HOME_URL).spawn();
        }
        ID_VERSION => dialogs::version(main),
        ID_ABOUT => dialogs::about(main),
        ID_NOTES => dialogs::notes(main),
        ID_STATS => dialogs::stats(main),
        ID_TRAY => {
            if tray::ready() {
                tray::hide_main_window(main.0 as isize);
                with_engine(|e| e.log("Main", "Minimized to tray."));
            } else {
                toast("The tray icon is not ready yet");
            }
        }
        ID_COLLAPSE_ALL => send_cmd(Command::SetAllExpanded(false)),
        ID_EXPAND_ALL => send_cmd(Command::SetAllExpanded(true)),
        ID_RESUME_ALL => send_cmd(Command::PauseAll(false)),
        ID_PAUSE_ALL => send_cmd(Command::PauseAll(true)),
        ID_COLUMNS => dialogs::columns(main),
        ID_P2_START | ID_P2_REMOVE | ID_P2_UP | ID_P2_DOWN => {
            let lv = ui(|u| u.par2);
            let Some(i) = form::lv_selected(lv) else { return };
            match id {
                ID_P2_START => send_cmd(Command::StartPar2Set(i)),
                ID_P2_REMOVE => with_engine(|e| {
                    if i < e.par2.len() {
                        let p = e.par2.remove(i);
                        e.log("Par2", &format!("Removed {} from the PAR2 list.", p.name));
                    }
                }),
                _ => {
                    let j = if id == ID_P2_UP { i.checked_sub(1) } else { Some(i + 1) };
                    let moved = with_engine(|e| match j {
                        Some(j) if j < e.par2.len() => {
                            e.par2.swap(i, j);
                            Some(j)
                        }
                        _ => None,
                    });
                    if let Some(j) = moved {
                        form::lv_select(lv, j);
                    }
                }
            }
            refresh_soon();
        }
        ID_P2_IMPORT => {
            for p in pick::open_files(main, "Import PAR2 files", &[("PAR2 files", "*.par2;*.PAR2")], true) {
                let path = std::path::PathBuf::from(&p);
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let size_mb = std::fs::metadata(&path).map(|m| m.len() as f64 / MB).unwrap_or(0.0);
                with_engine(|e| {
                    e.par2.push(Par2Set {
                        name,
                        size_mb,
                        blocks: 0,
                        info: "Imported".into(),
                        unrar: false,
                        progress: 0.0,
                        status: "Queued".into(),
                        folder: path.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default(),
                        collection_id: None,
                    })
                });
            }
            refresh_soon();
        }
        ID_LOG_CLEAR => {
            with_engine(|e| e.logs.clear());
            refresh_soon();
        }
        ID_LOG_COPY => {
            let text = ui(|u| u.log_rows.iter().filter_map(|i| u.snap.log_tail.get(*i)).map(|l| format!("{}\t{}\t{}", l.time, l.level, l.message)).collect::<Vec<_>>().join("\r\n"));
            copy_text(main, &text);
            toast("Log copied to the clipboard");
        }
        ID_RSS_REFRESH => send_cmd(Command::RefreshRss),
        ID_RSS_FEEDS => dialogs::rss_feeds(main),
        ID_RSS_HELP => dialogs::filter_help(main),
        ID_RSS_TIMER => {
            let b = ui(|u| u.rss_buttons.last().copied()).unwrap_or_default();
            send_cmd(Command::SetRssTimer(form::checked(b)));
        }
        _ => item_command(id, main),
    }
}

/// Commands on the selected collection.
fn item_command(id: i32, main: HWND) {
    let Some(c) = selected() else {
        if (ID_RESUME..=ID_EXECUTE).contains(&id) {
            toast("Select a collection first");
        }
        return;
    };
    let cmd = match id {
        ID_RESUME => Command::Pause(c.id, false),
        ID_HOLD => Command::Pause(c.id, true),
        ID_TOP => Command::Move(c.id, MoveDir::Top),
        ID_UP => Command::Move(c.id, MoveDir::Up),
        ID_DOWN => Command::Move(c.id, MoveDir::Down),
        ID_BOTTOM => Command::Move(c.id, MoveDir::Bottom),
        ID_UNRAR_ON => Command::SetUnrar(c.id, true),
        ID_UNRAR_OFF => Command::SetUnrar(c.id, false),
        ID_UNPACK => Command::Unpack(c.id),
        ID_DELETE => {
            if !form::confirm(Some(main), "Delete", &format!("Remove {} from the queue?", c.subject)) {
                return;
            }
            Command::Delete(c.id)
        }
        ID_CHANGE_FOLDER => match pick::folder(main, "Download folder", &c.folder) {
            Some(f) => Command::SetFolder(c.id, f),
            None => return,
        },
        ID_UNRAR_FOLDER => {
            let start = if c.unrar_folder.is_empty() { setting("unrar_dir").as_str().unwrap_or("").to_string() } else { c.unrar_folder.clone() };
            match pick::folder(main, "Unrar folder", &start) {
                Some(f) => Command::SetUnrarFolder(c.id, f),
                None => return,
            }
        }
        ID_PASSWORD => match dialogs::password(main, &c.subject, false) {
            Some(p) if !p.is_empty() => Command::SubmitPassword(c.id, p),
            _ => return,
        },
        ID_EXECUTE => match dialogs::execute(main, &c.execute) {
            Some(x) => Command::SetExecute(c.id, x),
            None => return,
        },
        ID_RENAME => match dialogs::rename(main, &c.subject) {
            Some(x) if !x.trim().is_empty() => Command::Rename(c.id, x),
            _ => return,
        },
        ID_EXPORT => {
            export_nzb(main, c.id);
            return;
        }
        ID_OPEN_FOLDER => {
            open_path(&c.folder);
            return;
        }
        ID_ETA => {
            let speed = ui(|u| u.snap.speed_bps);
            let eta = if speed > 0.0 { format_eta(c.remaining_bytes / speed) } else { "Unknown".into() };
            toast(format!("{}: ETA {eta}", c.subject));
            return;
        }
        _ => return,
    };
    send_cmd(cmd);
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn export_nzb(main: HWND, id: u64) {
    let Some(c) = with_engine(|e| e.queue.iter().find(|c| c.id == id).cloned()) else { return };
    let name = format!("{}.nzb", sanitize_folder(&c.subject));
    let Some(path) = pick::save_file(main, "Export NZB", &name, &[("NZB files", "*.nzb")], "nzb") else { return };
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<nzb xmlns=\"http://www.newzbin.com/DTD/2003/nzb\">\n");
    for f in &c.files {
        xml.push_str(&format!(
            "  <file subject=\"{}\" poster=\"{}\" date=\"0\">\n    <groups><group>{}</group></groups>\n    <segments>\n",
            xml_escape(&f.name),
            xml_escape(&c.poster),
            xml_escape(&c.group)
        ));
        for s in &f.segments {
            xml.push_str(&format!("      <segment bytes=\"{}\" number=\"{}\">{}</segment>\n", s.bytes, s.number, xml_escape(&s.id)));
        }
        xml.push_str("    </segments>\n  </file>\n");
    }
    xml.push_str("</nzb>\n");
    match std::fs::write(&path, xml) {
        Ok(()) => {
            with_engine(|e| e.log("Main", &format!("Exported NZB to {path}")));
            toast("NZB exported");
        }
        Err(e) => form::message(Some(main), "Export NZB", &format!("Cannot write {path}:\n{e}"), MB_ICONERROR),
    }
}

fn copy_text(owner: HWND, text: &str) {
    use windows::Win32::System::DataExchange::*;
    use windows::Win32::System::Memory::*;
    let z: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        if OpenClipboard(Some(owner)).is_err() {
            return;
        }
        let _ = EmptyClipboard();
        if let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE, z.len() * 2) {
            let p = GlobalLock(mem) as *mut u16;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(z.as_ptr(), p, z.len());
                let _ = GlobalUnlock(mem);
                let _ = SetClipboardData(13 /* CF_UNICODETEXT */, Some(HANDLE(mem.0)));
            }
        }
        let _ = CloseClipboard();
    }
}

// --------------------------------------------------------- timer duties ---

/// Engine events, the updater and the shutdown countdown (every 250 ms).
fn tick(h: HWND) {
    if ui(|u| u.demo) {
        return;
    }
    dialogs::poll_update();
    tick_shutdown(h);
    if ui(|u| u.busy) {
        return;
    }
    let events = with_engine(|e| e.take_events());
    for ev in events {
        match ev {
            Event::Toast(t) => toast(t),
            Event::Notify(title, text) => tray::notify(&title, &text),
            Event::Message(title, text) => {
                ui(|u| u.busy = true);
                form::message(Some(h), &title, &text, MB_ICONINFORMATION);
                ui(|u| u.busy = false);
            }
            Event::PasswordNeeded(id) => {
                let name = ui(|u| u.snap.queue.iter().find(|r| r.id == id).map(|r| r.subject.clone())).unwrap_or_default();
                ui(|u| u.busy = true);
                let answer = dialogs::password(h, &name, true);
                ui(|u| u.busy = false);
                match answer {
                    Some(p) if !p.is_empty() => send_cmd(Command::SubmitPassword(id, p)),
                    _ => {
                        send_cmd(Command::CancelPassword(id));
                        form::message(Some(h), "Unrar password", &format!("{name}\n\nUnpack cancelled because no password was supplied."), MB_ICONWARNING);
                    }
                }
            }
            Event::RssFeedsNeeded => {
                ui(|u| u.busy = true);
                dialogs::rss_feeds(h);
                ui(|u| u.busy = false);
            }
        }
    }
}

fn tick_shutdown(h: HWND) {
    let all_done = ui(|u| u.snap.all_done);
    let method = setting("shutdown_method").as_str().unwrap_or("Exit").to_string();
    let step = ui(|u| u.countdown.tick(Instant::now(), all_done, &method));
    match step {
        shutdown::CountdownStep::Nothing => {}
        shutdown::CountdownStep::Started => {
            with_engine(|e| e.log("Shutdown", "All tasks finished. Shutdown scheduled."));
            if !unsafe { IsWindowVisible(h) }.as_bool() {
                tray::notify("Shutdown", &format!("{method} in {} seconds", shutdown::COUNTDOWN_SECS));
            }
        }
        shutdown::CountdownStep::Cancelled => with_engine(|e| e.log("Shutdown", "Countdown cancelled: unfinished tasks remain.")),
        shutdown::CountdownStep::Fire(action) => {
            let mode = ui(|u| u.shutdown_mode);
            with_engine(|e| {
                e.log("Shutdown", &format!("All tasks finished. Running {}.", action.label()));
                if mode == shutdown::ShutdownMode::Real {
                    e.save_all();
                }
            });
            match shutdown::execute(action, mode) {
                Ok(shutdown::ShutdownOutcome::Executed(d)) => with_engine(|e| e.log("Shutdown", &format!("{}: {d}", action.label()))),
                Ok(shutdown::ShutdownOutcome::Mocked(d)) => with_engine(|e| e.log("Shutdown", &format!("{} simulated: {d}", action.label()))),
                Ok(shutdown::ShutdownOutcome::ExitApplication) => {
                    with_engine(|e| e.log("Shutdown", "Exiting NewsQueue."));
                    request_exit();
                }
                Err(err) => {
                    with_engine(|e| e.log("Error", &format!("Shutdown failed: {err}")));
                    form::message(Some(h), "Shutdown", &err, MB_ICONERROR);
                }
            }
        }
    }
}

/// Closes the app for real (saves on the way out), even from the tray.
pub fn request_exit() {
    let main = ui(|u| {
        u.exit_now = true;
        u.main
    });
    unsafe {
        let _ = PostMessageW(Some(main), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}
