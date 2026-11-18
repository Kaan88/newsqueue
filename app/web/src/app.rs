//! The web app: a classic desktop window in the browser. Menu bar, toolbar,
//! tabbed list views (queue, history, PAR2, log, RSS), a status bar and
//! modal dialogs, over the JSON API with live updates from `/api/events`.

use crate::net::{self, Client, Events, QueueRow, Snapshot};
use crate::settings::SettingsPage;
use crate::skin::{self, Columns, Edge};
use crate::theme::{self, BODY, FACE, GRAY_TEXT, Icon, SEL, SEL_TEXT, TEXT, WINDOW};
use egui::{Align2, Color32, Key, Modifiers, Rect, Sense, Stroke, Ui, pos2, vec2};
use std::sync::Arc;

const POLL_SECONDS: f64 = 1.0;
/// Below this width the page uses its phone layout.
const NARROW: f32 = 720.0;
const TABS: [(&str, Icon); 5] = [("Download queue", Icon::Queue), ("History", Icon::History), ("PAR2", Icon::Par2), ("Log", Icon::Log), ("RSS", Icon::Rss)];
const TABS_NARROW: [(&str, Icon); 5] = [("Queue", Icon::Queue), ("History", Icon::History), ("PAR2", Icon::Par2), ("Log", Icon::Log), ("RSS", Icon::Rss)];

enum Dialog {
    AddNzb,
    Delete(u64),
    Rename(u64, String),
    Password(u64, String),
    Speed(Option<u8>),
    PauseFor(f64),
    Stats(usize),
    About,
    Options,
}

pub struct WebApp {
    client: Client,
    key_draft: String,
    tab: usize,
    selected: Option<u64>,
    hist_sel: Option<usize>,
    par2_sel: Option<usize>,
    last_poll: f64,
    events: Option<Events>,
    last_events_try: f64,
    dialog: Option<Dialog>,
    add_url: String,
    add_paused: bool,
    dark: bool,
    show_conns: bool,
    /// The collection being dragged and the row it would land on.
    drag: Option<(u64, usize)>,
    /// Shown in the status bar until this time.
    toast: Option<(String, f64)>,
    settings: SettingsPage,
    queue_filter: String,
    log_filter: String,
    cols_queue: Columns,
    cols_queue_narrow: Columns,
    cols_conns: Columns,
    cols_hist: Columns,
    cols_par2: Columns,
    cols_log: Columns,
    cols_rss: Columns,
}

pub fn size(bytes: f64) -> String {
    let b = bytes.max(0.0);
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= 1024.0 {
        format!("{:.1} KB", b / 1024.0)
    } else {
        format!("{b:.0} B")
    }
}

fn duration(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    if s >= 86400 {
        format!("{}d {}h", s / 86400, s % 86400 / 3600)
    } else if s >= 3600 {
        format!("{}h {:02}m", s / 3600, s % 3600 / 60)
    } else {
        format!("{}m {:02}s", s / 60, s % 60)
    }
}

fn status_text(r: &QueueRow) -> String {
    let s = r.status.as_deref().unwrap_or("Queued");
    let label = match s {
        "Completed" => "Completed",
        "Incomplete" => "Incomplete",
        other => other,
    };
    if matches!(s, "Downloading" | "Paused" | "Queued") && r.progress > 0.0 {
        format!("{label} {:.0}%", r.progress * 100.0)
    } else {
        label.to_string()
    }
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

pub fn stored(name: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    return storage().and_then(|s| s.get_item(name).ok().flatten());
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = name;
        None
    }
}

pub fn store(name: &str, value: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(s) = storage() {
        let _ = s.set_item(name, value);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (name, value);
}

/// Whether the browser asks for dark colours (`prefers-color-scheme`).
fn system_dark() -> bool {
    #[cfg(target_arch = "wasm32")]
    return web_sys::window().and_then(|w| w.match_media("(prefers-color-scheme: dark)").ok().flatten()).is_some_and(|m| m.matches());
    #[cfg(not(target_arch = "wasm32"))]
    false
}

/// The API key: `?apikey=` in the address, else the one saved in this browser.
fn initial_key() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let from_url = web_sys::window().and_then(|w| w.location().search().ok()).and_then(|q| {
            q.trim_start_matches('?').split('&').find_map(|p| p.strip_prefix("apikey=").map(str::to_string))
        });
        if let Some(k) = from_url {
            return k;
        }
    }
    stored("nq-key").unwrap_or_default()
}

impl WebApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // View > Dark colors wins; until it is used, follow the system.
        let dark = match stored("nq-theme").as_deref() {
            Some("dark") => true,
            Some("light") => false,
            _ => system_dark(),
        };
        theme::install(&cc.egui_ctx, dark);
        let key = initial_key();
        Self {
            client: Client { shared: Arc::default(), key: key.clone() },
            key_draft: key,
            tab: 0,
            selected: None,
            hist_sel: None,
            par2_sel: None,
            last_poll: -10.0,
            events: None,
            last_events_try: -10.0,
            dialog: None,
            add_url: String::new(),
            add_paused: false,
            dark,
            show_conns: stored("nq-conns").as_deref() != Some("0"),
            drag: None,
            toast: None,
            settings: SettingsPage::default(),
            queue_filter: String::new(),
            log_filter: String::new(),
            cols_queue: Columns::new(
                "queue",
                &[("Subject", 330.0, false), ("Size", 82.0, true), ("Left", 82.0, true), ("Parts", 52.0, true), ("Age", 60.0, true), ("Status", 160.0, false), ("Group", 150.0, false), ("Poster", 110.0, false)],
            ),
            cols_queue_narrow: Columns::new("queue-narrow", &[("Subject", 200.0, false), ("Status", 120.0, false)]),
            cols_conns: Columns::new("conns", &[("#", 32.0, true), ("Action", 90.0, false), ("Description", 380.0, false), ("Status", 120.0, false), ("Server", 180.0, false)]),
            cols_hist: Columns::new("history", &[("Name", 380.0, false), ("Status", 110.0, false), ("Size", 84.0, true), ("Finished", 140.0, false)]),
            cols_par2: Columns::new("par2", &[("Name", 300.0, false), ("Size", 84.0, true), ("Blocks", 60.0, true), ("Info", 220.0, false), ("Unrar", 50.0, false), ("Status", 160.0, false)]),
            cols_log: Columns::new("log", &[("Time", 70.0, false), ("Area", 90.0, false), ("Message", 600.0, false)]),
            cols_rss: Columns::new("rss", &[("Name", 380.0, false), ("Date", 130.0, false), ("Description", 270.0, false)]),
        }
    }

    fn post(&mut self, ctx: &egui::Context, path: &str) {
        self.client.post(ctx, path, vec![], None);
        self.poll_soon(ctx);
    }

    fn poll_soon(&mut self, ctx: &egui::Context) {
        // Without the event stream, show the result soon rather than after a full interval.
        self.last_poll = ctx.input(|i| i.time) - POLL_SECONDS + 0.25;
    }

    fn act(&mut self, ctx: &egui::Context, id: u64, action: &str) {
        self.post(ctx, &format!("/api/queue/{id}/{action}"));
    }

    fn set_dark(&mut self, ctx: &egui::Context, dark: bool) {
        self.dark = dark;
        store("nq-theme", if dark { "dark" } else { "light" });
        theme::set_dark(ctx, dark);
    }

    fn selected_row<'a>(&self, snap: &'a Snapshot) -> Option<&'a QueueRow> {
        self.selected.and_then(|id| snap.queue.iter().find(|r| r.id == id))
    }

    fn open_options(&mut self, page: usize) {
        self.settings.page = page;
        self.settings.reset();
        self.dialog = Some(Dialog::Options);
    }
}

