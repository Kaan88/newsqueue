#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod container;
mod dialogs;
mod download;
mod files;
mod model;
mod par2;
mod rss;
mod shutdown;
mod theme;
mod tray;
mod ui;
mod unrar;

use chrono::{Datelike, Local, Timelike};
use eframe::egui;
use model::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub collection: usize,
    pub file: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveDir {
    Top,
    Up,
    Down,
    Bottom,
}

#[derive(Clone, Debug)]
pub enum UiAction {
    Select(Option<Selection>),
    ToggleExpand(usize),
    PauseSelected(bool),
    DeleteSelected,
    Move(MoveDir),
    ChangeFolder,
    Export,
    SetCategory,
    ToggleUnrar(bool),
    UnpackNow,
    ChangeUnrarFolder,
    SetExecute,
    JoinSplitParts,
    SplitSelectedFile,
    OpenFolder,
    Play,
    Eta,
    Rename,
    CollapseAll,
    ExpandAll,
    None,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryItem {
    pub name: String,
    pub date: String,
    pub size: String,
}

fn history_path() -> PathBuf {
    app_base().join("newsqueue-history.json")
}

fn load_history() -> Option<Vec<HistoryItem>> {
    let text = std::fs::read_to_string(history_path()).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_history(history: &[HistoryItem]) {
    if let Ok(text) = serde_json::to_string_pretty(history) {
        let _ = std::fs::write(history_path(), text);
    }
}

/// Persisted transfer totals. Only the current day and year are kept; older
/// buckets are intentionally not shown because the Statistics window has no
/// day picker yet.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct TransferStats {
    #[serde(default)]
    day: String,
    #[serde(default)]
    year: i32,
    #[serde(default)]
    daily: [f64; 24],
    #[serde(default)]
    monthly: [f64; 12],
    #[serde(default)]
    collections: u64,
    #[serde(default)]
    startups: u64,
    #[serde(default)]
    longest_uptime_secs: u64,
}

impl TransferStats {
    fn normalize(&mut self) {
        let now = Local::now();
        self.normalize_for(now.year(), &now.format("%Y-%m-%d").to_string());
    }

    fn normalize_for(&mut self, year: i32, day: &str) {
        if self.day != day {
            self.daily = [0.0; 24];
            self.day = day.to_string();
        }
        if self.year != year {
            self.monthly = [0.0; 12];
            self.year = year;
        }
    }
}

fn stats_path() -> PathBuf {
    app_base().join("newsqueue-stats.json")
}

fn load_transfer_stats() -> TransferStats {
    let mut stats = std::fs::read_to_string(stats_path())
        .ok()
        .and_then(|text| serde_json::from_str::<TransferStats>(&text).ok())
        .unwrap_or_default();
    stats.normalize();
    stats
}

fn save_transfer_stats(stats: &TransferStats) {
    if let Ok(text) = serde_json::to_string_pretty(stats) {
        let _ = std::fs::write(stats_path(), text);
    }
}

fn demo_daily_transfer() -> [f64; 24] {
    let mut daily = [0.0f64; 24];
    for (index, value) in daily.iter_mut().enumerate() {
        *value = ((index as f64 * 0.73).sin().abs() * 240.0
            + (index as f64 * 0.31).cos().abs() * 80.0)
            .round();
    }
    daily
}

fn demo_monthly_transfer() -> [f64; 12] {
    [
        3_240.0, 2_980.0, 4_120.0, 3_760.0, 4_880.0, 5_120.0, 4_460.0, 4_910.0, 5_380.0,
        5_010.0, 4_720.0, 5_640.0,
    ]
}

fn sample_history() -> Vec<HistoryItem> {
    vec![
        HistoryItem {
            name: "Quiet Harbor Sessions".into(),
            date: "2026-11-02 19:42".into(),
            size: "2.41 GB".into(),
        },
        HistoryItem {
            name: "Field Notes Collection".into(),
            date: "2026-11-01 08:15".into(),
            size: "918.2 MB".into(),
        },
        HistoryItem {
            name: "Toolkit Archive 2024".into(),
            date: "2026-10-29 22:05".into(),
            size: "4.12 GB".into(),
        },
    ]
}

pub struct App {
    pub settings: Settings,
    pub setup_draft: Settings,
    pub queue: Vec<Collection>,
    pub par2: Vec<Par2Set>,
    pub rss: Vec<RssItem>,
    pub logs: VecDeque<LogEntry>,
    pub threads: Vec<ThreadRow>,
    pub history: Vec<HistoryItem>,
    pub selected: Option<Selection>,
    pub selected_par2: Option<usize>,
    pub active_tab: usize,
    pub connected: bool,
    pub paused: bool,
    pub pause_until: Option<Instant>,
    pub pause_minutes: u32,
    pub resume_ids: Vec<u64>,
    pub speed_limit_tag: Option<u8>,
    pub speed_ceiling_mbps: f64,
    pub auto_speed: bool,
    pub auto_speed_ticks: u32,
    pub speed_samples: [f64; SPEED_SAMPLE_COUNT],
    pub speed_sample_index: usize,
    pub pending_speed_bps: f64,
    pub pending_bytes: f64,
    pub speed_bps: f64,
    pub speed_history: VecDeque<f32>,
    pub transfered_session: u64,
    pub session_start: Instant,
    pub startups: u64,
    pub total_collections: u64,
    pub session_collections: u64,
    pub longest_uptime_secs: u64,
    pub last_tick: Instant,
    pub last_sample: Instant,
    pub last_minute_stats: Instant,
    pub last_hour_stats: Instant,
    pub last_save: Instant,
    pub last_queue_save: Instant,
    pub low_space_warned: bool,
    pub frame: u64,
    pub tray_hidden: bool,
    pub screenshot_path: Option<String>,
    pub screenshot_requested: bool,
    pub screenshot_saved: bool,
    pub show_setup: bool,
    pub setup_page: usize,
    pub show_server_edit: bool,
    pub editing_server: Option<usize>,
    pub setup_server_selected: Option<usize>,
    pub server_draft: Server,
    pub server_edit_error: String,
    pub show_categories: bool,
    pub category_draft: Vec<Category>,
    pub show_stats: bool,
    pub stats_tab: usize,
    pub show_about: bool,
    pub show_notes: bool,
    pub notes_draft: String,
    pub show_columns: bool,
    pub column_visible_draft: [bool; 12],
    pub column_width_draft: [f32; 12],
    pub show_rename: bool,
    pub rename_draft: String,
    pub rename_index: Option<usize>,
    pub show_pause_for: bool,
    pub show_shutdown: bool,
    pub show_unrar_password: bool,
    pub unrar_password_draft: String,
    pub unrar_password_target: Option<u64>,
    pub show_speed_limit: bool,
    pub show_filter_help: bool,
    pub show_message: Option<(String, String)>,
    pub filter: String,
    pub log_filter: String,
    pub rss_filter: String,
    pub rss_timer: bool,
    pub rss_feed_draft: String,
    pub rss_filters_draft: String,
    pub show_rss_feeds: bool,
    pub rss_rx: Option<std::sync::mpsc::Receiver<Result<rss::FetchResult, String>>>,
    pub rss_last_refresh: Instant,
    pub last_auto_import: Instant,
    pub show_threads: bool,
    pub thread_height: f32,
    pub status_mode_speed: u8,
    pub status_mode_queue: u8,
    pub status_mode_transfered: u8,
    pub scheduler_hold: bool,
    pub scheduler_last_hold: bool,
    pub shutdown_armed: bool,
    pub shutdown_countdown: Option<Instant>,
    pub shutdown_mode: shutdown::ShutdownMode,
    pub shutdown_exit_requested: bool,
    pub shutdown_exit_sent: bool,
    pub repair_until: Option<(usize, Instant)>,
    pub explorer_path: PathBuf,
    pub explorer_path_edit: String,
    pub explorer_selected: Option<PathBuf>,
    pub explorer_selected_set: HashSet<PathBuf>,
    pub explorer_anchor: Option<usize>,
    pub explorer_history: Vec<PathBuf>,
    pub explorer_forward: Vec<PathBuf>,
    pub explorer_filter: String,
    pub show_explorer_rename: bool,
    pub explorer_rename_draft: String,
    pub explorer_rename_target: Option<PathBuf>,
    pub show_execute_command: bool,
    pub execute_command_draft: String,
    pub execute_command_target: Option<usize>,
    pub show_split_file: bool,
    pub split_file_target: Option<PathBuf>,
    pub split_size_mb: u32,
    pub explorer_clipboard: Option<(Vec<PathBuf>, bool)>,
    pub toast: Option<(String, Instant)>,
    pub daily_transfer: [f64; 24],
    pub monthly_transfer: [f64; 12],
    pub stats_day: String,
    pub stats_year: i32,
    pub startup_done: bool,
    pub last_log_day: String,
    pub active_dl: Option<download::ActiveDownload>,
    pub active_unrar: Option<unrar::ActiveUnrar>,
    pub active_par2: Option<par2::ActivePar2>,
    pub pending_unrar: VecDeque<u64>,
    pub resume_after_unrar: bool,
}

impl App {
    pub fn new(settings: Settings, screenshot_path: Option<String>) -> Self {
        ensure_dirs(&settings);
        prune_old_logs(settings.keep_log_days);
        let demo = screenshot_path.is_some();
        let queue = if demo {
            sample_queue(&settings)
        } else {
            let mut saved = load_queue_state().unwrap_or_default();
            // Older saves contain demonstration entries with no article ids.
            saved.retain(|c| c.has_segments());
            saved
        };
        let threads = build_threads(false, &queue, &settings, false, 0);
        let (
            daily,
            monthly,
            stats_day,
            stats_year,
            total_collections,
            total_startups,
            longest_uptime_secs,
        ) = if demo {
            let now = Local::now();
            (
                demo_daily_transfer(),
                demo_monthly_transfer(),
                now.format("%Y-%m-%d").to_string(),
                now.year(),
                0,
                1,
                0,
            )
        } else {
            let mut stats = load_transfer_stats();
            stats.startups = stats.startups.saturating_add(1);
            (
                stats.daily,
                stats.monthly,
                stats.day,
                stats.year,
                stats.collections,
                stats.startups,
                stats.longest_uptime_secs,
            )
        };
        let explorer_path = PathBuf::from(&settings.download_dir);
        let explorer_path_text = explorer_path.to_string_lossy().to_string();
        let _ = std::fs::create_dir_all(&explorer_path);
        let thread_height = settings.thread_list_height;
        let show_threads = settings.thread_list_visible;
        let status_mode_speed = settings.speed_bytes_mode;
        let status_mode_queue = settings.queue_bytes_mode;
        let status_mode_transfered = settings.transfered_bytes_mode;
        let speed_limit_tag = settings.startup_speed_limit;
        let speed_ceiling_mbps = settings.max_speed_mbps;
        let auto_speed = settings.max_speed_kbps <= 0.0;
        let notes_draft = settings.notes.clone();
        let rss_feed_draft = settings.rss_urls.join("\n");
        let rss_filters_draft = settings.rss_filters.clone();
        let column_visible_draft = settings.column_visible;
        let column_width_draft = settings.column_widths;
        let setup_draft = settings.clone();
        let category_draft = settings.categories.clone();
        let history = if demo {
            sample_history()
        } else {
            load_history().unwrap_or_default()
        };
        let mut app = Self {
            settings,
            setup_draft,
            queue,
            par2: if demo { sample_par2() } else { Vec::new() },
            rss: if demo { sample_rss() } else { Vec::new() },
            logs: if demo { sample_log() } else { VecDeque::new() },
            threads,
            history,
            selected: None,
            selected_par2: None,
            active_tab: 0,
            connected: false,
            paused: false,
            pause_until: None,
            pause_minutes: 0,
            resume_ids: Vec::new(),
            speed_limit_tag,
            speed_ceiling_mbps,
            auto_speed,
            auto_speed_ticks: 0,
            speed_samples: [0.0; SPEED_SAMPLE_COUNT],
            speed_sample_index: 0,
            pending_speed_bps: 0.0,
            pending_bytes: 0.0,
            speed_bps: 0.0,
            speed_history: VecDeque::from(vec![0.0; 120]),
            transfered_session: 0,
            session_start: Instant::now(),
            startups: total_startups,
            total_collections,
            session_collections: 0,
            longest_uptime_secs,
            last_tick: Instant::now(),
            last_sample: Instant::now(),
            last_minute_stats: Instant::now(),
            last_hour_stats: Instant::now(),
            last_save: Instant::now(),
            last_queue_save: Instant::now(),
            low_space_warned: false,
            frame: 0,
            tray_hidden: false,
            screenshot_path,
            screenshot_requested: false,
            screenshot_saved: false,
            show_setup: false,
            setup_page: 0,
            show_server_edit: false,
            editing_server: None,
            setup_server_selected: None,
            server_draft: Server::default(),
            server_edit_error: String::new(),
            show_categories: false,
            category_draft,
            show_stats: false,
            stats_tab: 0,
            show_about: false,
            show_notes: false,
            notes_draft,
            show_columns: false,
            column_visible_draft,
            column_width_draft,
            show_rename: false,
            rename_draft: String::new(),
            rename_index: None,
            show_pause_for: false,
            show_shutdown: false,
            show_unrar_password: false,
            unrar_password_draft: String::new(),
            unrar_password_target: None,
            show_speed_limit: false,
            show_filter_help: false,
            show_message: None,
            filter: String::new(),
            log_filter: String::new(),
            rss_filter: String::new(),
            rss_timer: false,
            rss_feed_draft,
            rss_filters_draft,
            show_rss_feeds: false,
            rss_rx: None,
            rss_last_refresh: Instant::now(),
            last_auto_import: Instant::now(),
            show_threads,
            thread_height,
            status_mode_speed,
            status_mode_queue,
            status_mode_transfered,
            scheduler_hold: false,
            scheduler_last_hold: false,
            shutdown_armed: false,
            shutdown_countdown: None,
            shutdown_mode: shutdown::ShutdownMode::Real,
            shutdown_exit_requested: false,
            shutdown_exit_sent: false,
            repair_until: None,
            explorer_path,
            explorer_path_edit: explorer_path_text,
            explorer_selected: None,
            explorer_selected_set: HashSet::new(),
            explorer_anchor: None,
            explorer_history: Vec::new(),
            explorer_forward: Vec::new(),
            explorer_filter: String::new(),
            show_explorer_rename: false,
            explorer_rename_draft: String::new(),
            explorer_rename_target: None,
            show_execute_command: false,
            execute_command_draft: String::new(),
            execute_command_target: None,
            show_split_file: false,
            split_file_target: None,
            split_size_mb: 100,
            explorer_clipboard: None,
            toast: None,
            daily_transfer: daily,
            monthly_transfer: monthly,
            stats_day,
            stats_year,
            startup_done: false,
            last_log_day: String::new(),
            active_dl: None,
            active_unrar: None,
            active_par2: None,
            pending_unrar: VecDeque::new(),
            resume_after_unrar: false,
        };
        if !demo {
            let history_count = app.history.len();
            app.log(
                "Main",
                &format!("{APP_NAME} {VERSION} ({BUILD_DATE}) started."),
            );
            app.log("Main", "OS Windows");
            app.log(
                "Main",
                &format!("{history_count} items loaded into download history list."),
            );
            app.log("Main", "Ready. No server connection is open.");
        }
        match std::env::var("NQ_DEMO_DIALOG").as_deref() {
            Ok("setup") => {
                app.setup_draft = app.settings.clone();
                app.show_setup = true;
            }
            Ok("server") => {
                app.setup_draft = app.settings.clone();
                app.show_setup = true;
                app.setup_page = 4;
                app.server_draft = app.settings.servers[0].clone();
                app.editing_server = Some(0);
                app.show_server_edit = true;
            }
            Ok("stats") => app.show_stats = true,
            Ok("shutdown") => app.show_shutdown = true,
            Ok("unrar-password") => app.show_unrar_password = true,
            Ok("categories") => app.show_categories = true,
            Ok("notes") => app.show_notes = true,
            Ok("about") => app.show_about = true,
            Ok("columns") => app.show_columns = true,
            Ok("pause") => app.show_pause_for = true,
            _ => {}
        }
        if let Ok(tab) = std::env::var("NQ_DEMO_TAB") {
            app.active_tab = match tab.as_str() {
                "par2" => 1,
                "explorer" => 2,
                "log" => 3,
                "rss" => 4,
                _ => 0,
            };
        }
        if let Ok(path) = std::env::var("NQ_DEMO_EXPLORER") {
            let path = PathBuf::from(path);
            if path.is_dir() {
                app.explorer_path = path;
                app.explorer_path_edit = app.explorer_path.to_string_lossy().to_string();
            }
        }
        if std::env::var("NQ_DEMO_CONNECT").is_ok() {
            app.connect();
        }
        if app.settings.startup_connect && !app.connected {
            app.connect();
        }
        if std::env::var("NQ_DEMO_RSS").is_ok() {
            app.refresh_rss();
        }
        #[cfg(debug_assertions)]
        if std::env::var("NQ_TEST_ARM_SHUTDOWN").as_deref() == Ok("exit") {
            app.settings.shutdown_method = "Exit".into();
            app.settings.minimize_to_tray = true;
            app.settings.minimize_to_tray_on_close = true;
            app.shutdown_armed = true;
            app.shutdown_mode = shutdown::ShutdownMode::Real;
            app.log("Shutdown", "Test shutdown armed (Exit).");
        }
        if app.screenshot_path.is_none() {
            for entry in app.logs.clone() {
                app.write_log_file(&entry);
            }
        }
        if let Ok(page) = std::env::var("NQ_DEMO_SETUP_PAGE") {
            if let Ok(page) = page.parse::<usize>() {
                app.setup_draft = app.settings.clone();
                app.setup_page = page.min(16);
                app.show_setup = true;
            }
        }
        app
    }

