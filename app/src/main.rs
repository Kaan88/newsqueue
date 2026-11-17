#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use newsqueue_core::command::{Applied, Command};
pub use newsqueue_core::command::MoveDir;
use newsqueue_core::engine::*;
use newsqueue_core::host::{EngineHost, TICK_INTERVAL};
use newsqueue_core::{download, model, par2, rss, unrar};
mod dialogs;
mod shutdown;
mod theme;
mod tray;
mod ui;
mod update;

use eframe::egui;
use model::*;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub collection: usize,
    pub file: Option<usize>,
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
    ToggleUnrar(bool),
    UnpackNow,
    ChangeUnrarFolder,
    SetExecute,
    OpenFolder,
    Eta,
    Rename,
    CollapseAll,
    ExpandAll,
    None,
}













pub struct App {
    pub engine: EngineHost,
    pub setup_draft: Settings,
    pub selected: Option<Selection>,
    pub selected_par2: Option<usize>,
    pub active_tab: usize,
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
    pub show_stats: bool,
    pub stats_tab: usize,
    pub show_about: bool,
    pub show_version: bool,
    pub update_rx: Option<std::sync::mpsc::Receiver<update::UpdateEvent>>,
    pub update_status: String,
    pub update_available: Option<update::Release>,
    pub update_notified: String,
    pub update_installing: bool,
    pub next_update_check: Option<Instant>,
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
    pub rss_feed_draft: String,
    pub rss_filters_draft: String,
    pub show_rss_feeds: bool,
    pub show_threads: bool,
    pub thread_height: f32,
    pub status_mode_speed: u8,
    pub status_mode_queue: u8,
    pub status_mode_transfered: u8,
    pub shutdown_armed: bool,
    pub shutdown_countdown: Option<Instant>,
    pub shutdown_mode: shutdown::ShutdownMode,
    pub shutdown_exit_requested: bool,
    pub shutdown_exit_sent: bool,
    pub show_execute_command: bool,
    pub execute_command_draft: String,
    pub execute_command_target: Option<usize>,
    pub toast: Option<(String, Instant)>,
    pub startup_done: bool,
}

impl std::ops::Deref for App {
    type Target = Engine;
    fn deref(&self) -> &Engine {
        &*self.engine
    }
}

impl std::ops::DerefMut for App {
    fn deref_mut(&mut self) -> &mut Engine {
        &mut *self.engine
    }
}

impl App {
    pub fn new(settings: Settings, screenshot_path: Option<String>) -> Self {
        let demo = screenshot_path.is_some();
        let thread_height = settings.thread_list_height;
        let show_threads = settings.thread_list_visible;
        let status_mode_speed = settings.speed_bytes_mode;
        let status_mode_queue = settings.queue_bytes_mode;
        let status_mode_transfered = settings.transfered_bytes_mode;
        let notes_draft = settings.notes.clone();
        let rss_feed_draft = settings.rss_urls.join("\n");
        let rss_filters_draft = settings.rss_filters.clone();
        let column_visible_draft = settings.column_visible;
        let column_width_draft = settings.column_widths;
        let setup_draft = settings.clone();
        let mut engine = EngineHost::new(Engine::new(settings, demo));
        if !demo {
            engine.start(TICK_INTERVAL);
        }
        let mut app = Self {
            engine,
            setup_draft,
            selected: None,
            selected_par2: None,
            active_tab: 0,
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
            show_stats: false,
            stats_tab: 0,
            show_about: false,
            show_version: false,
            update_rx: None,
            update_status: String::new(),
            update_available: None,
            update_notified: String::new(),
            update_installing: false,
            next_update_check: Some(Instant::now() + Duration::from_secs(15)),
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
            rss_feed_draft,
            rss_filters_draft,
            show_rss_feeds: false,
            show_threads,
            thread_height,
            status_mode_speed,
            status_mode_queue,
            status_mode_transfered,
            shutdown_armed: false,
            shutdown_countdown: None,
            shutdown_mode: shutdown::ShutdownMode::Real,
            shutdown_exit_requested: false,
            shutdown_exit_sent: false,
            show_execute_command: false,
            execute_command_draft: String::new(),
            execute_command_target: None,
            toast: None,
            startup_done: false,
        };
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
            Ok("notes") => app.show_notes = true,
            Ok("about") => app.show_about = true,
            Ok("columns") => app.show_columns = true,
            Ok("pause") => app.show_pause_for = true,
            _ => {}
        }
        if let Ok(tab) = std::env::var("NQ_DEMO_TAB") {
            app.active_tab = match tab.as_str() {
                "par2" => 1,
                "log" => 3,
                "rss" => 4,
                _ => 0,
            };
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
        if let Ok(page) = std::env::var("NQ_DEMO_SETUP_PAGE") {
            if let Ok(page) = page.parse::<usize>() {
                app.setup_draft = app.settings.clone();
                app.setup_page = page.min(16);
                app.show_setup = true;
            }
        }
        app
    }
















