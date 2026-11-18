//! Commands a front end sends to the [`Engine`](crate::engine::Engine).
//!
//! Every queue edit goes through [`Engine::apply`](crate::engine::Engine::apply)
//! so a desktop window and a web front end change the queue the same way.
//! Collections are addressed by their stable `id`, never by row index.

use crate::engine::Engine;
use crate::model::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MoveDir {
    Top,
    Up,
    Down,
    Bottom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Command {
    Connect,
    Disconnect,
    TogglePause,
    PauseFor(u32),
    SetSpeedLimit(Option<u8>),
    Pause(u64, bool),
    Delete(u64),
    Move(u64, MoveDir),
    SetExpanded(u64, bool),
    SetAllExpanded(bool),
    SetFolder(u64, String),
    SetUnrar(u64, bool),
    SetUnrarFolder(u64, String),
    SetExecute(u64, String),
    Rename(u64, String),
    Unpack(u64),
    /// Store the password on the collection and retry the unpack.
    SubmitPassword(u64, String),
    /// No password: the collection is marked `Error`.
    CancelPassword(u64),
    Cleanup,
    RefreshRss,
    /// Parse an NZB file on disk and queue it (optionally paused).
    AddNzbFile(String, bool),
}

/// What [`Engine::apply`] did, for front ends that track a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applied {
    Done,
    /// The collection moved to this row.
    MovedTo(usize),
    /// The collection was removed.
    Removed,
    /// No collection has that id, or nothing changed.
    Ignored,
    /// A collection was queued with this id.
    Added(u64),
    /// The command failed; the reason is also logged.
    Failed,
}

