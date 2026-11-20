//! Read-only view of the engine for front ends (GUI_PLAN Phase 0 step 5).
//!
//! The engine thread publishes a [`Snapshot`] after every tick; any number of
//! front ends read it through an [`EngineHandle`] and change the engine only
//! by sending [`Command`]s. A snapshot never contains secrets (no server
//! passwords, no archive passwords, no private settings).

use crate::command::Command;
use crate::engine::Engine;
use crate::model::ItemStatus;
use serde::Serialize;
use std::sync::mpsc::Sender;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct QueueRow {
    pub id: u64,
    pub subject: String,
    pub status: Option<ItemStatus>,
    pub total_bytes: u64,
    pub remaining_bytes: f64,
    pub progress: f32,
    pub parts: u32,
    pub files: usize,
    pub group: String,
    pub folder: String,
    pub unrar: bool,
    pub has_password: bool,
    pub expanded: bool,
    pub age_days: f64,
    pub poster: String,
    pub execute: String,
    pub delete_archives: bool,
    pub posted_on: String,
    pub unrar_folder: String,
    /// The collection's files, in queue order.
    pub file_rows: Vec<FileRow>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct FileRow {
    pub name: String,
    pub bytes: u64,
    pub parts: u32,
    pub progress: f32,
    /// A PAR2 volume held back until a repair needs it.
    pub held: bool,
    pub done: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Par2Row {
    pub name: String,
    pub size_mb: f64,
    pub blocks: u32,
    pub info: String,
    pub unrar: bool,
    pub progress: f32,
    pub status: String,
}

/// An RSS item; its URL can hold an indexer key and is not included.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RssRow {
    pub name: String,
    pub date: String,
    pub description: String,
}

/// Counters for the Statistics window.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Stats {
    /// Bytes per hour of today and per month of this year.
    pub daily: Vec<f64>,
    pub monthly: Vec<f64>,
    pub session_bytes: u64,
    pub session_secs: u64,
    pub session_collections: u64,
    pub total_collections: u64,
    pub startups: u64,
    pub longest_uptime_secs: u64,
    /// Recent speed samples (bytes/s), oldest first.
    pub speed_history: Vec<f32>,
}