    pub fn selected_id(&self) -> Option<u64> {
        self.selected_collection().map(|c| c.id)
    }

    pub fn pause_selected(&mut self, pause: bool) {
        if let Some(id) = self.selected_id() {
            self.engine.apply(Command::Pause(id, pause));
        }
    }

    pub fn delete_selected(&mut self) {
        if let Some(id) = self.selected_id() {
            if self.engine.apply(Command::Delete(id)) == Applied::Removed {
                self.selected = None;
            }
        }
    }

    pub fn move_selected(&mut self, dir: MoveDir) {
        let Some(id) = self.selected_id() else { return };
        if let Applied::MovedTo(j) = self.engine.apply(Command::Move(id, dir)) {
            self.selected = Some(Selection { collection: j, file: None });
        }
    }


    pub fn toggle_expand(&mut self, index: usize) {
        if let Some(c) = self.queue.get(index) {
            let (id, v) = (c.id, !c.expanded);
            self.engine.apply(Command::SetExpanded(id, v));
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

    /// Asks GitHub for the latest release. `manual` comes from the Version?
    /// dialog and reports "up to date" too.
    pub fn check_for_update(&mut self, manual: bool) {
        if self.screenshot_path.is_some() || self.update_rx.is_some() {
            return;
        }
        self.update_status = "Checking for a new version...".into();
        if manual {
            self.show_version = true;
        }
        self.update_rx = Some(update::spawn_check());
    }

    /// Downloads, verifies and installs the available release, then exits so
    /// the installer can replace this copy and start the new one.
    pub fn update_now(&mut self) {
        let Some(release) = self.update_available.clone() else {
            return;
        };
        if self.update_rx.is_some() || self.update_installing {
            return;
        }
        if update::install_dir().is_none() {
            self.update_status = format!(
                "{} {} is available. This is a development build; get it from the release page.",
                APP_NAME, release.version
            );
            return;
        }
        self.update_installing = true;
        self.update_status = format!("Downloading {} {}...", APP_NAME, release.version);
        self.log("Update", &format!("Downloading version {}.", release.version));
        self.update_rx = Some(update::spawn_download(release));
    }

    fn poll_update(&mut self) {
        let due = self
            .next_update_check
            .is_some_and(|at| Instant::now() >= at);
        if due {
            self.next_update_check = match self.settings.check_version_minutes {
                0 => None,
                minutes => Some(Instant::now() + Duration::from_secs(u64::from(minutes) * 60)),
            };
            if self.settings.check_version_minutes > 0 {
                self.check_for_update(false);
            }
        }
        // Install by itself only when nothing is being downloaded or unpacked.
        let idle = self.active_dl.is_none() && self.active_par2.is_none() && self.active_unrar.is_none();
        if self.settings.auto_update
            && idle
            && !self.update_installing
            && self.update_rx.is_none()
            && self.update_available.is_some()
            && update::install_dir().is_some()
        {
            self.update_now();
        }
        let Some(rx) = self.update_rx.take() else {
            return;
        };
        let event = match rx.try_recv() {
            Ok(event) => event,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.update_rx = Some(rx);
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                update::UpdateEvent::Failed("The update worker stopped unexpectedly".into())
            }
        };
        match event {
            update::UpdateEvent::UpToDate(latest) => {
                self.update_available = None;
                self.update_status = format!("{APP_NAME} {VERSION} is up to date (latest release: {latest}).");
            }
            update::UpdateEvent::Available(release) => {
                self.update_status = format!("{APP_NAME} {} is available (you have {VERSION}).", release.version);
                if self.update_notified != release.version {
                    self.update_notified = release.version.clone();
                    self.log("Update", &format!("Version {} is available.", release.version));
                    self.toast(format!("NewsQueue {} is available", release.version));
                    tray::notify("Update available", &format!("NewsQueue {} is available", release.version));
                }
                self.update_available = Some(release);
            }
            update::UpdateEvent::Downloaded(release, setup) => {
                let Some(dir) = update::install_dir() else {
                    self.update_installing = false;
                    return;
                };
                match update::launch_installer(&setup, &dir) {
                    Ok(()) => {
                        self.log("Update", &format!("Installing version {}; restarting.", release.version));
                        self.update_status = format!("Installing {} {}...", APP_NAME, release.version);
                        // Same exit path as the shutdown "Exit" action: saves
                        // state on drop and closes the window.
                        self.shutdown_exit_requested = true;
                    }
                    Err(error) => {
                        self.update_installing = false;
                        self.update_status = error.clone();
                        self.log("Error", &error);
                    }
                }
            }
            update::UpdateEvent::Failed(error) => {
                self.update_installing = false;
                self.update_status = error.clone();
                self.log("Error", &error);
            }
        }
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
        self.show_unrar_password = false;
        let password = self.unrar_password_draft.clone();
        self.engine.apply(Command::SubmitPassword(id, password));
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
        self.engine.apply(Command::CancelPassword(id));
        self.show_message = Some((
            "Unrar password".into(),
            format!("{subject}

Unpack cancelled because no password was supplied."),
        ));
    }

    pub fn unpack_selected(&mut self) {
        match self.selected_id() {
            Some(id) => {
                self.engine.apply(Command::Unpack(id));
            }
            None => self.toast("Select a queue item first"),
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
                if let Some(id) = self.selected_id() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        let folder = path.to_string_lossy().to_string();
                        self.engine.apply(Command::SetFolder(id, folder));
                    }
                }
            }
            UiAction::Export => self.export_selected(),
            UiAction::ToggleUnrar(v) => {
                if let Some(id) = self.selected_id() {
                    self.engine.apply(Command::SetUnrar(id, v));
                }
            }
            UiAction::UnpackNow => self.unpack_selected(),
            UiAction::ChangeUnrarFolder => {
                if let Some(id) = self.selected_id() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        let folder = path.to_string_lossy().to_string();
                        self.engine.apply(Command::SetUnrarFolder(id, folder));
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
            UiAction::OpenFolder => self.open_download_folder(),
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
                self.engine.apply(Command::SetAllExpanded(false));
            }
            UiAction::ExpandAll => {
                self.engine.apply(Command::SetAllExpanded(true));
            }
            UiAction::None => {}
        }
    }




    /// Runs the engine and the desktop-only parts (tray, updater, shutdown).
    pub fn tick(&mut self) {
        let now = Instant::now();
        tray::set_options(self.settings.speed_in_tray, self.settings.tray_balloon);
        if !self.engine.threaded() {
            self.engine.tick();
        }
        tray::set_speed(self.engine.speed_bps, self.settings.speed_bytes_mode);
        self.handle_engine_events();
        self.poll_update();
        self.tick_shutdown(now);
        if let Some((_, at)) = &self.toast {
            if now.duration_since(*at) > Duration::from_secs(3) {
                self.toast = None;
            }
        }
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), Instant::now()));
    }

    fn handle_engine_events(&mut self) {
        for event in self.engine.take_events() {
            match event {
                Event::Toast(text) => self.toast(text),
                Event::Message(title, text) => self.show_message = Some((title, text)),
                Event::Notify(title, text) => tray::notify(&title, &text),
                Event::PasswordNeeded(id) => {
                    self.unrar_password_target = Some(id);
                    self.unrar_password_draft.clear();
                    self.show_unrar_password = true;
                }
                Event::RssFeedsNeeded => {
                    self.show_rss_feeds = true;
                    self.rss_feed_draft = String::new();
                }
            }
        }
    }

    fn tick_shutdown(&mut self, now: Instant) {
        if self.shutdown_armed && self.shutdown_countdown.is_none() && self.all_tasks_done() {
            self.shutdown_countdown = Some(now + Duration::from_secs(15));
            self.log("Shutdown", "All tasks finished. Shutdown scheduled.");
        }
        if self.shutdown_countdown.is_some() && !self.all_tasks_done() {
            self.shutdown_countdown = None;
            self.log("Shutdown", "Countdown cancelled: unfinished tasks remain.");
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
                if self.shutdown_mode == shutdown::ShutdownMode::Real {
                    self.engine.save_all();
                }
                match shutdown::execute(action, self.shutdown_mode) {
                    Ok(shutdown::ShutdownOutcome::Executed(detail)) => {
                        self.log("Shutdown", &format!("{}: {detail}", action.label()));
                    }
                    Ok(shutdown::ShutdownOutcome::Mocked(detail)) => {
                        self.log("Shutdown", &format!("{} simulated: {detail}", action.label()));
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
            self.active_tab = match self.active_tab {
                3 => 1,
                tab => tab.saturating_sub(1),
            };
        }
        if f3 {
            self.active_tab = match self.active_tab {
                1 => 3,
                tab => (tab + 1).min(4),
            };
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
            } else if self.show_server_edit {
                self.show_server_edit = false;
            } else if self.show_setup {
                self.show_setup = false;
            }
        }
    }



    /// Toolbar "To tray": the same off-screen hide the minimize path uses, so
    /// downloads keep ticking while the window is away.
    pub fn send_to_tray(&mut self) {
        if self.screenshot_path.is_some() {
            return;
        }
        match tray::main_window_handle() {
            Some(handle) if tray::ready() => {
                tray::hide_main_window(handle);
                self.tray_hidden = true;
                self.log("Main", "Minimized to tray.");
            }
            _ => self.toast("The tray icon is not ready yet"),
        }
    }

    /// Toolbar "Home": the project page in the default browser.
    pub fn open_home_page(&mut self) {
        #[cfg(target_os = "windows")]
        let result = std::process::Command::new("explorer").arg(HOME_URL).spawn();
        #[cfg(not(target_os = "windows"))]
        let result = std::process::Command::new("xdg-open").arg(HOME_URL).spawn();
        if let Err(error) = result {
            self.toast(format!("Cannot open {HOME_URL}: {error}"));
        }
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
            self.screenshot_requested = true;
            let cb = egui::ScreenshotCallback::new(move |img| save_screenshot(&path, &img));
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(cb));
        }
        if self.screenshot_requested && self.frame > 240 {
            eprintln!("Timed out waiting for the screenshot event");
            std::process::exit(1);
        }
    }
}

