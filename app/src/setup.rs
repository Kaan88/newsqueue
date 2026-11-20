//! Setup: a modeless dialog with the 15 pages in the original order. Plain
//! fields are bound to `Settings` keys through the `FIELDS` table (load from
//! and save to the settings JSON); the server list and the scheduler grid
//! are the two hand-made pages. OK/Apply send `Command::ReplaceSettings`.

use crate::form::{self, Ev, IDCANCEL_, IDOK_};
use crate::util::*;
use crate::{pick, send_cmd, with_engine};
use newsqueue_core::command::Command;
use newsqueue_core::model::*;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;

pub const PAGES: [&str; 15] = [
    "Download",
    "Download #2",
    "NZB",
    "NZB #2",
    "Servers",
    "Log",
    "Connection",
    "Proxy",
    "Scheduler",
    "Par2",
    "Unpack",
    "Miscellaneous",
    "Miscellaneous #2",
    "Miscellaneous #3",
    "User interface",
];

#[derive(Clone, Copy)]
enum K {
    /// Checkbox; the label is its text.
    Check,
    Text,
    Password,
    Dir,
    /// An .exe path with a file picker.
    Exe,
    /// Integer with an up-down: min, max, unit.
    Num(i32, i32, &'static str),
    /// Float edit with a unit.
    Float(&'static str),
    /// Combo storing the item index.
    Index(&'static [&'static str]),
    /// Combo storing a string: (label, value).
    Str(&'static [(&'static str, &'static str)]),
    /// Startup speed limit: null or 1-9.
    SpeedTag,
    /// Gray explanatory text.
    Note,
}

struct F {
    page: usize,
    key: &'static str,
    label: &'static str,
    kind: K,
}

const fn f(page: usize, key: &'static str, label: &'static str, kind: K) -> F {
    F { page, key, label, kind }
}

const SPEED_MODES: [&str; 8] = ["KiB/s", "MiB/s", "kbit/s", "Mbit/s", "kB/s", "MB/s", "Automatic (bits)", "Automatic (bytes)"];
const SIZE_MODES: [&str; 5] = ["kB", "MB", "GB", "TB", "Automatic"];

static FIELDS: &[F] = &[
    // Download
    f(0, "download_dir", "Default download folder", K::Dir),
    f(0, "temp_dir", "Temporary folder", K::Dir),
    f(0, "unrar_dir", "Unrar folder", K::Dir),
    f(0, "low_free_space_mb", "Pause when free space drops below", K::Num(0, 1_000_000, "MB")),
    f(0, "autosave_minutes", "Autosave the download queue every", K::Num(1, 999, "min")),
    f(0, "skip_existing", "Skip files that are already downloaded", K::Check),
    f(0, "dblclick_expand", "Double-click expands or collapses a collection", K::Check),
    f(0, "", "Use the Columns button at the bottom of this window to edit the queue columns.", K::Note),
    // Download #2
    f(1, "retry_count", "Retries per article", K::Num(0, 999, "")),
    f(1, "retry_delay", "Delay between retries", K::Num(0, 1440, "s")),
    f(1, "duplicate_check", "NZB already in the queue or history", K::Index(&["Add normally", "Warn", "Add paused"])),
    f(1, "precheck_articles", "Check all articles (STAT) before downloading", K::Check),
    f(1, "abort_hopeless", "Abort a download that PAR2 cannot repair", K::Check),
    // NZB
    f(2, "nzb_dir", "Default NZB folder", K::Dir),
    f(2, "auto_import_folder", "Auto-import NZB files from", K::Dir),
    f(2, "auto_import_paused", "Add auto-imported NZB files paused", K::Check),
    f(2, "delete_nzb_after_import", "Delete the NZB file after importing it", K::Check),
    f(2, "", "Leave the auto-import folder empty to turn auto-import off.", K::Note),
    // NZB #2
    f(3, "case_insensitive_filters", "Filters and patterns ignore case", K::Check),
    f(3, "default_execute", "Default execute command", K::Text),
    f(3, "hide_execute_window", "Hide the window of the execute command", K::Check),
    f(3, "", "Execute commands run after download, PAR2 and unpack. Placeholders: {folder}, {name}, {file}.", K::Note),
    // Log
    f(5, "log_enabled", "Keep a log", K::Check),
    f(5, "log_to_file", "Write the log to files in the log folder", K::Check),
    f(5, "detailed_log", "Detailed log", K::Check),
    f(5, "keep_log_days", "Keep log files for", K::Num(1, 365, "days")),
    f(5, "max_log_lines", "Lines in the Log tab", K::Num(50, 100_000, "lines")),
    f(5, "clear_history_on_exit", "Clear the download history on exit", K::Check),
    // Connection
    f(6, "max_speed_kbps", "Line speed (0 = measure)", K::Float("KB/s")),
    f(6, "startup_speed_limit", "Speed limit at startup", K::SpeedTag),
    f(6, "startup_connect", "Connect to the servers on startup", K::Check),
    f(6, "", "The speed limit steps (10-90 %) are fractions of the line speed. Servers, connections and SSL are set on the Servers page.", K::Note),
    // Proxy
    f(7, "proxy_enabled", "Connect through a proxy", K::Check),
    f(7, "proxy_kind", "Proxy type", K::Str(&[("HTTP CONNECT", "http"), ("SOCKS5", "socks5")])),
    f(7, "proxy_address", "Proxy address", K::Text),
    f(7, "proxy_port", "Proxy port", K::Num(1, 65535, "")),
    f(7, "proxy_auth", "The proxy requires authentication", K::Check),
    f(7, "proxy_username", "Username", K::Text),
    f(7, "proxy_password", "Password", K::Password),
    // Scheduler (the grid is drawn by hand)
    f(8, "scheduler_enabled", "Enable the scheduler", K::Check),
    // Par2
    f(9, "par2_checking", "Check downloads with PAR2", K::Check),
    f(9, "par2_repairing", "Repair damaged downloads", K::Check),
    f(9, "cleanup_after_repair", "Delete PAR2 files after a successful repair", K::Check),
    f(9, "par2_on_demand", "Download recovery volumes only when a repair needs them", K::Check),
    f(9, "par2_path", "par2.exe (empty = bundled)", K::Exe),
    // Unpack
    f(10, "auto_unrar", "Unpack archives after download", K::Check),
    f(10, "direct_unpack", "Unpack while the rest of the set downloads", K::Check),
    f(10, "overwrite_existing", "Overwrite existing files", K::Check),
    f(10, "rename_existing", "Rename existing files instead", K::Check),
    f(10, "delete_archives", "Delete archives after a clean unpack", K::Check),
    f(10, "strip_root", "Strip a single root folder from the archive", K::Check),
    f(10, "auto_rename_to_collection", "Rename the output after the collection", K::Check),
    f(10, "delete_empty_folder", "Delete the download folder when it is empty", K::Check),
    f(10, "pause_while_unraring", "Pause downloading while unpacking", K::Check),
    f(10, "unrar_to_different", "Unpack to the unrar folder", K::Check),
    f(10, "create_dir_archive_name", "Subfolder named after the archive", K::Check),
    f(10, "create_dir_group_name", "Subfolder named after the group", K::Check),
    f(10, "create_dir_nzb_name", "Subfolder named after the NZB", K::Check),
    f(10, "move_files_to_unrar", "Move matching files to the unrar folder", K::Text),
    f(10, "delete_files_after_unrar", "Delete matching files after unpack", K::Text),
    f(10, "unrar_path", "UnRAR.exe (empty = bundled)", K::Exe),
    f(10, "sevenzip_path", "7z.exe (empty = search)", K::Exe),
    // Miscellaneous
    f(11, "minimize_to_tray", "Minimize to the tray", K::Check),
    f(11, "minimize_to_tray_on_close", "Closing the window minimizes to the tray", K::Check),
    f(11, "tray_balloon", "Show tray balloon hints", K::Check),
    f(11, "speed_in_tray", "Show the speed in the tray tooltip", K::Check),
    f(11, "check_version_minutes", "Version check every (0 = never)", K::Num(0, 100_000, "min")),
    f(11, "auto_update", "Install new versions automatically when idle", K::Check),
    // Miscellaneous #2
    f(12, "article_cache_mb", "Article cache size", K::Num(0, 500, "MB")),
    f(12, "cache_cleanup_minutes", "Cache cleanup time", K::Num(1, 60, "min")),
    f(12, "download_priority", "Download thread priority", K::Num(0, 5, "")),
    f(12, "sort_folder", "Sort finished downloads into", K::Dir),
    f(12, "sort_pattern", "Folder pattern", K::Text),
    f(12, "", "Leave the sort folder empty to keep downloads where they are. Pattern tokens: {name} {title} {year} {date} {group}.", K::Note),
    // Miscellaneous #3
    f(13, "rss_refresh_minutes", "RSS refresh interval", K::Num(1, 1440, "min")),
    f(13, "notify_url", "Notification URL (webhook or ntfy)", K::Text),
    f(13, "shutdown_method", "Shutdown method", K::Str(&[("Shutdown Windows", "Shutdown Windows"), ("Hibernate", "Hibernate"), ("Sleep", "Sleep"), ("Exit", "Exit")])),
    f(13, "", "Finished and failed jobs are posted to the notification URL: ntfy topics get plain text, anything else JSON {title, text}.", K::Note),
    f(13, "web_access", "Web access: control NewsQueue from a browser or phone", K::Check),
    f(13, "web_access_bind", "Listen on (address:port)", K::Text),
    f(13, "web_access_key", "Web access password", K::Password),
    f(13, "", "Open http://<this PC>:8480 in a browser. The password is required unless you listen on 127.0.0.1. No TLS: use a VPN or reverse proxy outside your home network. Turning web access off takes effect after a restart.", K::Note),
    // User interface
    f(14, "thread_list_visible", "Show the connections list [F9]", K::Check),
    f(14, "thread_list_height", "Height of the connections list", K::Num(60, 600, "px")),
    f(14, "speed_bytes_mode", "Speed unit", K::Index(&SPEED_MODES)),
    f(14, "queue_bytes_mode", "Queue size unit", K::Index(&SIZE_MODES)),
];

const ID_PAGES: i32 = 20;
const ID_HEAD: i32 = 21;
const ID_COLUMNS: i32 = 22;
const ID_APPLY: i32 = 23;
const ID_SRV_LIST: i32 = 40;
const ID_SRV_ADD: i32 = 41;
const ID_SRV_EDIT: i32 = 42;
const ID_SRV_DEL: i32 = 43;
const ID_SRV_UP: i32 = 44;
const ID_SRV_DOWN: i32 = 45;
const ID_GRID: i32 = 50;
const ID_GRID_ALL: i32 = 51;
const ID_GRID_NONE: i32 = 52;
const FIELD_ID: i32 = 1000;
const BROWSE_ID: i32 = 3000;

const PANEL_X: i32 = 186;
const PANEL_Y: i32 = 44;
const PANEL_W: i32 = 524;
const PANEL_H: i32 = 476;

thread_local! {
    /// The servers being edited and the scheduler grid of the open Setup.
    static DRAFT: RefCell<(Vec<Server>, [String; 7])> = RefCell::new((Vec::new(), Default::default()));
    static PANELS: RefCell<Vec<HWND>> = const { RefCell::new(Vec::new()) };
}

/// Builds Setup on `page` with the current settings (not shown).
pub fn build(owner: HWND, page: usize) -> HWND {
    let settings = with_engine(|e| e.settings.clone());
    let d = form::create(Some(owner), "Setup", 720, 576, false);
    form::listbox(d, ID_PAGES, &PAGES, 10, 10, 164, 510);
    form::label_id(d, ID_HEAD, "", PANEL_X, 12, 400, 20);
    send(form::item(d, ID_HEAD), WM_SETFONT, bold().0 as usize, 1);
    form::hline(d, PANEL_X, 36, PANEL_W);
    let value = serde_json::to_value(&settings).unwrap_or(Value::Null);
    let mut panels = vec![];
    for p in 0..PAGES.len() {
        let panel = form::panel(d, PANEL_X, PANEL_Y, PANEL_W, PANEL_H);
        let mut y = 4;
        for (i, fl) in FIELDS.iter().enumerate().filter(|(_, fl)| fl.page == p) {
            y = field(panel, FIELD_ID + i as i32, fl, &value, y);
        }
        match p {
            4 => servers_page(panel),
            8 => scheduler_page(panel, y),
            _ => {}
        }
        panels.push(panel);
    }
    DRAFT.with(|dr| *dr.borrow_mut() = (settings.servers.clone(), settings.scheduler.clone()));
    PANELS.with(|pp| *pp.borrow_mut() = panels);
    fill_servers(d);
    form::hline(d, 10, 530, 700);
    form::button(d, ID_COLUMNS, "Columns...", 10, 542, 90);
    form::button(d, IDOK_, "OK", 454, 542, 80);
    form::button(d, IDCANCEL_, "Cancel", 542, 542, 80);
    form::button(d, ID_APPLY, "Apply", 630, 542, 80);
    show_page(d, page.min(PAGES.len() - 1));
    d
}

/// Opens Setup modeless (or brings the open one to the front).
pub fn open(owner: HWND, page: usize) {
    if let Some(open) = form::modeless().into_iter().find(|h| form::text(*h) == "Setup") {
        unsafe {
            let _ = SetForegroundWindow(open);
        }
        return;
    }
    let d = build(owner, page);
    form::show_modeless(
        d,
        Box::new(|d, ev| {
            match ev {
                Ev::Cmd(ID_PAGES, code) if code == LBN_SELCHANGE => {
                    let i = send(form::item(d, ID_PAGES), LB_GETCURSEL, 0, 0).max(0) as usize;
                    show_page(d, i);
                }
                Ev::Cmd(id, _) if (BROWSE_ID..BROWSE_ID + 1000).contains(&id) => browse(d, (id - BROWSE_ID) as usize),
                Ev::Cmd(ID_COLUMNS, _) => crate::dialogs::columns(d),
                Ev::Cmd(ID_SRV_ADD, _) => edit_server(d, None),
                Ev::Cmd(ID_SRV_EDIT, _) => {
                    if let Some(i) = form::lv_selected(form::item(d, ID_SRV_LIST)) {
                        edit_server(d, Some(i));
                    }
                }
                Ev::Cmd(ID_SRV_DEL, _) => {
                    if let Some(i) = form::lv_selected(form::item(d, ID_SRV_LIST)) {
                        let name = DRAFT.with(|dr| dr.borrow().0.get(i).map(|s| s.name.clone())).unwrap_or_default();
                        if form::confirm(Some(d), "Delete server", &format!("Delete the server {name}?")) {
                            DRAFT.with(|dr| {
                                dr.borrow_mut().0.remove(i);
                            });
                            fill_servers(d);
                        }
                    }
                }
                Ev::Cmd(id @ (ID_SRV_UP | ID_SRV_DOWN), _) => {
                    let lv = form::item(d, ID_SRV_LIST);
                    if let Some(i) = form::lv_selected(lv) {
                        let j = if id == ID_SRV_UP { i.checked_sub(1) } else { Some(i + 1) };
                        let moved = DRAFT.with(|dr| {
                            let v = &mut dr.borrow_mut().0;
                            match j {
                                Some(j) if j < v.len() => {
                                    v.swap(i, j);
                                    Some(j)
                                }
                                _ => None,
                            }
                        });
                        if let Some(j) = moved {
                            fill_servers(d);
                            form::lv_select(lv, j);
                        }
                    }
                }
                Ev::Notify(ID_SRV_LIST, lp) => {
                    let nm = unsafe { &*(lp.0 as *const NMHDR) };
                    if nm.code == NM_DBLCLK {
                        if let Some(i) = form::lv_selected(form::item(d, ID_SRV_LIST)) {
                            edit_server(d, Some(i));
                        }
                    } else if nm.code == LVN_ITEMCHANGED {
                        let n = unsafe { &*(lp.0 as *const NMLISTVIEW) };
                        if n.uChanged.0 & LVIF_STATE.0 != 0 && (n.uNewState ^ n.uOldState) & LVIS_STATEIMAGEMASK.0 != 0 && n.uOldState & LVIS_STATEIMAGEMASK.0 != 0 {
                            let on = (n.uNewState >> 12) == 2;
                            DRAFT.with(|dr| {
                                if let Some(s) = dr.borrow_mut().0.get_mut(n.iItem as usize) {
                                    s.enabled = on;
                                }
                            });
                        }
                    }
                }
                Ev::Cmd(id @ (ID_GRID_ALL | ID_GRID_NONE), _) => {
                    let c = if id == ID_GRID_ALL { "1" } else { "0" };
                    DRAFT.with(|dr| dr.borrow_mut().1 = std::array::from_fn(|_| c.repeat(SCHEDULE_SLOTS)));
                    unsafe {
                        let _ = InvalidateRect(Some(form::item(d, ID_GRID)), None, false);
                    }
                }
                Ev::Cmd(IDOK_, _) => {
                    if save(d) {
                        form::end(d, IDOK_);
                    }
                }
                Ev::Cmd(ID_APPLY, _) => {
                    save(d);
                }
                Ev::Cmd(IDCANCEL_, _) | Ev::Close => form::end(d, IDCANCEL_),
                _ => {}
            }
            0
        }),
    );
}

fn show_page(d: HWND, i: usize) {
    send(form::item(d, ID_PAGES), LB_SETCURSEL, i, 0);
    form::set_text(form::item(d, ID_HEAD), PAGES[i]);
    PANELS.with(|pp| {
        for (j, p) in pp.borrow().iter().enumerate() {
            form::show(*p, i == j);
        }
    });
}

/// Creates the control(s) for one field at `y`; returns the next free y.
fn field(p: HWND, id: i32, fl: &F, value: &Value, y: i32) -> i32 {
    let v = value.get(fl.key).cloned().unwrap_or(Value::Null);
    let lx = 0;
    let cx = 220;
    match fl.kind {
        K::Check => {
            form::check(p, id, fl.label, lx, y, PANEL_W - 10, v.as_bool().unwrap_or(false));
            y + 24
        }
        K::Note => {
            let height = form::text_height(fl.label, PANEL_W-10);
            form::note(p, fl.label, lx, y + 6, PANEL_W - 10, height);
            y + 12 + height
        }
        K::Text | K::Password => {
            form::label(p, fl.label, lx, y, cx - 6);
            let e = form::edit(p, id, v.as_str().unwrap_or(""), cx, y, 296, if matches!(fl.kind, K::Password) { ES_PASSWORD as u32 } else { 0 });
            let _ = e;
            y + 30
        }
        K::Dir | K::Exe => {
            form::label(p, fl.label, lx, y, cx - 6);
            form::edit(p, id, v.as_str().unwrap_or(""), cx, y, 214, 0);
            form::button(p, BROWSE_ID + (id - FIELD_ID), "Browse...", cx + 220, y - 1, 76);
            y + 30
        }
        K::Num(lo_, hi_, unit) => {
            form::label(p, fl.label, lx, y, cx - 6);
            form::spin(p, id, v.as_f64().unwrap_or(0.0) as i64, lo_, hi_, cx, y, 90);
            if !unit.is_empty() {
                form::label(p, unit, cx + 96, y, 60);
            }
            y + 30
        }
        K::Float(unit) => {
            form::label(p, fl.label, lx, y, cx - 6);
            let x = v.as_f64().unwrap_or(0.0);
            form::edit(p, id, &trim_float(x), cx, y, 90, 0);
            form::label(p, unit, cx + 96, y, 60);
            y + 30
        }
        K::Index(items) => {
            form::label(p, fl.label, lx, y, cx - 6);
            form::combo(p, id, items, v.as_u64().unwrap_or(0) as usize, cx, y, 200);
            y + 30
        }
        K::Str(items) => {
            form::label(p, fl.label, lx, y, cx - 6);
            let cur = v.as_str().unwrap_or("");
            let sel = items.iter().position(|(_, val)| val.eq_ignore_ascii_case(cur)).unwrap_or(0);
            let labels: Vec<&str> = items.iter().map(|(l, _)| *l).collect();
            form::combo(p, id, &labels, sel, cx, y, 200);
            y + 30
        }
        K::SpeedTag => {
            form::label(p, fl.label, lx, y, cx - 6);
            let labels = ["No limit", "10%", "20%", "30%", "40%", "50%", "60%", "70%", "80%", "90%"];
            form::combo(p, id, &labels, v.as_u64().unwrap_or(0).min(9) as usize, cx, y, 120);
            y + 30
        }
    }
}

fn trim_float(x: f64) -> String {
    let s = format!("{x:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn browse(d: HWND, i: usize) {
    let Some(fl) = FIELDS.get(i) else { return };
    let e = form::item(d, FIELD_ID + i as i32);
    let cur = form::text(e);
    let picked = match fl.kind {
        K::Exe => pick::open_files(d, fl.label, &[("Programs", "*.exe")], false).into_iter().next(),
        _ => pick::folder(d, fl.label, &cur),
    };
    if let Some(p) = picked {
        form::set_text(e, &p);
    }
}

/// Reads every page back into `Settings` and sends it; false when a value
/// is invalid (the page with the problem is shown).
fn save(d: HWND) -> bool {
    let base = with_engine(|e| e.settings.clone());
    let mut value = serde_json::to_value(&base).unwrap_or(Value::Null);
    let Some(map) = value.as_object_mut() else { return false };
    for (i, fl) in FIELDS.iter().enumerate() {
        if fl.key.is_empty() {
            continue;
        }
        let c = form::item(d, FIELD_ID + i as i32);
        let v = match fl.kind {
            K::Check => json!(form::checked(c)),
            K::Text | K::Password | K::Dir | K::Exe => json!(form::text(c).trim()),
            K::Num(lo_, hi_, _) => json!(form::num(c).clamp(lo_ as i64, hi_ as i64)),
            K::Float(_) => match form::text(c).trim().replace(',', ".").parse::<f64>() {
                Ok(x) if x >= 0.0 => json!(x),
                _ => {
                    show_page(d, fl.page);
                    form::message(Some(d), "Setup", &format!("{}: enter a number.", fl.label), MB_ICONWARNING);
                    return false;
                }
            },
            K::Index(_) => json!(form::combo_sel(c)),
            K::Str(items) => json!(items.get(form::combo_sel(c)).map(|x| x.1).unwrap_or("")),
            K::SpeedTag => match form::combo_sel(c) {
                0 => Value::Null,
                t => json!(t),
            },
            K::Note => continue,
        };
        map.insert(fl.key.into(), v);
    }
    // Web access needs a password unless it only listens on this PC.
    let web_on = map.get("web_access").and_then(|v| v.as_bool()).unwrap_or(false);
    let bind = map.get("web_access_bind").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let key_empty = map.get("web_access_key").and_then(|v| v.as_str()).unwrap_or("").is_empty();
    if web_on {
        match bind.parse::<std::net::SocketAddr>() {
            Err(_) => {
                show_page(d, 13);
                form::message(Some(d), "Setup", "Web access: enter the address as address:port, for example 0.0.0.0:8480.", MB_ICONWARNING);
                return false;
            }
            Ok(a) if key_empty && !a.ip().is_loopback() => {
                show_page(d, 13);
                form::message(Some(d), "Setup", "Web access: set a password, or listen on 127.0.0.1 only.", MB_ICONWARNING);
                return false;
            }
            _ => {}
        }
    }
    // The two line-speed settings move together.
    if let Some(k) = map.get("max_speed_kbps").and_then(|v| v.as_f64()) {
        map.insert("max_speed_mbps".into(), json!(k * 8.0 / 1000.0));
    }
    let (servers, scheduler) = DRAFT.with(|dr| dr.borrow().clone());
    map.insert("servers".into(), serde_json::to_value(&servers).unwrap_or(Value::Null));
    map.insert("scheduler".into(), serde_json::to_value(&scheduler).unwrap_or(Value::Null));
    match serde_json::from_value::<Settings>(value) {
        Ok(s) => {
            let web = (s.web_access, s.web_access_bind.clone(), s.web_access_key.clone());
            if s != base {
                send_cmd(Command::ReplaceSettings(Box::new(s)));
            }
            crate::start_web_access(web.0, &web.1, &web.2);
            true
        }
        Err(e) => {
            form::message(Some(d), "Setup", &format!("Settings not saved: {e}"), MB_ICONERROR);
            false
        }
    }
}

// ------------------------------------------------------------- servers ----

fn servers_page(p: HWND) {
    let mut x = 0;
    for (id, t, w) in [(ID_SRV_ADD, "Add server", 92), (ID_SRV_EDIT, "Edit server", 92), (ID_SRV_DEL, "Delete server", 98), (ID_SRV_UP, "Move up", 80), (ID_SRV_DOWN, "Move down", 86)] {
        form::button(p, id, t, x, 0, w);
        x += w + 6;
    }
    form::listview(
        p,
        ID_SRV_LIST,
        &[("Name", 130, false), ("Address", 150, false), ("Port", 46, true), ("Conn.", 46, true), ("SSL", 38, false), ("Level", 52, true)],
        0,
        36,
        PANEL_W - 4,
        240,
        LVS_EX_CHECKBOXES,
    );
    form::note(
        p,
        "Tick a server to use it. Level 0 servers are used first; higher levels only fill in articles the lower ones miss. A server needs a name, an address and a non-zero port; SSL switches the default port between 563 and 119.",
        0,
        286,
        PANEL_W - 10,
        64,
    );
}

fn fill_servers(d: HWND) {
    let lv = form::item(d, ID_SRV_LIST);
    if lv.is_invalid() {
        return;
    }
    let servers = DRAFT.with(|dr| dr.borrow().0.clone());
    let rows: Vec<Vec<String>> = servers
        .iter()
        .map(|s| vec![s.name.clone(), s.address.clone(), s.port.to_string(), s.connections.to_string(), if s.ssl { "Yes" } else { "No" }.into(), s.level.to_string()])
        .collect();
    form::lv_fill(lv, &rows);
    for (i, s) in servers.iter().enumerate() {
        form::lv_set_checked(lv, i, s.enabled);
    }
}

fn edit_server(setup: HWND, index: Option<usize>) {
    let server = match index {
        Some(i) => DRAFT.with(|dr| dr.borrow().0.get(i).cloned()),
        None => Some(Server { name: "New server".into(), address: String::new(), username: String::new(), password: String::new(), ..Server::default() }),
    };
    let Some(server) = server else { return };
    let d = build_server(setup, &server, index.is_some());
    let result: Rc<RefCell<Option<Server>>> = Rc::new(RefCell::new(None));
    let r2 = Rc::clone(&result);
    form::run_modal(
        d,
        Some(setup),
        Box::new(move |d, ev| {
            match ev {
                Ev::Cmd(S_SSL, _) => {
                    // SSL flips the port between the two defaults.
                    let port = form::num(form::item(d, S_PORT));
                    let ssl = form::checked(form::item(d, S_SSL));
                    if (ssl && port == PLAIN_PORT as i64) || (!ssl && port == SSL_PORT as i64) {
                        form::set_text(form::item(d, S_PORT), &(if ssl { SSL_PORT } else { PLAIN_PORT }).to_string());
                    }
                }
                Ev::Cmd(S_AUTH, _) => sync_auth(d),
                Ev::Cmd(IDOK_, _) => match read_server(d, &server) {
                    Ok(s) => {
                        *r2.borrow_mut() = Some(s);
                        form::end(d, IDOK_);
                    }
                    Err(e) => form::message(Some(d), "Server", &e, MB_ICONWARNING),
                },
                Ev::Cmd(IDCANCEL_, _) | Ev::Close => form::end(d, IDCANCEL_),
                _ => {}
            }
            0
        }),
    );
    let Some(s) = result.take() else { return };
    DRAFT.with(|dr| {
        let v = &mut dr.borrow_mut().0;
        match index {
            Some(i) if i < v.len() => v[i] = s,
            _ => v.push(s),
        }
    });
    fill_servers(setup);
    let n = index.unwrap_or_else(|| DRAFT.with(|dr| dr.borrow().0.len().saturating_sub(1)));
    form::lv_select(form::item(setup, ID_SRV_LIST), n);
}

const S_NAME: i32 = 10;
const S_CONNS: i32 = 11;
const S_GROUP: i32 = 12;
const S_ADDR: i32 = 13;
const S_PORT: i32 = 14;
const S_SSL: i32 = 15;
const S_AUTH: i32 = 16;
const S_USER: i32 = 17;
const S_PASS: i32 = 18;
const S_ARTICLE: i32 = 19;
const S_IDLE: i32 = 20;
const S_PURPOSE: i32 = 21;
const S_AGE: i32 = 22;
const S_NIC: i32 = 23;
const S_ENABLED: i32 = 24;
const S_LEVEL: i32 = 25;
const S_QUOTA: i32 = 26;

pub fn build_server(owner: HWND, s: &Server, existing: bool) -> HWND {
    let title = if existing { format!("Edit server {}", s.name) } else { "Add server".into() };
    let d = form::create(Some(owner), &title, 600, 376, false);
    form::label(d, "Name", 14, 16, 90);
    form::edit(d, S_NAME, &s.name, 110, 16, 180, 0);
    form::label(d, "Nr. of connections", 312, 16, 108);
    form::spin(d, S_CONNS, s.connections as i64, 1, MAX_CONNECTIONS_PER_SERVER as i32, 424, 16, 60);
    form::check(d, S_GROUP, "Select group", 494, 17, 96, s.select_group);
    form::label(d, "Address", 14, 48, 90);
    form::edit(d, S_ADDR, &s.address, 110, 48, 180, 0);
    form::label(d, "Port", 312, 48, 108);
    form::spin(d, S_PORT, s.port as i64, 1, 65535, 424, 48, 64);
    form::check(d, S_SSL, "SSL", 494, 49, 60, s.ssl);
    form::group(d, "", 14, 86, 572, 108);
    form::check(d, S_AUTH, "Server requires authentication", 24, 82, 210, s.auth);
    form::label(d, "Username", 28, 110, 76);
    form::edit(d, S_USER, &s.username, 110, 110, 180, 0);
    form::label(d, "Password", 312, 110, 108);
    form::edit(d, S_PASS, &s.password, 424, 110, 150, ES_PASSWORD as u32);
    form::check(d, S_ARTICLE, "ARTICLE command instead of BODY", 28, 142, 260, s.article_cmd);
    form::check(d, S_IDLE, "Stay connected while idle", 28, 164, 220, s.connected_idle);
    form::label(d, "Server purpose", 14, 208, 94);
    form::edit(d, S_PURPOSE, &s.purpose, 110, 208, 180, 0);
    form::label(d, "Max. age", 312, 208, 108);
    form::spin(d, S_AGE, s.max_age as i64, 0, 99_999, 424, 208, 70);
    form::label(d, "days", 500, 208, 40);
    form::label(d, "Network adapter", 14, 240, 94);
    form::edit(d, S_NIC, &s.nic, 110, 240, 180, 0);
    form::label(d, "Monthly quota", 312, 240, 108);
    form::spin(d, S_QUOTA, s.quota_gb as i64, 0, 100_000, 424, 240, 70);
    form::label(d, "GB", 500, 240, 30);
    form::label(d, "Level", 14, 272, 94);
    form::combo(d, S_LEVEL, &["0 (primary)", "1 (backup)", "2 (backup)", "3 (backup)", "4 (backup)"], (s.level as usize).min(4), 110, 272, 140);
    form::check(d, S_ENABLED, "Use this server", 312, 273, 200, s.enabled);
    form::note(d, "Max. age 0 = no retention limit; quota 0 = unlimited.", 14, 302, 572, 18);
    form::hline(d, 14, 326, 572);
    form::button(d, IDOK_, "Save", 418, 338, 80);
    form::button(d, IDCANCEL_, "Cancel", 506, 338, 80);
    sync_auth(d);
    d
}

fn sync_auth(d: HWND) {
    let on = form::checked(form::item(d, S_AUTH));
    form::enable(form::item(d, S_USER), on);
    form::enable(form::item(d, S_PASS), on);
}

fn read_server(d: HWND, base: &Server) -> Result<Server, String> {
    let t = |id| form::text(form::item(d, id)).trim().to_string();
    let mut s = base.clone();
    s.name = t(S_NAME);
    s.address = t(S_ADDR);
    s.port = form::num(form::item(d, S_PORT)).clamp(0, 65535) as u16;
    s.connections = form::num(form::item(d, S_CONNS)).clamp(1, MAX_CONNECTIONS_PER_SERVER as i64) as u32;
    s.ssl = form::checked(form::item(d, S_SSL));
    s.select_group = form::checked(form::item(d, S_GROUP));
    s.auth = form::checked(form::item(d, S_AUTH));
    s.username = t(S_USER);
    s.password = form::text(form::item(d, S_PASS));
    s.article_cmd = form::checked(form::item(d, S_ARTICLE));
    s.connected_idle = form::checked(form::item(d, S_IDLE));
    s.purpose = t(S_PURPOSE);
    s.max_age = form::num(form::item(d, S_AGE)).max(0) as u32;
    s.nic = t(S_NIC);
    s.quota_gb = form::num(form::item(d, S_QUOTA)).max(0) as u32;
    s.level = form::combo_sel(form::item(d, S_LEVEL)) as u32;
    s.enabled = form::checked(form::item(d, S_ENABLED));
    if s.name.is_empty() || s.address.is_empty() || s.port == 0 {
        return Err("A server needs a name, an address and a non-zero port.".into());
    }
    Ok(s)
}

// ----------------------------------------------------------- scheduler ----

const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const CELL_W: i32 = 10;
const CELL_H: i32 = 18;
const GRID_LEFT: i32 = 40;

fn scheduler_page(p: HWND, y: i32) {
    form::note(p, "Each row is a day and each cell half an hour. Green slots allow downloading; click or drag to change them.", 0, y + 4, PANEL_W - 10, 32);
    register_grid();
    let h = 7 * CELL_H + 26;
    form::reg(p, ID_GRID, mk("NQSchedGrid", "", form::CHILD, 0, 0, y + 42, GRID_LEFT + CELL_W * SCHEDULE_SLOTS as i32 + 4, h, p, ID_GRID));
    let gy = y + 42 + h + 10;
    form::button(p, ID_GRID_ALL, "Allow all", 0, gy, 90);
    form::button(p, ID_GRID_NONE, "Block all", 96, gy, 90);
}

fn register_grid() {
    thread_local! { static DONE: RefCell<bool> = const { RefCell::new(false) }; }
    if DONE.with(|d| d.replace(true)) {
        return;
    }
    unsafe {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(grid_proc),
            hInstance: hinst(),
            hCursor: LoadCursorW(None, IDC_HAND).unwrap_or_default(),
            hbrBackground: GetSysColorBrush(COLOR_BTNFACE),
            lpszClassName: windows::core::w!("NQSchedGrid"),
            ..Default::default()
        };
        RegisterClassExW(&wc);
    }
}

thread_local! { static PAINT_VALUE: RefCell<Option<char>> = const { RefCell::new(None) }; }

fn grid_cell(lp: LPARAM) -> Option<(usize, usize)> {
    let (x, y) = (lo(lp.0) as i16 as i32, hi(lp.0) as i16 as i32);
    let col = (x - s(GRID_LEFT)) / s(CELL_W);
    let row = y / s(CELL_H);
    (x >= s(GRID_LEFT) && (0..SCHEDULE_SLOTS as i32).contains(&col) && (0..7).contains(&row)).then_some((row as usize, col as usize))
}

fn set_slot(day: usize, slot: usize, v: Option<char>) -> char {
    DRAFT.with(|dr| {
        let sched = &mut dr.borrow_mut().1;
        let mut chars: Vec<char> = sched[day].chars().collect();
        chars.resize(SCHEDULE_SLOTS, '0');
        let new = v.unwrap_or(if chars[slot] == '1' { '0' } else { '1' });
        chars[slot] = new;
        sched[day] = chars.into_iter().collect();
        new
    })
}

unsafe extern "system" fn grid_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_LBUTTONDOWN => {
            if let Some((d, c)) = grid_cell(lp) {
                let v = set_slot(d, c, None);
                PAINT_VALUE.with(|p| *p.borrow_mut() = Some(v));
                SetCapture(h);
                let _ = InvalidateRect(Some(h), None, false);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if let Some(v) = PAINT_VALUE.with(|p| *p.borrow()) {
                if let Some((d, c)) = grid_cell(lp) {
                    set_slot(d, c, Some(v));
                    let _ = InvalidateRect(Some(h), None, false);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            PAINT_VALUE.with(|p| *p.borrow_mut() = None);
            let _ = ReleaseCapture();
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(h, &mut ps);
            paint_grid(h, dc);
            let _ = EndPaint(h, &ps);
            LRESULT(0)
        }
        WM_PRINTCLIENT => {
            paint_grid(h, HDC(wp.0 as *mut std::ffi::c_void));
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

unsafe fn paint_grid(h: HWND, dc: HDC) {
    {
        {
            let mut rc = RECT::default();
            let _ = GetClientRect(h, &mut rc);
            let mem = CreateCompatibleDC(Some(dc));
            let bmp = CreateCompatibleBitmap(dc, rc.right, rc.bottom);
            let old = SelectObject(mem, bmp.into());
            FillRect(mem, &rc, GetSysColorBrush(COLOR_BTNFACE));
            SelectObject(mem, font().into());
            SetBkMode(mem, TRANSPARENT);
            let sched = DRAFT.with(|dr| dr.borrow().1.clone());
            let on = CreateSolidBrush(rgb(0x4C, 0xAF, 0x50));
            let off = CreateSolidBrush(rgb(0xFF, 0xFF, 0xFF));
            let line = CreateSolidBrush(rgb(0xB8, 0xBC, 0xC4));
            for (d, day) in DAYS.iter().enumerate() {
                let y = s(CELL_H) * d as i32;
                let mut z = wz(day);
                z.pop();
                let mut tr = RECT { left: 0, top: y, right: s(GRID_LEFT - 4), bottom: y + s(CELL_H) };
                SetTextColor(mem, COLORREF(GetSysColor(COLOR_BTNTEXT)));
                DrawTextW(mem, &mut z, &mut tr, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
                let chars: Vec<char> = sched[d].chars().collect();
                for slot in 0..SCHEDULE_SLOTS {
                    let x = s(GRID_LEFT) + s(CELL_W) * slot as i32;
                    let cell = RECT { left: x, top: y, right: x + s(CELL_W), bottom: y + s(CELL_H) };
                    FrameRect(mem, &cell, line);
                    let inner = RECT { left: cell.left + 1, top: cell.top + 1, right: cell.right - 1, bottom: cell.bottom - 1 };
                    FillRect(mem, &inner, if chars.get(slot) == Some(&'1') { on } else { off });
                }
            }
            for hour in (0..24).step_by(2) {
                let x = s(GRID_LEFT) + s(CELL_W) * hour * 2;
                let mut z = wz(&format!("{hour:02}"));
                z.pop();
                let mut tr = RECT { left: x - s(10), top: s(CELL_H) * 7 + s(4), right: x + s(10), bottom: s(CELL_H) * 7 + s(22) };
                SetTextColor(mem, COLORREF(GetSysColor(COLOR_GRAYTEXT)));
                DrawTextW(mem, &mut z, &mut tr, DT_CENTER | DT_SINGLELINE);
            }
            for b in [on, off, line] {
                let _ = DeleteObject(b.into());
            }
            let _ = BitBlt(dc, 0, 0, rc.right, rc.bottom, Some(mem), 0, 0, SRCCOPY);
            SelectObject(mem, old);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bound_key_is_a_settings_field_on_a_real_page() {
        let value = serde_json::to_value(Settings::default()).unwrap();
        for fl in FIELDS {
            assert!(fl.page < PAGES.len() && fl.page != 4, "{} on page {}", fl.key, fl.page);
            if !fl.key.is_empty() {
                assert!(value.get(fl.key).is_some(), "unknown settings key {}", fl.key);
            }
        }
        // Every page has something on it.
        for p in 0..PAGES.len() {
            assert!(p == 4 || FIELDS.iter().any(|f| f.page == p), "page {} is empty", PAGES[p]);
        }
    }

    #[test]
    fn no_setting_is_bound_twice() {
        let mut keys: Vec<&str> = FIELDS.iter().map(|f| f.key).filter(|k| !k.is_empty()).collect();
        keys.sort();
        let n = keys.len();
        keys.dedup();
        assert_eq!(n, keys.len());
    }
}
