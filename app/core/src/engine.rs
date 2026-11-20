//! The download engine: queue, settings, history, statistics, workers and
//! timers, with no UI or Win32 code. A front end owns an [`Engine`], calls
//! [`Engine::tick`] regularly, issues commands through its methods and reacts
//! to the [`Event`]s it queues ([`Engine::take_events`]).

use crate::model::*;
use crate::{download, par2, rss, unrar};
use chrono::{Datelike, Local, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// Something the front end should show or do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A short status message.
    Toast(String),
    /// A message box: title, text.
    Message(String, String),
    /// A desktop notification (tray balloon): title, text.
    Notify(String, String),
    /// An archive of this collection needs a password.
    PasswordNeeded(u64),
    /// RSS refresh was asked for with no feeds configured.
    RssFeedsNeeded,
}

pub struct Engine {
    /// Free space on the download drive, refreshed every tick.
    pub free_gb: f64,
    pub settings: Settings,
    pub queue: Vec<Collection>,
    pub par2: Vec<Par2Set>,
    pub rss: Vec<RssItem>,
    pub logs: VecDeque<LogEntry>,
    pub threads: Vec<ThreadRow>,
    pub history: Vec<HistoryItem>,
    pub connected: bool,
    pub paused: bool,
    pub pause_until: Option<Instant>,
    pub pause_minutes: u32,
    pub resume_ids: Vec<u64>,
    pub speed_limit_tag: Option<u8>,
    pub custom_speed_limit_mbs: Option<f64>,
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
    pub rss_timer: bool,
    pub rss_rx: Option<std::sync::mpsc::Receiver<Result<rss::FetchResult, String>>>,
    pub rss_last_refresh: Instant,
    pub last_auto_import: Instant,
    pub scheduler_hold: bool,
    pub scheduler_last_hold: bool,
    pub repair_until: Option<(usize, Instant)>,
    pub daily_transfer: [f64; 24],
    pub monthly_transfer: [f64; 12],
    pub stats_day: String,
    pub stats_year: i32,
    pub last_log_day: String,
    pub active_dl: Option<download::ActiveDownload>,
    pub active_unrar: Option<unrar::ActiveUnrar>,
    pub active_par2: Option<par2::ActivePar2>,
    pub pending_unrar: VecDeque<u64>,
    /// Direct unpack: extraction running while its collection downloads.
    pub active_direct: Option<unrar::DirectUnrar>,
    /// Servers of the running job (address, port, user) and the bytes
    /// already counted against their quota.
    job_servers: Vec<(String, u16, String)>,
    job_server_bytes: Vec<u64>,
    no_server_warned: bool,
    /// Collections direct unpack has tried, and whether it extracted them
    /// (`None` while it still runs).
    pub direct_result: HashMap<u64, Option<bool>>,
    pub resume_after_unrar: bool,
    /// Demo/screenshot mode: sample data, nothing is written to disk.
    pub demo: bool,
    pub events: Vec<Event>,
}