impl eframe::App for WebApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = ctx.input(|i| i.time);
        let (live, unauthorized, have_snap) = {
            let s = net::lock(&self.client.shared);
            (s.live, s.unauthorized, s.snap.is_some())
        };
        // Live: the event stream pushes every change. Otherwise poll, and
        // (re)open the stream once the server answers.
        if !live && now - self.last_poll >= POLL_SECONDS {
            self.last_poll = now;
            self.client.poll(ctx);
        }
        let stream_down = self.events.as_ref().is_none_or(|e| e.closed());
        if !unauthorized && have_snap && stream_down && now - self.last_events_try > 5.0 {
            self.last_events_try = now;
            if let Some(old) = self.events.take() {
                old.close();
            }
            self.events = Events::open(&self.client, ctx);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(if live { 1000 } else { 500 }));
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            self.client.upload_dropped(ctx, f, self.add_paused);
        }
    }

    fn ui(&mut self, root: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let now = ctx.input(|i| i.time);
        let (snap, error, unauthorized, toasts) = {
            let mut s = net::lock(&self.client.shared);
            (s.snap.clone(), s.error.clone(), s.unauthorized, std::mem::take(&mut s.toasts))
        };
        if let Some(t) = toasts.into_iter().last() {
            self.toast = Some((t, now + 6.0));
        }
        if self.toast.as_ref().is_some_and(|t| t.1 < now) {
            self.toast = None;
        }
        if unauthorized {
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(FACE())).show(root, |_| {});
            self.login(&ctx);
            return;
        }
        let snap = snap.unwrap_or_default();
        let narrow = root.available_width() < NARROW;
        if self.dialog.is_none() {
            self.shortcuts(&ctx, &snap);
        }

        egui::Panel::top("menubar").exact_size(21.0).frame(egui::Frame::NONE.fill(FACE())).show(root, |ui| self.menu_bar(ui, &snap));
        egui::Panel::top("toolbar")
            .exact_size(if narrow { 38.0 } else { 54.0 })
            .frame(egui::Frame::NONE.fill(FACE()))
            .show(root, |ui| self.toolbar(ui, &snap, narrow));
        egui::Panel::bottom("status").exact_size(24.0).frame(egui::Frame::NONE.fill(FACE())).show(root, |ui| {
            let message = self.toast.as_ref().map(|t| (t.0.clone(), false)).or(error.clone().map(|e| (e, true)));
            self.status_bar(ui, &snap, message, error.is_some(), narrow)
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(FACE()).inner_margin(egui::Margin { left: 4, right: 4, top: 4, bottom: 2 }))
            .show(root, |ui| {
                let tabs: &[(&str, Icon)] = if narrow { &TABS_NARROW } else { &TABS };
                let mut tab = self.tab;
                egui::ScrollArea::horizontal()
                    .id_salt("tabs")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .max_height(23.0)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        skin::tab_strip(ui, &mut tab, tabs);
                    });
                self.tab = tab;
                // The page: a raised panel joined to the selected tab.
                let page = ui.available_rect_before_wrap();
                let p = ui.painter();
                let t = 1.0;
                p.rect_filled(Rect::from_min_max(pos2(page.left(), page.top()), pos2(page.left() + t, page.bottom())), 0.0, theme::HILIGHT());
                p.rect_filled(Rect::from_min_max(pos2(page.left(), page.bottom() - t), page.max), 0.0, theme::DKSHADOW());
                p.rect_filled(Rect::from_min_max(pos2(page.right() - t, page.top()), page.max), 0.0, theme::DKSHADOW());
                p.rect_filled(Rect::from_min_max(pos2(page.left() + t, page.bottom() - 2.0 * t), pos2(page.right() - t, page.bottom() - t)), 0.0, theme::SHADOW());
                p.rect_filled(Rect::from_min_max(pos2(page.right() - 2.0 * t, page.top() + t), pos2(page.right() - t, page.bottom() - t)), 0.0, theme::SHADOW());
                let inner = page.shrink2(vec2(6.0, 6.0));
                let mut page_ui = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
                match self.tab {
                    0 => self.queue_page(&mut page_ui, &snap, narrow),
                    1 => self.history_page(&mut page_ui, &snap, narrow),
                    2 => self.par2_page(&mut page_ui, &snap, narrow),
                    3 => self.log_page(&mut page_ui, &snap, narrow),
                    _ => self.rss_page(&mut page_ui, &snap, narrow),
                }
                ui.allocate_rect(page, Sense::hover());
            });
        self.drop_hint(&ctx);
        self.dialogs(&ctx, &snap, narrow);
    }
}

// ------------------------------------------------------- menus and keys ---

impl WebApp {
    fn shortcuts(&mut self, ctx: &egui::Context, snap: &Snapshot) {
        if ctx.memory(|m| m.focused().is_some()) && ctx.egui_wants_keyboard_input() {
            return;
        }
        let pressed = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        if pressed(Modifiers::NONE, Key::F1) {
            self.post(ctx, if snap.paused { "/api/resume" } else { "/api/pause" });
        }
        if pressed(Modifiers::COMMAND, Key::O) {
            self.dialog = Some(Dialog::AddNzb);
        }
        if self.tab != 0 {
            return;
        }
        // Queue keys: arrows select, Ctrl+arrows move, Del deletes, F2 renames.
        let ids: Vec<u64> = snap.queue.iter().filter(|r| self.matches_filter(r)).map(|r| r.id).collect();
        let pos = self.selected.and_then(|id| ids.iter().position(|i| *i == id));
        if let Some(id) = self.selected.filter(|_| pos.is_some()) {
            if pressed(Modifiers::COMMAND, Key::ArrowUp) {
                self.act(ctx, id, "up");
            }
            if pressed(Modifiers::COMMAND, Key::ArrowDown) {
                self.act(ctx, id, "down");
            }
            if pressed(Modifiers::NONE, Key::Delete) {
                self.dialog = Some(Dialog::Delete(id));
            }
            if pressed(Modifiers::NONE, Key::F2) {
                let name = snap.queue.iter().find(|r| r.id == id).map(|r| r.subject.clone()).unwrap_or_default();
                self.dialog = Some(Dialog::Rename(id, name));
            }
            if pressed(Modifiers::NONE, Key::Enter) {
                let expanded = snap.queue.iter().find(|r| r.id == id).is_some_and(|r| r.expanded);
                self.act(ctx, id, if expanded { "collapse" } else { "expand" });
            }
        }
        if pressed(Modifiers::NONE, Key::ArrowDown) && !ids.is_empty() {
            self.selected = Some(ids[pos.map_or(0, |p| (p + 1).min(ids.len() - 1))]);
        }
        if pressed(Modifiers::NONE, Key::ArrowUp) && !ids.is_empty() {
            self.selected = Some(ids[pos.map_or(0, |p| p.saturating_sub(1))]);
        }
        if pressed(Modifiers::NONE, Key::Home) && !ids.is_empty() {
            self.selected = Some(ids[0]);
        }
        if pressed(Modifiers::NONE, Key::End) && !ids.is_empty() {
            self.selected = Some(ids[ids.len() - 1]);
        }
    }

    fn menu_bar(&mut self, ui: &mut Ui, snap: &Snapshot) {
        let ctx = ui.ctx().clone();
        ui.add_space(1.0);
        let sel = self.selected_row(snap).cloned();
        let has_sel = sel.is_some();
        skin::menu_bar(ui, &["File", "Queue", "View", "Tools", "Help"], |ui, menu| {
            let mut close = false;
            match menu {
                0 => {
                    if skin::menu_item(ui, "Add NZB files...", "Ctrl+O", true) {
                        self.dialog = Some(Dialog::AddNzb);
                        close = true;
                    }
                    skin::menu_separator(ui);
                    if skin::menu_item(ui, "Sign out", "", !self.client.key.is_empty()) {
                        self.client.key.clear();
                        store("nq-key", "");
                        net::lock(&self.client.shared).unauthorized = true;
                        close = true;
                    }
                }
                1 => {
                    if skin::menu_check(ui, "Connected", snap.connected, false) {
                        self.post(&ctx, if snap.connected { "/api/disconnect" } else { "/api/connect" });
                        close = true;
                    }
                    if skin::menu_check_key(ui, "Pause downloading", "F1", snap.paused) {
                        self.post(&ctx, if snap.paused { "/api/resume" } else { "/api/pause" });
                        close = true;
                    }
                    if skin::menu_item(ui, "Pause for...", "", true) {
                        self.dialog = Some(Dialog::PauseFor(10.0));
                        close = true;
                    }
                    if skin::menu_item(ui, "Speed limit...", "", true) {
                        self.dialog = Some(Dialog::Speed(snap.speed_limit_tag));
                        close = true;
                    }
                    skin::menu_separator(ui);
                    if skin::menu_item(ui, "Resume all", "", !snap.queue.is_empty()) {
                        self.post(&ctx, "/api/resume-all");
                        close = true;
                    }
                    if skin::menu_item(ui, "Pause all", "", !snap.queue.is_empty()) {
                        self.post(&ctx, "/api/pause-all");
                        close = true;
                    }
                    skin::menu_separator(ui);
                    let paused = sel.as_ref().is_some_and(|c| c.status.as_deref() == Some("Paused"));
                    if skin::menu_item(ui, if paused { "Resume selected" } else { "Pause selected" }, "", has_sel) {
                        if let Some(c) = &sel {
                            self.act(&ctx, c.id, if paused { "resume" } else { "pause" });
                        }
                        close = true;
                    }
                    for (label, key, action) in [("Move to top", "", "top"), ("Move up", "Ctrl+Up", "up"), ("Move down", "Ctrl+Down", "down"), ("Move to bottom", "", "bottom")] {
                        if skin::menu_item(ui, label, key, has_sel) {
                            if let Some(c) = &sel {
                                self.act(&ctx, c.id, action);
                            }
                            close = true;
                        }
                    }
                    skin::menu_separator(ui);
                    if skin::menu_item(ui, "Rename...", "F2", has_sel) {
                        if let Some(c) = &sel {
                            self.dialog = Some(Dialog::Rename(c.id, c.subject.clone()));
                        }
                        close = true;
                    }
                    if skin::menu_item(ui, "Archive password...", "", has_sel) {
                        if let Some(c) = &sel {
                            self.dialog = Some(Dialog::Password(c.id, String::new()));
                        }
                        close = true;
                    }
                    if skin::menu_item(ui, "Unpack now", "", has_sel) {
                        if let Some(c) = &sel {
                            self.act(&ctx, c.id, "unpack");
                        }
                        close = true;
                    }
                    skin::menu_separator(ui);
                    if skin::menu_item(ui, "Delete", "Del", has_sel) {
                        if let Some(c) = &sel {
                            self.dialog = Some(Dialog::Delete(c.id));
                        }
                        close = true;
                    }
                    if skin::menu_item(ui, "Clean up finished", "", true) {
                        self.post(&ctx, "/api/cleanup");
                        close = true;
                    }
                }
                2 => {
                    for (i, (name, _)) in TABS.iter().enumerate() {
                        if skin::menu_check(ui, name, self.tab == i, true) {
                            self.tab = i;
                            close = true;
                        }
                    }
                    skin::menu_separator(ui);
                    if skin::menu_check(ui, "Connections panel", self.show_conns, false) {
                        self.show_conns = !self.show_conns;
                        store("nq-conns", if self.show_conns { "1" } else { "0" });
                        close = true;
                    }
                    if skin::menu_item(ui, "Expand all", "", !snap.queue.is_empty()) {
                        self.post(&ctx, "/api/expand?on=1");
                        close = true;
                    }
                    if skin::menu_item(ui, "Collapse all", "", !snap.queue.is_empty()) {
                        self.post(&ctx, "/api/expand?on=0");
                        close = true;
                    }
                    skin::menu_separator(ui);
                    if skin::menu_item(ui, "Statistics...", "", true) {
                        self.dialog = Some(Dialog::Stats(0));
                        close = true;
                    }
                    skin::menu_separator(ui);
                    if skin::menu_check(ui, "Dark colors", self.dark, false) {
                        self.set_dark(&ctx, !self.dark);
                        close = true;
                    }
                }
                3 => {
                    if skin::menu_item(ui, "Options...", "", true) {
                        self.open_options(0);
                        close = true;
                    }
                    if skin::menu_item(ui, "RSS feeds...", "", true) {
                        self.open_options(crate::settings::RSS_PAGE);
                        close = true;
                    }
                    skin::menu_separator(ui);
                    if skin::menu_item(ui, "Refresh RSS now", "", true) {
                        self.client.post(&ctx, "/api/rss/refresh", vec![], Some("RSS refresh started".into()));
                        close = true;
                    }
                }
                _ => {
                    if skin::menu_item(ui, "About NewsQueue", "", true) {
                        self.dialog = Some(Dialog::About);
                        close = true;
                    }
                }
            }
            close
        });
    }