/// A server without its password. `username` is not serialized with the
/// snapshot; `GET /api/servers` adds it for the settings page.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ServerRow {
    pub name: String,
    pub address: String,
    pub port: u16,
    pub connections: u32,
    pub ssl: bool,
    pub enabled: bool,
    pub level: u32,
    pub auth: bool,
    pub max_age: u32,
    pub quota_gb: u32,
    pub used_bytes: u64,
    pub password_set: bool,
    #[serde(skip)]
    pub username: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct LogLine {
    pub time: String,
    pub level: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Snapshot {
    /// Increments on every publish, so readers can skip unchanged ones.
    pub seq: u64,
    pub connected: bool,
    pub paused: bool,
    pub speed_bps: f64,
    pub speed_limit_bps: f64,
    /// The speed-limit step (1-9 = 10-90 %), none = full speed.
    pub speed_limit_tag: Option<u8>,
    pub custom_speed_limit_mbs: Option<f64>,
    pub queue_remaining_bytes: f64,
    pub queue: Vec<QueueRow>,
    /// The newest log lines, oldest first.
    pub log_tail: Vec<LogLine>,
    pub history: Vec<crate::engine::HistoryItem>,
    /// `Settings::public_json`: no accounts, logins or private URLs.
    pub settings: serde_json::Value,
    /// Free space on the download drive.
    pub free_gb: f64,
    pub servers: Vec<ServerRow>,
    /// Feed URLs can hold indexer keys: never serialized with the snapshot,
    /// only sent by `GET /api/rss`.
    #[serde(skip)]
    pub rss_urls: Vec<String>,
    pub par2: Vec<Par2Row>,
    pub rss: Vec<RssRow>,
    pub rss_timer: bool,
    /// Download connections (the Connections panel).
    pub threads: Vec<crate::model::ThreadRow>,
    pub stats: Stats,
    /// Downloading, PAR2 and unpacking are all finished (shutdown waits for it).
    pub all_done: bool,
    pub scheduler_hold: bool,
    pub total_bytes: u64,
}

pub const LOG_TAIL: usize = 50;

impl Snapshot {
    pub fn of(engine: &Engine, seq: u64) -> Self {
        let skip = engine.logs.len().saturating_sub(LOG_TAIL);
        Self {
            seq,
            connected: engine.connected,
            paused: engine.paused,
            speed_bps: engine.speed_bps,
            speed_limit_bps: engine.speed_limit_bps(),
            speed_limit_tag: engine.speed_limit_tag,
            custom_speed_limit_mbs: engine.custom_speed_limit_mbs,
            queue_remaining_bytes: engine.queue_remaining(),
            queue: engine
                .queue
                .iter()
                .map(|c| QueueRow {
                    id: c.id,
                    subject: c.subject.clone(),
                    status: Some(c.status),
                    total_bytes: c.total_bytes,
                    remaining_bytes: c.remaining(),
                    progress: c.progress(),
                    parts: c.parts,
                    files: c.files.len(),
                    group: c.group.clone(),
                    folder: c.folder.clone(),
                    unrar: c.unrar,
                    has_password: !c.password.is_empty(),
                    expanded: c.expanded,
                    age_days: c.age_days,
                    poster: c.poster.clone(),
                    execute: c.execute.clone(),
                    delete_archives: c.delete_archives,
                    posted_on: c.posted_on.clone(),
                    unrar_folder: c.unrar_folder.clone(),
                    file_rows: c
                        .files
                        .iter()
                        .map(|f| FileRow {
                            name: f.name.clone(),
                            bytes: f.bytes,
                            parts: f.parts,
                            progress: if f.bytes == 0 { 0.0 } else { (f.downloaded / f.bytes as f64).clamp(0.0, 1.0) as f32 },
                            held: f.held,
                            done: f.done,
                        })
                        .collect(),
                })
                .collect(),
            log_tail: engine
                .logs
                .iter()
                .skip(skip)
                .map(|l| LogLine {
                    time: l.time.clone(),
                    level: l.level.clone(),
                    message: l.message.clone(),
                })
                .collect(),
            history: engine.history.clone(),
            settings: engine.settings.public_json(),
            free_gb: engine.free_gb,
            servers: engine
                .settings
                .servers
                .iter()
                .map(|s| ServerRow {
                    name: s.name.clone(),
                    address: s.address.clone(),
                    port: s.port,
                    connections: s.connections,
                    ssl: s.ssl,
                    enabled: s.enabled,
                    level: s.level,
                    auth: s.auth,
                    max_age: s.max_age,
                    quota_gb: s.quota_gb,
                    used_bytes: s.used_bytes,
                    password_set: !s.password.is_empty(),
                    username: s.username.clone(),
                })
                .collect(),
            rss_urls: engine.settings.rss_urls.clone(),
            par2: engine
                .par2
                .iter()
                .map(|p| Par2Row {
                    name: p.name.clone(),
                    size_mb: p.size_mb,
                    blocks: p.blocks,
                    info: p.info.clone(),
                    unrar: p.unrar,
                    progress: p.progress,
                    status: p.status.clone(),
                })
                .collect(),
            rss: engine.rss.iter().map(|r| RssRow { name: r.name.clone(), date: r.date.clone(), description: r.description.clone() }).collect(),
            rss_timer: engine.rss_timer,
            threads: engine.threads.clone(),
            stats: Stats {
                daily: engine.daily_transfer.to_vec(),
                monthly: engine.monthly_transfer.to_vec(),
                session_bytes: engine.transfered_session,
                session_secs: engine.session_start.elapsed().as_secs(),
                session_collections: engine.session_collections,
                total_collections: engine.total_collections,
                startups: engine.startups,
                longest_uptime_secs: engine.longest_uptime_secs,
                speed_history: engine.speed_history.iter().copied().collect(),
            },
            all_done: engine.all_tasks_done(),
            scheduler_hold: engine.scheduler_hold,
            total_bytes: engine.total_queue_size(),
        }
    }
}

/// What a front end holds: send commands, read the latest snapshot.
/// Cheap to clone; safe to move to another thread (e.g. a web server).
#[derive(Clone)]
pub struct EngineHandle {
    pub(crate) commands: Sender<Command>,
    pub(crate) snapshot: Arc<RwLock<Snapshot>>,
}

impl EngineHandle {
    /// Queues a command; it runs before the engine's next tick.
    /// Returns false when the engine is gone.
    pub fn send(&self, command: Command) -> bool {
        self.commands.send(command).is_ok()
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Reads the snapshot in place, without cloning it.
    pub fn read<R>(&self, f: impl FnOnce(&Snapshot) -> R) -> R {
        f(&self.snapshot.read().unwrap_or_else(|e| e.into_inner()))
    }
}