    pub fn log(&mut self, level: &str, message: &str) {
        let entry = LogEntry {
            time: now_time(),
            level: level.to_string(),
            message: message.to_string(),
        };
        self.logs.push_back(entry.clone());
        while self.logs.len() > self.settings.max_log_lines.max(1) {
            self.logs.pop_front();
        }
        self.write_log_file(&entry);
    }

    fn write_log_file(&self, entry: &LogEntry) {
        if self.screenshot_path.is_some()
            || !self.settings.log_enabled
            || !self.settings.log_to_file
        {
            return;
        }
        let dir = app_base().join("log");
        let _ = std::fs::create_dir_all(&dir);
        let name = Local::now().format("%Y%m%d.log").to_string();
        let line = format!(
            "[{}] [{}] {}\r\n",
            Local::now().format("%m/%d/%Y %I:%M:%S %p"),
            entry.level,
            entry.message
        );
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(name))
        {
            let _ = f.write_all(line.as_bytes());
        }
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), Instant::now()));
    }

    pub fn active_collection(&self) -> Option<usize> {
        active_download(&self.queue)
    }

    /// Global bandwidth limit in bytes per second. `None` means "No limit"
    /// and is 0 (unthrottled), matching function 0x817178.
    pub fn speed_limit_bps(&self) -> f64 {
        match self.speed_limit_tag {
            Some(tag) => speed_limit_bps(tag, self.speed_ceiling_mbps),
            None => 0.0,
        }
    }

    /// Function 0x835e38 applies the global limit divided by the number of
    /// active connections to each connection.
    pub fn per_connection_speed(&self, connections: u32) -> f64 {
        per_connection_limit(self.speed_limit_bps(), connections)
    }

    pub fn total_queue_size(&self) -> u64 {
        queue_total_bytes(&self.queue)
    }

    pub fn queue_remaining(&self) -> f64 {
        queue_remaining_bytes(&self.queue)
    }

    pub fn connect(&mut self) {
        if self.settings.enabled_servers().is_empty() {
            self.show_message = Some((
                "Information".into(),
                "You don't have enabled servers. Go into setup and enable at least one server."
                    .into(),
            ));
            self.log(
                "Main",
                "You don't have enabled servers. Go into setup and enable at least one server.",
            );
            return;
        }
        self.connected = true;
        self.paused = false;
        self.pause_until = None;
        // Anything that was running when the connection dropped, or that was
        // added while the connection was down, must be queued again so the
        // first tick can start it. Individually paused entries stay paused.
        let resume: Vec<u64> = self.resume_ids.drain(..).collect();
        for id in resume {
            if let Some(c) = self.queue.iter_mut().find(|c| c.id == id) {
                if c.status == ItemStatus::Paused {
                    c.status = ItemStatus::Queued;
                }
            }
        }
        for c in &mut self.queue {
            if c.status == ItemStatus::Downloading {
                c.status = ItemStatus::Queued;
            }
        }
        self.threads = build_threads(true, &self.queue, &self.settings, false, 0);
        let names = self
            .settings
            .enabled_servers()
            .iter()
            .map(|s| format!("{}:{}", s.address, s.port))
            .collect::<Vec<_>>()
            .join(", ");
        self.log("Connection", &format!("Connected to {names}."));
        self.toast("Connected");
    }

    pub fn disconnect(&mut self) {
        self.connected = false;
        self.paused = false;
        self.active_dl = None;
        for c in &mut self.queue {
            if c.status == ItemStatus::Downloading {
                c.status = ItemStatus::Paused;
                // Remember it so a later Connect resumes it automatically.
                if !self.resume_ids.contains(&c.id) {
                    self.resume_ids.push(c.id);
                }
            }
        }
        self.threads.clear();
        self.speed_bps = 0.0;
        self.log("Connection", "Disconnected.");
        self.toast("Disconnected");
    }

    pub fn toggle_connect(&mut self) {
        if self.connected {
            self.disconnect();
        } else {
            self.connect();
        }
    }

    pub fn toggle_pause(&mut self) {
        if self.paused {
            self.resume_paused_ids();
            self.paused = false;
            self.pause_until = None;
            self.pause_minutes = 0;
            self.log("Main", "Downloading resumed.");
            self.toast("Resumed");
        } else {
            self.paused = true;
            for c in &mut self.queue {
                if c.status == ItemStatus::Downloading {
                    c.status = ItemStatus::Paused;
                    if !self.resume_ids.contains(&c.id) {
                        self.resume_ids.push(c.id);
                    }
                }
            }
            self.log("Main", "Downloading paused.");
            self.toast("Paused");
        }
    }

    /// Restore every entry that the global pause put on hold. The item that
    /// already owns a worker is marked Downloading so that worker resumes;
    /// the rest go back to Queued so the scheduler starts them in turn.
    fn resume_paused_ids(&mut self) {
        let active_id = self.active_dl.as_ref().map(|a| a.id);
        for id in self.resume_ids.drain(..).collect::<Vec<_>>() {
            if let Some(c) = self.queue.iter_mut().find(|c| c.id == id) {
                if c.status == ItemStatus::Paused {
                    c.status = if active_id == Some(id) {
                        ItemStatus::Downloading
                    } else {
                        ItemStatus::Queued
                    };
                }
            }
        }
    }

    pub fn pause_for(&mut self, minutes: u32) {
        if minutes == 0 {
            self.paused = false;
            self.pause_until = None;
            self.log("Main", "Pause timer disabled.");
            return;
        }
        if !self.paused {
            self.toggle_pause();
        }
        self.pause_minutes = minutes;
        self.pause_until = Some(Instant::now() + Duration::from_secs(minutes as u64 * 60));
        self.log("Main", &format!("Paused for {minutes} minutes."));
        self.toast(format!("Paused for {minutes} min"));
    }

    pub fn set_speed_limit(&mut self, tag: Option<u8>) {
        self.speed_limit_tag = tag;
        match tag {
            Some(t) => {
                let pct = t * 10;
                let bps = speed_limit_bps(t, self.speed_ceiling_mbps);
                self.log(
                    "Connection",
                    &format!(
                        "Speed limit set to {pct}% ({}).",
                        format_speed(bps, self.status_mode_speed)
                    ),
                );
                self.toast(format!("Speed limit {pct}%"));
            }
            None => {
                self.log("Connection", "Speed limit removed (no limit).");
                self.toast("No limit");
            }
        }
    }

    pub fn pause_selected(&mut self, pause: bool) {
        let Some(sel) = self.selected else { return };
        let active_id = self.active_dl.as_ref().map(|a| a.id);
        let mut changed: Option<(String, bool)> = None;
        if let Some(c) = self.queue.get_mut(sel.collection) {
            if pause {
                if c.status != ItemStatus::Completed {
                    c.status = ItemStatus::Paused;
                    changed = Some((c.subject.clone(), true));
                }
            } else if c.status == ItemStatus::Paused {
                // Only the item that already has a worker should be marked
                // Downloading; anything else goes back to Queued or it would
                // never be picked up by the starter.
                let id = c.id;
                c.status = if active_id == Some(id) {
                    ItemStatus::Downloading
                } else {
                    ItemStatus::Queued
                };
                changed = Some((c.subject.clone(), false));
            }
        }
        if let Some((subject, paused)) = changed {
            self.log(
                "Queue",
                &format!("{}: {subject}", if paused { "Paused" } else { "Unpaused" }),
            );
        }
    }

    pub fn delete_selected(&mut self) {
        let Some(sel) = self.selected else { return };
        if sel.collection < self.queue.len() {
            let c = self.queue.remove(sel.collection);
            self.log("Queue", &format!("Deleted: {}", c.subject));
            self.selected = None;
        }
    }

    pub fn move_selected(&mut self, dir: MoveDir) {
        let Some(sel) = self.selected else { return };
        let i = sel.collection;
        if i >= self.queue.len() {
            return;
        }
        let j = match dir {
            MoveDir::Top => 0,
            MoveDir::Up => i.saturating_sub(1),
            MoveDir::Down => (i + 1).min(self.queue.len() - 1),
            MoveDir::Bottom => self.queue.len() - 1,
        };
        if i != j {
            let item = self.queue.remove(i);
            self.queue.insert(j, item);
            self.selected = Some(Selection {
                collection: j,
                file: None,
            });
        }
    }

    pub fn cleanup(&mut self) {
        let before = self.queue.len();
        self.queue.retain(|c| c.status != ItemStatus::Completed);
        let removed = before - self.queue.len();
        self.par2.retain(|p| p.status != "OK");
        self.log(
            "Main",
            &format!("Cleanup removed {removed} completed queue entries."),
        );
        self.toast(format!("Cleanup: {removed} removed"));
    }

    pub fn toggle_expand(&mut self, index: usize) {
        if let Some(c) = self.queue.get_mut(index) {
            c.expanded = !c.expanded;
        }
    }

    pub fn selected_collection(&self) -> Option<&Collection> {
        self.selected.and_then(|s| self.queue.get(s.collection))
    }

    pub fn export_selected(&mut self) {
        let Some(c) = self.selected_collection().cloned() else {
            self.toast("Select a collection first");
            return;
        };
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("NZB files", &["nzb"])
            .set_file_name(format!("{}.nzb", sanitize_folder(&c.subject)))
            .save_file()
        {
            let mut xml = String::new();
            xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<nzb xmlns=\"http://www.newzbin.com/DTD/2003/nzb\">\n");
            for f in &c.files {
                xml.push_str(&format!(
                    "  <file subject=\"{}\" poster=\"{}\" date=\"0\">\n    <groups><group>{}</group></groups>\n    <segments><segment bytes=\"{}\" number=\"1\">export</segment></segments>\n  </file>\n",
                    xml_escape(&f.name),
                    xml_escape(&c.poster),
                    xml_escape(&c.group),
                    f.bytes
                ));
            }
            xml.push_str("</nzb>\n");
            if std::fs::write(&path, xml).is_ok() {
                self.log("Main", &format!("Exported NZB to {}", path.display()));
                self.toast("NZB exported");
            }
        }
    }

    fn matching_category(&self, subject: &str, group: &str) -> Option<Category> {
        let haystack = format!("{subject}\n{group}").to_ascii_lowercase();
        self.settings.categories.iter().find(|category| {
            category
                .keywords
                .split(['|', ';', ','])
                .map(str::trim)
                .filter(|keyword| !keyword.is_empty())
                .any(|keyword| haystack.contains(&keyword.to_ascii_lowercase()))
        }).cloned()
    }

    fn add_nzb_from_parsed(&mut self, nzb: ParsedNzb, source: String, filter_paused: bool) -> u64 {
        let files: Vec<QueueFile> = nzb
            .files
            .iter()
            .map(|file| QueueFile {
                name: file.name.clone(),
                bytes: file.bytes,
                downloaded: 0.0,
                parts: file.parts,
                segments: file.segments.clone(),
            })
            .collect();
        let id = self.queue.iter().map(|collection| collection.id).max().unwrap_or(0) + 1;
        let paused = self.paused || filter_paused;
        let category = self.matching_category(&nzb.name, &nzb.group);
        let group = if nzb.group.is_empty() {
            "alt.binaries.misc".to_string()
        } else {
            nzb.group.clone()
        };
        let folder = category
            .as_ref()
            .map(|category| category.download_folder.clone())
            .filter(|folder| !folder.trim().is_empty())
            .unwrap_or_else(|| {
                PathBuf::from(&self.settings.download_dir)
                    .join(sanitize_folder(&nzb.name))
                    .to_string_lossy()
                    .to_string()
            });
        let unrar = category.as_ref().map(|category| category.unrar).unwrap_or(true);
        let unrar_folder = category
            .as_ref()
            .map(|category| category.unrar_folder.clone())
            .unwrap_or_default();
        let category_name = category
            .as_ref()
            .map(|category| category.name.clone())
            .unwrap_or_else(|| "Default".into());
        let collection = Collection {
            id,
            subject: nzb.name.clone(),
            total_bytes: nzb.total_bytes.max(1),
            downloaded: 0.0,
            parts: nzb.parts,
            age_days: nzb.age_days,
            group,
            poster: if nzb.poster.is_empty() {
                "unknown".into()
            } else {
                nzb.poster.clone()
            },
            status: if paused {
                ItemStatus::Paused
            } else {
                ItemStatus::Queued
            },
            folder,
            unrar_folder,
            unrar,
            execute: String::new(),
            execute_done: false,
            expanded: true,
            files,
            posted_on: nzb.posted_on.clone(),
            delete_archives: false,
            category: category_name,
            password: String::new(),
            source,
        };
        let subject = collection.subject.clone();
        if self.paused && !self.resume_ids.contains(&id) {
            self.resume_ids.push(id);
        }
        self.queue.push(collection);
        self.log("Queue", &format!("Added: {subject}"));
        self.toast("Added to queue");
        id
    }

    pub fn import_nzb(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("NZB files", &["nzb", "NZB"])
            .pick_file()
        else {
            return;
        };
        match parse_nzb(&path) {
            Ok(nzb) => {
                let files: Vec<QueueFile> = nzb
                    .files
                    .iter()
                    .map(|f| QueueFile {
                        name: f.name.clone(),
                        bytes: f.bytes,
                        downloaded: 0.0,
                        parts: f.parts,
                        segments: f.segments.clone(),
                    })
                    .collect();
                let id = self.queue.iter().map(|c| c.id).max().unwrap_or(0) + 1;
                let paused = self.paused;
                let category = self.matching_category(&nzb.name, &nzb.group);
                let group = if nzb.group.is_empty() {
                    "alt.binaries.misc".to_string()
                } else {
                    nzb.group.clone()
                };
                let folder = category
                    .as_ref()
                    .map(|category| category.download_folder.clone())
                    .filter(|folder| !folder.trim().is_empty())
                    .unwrap_or_else(|| {
                        PathBuf::from(&self.settings.download_dir)
                            .join(sanitize_folder(&nzb.name))
                            .to_string_lossy()
                            .to_string()
                    });
                let unrar = category.as_ref().map(|category| category.unrar).unwrap_or(true);
                let unrar_folder = category
                    .as_ref()
                    .map(|category| category.unrar_folder.clone())
                    .unwrap_or_default();
                let category_name = category
                    .as_ref()
                    .map(|category| category.name.clone())
                    .unwrap_or_else(|| "Default".into());
                let collection = Collection {
                    id,
                    subject: nzb.name.clone(),
                    total_bytes: nzb.total_bytes.max(1),
                    downloaded: 0.0,
                    parts: nzb.parts,
                    age_days: nzb.age_days,
                    group,
                    poster: if nzb.poster.is_empty() {
                        "unknown".into()
                    } else {
                        nzb.poster.clone()
                    },
                    status: if paused {
                        ItemStatus::Paused
                    } else {
                        ItemStatus::Queued
                    },
                    folder,
                    unrar_folder,
                    unrar,
                    execute: String::new(),
                    execute_done: false,
                    expanded: true,
                    files,
                    posted_on: nzb.posted_on.clone(),
                    delete_archives: false,
                    category: category_name,
                    password: String::new(),
                    source: path.to_string_lossy().to_string(),
                };
                let subject = collection.subject.clone();
                if paused && !self.resume_ids.contains(&id) {
                    // A globally paused import must resume with the global
                    // Unpause, otherwise it would sit at Paused forever.
                    self.resume_ids.push(id);
                }
                self.queue.push(collection);
                self.log("Queue", &format!("Imported NZB: {subject}"));
                self.toast("NZB imported");
                if self.settings.delete_nzb_after_import {
                    let _ = std::fs::remove_file(&path);
                }
            }
            Err(e) => {
                self.show_message = Some(("Error".into(), format!("Could not import NZB:\n{e}")));
                self.log("Error", &format!("NZB import failed: {e}"));
            }
        }
    }

    pub fn open_download_folder(&mut self) {
        let path = PathBuf::from(&self.settings.download_dir);
        let _ = std::fs::create_dir_all(&path);
        #[cfg(target_os = "windows")]
        {
            let _ = std::process::Command::new("explorer").arg(path).spawn();
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = std::process::Command::new("xdg-open").arg(path).spawn();
        }
        self.toast("Opening download folder");
    }

    pub fn open_unrar_folder(&mut self) {
        let path = PathBuf::from(&self.settings.unrar_dir);
        let _ = std::fs::create_dir_all(&path);
        #[cfg(target_os = "windows")]
        {
            let _ = std::process::Command::new("explorer").arg(path).spawn();
        }
        self.toast("Opening unrar folder");
    }

    pub fn explorer_navigate(&mut self, path: PathBuf) {
        if path == self.explorer_path {
            return;
        }
        if !path.is_dir() {
            self.toast("Folder does not exist");
            return;
        }
        self.explorer_history.push(self.explorer_path.clone());
        self.explorer_forward.clear();
        self.explorer_path = path;
        self.explorer_path_edit = self.explorer_path.to_string_lossy().to_string();
        self.explorer_selected = None;
        self.explorer_selected_set.clear();
        self.explorer_anchor = None;
    }

    pub fn explorer_back(&mut self) {
        if let Some(previous) = self.explorer_history.pop() {
            let current = std::mem::replace(&mut self.explorer_path, previous);
            self.explorer_forward.push(current);
            self.explorer_path_edit = self.explorer_path.to_string_lossy().to_string();
            self.explorer_selected = None;
            self.explorer_selected_set.clear();
            self.explorer_anchor = None;
        }
    }

    pub fn explorer_forward(&mut self) {
        if let Some(next) = self.explorer_forward.pop() {
            let current = std::mem::replace(&mut self.explorer_path, next);
            self.explorer_history.push(current);
            self.explorer_path_edit = self.explorer_path.to_string_lossy().to_string();
            self.explorer_selected = None;
            self.explorer_selected_set.clear();
            self.explorer_anchor = None;
        }
    }

    pub fn explorer_up(&mut self) {
        if let Some(parent) = self.explorer_path.parent().map(Path::to_path_buf) {
            self.explorer_navigate(parent);
        }
    }

    pub fn explorer_open_entry(&mut self, path: PathBuf, is_dir: bool) {
        if is_dir {
            self.explorer_navigate(path);
        } else {
            shell_open(&path);
            self.log("Explorer", &format!("Opened {}", path.display()));
        }
    }

    pub fn explorer_open_containing(&mut self, path: PathBuf) {
        if let Some(parent) = path.parent() {
            shell_open(parent);
            self.log("Explorer", &format!("Opened {}", parent.display()));
        }
    }

    pub fn explorer_join_selected(&mut self) {
        let mut paths = self.explorer_selected_paths();
        if paths.len() < 2 {
            self.toast("Select at least two files to join");
            return;
        }
        files::sort_split_paths(&mut paths);
        let Some(output) = rfd::FileDialog::new()
            .set_file_name("joined.bin")
            .save_file()
        else {
            return;
        };
        match files::join_files(&paths, &output) {
            Ok(bytes) => {
                self.log(
                    "Explorer",
                    &format!("Joined {} files into {} ({} bytes)", paths.len(), output.display(), bytes),
                );
                self.toast("Files joined");
            }
            Err(error) => self.show_message = Some(("Join".into(), error)),
        }
    }

    pub fn explorer_split_file(&mut self, path: PathBuf) {
        self.split_file_target = Some(path);
        self.show_split_file = true;
    }

    fn explorer_selected_paths(&self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = self.explorer_selected_set.iter().cloned().collect();
        if paths.is_empty() {
            if let Some(path) = &self.explorer_selected {
                paths.push(path.clone());
            }
        }
        paths.sort();
        paths
    }

    pub fn explorer_copy_selected(&mut self, cut: bool) {
        let paths = self.explorer_selected_paths();
        if paths.is_empty() {
            self.toast("Select an item first");
            return;
        }
        let count = paths.len();
        self.explorer_clipboard = Some((paths, cut));
        self.toast(if cut {
            format!("Cut {count} item(s)")
        } else {
            format!("Copied {count} item(s)")
        });
        self.log(
            "Explorer",
            &format!("{} {count} item(s)", if cut { "Cut" } else { "Copied" }),
        );
    }

    pub fn explorer_paste(&mut self) {
        let Some((sources, cut)) = self.explorer_clipboard.clone() else {
            return;
        };
        let mut pasted = Vec::new();
        let mut failures = Vec::new();
        for source in sources {
            if !source.exists() {
                failures.push(format!("{} no longer exists", source.display()));
                continue;
            }
            let Some(name) = source.file_name() else {
                continue;
            };
            let destination = unique_destination(self.explorer_path.join(name));
            if destination == source {
                continue;
            }
            let result = if cut {
                std::fs::rename(&source, &destination)
            } else if source.is_dir() {
                copy_directory(&source, &destination)
            } else {
                std::fs::copy(&source, &destination).map(|_| ())
            };
            match result {
                Ok(()) => pasted.push(destination),
                Err(error) => failures.push(format!("{}: {error}", source.display())),
            }
        }
        if cut {
            self.explorer_clipboard = None;
        }
        if !pasted.is_empty() {
            self.explorer_selected_set = pasted.iter().cloned().collect();
            self.explorer_selected = pasted.first().cloned();
            self.explorer_anchor = None;
            self.log("Explorer", &format!("Pasted {} item(s)", pasted.len()));
            self.toast(format!("Pasted {} item(s)", pasted.len()));
        }
        if !failures.is_empty() {
            self.show_message = Some(("Explorer".into(), format!("Paste failed:\n{}", failures.join("\n"))));
        }
    }

    pub fn explorer_drop_paths(&mut self, paths: Vec<PathBuf>) {        let mut copied = 0;
        for source in paths {
            if !source.exists() {
                continue;
            }
            let Some(name) = source.file_name() else {
                continue;
            };
            let destination = unique_destination(self.explorer_path.join(name));
            let result = if source.is_dir() {
                copy_directory(&source, &destination)
            } else {
                std::fs::copy(&source, &destination).map(|_| ())
            };
            match result {
                Ok(()) => copied += 1,
                Err(error) => self.log(
                    "Error",
                    &format!("Explorer drop failed for {}: {error}", source.display()),
                ),
            }
        }
        if copied > 0 {
            self.toast(format!("Added {copied} item(s)"));
            self.log("Explorer", &format!("Dropped {copied} item(s)"));
        }
    }
    pub fn explorer_open_with(&mut self, path: PathBuf) {        let Some(program) = rfd::FileDialog::new()
            .add_filter("Programs", &["exe", "com", "bat", "cmd"])
            .pick_file()
        else {
            return;
        };
        match std::process::Command::new(&program).arg(&path).spawn() {
            Ok(_) => self.log(
                "Explorer",
                &format!("Opened {} with {}", path.display(), program.display()),
            ),
            Err(error) => {
                self.show_message = Some((
                    "Explorer".into(),
                    format!("Could not open file:\n{error}"),
                ));
            }
        }
    }

    pub fn explorer_rename(&mut self, path: PathBuf) {
        self.explorer_rename_draft = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        self.explorer_rename_target = Some(path);
        self.show_explorer_rename = true;
    }

    pub fn explorer_new_folder(&mut self) {
        let mut name = "New folder".to_string();
        let mut index = 1;
        while self.explorer_path.join(&name).exists() {
            name = format!("New folder ({index})");
            index += 1;
        }
        let path = self.explorer_path.join(&name);
        match std::fs::create_dir(&path) {
            Ok(()) => {
                self.explorer_selected = Some(path.clone());
                self.explorer_selected_set.clear();
                self.explorer_selected_set.insert(path.clone());
                self.explorer_anchor = None;
                self.log("Explorer", &format!("Created {}", path.display()));
                self.toast("Folder created");
            }
            Err(error) => {
                self.show_message = Some(("Explorer".into(), format!("Could not create folder:\n{error}")));
            }
        }
    }

    pub fn explorer_properties(&mut self, path: PathBuf) {
        let metadata = std::fs::metadata(&path);
        let mut lines = vec![path.display().to_string()];
        match metadata {
            Ok(meta) => {
                if meta.is_dir() {
                    lines.push("Type: Folder".into());
                } else {
                    lines.push(format!("Size: {} bytes", meta.len()));
                    lines.push("Type: File".into());
                }
                if let Ok(modified) = meta.modified() {
                    let modified = chrono::DateTime::<chrono::Local>::from(modified);
                    lines.push(format!("Modified: {}", modified.format("%Y-%m-%d %H:%M")));
                }
                if meta.permissions().readonly() {
                    lines.push("Attributes: Read-only".into());
                }
            }
            Err(error) => lines.push(format!("Cannot read metadata: {error}")),
        }
        self.show_message = Some(("Properties".into(), lines.join("\n")));
    }

    pub fn explorer_delete_selected(&mut self) {
        let paths = self.explorer_selected_paths();
        if paths.is_empty() {
            return;
        }
        let description = if paths.len() == 1 {
            format!(
                "Delete {}?",
                paths[0]
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| paths[0].display().to_string())
            )
        } else {
            format!("Delete {} selected items?", paths.len())
        };
        let answer = rfd::MessageDialog::new()
            .set_title("Delete")
            .set_description(description)
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if !matches!(answer, rfd::MessageDialogResult::Yes) {
            return;
        }
        let mut deleted = 0;
        let mut failures = Vec::new();
        for path in &paths {
            let result = match std::fs::symlink_metadata(path) {
                Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
                Ok(_) => std::fs::remove_file(path),
                Err(error) => Err(error),
            };
            match result {
                Ok(()) => deleted += 1,
                Err(error) => failures.push(format!("{}: {error}", path.display())),
            }
        }
        self.explorer_selected = None;
        self.explorer_selected_set.clear();
        self.explorer_anchor = None;
        self.log("Explorer", &format!("Deleted {deleted} item(s)"));
        self.toast(format!("Deleted {deleted} item(s)"));
        if !failures.is_empty() {
            self.show_message = Some((
                "Explorer".into(),
                format!("Some items could not be deleted:\n{}", failures.join("\n")),
            ));
        }
    }

    pub fn refresh_rss(&mut self) {
        if self.rss_rx.is_some() {
            self.toast("RSS refresh already running");
            return;
        }
        if self.settings.rss_urls.is_empty() {
            self.show_rss_feeds = true;
            self.rss_feed_draft = String::new();
            self.toast("Add an RSS feed first");
            return;
        }
        let urls = self.settings.rss_urls.clone();
        let filters = self.settings.rss_filters.clone();
        let nzb_dir = PathBuf::from(&self.settings.nzb_dir);
        let existing: Vec<String> = self
            .queue
            .iter()
            .map(|collection| collection.source.clone())
            .collect();
        let count = urls.len();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(rss::fetch_and_download(
                &urls,
                &filters,
                &nzb_dir,
                &existing,
            ));
        });
        self.rss_rx = Some(rx);
        self.rss_last_refresh = Instant::now();
        self.log("RSS", &format!("Refreshing {count} feed(s)."));
    }

    fn poll_rss(&mut self) {
        let Some(rx) = self.rss_rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(result)) => {
                let count = result.items.len();
                self.rss = result.items;
                for error in &result.errors {
                    self.log("Error", &format!("RSS autodownload: {error}"));
                }
                for download in result.downloads {
                    self.add_nzb_from_parsed(download.nzb, download.source, download.paused);
                }
                self.log("RSS", &format!("Loaded {count} RSS item(s)."));
                if result.errors.is_empty() {
                    self.toast(format!("RSS: {count} items"));
                } else {
                    self.toast(format!(
                        "RSS: {count} items, {} download error(s)",
                        result.errors.len()
                    ));
                }
            }
            Ok(Err(error)) => {
                self.log("Error", &format!("RSS refresh failed: {error}"));
                self.toast("RSS refresh failed");
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => self.rss_rx = Some(rx),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.log("Error", "RSS worker stopped unexpectedly.");
            }
        }
    }

    fn auto_import_nzbs(&mut self) {
        let folder = PathBuf::from(&self.settings.auto_import_folder);
        if folder.as_os_str().is_empty() {
            return;
        }
        let Ok(reader) = std::fs::read_dir(&folder) else {
            return;
        };
        let mut paths: Vec<PathBuf> = reader
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .map(|extension| extension.eq_ignore_ascii_case("nzb"))
                        .unwrap_or(false)
            })
            .collect();
        paths.sort();
        for path in paths {
            let source = path.to_string_lossy().to_string();
            if self.queue.iter().any(|collection| collection.source == source) {
                continue;
            }
            match parse_nzb(&path) {
                Ok(nzb) => {
                    self.add_nzb_from_parsed(nzb, source, self.settings.auto_import_paused);
                    if self.settings.delete_nzb_after_import {
                        let _ = std::fs::remove_file(&path);
                    }
                }
                Err(error) => {
                    self.log("Error", &format!("Auto-import failed for {}: {error}", path.display()));
                }
            }
        }
    }

    pub fn run_execute_hook(&mut self, id: u64) {
        let Some(index) = self.queue.iter().position(|collection| collection.id == id) else {
            return;
        };
        if self.queue[index].execute_done {
            return;
        }
        let command = if self.queue[index].execute.trim().is_empty() {
            self.settings.default_execute.clone()
        } else {
            self.queue[index].execute.clone()
        };
        if command.trim().is_empty() {
            self.queue[index].execute_done = true;
            return;
        }
        let folder = self.queue[index].folder.clone();
        let subject = self.queue[index].subject.clone();
        let first_file = self.queue[index].files.iter().find_map(|file| {
            let path = PathBuf::from(&folder).join(&file.name);
            path.is_file().then(|| path.to_string_lossy().to_string())
        });
        let values = [
            ("{folder}", "NQ_FOLDER", folder.clone()),
            ("{name}", "NQ_NAME", subject.clone()),
            ("{file}", "NQ_FILE", first_file.clone().unwrap_or_default()),
        ];
        // What the log shows; the shell never sees these values as text.
        let display = values
            .iter()
            .fold(command.clone(), |text, (placeholder, _, value)| text.replace(placeholder, value));
        let shell_command = hook_shell_command(&command, cfg!(target_os = "windows"));
        let mut process = if cfg!(target_os = "windows") {
            let mut process = std::process::Command::new("cmd");
            process.args(["/D", "/S", "/C"]);
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                // `/S /C "<line>"` strips exactly the outer quotes, so the line
                // reaches cmd unchanged instead of with Rust's `\"` escaping.
                process.raw_arg(format!("\"{shell_command}\""));
            }
            process
        } else {
            let mut process = std::process::Command::new("sh");
            process.args(["-c", &shell_command]);
            process
        };
        for (_, variable, value) in &values {
            process.env(variable, hook_env_value(value));
        }
        process.current_dir(&folder);
        #[cfg(target_os = "windows")]
        if self.settings.hide_execute_window {
            use std::os::windows::process::CommandExt;
            process.creation_flags(0x0800_0000);
        }
        match process.spawn() {
            Ok(_) => {
                self.queue[index].execute_done = true;
                self.log("Execute", &format!("Started for {subject}: {display}"));
            }
            Err(error) => {
                self.log("Error", &format!("Execute failed for {subject}: {error}"));
                self.show_message = Some(("Execute".into(), format!("Could not start command:\n{error}")));
            }
        }
    }

    /// Start a PAR2 check for a completed collection. Returns true when a
    /// worker was started.
    pub fn start_par2(&mut self, id: u64) -> bool {
        if self.active_par2.is_some() {
            self.toast("PAR2 check already running");
            return false;
        }
        let Some(index) = self.queue.iter().position(|c| c.id == id) else {
            return false;
        };
        let collection = self.queue[index].clone();
        let source = PathBuf::from(&collection.folder);
        let expected: Vec<String> = collection
            .files
            .iter()
            .map(|file| file.name.clone())
            .collect();
        let Some(par2_file) = par2::find_par2_file(&source, &expected) else {
            return false;
        };
        self.ensure_par2_set(&collection, &par2_file);
        self.queue[index].status = ItemStatus::Repairing;
        self.log(
            "PAR2",
            &format!(
                "Checking {} with {}",
                collection.subject,
                par2_file
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ),
        );
        self.toast("Checking PAR2...");
        self.active_par2 = Some(par2::spawn(
            id,
            source,
            par2_file,
            par2::Par2Options::from_settings(&self.settings),
        ));
        true
    }

    fn ensure_par2_set(&mut self, collection: &Collection, par2_file: &Path) {
        let name = par2_file
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "collection.par2".into());
        let size_mb = std::fs::metadata(par2_file)
            .map(|meta| meta.len() as f64 / MB)
            .unwrap_or(0.0);
        if let Some(set) = self.par2.iter_mut().find(|set| {
            set.collection_id == Some(collection.id)
                || (set.folder == collection.folder && set.name.eq_ignore_ascii_case(&name))
        }) {
            set.name = name;
            set.size_mb = size_mb;
            set.unrar = collection.unrar;
            set.folder = collection.folder.clone();
            set.collection_id = Some(collection.id);
            set.status = "Checking".into();
            set.info = "Queued".into();
            set.progress = 0.0;
        } else {
            self.par2.push(Par2Set {
                name,
                size_mb,
                blocks: 0,
                info: "Queued".into(),
                unrar: collection.unrar,
                progress: 0.0,
                status: "Checking".into(),
                folder: collection.folder.clone(),
                collection_id: Some(collection.id),
            });
        }
    }

    pub fn start_par2_for_set(&mut self, index: usize) {
        let Some(set) = self.par2.get(index).cloned() else {
            return;
        };
        if let Some(id) = set.collection_id {
            self.start_par2(id);
            return;
        }
        if set.folder.is_empty() {
            self.toast("No collection linked to this PAR2 set");
            return;
        }
        if self.active_par2.is_some() {
            self.toast("PAR2 check already running");
            return;
        }
        let source = PathBuf::from(&set.folder);
        let expected = vec![set.name.clone()];
        let Some(par2_file) = par2::find_par2_file(&source, &expected) else {
            self.toast("PAR2 file not found");
            return;
        };
        self.par2[index].status = "Repairing".into();
        self.par2[index].info = "Checking".into();
        self.par2[index].progress = 0.0;
        self.log("PAR2", &format!("Checking {}", set.name));
        self.active_par2 = Some(par2::spawn(
            0,
            source,
            par2_file,
            par2::Par2Options::from_settings(&self.settings),
        ));
    }

    fn poll_par2(&mut self) {
        let Some(active) = self.active_par2.take() else {
            return;
        };
        match par2::poll(&active) {
            None => self.active_par2 = Some(active),
            Some(Ok(message)) => {
                if let Some(collection) = self
                    .queue
                    .iter_mut()
                    .find(|collection| collection.id == active.collection_id)
                {
                    collection.status = ItemStatus::Completed;
                }
                for set in &mut self.par2 {
                    if set.collection_id == Some(active.collection_id)
                        || (active.collection_id == 0 && set.status == "Repairing")
                    {
                        set.status = if message.contains("repaired") {
                            "Repaired".into()
                        } else {
                            "OK".into()
                        };
                        set.info = message.clone();
                        set.progress = 1.0;
                    }
                }
                self.log("PAR2", &message);
                self.toast("PAR2 check complete");
                let auto_unrar = active.collection_id != 0
                    && self.settings.auto_unrar
                    && self
                        .queue
                        .iter()
                        .find(|collection| collection.id == active.collection_id)
                        .map(|collection| collection.unrar)
                        .unwrap_or(false);
                if auto_unrar {
                    if self.start_unrar(active.collection_id) {
                        return;
                    }
                }
                self.run_execute_hook(active.collection_id);
            }
            Some(Err(error)) => {
                if let Some(collection) = self
                    .queue
                    .iter_mut()
                    .find(|collection| collection.id == active.collection_id)
                {
                    collection.status = ItemStatus::Incomplete;
                }
                for set in &mut self.par2 {
                    if set.collection_id == Some(active.collection_id)
                        || (active.collection_id == 0 && set.status == "Repairing")
                    {
                        set.status = "Incomplete".into();
                        set.info = error.clone();
                    }
                }
                self.log("PAR2", &error);
                self.show_message = Some(("PAR2".into(), error));
            }
        }
    }

    fn schedule_after_download(&mut self, id: u64) {
        let check_par2 = self.settings.par2_checking
            && self
                .queue
                .iter()
                .find(|collection| collection.id == id)
                .map(|collection| {
                    let expected: Vec<String> = collection
                        .files
                        .iter()
                        .map(|file| file.name.clone())
                        .collect();
                    par2::has_par2(Path::new(&collection.folder), &expected)
                })
                .unwrap_or(false);
        if check_par2 && self.start_par2(id) {
            return;
        }
        if self.settings.auto_unrar
            && self
                .queue
                .iter()
                .find(|collection| collection.id == id)
                .map(|collection| collection.unrar)
                .unwrap_or(false)
            && self.start_unrar(id)
        {
            return;
        }
        self.run_execute_hook(id);
    }

    /// Start unpacking a completed collection. Returns true when a worker was
    /// started or the request was queued; false when there is nothing to unpack.
    pub fn start_unrar(&mut self, id: u64) -> bool {
        if self.active_unrar.is_some() {
            if !self.pending_unrar.contains(&id) {
                self.pending_unrar.push_back(id);
            }
            return true;
        }
        let Some(index) = self.queue.iter().position(|c| c.id == id) else {
            return false;
        };
        let collection = self.queue[index].clone();
        if !matches!(
            collection.status,
            ItemStatus::Completed | ItemStatus::Incomplete | ItemStatus::Error
        ) {
            return false;
        }
        let source = PathBuf::from(&collection.folder);
        let options = unrar::UnrarOptions::from_collection(&self.settings, &collection);
        if !unrar::has_archive(&source, &options.expected_files) {
            self.log("Unrar", &format!("Nothing to unrar for {}", collection.subject));
            self.toast("No RAR archive found");
            return false;
        }
        let dest = unrar::target_dir(&self.settings, &collection);
        self.queue[index].status = ItemStatus::Unpacking;
        if self.settings.pause_while_unraring && self.connected && !self.paused {
            self.toggle_pause();
            self.resume_after_unrar = true;
        }
        self.log(
            "Unrar",
            &format!("Unpack started: {} -> {}", collection.subject, dest.display()),
        );
        self.toast("Unpacking...");
        self.active_unrar = Some(unrar::spawn(id, source, dest, options));
        true
    }

    pub fn submit_unrar_password(&mut self) {
        let Some(id) = self.unrar_password_target.take() else {
            self.show_unrar_password = false;
            return;
        };
        if self.unrar_password_draft.trim().is_empty() {
            self.unrar_password_target = Some(id);
            self.toast("Enter an unpack password");
            return;
        }
        let password = self.unrar_password_draft.clone();
        self.show_unrar_password = false;
        let subject = self
            .queue
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.subject.clone())
            .unwrap_or_else(|| format!("collection {id}"));
        if let Some(collection) = self.queue.iter_mut().find(|c| c.id == id) {
            collection.password = password;
            collection.status = ItemStatus::Completed;
        }
        self.log(
            "Unrar",
            &format!("Retrying unpack with the supplied password: {subject}"),
        );
        self.start_unrar(id);
    }

    pub fn cancel_unrar_password(&mut self) {
        self.show_unrar_password = false;
        let Some(id) = self.unrar_password_target.take() else {
            return;
        };
        let subject = self
            .queue
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.subject.clone())
            .unwrap_or_else(|| format!("collection {id}"));
        if let Some(collection) = self.queue.iter_mut().find(|c| c.id == id) {
            collection.status = ItemStatus::Error;
        }
        self.log(
            "Unrar",
            &format!("Unpack cancelled: a password is required for {subject}."),
        );
        self.show_message = Some((
            "Unrar password".into(),
            format!("{subject}\n\nUnpack cancelled because no password was supplied."),
        ));
    }

    pub fn unpack_selected(&mut self) {
        let Some(sel) = self.selected else {
            self.toast("Select a queue item first");
            return;
        };
        if let Some(c) = self.queue.get(sel.collection) {
            let id = c.id;
            self.start_unrar(id);
        }
    }

    fn poll_unrar(&mut self) {
        let Some(active) = self.active_unrar.take() else {
            if let Some(id) = self.pending_unrar.pop_front() {
                self.start_unrar(id);
            }
            return;
        };
        match unrar::poll(&active) {
            None => self.active_unrar = Some(active),
            Some(Ok(message)) => {
                if let Some(c) = self.queue.iter_mut().find(|c| c.id == active.collection_id) {
                    c.status = ItemStatus::Completed;
                }
                let subject = self
                    .queue
                    .iter()
                    .find(|c| c.id == active.collection_id)
                    .map(|c| c.subject.clone())
                    .unwrap_or_default();
                self.log("Unrar", &message);
                self.toast("Unpack complete");
                tray::notify("Unpack complete", &subject);
                self.run_execute_hook(active.collection_id);
            }
            Some(Err(unrar::UnrarError::PasswordRequired)) => {
                if let Some(c) = self.queue.iter_mut().find(|c| c.id == active.collection_id) {
                    c.status = ItemStatus::Completed;
                }
                self.unrar_password_target = Some(active.collection_id);
                self.unrar_password_draft.clear();
                self.show_unrar_password = true;
                self.log(
                    "Unrar",
                    &format!("Password required for collection {}.", active.collection_id),
                );
                self.toast("Unpack password required");
            }
            Some(Err(unrar::UnrarError::Failed(error))) => {
                if let Some(c) = self.queue.iter_mut().find(|c| c.id == active.collection_id) {
                    c.status = ItemStatus::Error;
                }
                self.log("Error", &error);
                self.show_message = Some(("Unrar error".into(), error));
            }
        }
        if self.resume_after_unrar {
            self.resume_after_unrar = false;
            if self.paused {
                self.toggle_pause();
            }
        }
        if self.active_unrar.is_none() {
            if let Some(id) = self.pending_unrar.pop_front() {
                self.start_unrar(id);
            }
        }
    }

    pub fn apply_ui_action(&mut self, action: UiAction) {
        match action {
            UiAction::Select(s) => self.selected = s,
            UiAction::ToggleExpand(i) => self.toggle_expand(i),
            UiAction::PauseSelected(p) => self.pause_selected(p),
            UiAction::DeleteSelected => self.delete_selected(),
            UiAction::Move(d) => self.move_selected(d),
            UiAction::ChangeFolder => {
                let mut changed = None;
                if let Some(sel) = self.selected {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        if let Some(c) = self.queue.get_mut(sel.collection) {
                            c.folder = path.to_string_lossy().to_string();
                            changed = Some(c.subject.clone());
                        }
                    }
                }
                if let Some(subject) = changed {
                    self.log("Queue", &format!("Download folder changed for {subject}"));
                }
            }
            UiAction::Export => self.export_selected(),
            UiAction::SetCategory => {
                if let Some(sel) = self.selected {
                    if let Some(c) = self.queue.get_mut(sel.collection) {
                        if self.settings.categories.is_empty() {
                            c.category = "Default".into();
                        } else {
                            let current = self
                                .settings
                                .categories
                                .iter()
                                .position(|category| category.name == c.category)
                                .unwrap_or(usize::MAX);
                            let next = if current == usize::MAX {
                                0
                            } else {
                                (current + 1) % self.settings.categories.len()
                            };
                            let category = &self.settings.categories[next];
                            c.category = category.name.clone();
                            if !category.download_folder.trim().is_empty() {
                                c.folder = category.download_folder.clone();
                            }
                            c.unrar = category.unrar;
                            if !category.unrar_folder.trim().is_empty() {
                                c.unrar_folder = category.unrar_folder.clone();
                            }
                        }
                    }
                }
                self.toast("Category changed");
            }
            UiAction::ToggleUnrar(v) => {
                if let Some(sel) = self.selected {
                    let mut start = false;
                    if let Some(c) = self.queue.get_mut(sel.collection) {
                        c.unrar = v;
                        start = v && c.status == ItemStatus::Completed;
                    }
                    if start {
                        if let Some(c) = self.queue.get(sel.collection) {
                            let id = c.id;
                            self.start_unrar(id);
                        }
                    }
                }
            }
            UiAction::UnpackNow => self.unpack_selected(),
            UiAction::ChangeUnrarFolder => {
                if let Some(sel) = self.selected {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        if let Some(c) = self.queue.get_mut(sel.collection) {
                            c.unrar_folder = path.to_string_lossy().to_string();
                            let subject = c.subject.clone();
                            self.log("Queue", &format!("Unrar folder changed for {subject}"));
                        }
                    }
                }
            }
            UiAction::SetExecute => {
                if let Some(sel) = self.selected {
                    if let Some(c) = self.queue.get(sel.collection) {
                        self.execute_command_draft = c.execute.clone();
                        self.execute_command_target = Some(sel.collection);
                        self.show_execute_command = true;
                    }
                }
            }
            UiAction::JoinSplitParts => {
                if let Some(collection) = self.selected_collection() {
                    let folder = PathBuf::from(&collection.folder);
                    match files::join_split_parts(&folder) {
                        Ok(Some(path)) => {
                            self.log("Queue", &format!("Joined split parts to {}", path.display()));
                            self.toast("Parts joined");
                        }
                        Ok(None) => self.toast("No numbered split parts found"),
                        Err(error) => self.show_message = Some(("Join".into(), error)),
                    }
                }
            }
            UiAction::SplitSelectedFile => {
                if let Some(collection) = self.selected_collection() {
                    let target = collection.files.iter().find_map(|file| {
                        let path = PathBuf::from(&collection.folder).join(&file.name);
                        path.is_file().then_some(path)
                    });
                    if let Some(target) = target {
                        self.split_file_target = Some(target);
                        self.show_split_file = true;
                    } else {
                        self.toast("No downloaded file to split");
                    }
                }
            }
            UiAction::OpenFolder => self.open_download_folder(),
            UiAction::Play => {
                let Some(collection) = self.selected_collection() else {
                    self.toast("Select a queue item first");
                    return;
                };
                if self.settings.player_exe.trim().is_empty() {
                    self.toast("No media player configured");
                    return;
                }
                let path = collection.files.iter().find_map(|file| {
                    let path = PathBuf::from(&collection.folder).join(&file.name);
                    path.is_file().then_some(path)
                });
                let Some(path) = path else {
                    self.toast("No downloaded file to play");
                    return;
                };
                match std::process::Command::new(&self.settings.player_exe)
                    .arg(&path)
                    .spawn()
                {
                    Ok(_) => self.log("Main", &format!("Playing {}", path.display())),
                    Err(error) => self.toast(format!("Cannot start player: {error}")),
                }
            }
            UiAction::Eta => {
                if let Some(c) = self.selected_collection() {
                    let msg = if self.speed_bps > 0.0 {
                        format!(
                            "{}: ETA {}",
                            c.subject,
                            format_eta(c.remaining() / self.speed_bps)
                        )
                    } else {
                        format!("{}: ETA Unknown", c.subject)
                    };
                    self.toast(msg);
                }
            }
            UiAction::Rename => {
                if let Some(sel) = self.selected {
                    let subject = self.queue.get(sel.collection).map(|c| c.subject.clone());
                    if let Some(subject) = subject {
                        self.rename_draft = subject;
                        self.rename_index = Some(sel.collection);
                        self.show_rename = true;
                    }
                }
            }
            UiAction::CollapseAll => {
                for c in &mut self.queue {
                    c.expanded = false;
                }
            }
            UiAction::ExpandAll => {
                for c in &mut self.queue {
                    c.expanded = true;
                }
            }
            UiAction::None => {}
        }
    }

    fn all_tasks_done(&self) -> bool {
        self.queue
            .iter()
            .all(|c| matches!(c.status, ItemStatus::Completed | ItemStatus::Error))
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_tick).as_secs_f64().clamp(0.0, 0.5);
        self.last_tick = now;
        tray::set_options(self.settings.speed_in_tray, self.settings.tray_balloon);
        self.poll_unrar();
        self.poll_par2();
        self.poll_rss();
        if self.rss_timer
            && self.rss_rx.is_none()
            && now.duration_since(self.rss_last_refresh)
                >= Duration::from_secs(self.settings.rss_refresh_minutes.max(1) as u64 * 60)
        {
            self.refresh_rss();
        }
        if now.duration_since(self.last_auto_import) >= Duration::from_secs(30) {
            self.last_auto_import = now;
            self.auto_import_nzbs();
        }

        // Keep the limit reference in step with Setup. A maximum of 0 means
        // Auto (language hint `[0-10000] 0-Auto`), so the measured ceiling is
        // allowed to grow during the calibration window.
        self.auto_speed = self.settings.max_speed_kbps <= 0.0;
        if !self.auto_speed
            && (self.speed_ceiling_mbps - self.settings.max_speed_mbps).abs() > 1e-9
        {
            self.speed_ceiling_mbps = self.settings.max_speed_mbps;
        }

        if let Some(until) = self.pause_until {
            if now >= until {
                self.paused = false;
                self.pause_until = None;
                self.pause_minutes = 0;
                self.resume_paused_ids();
                self.log("Main", "Pause timer expired; downloading resumed.");
            }
        }

        self.scheduler_hold = self.connected && !scheduler_allows(&self.settings) && !self.paused;
        if self.scheduler_hold && !self.scheduler_last_hold {
            self.log("Scheduler", "Scheduler window is off; downloading held.");
        } else if !self.scheduler_hold && self.scheduler_last_hold {
            self.log("Scheduler", "Scheduler window is on; downloading allowed.");
        }
        self.scheduler_last_hold = self.scheduler_hold;

        let free_mb = free_gb(&self.settings.download_dir) * 1024.0;
        if self.connected && free_mb < self.settings.low_free_space_mb as f64 {
            if !self.low_space_warned {
                self.low_space_warned = true;
                self.log(
                    "Error",
                    "Error! Free space on destination drive is under configured value. Downloading will be paused.",
                );
                self.log("Main", "Downloading paused.");
                if !self.paused {
                    self.toggle_pause();
                } else {
                    self.paused = true;
                }
            }
        } else {
            self.low_space_warned = false;
        }

        let can_download = self.connected
            && !self.paused
            && !self.scheduler_hold
            && !self.settings.enabled_servers().is_empty();

        if let Some(active) = &self.active_dl {
            let collection_active = self
                .queue
                .iter()
                .find(|c| c.id == active.id)
                .map(|c| c.status == ItemStatus::Downloading)
                .unwrap_or(false);
            active
                .pause
                .store(!can_download || !collection_active, Ordering::Relaxed);
        }

        if let Some(active) = &self.active_dl {
            if !self.queue.iter().any(|c| c.id == active.id) {
                active.stop.store(true, Ordering::Relaxed);
                self.active_dl = None;
            }
        }

        let mut speed_this_frame = 0.0;
        if can_download && self.active_dl.is_none() {
            if let Some(i) = self
                .queue
                .iter()
                .position(|c| c.status == ItemStatus::Queued && c.has_segments())
            {
                let servers: Vec<Server> = self
                    .settings
                    .enabled_servers()
                    .into_iter()
                    .cloned()
                    .collect();
                if !servers.is_empty() {
                    let job = download::job_for_collection(
                        &self.queue[i],
                        self.settings.skip_existing,
                        self.settings.retry_count,
                        self.settings.retry_delay,
                        self.settings.download_priority,
                        self.settings.article_cache_mb,
                        self.settings.cache_cleanup_minutes,
                        ProxyConfig::from_settings(&self.settings),
                    );
                    // The downloader opens each server's connection count and
                    // splits the global limit across all workers
                    // (function 0x835e38 divides by the active count).
                    let throttle = self.speed_limit_bps();
                    self.queue[i].status = ItemStatus::Downloading;
                    let subject = self.queue[i].subject.clone();
                    self.log("Queue", &format!("Starting: {subject}"));
                    self.active_dl = Some(download::spawn(job, servers, throttle));
                }
            }
        }

        let snapshot = self
            .active_dl
            .as_ref()
            .and_then(|a| a.snapshot.lock().ok().map(|s| s.clone()));
        if let Some(snapshot) = snapshot {
            let mut clear = false;
            let mut delta = 0.0;
            let mut completed: Option<(String, u64)> = None;
            let mut completed_id: Option<u64> = None;
            let mut count_collection = false;
            let mut failed: Option<(String, String)> = None;
            if let Some(c) = self.queue.iter_mut().find(|c| c.id == snapshot.id) {
                let previous = c.downloaded;
                if snapshot.stage == download::Stage::Completed {
                    c.downloaded = c.total_bytes as f64;
                    for f in &mut c.files {
                        f.downloaded = f.bytes as f64;
                    }
                    c.status = ItemStatus::Completed;
                } else {
                    c.downloaded = snapshot.bytes as f64;
                    for (index, f) in c.files.iter_mut().enumerate() {
                        if let Some(progress) = snapshot.file_progress.get(index) {
                            f.downloaded = (*progress as f64).min(f.bytes as f64);
                        } else if index < snapshot.file_index {
                            f.downloaded = f.bytes as f64;
                        }
                    }
                }
                delta = (c.downloaded - previous).max(0.0);
                if can_download && snapshot.stage == download::Stage::Downloading {
                    speed_this_frame = snapshot.speed_bps;
                }
                match snapshot.stage {
                    download::Stage::Completed => {
                        completed = Some((c.subject.clone(), c.total_bytes));
                        completed_id = Some(c.id);
                        count_collection = true;
                        clear = true;
                    }
                    download::Stage::Incomplete => {
                        c.status = ItemStatus::Incomplete;
                        completed = Some((c.subject.clone(), c.total_bytes));
                        completed_id = Some(c.id);
                        clear = true;
                    }
                    download::Stage::Error => {
                        c.status = ItemStatus::Error;
                        failed = Some((c.subject.clone(), snapshot.message.clone()));
                        clear = true;
                    }
                    download::Stage::Stopped => {
                        if c.status == ItemStatus::Downloading {
                            c.status = ItemStatus::Paused;
                        }
                        clear = true;
                    }
                    _ => {}
                }
            } else {
                clear = true;
            }
            self.transfered_session = self.transfered_session.saturating_add(delta as u64);
            self.pending_bytes += delta;
            if count_collection {
                self.total_collections = self.total_collections.saturating_add(1);
                self.session_collections = self.session_collections.saturating_add(1);
            }
            if let Some((subject, total)) = completed {
                self.log("Main", &format!("Download complete: {subject}"));
                if count_collection {
                    tray::notify("Download complete", &subject);
                } else {
                    tray::notify("Download incomplete", &subject);
                }
                self.history.insert(
                    0,
                    HistoryItem {
                        name: subject,
                        date: now_datetime(),
                        size: format_size_short(total),
                    },
                );
                if self.history.len() > 50 {
                    self.history.pop();
                }
            }
            if let Some((subject, message)) = failed {
                self.log(
                    "Error",
                    &format!("Download failed for {subject}: {message}"),
                );
                self.show_message = Some((
                    "Download error".into(),
                    format!("{subject}\n\n{message}"),
                ));
            }
            if clear {
                self.active_dl = None;
            }
            if let Some(id) = completed_id {
                self.schedule_after_download(id);
            }
        }
        self.pending_speed_bps = speed_this_frame;

        // Function 0x8345d4: once a second, keep the last four samples and
        // display their average. Function 0x833ffc resets the counters at
        // 1000 ms, 60000 ms and 3600000 ms.
        if now.duration_since(self.last_sample) >= Duration::from_secs(1) {
            self.last_sample = now;
            let previous_speed = self.speed_bps;
            let slot = self.speed_sample_index % SPEED_SAMPLE_COUNT;
            self.speed_samples[slot] = self.pending_speed_bps;
            self.speed_sample_index = self.speed_sample_index.wrapping_add(1);
            // The original fills the whole ring with the first non-zero
            // sample after a zero reading so the average is not diluted.
            if previous_speed <= 0.0 && self.pending_speed_bps > 0.0 {
                self.speed_samples = [self.pending_speed_bps; SPEED_SAMPLE_COUNT];
            }
            let average = self.speed_samples.iter().sum::<f64>() / SPEED_SAMPLE_COUNT as f64;
            self.speed_bps = average;
            tray::set_speed(average, self.settings.speed_bytes_mode);
            self.speed_history.push_back(average as f32);
            while self.speed_history.len() > 120 {
                self.speed_history.pop_front();
            }
            // When the maximum speed is configured as 0 (Auto), the observed
            // Mbit/s raises the ceiling for at most 60 one-second ticks.
            if self.auto_speed && self.auto_speed_ticks < AUTO_SPEED_WINDOW_TICKS {
                let observed_mbps = average / BPS_PER_MBPS;
                if observed_mbps > self.speed_ceiling_mbps {
                    self.speed_ceiling_mbps = observed_mbps;
                }
                self.auto_speed_ticks += 1;
            }
            self.roll_transfer_stats();
            let now_local = Local::now();
            let hour = now_local.hour() as usize;
            let month = now_local.month0() as usize;
            let bytes_this_second = std::mem::take(&mut self.pending_bytes);
            let megabytes = bytes_this_second / MB;
            self.daily_transfer[hour] += megabytes;
            self.monthly_transfer[month] += megabytes;
        }

        if now.duration_since(self.last_minute_stats) >= Duration::from_millis(MINUTE_STATS_MS) {
            self.last_minute_stats = now;
            if self.connected && self.settings.detailed_log {
                self.log(
                    "Stats",
                    &format!(
                        "Minute statistics: {} this session.",
                        format_bytes_auto(self.transfered_session as f64)
                    ),
                );
            }
        }
        if now.duration_since(self.last_hour_stats) >= Duration::from_millis(HOUR_STATS_MS) {
            self.last_hour_stats = now;
            if self.connected && self.settings.detailed_log {
                self.log(
                    "Stats",
                    &format!(
                        "Hourly statistics: {} this session.",
                        format_bytes_auto(self.transfered_session as f64)
                    ),
                );
            }
        }

        if let Some((idx, until)) = self.repair_until {
            let mut finished_name: Option<String> = None;
            if let Some(p) = self.par2.get_mut(idx) {
                if p.progress < 1.0 {
                    p.progress = (p.progress + dt as f32 * 0.12).min(1.0);
                    if now >= until || p.progress >= 1.0 {
                        p.progress = 1.0;
                        p.status = if p.info == "All files OK" {
                            "OK".into()
                        } else {
                            "Repaired".into()
                        };
                        p.info = "Repair complete".into();
                        finished_name = Some(p.name.clone());
                    }
                }
            }
            if let Some(name) = finished_name {
                self.log("PAR2", &format!("Repair finished: {name}"));
            }
            if now >= until {
                self.repair_until = None;
            }
        }

        if self.connected {
            let active_workers = self
                .active_dl
                .as_ref()
                .and_then(|a| a.snapshot.lock().ok().map(|s| s.workers))
                .unwrap_or(0);
            self.threads = build_threads(
                true,
                &self.queue,
                &self.settings,
                self.paused || self.scheduler_hold,
                active_workers,
            );
        } else if !self.threads.is_empty() {
            self.threads.clear();
        }

        if self.shutdown_armed && self.shutdown_countdown.is_none() && self.all_tasks_done() {
            self.shutdown_countdown = Some(now + Duration::from_secs(15));
            self.log("Shutdown", "All tasks finished. Shutdown scheduled.");
        }
        if self.shutdown_countdown.is_some() && !self.all_tasks_done() {
            self.shutdown_countdown = None;
            self.log(
                "Shutdown",
                "Countdown cancelled: unfinished tasks remain.",
            );
        }
        if let Some(until) = self.shutdown_countdown {
            if now >= until {
                self.shutdown_countdown = None;
                self.shutdown_armed = false;
                let action = shutdown::PowerAction::from_setting(&self.settings.shutdown_method);
                self.log(
                    "Shutdown",
                    &format!("All tasks finished. Running {}.", action.label()),
                );
                if self.shutdown_mode == shutdown::ShutdownMode::Real
                    && self.screenshot_path.is_none()
                {
                    self.settings.save();
                    save_queue_state(&self.queue);
                    if self.settings.clear_history_on_exit {
                        self.history.clear();
                    }
                    save_history(&self.history);
                    self.save_statistics();
                }
                match shutdown::execute(action, self.shutdown_mode) {
                    Ok(shutdown::ShutdownOutcome::Executed(detail)) => {
                        self.log("Shutdown", &format!("{}: {detail}", action.label()));
                    }
                    Ok(shutdown::ShutdownOutcome::Mocked(detail)) => {
                        self.log(
                            "Shutdown",
                            &format!("{} simulated: {detail}", action.label()),
                        );
                    }
                    Ok(shutdown::ShutdownOutcome::ExitApplication) => {
                        self.log("Shutdown", "Exiting NewsQueue.");
                        self.shutdown_exit_requested = true;
                    }
                    Err(error) => {
                        self.log("Error", &format!("Shutdown failed: {error}"));
                        self.show_message = Some(("Shutdown".into(), error));
                    }
                }
            }
        }

        if let Some((text, at)) = &self.toast {
            if now.duration_since(*at) > Duration::from_secs(3) {
                self.toast = None;
            } else {
                let _ = text;
            }
        }

        if now.duration_since(self.last_save) > Duration::from_secs(30) {
            self.last_save = now;
            self.settings.save();
            self.save_statistics();
        }
        if now.duration_since(self.last_queue_save)
            > Duration::from_secs(self.settings.autosave_minutes.max(1) as u64 * 60)
        {
            self.last_queue_save = now;
            save_queue_state(&self.queue);
        }
    }

    pub fn handle_keys(&mut self, ctx: &egui::Context) {
        let (f1, f2, f3, f9, ctrl_p, ctrl_d, del, ctrl_up, ctrl_down, ctrl_e, escape) = ctx
            .input(|i| {
                (
                    i.key_pressed(egui::Key::F1),
                    i.key_pressed(egui::Key::F2),
                    i.key_pressed(egui::Key::F3),
                    i.key_pressed(egui::Key::F9),
                    i.modifiers.command && i.key_pressed(egui::Key::P),
                    i.modifiers.command && i.key_pressed(egui::Key::D),
                    i.key_pressed(egui::Key::Delete),
                    i.modifiers.command && i.key_pressed(egui::Key::ArrowUp),
                    i.modifiers.command && i.key_pressed(egui::Key::ArrowDown),
                    i.modifiers.command && i.key_pressed(egui::Key::E),
                    i.key_pressed(egui::Key::Escape),
                )
            });
        if f1 {
            self.toggle_pause();
        }
        if f2 {
            self.active_tab = self.active_tab.saturating_sub(1);
        }
        if f3 {
            self.active_tab = (self.active_tab + 1).min(4);
        }
        if f9 {
            self.show_threads = !self.show_threads;
            self.settings.thread_list_visible = self.show_threads;
        }
        if ctrl_p {
            self.pause_selected(true);
        }
        if ctrl_d {
            self.pause_selected(false);
        }
        if del && !self.show_setup {
            self.delete_selected();
        }
        if ctrl_up {
            self.move_selected(MoveDir::Up);
        }
        if ctrl_down {
            self.move_selected(MoveDir::Down);
        }
        if ctrl_e {
            self.export_selected();
        }
        if escape {
            if self.show_message.is_some() {
                self.show_message = None;
            } else if self.show_speed_limit {
                self.show_speed_limit = false;
            } else if self.show_pause_for {
                self.show_pause_for = false;
            } else if self.show_columns {
                self.show_columns = false;
            } else if self.show_rename {
                self.show_rename = false;
            } else if self.show_notes {
                self.show_notes = false;
            } else if self.show_about {
                self.show_about = false;
            } else if self.show_stats {
                self.show_stats = false;
            } else if self.show_categories {
                self.show_categories = false;
            } else if self.show_server_edit {
                self.show_server_edit = false;
            } else if self.show_setup {
                self.show_setup = false;
            }
        }
    }

    fn roll_transfer_stats(&mut self) {
        let now = Local::now();
        let day = now.format("%Y-%m-%d").to_string();
        if self.stats_day != day {
            self.daily_transfer = [0.0; 24];
            self.stats_day = day;
        }
        let year = now.year();
        if self.stats_year != year {
            self.monthly_transfer = [0.0; 12];
            self.stats_year = year;
        }
    }

    fn save_statistics(&self) {
        if self.screenshot_path.is_some() {
            return;
        }
        let stats = TransferStats {
            day: self.stats_day.clone(),
            year: self.stats_year,
            daily: self.daily_transfer,
            monthly: self.monthly_transfer,
            collections: self.total_collections,
            startups: self.startups,
            longest_uptime_secs: self
                .longest_uptime_secs
                .max(self.session_start.elapsed().as_secs()),
        };
        save_transfer_stats(&stats);
    }

    fn handle_tray(&mut self, ctx: &egui::Context) {
        if self.screenshot_path.is_some() {
            return;
        }
        let close_requested = ctx.input(|input| input.viewport().close_requested());
        let Some(handle) = tray::main_window_handle() else {
            return;
        };
        if close_requested {
            match tray::close_action(
                tray::exit_requested() || self.shutdown_exit_requested,
                self.settings.minimize_to_tray_on_close,
                tray::ready(),
            ) {
                tray::CloseAction::HideToTray => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    tray::hide_main_window(handle);
                    self.tray_hidden = true;
                    self.log("Main", "Minimized to tray on close.");
                }
                tray::CloseAction::Exit => {}
            }
        }
        if self.tray_hidden {
            if tray::is_visible(handle) {
                self.tray_hidden = false;
            }
        } else if self.settings.minimize_to_tray && tray::is_minimized(handle) {
            tray::hide_main_window(handle);
            self.tray_hidden = true;
            self.log("Main", "Minimized to tray.");
        }
    }

    pub fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot_path.clone() else {
            return;
        };
        if self.frame >= 120 && !self.screenshot_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            self.screenshot_requested = true;
        }
        if self.screenshot_requested && !self.screenshot_saved {
            if self.frame > 240 {
                eprintln!("Timed out waiting for the screenshot event");
                std::process::exit(1);
            }
            let mut image: Option<std::sync::Arc<egui::ColorImage>> = None;
            ctx.input(|i| {
                for event in &i.events {
                    if let egui::Event::Screenshot { image: img, .. } = event {
                        image = Some(img.clone());
                    }
                }
            });
            if let Some(img) = image {
                let w = img.width() as u32;
                let h = img.height() as u32;
                let mut buf = Vec::with_capacity((w * h * 4) as usize);
                for px in &img.pixels {
                    let [r, g, b, a] = px.to_array();
                    buf.extend_from_slice(&[r, g, b, a]);
                }
                if let Some(parent) = std::path::Path::new(&path).parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match image::RgbaImage::from_raw(w, h, buf) {
                    Some(img) => match img.save(&path) {
                        Ok(_) => {
                            println!("Screenshot saved to {path}");
                            self.screenshot_saved = true;
                            std::process::exit(0);
                        }
                        Err(e) => {
                            eprintln!("Could not save screenshot: {e}");
                            std::process::exit(1);
                        }
                    },
                    None => {
                        eprintln!("Could not build screenshot image");
                        std::process::exit(1);
                    }
                }
            }
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if self.screenshot_path.is_none() {
            self.settings.save();
            save_queue_state(&self.queue);
            if self.settings.clear_history_on_exit {
                self.history.clear();
            }
            save_history(&self.history);
            self.save_statistics();
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame += 1;
        self.handle_tray(ctx);
        self.tick();
        if self.shutdown_exit_requested && !self.shutdown_exit_sent {
            self.shutdown_exit_sent = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.handle_keys(ctx);
        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect()
        });
        if !dropped.is_empty() && self.active_tab == 2 {
            self.explorer_drop_paths(dropped);
        }
        ui::draw(self, ctx);
        dialogs::draw(self, ctx);
        self.handle_screenshot(ctx);

        if self.screenshot_path.is_some() {
            ctx.request_repaint();
        } else if self.connected && !self.paused {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }
}

