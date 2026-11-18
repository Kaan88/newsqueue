//! The web app: Instrument-skinned queue, history, log and settings over
//! the JSON API, with live updates from `/api/events`.

use crate::net::{self, Client, Events, QueueRow, Snapshot};
use crate::settings::SettingsPage;
use crate::{skin, theme};
use egui::{Align2, Color32, Rect, RichText, Sense, Stroke, Ui, Vec2, pos2, vec2};
use std::sync::Arc;

const TOOLBAR_HEIGHT: f32 = 59.0;
const POLL_SECONDS: f64 = 1.0;
/// Below this width the page uses its phone layout.
const NARROW: f32 = 720.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Queue,
    History,
    Log,
    Settings,
}

pub struct WebApp {
    client: Client,
    key_draft: String,
    tab: Tab,
    selected: Option<u64>,
    last_poll: f64,
    events: Option<Events>,
    last_events_try: f64,
    password_for: Option<(u64, String)>,
    rename_for: Option<(u64, String)>,
    confirm_delete: Option<u64>,
    add_open: bool,
    add_url: String,
    add_paused: bool,
    theme: String,
    /// The collection being dragged and the row it would land on.
    drag: Option<(u64, usize)>,
    /// Shown in the status line until this time.
    toast: Option<(String, f64)>,
    settings: SettingsPage,
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
        format!("{:.1} kB", b / 1024.0)
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

fn status_label(r: &QueueRow) -> String {
    let s = r.status.as_deref().unwrap_or("Queued");
    let label = match s {
        "Completed" => "Done",
        "Error" => "Error",
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

fn stored(name: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    return storage().and_then(|s| s.get_item(name).ok().flatten());
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = name;
        None
    }
}

fn store(name: &str, value: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(s) = storage() {
        let _ = s.set_item(name, value);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (name, value);
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
        theme::install(&cc.egui_ctx, false);
        let key = initial_key();
        Self {
            client: Client { shared: Arc::default(), key: key.clone() },
            key_draft: key,
            tab: Tab::Queue,
            selected: None,
            last_poll: -10.0,
            events: None,
            last_events_try: -10.0,
            password_for: None,
            rename_for: None,
            confirm_delete: None,
            add_open: false,
            add_url: String::new(),
            add_paused: false,
            theme: stored("nq-theme").unwrap_or_else(|| "system".into()),
            drag: None,
            toast: None,
            settings: SettingsPage::default(),
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
}

impl eframe::App for WebApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::follow(ctx, &self.theme);
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
            self.toast = Some((t, now + 5.0));
        }
        if self.toast.as_ref().is_some_and(|t| t.1 < now) {
            self.toast = None;
        }
        if unauthorized {
            self.login(root);
            skin::flush(&ctx);
            return;
        }
        let snap = snap.unwrap_or_default();
        let narrow = root.available_width() < NARROW;

        egui::Panel::top("toolbar")
            .exact_size(TOOLBAR_HEIGHT)
            .frame(egui::Frame::NONE.fill(theme::BG()))
            .show(root, |ui| self.toolbar(ui, &snap, narrow));
        egui::Panel::top("tabs")
            .exact_size(27.0)
            .frame(egui::Frame::NONE.fill(theme::BG()))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    if narrow {
                        ui.spacing_mut().item_spacing.x = 0.0;
                    }
                    let w = if narrow { ((ui.available_width() - 8.0) / 4.0).min(96.0) } else { 96.0 };
                    let history = format!("History{}", if narrow || snap.history.is_empty() { String::new() } else { format!(" ({})", snap.history.len()) });
                    for (tab, name, icon) in [
                        (Tab::Queue, "Queue", theme::Icon::Download),
                        (Tab::History, history.as_str(), theme::Icon::Stats),
                        (Tab::Log, "Log", theme::Icon::Notes),
                        (Tab::Settings, "Settings", theme::Icon::Setup),
                    ] {
                        let r = if narrow { compact_tab(ui, name, self.tab == tab, w) } else { theme::tab_button(ui, name, icon, self.tab == tab, w) };
                        if r.clicked() {
                            self.tab = tab;
                            ui.ctx().request_repaint();
                        }
                    }
                });
            });
        egui::Panel::bottom("status")
            .exact_size(if narrow { 40.0 } else { 22.0 })
            .frame(egui::Frame::NONE.fill(theme::BG()))
            .show(root, |ui| {
                let message = self.toast.as_ref().map(|t| (t.0.as_str(), false)).or(error.as_deref().map(|e| (e, true)));
                status_bar(ui, &snap, message, narrow)
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(if self.tab == Tab::Settings { theme::BG() } else { theme::PANEL_WHITE() }))
            .show(root, |ui| match self.tab {
                Tab::Queue => self.queue(ui, &snap, narrow),
                Tab::History => self.history(ui, &snap, narrow),
                Tab::Log => log(ui, &snap),
                Tab::Settings => self.settings.ui(ui, &self.client, &snap, narrow),
            });
        self.dialogs(&ctx, narrow);
        skin::flush(&ctx);
    }
}