fn save_screenshot(path: &str, img: &egui::ColorImage) -> ! {
    let (w, h) = (img.width() as u32, img.height() as u32);
    let mut buf = Vec::with_capacity((w * h * 4) as usize);
    for px in &img.pixels {
        buf.extend_from_slice(&px.to_array());
    }
    if let Some(parent) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match image::RgbaImage::from_raw(w, h, buf).map(|i| i.save(path)) {
        Some(Ok(_)) => {
            println!("Screenshot saved to {path}");
            std::process::exit(0);
        }
        Some(Err(e)) => {
            eprintln!("Could not save screenshot: {e}");
            std::process::exit(1);
        }
        None => {
            eprintln!("Could not build screenshot image");
            std::process::exit(1);
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.engine.checkout();
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
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &root.ctx().clone();
        // The engine thread ticks it between frames; this frame uses it directly.
        self.engine.checkout();
        self.frame += 1;
        self.handle_tray(ctx);
        self.tick();
        if self.shutdown_exit_requested && !self.shutdown_exit_sent {
            self.shutdown_exit_sent = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.handle_keys(ctx);
        ui::draw(self, root);
        dialogs::draw(self, ctx);
        self.handle_screenshot(ctx);

        if self.screenshot_path.is_some() {
            ctx.request_repaint();
        } else if self.connected && !self.paused {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        self.engine.checkin();
    }
}


fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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
    if args.iter().any(|a| a == "--update-test") {
        // Checks GitHub for a newer release; never downloads or installs.
        match update::spawn_check().recv() {
            Ok(update::UpdateEvent::UpToDate(latest)) => {
                println!("{APP_NAME} {VERSION} is up to date (latest release {latest})");
                return Ok(());
            }
            Ok(update::UpdateEvent::Available(release)) => {
                println!(
                    "Version {} is available: setup {} / sha256 {}",
                    release.version,
                    if release.setup_url.is_empty() { "missing" } else { "found" },
                    if release.sha256_url.is_empty() { "missing" } else { "found" }
                );
                return Ok(());
            }
            Ok(other) => {
                eprintln!("update test failed: {other:?}");
                std::process::exit(1);
            }
            Err(error) => {
                eprintln!("update test failed: {error}");
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
            theme::install(&cc.egui_ctx, screenshot.is_some());
            Ok(Box::new(App::new(settings, screenshot)))
        }),
    )
}

#[cfg(test)]
mod integration_tests {
    use super::*;

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
            app.handle_engine_events();
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