fn build_threads(
    connected: bool,
    queue: &[Collection],
    settings: &Settings,
    paused: bool,
    active_workers: usize,
) -> Vec<ThreadRow> {
    if !connected {
        return Vec::new();
    }
    let mut rows = Vec::new();
    let active = active_download(queue);
    let active_subject = active
        .and_then(|i| queue.get(i))
        .map(|c| c.subject.clone())
        .unwrap_or_else(|| "Waiting for work".into());
    let mut n: u32 = 1;
    for server in settings.enabled_servers() {
        for _ in 0..server.connections {
            let (action, desc, status) = if paused {
                (
                    "Paused",
                    "Downloading paused".to_string(),
                    "Paused".to_string(),
                )
            } else if active.is_some() && n as usize <= active_workers {
                (
                    "Article",
                    format!("Retrieving article for {active_subject}"),
                    "Downloading".to_string(),
                )
            } else {
                ("Idle", "Waiting for work".to_string(), "Idle".to_string())
            };
            rows.push(ThreadRow {
                number: n,
                action: action.into(),
                description: desc,
                status,
                server: format!("{}:{}", server.address, server.port),
            });
            n += 1;
        }
    }
    rows
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Turns an execute-hook template into a shell line that reads `{folder}`,
/// `{name}` and `{file}` from the `NQ_FOLDER`, `NQ_NAME` and `NQ_FILE`
/// environment variables instead of pasting them in as text. The NZB subject
/// comes from feeds and indexers, so a name such as `x & evil.exe` must stay
/// data. A placeholder outside quotes gets quotes of its own; one the user
/// already quoted is left inside their quotes.
fn hook_shell_command(template: &str, windows: bool) -> String {
    let placeholders = [
        ("{folder}", "NQ_FOLDER"),
        ("{name}", "NQ_NAME"),
        ("{file}", "NQ_FILE"),
    ];
    let mut out = String::with_capacity(template.len() + 32);
    let mut quoted = false;
    let mut rest = template;
    'outer: while let Some(ch) = rest.chars().next() {
        for (placeholder, variable) in placeholders {
            if let Some(after) = rest.strip_prefix(placeholder) {
                let reference = if windows {
                    format!("%{variable}%")
                } else {
                    format!("${{{variable}}}")
                };
                if quoted {
                    out.push_str(&reference);
                } else {
                    out.push('"');
                    out.push_str(&reference);
                    out.push('"');
                }
                rest = after;
                continue 'outer;
            }
        }
        if ch == '"' {
            quoted = !quoted;
        }
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}

/// A value for a hook variable: quotes and line breaks would end the quoted
/// string around it, so they are dropped. Windows paths cannot contain `"`.
fn hook_env_value(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !matches!(ch, '"' | '\r' | '\n' | '\0'))
        .collect()
}