impl WebApp {
    fn toolbar(&mut self, ui: &mut Ui, snap: &Snapshot, narrow: bool) {
        theme::paint_toolbar_gradient(ui, ui.max_rect());
        let ctx = ui.ctx().clone();
        egui::ScrollArea::horizontal().scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
            ui.set_min_height(TOOLBAR_HEIGHT);
            ui.horizontal_centered(|ui| {
                ui.add_space(4.0);
                ui.spacing_mut().item_spacing.x = 1.0;
                if theme::tool_button(ui, "Add NZB", theme::Icon::Nzb, self.add_open).clicked() {
                    self.add_open = true;
                }
                let (label, path) = if snap.connected { ("Disconnect", "/api/disconnect") } else { ("Connect", "/api/connect") };
                if theme::tool_button(ui, label, theme::Icon::Connect, snap.connected).clicked() {
                    self.post(&ctx, path);
                }
                let (label, path) = if snap.paused { ("Resume", "/api/resume") } else { ("Pause", "/api/pause") };
                if theme::tool_button(ui, label, theme::Icon::Pause, snap.paused).clicked() {
                    self.post(&ctx, path);
                }
                let limit = theme::tool_button(ui, "Speed", theme::Icon::Eta, snap.speed_limit_tag.is_some());
                egui::Popup::menu(&limit).show(|ui| {
                    ui.set_min_width(150.0);
                    if skin::item(ui, snap.speed_limit_tag.is_none(), "Full speed").clicked() {
                        self.post(&ctx, "/api/speed-limit?tag=none");
                    }
                    for t in (1..=9u8).rev() {
                        if skin::item(ui, snap.speed_limit_tag == Some(t), format!("{} %", t * 10)).clicked() {
                            self.post(&ctx, &format!("/api/speed-limit?tag={t}"));
                        }
                    }
                });
                if theme::tool_button(ui, "Clean up", theme::Icon::Cleanup, false).clicked() {
                    self.post(&ctx, "/api/cleanup");
                }
                ui.add_space(10.0);
                let sel = self.selected.filter(|id| snap.queue.iter().any(|r| r.id == *id));
                let movers: &[(&str, theme::Icon, &str)] = if narrow {
                    &[("Up", theme::Icon::Up, "up"), ("Down", theme::Icon::Down, "down")]
                } else {
                    &[
                        ("Top", theme::Icon::Top, "top"),
                        ("Up", theme::Icon::Up, "up"),
                        ("Down", theme::Icon::Down, "down"),
                        ("Bottom", theme::Icon::Bottom, "bottom"),
                        ("Unpack", theme::Icon::Unrar, "unpack"),
                    ]
                };
                for (label, icon, action) in movers {
                    let r = ui.add_enabled_ui(sel.is_some(), |ui| theme::tool_button(ui, label, *icon, false)).inner;
                    if r.clicked() {
                        if let Some(id) = sel {
                            self.act(&ctx, id, action);
                        }
                    }
                }
                let r = ui.add_enabled_ui(sel.is_some(), |ui| theme::tool_button(ui, "Delete", theme::Icon::Trash, false)).inner;
                if r.clicked() {
                    self.confirm_delete = sel;
                }
                ui.add_space(10.0);
                let label = match self.theme.as_str() {
                    "light" => "Light",
                    "dark" => "Dark",
                    _ => "System",
                };
                if theme::tool_button(ui, label, theme::Icon::Page, false).clicked() {
                    self.theme = match self.theme.as_str() {
                        "system" => "light",
                        "light" => "dark",
                        _ => "system",
                    }
                    .into();
                    store("nq-theme", &self.theme);
                }
                ui.add_space(6.0);
            });
        });
    }

    fn queue(&mut self, ui: &mut Ui, snap: &Snapshot, narrow: bool) {
        let ctx = ui.ctx().clone();
        let row_h = if narrow { 34.0 } else { 22.0 };
        let cols: Vec<(&str, f32)> = if narrow {
            vec![("Name", 0.0), ("Status", 118.0)]
        } else {
            vec![("Name", 0.0), ("Size", 84.0), ("Left", 84.0), ("Status", 170.0), ("Age", 64.0), ("Files", 52.0), ("Group", 180.0)]
        };
        let fixed: f32 = cols.iter().map(|c| c.1).sum();
        let full = ui.available_width();
        let name_w = (full - fixed).max(180.0);
        let widths: Vec<f32> = cols.iter().map(|c| if c.1 == 0.0 { name_w } else { c.1 }).collect();
        let total_w = widths.iter().sum::<f32>();
        // Header.
        let (hdr, _) = ui.allocate_exact_size(vec2(full, 24.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(hdr, 0.0, theme::BG_LIGHT());
        p.line_segment([hdr.left_bottom(), hdr.right_bottom()], Stroke::new(1.0, theme::BORDER()));
        let mut x = hdr.left();
        for (i, (name, _)) in cols.iter().enumerate() {
            let right = matches!(*name, "Size" | "Left" | "Files" | "Age");
            let (pos, align) = if right {
                (pos2(x + widths[i] - 8.0, hdr.center().y), Align2::RIGHT_CENTER)
            } else {
                (pos2(x + 8.0, hdr.center().y), Align2::LEFT_CENTER)
            };
            p.text(pos, align, *name, theme::semibold(12.0), theme::TEXT_MUTED());
            x += widths[i];
            if i + 1 < cols.len() {
                p.line_segment([pos2(x, hdr.top() + 5.0), pos2(x, hdr.bottom() - 5.0)], Stroke::new(1.0, theme::BORDER_LIGHT()));
            }
        }
        if snap.queue.is_empty() {
            empty_state(ui, "The queue is empty", "Drop .nzb files on this page, or use Add NZB.");
            return;
        }
        // Flatten collections and the files of expanded ones.
        let mut rows: Vec<(usize, Option<usize>)> = Vec::new();
        for (ci, c) in snap.queue.iter().enumerate() {
            rows.push((ci, None));
            if c.expanded {
                rows.extend((0..c.file_rows.len()).map(|fi| (ci, Some(fi))));
            }
        }
        let pointer = ctx.input(|i| i.pointer.interact_pos());
        let released = ctx.input(|i| i.pointer.any_released());
        let mut drop_line: Option<f32> = None;
        let mut new_target: Option<usize> = None;
        let body = egui::ScrollArea::both().auto_shrink([false, false]).show_rows(ui, row_h, rows.len(), |ui, range| {
            for idx in range {
                let (ci, file) = rows[idx];
                let c = &snap.queue[ci];
                let (rect, resp) = ui.allocate_exact_size(vec2(total_w.max(full), row_h), Sense::click_and_drag());
                if file.is_none() {
                    if let (Some((_, _)), Some(p)) = (self.drag, pointer) {
                        if p.y >= rect.top() && p.y < rect.bottom() {
                            new_target = Some(ci);
                        }
                    }
                }
                if let Some(fi) = file {
                    self.file_row(ui, rect, &c.file_rows[fi], &widths, narrow);
                    continue;
                }
                let selected = self.selected == Some(c.id);
                let dragging_this = self.drag.is_some_and(|d| d.0 == c.id);
                let p = ui.painter();
                if selected || dragging_this {
                    p.rect_filled(rect, 0.0, theme::ROW_SELECT());
                } else if resp.hovered() {
                    p.rect_filled(rect, 0.0, theme::BG_HOVER());
                }
                p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, theme::BORDER_LIGHT().gamma_multiply(0.6)));
                let done = c.status.as_deref() == Some("Completed");
                let color = if done { theme::TEXT_MUTED() } else { theme::TEXT() };
                let font = skin::sans(if narrow { 13.0 } else { 12.5 });
                let mut x = rect.left();
                for (i, (name, _)) in cols.iter().enumerate() {
                    let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(widths[i], row_h));
                    x += widths[i];
                    let right = |t: &str| {
                        ui.painter().with_clip_rect(cell).text(pos2(cell.right() - 8.0, cell.center().y), Align2::RIGHT_CENTER, t, font.clone(), color);
                    };
                    match *name {
                        "Name" => {
                            // Expand glyph, folder icon, subject.
                            let glyph = Rect::from_center_size(pos2(cell.left() + 12.0, cell.center().y), vec2(16.0, 16.0));
                            if !c.file_rows.is_empty() {
                                let g = ui.interact(glyph.expand(4.0), ui.scope_id().with(("exp", c.id)), Sense::click());
                                expander(ui.painter(), glyph.center(), c.expanded, g.hovered());
                                if g.clicked() {
                                    self.act(&ctx, c.id, if c.expanded { "collapse" } else { "expand" });
                                }
                            }
                            let icon_r = Rect::from_center_size(pos2(cell.left() + 31.0, cell.center().y), vec2(15.0, 15.0));
                            theme::paint_icon(ui.painter(), icon_r, theme::Icon::Folder, theme::TEXT_GROUP());
                            let clip = ui.painter().with_clip_rect(cell.shrink2(vec2(4.0, 0.0)));
                            if narrow {
                                clip.text(pos2(cell.left() + 44.0, cell.center().y - 7.0), Align2::LEFT_CENTER, &c.subject, theme::semibold(13.0), theme::TEXT_GROUP());
                                let sub = format!("{} · {} left", size(c.total_bytes as f64), size(c.remaining_bytes));
                                clip.text(pos2(cell.left() + 44.0, cell.center().y + 8.0), Align2::LEFT_CENTER, sub, skin::sans(11.5), theme::TEXT_MUTED());
                            } else {
                                clip.text(pos2(cell.left() + 44.0, cell.center().y), Align2::LEFT_CENTER, &c.subject, font.clone(), if done { color } else { theme::TEXT_GROUP() });
                            }
                        }
                        "Size" => right(&size(c.total_bytes as f64)),
                        "Left" => right(&size(c.remaining_bytes)),
                        "Age" => right(&if c.age_days > 0.0 { format!("{:.0} d", c.age_days) } else { String::new() }),
                        "Files" => right(&c.files.to_string()),
                        "Group" => {
                            let clip = ui.painter().with_clip_rect(cell.shrink2(vec2(4.0, 0.0)));
                            clip.text(pos2(cell.left() + 8.0, cell.center().y), Align2::LEFT_CENTER, &c.group, font.clone(), theme::TEXT_MUTED());
                        }
                        _ => {
                            let bar = cell.shrink2(vec2(5.0, if narrow { 8.0 } else { 3.0 }));
                            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(bar));
                            theme::progress_bar(&mut child, c.progress, &status_label(c), done);
                        }
                    }
                }
                if resp.clicked() || resp.secondary_clicked() {
                    self.selected = Some(c.id);
                }
                if resp.double_clicked() && !c.file_rows.is_empty() {
                    self.act(&ctx, c.id, if c.expanded { "collapse" } else { "expand" });
                }
                if resp.drag_started() {
                    self.selected = Some(c.id);
                    self.drag = Some((c.id, ci));
                }
                if let Some((id, target)) = self.drag {
                    if target == ci && id != c.id {
                        let from = snap.queue.iter().position(|r| r.id == id).unwrap_or(ci);
                        drop_line = Some(if target > from { rect.bottom() } else { rect.top() });
                    }
                }
                self.row_menu(&ctx, &resp, c);
            }
        });
        if let Some(t) = new_target {
            if let Some(d) = self.drag.as_mut() {
                d.1 = t;
            }
        }
        if let Some(y) = drop_line {
            let r = body.inner_rect;
            ui.painter().with_clip_rect(r).line_segment([pos2(r.left(), y), pos2(r.right(), y)], Stroke::new(2.0, theme::ACCENT()));
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
    }

    fn file_row(&self, ui: &mut Ui, rect: Rect, f: &net::FileRow, widths: &[f32], narrow: bool) {
        let p = ui.painter().clone();
        p.rect_filled(rect, 0.0, theme::ROW_ALT());
        let font = skin::sans(12.0);
        let color = if f.held || f.done || f.progress >= 1.0 { theme::TEXT_MUTED() } else { theme::TEXT() };
        let name = Rect::from_min_size(rect.min, vec2(widths[0], rect.height()));
        let icon_r = Rect::from_center_size(pos2(name.left() + 48.0, name.center().y), vec2(13.0, 13.0));
        theme::paint_icon(&p, icon_r, theme::Icon::File, theme::TEXT_MUTED());
        p.with_clip_rect(name.shrink2(vec2(4.0, 0.0))).text(pos2(name.left() + 60.0, name.center().y), Align2::LEFT_CENTER, &f.name, font.clone(), color);
        let mut x = rect.left() + widths[0];
        let status_text = if f.held {
            "Held (PAR2)".to_string()
        } else if f.done || f.progress >= 1.0 {
            "Done".to_string()
        } else if f.progress > 0.0 {
            format!("{:.0}%", f.progress * 100.0)
        } else {
            String::new()
        };
        if narrow {
            let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(widths[1], rect.height()));
            p.text(pos2(cell.right() - 8.0, cell.center().y), Align2::RIGHT_CENTER, format!("{}  {}", size(f.bytes as f64), status_text), skin::sans(11.5), theme::TEXT_MUTED());
            return;
        }
        for (i, w) in widths.iter().enumerate().skip(1) {
            let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(*w, rect.height()));
            x += w;
            match i {
                1 => {
                    p.text(pos2(cell.right() - 8.0, cell.center().y), Align2::RIGHT_CENTER, size(f.bytes as f64), font.clone(), color);
                }
                2 => {
                    let left = f.bytes as f64 * (1.0 - f.progress as f64);
                    p.text(pos2(cell.right() - 8.0, cell.center().y), Align2::RIGHT_CENTER, size(left), font.clone(), color);
                }
                3 => {
                    let bar = cell.shrink2(vec2(5.0, 3.0));
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(bar));
                    let complete = f.done || f.progress >= 1.0 || f.held;
                    theme::progress_bar(&mut child, if f.held { 0.0 } else { f.progress }, &status_text, complete);
                }
                _ => {}
            }
        }
    }

    fn row_menu(&mut self, ctx: &egui::Context, resp: &egui::Response, c: &QueueRow) {
        let id = c.id;
        let paused = c.status.as_deref() == Some("Paused");
        skin::context_menu(resp, |ui| {
            let mut pick: Option<&str> = None;
            if skin::item(ui, false, if paused { "Resume" } else { "Pause" }).clicked() {
                pick = Some(if paused { "resume" } else { "pause" });
            }
            if !c.file_rows.is_empty() && skin::item(ui, false, if c.expanded { "Collapse" } else { "Show files" }).clicked() {
                pick = Some(if c.expanded { "collapse" } else { "expand" });
            }
            skin::separator(ui);
            for (label, a) in [("Move to top", "top"), ("Move up", "up"), ("Move down", "down"), ("Move to bottom", "bottom")] {
                if skin::item(ui, false, label).clicked() {
                    pick = Some(a);
                }
            }
            skin::separator(ui);
            if skin::item(ui, false, "Rename…").clicked() {
                pick = Some("rename");
            }
            if skin::item(ui, false, "Unpack now").clicked() {
                pick = Some("unpack");
            }
            if skin::item(ui, false, if c.has_password { "Change archive password…" } else { "Set archive password…" }).clicked() {
                pick = Some("password");
            }
            skin::separator(ui);
            if skin::item(ui, false, "Delete").clicked() {
                pick = Some("delete");
            }
            if let Some(a) = pick {
                ui.close();
                match a {
                    "password" => self.password_for = Some((id, String::new())),
                    "rename" => self.rename_for = Some((id, c.subject.clone())),
                    "delete" => self.confirm_delete = Some(id),
                    a => self.act(ctx, id, a),
                }
            }
        });
    }

    fn history(&mut self, ui: &mut Ui, snap: &Snapshot, narrow: bool) {
        let ctx = ui.ctx().clone();
        if snap.history.is_empty() {
            empty_state(ui, "No history yet", "Finished and failed jobs are listed here.");
            return;
        }
        let row_h = if narrow { 40.0 } else { 24.0 };
        let full = ui.available_width();
        let cols: Vec<(&str, f32)> =
            if narrow { vec![("Name", 0.0), ("", 76.0)] } else { vec![("Name", 0.0), ("Status", 110.0), ("Size", 90.0), ("Finished", 150.0), ("", 80.0)] };
        let fixed: f32 = cols.iter().map(|c| c.1).sum();
        let widths: Vec<f32> = cols.iter().map(|c| if c.1 == 0.0 { (full - fixed).max(160.0) } else { c.1 }).collect();
        let (hdr, _) = ui.allocate_exact_size(vec2(full, 24.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(hdr, 0.0, theme::BG_LIGHT());
        p.line_segment([hdr.left_bottom(), hdr.right_bottom()], Stroke::new(1.0, theme::BORDER()));
        let mut x = hdr.left();
        for (i, (name, _)) in cols.iter().enumerate() {
            p.text(pos2(x + 8.0, hdr.center().y), Align2::LEFT_CENTER, *name, theme::semibold(12.0), theme::TEXT_MUTED());
            x += widths[i];
        }
        let n = snap.history.len();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_h, n, |ui, range| {
            for row in range {
                // Newest first.
                let index = n - 1 - row;
                let h = &snap.history[index];
                let (rect, resp) = ui.allocate_exact_size(vec2(full, row_h), Sense::hover());
                if resp.hovered() {
                    ui.painter().rect_filled(rect, 0.0, theme::BG_HOVER());
                }
                ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, theme::BORDER_LIGHT().gamma_multiply(0.6)));
                let status_color = match h.status.as_str() {
                    "Failed" => theme::RED(),
                    "Incomplete" => theme::ORANGE(),
                    _ => theme::GREEN(),
                };
                let status = if h.status.is_empty() { "Completed" } else { h.status.as_str() };
                let mut x = rect.left();
                for (i, (name, _)) in cols.iter().enumerate() {
                    let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(widths[i], row_h));
                    x += widths[i];
                    let clip = ui.painter().with_clip_rect(cell.shrink2(vec2(4.0, 0.0)));
                    match *name {
                        "Name" if narrow => {
                            clip.text(pos2(cell.left() + 10.0, cell.center().y - 8.0), Align2::LEFT_CENTER, &h.name, skin::sans(13.0), theme::TEXT());
                            let dot = pos2(cell.left() + 13.0, cell.center().y + 9.0);
                            clip.circle_filled(dot, 3.5, status_color);
                            clip.text(pos2(cell.left() + 22.0, cell.center().y + 9.0), Align2::LEFT_CENTER, format!("{status} · {} · {}", h.size, h.date), skin::sans(11.5), theme::TEXT_MUTED());
                        }
                        "Name" => {
                            clip.text(pos2(cell.left() + 8.0, cell.center().y), Align2::LEFT_CENTER, &h.name, skin::sans(12.5), theme::TEXT());
                        }
                        "Status" => {
                            clip.circle_filled(pos2(cell.left() + 12.0, cell.center().y), 4.0, status_color);
                            clip.text(pos2(cell.left() + 22.0, cell.center().y), Align2::LEFT_CENTER, status, skin::sans(12.5), theme::TEXT());
                        }
                        "Size" => {
                            clip.text(pos2(cell.right() - 8.0, cell.center().y), Align2::RIGHT_CENTER, &h.size, skin::sans(12.5), theme::TEXT());
                        }
                        "Finished" => {
                            clip.text(pos2(cell.left() + 8.0, cell.center().y), Align2::LEFT_CENTER, &h.date, skin::sans(12.5), theme::TEXT_MUTED());
                        }
                        _ => {
                            if !h.source.is_empty() {
                                let b = cell.shrink2(vec2(6.0, if narrow { 7.0 } else { 1.0 }));
                                let r = ui.put(b, skin::Button::new("Retry"));
                                if r.clicked() {
                                    self.client.post(&ctx, &format!("/api/history/{index}/retry"), vec![], Some(format!("Queued {} again", h.name)));
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context, narrow: bool) {
        let width = if narrow { (ctx.content_rect().width() - 40.0).max(220.0) } else { 420.0 };
        if self.add_open {
            let mut open = true;
            let mut close = false;
            skin::window("Add NZB")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.set_width(width);
                    ui.label(RichText::new("From this device").font(theme::semibold(13.0)));
                    ui.add_space(4.0);
                    if skin::button(ui, "Choose .nzb files…").clicked() {
                        self.client.pick_files(ctx, self.add_paused);
                        close = true;
                    }
                    ui.label(RichText::new("You can also drop .nzb files anywhere on the page.").font(skin::sans(11.5)).color(theme::TEXT_MUTED()));
                    ui.add_space(12.0);
                    ui.label(RichText::new("From a URL").font(theme::semibold(13.0)));
                    ui.add_space(4.0);
                    let r = skin::text_edit(ui, width, egui::TextEdit::singleline(&mut self.add_url).hint_text("https://…/file.nzb"));
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.add_space(8.0);
                    skin::checkbox_label(ui, &mut self.add_paused, "Add paused");
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let url = self.add_url.trim().to_string();
                        let can = url.starts_with("http://") || url.starts_with("https://");
                        if ui.add_enabled_ui(can, |ui| skin::button(ui, "Fetch URL")).inner.clicked() || (enter && can) {
                            let mut path = format!("/api/nzb/url?url={}", net::url_encode(&url));
                            if self.add_paused {
                                path.push_str("&paused=1");
                            }
                            self.client.post(ctx, &path, vec![], Some("NZB fetched and queued".into()));
                            self.add_url.clear();
                            close = true;
                        }
                        if skin::button(ui, "Close").clicked() {
                            close = true;
                        }
                    });
                });
            if !open || close {
                self.add_open = false;
            }
        }
        if let Some(id) = self.confirm_delete {
            let mut close = false;
            let subject = net::lock(&self.client.shared).snap.as_ref().and_then(|s| s.queue.iter().find(|r| r.id == id).map(|r| r.subject.clone()));
            skin::window("Delete")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.set_width(width.min(380.0));
                    ui.label("Remove this item from the queue?");
                    if let Some(s) = subject {
                        ui.add(egui::Label::new(RichText::new(s).color(theme::TEXT_MUTED())).truncate());
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if skin::button(ui, "Delete").clicked() {
                            self.act(ctx, id, "delete");
                            close = true;
                        }
                        if skin::button(ui, "Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.confirm_delete = None;
            }
        }
        for (title, prompt, password) in [("Archive password", "Password for the archive:", true), ("Rename", "New name:", false)] {
            let slot = if password { &mut self.password_for } else { &mut self.rename_for };
            let Some((id, mut draft)) = slot.take() else { continue };
            let mut keep = true;
            let mut submit = None;
            skin::window(title)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(prompt);
                    let mut edit = egui::TextEdit::singleline(&mut draft);
                    if password {
                        edit = edit.password(true);
                    }
                    let r = skin::text_edit(ui, width.min(320.0), edit);
                    r.request_focus();
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if (skin::button(ui, "OK").clicked() || enter) && !draft.trim().is_empty() {
                            submit = Some(draft.trim().to_string());
                            keep = false;
                        }
                        if skin::button(ui, "Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            if let Some(text) = submit {
                let path = format!("/api/queue/{id}/{}", if password { "password" } else { "rename" });
                self.client.post(ctx, &path, text.into_bytes(), None);
            }
            let slot = if password { &mut self.password_for } else { &mut self.rename_for };
            if keep {
                *slot = Some((id, draft));
            }
        }
    }

    fn login(&mut self, root: &mut Ui) {
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(theme::BG())).show(root, |ui| {
            ui.add_space(ui.available_height() * 0.28);
            ui.vertical_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                theme::paint_icon(ui.painter(), r, theme::Icon::Download, theme::ACCENT());
                ui.add_space(6.0);
                ui.label(RichText::new("NewsQueue").font(theme::semibold(20.0)));
                ui.add_space(4.0);
                ui.label(RichText::new("This server needs its API key.").color(theme::TEXT_MUTED()));
                ui.add_space(12.0);
                let w = (ui.available_width() - 40.0).min(280.0);
                let r = skin::text_edit(ui, w, egui::TextEdit::singleline(&mut self.key_draft).password(true).hint_text("API key"));
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.add_space(10.0);
                if skin::button(ui, "Sign in").clicked() || enter {
                    self.client.key = self.key_draft.trim().to_string();
                    store("nq-key", &self.client.key);
                    let mut s = net::lock(&self.client.shared);
                    s.unauthorized = false;
                    s.error = None;
                    self.last_poll = -10.0;
                    self.last_events_try = -10.0;
                }
            });
        });
    }
}