    fn toolbar(&mut self, ui: &mut Ui, snap: &Snapshot, narrow: bool) {
        let ctx = ui.ctx().clone();
        let full = ui.max_rect();
        skin::hline(ui.painter(), full.left(), full.right(), full.top());
        skin::hline(ui.painter(), full.left(), full.right(), full.bottom() - 2.0);
        let h = if narrow { 30.0 } else { 46.0 };
        let sel = self.selected_row(snap).cloned();
        egui::ScrollArea::horizontal()
            .id_salt("toolbar")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 1.0;
                    ui.add_space(2.0);
                    skin::gripper(ui, h);
                    let t = |s: &'static str| if narrow { "" } else { s };
                    if skin::tool_button(ui, t("Add NZB"), Icon::Nzb, false, narrow).on_hover_text("Add NZB files (Ctrl+O)").clicked() {
                        self.dialog = Some(Dialog::AddNzb);
                    }
                    skin::tool_separator(ui, h);
                    let r = skin::tool_button(ui, t("Connect"), Icon::Connect, snap.connected, narrow);
                    if r.on_hover_text(if snap.connected { "Disconnect from the servers" } else { "Connect to the servers" }).clicked() {
                        self.post(&ctx, if snap.connected { "/api/disconnect" } else { "/api/connect" });
                    }
                    let r = skin::tool_button(ui, t("Pause"), Icon::Pause, snap.paused, narrow);
                    if r.on_hover_text(if snap.paused { "Resume downloading (F1)" } else { "Pause downloading (F1)" }).clicked() {
                        self.post(&ctx, if snap.paused { "/api/resume" } else { "/api/pause" });
                    }
                    if !narrow {
                        if skin::tool_button(ui, "Pause for", Icon::PauseFor, false, false).on_hover_text("Pause for a number of minutes").clicked() {
                            self.dialog = Some(Dialog::PauseFor(10.0));
                        }
                        if skin::tool_button(ui, "Speed", Icon::Speed, snap.speed_limit_tag.is_some(), false).on_hover_text("Speed limit").clicked() {
                            self.dialog = Some(Dialog::Speed(snap.speed_limit_tag));
                        }
                    }
                    skin::tool_separator(ui, h);
                    let movers: &[(&str, Icon, &str, &str)] = if narrow {
                        &[("Up", Icon::Up, "up", "Move up"), ("Down", Icon::Down, "down", "Move down")]
                    } else {
                        &[("Top", Icon::Top, "top", "Move to top"), ("Up", Icon::Up, "up", "Move up (Ctrl+Up)"), ("Down", Icon::Down, "down", "Move down (Ctrl+Down)"), ("Bottom", Icon::Bottom, "bottom", "Move to bottom")]
                    };
                    ui.add_enabled_ui(sel.is_some(), |ui| {
                        for (label, icon, action, tip) in movers {
                            if skin::tool_button(ui, t(label), *icon, false, narrow).on_hover_text(*tip).on_disabled_hover_text("Select a job first").clicked() {
                                if let Some(c) = &sel {
                                    self.act(&ctx, c.id, action);
                                }
                            }
                        }
                        skin::tool_separator(ui, h);
                        if !narrow && skin::tool_button(ui, "Unpack", Icon::Unpack, false, false).on_hover_text("Unpack now").on_disabled_hover_text("Select a job first").clicked() {
                            if let Some(c) = &sel {
                                self.act(&ctx, c.id, "unpack");
                            }
                        }
                        if skin::tool_button(ui, t("Delete"), Icon::Delete, false, narrow).on_hover_text("Delete the job (Del)").on_disabled_hover_text("Select a job first").clicked() {
                            if let Some(c) = &sel {
                                self.dialog = Some(Dialog::Delete(c.id));
                            }
                        }
                    });
                    if !narrow {
                        skin::tool_separator(ui, h);
                        if skin::tool_button(ui, "Clean up", Icon::Cleanup, false, false).on_hover_text("Remove finished jobs").clicked() {
                            self.post(&ctx, "/api/cleanup");
                        }
                        if skin::tool_button(ui, "Stats", Icon::Stats, false, false).on_hover_text("Statistics").clicked() {
                            self.dialog = Some(Dialog::Stats(0));
                        }
                    }
                    skin::tool_separator(ui, h);
                    if skin::tool_button(ui, t("Options"), Icon::Setup, false, narrow).on_hover_text("Options").clicked() {
                        self.open_options(0);
                    }
                    ui.add_space(4.0);
                });
            });
    }

    fn status_bar(&mut self, ui: &mut Ui, snap: &Snapshot, message: Option<(String, bool)>, offline: bool, narrow: bool) {
        let full = ui.max_rect();
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_space(2.0);
            let (state, dot) = if offline {
                ("Offline", theme::RED())
            } else if snap.connected {
                ("Connected", Color32::from_rgb(0x30, 0xC0, 0x30))
            } else {
                ("Not connected", theme::GRAY_TEXT())
            };
            skin::status_pane(ui, if narrow { 108.0 } else { 118.0 }, state, TEXT(), Some(dot));
            let speed = if snap.paused { "Paused".to_string() } else { format!("{}/s", size(snap.speed_bps)) };
            if skin::status_pane(ui, 90.0, &speed, TEXT(), None).on_hover_text("Speed limit...").clicked() {
                self.dialog = Some(Dialog::Speed(snap.speed_limit_tag));
            }
            if !narrow {
                let limit = match snap.speed_limit_tag {
                    Some(t) => format!("Limit {} %", t as u32 * 10),
                    None => "No limit".into(),
                };
                if skin::status_pane(ui, 80.0, &limit, TEXT(), None).on_hover_text("Speed limit...").clicked() {
                    self.dialog = Some(Dialog::Speed(snap.speed_limit_tag));
                }
                skin::status_pane(ui, 100.0, &format!("{} left", size(snap.queue_remaining_bytes)), TEXT(), None);
                let eta = if snap.speed_bps > 1.0 && snap.queue_remaining_bytes > 0.0 && !snap.paused {
                    format!("ETA {}", duration(snap.queue_remaining_bytes / snap.speed_bps))
                } else {
                    "ETA -".into()
                };
                skin::status_pane(ui, 92.0, &eta, TEXT(), None);
                skin::status_pane(ui, 110.0, &format!("Free: {:.1} GB", snap.free_gb), TEXT(), None);
            }
            let (text, color) = match &message {
                Some((m, true)) => (m.clone(), theme::RED()),
                Some((m, false)) => (m.clone(), TEXT()),
                None if snap.scheduler_hold => ("Waiting for the scheduler".into(), TEXT()),
                None => (String::new(), TEXT()),
            };
            skin::status_pane(ui, 0.0, &text, color, None);
        });
        skin::size_grip(ui.painter(), full.right_bottom() - vec2(1.0, 1.0));
    }

    /// While files are dragged over the page: where to drop them.
    fn drop_hint(&self, ctx: &egui::Context) {
        if ctx.input(|i| i.raw.hovered_files.is_empty()) {
            return;
        }
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("drop")));
        let r = ctx.content_rect().shrink(24.0);
        skin::focus_rect(&p, r);
        let b = Rect::from_center_size(r.center(), vec2(300.0, 60.0));
        p.rect_filled(b, 0.0, theme::INFO());
        p.rect_stroke(b, 0.0, Stroke::new(1.0, TEXT()), egui::StrokeKind::Inside);
        theme::paint_icon(&p, Rect::from_center_size(pos2(b.left() + 30.0, b.center().y), vec2(32.0, 32.0)), Icon::Nzb, true);
        p.text(pos2(b.left() + 56.0, b.center().y - 8.0), Align2::LEFT_CENTER, "Drop .nzb files to add them", theme::bold(BODY), TEXT());
        p.text(pos2(b.left() + 56.0, b.center().y + 9.0), Align2::LEFT_CENTER, if self.add_paused { "They are added paused." } else { "They start downloading in turn." }, theme::font(theme::SMALL), TEXT());
    }
}