/// Open a file or folder with the Windows shell, matching a double-click in
/// Explorer. Uses ShellExecuteW so commas and quotes in paths are safe.
#[cfg(target_os = "windows")]
fn shell_open(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            hwnd: *mut std::ffi::c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> isize;
    }
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            std::ptr::null(),
            wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn shell_open(path: &Path) {
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

fn unique_destination(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let extension = path
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    for index in 1..10_000 {
        let candidate = parent.join(format!("{stem} ({index}){extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    path
}

fn copy_directory(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn parse_screenshot_arg() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--screenshot" {
            return args.get(i + 1).cloned();
        }
        if let Some(v) = args[i].strip_prefix("--screenshot=") {
            return Some(v.to_string());
        }
        i += 1;
    }
    None
}

fn load_window_icon() -> egui::IconData {
    let bytes = include_bytes!("../assets/newsqueue-256.png");
    let image = image::load_from_memory(bytes)
        .expect("embedded app icon is not a valid PNG")
        .into_rgba8();
    let (width, height) = image.dimensions();
    egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--rss-download-test") {
        let settings = Settings::load();
        match rss::fetch_and_download(
            &settings.rss_urls,
            &settings.rss_filters,
            Path::new(&settings.nzb_dir),
            &[],
        ) {
            Ok(result) => {
                println!(
                    "{} RSS item(s), {} NZB download(s)",
                    result.items.len(),
                    result.downloads.len()
                );
                for error in &result.errors {
                    println!("error: {error}");
                }
                for download in result.downloads {
                    println!("{} | {} bytes", download.nzb.name, download.nzb.total_bytes);
                }
                return Ok(());
            }
            Err(error) => {
                eprintln!("rss download test failed: {error}");
                std::process::exit(1);
            }
        }
    }
    if args.iter().any(|arg| arg == "--rss-test") {
        let settings = Settings::load();
        match rss::fetch_feeds(&settings.rss_urls) {
            Ok(items) => {
                println!("{} RSS item(s)", items.len());
                for item in items.iter().take(10) {
                    println!("{} | {}", item.name, item.url);
                }
                return Ok(());
            }
            Err(error) => {
                eprintln!("rss test failed: {error}");
                std::process::exit(1);
            }
        }
    }
    if let Some(position) = args.iter().position(|a| a == "--par2-test") {
        let source = args
            .get(position + 1)
            .cloned()
            .unwrap_or_else(|| ".".into());
        let source = PathBuf::from(source);
        let settings = Settings::load();
        let Some(par2_file) = par2::find_par2_file(&source, &[]) else {
            eprintln!("No PAR2 file found in {}", source.display());
            std::process::exit(1);
        };
        match par2::verify_or_repair(
            &source,
            &par2_file,
            &par2::Par2Options::from_settings(&settings),
        ) {
            Ok(message) => {
                println!("{message}");
                return Ok(());
            }
            Err(error) => {
                eprintln!("par2 test failed: {error}");
                std::process::exit(1);
            }
        }
    }
    if let Some(position) = args.iter().position(|a| a == "--unrar-test") {
        let source = args
            .get(position + 1)
            .cloned()
            .unwrap_or_else(|| ".".into());
        let dest = args
            .get(position + 2)
            .cloned()
            .unwrap_or_else(|| "unrar-test-output".into());
        let options = unrar::UnrarOptions {
            overwrite: true,
            rename_existing: true,
            strip_root: true,
            delete_archives: false,
            delete_empty_folder: false,
            auto_rename: false,
            collection_name: String::new(),
            password: String::new(),
            move_patterns: Vec::new(),
            delete_patterns: Vec::new(),
            case_insensitive: true,
            expected_files: Vec::new(),
        };
        match unrar::extract(Path::new(&source), Path::new(&dest), &options) {
            Ok(message) => {
                println!("{message}");
                return Ok(());
            }
            Err(error) => {
                eprintln!("unrar test failed: {error}");
                std::process::exit(1);
            }
        }
    }
    if let Some(position) = args.iter().position(|a| a == "--download-test") {
        let path = args
            .get(position + 1)
            .cloned()
            .unwrap_or_else(|| "dancing-4c5722.nzb".into());
        match download::run_download_test(&path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("download test failed: {e}");
                std::process::exit(1);
            }
        }
    }
    let screenshot = parse_screenshot_arg();
    if screenshot.is_none() {
        tray::spawn();
    }
    let settings = if screenshot.is_some() {
        Settings::default()
    } else {
        Settings::load()
    };
    ensure_dirs(&settings);
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([SAVED_WIDTH, SAVED_HEIGHT])
            .with_min_inner_size([940.0, 620.0])
            .with_icon(load_window_icon())
            .with_title(format!("{APP_NAME} {VERSION}")),
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        &format!("{APP_NAME} {VERSION}"),
        native_options,
        Box::new(move |cc| {
            theme::install(&cc.egui_ctx);
            Ok(Box::new(App::new(settings, screenshot)))
        }),
    )
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn explorer_navigation_creates_and_reports_folders() {
        let root = std::env::temp_dir().join(format!(
            "newsqueue-explorer-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let first = root.join("first");
        let second = root.join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();

        let mut app = App::new(Settings::default(), Some("unused.png".into()));
        app.explorer_navigate(first.clone());
        assert_eq!(app.explorer_path, first);
        app.explorer_navigate(second.clone());
        assert_eq!(app.explorer_path, second);
        app.explorer_back();
        assert_eq!(app.explorer_path, first);
        app.explorer_forward();
        assert_eq!(app.explorer_path, second);
        app.explorer_new_folder();
        let created = app.explorer_selected.clone().unwrap();
        assert!(created.is_dir());
        app.explorer_properties(created.clone());
        assert!(app.show_message.is_some());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn transfer_stats_reset_daily_on_a_new_day_and_monthly_on_a_new_year() {
        let mut stats = TransferStats {
            day: "2026-11-03".into(),
            year: 2026,
            daily: [1.0; 24],
            monthly: [2.0; 12],
            ..Default::default()
        };
        stats.normalize_for(2026, "2026-11-04");
        assert_eq!(stats.daily, [0.0; 24]);
        assert_eq!(stats.monthly, [2.0; 12]);

        stats.daily[7] = 9.0;
        stats.normalize_for(2026, "2026-11-04");
        assert_eq!(stats.daily[7], 9.0);

        stats.normalize_for(2027, "2027-01-01");
        assert_eq!(stats.monthly, [0.0; 12]);
    }

    #[test]
    fn transfer_stats_round_trip_keeps_counters() {
        let stats = TransferStats {
            day: "2026-11-04".into(),
            year: 2026,
            daily: [1.5; 24],
            monthly: [2.5; 12],
            collections: 12,
            startups: 7,
            longest_uptime_secs: 5_400,
        };
        let text = serde_json::to_string(&stats).unwrap();
        let loaded: TransferStats = serde_json::from_str(&text).unwrap();
        assert_eq!(loaded.collections, 12);
        assert_eq!(loaded.startups, 7);
        assert_eq!(loaded.longest_uptime_secs, 5_400);
        assert_eq!(loaded.daily, [1.5; 24]);
        assert_eq!(loaded.monthly, [2.5; 12]);
    }

    #[test]
    fn hook_placeholders_become_quoted_variables() {
        assert_eq!(
            hook_shell_command("echo {name}>x.txt", true),
            "echo \"%NQ_NAME%\">x.txt"
        );
        assert_eq!(
            hook_shell_command("copy \"{folder}\\a.txt\" {file}", true),
            "copy \"%NQ_FOLDER%\\a.txt\" \"%NQ_FILE%\""
        );
        assert_eq!(hook_shell_command("run {name}", false), "run \"${NQ_NAME}\"");
        assert_eq!(hook_env_value("a\"b\r\nc"), "abc");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn execute_hook_does_not_run_commands_from_the_nzb_name() {
        let root = std::env::temp_dir().join(format!("newsqueue-execute-inject-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut app = App::new(Settings::default(), Some("unused.png".into()));
        app.queue.clear();
        let mut collection = sample_queue(&app.settings).remove(0);
        collection.id = 301;
        collection.folder = root.to_string_lossy().to_string();
        collection.subject = "safe & echo pwned>pwned.txt \" & echo two>pwned2.txt".into();
        collection.execute = "echo {name}>marker.txt".into();
        collection.execute_done = false;
        app.queue = vec![collection];
        app.run_execute_hook(301);
        for _ in 0..100 {
            if root.join("marker.txt").is_file() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
        let marker = std::fs::read_to_string(root.join("marker.txt")).unwrap();
        assert!(marker.contains("safe & echo pwned"), "{marker}");
        assert!(!root.join("pwned.txt").exists());
        assert!(!root.join("pwned2.txt").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn execute_hooks_use_per_collection_and_default_commands() {
        let root = std::env::temp_dir().join(format!(
            "newsqueue-execute-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let first = root.join("first");
        let second = root.join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let mut app = App::new(Settings::default(), Some("unused.png".into()));
        app.queue.clear();
        let mut per_collection = sample_queue(&app.settings).remove(0);
        per_collection.id = 201;
        per_collection.folder = first.to_string_lossy().to_string();
        per_collection.execute = "cmd /C echo one>marker.txt".into();
        per_collection.execute_done = false;
        let mut defaulted = sample_queue(&app.settings).remove(0);
        defaulted.id = 202;
        defaulted.folder = second.to_string_lossy().to_string();
        defaulted.execute.clear();
        defaulted.execute_done = false;
        app.queue = vec![per_collection, defaulted];
        app.settings.default_execute = "cmd /C echo two>marker.txt".into();

        app.run_execute_hook(201);
        app.run_execute_hook(202);
        let one_ready = || {
            std::fs::read_to_string(first.join("marker.txt"))
                .map(|text| text.trim() == "one")
                .unwrap_or(false)
        };
        let two_ready = || {
            std::fs::read_to_string(second.join("marker.txt"))
                .map(|text| text.trim() == "two")
                .unwrap_or(false)
        };
        for _ in 0..100 {
            if one_ready() && two_ready() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(
            std::fs::read_to_string(first.join("marker.txt")).unwrap().trim(),
            "one"
        );
        assert_eq!(
            std::fs::read_to_string(second.join("marker.txt")).unwrap().trim(),
            "two"
        );
        assert!(app.queue.iter().all(|collection| collection.execute_done));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn unrar_password_prompt_retries_and_extracts() {
        let root = std::env::temp_dir().join(format!(
            "newsqueue-unrar-prompt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let source = root.join("source");
        let dest = root.join("dest");
        std::fs::create_dir_all(&source).unwrap();
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("encrypted.rar");
        std::fs::copy(fixture, source.join("encrypted.rar")).unwrap();

        let mut app = App::new(Settings::default(), Some("unused.png".into()));
        app.settings.strip_root = false;
        app.settings.auto_rename_to_collection = false;
        app.settings.delete_archives = false;
        app.settings.delete_empty_folder = false;
        app.settings.create_dir_archive_name = false;
        app.settings.create_dir_group_name = false;
        app.settings.create_dir_nzb_name = false;
        app.settings.pause_while_unraring = false;
        app.queue.clear();
        let mut collection = sample_queue(&app.settings).remove(0);
        collection.id = 77;
        collection.subject = "Password test".into();
        collection.folder = source.to_string_lossy().to_string();
        collection.unrar_folder = dest.to_string_lossy().to_string();
        collection.unrar = true;
        collection.password.clear();
        collection.status = ItemStatus::Completed;
        collection.files.clear();
        app.queue = vec![collection];

        assert!(app.start_unrar(77));
        for _ in 0..300 {
            app.poll_unrar();
            if app.show_unrar_password {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(app.show_unrar_password, "password prompt was not shown");
        assert_eq!(app.unrar_password_target, Some(77));

        app.unrar_password_draft = "secret".into();
        app.submit_unrar_password();
        for _ in 0..300 {
            app.poll_unrar();
            if app.active_unrar.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(app.active_unrar.is_none(), "retry did not finish");
        assert_eq!(app.queue[0].status, ItemStatus::Completed);
        assert_eq!(
            std::fs::read_to_string(dest.join("payload.txt"))
                .unwrap()
                .trim(),
            "secret payload"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn shutdown_countdown_uses_mock_backend() {
        let mut app = App::new(Settings::default(), Some("unused.png".into()));
        app.queue.clear();
        let mut collection = sample_queue(&app.settings).remove(0);
        collection.status = ItemStatus::Completed;
        app.queue = vec![collection];
        app.settings.shutdown_method = "Shutdown Windows".into();
        app.shutdown_mode = shutdown::ShutdownMode::Mock;
        app.shutdown_armed = true;
        app.shutdown_countdown = Some(std::time::Instant::now());
        app.tick();
        assert!(!app.shutdown_armed);
        assert!(app.shutdown_countdown.is_none());
        assert!(!app.shutdown_exit_requested);
        assert!(app
            .logs
            .iter()
            .any(|entry| entry.message.contains("simulated")
                && entry.message.contains("shutdown.exe /s /t 0")));
    }

    #[test]
    fn shutdown_exit_method_only_requests_the_app_to_close() {
        let mut app = App::new(Settings::default(), Some("unused.png".into()));
        app.queue.clear();
        app.settings.shutdown_method = "Exit".into();
        app.shutdown_mode = shutdown::ShutdownMode::Mock;
        app.shutdown_armed = true;
        app.shutdown_countdown = Some(std::time::Instant::now());
        app.tick();
        assert!(!app.shutdown_armed);
        assert!(app.shutdown_countdown.is_none());
        assert!(app.shutdown_exit_requested);
        assert!(app
            .logs
            .iter()
            .any(|entry| entry.message.contains("Exiting NewsQueue")));
    }

    #[test]
    fn shutdown_waits_for_unfinished_tasks() {
        let mut app = App::new(Settings::default(), Some("unused.png".into()));
        app.queue.clear();
        let mut collection = sample_queue(&app.settings).remove(0);
        collection.status = ItemStatus::Downloading;
        app.queue = vec![collection];
        app.shutdown_mode = shutdown::ShutdownMode::Mock;
        app.shutdown_armed = true;
        app.tick();
        assert!(app.shutdown_countdown.is_none());
        assert!(app.shutdown_armed);
    }

    #[test]
    fn shutdown_countdown_cancels_if_new_work_appears() {
        let mut app = App::new(Settings::default(), Some("unused.png".into()));
        app.queue.clear();
        let mut collection = sample_queue(&app.settings).remove(0);
        collection.status = ItemStatus::Downloading;
        app.queue = vec![collection];
        app.shutdown_mode = shutdown::ShutdownMode::Mock;
        app.shutdown_armed = true;
        app.shutdown_countdown = Some(std::time::Instant::now() + Duration::from_secs(10));
        app.tick();
        assert!(app.shutdown_countdown.is_none());
        assert!(app.shutdown_armed);
        assert!(app
            .logs
            .iter()
            .any(|entry| entry.message.contains("Countdown cancelled")));
    }

    #[test]
    fn explorer_multi_copy_pastes_all_selected_files() {
        let root = std::env::temp_dir().join(format!(
            "newsqueue-multi-copy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let source = root.join("source");
        let destination = root.join("destination");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&destination).unwrap();
        let first = source.join("one.txt");
        let second = source.join("two.txt");
        std::fs::write(&first, "one").unwrap();
        std::fs::write(&second, "two").unwrap();
        let mut app = App::new(Settings::default(), Some("unused.png".into()));
        app.explorer_path = source;
        app.explorer_selected_set.insert(first);
        app.explorer_selected_set.insert(second);
        app.explorer_copy_selected(false);
        app.explorer_path = destination.clone();
        app.explorer_paste();
        assert_eq!(
            std::fs::read_to_string(destination.join("one.txt")).unwrap(),
            "one"
        );
        assert_eq!(
            std::fs::read_to_string(destination.join("two.txt")).unwrap(),
            "two"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn explorer_drop_copies_files() {
        let root = std::env::temp_dir().join(format!(
            "newsqueue-drop-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let source = root.join("source.txt");
        let destination = root.join("destination");
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::write(&source, "dropped").unwrap();
        let mut app = App::new(Settings::default(), Some("unused.png".into()));
        app.explorer_path = destination.clone();
        app.explorer_drop_paths(vec![source]);
        assert_eq!(
            std::fs::read_to_string(destination.join("source.txt")).unwrap(),
            "dropped"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn auto_imports_nzbs_from_the_watch_folder() {
        let root = std::env::temp_dir().join(format!(
            "newsqueue-auto-import-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let xml = r#"<?xml version="1.0"?>
<nzb xmlns="http://www.newzbin.com/DTD/2003/nzb">
<file subject="&quot;watch-test.rar&quot; yEnc (1/1)" poster="tester" date="1700000000">
<groups><group>alt.binaries.test</group></groups>
<segments><segment bytes="10" number="1">watch@example</segment></segments>
</file>
</nzb>"#;
        std::fs::write(root.join("watch.nzb"), xml).unwrap();
        let mut app = App::new(Settings::default(), Some("unused.png".into()));
        app.queue.clear();
        app.settings.auto_import_folder = root.to_string_lossy().to_string();
        app.auto_import_nzbs();
        assert_eq!(app.queue.len(), 1);
        assert_eq!(app.queue[0].subject, "watch");
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn par2_repair_then_unrar_chain_works() {
        let root = std::env::temp_dir().join(format!(
            "newsqueue-chain-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let source = root.join("source");
        let dest = root.join("dest");
        std::fs::create_dir_all(&source).unwrap();
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("tiny.rar");
        let payload = source.join("payload.rar");
        std::fs::copy(fixture, &payload).unwrap();
        let par2_exe = app_base().join("tools").join("par2.exe");
        let output = std::process::Command::new(&par2_exe)
            .args(["c", "-q", "-r20", "payload.par2", "payload.rar"])
            .current_dir(&source)
            .output()
            .unwrap();
        assert!(output.status.success());
        let mut damaged = std::fs::read(&payload).unwrap();
        let index = damaged.len() / 2;
        damaged[index] ^= 0xff;
        std::fs::write(&payload, damaged).unwrap();

        let mut app = App::new(Settings::default(), Some("unused.png".into()));
        app.settings.auto_unrar = true;
        app.settings.par2_checking = true;
        app.settings.par2_repairing = true;
        app.settings.overwrite_existing = true;
        app.settings.unrar_to_different = true;
        app.settings.unrar_dir = dest.to_string_lossy().to_string();
        let mut collection = sample_queue(&app.settings).remove(0);
        collection.id = 77;
        collection.subject = "Chain test".into();
        collection.folder = source.to_string_lossy().to_string();
        collection.unrar = true;
        collection.unrar_folder.clear();
        collection.files.clear();
        collection.status = ItemStatus::Completed;
        app.queue = vec![collection];

        assert!(app.start_par2(77));
        for _ in 0..400 {
            app.poll_par2();
            app.poll_unrar();
            if app.active_par2.is_none() && app.active_unrar.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(app.active_par2.is_none(), "par2 did not finish");
        assert!(app.active_unrar.is_none(), "unrar did not finish");
        assert_eq!(app.queue[0].status, ItemStatus::Completed);
        assert_eq!(
            std::fs::read_to_string(dest.join("readme.txt"))
                .unwrap()
                .trim(),
            "tiny rar fixture"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn completed_collection_can_be_unpacked() {
        let root = std::env::temp_dir().join(format!(
            "newsqueue-app-unrar-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let source = root.join("source");
        let dest = root.join("dest");
        std::fs::create_dir_all(&source).unwrap();
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("tiny.rar");
        std::fs::copy(fixture, source.join("tiny.rar")).unwrap();

        let mut app = App::new(Settings::default(), Some("unused.png".into()));
        app.settings.unrar_to_different = true;
        app.settings.unrar_dir = dest.to_string_lossy().to_string();
        let mut collection = sample_queue(&app.settings).remove(0);
        collection.id = 99;
        collection.subject = "Tiny test".into();
        collection.folder = source.to_string_lossy().to_string();
        collection.unrar = true;
        collection.unrar_folder.clear();
        collection.files.clear();
        collection.status = ItemStatus::Completed;
        app.queue = vec![collection];

        assert!(app.start_unrar(99));
        for _ in 0..200 {
            app.poll_unrar();
            if app.active_unrar.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(app.active_unrar.is_none(), "unrar did not finish");
        assert_eq!(app.queue[0].status, ItemStatus::Completed);
        assert_eq!(
            std::fs::read_to_string(dest.join("readme.txt"))
                .unwrap()
                .trim(),
            "tiny rar fixture"
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
