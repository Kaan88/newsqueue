use chrono::{DateTime, Datelike, Local, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
pub const APP_NAME: &str = "NewsQueue";
pub const VERSION: &str = "0.48.4";
pub const BUILD_DATE: &str = "2024-10-07";
pub const MAX_SPEED_KBPS: f64 = 55.0;
pub const MAX_SPEED_MBPS: f64 = 0.44;
#[allow(dead_code)]
pub const MAX_SPEED_BPS: f64 = 55_000.0;
#[allow(dead_code)]
pub const SPEED_LIMIT_STEP_BPS: f64 = 5_500.0;
/// 0.44 Mbit/s expressed in bytes per second (0.44 * 125000).
pub const BPS_PER_MBPS: f64 = 125_000.0;
/// Function 0x8345d4 keeps a four-sample, one-second speed average.
pub const SPEED_SAMPLE_COUNT: usize = 4;
/// Function 0x8345d4 raises the ceiling for at most 60 one-second ticks
/// when the configured maximum speed is 0 (Auto).
pub const AUTO_SPEED_WINDOW_TICKS: u32 = 60;
pub const DEFAULT_CONNECTIONS: u32 = 8;
pub const MAX_CONNECTIONS_PER_SERVER: u32 = 200;
pub const MAX_SERVERS_PER_LEVEL: u32 = 8;
pub const SSL_PORT: u16 = 563;
pub const PLAIN_PORT: u16 = 119;
pub const CONNECTION_RETRY_DELAY: u32 = 10;
pub const CONNECTION_TIMEOUT: u32 = 30;
pub const XPAT_TIMEOUT: u32 = 300;
pub const RETRY_COUNT: u32 = 0;
pub const RETRY_DELAY: u32 = 5;
pub const LOW_FREE_SPACE_MB: u64 = 1000;
pub const AUTOSAVE_MINUTES: u32 = 5;
pub const MAX_PATH_LENGTH: u32 = 200;
pub const LOG_KEEP_DAYS: u32 = 7;
pub const LOG_MAX_LINES: usize = 200;
pub const MINUTE_STATS_MS: u64 = 60_000;
pub const HOUR_STATS_MS: u64 = 3_600_000;
pub const SCHEDULE_SLOTS: usize = 48;
pub const SCHEDULE_SLOT_MINUTES: u32 = 30;
pub const THREAD_PANEL_HEIGHT: f32 = 278.0;
pub const STATUS_BAR_HEIGHT: f32 = 21.0;
pub const TOOLBAR_HEIGHT: f32 = 55.0;
pub const TOOLBAR_BUTTON_WIDTH: f32 = 55.0;
pub const TOOLBAR_BUTTON_HEIGHT: f32 = 53.0;
pub const DQ_TOOLBAR_HEIGHT: f32 = 30.0;
pub const TAB_BAR_HEIGHT: f32 = 26.0;
#[allow(dead_code)]
pub const CLIENT_WIDTH: f32 = 1490.0;
#[allow(dead_code)]
pub const CLIENT_HEIGHT: f32 = 668.0;
pub const SAVED_WIDTH: f32 = 1285.0;
pub const SAVED_HEIGHT: f32 = 881.0;
pub const QUEUE_COLUMN_WIDTHS: [f32; 12] = [
    390.0, 95.0, 95.0, 95.0, 95.0, 151.0, 95.0, 220.0, 38.0, 95.0, 50.0, 50.0,
];
pub const QUEUE_COLUMN_NAMES: [&str; 12] = [
    "Subject",
    "Size",
    "Parts",
    "Age",
    "Group",
    "Poster",
    "Status",
    "Download folder",
    "Unrar",
    "Execute",
    "Delete archives",
    "Posted on",
];
pub const EXPLORER_COLUMN_NAMES: [&str; 9] = [
    "Name",
    "Size",
    "Type",
    "Modified",
    "Attributes",
    "Accessed",
    "Created",
    "DOS Name",
    "Path",
];
pub const MB: f64 = 1024.0 * 1024.0;
pub const GB: f64 = MB * 1024.0;
pub const TB: f64 = GB * 1024.0;

/// Function 0x817178: `trunc(tag * MaxSpeedMbps / 10 * 125000)`.
/// Tag 0 is "No limit" and yields 0 (unthrottled).
pub fn speed_limit_bps(tag: u8, ceiling_mbps: f64) -> f64 {
    (tag as f64 * ceiling_mbps / 10.0 * BPS_PER_MBPS).trunc()
}

/// Functions 0x817178 and 0x835e38 divide the global limit by the number of
/// active connections; zero active connections means no per-connection limit.
pub fn per_connection_limit(global_bps: f64, active_connections: u32) -> f64 {
    if active_connections > 0 {
        (global_bps / active_connections as f64).trunc()
    } else {
        0.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemStatus {
    Queued,
    Downloading,
    Paused,
    Repairing,
    Unpacking,
    Completed,
    Incomplete,
    Error,
}
impl ItemStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Downloading => "Downloading",
            Self::Paused => "Paused",
            Self::Repairing => "Repairing",
            Self::Unpacking => "Unpacking",
            Self::Completed => "Download complete",
            Self::Incomplete => "Incomplete",
            Self::Error => "Error!",
        }
    }
    pub fn is_active(self) -> bool {
        matches!(self, Self::Downloading | Self::Repairing | Self::Unpacking)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Segment {
    pub number: u32,
    pub bytes: u64,
    pub id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueueFile {
    pub name: String,
    pub bytes: u64,
    pub downloaded: f64,
    pub parts: u32,
    #[serde(default)]
    pub segments: Vec<Segment>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Collection {
    pub id: u64,
    pub subject: String,
    pub total_bytes: u64,
    pub downloaded: f64,
    pub parts: u32,
    pub age_days: f64,
    pub group: String,
    pub poster: String,
    pub status: ItemStatus,
    pub folder: String,
    #[serde(default)]
    pub unrar_folder: String,
    pub unrar: bool,
    pub execute: String,
    pub expanded: bool,
    pub files: Vec<QueueFile>,
    pub posted_on: String,
    pub delete_archives: bool,
    pub category: String,
    pub password: String,
    pub source: String,
}
impl Collection {
    pub fn progress(&self) -> f32 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        (self.downloaded / self.total_bytes as f64).clamp(0.0, 1.0) as f32
    }
    pub fn remaining(&self) -> f64 {
        (self.total_bytes as f64 - self.downloaded).max(0.0)
    }
    pub fn advance(&mut self, bytes: f64) {
        if self.total_bytes == 0 {
            return;
        }
        self.downloaded = (self.downloaded + bytes).min(self.total_bytes as f64);
        let mut left = bytes;
        for file in &mut self.files {
            if left <= 0.0 {
                break;
            }
            let room = file.bytes as f64 - file.downloaded;
            if room > 0.0 {
                let add = room.min(left);
                file.downloaded += add;
                left -= add;
            }
        }
        if self.downloaded >= self.total_bytes as f64 {
            self.downloaded = self.total_bytes as f64;
            for file in &mut self.files {
                file.downloaded = file.bytes as f64;
            }
            self.status = ItemStatus::Completed;
        }
    }
    pub fn file(&self, index: usize) -> Option<&QueueFile> {
        self.files.get(index)
    }
    pub fn has_segments(&self) -> bool {
        self.files.iter().any(|f| !f.segments.is_empty())
    }
}
#[derive(Clone, Debug)]
pub struct ThreadRow {
    pub number: u32,
    pub action: String,
    pub description: String,
    pub status: String,
    pub server: String,
}
#[derive(Clone, Debug)]
pub struct Par2Set {
    pub name: String,
    pub size_mb: f64,
    pub blocks: u32,
    pub info: String,
    pub unrar: bool,
    pub progress: f32,
    pub status: String,
    pub folder: String,
    pub collection_id: Option<u64>,
}
#[derive(Clone, Debug)]
pub struct LogEntry {
    pub time: String,
    pub level: String,
    pub message: String,
}
#[derive(Clone, Debug)]
pub struct RssItem {
    pub name: String,
    pub date: String,
    pub description: String,
    pub url: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Category {
    pub name: String,
    pub download_folder: String,
    pub use_group: bool,
    pub use_name: bool,
    pub unrar: bool,
    pub unrar_folder: String,
    pub unrar_group: bool,
    pub unrar_name: bool,
    pub keywords: String,
}
impl Default for Category {
    fn default() -> Self {
        Self {
            name: "Default".into(),
            download_folder: String::new(),
            use_group: false,
            use_name: true,
            unrar: false,
            unrar_folder: String::new(),
            unrar_group: false,
            unrar_name: false,
            keywords: String::new(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Server {
    pub name: String,
    pub address: String,
    pub port: u16,
    pub connections: u32,
    pub ssl: bool,
    pub auth: bool,
    pub username: String,
    pub password: String,
    pub enabled: bool,
    pub level: u32,
    pub max_age: u32,
    pub select_group: bool,
    pub article_cmd: bool,
    pub connected_idle: bool,
    pub nic: String,
    pub purpose: String,
}
impl Default for Server {
    fn default() -> Self {
        // New installs start unconfigured; the account lives in the local,
        // gitignored settings file. The saved profile in the original uses SSL
        // on port 563 with eight connections.
        Self {
            name: "Usenet".into(),
            address: String::new(),
            port: SSL_PORT,
            connections: DEFAULT_CONNECTIONS,
            ssl: true,
            auth: true,
            username: String::new(),
            password: String::new(),
            enabled: false,
            level: 0,
            max_age: 0,
            select_group: false,
            article_cmd: false,
            connected_idle: true,
            nic: "default".into(),
            purpose: "Level 0 (Primary)".into(),
        }
    }
}
pub fn app_base() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(mut exe_dir) = exe.parent() {
            if exe_dir.file_name().and_then(|s| s.to_str()) == Some("deps") {
                if let Some(parent) = exe_dir.parent() {
                    exe_dir = parent;
                }
            }
            let name = exe_dir.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name == "debug" || name == "release" {
                if let Some(project) = exe_dir.parent().and_then(|p| p.parent()) {
                    return project.to_path_buf();
                }
            }
            return exe_dir.to_path_buf();
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Move a path saved under an older project directory into this app folder.
fn rebase_app_path(path: &str, base: &Path) -> String {
    let lower = path.to_ascii_lowercase();
    if let Some(idx) = lower.find("\\app\\") {
        let project = lower[..idx].rsplit('\\').next().unwrap_or("");
        if project == "altbinz" || project == "altbinz-copy" {
            let rest = &path[idx + "\\app\\".len()..];
            return base.join(rest).to_string_lossy().to_string();
        }
    }
    path.to_string()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub download_dir: String,
    pub nzb_dir: String,
    pub temp_dir: String,
    pub unrar_dir: String,
    pub max_speed_kbps: f64,
    pub max_speed_mbps: f64,
    pub connections_per_thread: u32,
    pub connection_retry_delay: u32,
    pub connection_timeout: u32,
    pub xpat_timeout: u32,
    pub retry_count: u32,
    pub retry_delay: u32,
    pub connected_while_idle: bool,
    pub grace_period: u32,
    pub low_free_space_mb: u64,
    pub max_path_length: u32,
    pub autosave_minutes: u32,
    pub cleanup_queue: bool,
    pub skip_existing: bool,
    pub prompt_for_name: bool,
    pub dblclick_expand: bool,
    pub minimize_to_tray: bool,
    pub tray_balloon: bool,
    pub speed_in_tray: bool,
    pub progress_taskbar: bool,
    pub check_version_minutes: u32,
    pub status_xml: bool,
    pub clear_history_on_exit: bool,
    pub log_enabled: bool,
    pub log_to_file: bool,
    pub keep_log_days: u32,
    pub max_log_lines: usize,
    pub detailed_log: bool,
    pub download_history: bool,
    pub speed_and_bytes: bool,
    pub par2_checking: bool,
    pub par2_repairing: bool,
    pub cleanup_after_repair: bool,
    pub delete_small_par2: bool,
    pub save_par2_info: bool,
    pub auto_unrar: bool,
    pub create_dir_nzb_name: bool,
    pub create_dir_group_name: bool,
    pub auto_rename_to_collection: bool,
    pub delete_empty_folder: bool,
    pub pause_while_unraring: bool,
    pub move_files_to_unrar: String,
    pub delete_files_after_unrar: String,
    pub overwrite_existing: bool,
    pub delete_archives: bool,
    pub strip_root: bool,
    pub rename_existing: bool,
    pub unrar_to_different: bool,
    pub proxy_enabled: bool,
    pub proxy_address: String,
    pub proxy_port: u16,
    pub proxy_auth: bool,
    pub proxy_username: String,
    pub proxy_password: String,
    pub max_connections_per_server: u32,
    pub max_servers_per_level: u32,
    pub queue_bytes_mode: u8,
    pub speed_bytes_mode: u8,
    pub transfered_bytes_mode: u8,
    pub thread_list_height: f32,
    pub thread_list_visible: bool,
    pub show_tabs: [bool; 5],
    pub show_captions: bool,
    pub column_visible: [bool; 12],
    pub column_widths: [f32; 12],
    pub categories: Vec<Category>,
    pub servers: Vec<Server>,
    pub notes: String,
    pub scheduler: [String; 7],
    pub scheduler_enabled: bool,
    pub shutdown_method: String,
    pub auto_import_folder: String,
    pub delete_nzb_after_import: bool,
    pub merge_nzb: bool,
    pub auto_import_paused: bool,
    pub sort_nzb: bool,
    pub sort_rars: bool,
    pub add_to_top: bool,
    pub pause_nzb: bool,
    pub pause_samples: bool,
    pub pause_par2: bool,
    pub autofocus: bool,
    pub autofocus_search: bool,
    pub alternate_rename: bool,
    pub disable_undocking: bool,
    pub tray_single_click: bool,
    pub keep_nfo: bool,
    pub case_insensitive_filters: bool,
    pub search_history_limit: u32,
    pub dropdown_limit: u32,
    pub rss_refresh_minutes: u32,
    #[serde(default)]
    pub rss_urls: Vec<String>,
    #[serde(default)]
    pub rss_filters: String,
    #[serde(default)]
    pub newznab_url: String,
    #[serde(default)]
    pub newznab_api_key: String,
    pub auto_cleanup_minutes: u32,
    pub player_exe: String,
    pub delete_rars_after_extract: bool,
    pub default_execute: String,
    pub hide_execute_window: bool,
    pub article_cache_mb: u32,
    pub cache_cleanup_minutes: u32,
    pub decoder_sleep_lines: u32,
    pub download_priority: u32,
    pub decode_priority: u32,
    pub par2_priority: u32,
    pub show_images: bool,
    pub small_images: bool,
    pub exp_folders_visible: bool,
    pub exp_folders_width: f32,
    pub explorer_columns: [bool; 9],
    pub explorer_column_widths: [f32; 9],
    pub explorer_favorites: Vec<String>,
    pub startup_connect: bool,
    pub startup_pause: bool,
    pub startup_scheduler: bool,
    pub startup_speed_limit: Option<u8>,
}
impl Default for Settings {
    fn default() -> Self {
        let base = app_base();
        let dir = |name: &str| base.join(name).to_string_lossy().to_string();
        let download = dir("download");
        Self {
            download_dir: download.clone(),
            nzb_dir: dir("nzb"),
            temp_dir: dir("temp"),
            unrar_dir: download.clone(),
            max_speed_kbps: MAX_SPEED_KBPS,
            max_speed_mbps: MAX_SPEED_MBPS,
            connections_per_thread: 1,
            connection_retry_delay: CONNECTION_RETRY_DELAY,
            connection_timeout: CONNECTION_TIMEOUT,
            xpat_timeout: XPAT_TIMEOUT,
            retry_count: RETRY_COUNT,
            retry_delay: RETRY_DELAY,
            connected_while_idle: true,
            grace_period: 0,
            low_free_space_mb: LOW_FREE_SPACE_MB,
            max_path_length: MAX_PATH_LENGTH,
            autosave_minutes: AUTOSAVE_MINUTES,
            cleanup_queue: true,
            skip_existing: true,
            prompt_for_name: true,
            dblclick_expand: false,
            minimize_to_tray: true,
            tray_balloon: true,
            speed_in_tray: true,
            progress_taskbar: true,
            check_version_minutes: 1440,
            status_xml: false,
            clear_history_on_exit: false,
            log_enabled: true,
            log_to_file: true,
            keep_log_days: LOG_KEEP_DAYS,
            max_log_lines: LOG_MAX_LINES,
            detailed_log: true,
            download_history: true,
            speed_and_bytes: true,
            par2_checking: true,
            par2_repairing: true,
            cleanup_after_repair: true,
            delete_small_par2: false,
            save_par2_info: true,
            auto_unrar: false,
            create_dir_nzb_name: false,
            create_dir_group_name: false,
            auto_rename_to_collection: true,
            delete_empty_folder: false,
            pause_while_unraring: false,
            move_files_to_unrar: String::new(),
            delete_files_after_unrar: String::new(),
            overwrite_existing: false,
            delete_archives: false,
            strip_root: true,
            rename_existing: true,
            unrar_to_different: false,
            proxy_enabled: false,
            proxy_address: String::new(),
            proxy_port: 8080,
            proxy_auth: false,
            proxy_username: String::new(),
            proxy_password: String::new(),
            max_connections_per_server: MAX_CONNECTIONS_PER_SERVER,
            max_servers_per_level: MAX_SERVERS_PER_LEVEL,
            queue_bytes_mode: 1,
            speed_bytes_mode: 3,
            transfered_bytes_mode: 0,
            thread_list_height: THREAD_PANEL_HEIGHT,
            thread_list_visible: true,
            show_tabs: [true; 5],
            show_captions: true,
            column_visible: [
                true, true, true, true, true, true, true, true, true, true, false, false,
            ],
            column_widths: QUEUE_COLUMN_WIDTHS,
            categories: vec![Category {
                download_folder: download.clone(),
                unrar_folder: download,
                ..Category::default()
            }],
            servers: vec![Server::default()],
            notes: String::new(),
            scheduler: std::array::from_fn(|_| "10".repeat(24)),
            scheduler_enabled: false,
            shutdown_method: "Exit".into(),
            auto_import_folder: String::new(),
            delete_nzb_after_import: false,
            merge_nzb: true,
            auto_import_paused: false,
            sort_nzb: true,
            sort_rars: false,
            add_to_top: false,
            pause_nzb: true,
            pause_samples: true,
            pause_par2: true,
            autofocus: false,
            autofocus_search: false,
            alternate_rename: false,
            disable_undocking: false,
            tray_single_click: false,
            keep_nfo: true,
            case_insensitive_filters: true,
            search_history_limit: 10,
            dropdown_limit: 10,
            rss_refresh_minutes: 15,
            rss_urls: Vec::new(),
            rss_filters: String::new(),
            newznab_url: String::new(),
            newznab_api_key: String::new(),
            auto_cleanup_minutes: 0,
            player_exe: String::new(),
            delete_rars_after_extract: false,
            default_execute: String::new(),
            hide_execute_window: false,
            article_cache_mb: 0,
            cache_cleanup_minutes: 30,
            decoder_sleep_lines: 4000,
            download_priority: 3,
            decode_priority: 3,
            par2_priority: 3,
            show_images: true,
            small_images: false,
            exp_folders_visible: true,
            exp_folders_width: 250.0,
            explorer_columns: [true, true, true, true, true, false, false, false, false],
            explorer_column_widths: [200.0; 9],
            explorer_favorites: Vec::new(),
            startup_connect: false,
            startup_pause: false,
            startup_scheduler: false,
            startup_speed_limit: None,
        }
    }
}
impl Settings {
    pub fn load() -> Self {
        let path = settings_path();
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(mut s) = serde_json::from_str::<Settings>(&text) {
                let mut changed = false;
                if s.download_dir.is_empty() {
                    s.download_dir = Settings::default().download_dir;
                    changed = true;
                }
                // The project was renamed from altbinz-copy to newsqueue. Saved
                // settings can still point at the old app/ directory, which
                // makes downloads and unrar look in the wrong place.
                let base = app_base();
                for value in [
                    &mut s.download_dir,
                    &mut s.nzb_dir,
                    &mut s.temp_dir,
                    &mut s.unrar_dir,
                ] {
                    let rebased = rebase_app_path(value, &base);
                    if rebased != *value {
                        *value = rebased;
                        changed = true;
                    }
                }
                for category in &mut s.categories {
                    for value in [
                        &mut category.download_folder,
                        &mut category.unrar_folder,
                    ] {
                        let rebased = rebase_app_path(value, &base);
                        if rebased != *value {
                            *value = rebased;
                            changed = true;
                        }
                    }
                }
                let is_old_demo = s.servers.len() == 1
                    && s.servers[0].address.eq_ignore_ascii_case("news.example.com");
                if is_old_demo {
                    s.servers[0] = Server::default();
                    changed = true;
                }
                if s.servers.is_empty() {
                    s.servers.push(Server::default());
                    changed = true;
                }
                if changed {
                    s.save();
                }
                return s;
            }
        }
        Self::default()
    }
    pub fn save(&self) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(settings_path(), text);
        }
    }
    pub fn enabled_servers(&self) -> Vec<&Server> {
        self.servers.iter().filter(|s| s.enabled).collect()
    }
    pub fn total_connections(&self) -> u32 {
        self.servers
            .iter()
            .filter(|s| s.enabled)
            .map(|s| s.connections)
            .sum::<u32>()
            .max(1)
    }
}
pub fn settings_path() -> PathBuf {
    app_base().join("newsqueue-settings.json")
}

pub fn queue_state_path() -> PathBuf {
    app_base().join("newsqueue-queue.json")
}

pub fn load_queue_state() -> Option<Vec<Collection>> {
    let text = std::fs::read_to_string(queue_state_path()).ok()?;
    let mut queue: Vec<Collection> = serde_json::from_str(&text).ok()?;
    let base = app_base();
    for collection in &mut queue {
        collection.folder = rebase_app_path(&collection.folder, &base);
        collection.unrar_folder = rebase_app_path(&collection.unrar_folder, &base);
    }
    Some(queue)
}

pub fn save_queue_state(queue: &[Collection]) {
    if let Ok(text) = serde_json::to_string_pretty(queue) {
        let _ = std::fs::write(queue_state_path(), text);
    }
}
pub fn ensure_dirs(settings: &Settings) {
    for path in [
        &settings.download_dir,
        &settings.nzb_dir,
        &settings.temp_dir,
        &settings.unrar_dir,
    ] {
        let _ = std::fs::create_dir_all(path);
    }
}

pub fn prune_old_logs(keep_days: u32) {
    if keep_days == 0 {
        return;
    }
    let Ok(reader) = std::fs::read_dir(app_base().join("log")) else {
        return;
    };
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(keep_days as u64 * 86_400));
    let Some(cutoff) = cutoff else { return };
    for entry in reader.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("log") {
            continue;
        }
        if entry
            .metadata()
            .ok()
            .and_then(|meta| meta.modified().ok())
            .map(|modified| modified < cutoff)
            .unwrap_or(false)
        {
            let _ = std::fs::remove_file(path);
        }
    }
}
pub fn file_name_from_subject(subject: &str) -> String {
    if let Some(start) = subject.find('"') {
        if let Some(end) = subject[start + 1..].find('"') {
            return subject[start + 1..start + 1 + end].to_string();
        }
    }
    let mut s = subject.trim().to_string();
    if s.len() > 90 {
        s.truncate(90);
    }
    s
}
pub fn sanitize_folder(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, ' ' | '.' | '-' | '_' | '(' | ')' | '[' | ']') {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    let out = out.trim().trim_matches('.').to_string();
    if out.is_empty() {
        "collection".into()
    } else {
        out
    }
}
#[derive(Clone, Debug)]
pub struct ParsedFile {
    pub name: String,
    pub bytes: u64,
    pub parts: u32,
    pub segments: Vec<Segment>,
}
pub struct ParsedNzb {
    pub name: String,
    pub total_bytes: u64,
    pub parts: u32,
    pub group: String,
    pub poster: String,
    pub posted_on: String,
    pub age_days: f64,
    pub files: Vec<ParsedFile>,
}
#[derive(Default)]
struct NzbFileDraft {
    subject: String,
    bytes: u64,
    parts: u32,
    segments: Vec<Segment>,
}
fn local_xml_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}

pub fn parse_nzb(path: &Path) -> Result<ParsedNzb, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut reader = quick_xml::Reader::from_str(&text);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut files: Vec<ParsedFile> = Vec::new();
    let mut name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Imported NZB".into());
    let mut group = String::new();
    let mut poster = String::new();
    let mut date = String::new();
    let mut cur: Option<NzbFileDraft> = None;
    let mut in_group = false;
    let mut in_segment = false;
    let mut seg_draft: Option<Segment> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) => {
                let tag = local_xml_name(e.name().as_ref()).to_vec();
                match tag.as_slice() {
                    b"file" => {
                        let mut subject = String::new();
                        let mut fposter = String::new();
                        let mut fdate = String::new();
                        for attr in e.attributes().flatten() {
                            let key = attr.key.as_ref();
                            let value = attr
                                .unescape_value()
                                .map(|v| v.to_string())
                                .unwrap_or_else(|_| {
                                    String::from_utf8_lossy(&attr.value).to_string()
                                });
                            match key {
                                b"subject" => subject = value,
                                b"poster" => fposter = value,
                                b"date" => fdate = value,
                                _ => {}
                            }
                        }
                        if !subject.is_empty() && name == "Imported NZB" {
                            name = subject.clone();
                        }
                        if poster.is_empty() {
                            poster = fposter;
                        }
                        if date.is_empty() {
                            date = fdate;
                        }
                        cur = Some(NzbFileDraft {
                            subject,
                            ..NzbFileDraft::default()
                        });
                    }
                    b"group" => in_group = true,
                    b"segment" => {
                        let mut seg = Segment::default();
                        for attr in e.attributes().flatten() {
                            let v = String::from_utf8_lossy(&attr.value);
                            match attr.key.as_ref() {
                                b"bytes" => seg.bytes = v.parse::<u64>().unwrap_or(0),
                                b"number" => seg.number = v.parse::<u32>().unwrap_or(0),
                                _ => {}
                            }
                        }
                        seg_draft = Some(seg);
                        in_segment = true;
                    }
                    _ => {}
                }
            }
            Ok(quick_xml::events::Event::Empty(e)) => {
                if local_xml_name(e.name().as_ref()) == b"segment" {
                    let mut seg = Segment::default();
                    for attr in e.attributes().flatten() {
                        let v = String::from_utf8_lossy(&attr.value);
                        match attr.key.as_ref() {
                            b"bytes" => seg.bytes = v.parse::<u64>().unwrap_or(0),
                            b"number" => seg.number = v.parse::<u32>().unwrap_or(0),
                            _ => {}
                        }
                    }
                    if let Some(cur) = cur.as_mut() {
                        cur.bytes += seg.bytes;
                        cur.parts += 1;
                        cur.segments.push(seg);
                    }
                }
            }
            Ok(quick_xml::events::Event::Text(e)) => {
                if in_segment {
                    if let Some(seg) = seg_draft.as_mut() {
                        seg.id.push_str(&e.unescape().unwrap_or_default());
                    }
                } else if in_group {
                    let g = e.unescape().unwrap_or_default().to_string();
                    if group.is_empty() {
                        group = g;
                    }
                }
            }
            Ok(quick_xml::events::Event::End(e)) => match local_xml_name(e.name().as_ref()) {
                b"segment" => {
                    if let (Some(cur), Some(seg)) = (cur.as_mut(), seg_draft.take()) {
                        let seg = Segment {
                            number: if seg.number == 0 {
                                cur.parts + 1
                            } else {
                                seg.number
                            },
                            id: seg.id.trim().to_string(),
                            ..seg
                        };
                        cur.bytes += seg.bytes;
                        cur.parts += 1;
                        cur.segments.push(seg);
                    }
                    in_segment = false;
                }
                b"file" => {
                    if let Some(draft) = cur.take() {
                        let name = file_name_from_subject(&draft.subject);
                        files.push(ParsedFile {
                            name,
                            bytes: draft.bytes,
                            parts: draft.parts,
                            segments: draft.segments,
                        });
                    }
                }
                b"group" => in_group = false,
                _ => {}
            },
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => return Err(format!("Invalid NZB XML: {e}")),
            _ => {}
        }
        buf.clear();
    }
    if files.is_empty() {
        return Err("No files found in NZB".into());
    }
    let total_bytes = files.iter().map(|f| f.bytes).sum::<u64>();
    let parts = files.iter().map(|f| f.parts).sum::<u32>();
    let age_days = if let Ok(secs) = date.parse::<i64>() {
        let then = Utc.timestamp_opt(secs, 0).single();
        if let Some(then) = then {
            (Utc::now() - then).num_hours().max(0) as f64 / 24.0
        } else {
            0.0
        }
    } else {
        0.0
    };
    let posted_on = if let Ok(secs) = date.parse::<i64>() {
        Utc.timestamp_opt(secs, 0)
            .single()
            .map(|d| d.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let name = name
        .trim_matches('"')
        .trim_end_matches(".nzb")
        .trim_end_matches(".NZB")
        .to_string();
    Ok(ParsedNzb {
        name,
        total_bytes,
        parts,
        group,
        poster,
        posted_on,
        age_days,
        files,
    })
}
pub fn now_time() -> String {
    Local::now().format("%H:%M:%S").to_string()
}
pub fn now_datetime() -> String {
    Local::now().format("%Y-%m-%d %H:%M").to_string()
}
#[allow(dead_code)]
pub fn add_thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}
pub fn add_thousands_f(n: f64, decimals: usize) -> String {
    let s = format!("{:.*}", decimals, n);
    let (int_part, frac) = s.split_once('.').unwrap_or((&s, ""));
    let neg = int_part.starts_with('-');
    let digits = int_part.trim_start_matches('-');
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if neg {
        out.insert(0, '-');
    }
    if decimals > 0 {
        out.push('.');
        out.push_str(frac);
    }
    out
}
/// Generic auto formatter used for log lines. The status bar uses the
/// measured display modes below.
pub fn format_bytes_auto(bytes: f64) -> String {
    if bytes >= TB {
        format!("{:.2} TB", bytes / TB)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes / GB)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes / MB)
    } else if bytes >= 1024.0 {
        format!("{:.2} kB", bytes / 1024.0)
    } else {
        format!("{:.0} B", bytes)
    }
}
pub fn format_size_short(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= 1024.0 {
        format!("{:.1} kB", b / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Function 0x8060f0. Modes 0..3 are fixed units and use the Delphi format
/// strings `#,#0`, `#,#0.0`, `#,#0.00`; mode 4/6 is automatic kB/MB and
/// mode 8 is the decimal-kB variant. Units are the language-table strings.
pub fn format_bytes_mode(bytes: f64, mode: u8) -> String {
    match mode {
        0 => format!("{} kB", add_thousands_f(bytes / 1024.0, 0)),
        1 => format!("{} MB", add_thousands_f(bytes / MB, 2)),
        2 => format!("{} GB", add_thousands_f(bytes / GB, 2)),
        3 => format!("{} TB", add_thousands_f(bytes / TB, 2)),
        4 | 6 => {
            if bytes < MB {
                format!("{} kB", add_thousands_f(bytes / 1024.0, 0))
            } else {
                format!("{} MB", add_thousands_f(bytes / MB, 2))
            }
        }
        8 => {
            if bytes < MB {
                format!("{} kB", add_thousands_f(bytes / 1000.0, 0))
            } else {
                format!("{} MB", add_thousands_f(bytes / 1_000_000.0, 2))
            }
        }
        _ => format_bytes_auto(bytes),
    }
}

/// Queue-size panel (function 0x80dc2c) passes `QueueBytesMode` straight to
/// the formatter: 0 kB, 1 MB, 2 GB, 3 TB, 4 automatic.
pub fn format_queue_bytes(bytes: f64, mode: u8) -> String {
    format_bytes_mode(bytes, mode)
}

/// Transfered panel (function 0x836048): mode 0 -> MB, 1 -> GB, 2 -> TB,
/// 3 -> automatic kB/MB. The saved value is 0, so the panel shows MB.
pub fn format_transfered_bytes(bytes: f64, mode: u8) -> String {
    match mode {
        0 => format_bytes_mode(bytes, 1),
        1 => format_bytes_mode(bytes, 2),
        2 => format_bytes_mode(bytes, 3),
        _ => format_bytes_mode(bytes, 6),
    }
}

/// Functions 0x8340ec and 0x834530. Divisors are the measured constants:
/// 1024, 1048576, 1000 and 1000000; bit rates multiply by 8.
pub fn format_speed(bps: f64, mode: u8) -> String {
    match mode {
        0 => format!("{} KiB/s", add_thousands_f(bps / 1024.0, 0)),
        1 => format!(
            "{} MiB/s",
            add_thousands_f(bps / (1024.0 * 1024.0), 2)
        ),
        2 => format!("{} kbit/s", add_thousands_f(bps * 8.0 / 1000.0, 2)),
        3 => format!(
            "{} Mbit/s",
            add_thousands_f(bps * 8.0 / 1_000_000.0, 2)
        ),
        4 => format!("{} kB/s", add_thousands_f(bps / 1000.0, 0)),
        5 => format!(
            "{} MB/s",
            add_thousands_f(bps / 1_000_000.0, 2)
        ),
        6 | 7 => {
            if bps < 125_000.0 {
                format!("{} kbit/s", add_thousands_f(bps * 8.0 / 1000.0, 2))
            } else {
                format!(
                    "{} Mbit/s",
                    add_thousands_f(bps * 8.0 / 1_000_000.0, 2)
                )
            }
        }
        _ => format!(
            "{} Mbit/s",
            add_thousands_f(bps * 8.0 / 1_000_000.0, 2)
        ),
    }
}

/// Function 0x809f1c: ETA in whole seconds split into d/h/m/s components.
/// Zero is shown as `Unknown` by the caller because there is no measurable
/// speed; otherwise the nonzero components are joined with spaces.
pub fn format_eta(seconds: f64) -> String {
    if !seconds.is_finite() || seconds <= 0.0 {
        return "Unknown".into();
    }
    let total = seconds.floor() as u64;
    let d = total / 86_400;
    let h = (total % 86_400) / 3_600;
    let m = (total % 3_600) / 60;
    let s = total % 60;
    let mut parts = Vec::new();
    if d > 0 {
        parts.push(format!("{d}d"));
    }
    if h > 0 {
        parts.push(format!("{h}h"));
    }
    if m > 0 {
        parts.push(format!("{m}m"));
    }
    if s > 0 {
        parts.push(format!("{s}s"));
    }
    if parts.is_empty() {
        "0s".into()
    } else {
        parts.join(" ")
    }
}
pub fn format_age(days: f64) -> String {
    format!("{:.1}d", days)
}
pub fn free_gb(path: &str) -> f64 {
    fs2::available_space(path).unwrap_or(0) as f64 / GB
}
pub fn sample_queue(settings: &Settings) -> Vec<Collection> {
    let dl = PathBuf::from(&settings.download_dir);
    let first_name = "Aurora.Field.Recordings.2025";
    let second_name = "Harbor.Lights.1080p";
    let third_name = "Workshop.Tools.Archive.2024";
    let mk = |name: &str, size_mb: f64, parts: u32, file_count: usize| -> Vec<QueueFile> {
        let total = size_mb * MB;
        let each = total / file_count as f64;
        (1..=file_count)
            .map(|i| QueueFile {
                name: format!("{}.part{:02}.rar", name, i),
                bytes: each as u64,
                downloaded: 0.0,
                parts: (parts / file_count as u32).max(1),
                segments: Vec::new(),
            })
            .collect()
    };
    let mut a = Collection {
        id: 1,
        subject: first_name.into(),
        total_bytes: (12_288.0 * MB) as u64,
        downloaded: 12_288.0 * MB,
        parts: 8,
        age_days: 12.4,
        group: "alt.binaries.movies".into(),
        poster: "northstar".into(),
        status: ItemStatus::Completed,
        folder: dl.join(first_name).to_string_lossy().to_string(),
        unrar_folder: String::new(),
        unrar: true,
        execute: String::new(),
        expanded: true,
        files: mk(first_name, 12_288.0, 8, 4),
        posted_on: "2026-10-21 21:14".into(),
        delete_archives: false,
        category: "Default".into(),
        password: String::new(),
        source: String::new(),
    };
    for f in &mut a.files {
        f.downloaded = f.bytes as f64;
    }
    let mut b = Collection {
        id: 2,
        subject: second_name.into(),
        total_bytes: (10_240.0 * MB) as u64,
        downloaded: 10_240.0 * MB,
        parts: 5,
        age_days: 3.1,
        group: "alt.binaries.tv".into(),
        poster: "lumen".into(),
        status: ItemStatus::Completed,
        folder: dl.join(second_name).to_string_lossy().to_string(),
        unrar_folder: String::new(),
        unrar: true,
        execute: String::new(),
        expanded: false,
        files: mk(second_name, 10_240.0, 5, 5),
        posted_on: "2026-10-31 08:02".into(),
        delete_archives: false,
        category: "Default".into(),
        password: String::new(),
        source: String::new(),
    };
    let mut c = Collection {
        id: 3,
        subject: third_name.into(),
        total_bytes: (11_305.32 * MB) as u64,
        downloaded: 11_305.32 * MB,
        parts: 12,
        age_days: 60.0,
        group: "alt.binaries.apps".into(),
        poster: "archivebot".into(),
        status: ItemStatus::Completed,
        folder: dl.join(third_name).to_string_lossy().to_string(),
        unrar_folder: String::new(),
        unrar: false,
        execute: String::new(),
        expanded: false,
        files: mk(third_name, 11_305.32, 12, 3),
        posted_on: "2026-09-04 17:40".into(),
        delete_archives: false,
        category: "Default".into(),
        password: String::new(),
        source: String::new(),
    };
    for f in b.files.iter_mut().chain(c.files.iter_mut()) {
        f.downloaded = f.bytes as f64;
    }
    vec![a, b, c]
}
pub fn sample_par2() -> Vec<Par2Set> {
    vec![
        Par2Set {
            name: "Aurora.Field.Recordings.par2".into(),
            size_mb: 1_024.0,
            blocks: 512,
            info: "1 block short".into(),
            unrar: true,
            progress: 0.65,
            status: "Repairing".into(),
            folder: String::new(),
            collection_id: None,
        },
        Par2Set {
            name: "Harbor.Lights.par2".into(),
            size_mb: 1_024.0,
            blocks: 512,
            info: "All files OK".into(),
            unrar: true,
            progress: 1.0,
            status: "OK".into(),
            folder: String::new(),
            collection_id: None,
        },
        Par2Set {
            name: "Workshop.Tools.sfv".into(),
            size_mb: 64.0,
            blocks: 0,
            info: "Checked".into(),
            unrar: false,
            progress: 1.0,
            status: "OK".into(),
            folder: String::new(),
            collection_id: None,
        },
    ]
}
pub fn sample_rss() -> Vec<RssItem> {
    vec![
        RssItem {
            name: "Daily Releases".into(),
            date: "2026-11-03 06:00".into(),
            description: "Automatic feed for the daily queue".into(),
            url: "http://example.invalid/daily".into(),
        },
        RssItem {
            name: "Weekend Picks".into(),
            date: "2026-11-02 18:30".into(),
            description: "Curated weekend list".into(),
            url: "http://example.invalid/weekend".into(),
        },
        RssItem {
            name: "Archive Updates".into(),
            date: "2026-11-01 12:15".into(),
            description: "New items in the archive category".into(),
            url: "http://example.invalid/archive".into(),
        },
        RssItem {
            name: "Notes and Docs".into(),
            date: "2026-10-30 09:45".into(),
            description: "Text and document releases".into(),
            url: "http://example.invalid/docs".into(),
        },
    ]
}
pub fn sample_log() -> VecDeque<LogEntry> {
    let mut out = VecDeque::new();
    out.push_back(LogEntry {
        time: now_time(),
        level: "Main".into(),
        message: format!("{APP_NAME} {VERSION}.0 ({BUILD_DATE}) started."),
    });
    out.push_back(LogEntry {
        time: now_time(),
        level: "Main".into(),
        message: "OS Windows 11".into(),
    });
    out.push_back(LogEntry {
        time: now_time(),
        level: "Main".into(),
        message: "0 items loaded into download history list in 0ms.".into(),
    });
    out.push_back(LogEntry {
        time: now_time(),
        level: "Main".into(),
        message: "0 outdated items removed from download history list in 0ms.".into(),
    });
    out.push_back(LogEntry {
        time: now_time(),
        level: "Main".into(),
        message: "Ready. No server connection is open.".into(),
    });
    out
}
pub fn queue_total_bytes(queue: &[Collection]) -> u64 {
    queue.iter().map(|c| c.total_bytes).sum()
}
pub fn queue_remaining_bytes(queue: &[Collection]) -> f64 {
    queue
        .iter()
        .filter(|c| c.status != ItemStatus::Completed)
        .map(|c| c.remaining())
        .sum()
}
pub fn active_download(queue: &[Collection]) -> Option<usize> {
    queue
        .iter()
        .position(|c| c.status == ItemStatus::Downloading)
}
pub fn scheduler_slot() -> usize {
    let now = Local::now();
    let minutes = now.hour() as usize * 60 + now.minute() as usize;
    (minutes / SCHEDULE_SLOT_MINUTES as usize) % SCHEDULE_SLOTS
}
pub fn scheduler_allows(settings: &Settings) -> bool {
    if !settings.scheduler_enabled {
        return true;
    }
    let day = Local::now().weekday().num_days_from_monday() as usize;
    let slot = scheduler_slot();
    settings
        .scheduler
        .get(day)
        .and_then(|s| s.as_bytes().get(slot))
        .map(|b| *b == b'1')
        .unwrap_or(true)
}
#[allow(dead_code)]
pub fn timestamp_from_str(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_str(s, "%Y-%m-%d %H:%M")
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_matches_mechanics() {
        assert_eq!(format_speed(55_000.0, 3), "0.44 Mbit/s");
        assert_eq!(format_queue_bytes(33_833.32 * MB, 1), "33,833.32 MB");
        assert_eq!(format_eta(0.0), "Unknown");
        assert_eq!(format_age(12.4), "12.4d");
        assert_eq!(SPEED_LIMIT_STEP_BPS * 9.0, 49_500.0);
    }

    #[test]
    fn speed_limit_uses_decompiled_formula() {
        // 0x817178: trunc(tag * 0.44 / 10 * 125000) = tag * 5500.
        assert_eq!(speed_limit_bps(9, MAX_SPEED_MBPS), 49_500.0);
        assert_eq!(speed_limit_bps(10, MAX_SPEED_MBPS), 55_000.0);
        assert_eq!(speed_limit_bps(0, MAX_SPEED_MBPS), 0.0);
        // 0x835e38 divides the global limit by the active connection count.
        assert_eq!(per_connection_limit(55_000.0, 8), 6_875.0);
        assert_eq!(per_connection_limit(55_000.0, 0), 0.0);
    }

    #[test]
    fn eta_and_units_use_measured_components() {
        assert_eq!(format_eta(3_661.0), "1h 1m 1s");
        assert_eq!(format_eta(90_061.0), "1d 1h 1m 1s");
        assert_eq!(format_bytes_mode(1024.0, 0), "1 kB");
        assert_eq!(format_bytes_mode(1.5 * MB, 1), "1.50 MB");
        assert_eq!(format_transfered_bytes(2.0 * GB, 0), "2,048.00 MB");
        assert_eq!(format_speed(55_000.0, 0), "54 KiB/s");
        assert_eq!(format_speed(55_000.0, 2), "440.00 kbit/s");
    }

    #[test]
    fn sample_queue_total_matches_status_bar() {
        let settings = Settings::default();
        let queue = sample_queue(&settings);
        let total_mb = queue_total_bytes(&queue) as f64 / MB;
        assert!((total_mb - 33_833.32).abs() < 0.1, "total was {total_mb}");
    }

    #[test]
    fn parses_a_minimal_nzb() {
        let dir = std::env::temp_dir().join("newsqueue-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("sample.nzb");
        let xml = r#"<?xml version="1.0"?>
<nzb xmlns="http://www.newzbin.com/DTD/2003/nzb">
<file subject="&quot;example.part01.rar&quot; yEnc (1/2)" poster="tester" date="1700000000">
<groups><group>alt.binaries.test</group></groups>
<segments>
<segment bytes="1024" number="1">a@example</segment>
<segment bytes="2048" number="2">b@example</segment>
</segments>
</file>
</nzb>"#;
        std::fs::write(&path, xml).unwrap();
        let parsed = parse_nzb(&path).unwrap();
        assert_eq!(parsed.files.len(), 1);
        assert_eq!(parsed.total_bytes, 3072);
        assert_eq!(parsed.parts, 2);
        assert_eq!(parsed.group, "alt.binaries.test");
        assert_eq!(parsed.poster, "tester");
        assert_eq!(parsed.files[0].name, "example.part01.rar");
        assert_eq!(parsed.files[0].segments.len(), 2);
        assert_eq!(parsed.files[0].segments[1].id, "b@example");
        assert_eq!(parsed.files[0].segments[1].bytes, 2048);
        let _ = std::fs::remove_file(&path);
    }
}