// ------------------------------------------------------------ list view ---

/// A list view in a sunken box filling `height` (0: the rest): a resizable
/// header and `n` rows painted by `row(ui, index, rect, lefts)`.
fn list_view(ui: &mut Ui, cols: &mut Columns, id: &str, height: f32, n: usize, row_h: f32, empty: &str, stick_bottom: bool, mut row: impl FnMut(&mut Ui, usize, Rect, &[f32], &Columns)) {
    let h = if height <= 0.0 { ui.available_height() } else { height };
    let (rect, bg) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    ui.painter().rect_filled(rect, 0.0, WINDOW());
    skin::edge(ui.painter(), rect, Edge::Sunken);
    let inner = rect.shrink(2.0);
    let body = Rect::from_min_max(pos2(inner.left(), inner.top() + skin::HEADER_H), inner.max);
    let total = cols.total();
    let out = skin::scroll_view(ui, body, id, [true, true], stick_bottom, row_h, |area, ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        area.show_rows(ui, row_h, n, |ui, range| {
            let w = total.max(ui.available_width());
            for i in range {
                let (r, _) = ui.allocate_exact_size(vec2(w, row_h), Sense::hover());
                let lefts = cols.lefts(r.left());
                row(ui, i, r, &lefts, cols);
            }
        })
    });
    if n == 0 {
        ui.painter().with_clip_rect(body).text(pos2(body.center().x, body.top() + 22.0), Align2::CENTER_CENTER, empty, theme::font(BODY), GRAY_TEXT());
    }
    let _ = bg;
    skin::list_header(ui, cols, inner, out.state.clamped_offset().x);
}

/// On a phone the first column fills the visible width (the others are a
/// horizontal scroll away). Returns the width to keep for the desktop.
fn fit_first_column(ui: &Ui, cols: &mut Columns, narrow: bool) -> f32 {
    let keep = cols.widths[0];
    if narrow {
        cols.widths[0] = (ui.available_width() - 6.0).max(120.0);
    }
    keep
}

/// Text in column `c` of a row, clipped to the cell.
fn cell(p: &egui::Painter, r: Rect, lefts: &[f32], cols: &Columns, c: usize, text: &str, color: Color32) {
    let cell = Rect::from_min_size(pos2(lefts[c], r.top()), vec2(cols.widths[c], r.height()));
    let clip = cell.shrink2(vec2(6.0, 0.0)).intersect(p.clip_rect());
    let (pos, align) = if cols.right[c] { (pos2(clip.right(), r.center().y), Align2::RIGHT_CENTER) } else { (pos2(cell.left() + 6.0, r.center().y), Align2::LEFT_CENTER) };
    let text = if cols.right[c] { text.to_string() } else { elide(p, text, theme::font(BODY), clip.right() - pos.x) };
    p.with_clip_rect(clip).text(pos, align, text, theme::font(BODY), color);
}

/// `text` shortened with "..." to fit `width` points, as list views do.
pub fn elide(p: &egui::Painter, text: &str, font: egui::FontId, width: f32) -> String {
    let fits = |t: &str| p.layout_no_wrap(t.to_string(), font.clone(), Color32::WHITE).size().x <= width;
    if fits(text) {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        let t: String = chars[..mid].iter().collect::<String>() + "...";
        if fits(&t) { lo = mid } else { hi = mid - 1 }
    }
    chars[..lo].iter().collect::<String>().trim_end().to_string() + "..."
}

fn row_ground(ui: &Ui, r: Rect, selected: bool) -> Color32 {
    if selected {
        ui.painter().rect_filled(r, 0.0, SEL());
        SEL_TEXT()
    } else {
        TEXT()
    }
}

/// The classic tree box: [+] or [-].
fn expander(p: &egui::Painter, center: egui::Pos2, open: bool) {
    let r = skin::snap(p.ctx(), Rect::from_center_size(center, vec2(9.0, 9.0)));
    p.rect_filled(r, 0.0, WINDOW());
    p.rect_stroke(r, 0.0, Stroke::new(1.0, theme::SHADOW()), egui::StrokeKind::Inside);
    let c = r.center();
    p.line_segment([pos2(r.left() + 2.0, c.y), pos2(r.right() - 2.0, c.y)], Stroke::new(1.0, TEXT()));
    if !open {
        p.line_segment([pos2(c.x, r.top() + 2.0), pos2(c.x, r.bottom() - 2.0)], Stroke::new(1.0, TEXT()));
    }
}

/// A row of buttons and fields above a list.
fn button_row(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 25.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        add(ui);
    });
    ui.add_space(5.0);
}

// ---------------------------------------------------------------- pages ---

impl WebApp {
    fn matches_filter(&self, r: &QueueRow) -> bool {
        let f = self.queue_filter.trim().to_lowercase();
        f.is_empty() || r.subject.to_lowercase().contains(&f)
    }