impl Engine {
    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.queue.iter().position(|c| c.id == id)
    }

    fn subject_of(&self, id: u64) -> String {
        self.queue
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.subject.clone())
            .unwrap_or_else(|| format!("collection {id}"))
    }

    pub fn apply(&mut self, command: Command) -> Applied {
        use Command::*;
        match command {
            Connect => self.connect(),
            Disconnect => self.disconnect(),
            TogglePause => self.toggle_pause(),
            PauseFor(m) => self.pause_for(m),
            SetSpeedLimit(tag) => self.set_speed_limit(tag),
            Cleanup => self.cleanup(),
            RefreshRss => self.refresh_rss(),
            AddNzbFile(path, paused) => {
                return match parse_nzb(std::path::Path::new(&path)) {
                    Ok(nzb) => Applied::Added(self.add_nzb_from_parsed(nzb, path, paused)),
                    Err(error) => {
                        self.log("Error", &format!("Cannot add {path}: {error}"));
                        Applied::Failed
                    }
                };
            }
            SetAllExpanded(v) => {
                for c in &mut self.queue {
                    c.expanded = v;
                }
            }
            Pause(id, pause) => return self.pause_collection(id, pause),
            Delete(id) => {
                let Some(i) = self.index_of(id) else { return Applied::Ignored };
                let c = self.queue.remove(i);
                self.log("Queue", &format!("Deleted: {}", c.subject));
                return Applied::Removed;
            }
            Move(id, dir) => {
                let Some(i) = self.index_of(id) else { return Applied::Ignored };
                let last = self.queue.len() - 1;
                let j = match dir {
                    MoveDir::Top => 0,
                    MoveDir::Up => i.saturating_sub(1),
                    MoveDir::Down => (i + 1).min(last),
                    MoveDir::Bottom => last,
                };
                if i != j {
                    let item = self.queue.remove(i);
                    self.queue.insert(j, item);
                }
                return Applied::MovedTo(j);
            }
            SetExpanded(id, v) => {
                let Some(c) = self.queue.iter_mut().find(|c| c.id == id) else {
                    return Applied::Ignored;
                };
                c.expanded = v;
            }
            SetFolder(id, folder) => {
                let Some(c) = self.queue.iter_mut().find(|c| c.id == id) else {
                    return Applied::Ignored;
                };
                c.folder = folder;
                let subject = c.subject.clone();
                self.log("Queue", &format!("Download folder changed for {subject}"));
            }
            SetUnrar(id, v) => {
                let Some(c) = self.queue.iter_mut().find(|c| c.id == id) else {
                    return Applied::Ignored;
                };
                c.unrar = v;
                if v && c.status == ItemStatus::Completed {
                    self.start_unrar(id);
                }
            }
            SetUnrarFolder(id, folder) => {
                let Some(c) = self.queue.iter_mut().find(|c| c.id == id) else {
                    return Applied::Ignored;
                };
                c.unrar_folder = folder;
                let subject = c.subject.clone();
                self.log("Queue", &format!("Unrar folder changed for {subject}"));
            }
            SetExecute(id, execute) => {
                let Some(c) = self.queue.iter_mut().find(|c| c.id == id) else {
                    return Applied::Ignored;
                };
                c.execute = execute;
                c.execute_done = false;
            }
            Rename(id, subject) => {
                let folder = PathBuf::from(&self.settings.download_dir)
                    .join(sanitize_folder(&subject))
                    .to_string_lossy()
                    .to_string();
                let Some(c) = self.queue.iter_mut().find(|c| c.id == id) else {
                    return Applied::Ignored;
                };
                c.subject = subject;
                c.folder = folder;
                self.log("Queue", "Collection renamed.");
            }
            Unpack(id) => {
                if self.index_of(id).is_none() {
                    return Applied::Ignored;
                }
                self.start_unrar(id);
            }
            SubmitPassword(id, password) => {
                let subject = self.subject_of(id);
                if let Some(c) = self.queue.iter_mut().find(|c| c.id == id) {
                    c.password = password;
                    c.status = ItemStatus::Completed;
                }
                self.log(
                    "Unrar",
                    &format!("Retrying unpack with the supplied password: {subject}"),
                );
                self.start_unrar(id);
            }
            CancelPassword(id) => {
                let subject = self.subject_of(id);
                if let Some(c) = self.queue.iter_mut().find(|c| c.id == id) {
                    c.status = ItemStatus::Error;
                }
                self.log(
                    "Unrar",
                    &format!("Unpack cancelled: a password is required for {subject}."),
                );
            }
        }
        Applied::Done
    }

    fn pause_collection(&mut self, id: u64, pause: bool) -> Applied {
        let active_id = self.active_dl.as_ref().map(|a| a.id);
        let Some(c) = self.queue.iter_mut().find(|c| c.id == id) else {
            return Applied::Ignored;
        };
        if pause {
            if c.status == ItemStatus::Completed {
                return Applied::Ignored;
            }
            c.status = ItemStatus::Paused;
        } else if c.status == ItemStatus::Paused {
            // Only the item that already has a worker is Downloading; anything
            // else goes back to Queued or the starter would never pick it up.
            c.status = if active_id == Some(id) {
                ItemStatus::Downloading
            } else {
                ItemStatus::Queued
            };
        } else {
            return Applied::Ignored;
        }
        let subject = c.subject.clone();
        self.log(
            "Queue",
            &format!("{}: {subject}", if pause { "Paused" } else { "Unpaused" }),
        );
        Applied::Done
    }

    /// Removes completed entries and finished PAR2 sets.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_address_collections_by_id() {
        let mut e = Engine::new(Settings::default(), true);
        assert!(e.queue.len() >= 3);
        let id = e.queue[2].id;
        assert_eq!(e.apply(Command::Move(id, MoveDir::Top)), Applied::MovedTo(0));
        assert_eq!(e.queue[0].id, id);
        e.apply(Command::Rename(id, "New name".into()));
        assert_eq!(e.queue[0].subject, "New name");
        e.apply(Command::SetExecute(id, "echo".into()));
        assert_eq!(e.queue[0].execute, "echo");
        assert_eq!(e.apply(Command::Delete(id)), Applied::Removed);
        assert!(e.index_of(id).is_none());
        assert_eq!(e.apply(Command::Delete(id)), Applied::Ignored);
    }
}