/// A tab without its icon, text centred: the phone layout's tab strip.
fn compact_tab(ui: &mut Ui, caption: &str, selected: bool, width: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
    let (dy, color) = skin::tab_chrome(ui, rect, &resp, selected, true);
    ui.painter().text(
        pos2(rect.center().x, rect.center().y + dy),
        Align2::CENTER_CENTER,
        caption,
        if selected { theme::semibold(13.0) } else { skin::sans(13.0) },
        if selected { theme::TEXT() } else { color },
    );
    resp
}

/// The tree's +/− box.
fn expander(p: &egui::Painter, center: egui::Pos2, open: bool, hot: bool) {
    let color = if hot { theme::ACCENT() } else { theme::TEXT_MUTED() };
    let r = Rect::from_center_size(center, vec2(11.0, 11.0));
    p.rect_stroke(r, 2.0, Stroke::new(1.0, color), egui::StrokeKind::Inside);
    p.line_segment([pos2(r.left() + 3.0, center.y), pos2(r.right() - 3.0, center.y)], Stroke::new(1.2, color));
    if !open {
        p.line_segment([pos2(center.x, r.top() + 3.0), pos2(center.x, r.bottom() - 3.0)], Stroke::new(1.2, color));
    }
}

fn empty_state(ui: &mut Ui, title: &str, text: &str) {
    ui.add_space((ui.available_height() * 0.3).min(160.0));
    ui.vertical_centered(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
        theme::paint_icon(ui.painter(), r, theme::Icon::Queue, theme::TEXT_MUTED());
        ui.add_space(8.0);
        ui.label(RichText::new(title).font(theme::semibold(15.0)).color(theme::TEXT()));
        ui.add_space(2.0);
        ui.label(RichText::new(text).color(theme::TEXT_MUTED()));
    });
}

