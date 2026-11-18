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
    /// To this row (clamped), e.g. after a drag.
    To(usize),
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
    /// Queue the NZB of history entry N again.
    RetryHistory(usize),
    /// Replace some settings (a JSON object; private keys are refused).
    MergeSettings(serde_json::Value),
    /// Replace the server list with these JSON objects. Each one is laid
    /// over the current server at its `index` (if given), so fields a front
    /// end does not know survive; an empty or missing password keeps the
    /// current one (front ends never see passwords).
    SetServers(Vec<serde_json::Value>),
    /// Replace the RSS feed URLs.
    SetRssUrls(Vec<String>),
    /// Replace every setting (the desktop's Setup window; it holds the
    /// passwords itself). Saved at once.
    ReplaceSettings(Box<Settings>),
    /// Verify/repair the PAR2 set at this row of the PAR2 list.
    StartPar2Set(usize),
    /// Periodic RSS refresh on or off.
    SetRssTimer(bool),
    PauseAll(bool),
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
            RetryHistory(index) => return self.retry_history(index),
            MergeSettings(patch) => {
                return match self.settings.merged(&patch) {
                    Ok(settings) => {
                        self.settings = settings;
                        if !self.demo {
                            self.settings.save();
                        }
                        let keys: Vec<&String> = patch.as_object().map(|m| m.keys().collect()).unwrap_or_default();
                        self.log("Main", &format!("Settings changed: {}", keys.iter().map(|k| k.as_str()).collect::<Vec<_>>().join(", ")));
                        Applied::Done
                    }
                    Err(error) => {
                        self.log("Error", &format!("Settings not changed: {error}"));
                        Applied::Failed
                    }
                };
            }
            SetServers(patches) => {
                let mut servers = Vec::new();
                for patch in patches {
                    let Some(obj) = patch.as_object() else {
                        self.log("Error", "Servers not changed: each server must be a JSON object");
                        return Applied::Failed;
                    };
                    let old = obj.get("index").and_then(|v| v.as_u64()).and_then(|i| self.settings.servers.get(i as usize)).cloned();
                    let mut value = serde_json::to_value(old.clone().unwrap_or_default()).unwrap_or_default();
                    if let Some(map) = value.as_object_mut() {
                        for (k, v) in obj {
                            if k == "index" || (k == "password" && v.as_str().is_none_or(str::is_empty)) {
                                continue;
                            }
                            map.insert(k.clone(), v.clone());
                        }
                    }
                    match serde_json::from_value::<Server>(value) {
                        Ok(s) => servers.push(s),
                        Err(e) => {
                            self.log("Error", &format!("Servers not changed: {e}"));
                            return Applied::Failed;
                        }
                    }
                }
                self.settings.servers = servers;
                if !self.demo {
                    self.settings.save();
                }
                self.log("Main", &format!("Servers changed: {} configured", self.settings.servers.len()));
                return Applied::Done;
            }
            SetRssUrls(urls) => {
                self.settings.rss_urls = urls.into_iter().map(|u| u.trim().to_string()).filter(|u| !u.is_empty()).collect();
                if !self.demo {
                    self.settings.save();
                }
                self.log("RSS", &format!("Feeds changed: {} configured", self.settings.rss_urls.len()));
                return Applied::Done;
            }
            ReplaceSettings(settings) => {
                self.settings = *settings;
                if !self.demo {
                    self.settings.save();
                }
                self.log("Main", "Settings saved.");
                return Applied::Done;
            }
            StartPar2Set(index) => {
                if index >= self.par2.len() {
                    return Applied::Ignored;
                }
                self.start_par2_for_set(index);
            }
            SetRssTimer(on) => {
                self.rss_timer = on;
                self.log("RSS", if on { "RSS timer started." } else { "RSS timer stopped." });
            }
            PauseAll(pause) => {
                let ids: Vec<u64> = self.queue.iter().map(|c| c.id).collect();
                for id in ids {
                    self.pause_collection(id, pause);
                }
            }
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
                    MoveDir::To(n) => n.min(last),
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

    fn nzb_with(names: &[&str]) -> crate::model::ParsedNzb {
        crate::model::ParsedNzb {
            name: "Rel".into(),
            total_bytes: 100 * names.len() as u64,
            parts: names.len() as u32,
            files: names
                .iter()
                .map(|n| crate::model::ParsedFile {
                    name: n.to_string(),
                    bytes: 100,
                    parts: 1,
                    segments: vec![crate::model::Segment { number: 1, bytes: 100, id: format!("{n}@x") }],
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn duplicates_are_warned_or_added_paused() {
        let mut e = Engine::new(Settings::default(), true);
        e.queue.clear();
        e.history.clear();
        e.add_history("Rel".into(), 1, "Completed", String::new());
        e.settings.duplicate_check = 1;
        let id = e.add_nzb_from_parsed(nzb_with(&["a.rar"]), String::new(), false);
        assert!(e.logs.iter().any(|l| l.message.starts_with("Duplicate: Rel")));
        assert_eq!(e.queue[e.index_of(id).unwrap()].status, crate::model::ItemStatus::Queued);
        e.settings.duplicate_check = 2;
        let id = e.add_nzb_from_parsed(nzb_with(&["a.rar"]), String::new(), false);
        assert_eq!(e.queue[e.index_of(id).unwrap()].status, crate::model::ItemStatus::Paused);
        // A failed attempt in the history is no reason to warn.
        e.queue.clear();
        e.history.clear();
        e.add_history("Rel".into(), 1, "Failed", String::new());
        e.logs.clear();
        e.add_nzb_from_parsed(nzb_with(&["a.rar"]), String::new(), false);
        assert!(!e.logs.iter().any(|l| l.message.starts_with("Duplicate")));
    }

    #[test]
    fn history_entries_can_be_retried() {
        let dir = std::env::temp_dir().join(format!("newsqueue-retry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let nzb = dir.join("again.nzb");
        std::fs::write(
            &nzb,
            r#"<?xml version="1.0"?><nzb xmlns="http://www.newzbin.com/DTD/2003/nzb"><file poster="p" date="1" subject="again.rar (1/1)"><groups><group>a.b</group></groups><segments><segment bytes="10" number="1">x@y</segment></segments></file></nzb>"#,
        )
        .unwrap();
        let mut e = Engine::new(Settings::default(), true);
        e.queue.clear();
        e.history.clear();
        e.add_history("again".into(), 10, "Failed", nzb.to_string_lossy().to_string());
        e.add_history("gone".into(), 10, "Failed", dir.join("gone.nzb").to_string_lossy().to_string());
        assert_eq!(e.apply(Command::RetryHistory(0)), Applied::Failed, "gone.nzb is missing");
        assert!(matches!(e.apply(Command::RetryHistory(1)), Applied::Added(_)));
        assert_eq!(e.queue.len(), 1);
        assert_eq!(e.apply(Command::RetryHistory(9)), Applied::Ignored);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn notifications_post_json_or_ntfy_text() {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut head = String::new();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap();
                    }
                    if line == "\r\n" {
                        break;
                    }
                    head.push_str(&line);
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
                seen.push((head, String::from_utf8(body).unwrap()));
            }
            seen
        });
        crate::engine::send_webhook(&format!("http://127.0.0.1:{port}/hook"), "Download complete", "Rel").unwrap();
        crate::engine::send_webhook(&format!("http://127.0.0.1:{port}/ntfy/topic"), "Download failed", "Rel").unwrap();
        let seen = server.join().unwrap();
        assert!(seen[0].0.contains("application/json"));
        assert!(seen[0].1.contains("\"title\":\"Download complete\"") && seen[0].1.contains("\"text\":\"Rel\""));
        assert!(seen[1].0.contains("Title: Download failed"), "{}", seen[1].0);
        assert_eq!(seen[1].1, "Rel");
    }

    #[test]
    fn servers_respect_retention_and_monthly_quota() {
        let mut s = crate::model::Server::default();
        assert!(s.usable_for(5000.0, "2026-11"));
        s.max_age = 1000;
        assert!(s.usable_for(999.0, "2026-11"));
        assert!(!s.usable_for(1001.0, "2026-11"));
        s.quota_gb = 1;
        s.quota_month = "2026-11".into();
        s.used_bytes = 1_000_000_000;
        assert!(!s.usable_for(1.0, "2026-11"), "quota used up");
        assert!(s.usable_for(1.0, "2026-12"), "a new month starts at zero");
    }

    #[test]
    fn direct_unpack_extracts_while_downloading() {
        use crate::download::tests::{local_server, mock_server_counted, yenc_part};
        if !crate::unrar::unrar_available() {
            return;
        }
        let root = std::env::temp_dir().join(format!("newsqueue-direct-engine-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let fixtures = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/multi"));
        let names = ["multi.part1.rar", "multi.part2.rar", "multi.part3.rar"];
        let articles = names
            .iter()
            .map(|n| {
                let data = std::fs::read(fixtures.join(n)).unwrap();
                (format!("{n}@x"), yenc_part(&data, data.len(), 1))
            })
            .collect();
        let (port, _) = mock_server_counted(articles);
        let mut s = Settings::default();
        s.servers = vec![local_server(port, 1)];
        s.download_dir = root.to_string_lossy().to_string();
        s.low_free_space_mb = 0;
        s.direct_unpack = true;
        s.auto_unrar = true;
        s.par2_checking = false;
        s.sort_folder = root.join("sorted").to_string_lossy().to_string();
        s.sort_pattern = "{date}/{name}".into();
        let mut e = Engine::new(s, true);
        e.queue.clear();
        let mut nzb = nzb_with(&names);
        for (file, n) in nzb.files.iter_mut().zip(names) {
            let len = std::fs::metadata(fixtures.join(n)).unwrap().len();
            file.bytes = len;
            file.segments[0].bytes = len;
        }
        let id = e.add_nzb_from_parsed(nzb, String::new(), false);
        let i = e.index_of(id).unwrap();
        e.queue[i].unrar = true;
        e.connect();
        let started = std::time::Instant::now();
        let mut saw_direct = false;
        while started.elapsed() < std::time::Duration::from_secs(20) {
            e.tick();
            saw_direct |= e.active_direct.is_some();
            if e.active_dl.is_none() && e.active_direct.is_none() && e.active_unrar.is_none() && saw_direct {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(saw_direct, "direct unpack never started");
        assert!(e.logs.iter().any(|l| l.message.starts_with("Direct unpack: ")), "{:?}", e.logs.iter().map(|l| &l.message).collect::<Vec<_>>());
        assert!(!e.logs.iter().any(|l| l.message.starts_with("Unpack started")), "no second unpack");
        let found = walk(&root).into_iter().find(|p| p.ends_with("payload.bin")).expect("payload extracted");
        assert_eq!(std::fs::metadata(&found).unwrap().len(), 150_000);
        assert!(found.starts_with(root.join("sorted")), "moved by the sort pattern: {}", found.display());
        assert!(found.parent().unwrap().ends_with("Rel"));
        // The bytes count against the server's monthly use.
        assert!(e.settings.servers[0].used_bytes >= 150_000, "{}", e.settings.servers[0].used_bytes);
        assert_eq!(e.history[0].status, "Completed");
        let _ = std::fs::remove_dir_all(root);
    }

    fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = entry.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
        out
    }

    #[test]
    fn a_download_par2_cannot_save_is_stopped() {
        use crate::download::tests::{local_server, mock_server_counted, yenc_part};
        let root = std::env::temp_dir().join(format!("newsqueue-hopeless-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        // Only the first of five a.rar articles exists; the set has one
        // article's worth of recovery data.
        let (port, _) = mock_server_counted(vec![("a.rar@x".into(), yenc_part(b"ABCDE", 5, 1))]);
        let mut s = Settings::default();
        s.servers = vec![local_server(port, 2)];
        s.download_dir = root.to_string_lossy().to_string();
        s.low_free_space_mb = 0;
        let mut e = Engine::new(s, true);
        e.queue.clear();
        let mut nzb = nzb_with(&["a.rar", "a.par2", "a.vol0+1.par2"]);
        nzb.files[0].segments = (1..=5)
            .map(|n| crate::model::Segment {
                number: n,
                bytes: 100,
                id: if n == 1 { "a.rar@x".into() } else { format!("gone{n}@x") },
            })
            .collect();
        nzb.files[0].bytes = 500;
        let id = e.add_nzb_from_parsed(nzb, String::new(), false);
        e.connect();
        let started = std::time::Instant::now();
        while started.elapsed() < std::time::Duration::from_secs(10) {
            e.tick();
            let status = e.queue[e.index_of(id).unwrap()].status;
            if !matches!(status, crate::model::ItemStatus::Queued | crate::model::ItemStatus::Downloading) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(e.queue[e.index_of(id).unwrap()].status, crate::model::ItemStatus::Error);
        assert!(e.logs.iter().any(|l| l.message.contains("articles are missing; the PAR2 set can rebuild about 1")));
        assert!(e.active_dl.is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn repair_budget_comes_from_the_recovery_volumes() {
        let mut e = Engine::new(Settings::default(), true);
        // 100-byte articles; 300 bytes of recovery volumes: about 3 articles.
        let id = e.add_nzb_from_parsed(
            nzb_with(&["a.rar", "b.rar", "a.par2", "a.vol0+1.par2", "a.vol1+2.par2", "a.vol3+4.par2"]),
            String::new(),
            true,
        );
        let c = &e.queue[e.index_of(id).unwrap()];
        assert_eq!(crate::engine::repair_budget(c), Some(3));
        let id = e.add_nzb_from_parsed(nzb_with(&["a.rar", "b.rar"]), String::new(), true);
        assert_eq!(crate::engine::repair_budget(&e.queue[e.index_of(id).unwrap()]), None);
        let id = e.add_nzb_from_parsed(nzb_with(&["a.rar", "a.par2"]), String::new(), true);
        assert_eq!(crate::engine::repair_budget(&e.queue[e.index_of(id).unwrap()]), Some(0));
    }

    #[test]
    fn par2_volumes_are_held_then_released_on_demand() {
        let mut e = Engine::new(Settings::default(), true);
        e.queue.clear();
        let id = e.add_nzb_from_parsed(
            nzb_with(&["Rel.part1.rar", "Rel.par2", "Rel.vol00+01.par2", "Rel.vol01+02.par2", "Rel.vol03+04.par2"]),
            String::new(),
            true,
        );
        let c = &e.queue[e.index_of(id).unwrap()];
        let held: Vec<&str> = c.files.iter().filter(|f| f.held).map(|f| f.name.as_str()).collect();
        assert_eq!(held, ["Rel.vol00+01.par2", "Rel.vol01+02.par2", "Rel.vol03+04.par2"]);
        let job = crate::download::job_for_collection(c, false, 0, 1, 3, 0, 30, Default::default());
        assert_eq!(job.files.iter().filter(|f| f.skip).count(), 3);

        // A verify that is two blocks short releases the +02 volume only.
        assert!(e.release_par2_volumes(id, 2));
        let c = &e.queue[e.index_of(id).unwrap()];
        assert_eq!(c.status, crate::model::ItemStatus::Queued);
        let fetch: Vec<&str> = c.files.iter().filter(|f| !f.held && !f.done).map(|f| f.name.as_str()).collect();
        assert_eq!(fetch, ["Rel.vol01+02.par2"]);
        let job = crate::download::job_for_collection(c, false, 0, 1, 3, 0, 30, Default::default());
        assert_eq!(job.files.iter().filter(|f| !f.skip).count(), 1);

        // Off: nothing is held; a set without an index .par2 is never held.
        let mut s = Settings::default();
        s.par2_on_demand = false;
        let mut e = Engine::new(s, true);
        let id = e.add_nzb_from_parsed(nzb_with(&["a.rar", "a.par2", "a.vol0+1.par2"]), String::new(), true);
        assert!(e.queue[e.index_of(id).unwrap()].files.iter().all(|f| !f.held));
        let mut e = Engine::new(Settings::default(), true);
        let id = e.add_nzb_from_parsed(nzb_with(&["a.rar", "a.vol0+1.par2"]), String::new(), true);
        assert!(e.queue[e.index_of(id).unwrap()].files.iter().all(|f| !f.held));
        assert!(!e.release_par2_volumes(id, 1));
    }
}