    fn queue_page(&mut self, ui: &mut Ui, snap: &Snapshot, narrow: bool) {
        let ctx = ui.ctx().clone();
        button_row(ui, |ui| {
            let n = snap.queue.len();
            let total: u64 = snap.queue.iter().map(|r| r.total_bytes).sum();
            let field_w = if narrow { 120.0 } else { 190.0 };
            // Shortened, never drawn over the filter on a very narrow page.
            let room = ui.available_width() - field_w - 56.0;
            let summary = format!("{n} job{}, {}", if n == 1 { "" } else { "s" }, size(total as f64));
            if room >= 40.0 {
                skin::label(ui, &elide(ui.painter(), &summary, theme::font(BODY), room));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                skin::field(ui, field_w, 1, egui::TextEdit::singleline(&mut self.queue_filter).hint_text("Filter subjects"));
                skin::label(ui, "Filter:");
            });
        });
        // Collections that match the filter, and the files of expanded ones.
        let shown: Vec<usize> = (0..snap.queue.len()).filter(|i| self.matches_filter(&snap.queue[*i])).collect();
        let mut rows: Vec<(usize, Option<usize>)> = Vec::new();
        for &ci in &shown {
            rows.push((ci, None));
            if snap.queue[ci].expanded {
                rows.extend((0..snap.queue[ci].file_rows.len()).map(|fi| (ci, Some(fi))));
            }
        }
        let conns_h = if self.show_conns && !narrow { (ui.available_height() * 0.3).clamp(90.0, 160.0) } else { 0.0 };
        let row_h = if narrow { 24.0 } else { 18.0 };
        let pointer = ctx.input(|i| i.pointer.interact_pos());
        let released = ctx.input(|i| i.pointer.any_released());
        let mut drop_line: Option<(f32, Rect)> = None;
        let mut new_target: Option<usize> = None;
        let mut actions: Vec<(u64, &'static str)> = Vec::new();
        let mut open_dialog: Option<Dialog> = None;
        let mut select: Option<u64> = None;
        let mut start_drag: Option<(u64, usize)> = None;
        let drag = self.drag;
        let selected = self.selected;
        let mut cols = if narrow { self.cols_queue_narrow.clone() } else { self.cols_queue.clone() };
        if narrow {
            // On a phone the subject takes whatever the status leaves.
            cols.widths[0] = (ui.available_width() - cols.widths[1] - 22.0).max(120.0);
        }
        let empty = if snap.queue.is_empty() { "The queue is empty. Drop .nzb files here or use File > Add NZB files." } else { "No jobs match the filter." };
        list_view(ui, &mut cols, "queue", -conns_h - if conns_h > 0.0 { 6.0 } else { 0.0 } + ui.available_height(), rows.len(), row_h, empty, false, |ui, idx, r, lefts, cols| {
            let (ci, file) = rows[idx];
            let c = &snap.queue[ci];
            let resp = ui.interact(r, ui.scope_id().with(("qrow", c.id, file)), Sense::click_and_drag());
            if file.is_none() && drag.is_some() && pointer.is_some_and(|p| p.y >= r.top() && p.y < r.bottom()) {
                new_target = Some(ci);
            }
            let p = ui.painter().clone();
            let sel = file.is_none() && (selected == Some(c.id) || drag.is_some_and(|d| d.0 == c.id));
            let fg = row_ground(ui, r, sel);
            let done = c.status.as_deref() == Some("Completed");
            let dim = if sel { fg } else if done { GRAY_TEXT() } else { fg };
            if let Some(fi) = file {
                let f = &c.file_rows[fi];
                let fdone = f.done || f.progress >= 1.0;
                let color = if f.held || fdone || done { GRAY_TEXT() } else { TEXT() };
                let name_cell = Rect::from_min_size(pos2(lefts[0], r.top()), vec2(cols.widths[0], r.height()));
                theme::paint_icon(&p, Rect::from_min_size(pos2(name_cell.left() + 24.0, r.center().y - 8.0), vec2(16.0, 16.0)), Icon::File, true);
                p.with_clip_rect(name_cell.shrink2(vec2(4.0, 0.0)).intersect(p.clip_rect())).text(pos2(name_cell.left() + 44.0, r.center().y), Align2::LEFT_CENTER, elide(&p, &f.name, theme::font(BODY), name_cell.width() - 48.0), theme::font(BODY), color);
                let status = if f.held { "Held (PAR2)".to_string() } else if fdone { "Completed".into() } else if f.progress > 0.0 { format!("{:.0}%", f.progress * 100.0) } else { String::new() };
                for (k, title) in cols.titles.clone().iter().enumerate().skip(1) {
                    match *title {
                        "Size" => cell(&p, r, lefts, &cols, k, &size(f.bytes as f64), color),
                        "Left" => cell(&p, r, lefts, &cols, k, &size(f.bytes as f64 * (1.0 - f.progress as f64)), color),
                        "Status" => {
                            let sc = Rect::from_min_size(pos2(lefts[k], r.top()), vec2(cols.widths[k], r.height()));
                            if fdone || f.held || f.progress <= 0.0 {
                                cell(&p, r, lefts, &cols, k, &status, GRAY_TEXT());
                            } else {
                                skin::progress(&p, sc.shrink2(vec2(3.0, 2.0)), f.progress, &status, theme::BAR());
                            }
                        }
                        _ => {}
                    }
                }
                return;
            }
            // A collection row.
            let name_cell = Rect::from_min_size(pos2(lefts[0], r.top()), vec2(cols.widths[0], r.height()));
            if !c.file_rows.is_empty() {
                let g = Rect::from_center_size(pos2(name_cell.left() + 10.0, r.center().y), vec2(14.0, 14.0));
                expander(&p, g.center(), c.expanded);
                if ui.interact(g, ui.scope_id().with(("exp", c.id)), Sense::click()).clicked() {
                    actions.push((c.id, if c.expanded { "collapse" } else { "expand" }));
                }
            }
            theme::paint_icon(&p, Rect::from_min_size(pos2(name_cell.left() + 19.0, r.center().y - 8.0), vec2(16.0, 16.0)), Icon::Folder, true);
            let active = c.status.as_deref() == Some("Downloading");
            p.with_clip_rect(name_cell.shrink2(vec2(4.0, 0.0)).intersect(p.clip_rect())).text(
                pos2(name_cell.left() + 39.0, r.center().y),
                Align2::LEFT_CENTER,
                elide(&p, &c.subject, if active { theme::bold(BODY) } else { theme::font(BODY) }, name_cell.width() - 43.0),
                if active { theme::bold(BODY) } else { theme::font(BODY) },
                dim,
            );
            for (k, title) in cols.titles.clone().iter().enumerate().skip(1) {
                match *title {
                    "Size" => cell(&p, r, lefts, &cols, k, &size(c.total_bytes as f64), dim),
                    "Left" => cell(&p, r, lefts, &cols, k, &size(c.remaining_bytes), dim),
                    "Parts" => cell(&p, r, lefts, &cols, k, &c.parts.to_string(), dim),
                    "Age" => cell(&p, r, lefts, &cols, k, &if c.age_days > 0.0 { format!("{:.1} d", c.age_days) } else { String::new() }, dim),
                    "Group" => cell(&p, r, lefts, &cols, k, &c.group, dim),
                    "Poster" => cell(&p, r, lefts, &cols, k, &c.poster, dim),
                    "Status" => {
                        let st = c.status.as_deref().unwrap_or("Queued");
                        if matches!(st, "Completed" | "Incomplete" | "Error") || (st == "Queued" && c.progress <= 0.0) {
                            let color = match st {
                                _ if sel => fg,
                                "Error" => theme::RED(),
                                "Incomplete" => theme::ORANGE(),
                                _ => GRAY_TEXT(),
                            };
                            cell(&p, r, lefts, &cols, k, &status_text(c), color);
                        } else {
                            let fill = match st {
                                "Paused" => theme::ORANGE(),
                                _ => theme::BAR(),
                            };
                            let sc = Rect::from_min_size(pos2(lefts[k], r.top()), vec2(cols.widths[k], r.height()));
                            skin::progress(&p, sc.shrink2(vec2(3.0, 2.0)), c.progress, &status_text(c), fill);
                        }
                    }
                    _ => {}
                }
            }
            if resp.clicked() || resp.secondary_clicked() {
                select = Some(c.id);
            }
            if resp.double_clicked() && !c.file_rows.is_empty() {
                actions.push((c.id, if c.expanded { "collapse" } else { "expand" }));
            }
            if resp.drag_started() {
                select = Some(c.id);
                start_drag = Some((c.id, ci));
            }
            if let Some((id, target)) = drag {
                if target == ci && id != c.id {
                    let from = snap.queue.iter().position(|q| q.id == id).unwrap_or(ci);
                    drop_line = Some((if target > from { r.bottom() } else { r.top() }, p.clip_rect()));
                }
            }
            let paused = c.status.as_deref() == Some("Paused");
            skin::context_menu(&resp, |ui| {
                let mut pick: Option<&'static str> = None;
                if skin::menu_item(ui, if paused { "Resume" } else { "Pause" }, "", true) {
                    pick = Some(if paused { "resume" } else { "pause" });
                }
                if !c.file_rows.is_empty() && skin::menu_item(ui, if c.expanded { "Hide files" } else { "Show files" }, "Enter", true) {
                    pick = Some(if c.expanded { "collapse" } else { "expand" });
                }
                skin::menu_separator(ui);
                for (label, key, a) in [("Move to top", "", "top"), ("Move up", "Ctrl+Up", "up"), ("Move down", "Ctrl+Down", "down"), ("Move to bottom", "", "bottom")] {
                    if skin::menu_item(ui, label, key, true) {
                        pick = Some(a);
                    }
                }
                skin::menu_separator(ui);
                if skin::menu_item(ui, "Rename...", "F2", true) {
                    pick = Some("rename");
                }
                if skin::menu_item(ui, if c.has_password { "Change archive password..." } else { "Archive password..." }, "", true) {
                    pick = Some("password");
                }
                if skin::menu_item(ui, "Unpack now", "", true) {
                    pick = Some("unpack");
                }
                skin::menu_separator(ui);
                if skin::menu_item(ui, "Delete", "Del", true) {
                    pick = Some("delete");
                }
                if let Some(a) = pick {
                    ui.close();
                    match a {
                        "password" => open_dialog = Some(Dialog::Password(c.id, String::new())),
                        "rename" => open_dialog = Some(Dialog::Rename(c.id, c.subject.clone())),
                        "delete" => open_dialog = Some(Dialog::Delete(c.id)),
                        a => actions.push((c.id, a)),
                    }
                }
            });
        });
        if narrow {
            self.cols_queue_narrow = cols;
        } else {
            self.cols_queue = cols;
        }
        if let Some(s) = select {
            self.selected = Some(s);
        }
        if let Some(d) = start_drag {
            self.drag = Some(d);
        }
        if let Some(d) = open_dialog {
            self.dialog = Some(d);
        }
        for (id, a) in actions {
            self.act(&ctx, id, a);
        }
        if let Some(t) = new_target {
            if let Some(d) = self.drag.as_mut() {
                d.1 = t;
            }
        }
        if let Some((y, clip)) = drop_line {
            ui.painter().with_clip_rect(clip).line_segment([pos2(clip.left(), y), pos2(clip.right(), y)], Stroke::new(2.0, TEXT()));
        }
        if released {
            if let Some((id, target)) = self.drag.take() {
                let from = snap.queue.iter().position(|r| r.id == id);
                if from.is_some_and(|f| f != target) {
                    self.post(&ctx, &format!("/api/queue/{id}/move?to={target}"));
                }
            }
        }
        if self.drag.is_some() {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if conns_h > 0.0 {
            ui.add_space(6.0);
            let threads = &snap.threads;
            let mut cols = self.cols_conns.clone();
            let empty = if snap.connected { "No connection is busy." } else { "Not connected. Queue > Connected, or the Connect button, connects." };
            list_view(ui, &mut cols, "conns", 0.0, threads.len(), 18.0, empty, false, |ui, i, r, lefts, cols| {
                let t = &threads[i];
                let p = ui.painter();
                cell(p, r, lefts, &cols, 0, &t.number.to_string(), TEXT());
                cell(p, r, lefts, &cols, 1, &t.action, TEXT());
                cell(p, r, lefts, &cols, 2, &t.description, TEXT());
                cell(p, r, lefts, &cols, 3, &t.status, TEXT());
                cell(p, r, lefts, &cols, 4, &t.server, TEXT());
            });
            self.cols_conns = cols;
        }
    }

    fn history_page(&mut self, ui: &mut Ui, snap: &Snapshot, narrow: bool) {
        let ctx = ui.ctx().clone();
        let n = snap.history.len();
        // The engine keeps the newest first.
        let index_of = |row: usize| row;
        let sel = self.hist_sel.filter(|i| *i < n);
        let can_retry = sel.is_some_and(|i| !snap.history[i].source.is_empty());
        let mut retry: Option<usize> = None;
        button_row(ui, |ui| {
            if ui.add_enabled_ui(can_retry, |ui| skin::icon_button(ui, "Download again", Icon::Refresh, 120.0)).inner.on_disabled_hover_text("Select a job that was added from a file or URL").clicked() {
                retry = sel;
            }
            if !narrow {
                skin::label(ui, &format!("{n} finished job{}", if n == 1 { "" } else { "s" }));
            }
        });
        let mut cols = self.cols_hist.clone();
        let desktop_w = fit_first_column(ui, &mut cols, narrow);
        let mut select = None;
        list_view(ui, &mut cols, "history", 0.0, n, if narrow { 24.0 } else { 18.0 }, "No history yet. Finished and failed jobs are listed here.", false, |ui, row, r, lefts, cols| {
            let i = index_of(row);
            let h = &snap.history[i];
            let resp = ui.interact(r, ui.scope_id().with(("hrow", i)), Sense::click());
            let fg = row_ground(ui, r, sel == Some(i));
            let p = ui.painter().clone();
            let status = if h.status.is_empty() { "Completed" } else { h.status.as_str() };
            let icon = match status {
                "Failed" => Icon::Error,
                "Incomplete" => Icon::Warning,
                _ => Icon::Check,
            };
            let name_cell = Rect::from_min_size(pos2(lefts[0], r.top()), vec2(cols.widths[0], r.height()));
            theme::paint_icon(&p, Rect::from_min_size(pos2(name_cell.left() + 4.0, r.center().y - 7.0), vec2(14.0, 14.0)), icon, true);
            p.with_clip_rect(name_cell.shrink2(vec2(4.0, 0.0)).intersect(p.clip_rect())).text(pos2(name_cell.left() + 24.0, r.center().y), Align2::LEFT_CENTER, elide(&p, &h.name, theme::font(BODY), name_cell.width() - 28.0), theme::font(BODY), fg);
            cell(&p, r, lefts, &cols, 1, status, fg);
            cell(&p, r, lefts, &cols, 2, &h.size, fg);
            cell(&p, r, lefts, &cols, 3, &h.date, fg);
            if resp.clicked() || resp.secondary_clicked() {
                select = Some(i);
            }
            let has_source = !h.source.is_empty();
            skin::context_menu(&resp, |ui| {
                if skin::menu_item(ui, "Download again", "", has_source) {
                    retry = Some(i);
                    ui.close();
                }
            });
        });
        cols.widths[0] = desktop_w;
        self.cols_hist = cols;
        if select.is_some() {
            self.hist_sel = select;
        }
        if let Some(i) = retry {
            let name = snap.history.get(i).map(|h| h.name.clone()).unwrap_or_default();
            self.client.post(&ctx, &format!("/api/history/{i}/retry"), vec![], Some(format!("Queued {name} again")));
        }
    }

    fn par2_page(&mut self, ui: &mut Ui, snap: &Snapshot, narrow: bool) {
        let ctx = ui.ctx().clone();
        let n = snap.par2.len();
        let sel = self.par2_sel.filter(|i| *i < n);
        let mut start = None;
        button_row(ui, |ui| {
            if ui.add_enabled_ui(sel.is_some(), |ui| skin::icon_button(ui, "Start", Icon::Play, 80.0)).inner.on_hover_text("Verify and repair the selected set").clicked() {
                start = sel;
            }
            if !narrow {
                skin::label(ui, "PAR2 sets are verified and repaired after their download.");
            }
        });
        let mut cols = self.cols_par2.clone();
        let desktop_w = fit_first_column(ui, &mut cols, narrow);
        let mut select = None;
        list_view(ui, &mut cols, "par2", 0.0, n, if narrow { 24.0 } else { 18.0 }, "No PAR2 sets.", false, |ui, i, r, lefts, cols| {
            let s = &snap.par2[i];
            let resp = ui.interact(r, ui.scope_id().with(("prow", i)), Sense::click());
            let fg = row_ground(ui, r, sel == Some(i));
            let p = ui.painter().clone();
            cell(&p, r, lefts, &cols, 0, &s.name, fg);
            cell(&p, r, lefts, &cols, 1, &size(s.size_mb * 1024.0 * 1024.0), fg);
            cell(&p, r, lefts, &cols, 2, &s.blocks.to_string(), fg);
            cell(&p, r, lefts, &cols, 3, &s.info, fg);
            cell(&p, r, lefts, &cols, 4, if s.unrar { "Yes" } else { "No" }, fg);
            let sc = Rect::from_min_size(pos2(lefts[5], r.top()), vec2(cols.widths[5], r.height()));
            if s.progress > 0.0 && s.progress < 1.0 {
                skin::progress(&p, sc.shrink2(vec2(3.0, 2.0)), s.progress, &format!("{} {:.0}%", s.status, s.progress * 100.0), theme::BAR());
            } else {
                cell(&p, r, lefts, &cols, 5, &s.status, fg);
            }
            if resp.clicked() || resp.secondary_clicked() {
                select = Some(i);
            }
            if resp.double_clicked() {
                start = Some(i);
            }
        });
        cols.widths[0] = desktop_w;
        self.cols_par2 = cols;
        if select.is_some() {
            self.par2_sel = select;
        }
        if let Some(i) = start {
            self.client.post(&ctx, &format!("/api/par2/{i}/start"), vec![], Some("PAR2 check started".into()));
        }
    }

    fn log_page(&mut self, ui: &mut Ui, snap: &Snapshot, narrow: bool) {
        let ctx = ui.ctx().clone();
        let f = self.log_filter.trim().to_lowercase();
        let lines: Vec<&net::LogLine> = snap.log_tail.iter().filter(|l| f.is_empty() || l.message.to_lowercase().contains(&f) || l.level.to_lowercase().contains(&f)).collect();
        button_row(ui, |ui| {
            if skin::button(ui, "Copy").on_hover_text("Copy the lines shown").clicked() {
                let text: Vec<String> = lines.iter().map(|l| format!("{}\t{}\t{}", l.time, l.level, l.message)).collect();
                ctx.copy_text(text.join("\n"));
                self.toast = Some(("Log copied".into(), ctx.input(|i| i.time) + 4.0));
            }
            skin::label(ui, "Filter:");
            skin::field(ui, if narrow { 120.0 } else { 200.0 }, 1, egui::TextEdit::singleline(&mut self.log_filter).hint_text("Filter log lines"));
            skin::label(ui, &if narrow { format!("{}/{}", lines.len(), snap.log_tail.len()) } else { format!("{} of {} lines", lines.len(), snap.log_tail.len()) });
        });
        let mut cols = self.cols_log.clone();
        list_view(ui, &mut cols, "log", 0.0, lines.len(), if narrow { 24.0 } else { 18.0 }, "The log is empty.", true, |ui, i, r, lefts, cols| {
            let l = lines[i];
            let color = match l.level.as_str() {
                "Error" => theme::RED(),
                "Warning" | "Shutdown" => theme::ORANGE(),
                "Par2" | "PAR2" | "Unrar" => theme::BLUE(),
                _ => TEXT(),
            };
            let p = ui.painter();
            cell(p, r, lefts, &cols, 0, &l.time, GRAY_TEXT());
            cell(p, r, lefts, &cols, 1, &l.level, color);
            cell(p, r, lefts, &cols, 2, &l.message, color);
        });
        self.cols_log = cols;
    }

    fn rss_page(&mut self, ui: &mut Ui, snap: &Snapshot, narrow: bool) {
        let ctx = ui.ctx().clone();
        button_row(ui, |ui| {
            if skin::icon_button(ui, "Refresh", Icon::Refresh, 90.0).clicked() {
                self.client.post(&ctx, "/api/rss/refresh", vec![], Some("RSS refresh started".into()));
            }
            if skin::button(ui, "Feeds...").clicked() {
                self.open_options(crate::settings::RSS_PAGE);
            }
            let mut timer = snap.rss_timer;
            if skin::checkbox(ui, &mut timer, if narrow { "Timer" } else { "Refresh on a timer" }).changed() {
                self.post(&ctx, &format!("/api/rss/timer?on={}", if timer { 1 } else { 0 }));
            }
        });
        let mut cols = self.cols_rss.clone();
        let desktop_w = fit_first_column(ui, &mut cols, narrow);
        list_view(ui, &mut cols, "rss", 0.0, snap.rss.len(), if narrow { 24.0 } else { 18.0 }, "No feed items. Add feeds with Feeds..., then Refresh.", false, |ui, i, r, lefts, cols| {
            let it = &snap.rss[i];
            let p = ui.painter();
            cell(p, r, lefts, &cols, 0, &it.name, TEXT());
            cell(p, r, lefts, &cols, 1, &it.date, TEXT());
            cell(p, r, lefts, &cols, 2, &it.description, GRAY_TEXT());
        });
        cols.widths[0] = desktop_w;
        self.cols_rss = cols;
    }
}

// -------------------------------------------------------------- dialogs ---

/// A message box body: a 32 px icon left of the text.
fn message(ui: &mut Ui, icon: Icon, lines: &[&str]) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(40.0, 36.0), Sense::hover());
        theme::paint_icon(ui.painter(), Rect::from_min_size(r.min + vec2(2.0, 2.0), vec2(32.0, 32.0)), icon, true);
        ui.vertical(|ui| {
            ui.add_space(2.0);
            for l in lines {
                ui.add(egui::Label::new(egui::RichText::new(*l).font(theme::font(BODY)).color(TEXT())).wrap());
            }
        });
    });
}