fn status_bar(ui: &mut Ui, snap: &Snapshot, message: Option<(&str, bool)>, narrow: bool) {
    let rect = ui.max_rect();
    ui.painter().line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, theme::BORDER()));
    let font = skin::sans(12.0);
    let dot_color = if snap.connected { theme::GREEN() } else { theme::TEXT_MUTED() };
    let mut parts = vec![
        if snap.connected { "Connected".to_string() } else { "Not connected".to_string() },
        format!("{}/s{}", size(snap.speed_bps), if snap.paused { " (paused)" } else { "" }),
        format!("{} left", size(snap.queue_remaining_bytes)),
    ];
    if snap.speed_bps > 1.0 && snap.queue_remaining_bytes > 0.0 && !snap.paused {
        parts.push(format!("ETA {}", duration(snap.queue_remaining_bytes / snap.speed_bps)));
    }
    if !narrow {
        if snap.speed_limit_bps > 0.0 {
            parts.push(format!("limit {}/s", size(snap.speed_limit_bps)));
        }
        if snap.free_gb > 0.0 {
            parts.push(format!("{:.1} GB free", snap.free_gb));
        }
    }
    let line = |ui: &mut Ui, parts: &[String]| {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 4.0, dot_color);
            ui.label(RichText::new(parts.join("   ·   ")).font(font.clone()));
        });
    };
    let message_label = |ui: &mut Ui| {
        if let Some((m, is_error)) = message {
            let color: Color32 = if is_error { theme::RED() } else { theme::ACCENT() };
            ui.add(egui::Label::new(RichText::new(m).font(font.clone()).color(color)).truncate());
        }
    };
    if narrow {
        ui.vertical(|ui| {
            ui.add_space(2.0);
            line(ui, &parts);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                message_label(ui);
            });
        });
    } else {
        ui.horizontal_centered(|ui| {
            line(ui, &parts);
            ui.add_space(16.0);
            message_label(ui);
        });
    }
}

fn log(ui: &mut Ui, snap: &Snapshot) {
    if snap.log_tail.is_empty() {
        empty_state(ui, "The log is empty", "Connections, downloads and post-processing are logged here.");
        return;
    }
    egui::ScrollArea::both().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
        ui.add_space(4.0);
        for l in &snap.log_tail {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                let mono = skin::mono(12.0);
                ui.label(RichText::new(&l.time).font(mono.clone()).color(theme::TEXT_MUTED()));
                let level_color = match l.level.as_str() {
                    "Error" => theme::RED(),
                    "PAR2" | "Unrar" => theme::BLUE(),
                    "Connection" => theme::GREEN(),
                    _ => theme::TEXT_MUTED(),
                };
                ui.add_sized([84.0, 16.0], egui::Label::new(RichText::new(&l.level).font(mono.clone()).color(level_color)));
                let msg_color = if l.level == "Error" { theme::RED() } else { theme::TEXT() };
                ui.label(RichText::new(&l.message).font(mono).color(msg_color));
            });
        }
        ui.add_space(4.0);
    });
}
