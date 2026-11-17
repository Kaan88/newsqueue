//! Read-only view of the engine for front ends (GUI_PLAN Phase 0 step 5).
//!
//! The engine thread publishes a [`Snapshot`] after every tick; any number of
//! front ends read it through an [`EngineHandle`] and change the engine only
//! by sending [`Command`]s. A snapshot never contains secrets (no server
//! passwords, no archive passwords, no settings).

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
    pub queue_remaining_bytes: f64,
    pub queue: Vec<QueueRow>,
    /// The newest log lines, oldest first.
    pub log_tail: Vec<LogLine>,
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