impl WebApp {
    fn dialogs(&mut self, ctx: &egui::Context, snap: &Snapshot, narrow: bool) {
        let Some(dialog) = self.dialog.take() else { return };
        let enter = || ctx.input(|i| i.key_pressed(Key::Enter));
        let mut keep = true;
        let next = match dialog {
            Dialog::AddNzb => {
                let (_, close) = skin::dialog(ctx, "add", "Add NZB Files", 440.0, |ui| {
                    skin::group(ui, "From this computer", |ui| {
                        skin::note(ui, "Pick one or more .nzb files. You can also drop them anywhere on this page.");
                        ui.add_space(2.0);
                        if skin::icon_button(ui, "Browse...", Icon::Folder, 100.0).clicked() {
                            self.client.pick_files(ctx, self.add_paused);
                            keep = false;
                        }
                    });
                    skin::group(ui, "From a web address", |ui| {
                        let w = ui.available_width();
                        let r = skin::field(ui, w, 1, egui::TextEdit::singleline(&mut self.add_url).hint_text("https://.../file.nzb"));
                        skin::note(ui, "The server downloads the NZB itself.");
                        if r.lost_focus() && enter() {
                            self.fetch_url(ctx);
                            keep = false;
                        }
                    });
                    skin::checkbox(ui, &mut self.add_paused, "Add them paused");
                    skin::dialog_buttons(ui, |ui| {
                        if skin::button(ui, "Cancel").clicked() {
                            keep = false;
                        }
                        let url = self.add_url.trim();
                        let can = url.starts_with("http://") || url.starts_with("https://");
                        if ui.add_enabled_ui(can, |ui| skin::default_button(ui, "Add URL")).inner.clicked() {
                            self.fetch_url(ctx);
                            keep = false;
                        }
                    });
                });
                if close {
                    keep = false;
                }
                Dialog::AddNzb
            }
            Dialog::Delete(id) => {
                let subject = snap.queue.iter().find(|r| r.id == id).map(|r| r.subject.clone()).unwrap_or_default();
                let (_, close) = skin::dialog(ctx, "delete", "Delete Job", 400.0, |ui| {
                    message(ui, Icon::Question, &["Remove this job from the queue?", &subject]);
                    ui.horizontal(|ui| {
                        ui.add_space(46.0);
                        skin::note(ui, "Files already downloaded stay on disk.");
                    });
                    skin::dialog_buttons(ui, |ui| {
                        if skin::button(ui, "No").clicked() {
                            keep = false;
                        }
                        if skin::default_button(ui, "Yes").clicked() || enter() {
                            self.act(ctx, id, "delete");
                            keep = false;
                        }
                    });
                });
                if close {
                    keep = false;
                }
                Dialog::Delete(id)
            }
            Dialog::Rename(id, mut draft) => {
                let (_, close) = skin::dialog(ctx, "rename", "Rename Job", 420.0, |ui| {
                    skin::label(ui, "New name:");
                    let w = ui.available_width();
                    let r = skin::field(ui, w, 1, egui::TextEdit::singleline(&mut draft));
                    if !r.has_focus() && ui.memory(|m| m.focused().is_none()) {
                        r.request_focus();
                    }
                    let submit = r.lost_focus() && enter();
                    skin::dialog_buttons(ui, |ui| {
                        if skin::button(ui, "Cancel").clicked() {
                            keep = false;
                        }
                        let ok = !draft.trim().is_empty();
                        if ui.add_enabled_ui(ok, |ui| skin::default_button(ui, "OK")).inner.clicked() || (submit && ok) {
                            self.client.post(ctx, &format!("/api/queue/{id}/rename"), draft.trim().as_bytes().to_vec(), None);
                            self.poll_soon(ctx);
                            keep = false;
                        }
                    });
                });
                if close {
                    keep = false;
                }
                Dialog::Rename(id, draft)
            }
            Dialog::Password(id, mut draft) => {
                let subject = snap.queue.iter().find(|r| r.id == id).map(|r| r.subject.clone()).unwrap_or_default();
                let (_, close) = skin::dialog(ctx, "password", "Archive Password", 400.0, |ui| {
                    message(ui, Icon::Key, &["The password used to unpack this job:", &subject]);
                    ui.horizontal(|ui| {
                        skin::label(ui, "Password:");
                        let w = ui.available_width();
                        let r = skin::field(ui, w, 1, egui::TextEdit::singleline(&mut draft));
                        if !r.has_focus() && ui.memory(|m| m.focused().is_none()) {
                            r.request_focus();
                        }
                        if r.lost_focus() && enter() {
                            self.client.post(ctx, &format!("/api/queue/{id}/password"), draft.as_bytes().to_vec(), Some("Password saved".into()));
                            keep = false;
                        }
                    });
                    skin::dialog_buttons(ui, |ui| {
                        if skin::button(ui, "Cancel").clicked() {
                            keep = false;
                        }
                        if keep && skin::default_button(ui, "OK").clicked() {
                            self.client.post(ctx, &format!("/api/queue/{id}/password"), draft.as_bytes().to_vec(), Some("Password saved".into()));
                            keep = false;
                        }
                    });
                });
                if close {
                    keep = false;
                }
                Dialog::Password(id, draft)
            }
            Dialog::Speed(mut tag) => {
                let full = snap.settings["max_speed_mbps"].as_f64().unwrap_or(0.0);
                let (_, close) = skin::dialog(ctx, "speed", "Speed Limit", 300.0, |ui| {
                    if full > 0.0 {
                        skin::label(ui, &format!("Full speed: {full:.2} Mbit/s"));
                    } else {
                        skin::note(ui, "Set the line speed in Options > Download to see the limits in Mbit/s.");
                    }
                    ui.add_space(2.0);
                    if skin::radio(ui, tag.is_none(), "No limit").clicked() {
                        tag = None;
                    }
                    for t in (1..=9u8).rev() {
                        let text = if full > 0.0 { format!("{} %   ({:.2} Mbit/s)", t as u32 * 10, full * t as f64 / 10.0) } else { format!("{} %", t as u32 * 10) };
                        if skin::radio(ui, tag == Some(t), &text).clicked() {
                            tag = Some(t);
                        }
                    }
                    skin::note(ui, "The limit applies to all connections together.");
                    skin::dialog_buttons(ui, |ui| {
                        if skin::button(ui, "Cancel").clicked() {
                            keep = false;
                        }
                        if skin::default_button(ui, "OK").clicked() || enter() {
                            let v = tag.map_or("none".to_string(), |t| t.to_string());
                            self.post(ctx, &format!("/api/speed-limit?tag={v}"));
                            keep = false;
                        }
                    });
                });
                if close {
                    keep = false;
                }
                Dialog::Speed(tag)
            }
            Dialog::PauseFor(mut minutes) => {
                let (_, close) = skin::dialog(ctx, "pausefor", "Pause For", 330.0, |ui| {
                    skin::label(ui, "Pause downloading for:");
                    let mut chosen = None;
                    for row in [[(5, "5 min"), (15, "15 min"), (30, "30 min"), (60, "1 h")], [(120, "2 h"), (180, "3 h"), (240, "4 h"), (300, "5 h")]] {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            for (m, label) in row {
                                if skin::button_w(ui, label, 70.0).clicked() {
                                    chosen = Some(m as f64);
                                }
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        skin::label(ui, "Custom:");
                        skin::spin(ui, "pause-min", &mut minutes, 1.0, 1440.0, 0, 70.0);
                        skin::label(ui, "min");
                        if skin::button_w(ui, "Pause", 70.0).clicked() {
                            chosen = Some(minutes);
                        }
                    });
                    if let Some(m) = chosen {
                        self.client.post(ctx, &format!("/api/pause-for?minutes={}", m as u32), vec![], Some(format!("Paused for {} min", m as u32)));
                        keep = false;
                    }
                    skin::dialog_buttons(ui, |ui| {
                        if skin::button(ui, "Close").clicked() {
                            keep = false;
                        }
                        if ui.add_enabled_ui(snap.paused, |ui| skin::button_w(ui, "Resume now", 90.0)).inner.clicked() {
                            self.post(ctx, "/api/resume");
                            keep = false;
                        }
                    });
                });
                if close {
                    keep = false;
                }
                Dialog::PauseFor(minutes)
            }
            Dialog::Stats(mut page) => {
                let (_, close) = skin::dialog(ctx, "stats", "Statistics", if narrow { 400.0 } else { 600.0 }, |ui| {
                    let titles = [("Daily transfer", Icon::Stats), ("Monthly transfer", Icon::Stats), ("Totals", Icon::Info)];
                    skin::tab_strip(ui, &mut page, &titles);
                    let h = 260.0;
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(r, 0.0, FACE());
                    skin::edge(p, r, Edge::Raised);
                    let inner = r.shrink(10.0);
                    match page {
                        0 | 1 => {
                            let (values, unit, labels): (&[f64], &str, Vec<String>) = if page == 0 {
                                (&snap.stats.daily, "MB per hour, today", (0..24).map(|h| format!("{h:02}")).collect())
                            } else {
                                (&snap.stats.monthly, "MB per month", ["J", "F", "M", "A", "M", "J", "J", "A", "S", "O", "N", "D"].iter().map(|s| s.to_string()).collect())
                            };
                            chart(p, inner, values, unit, &labels);
                        }
                        _ => {
                            let s = &snap.stats;
                            let rows = [
                                ("Downloaded this session", size(s.session_bytes as f64)),
                                ("Running for", duration(s.session_secs as f64)),
                                ("Jobs this session", s.session_collections.to_string()),
                                ("Jobs in total", s.total_collections.to_string()),
                                ("Program starts", s.startups.to_string()),
                                ("Longest run", duration(s.longest_uptime_secs as f64)),
                            ];
                            for (k, (a, b)) in rows.iter().enumerate() {
                                let y = inner.top() + 8.0 + k as f32 * 22.0;
                                p.text(pos2(inner.left() + 8.0, y), Align2::LEFT_TOP, *a, theme::font(BODY), TEXT());
                                p.text(pos2(inner.left() + 230.0, y), Align2::LEFT_TOP, b, theme::bold(BODY), TEXT());
                            }
                        }
                    }
                    skin::dialog_buttons(ui, |ui| {
                        if skin::default_button(ui, "Close").clicked() || enter() {
                            keep = false;
                        }
                    });
                });
                if close {
                    keep = false;
                }
                Dialog::Stats(page)
            }
            Dialog::About => {
                let (_, close) = skin::dialog(ctx, "about", "About NewsQueue", 380.0, |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
                        theme::paint_icon(ui.painter(), r, Icon::Nzb, true);
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("NewsQueue").font(theme::bold(14.0)).color(TEXT()));
                            skin::label(ui, &format!("Web access, version {}", env!("CARGO_PKG_VERSION")));
                            ui.add_space(4.0);
                            skin::note(ui, "A Usenet download queue. This page runs in your browser and talks to the NewsQueue on the server.");
                        });
                    });
                    skin::dialog_buttons(ui, |ui| {
                        if skin::default_button(ui, "OK").clicked() || enter() {
                            keep = false;
                        }
                    });
                });
                if close {
                    keep = false;
                }
                Dialog::About
            }
            Dialog::Options => {
                if self.settings.dialog(ctx, &self.client, snap, narrow) {
                    keep = false;
                }
                Dialog::Options
            }
        };
        if keep && self.dialog.is_none() {
            self.dialog = Some(next);
        } else if self.dialog.is_none() {
            // egui forgets the modal on the next frame; draw it now, or the
            // first click after closing a dialog is swallowed by it.
            ctx.request_repaint();
        }
    }

    fn fetch_url(&mut self, ctx: &egui::Context) {
        let url = self.add_url.trim().to_string();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return;
        }
        let mut path = format!("/api/nzb/url?url={}", net::url_encode(&url));
        if self.add_paused {
            path.push_str("&paused=1");
        }
        self.client.post(ctx, &path, vec![], Some("NZB fetched and queued".into()));
        self.add_url.clear();
    }

    fn login(&mut self, ctx: &egui::Context) {
        let mut sign_in = false;
        let _ = skin::dialog(ctx, "login", "Log On to NewsQueue", 380.0, |ui| {
            let (banner, _) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::hover());
            skin::gradient(ui.painter(), banner, theme::CAPTION_A(), theme::CAPTION_B());
            theme::paint_icon(ui.painter(), Rect::from_min_size(banner.min + vec2(10.0, 10.0), vec2(32.0, 32.0)), Icon::Nzb, true);
            ui.painter().text(pos2(banner.left() + 52.0, banner.center().y - 7.0), Align2::LEFT_CENTER, "NewsQueue", theme::bold(16.0), Color32::WHITE);
            ui.painter().text(pos2(banner.left() + 52.0, banner.center().y + 11.0), Align2::LEFT_CENTER, "Web access", theme::font(BODY), Color32::WHITE);
            ui.add_space(6.0);
            skin::label(ui, "This server needs its password (API key).");
            ui.horizontal(|ui| {
                skin::label(ui, "Password:");
                let w = ui.available_width();
                let r = skin::field(ui, w, 1, egui::TextEdit::singleline(&mut self.key_draft).password(true));
                if !r.has_focus() && self.key_draft.is_empty() {
                    r.request_focus();
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    sign_in = true;
                }
            });
            skin::dialog_buttons(ui, |ui| {
                if skin::default_button(ui, "OK").clicked() {
                    sign_in = true;
                }
            });
        });
        if sign_in {
            self.client.key = self.key_draft.trim().to_string();
            store("nq-key", &self.client.key);
            let mut s = net::lock(&self.client.shared);
            s.unauthorized = false;
            s.error = None;
            self.last_poll = -10.0;
            self.last_events_try = -10.0;
        }
    }
}