impl Engine {
    pub fn new(settings: Settings, demo: bool) -> Self {
        ensure_dirs(&settings);
        crate::tools::configure(&settings.unrar_path, &settings.par2_path, &settings.sevenzip_path);
        prune_old_logs(settings.keep_log_days);
        let queue = if demo {
            sample_queue(&settings)
        } else {
            let mut saved = load_queue_state().unwrap_or_default();
            // Older saves contain demonstration entries with no article ids.
            saved.retain(|c| c.has_segments());
            saved
        };
        let threads = build_threads(false, &queue, &settings, false, 0);
        let (daily, monthly, stats_day, stats_year, total_collections, startups, longest_uptime_secs) =
            if demo {
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
        let history = if demo {
            sample_history()
        } else {
            load_history().unwrap_or_default()
        };
        let now = Instant::now();
        let mut engine = Self {
            speed_limit_tag: settings.startup_speed_limit,
            custom_speed_limit_mbs: None,
            speed_ceiling_mbps: settings.max_speed_mbps,
            auto_speed: settings.max_speed_kbps <= 0.0,
            settings,
            queue,
            par2: if demo { sample_par2() } else { Vec::new() },
            rss: if demo { sample_rss() } else { Vec::new() },
            logs: if demo { sample_log() } else { VecDeque::new() },
            threads,
            history,
            connected: false,
            paused: false,
            pause_until: None,
            pause_minutes: 0,
            resume_ids: Vec::new(),
            auto_speed_ticks: 0,
            speed_samples: [0.0; SPEED_SAMPLE_COUNT],
            speed_sample_index: 0,
            pending_speed_bps: 0.0,
            pending_bytes: 0.0,
            speed_bps: 0.0,
            speed_history: VecDeque::from(vec![0.0; 120]),
            transfered_session: 0,
            session_start: now,
            startups,
            total_collections,
            session_collections: 0,
            longest_uptime_secs,
            last_tick: now,
            last_sample: now,
            last_minute_stats: now,
            last_hour_stats: now,
            last_save: now,
            last_queue_save: now,
            low_space_warned: false,
            free_gb: 0.0,
            rss_timer: false,
            rss_rx: None,
            rss_last_refresh: now,
            last_auto_import: now,
            scheduler_hold: false,
            scheduler_last_hold: false,
            repair_until: None,
            daily_transfer: daily,
            monthly_transfer: monthly,
            stats_day,
            stats_year,
            last_log_day: String::new(),
            active_dl: None,
            active_unrar: None,
            active_par2: None,
            pending_unrar: VecDeque::new(),
            active_direct: None,
            job_servers: Vec::new(),
            job_server_bytes: Vec::new(),
            no_server_warned: false,
            direct_result: HashMap::new(),
            resume_after_unrar: false,
            demo,
            events: Vec::new(),
        };
        if !demo {
            let history_count = engine.history.len();
            engine.log("Main", &format!("{APP_NAME} {VERSION} ({BUILD_DATE}) started."));
            engine.log("Main", &format!("OS {}", std::env::consts::OS));
            engine.log(
                "Main",
                &format!("{history_count} items loaded into download history list."),
            );
            engine.log("Main", "Ready. No server connection is open.");
        }
        engine
    }

    /// Events queued since the last call.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub fn notify(&mut self, title: &str, text: &str) {
        self.events.push(Event::Notify(title.to_string(), text.to_string()));
        let url = self.settings.notify_url.trim().to_string();
        if !url.is_empty() {
            let (title, text) = (title.to_string(), text.to_string());
            std::thread::spawn(move || {
                let _ = send_webhook(&url, &title, &text);
            });
        }
    }

    pub fn add_history(&mut self, name: String, total: u64, status: &str, source: String) {
        self.history.insert(
            0,
            HistoryItem { name, date: now_datetime(), size: format_size_short(total), status: status.into(), source },
        );
        if self.history.len() > 50 {
            self.history.pop();
        }
    }

    /// Queue a history entry's NZB again.
    pub fn retry_history(&mut self, index: usize) -> crate::command::Applied {
        let Some(item) = self.history.get(index).cloned() else {
            return crate::command::Applied::Ignored;
        };
        if item.source.is_empty() || !Path::new(&item.source).is_file() {
            self.log("Queue", &format!("Cannot retry {}: its NZB is no longer on disk", item.name));
            self.toast("NZB no longer on disk");
            return crate::command::Applied::Failed;
        }
        self.log("Queue", &format!("Retrying from history: {}", item.name));
        self.apply(crate::command::Command::AddNzbFile(item.source, false))
    }