/// A bar chart in a sunken white box, bars in the selection blue.
fn chart(p: &egui::Painter, r: Rect, values: &[f64], unit: &str, labels: &[String]) {
    p.rect_filled(r, 0.0, WINDOW());
    skin::edge(p, r, Edge::Sunken);
    let max = values.iter().cloned().fold(0.0, f64::max);
    p.text(pos2(r.left() + 10.0, r.top() + 8.0), Align2::LEFT_TOP, format!("{unit} (highest {max:.0} MB)"), theme::font(theme::SMALL), GRAY_TEXT());
    let plot = Rect::from_min_max(pos2(r.left() + 12.0, r.top() + 28.0), pos2(r.right() - 12.0, r.bottom() - 22.0));
    for k in 1..4 {
        let y = plot.bottom() - plot.height() * k as f32 / 4.0;
        p.line_segment([pos2(plot.left(), y), pos2(plot.right(), y)], Stroke::new(1.0, theme::TRACK()));
    }
    p.line_segment([plot.left_bottom(), plot.right_bottom()], Stroke::new(1.0, theme::SHADOW()));
    if values.is_empty() || max <= 0.0 {
        p.text(plot.center(), Align2::CENTER_CENTER, "Nothing downloaded yet.", theme::font(BODY), GRAY_TEXT());
        return;
    }
    let n = values.len();
    let slot = plot.width() / n as f32;
    for (i, v) in values.iter().enumerate() {
        let h = (plot.height() * (*v / max) as f32).max(if *v > 0.0 { 1.0 } else { 0.0 });
        let b = Rect::from_min_max(pos2(plot.left() + i as f32 * slot + slot * 0.18, plot.bottom() - h), pos2(plot.left() + (i + 1) as f32 * slot - slot * 0.18, plot.bottom()));
        p.rect_filled(b, 0.0, theme::BAR());
        // A 3D touch: light left edge, dark right edge.
        p.rect_filled(Rect::from_min_max(b.min, pos2(b.left() + 1.0, b.bottom())), 0.0, Color32::from_rgb(0x6A, 0x84, 0xC8));
        p.rect_filled(Rect::from_min_max(pos2(b.right() - 1.0, b.top()), b.max), 0.0, Color32::from_rgb(0x04, 0x10, 0x30));
        let step = if n > 12 { 2 } else { 1 };
        if i % step == 0 {
            if let Some(l) = labels.get(i) {
                p.text(pos2(b.center().x, plot.bottom() + 4.0), Align2::CENTER_TOP, l, theme::font(theme::SMALL), TEXT());
            }
        }
    }
}