    /// Saves settings, queue, history and statistics (not in demo mode).
    pub fn save_all(&mut self) {
        if self.demo {
            return;
        }
        self.settings.save();
        save_queue_state(&self.queue);
        if self.settings.clear_history_on_exit {
            self.history.clear();
        }
        save_history(&self.history);
        self.save_statistics();
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

    pub fn write_log_file(&self, entry: &LogEntry) {
        if self.demo
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
        self.events.push(Event::Toast(text.into()));
    }

    pub fn active_collection(&self) -> Option<usize> {
        active_download(&self.queue)
    }

    /// Global bandwidth limit in bytes per second. `None` means "No limit"
    /// and is 0 (unthrottled), matching function 0x817178.
    pub fn speed_limit_bps(&self) -> f64 {
        if let Some(mbs) = self.custom_speed_limit_mbs { return mbs * 1_000_000.0; }
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
            self.events.push(Event::Message(
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
    pub fn resume_paused_ids(&mut self) {
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

    pub fn set_custom_speed_limit(&mut self, mbs: f64) {
        self.custom_speed_limit_mbs = Some(mbs);
        self.speed_limit_tag = None;
        self.log("Connection", &format!("Speed limit set to {mbs:.2} MB/s."));
        self.toast(format!("Speed limit {mbs:.2} MB/s"));
    }

    pub fn set_speed_limit(&mut self, tag: Option<u8>) {
        self.custom_speed_limit_mbs = None;
        self.speed_limit_tag = tag;
        match tag {
            Some(t) => {
                let pct = t * 10;
                let bps = speed_limit_bps(t, self.speed_ceiling_mbps);
                self.log(
                    "Connection",
                    &format!(
                        "Speed limit set to {pct}% ({}).",
                        format_speed(bps, self.settings.speed_bytes_mode)
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

    pub fn add_nzb_from_parsed(&mut self, nzb: ParsedNzb, source: String, filter_paused: bool) -> u64 {
        let duplicate = self.settings.duplicate_check > 0 && {
            let name = nzb.name.to_lowercase();
            self.history.iter().any(|h| h.name.to_lowercase() == name && h.status != "Failed")
                || self.queue.iter().any(|c| c.subject.to_lowercase() == name)
        };
        if duplicate {
            let paused = if self.settings.duplicate_check >= 2 { " (added paused)" } else { "" };
            self.log("Queue", &format!("Duplicate: {} is already in the queue or history{paused}", nzb.name));
            self.toast("Duplicate NZB");
        }
        let filter_paused = filter_paused || (duplicate && self.settings.duplicate_check >= 2);
        let mut files: Vec<QueueFile> = nzb
            .files
            .iter()
            .map(|file| QueueFile {
                held: false,
                done: false,
                name: file.name.clone(),
                bytes: file.bytes,
                downloaded: 0.0,
                parts: file.parts,
                segments: file.segments.clone(),
            })
            .collect();
        let held = self.hold_par2_volumes(&mut files);
        let id = self.queue.iter().map(|collection| collection.id).max().unwrap_or(0) + 1;
        let paused = self.paused || filter_paused;
        let group = if nzb.group.is_empty() {
            "alt.binaries.misc".to_string()
        } else {
            nzb.group.clone()
        };
        let folder = PathBuf::from(&self.settings.download_dir)
                    .join(sanitize_folder(&nzb.name))
                    .to_string_lossy()
                    .to_string();
        let unrar = true;
        let unrar_folder = String::new();
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
            password: nzb.password.clone(),
            source,
        };
        let subject = collection.subject.clone();
        if self.paused && !self.resume_ids.contains(&id) {
            self.resume_ids.push(id);
        }
        self.queue.push(collection);
        self.log("Queue", &format!("Added: {subject}"));
        if held > 0 {
            self.log("PAR2", &format!("{subject}: {held} PAR2 recovery volume(s) held until a repair needs them"));
        }
        if !nzb.password.is_empty() {
            // The password itself never goes to the log.
            self.log("Queue", &format!("{subject}: archive password taken from the NZB"));
        }
        self.toast("Added to queue");
        id
    }

    pub fn refresh_rss(&mut self) {
        if self.rss_rx.is_some() {
            self.toast("RSS refresh already running");
            return;
        }
        if self.settings.rss_urls.is_empty() {
            self.events.push(Event::RssFeedsNeeded);
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

    pub fn poll_rss(&mut self) {
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

    pub fn auto_import_nzbs(&mut self) {
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
                self.events.push(Event::Message("Execute".into(), format!("Could not start command:\n{error}")));
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
            par2::Par2Options::from_settings(&self.settings)
                .with_nzb_files(expected)
                .with_fetch_more(collection.files.iter().any(|f| f.held)),
        ));
        true
    }

    pub fn ensure_par2_set(&mut self, collection: &Collection, par2_file: &Path) {
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

    pub fn poll_par2(&mut self) {
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
            Some(Err(error)) if error.starts_with(par2::NEED_BLOCKS) => {
                let need: u32 = error[par2::NEED_BLOCKS.len()..].trim().parse().unwrap_or(u32::MAX);
                if !self.release_par2_volumes(active.collection_id, need) {
                    self.fail_par2(active.collection_id, format!("PAR2 repair is not possible: {need} more recovery blocks are needed"));
                }
            }
            Some(Err(error)) => self.fail_par2(active.collection_id, error),
        }
    }

    /// With `par2_on_demand`, marks the recovery volumes of a set that also
    /// has its small index `.par2` as held. Returns how many were held.
    pub fn hold_par2_volumes(&self, files: &mut [QueueFile]) -> usize {
        let s = &self.settings;
        if !(s.par2_on_demand && s.par2_checking && s.par2_repairing) {
            return 0;
        }
        let is_par2 = |n: &str| n.to_ascii_lowercase().ends_with(".par2");
        let has_index = files
            .iter()
            .any(|f| is_par2(&f.name) && par2::volume_blocks(&f.name).is_none());
        if !has_index {
            return 0;
        }
        let mut held = 0;
        for f in files.iter_mut() {
            if is_par2(&f.name) && par2::volume_blocks(&f.name).is_some() {
                f.held = true;
                held += 1;
            }
        }
        held
    }

    /// Queues held recovery volumes worth at least `need` blocks for
    /// download; PAR2 runs again when they arrive. False when none are held.
    pub fn release_par2_volumes(&mut self, id: u64, need: u32) -> bool {
        let Some(c) = self.queue.iter_mut().find(|c| c.id == id) else {
            return false;
        };
        let volumes: Vec<(usize, u32)> = c
            .files
            .iter()
            .enumerate()
            .filter(|(_, f)| f.held)
            .map(|(i, f)| (i, par2::volume_blocks(&f.name).unwrap_or(1)))
            .collect();
        if volumes.is_empty() {
            return false;
        }
        let picked = par2::pick_volumes(&volumes, need);
        let blocks: u32 = volumes.iter().filter(|v| picked.contains(&v.0)).map(|v| v.1).sum();
        for f in c.files.iter_mut() {
            if !f.held {
                f.done = true;
            }
        }
        for &i in &picked {
            c.files[i].held = false;
            c.files[i].downloaded = 0.0;
        }
        let extra: u64 = picked.iter().map(|&i| c.files[i].bytes).sum();
        c.downloaded = (c.downloaded - extra as f64).max(0.0);
        c.status = ItemStatus::Queued;
        let subject = c.subject.clone();
        self.log(
            "PAR2",
            &format!(
                "{subject}: {need} recovery block(s) needed; fetching {} volume(s) with {blocks} block(s)",
                picked.len()
            ),
        );
        true
    }

    fn fail_par2(&mut self, collection_id: u64, error: String) {
        if let Some(collection) = self.queue.iter_mut().find(|c| c.id == collection_id) {
            collection.status = ItemStatus::Incomplete;
        }
        for set in &mut self.par2 {
            if set.collection_id == Some(collection_id)
                || (collection_id == 0 && set.status == "Repairing")
            {
                set.status = "Incomplete".into();
                set.info = error.clone();
            }
        }
        self.log("PAR2", &error);
        self.events.push(Event::Message("PAR2".into(), error));
    }

    pub fn schedule_after_download(&mut self, id: u64) {
        match self.direct_result.get(&id).copied() {
            Some(None) => {
                // Still extracting: it now has every volume it will get.
                if let Some(direct) = self.active_direct.as_ref().filter(|d| d.collection_id == id) {
                    direct.finished.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                return;
            }
            Some(Some(true)) => {
                self.direct_result.remove(&id);
                self.sort_output(id);
                self.run_execute_hook(id);
                return;
            }
            Some(Some(false)) => {
                self.direct_result.remove(&id);
            }
            None => {}
        }
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

    /// Moves a cleanly unpacked collection's output to the sort folder. The
    /// collection's unrar folder then points there, so the execute hook and
    /// "open folder" find it.
    pub fn sort_output(&mut self, id: u64) {
        let root = self.settings.sort_folder.trim().to_string();
        if root.is_empty() {
            return;
        }
        let Some(index) = self.index_of(id) else { return };
        let collection = self.queue[index].clone();
        let from = unrar::target_dir(&self.settings, &collection);
        if !from.is_dir() || from == PathBuf::from(&self.settings.download_dir) {
            return;
        }
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let to = PathBuf::from(&root).join(unrar::expand_sort_pattern(
            &self.settings.sort_pattern,
            &collection.subject,
            &collection.group,
            &today,
        ));
        match unrar::move_dir(&from, &to) {
            Ok(target) => {
                self.log("Unrar", &format!("Moved {} to {}", collection.subject, target.display()));
                self.queue[index].unrar_folder = target.to_string_lossy().to_string();
            }
            Err(e) => self.log("Error", &format!("Cannot move {} to {}: {e}", collection.subject, to.display())),
        }
    }

    /// Adds what each server delivered since the last tick to its monthly use.
    fn count_server_bytes(&mut self, totals: &[u64]) {
        let month = chrono::Local::now().format("%Y-%m").to_string();
        for (k, &total) in totals.iter().enumerate() {
            let before = self.job_server_bytes.get(k).copied().unwrap_or(0);
            let delta = total.saturating_sub(before);
            if delta == 0 {
                continue;
            }
            if let Some(slot) = self.job_server_bytes.get_mut(k) {
                *slot = total;
            }
            let Some((address, port, user)) = self.job_servers.get(k).cloned() else { continue };
            if let Some(server) = self
                .settings
                .servers
                .iter_mut()
                .find(|s| s.address == address && s.port == port && s.username == user)
            {
                if server.quota_month != month {
                    server.quota_month = month.clone();
                    server.used_bytes = 0;
                }
                server.used_bytes = server.used_bytes.saturating_add(delta);
            }
        }
    }

    /// Direct unpack: start UnRAR on the first finished RAR volume of the
    /// collection being downloaded, tell it which volumes are complete, and
    /// stop it when articles go missing (PAR2 has to repair first then).
    fn drive_direct(&mut self, snapshot: &download::DownloadSnapshot) {
        use std::sync::atomic::Ordering;
        let id = snapshot.id;
        let ready: Vec<PathBuf> = (0..snapshot.paths.len())
            .filter(|&i| snapshot.sizes[i] > 0 && snapshot.file_progress[i] >= snapshot.sizes[i])
            .filter_map(|i| snapshot.paths[i].clone())
            .collect();
        if let Some(direct) = self.active_direct.as_ref().filter(|d| d.collection_id == id) {
            if snapshot.missing > 0 {
                direct.cancel.store(true, Ordering::Relaxed);
                return;
            }
            if let Ok(mut list) = direct.ready.lock() {
                *list = ready;
            }
            if matches!(snapshot.stage, download::Stage::Completed | download::Stage::Error | download::Stage::Incomplete) {
                direct.finished.store(true, Ordering::Relaxed);
            }
            return;
        }
        if !self.settings.direct_unpack
            || !self.settings.auto_unrar
            || snapshot.missing > 0
            || self.active_direct.is_some()
            || self.active_unrar.is_some()
            || self.direct_result.contains_key(&id)
            || !unrar::unrar_available()
        {
            return;
        }
        let Some(collection) = self.queue.iter().find(|c| c.id == id).filter(|c| c.unrar).cloned() else {
            return;
        };
        let Some(first) = ready.iter().find(|p| {
            p.file_name().map(|n| unrar::is_first_rar_volume(&n.to_string_lossy())).unwrap_or(false)
        }) else {
            return;
        };
        let options = unrar::UnrarOptions::from_collection(&self.settings, &collection);
        let dest = unrar::target_dir(&self.settings, &collection);
        self.log("Unrar", &format!("Direct unpack started: {} -> {}", collection.subject, dest.display()));
        let direct = unrar::spawn_direct(id, first.clone(), PathBuf::from(&collection.folder), dest, options);
        if let Ok(mut list) = direct.ready.lock() {
            *list = ready.clone();
        }
        self.active_direct = Some(direct);
        self.direct_result.insert(id, None);
    }

    fn poll_direct(&mut self) {
        let Some(direct) = self.active_direct.as_ref() else {
            return;
        };
        let id = direct.collection_id;
        let downloading = self.active_dl.as_ref().map(|a| a.id) == Some(id);
        let over = direct.finished.load(std::sync::atomic::Ordering::Relaxed);
        if !downloading && !over {
            // The download went away without finishing (deleted, stopped).
            direct.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let Some(result) = direct.poll() else {
            return;
        };
        self.active_direct = None;
        let subject = self.queue.iter().find(|c| c.id == id).map(|c| c.subject.clone()).unwrap_or_default();
        let ok = match result {
            Ok(message) => {
                self.log("Unrar", &format!("Direct unpack: {message}"));
                true
            }
            Err(unrar::UnrarError::PasswordRequired) => {
                self.log("Unrar", "Direct unpack needs a password; unpacking after the download.");
                false
            }
            Err(unrar::UnrarError::Failed(error)) => {
                self.log("Unrar", &format!("Direct unpack stopped ({error}); unpacking after the download."));
                false
            }
        };
        self.direct_result.insert(id, Some(ok));
        if !downloading && over {
            // The download finished first and is waiting for us.
            if ok {
                self.toast("Unpack complete");
                self.notify("Unpack complete", &subject);
            }
            self.schedule_after_download(id);
        } else if !downloading {
            self.direct_result.remove(&id);
        }
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

    pub fn poll_unrar(&mut self) {
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
                self.notify("Unpack complete", &subject);
                self.sort_output(active.collection_id);
                self.run_execute_hook(active.collection_id);
            }
            Some(Err(unrar::UnrarError::PasswordRequired)) => {
                if let Some(c) = self.queue.iter_mut().find(|c| c.id == active.collection_id) {
                    c.status = ItemStatus::Completed;
                }
                self.events.push(Event::PasswordNeeded(active.collection_id));
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
                self.events.push(Event::Message("Unrar error".into(), error));
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

    pub fn all_tasks_done(&self) -> bool {
        self.queue
            .iter()
            .all(|c| matches!(c.status, ItemStatus::Completed | ItemStatus::Error))
    }

    pub fn roll_transfer_stats(&mut self) {
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

    pub fn save_statistics(&self) {
        if self.demo {
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

    pub fn tick(&mut self) {
        crate::tools::configure(&self.settings.unrar_path, &self.settings.par2_path, &self.settings.sevenzip_path);
        let now = Instant::now();
        let dt = (now - self.last_tick).as_secs_f64().clamp(0.0, 0.5);
        self.last_tick = now;
        self.poll_unrar();
        self.poll_direct();
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

        self.free_gb = free_gb(&self.settings.download_dir);
        let free_mb = self.free_gb * 1024.0;
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
            active.throttle_bps.store(self.speed_limit_bps().to_bits(), Ordering::Relaxed);
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
                let month = chrono::Local::now().format("%Y-%m").to_string();
                let age = self.queue[i].age_days;
                let servers: Vec<Server> = self
                    .settings
                    .enabled_servers()
                    .into_iter()
                    .filter(|s| s.usable_for(age, &month))
                    .cloned()
                    .collect();
                if servers.is_empty() && !self.no_server_warned {
                    self.no_server_warned = true;
                    self.log("Queue", "No server can take the next job (retention or monthly quota); waiting.");
                }
                if !servers.is_empty() {
                    self.no_server_warned = false;
                    self.job_servers = servers.iter().map(|s| (s.address.clone(), s.port, s.username.clone())).collect();
                    self.job_server_bytes = vec![0; servers.len()];
                    let mut job = download::job_for_collection(
                        &self.queue[i],
                        self.settings.skip_existing,
                        self.settings.retry_count,
                        self.settings.retry_delay,
                        self.settings.download_priority,
                        self.settings.article_cache_mb,
                        self.settings.cache_cleanup_minutes,
                        ProxyConfig::from_settings(&self.settings),
                    );
                    job.precheck = self.settings.precheck_articles;
                    job.max_missing = repair_budget(&self.queue[i]);
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
            self.drive_direct(&snapshot);
            self.count_server_bytes(&snapshot.server_bytes);
            let mut clear = false;
            let mut delta = 0.0;
            let mut completed: Option<(String, u64)> = None;
            let mut completed_id: Option<u64> = None;
            let mut count_collection = false;
            let mut failed: Option<(String, String)> = None;
            let abort_hopeless = self.settings.abort_hopeless;
            if let Some(c) = self.queue.iter_mut().find(|c| c.id == snapshot.id) {
                // More is missing than the PAR2 set can rebuild: stop now
                // instead of downloading the rest for nothing.
                let judged = matches!(snapshot.stage, download::Stage::Downloading | download::Stage::Incomplete);
                if abort_hopeless && judged {
                    if let Some(budget) = repair_budget(c).filter(|b| snapshot.missing > *b) {
                        c.status = ItemStatus::Error;
                        failed = Some((
                            c.subject.clone(),
                            format!(
                                "Stopped: {} articles are missing; the PAR2 set can rebuild about {budget}.",
                                snapshot.missing
                            ),
                        ));
                        clear = true;
                    }
                }
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
                    _ if failed.is_some() => {}
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
            let (source, total_bytes) = self
                .queue
                .iter()
                .find(|c| c.id == snapshot.id)
                .map(|c| (c.source.clone(), c.total_bytes))
                .unwrap_or_default();
            if let Some((subject, total)) = completed {
                self.log("Main", &format!("Download complete: {subject}"));
                if count_collection {
                    self.notify("Download complete", &subject);
                } else {
                    self.notify("Download incomplete", &subject);
                }
                let status = if count_collection { "Completed" } else { "Incomplete" };
                self.add_history(subject, total, status, source.clone());
            }
            if let Some((subject, _)) = &failed {
                self.notify("Download failed", subject);
                self.add_history(subject.clone(), total_bytes, "Failed", source);
            }
            if let Some((subject, message)) = failed {
                self.log(
                    "Error",
                    &format!("Download failed for {subject}: {message}"),
                );
                self.events.push(Event::Message(
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
    }}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryItem {
    pub name: String,
    pub date: String,
    pub size: String,
    /// "Completed", "Incomplete" or "Failed" (empty in old files).
    pub status: String,
    /// The NZB it came from, for Retry.
    pub source: String,
}

impl Default for HistoryItem {
    fn default() -> Self {
        Self { name: String::new(), date: String::new(), size: String::new(), status: String::new(), source: String::new() }
    }
}

/// Posts a notification: ntfy gets plain text with a Title header, any
/// other URL a JSON body {"title", "text", "app"}.
pub fn send_webhook(url: &str, title: &str, text: &str) -> Result<(), String> {
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(10)).build();
    let result = if url.contains("ntfy") {
        agent.post(url).set("Title", title).send_string(text)
    } else {
        let body = serde_json::json!({ "title": title, "text": text, "app": "NewsQueue" });
        agent.post(url).set("Content-Type", "application/json").send_string(&body.to_string())
    };
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// How many missing articles the collection's PAR2 recovery volumes can
/// make up for (held ones included), estimated from their size and the
/// average article size. `None` when there is no PAR2 data to judge by.
pub fn repair_budget(c: &Collection) -> Option<usize> {
    let is_par2 = |n: &str| n.to_ascii_lowercase().ends_with(".par2");
    if !c.files.iter().any(|f| is_par2(&f.name)) {
        return None;
    }
    let recovery: u64 = c
        .files
        .iter()
        .filter(|f| par2::volume_blocks(&f.name).is_some())
        .map(|f| f.bytes)
        .sum();
    let (bytes, articles) = c
        .files
        .iter()
        .filter(|f| !is_par2(&f.name))
        .flat_map(|f| f.segments.iter())
        .fold((0u64, 0u64), |(b, n), s| (b + s.bytes, n + 1));
    if articles == 0 {
        return None;
    }
    let average = (bytes / articles).max(1);
    Some((recovery / average) as usize)
}

pub fn history_path() -> PathBuf {
    app_base().join("newsqueue-history.json")
}

pub fn load_history() -> Option<Vec<HistoryItem>> {
    let text = std::fs::read_to_string(history_path()).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_history(history: &[HistoryItem]) {
    if let Ok(text) = serde_json::to_string_pretty(history) {
        let _ = std::fs::write(history_path(), text);
    }
}

/// Persisted transfer totals. Only the current day and year are kept; older
/// buckets are intentionally not shown because the Statistics window has no
/// day picker yet.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TransferStats {
    #[serde(default)]
    pub day: String,
    #[serde(default)]
    pub year: i32,
    #[serde(default)]
    pub daily: [f64; 24],
    #[serde(default)]
    pub monthly: [f64; 12],
    #[serde(default)]
    pub collections: u64,
    #[serde(default)]
    pub startups: u64,
    #[serde(default)]
    pub longest_uptime_secs: u64,
}

impl TransferStats {
    pub fn normalize(&mut self) {
        let now = Local::now();
        self.normalize_for(now.year(), &now.format("%Y-%m-%d").to_string());
    }

    pub fn normalize_for(&mut self, year: i32, day: &str) {
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

pub fn stats_path() -> PathBuf {
    app_base().join("newsqueue-stats.json")
}

pub fn load_transfer_stats() -> TransferStats {
    let mut stats = std::fs::read_to_string(stats_path())
        .ok()
        .and_then(|text| serde_json::from_str::<TransferStats>(&text).ok())
        .unwrap_or_default();
    stats.normalize();
    stats
}

pub fn save_transfer_stats(stats: &TransferStats) {
    if let Ok(text) = serde_json::to_string_pretty(stats) {
        let _ = std::fs::write(stats_path(), text);
    }
}

pub fn demo_daily_transfer() -> [f64; 24] {
    let mut daily = [0.0f64; 24];
    for (index, value) in daily.iter_mut().enumerate() {
        *value = ((index as f64 * 0.73).sin().abs() * 240.0
            + (index as f64 * 0.31).cos().abs() * 80.0)
            .round();
    }
    daily
}

pub fn demo_monthly_transfer() -> [f64; 12] {
    [
        3_240.0, 2_980.0, 4_120.0, 3_760.0, 4_880.0, 5_120.0, 4_460.0, 4_910.0, 5_380.0,
        5_010.0, 4_720.0, 5_640.0,
    ]
}

/// The sample engine of screenshots and `newsqueue-server --demo`: neutral
/// C:/NewsQueue paths, one job downloading, one paused part-way, demo
/// connections and speed history. Front ends show it without ticking it.
pub fn showcase_engine() -> Engine {
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
    .map(|(i, (a, d, st, srv))| crate::model::ThreadRow { number: i as u32 + 1, action: a.into(), description: d, status: st.into(), server: srv.into() })
    .collect();
    engine.speed_history = (0..120).map(|i| (4.6 + 0.35 * ((i as f32) * 0.31).sin() + 0.15 * ((i as f32) * 0.9).cos()) * 1024.0 * 1024.0).collect();
    engine.speed_bps = 4.82 * 1024.0 * 1024.0;
    engine.connected = true;
    engine.free_gb = 412.33;
    engine
}

pub fn sample_history() -> Vec<HistoryItem> {
    vec![
        HistoryItem {
            name: "Quiet Harbor Sessions".into(),
            date: "2026-11-02 19:42".into(),
            size: "2.41 GB".into(),
            status: "Completed".into(),
            source: String::new(),
        },
        HistoryItem {
            name: "Field Notes Collection".into(),
            date: "2026-11-01 08:15".into(),
            size: "918.2 MB".into(),
            status: "Completed".into(),
            source: String::new(),
        },
        HistoryItem {
            name: "Toolkit Archive 2024".into(),
            date: "2026-10-29 22:05".into(),
            size: "4.12 GB".into(),
            status: "Failed".into(),
            source: String::new(),
        },
    ]
}

pub fn build_threads(
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

/// Turns an execute-hook template into a shell line that reads `{folder}`,
/// `{name}` and `{file}` from the `NQ_FOLDER`, `NQ_NAME` and `NQ_FILE`
/// environment variables instead of pasting them in as text. The NZB subject
/// comes from feeds and indexers, so a name such as `x & evil.exe` must stay
/// data. A placeholder outside quotes gets quotes of its own; one the user
/// already quoted is left inside their quotes.
pub fn hook_shell_command(template: &str, windows: bool) -> String {
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
pub fn hook_env_value(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !matches!(ch, '"' | '\r' | '\n' | '\0'))
        .collect()
}
