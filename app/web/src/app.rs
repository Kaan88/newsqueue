//! The web app: Instrument-skinned queue, log and toolbar over the JSON API.

use crate::{skin, theme};
use egui::{Align2, Color32, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use serde::Deserialize;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct QueueRow {
    pub id: u64,
    pub subject: String,
    pub status: Option<String>,
    pub total_bytes: u64,
    pub remaining_bytes: f64,
    pub progress: f32,
    pub files: usize,
    pub group: String,
    pub folder: String,
    pub has_password: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct LogLine {
    pub time: String,
    pub level: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Snapshot {
    pub seq: u64,
    pub connected: bool,
    pub paused: bool,
    pub speed_bps: f64,
    pub speed_limit_bps: f64,
    pub queue_remaining_bytes: f64,
    pub queue: Vec<QueueRow>,
    pub log_tail: Vec<LogLine>,
}

/// What the HTTP callbacks hand back to the UI thread.
#[derive(Default)]
struct Shared {
    snap: Option<Snapshot>,
    /// Last error from the server (connection, 4xx/5xx).
    error: Option<String>,
    unauthorized: bool,
    in_flight: bool,
    toast: Option<String>,
}

pub struct WebApp {
    shared: Arc<Mutex<Shared>>,
    key: String,
    key_draft: String,
    tab: usize,
    selected: Option<u64>,
    last_poll: f64,
    password_for: Option<(u64, String)>,
    confirm_delete: Option<u64>,
    theme: String,
}

const TOOLBAR_HEIGHT: f32 = 59.0;
const ROW_H: f32 = 21.0;
const POLL_SECONDS: f64 = 1.0;

fn size(bytes: f64) -> String {
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

fn status_label(r: &QueueRow) -> String {
    let s = r.status.as_deref().unwrap_or("Queued");
    let label = match s {
        "Completed" => "Download complete",
        "Error" => "Error!",
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

/// The API key: `?apikey=` in the address, else the one saved in this browser.
fn initial_key() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let from_url = web_sys::window()
            .and_then(|w| w.location().search().ok())
            .and_then(|q| {
                q.trim_start_matches('?')
                    .split('&')
                    .find_map(|p| p.strip_prefix("apikey=").map(str::to_string))
            });
        if let Some(k) = from_url {
            return k;
        }
        if let Some(k) = storage().and_then(|s| s.get_item("nq-key").ok().flatten()) {
            return k;
        }
    }
    String::new()
}

fn save_key(_key: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(s) = storage() {
        let _ = s.set_item("nq-key", _key);
    }
}

fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

impl WebApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx, false);
        let key = initial_key();
        Self {
            shared: Arc::default(),
            key_draft: key.clone(),
            key,
            tab: 0,
            selected: None,
            last_poll: -10.0,
            password_for: None,
            confirm_delete: None,
            theme: "system".into(),
        }
    }

    fn request(&self, method: &str, path: &str, body: Vec<u8>) -> ehttp::Request {
        let mut req = ehttp::Request {
            method: method.into(),
            url: path.into(),
            body,
            headers: ehttp::Headers::new(&[]),
            mode: ehttp::Mode::default(),
        };
        if !self.key.is_empty() {
            req.headers.insert("X-Api-Key", &self.key);
        }
        req
    }

    fn poll(&mut self, ctx: &egui::Context) {
        {
            let s = self.shared.lock().unwrap();
            if s.in_flight || s.unauthorized {
                return;
            }
        }
        self.shared.lock().unwrap().in_flight = true;
        let shared = Arc::clone(&self.shared);
        let ctx = ctx.clone();
        ehttp::fetch(self.request("GET", "/api/state", vec![]), move |res| {
            let mut s = shared.lock().unwrap();
            s.in_flight = false;
            match res {
                Ok(r) if r.status == 401 => s.unauthorized = true,
                Ok(r) if r.ok => match serde_json::from_slice::<Snapshot>(&r.bytes) {
                    Ok(snap) => {
                        s.snap = Some(snap);
                        s.error = None;
                    }
                    Err(e) => s.error = Some(format!("bad state: {e}")),
                },
                Ok(r) => s.error = Some(format!("server: {} {}", r.status, r.status_text)),
                Err(e) => s.error = Some(format!("offline: {e}")),
            }
            ctx.request_repaint();
        });
    }

    /// Fire-and-forget action; the next poll shows its effect.
    fn post(&mut self, ctx: &egui::Context, path: &str, body: Vec<u8>) {
        let shared = Arc::clone(&self.shared);
        let ctx2 = ctx.clone();
        ehttp::fetch(self.request("POST", path, body), move |res| {
            let mut s = shared.lock().unwrap();
            match res {
                Ok(r) if r.status == 401 => s.unauthorized = true,
                Ok(r) if !r.ok => {
                    let msg = serde_json::from_slice::<serde_json::Value>(&r.bytes)
                        .ok()
                        .and_then(|v| v["error"].as_str().map(str::to_string))
                        .unwrap_or_else(|| r.status_text.clone());
                    s.toast = Some(msg);
                }
                Err(e) => s.toast = Some(e),
                _ => {}
            }
            ctx2.request_repaint();
        });
        self.poll_soon(ctx);
    }

    fn poll_soon(&mut self, ctx: &egui::Context) {
        // Show the result soon rather than after a full poll interval.
        self.last_poll = ctx.input(|i| i.time) - POLL_SECONDS + 0.25;
    }

    /// Reads a dropped file (asynchronously in the browser) and posts it as an NZB.
    fn upload(&mut self, ctx: &egui::Context, file: std::sync::Arc<dyn egui::DroppedFile + Send + Sync>) {
        let name = file
            .path()
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "upload.nzb".into());
        let req = self.request("POST", &format!("/api/nzb?name={}", url_encode(&name)), vec![]);
        let shared = Arc::clone(&self.shared);
        let ctx2 = ctx.clone();
        let done = move |res: ehttp::Result<ehttp::Response>| {
            let mut s = shared.lock().unwrap();
            match res {
                Ok(r) if r.ok => s.toast = Some(format!("Added {name}")),
                Ok(r) => {
                    let msg = serde_json::from_slice::<serde_json::Value>(&r.bytes)
                        .ok()
                        .and_then(|v| v["error"].as_str().map(str::to_string))
                        .unwrap_or_else(|| r.status_text.clone());
                    s.toast = Some(format!("{name}: {msg}"));
                }
                Err(e) => s.toast = Some(format!("{name}: {e}")),
            }
            ctx2.request_repaint();
        };
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            match file.bytes_async().await {
                Ok(bytes) => ehttp::fetch(ehttp::Request { body: bytes, ..req }, done),
                Err(e) => done(Err(e)),
            }
        });
        #[cfg(not(target_arch = "wasm32"))]
        match file.bytes() {
            Ok(bytes) => ehttp::fetch(ehttp::Request { body: bytes, ..req }, done),
            Err(e) => done(Err(e)),
        }
        self.poll_soon(ctx);
    }

    fn act(&mut self, ctx: &egui::Context, id: u64, action: &str) {
        self.post(ctx, &format!("/api/queue/{id}/{action}"), vec![]);
    }
}

impl eframe::App for WebApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::follow(ctx, &self.theme);
        let now = ctx.input(|i| i.time);
        if now - self.last_poll >= POLL_SECONDS {
            self.last_poll = now;
            self.poll(ctx);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
        // NZB files dropped on the page.
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            self.upload(ctx, f);
        }
    }

    fn ui(&mut self, root: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let (snap, error, unauthorized, toast) = {
            let mut s = self.shared.lock().unwrap();
            (s.snap.clone(), s.error.clone(), s.unauthorized, s.toast.take())
        };
        if let Some(t) = toast {
            self.shared.lock().unwrap().error = Some(t);
        }
        if unauthorized {
            self.login(root);
            skin::flush(&ctx);
            return;
        }
        let snap = snap.unwrap_or_default();

        egui::Panel::top("toolbar")
            .exact_size(TOOLBAR_HEIGHT)
            .frame(egui::Frame::NONE.fill(theme::BG()))
            .show(root, |ui| self.toolbar(ui, &snap));
        egui::Panel::top("tabs")
            .exact_size(27.0)
            .frame(egui::Frame::NONE.fill(theme::BG()))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    for (i, (name, icon)) in [("Queue", theme::Icon::Download), ("Log", theme::Icon::Notes)].into_iter().enumerate() {
                        if theme::tab_button(ui, name, icon, self.tab == i, 96.0).clicked() {
                            self.tab = i;
                        }
                    }
                });
            });
        egui::Panel::bottom("status")
            .exact_size(22.0)
            .frame(egui::Frame::NONE.fill(theme::BG()))
            .show(root, |ui| status_bar(ui, &snap, error.as_deref()));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::PANEL_WHITE()))
            .show(root, |ui| match self.tab {
                0 => self.queue(ui, &snap),
                _ => log(ui, &snap),
            });
        self.dialogs(&ctx);
        skin::flush(&ctx);
    }
}

impl WebApp {
    fn toolbar(&mut self, ui: &mut Ui, snap: &Snapshot) {
        theme::paint_toolbar_gradient(ui, ui.max_rect());
        let ctx = ui.ctx().clone();
        ui.horizontal_centered(|ui| {
            ui.add_space(4.0);
            ui.spacing_mut().item_spacing.x = 1.0;
            let (label, path) = if snap.connected { ("Disconnect", "/api/disconnect") } else { ("Connect", "/api/connect") };
            if theme::tool_button(ui, label, theme::Icon::Connect, snap.connected).clicked() {
                self.post(&ctx, path, vec![]);
            }
            let (label, path) = if snap.paused { ("Resume", "/api/resume") } else { ("Pause", "/api/pause") };
            if theme::tool_button(ui, label, theme::Icon::Pause, snap.paused).clicked() {
                self.post(&ctx, path, vec![]);
            }
            if theme::tool_button(ui, "Clean up", theme::Icon::Cleanup, false).clicked() {
                self.post(&ctx, "/api/cleanup", vec![]);
            }
            ui.add_space(10.0);
            let sel = self.selected.filter(|id| snap.queue.iter().any(|r| r.id == *id));
            for (label, icon, action) in [
                ("Top", theme::Icon::Top, "top"),
                ("Up", theme::Icon::Up, "up"),
                ("Down", theme::Icon::Down, "down"),
                ("Bottom", theme::Icon::Bottom, "bottom"),
                ("Unpack", theme::Icon::Unrar, "unpack"),
            ] {
                let r = ui.add_enabled_ui(sel.is_some(), |ui| theme::tool_button(ui, label, icon, false)).inner;
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
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                let next = match self.theme.as_str() {
                    "system" => "light",
                    "light" => "dark",
                    _ => "system",
                };
                if skin::button(ui, format!("Theme: {}", self.theme)).clicked() {
                    self.theme = next.into();
                }
                ui.label(egui::RichText::new("Drop .nzb files anywhere to add them").color(theme::TEXT_MUTED()).font(skin::sans(12.0)));
            });
        });
    }

    fn queue(&mut self, ui: &mut Ui, snap: &Snapshot) {
        let ctx = ui.ctx().clone();
        let cols: [(&str, f32); 6] = [("Subject", 0.0), ("Size", 80.0), ("Left", 80.0), ("Status", 170.0), ("Files", 50.0), ("Group", 170.0)];
        let fixed: f32 = cols.iter().map(|c| c.1).sum();
        let full = ui.available_width();
        let subject_w = (full - fixed).max(200.0);
        let widths: Vec<f32> = cols.iter().map(|c| if c.1 == 0.0 { subject_w } else { c.1 }).collect();
        // Header.
        let (hdr, _) = ui.allocate_exact_size(vec2(full, 22.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(hdr, 0.0, theme::BG_LIGHT());
        p.line_segment([hdr.left_bottom(), hdr.right_bottom()], Stroke::new(1.0, theme::BORDER()));
        let mut x = hdr.left();
        for (i, (name, _)) in cols.iter().enumerate() {
            p.text(pos2(x + 6.0, hdr.center().y), Align2::LEFT_CENTER, *name, theme::semibold(12.0), theme::TEXT_MUTED());
            x += widths[i];
            p.line_segment([pos2(x, hdr.top() + 4.0), pos2(x, hdr.bottom() - 4.0)], Stroke::new(1.0, theme::BORDER_LIGHT()));
        }
        if snap.queue.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("The queue is empty. Drop .nzb files here to add them.").color(theme::TEXT_MUTED()));
            });
            return;
        }
        egui::ScrollArea::both().auto_shrink([false, false]).show_rows(ui, ROW_H, snap.queue.len(), |ui, range| {
            for idx in range {
                let r = &snap.queue[idx];
                let (rect, resp) = ui.allocate_exact_size(vec2(full.max(fixed + 200.0), ROW_H), Sense::click());
                let selected = self.selected == Some(r.id);
                let p = ui.painter();
                if selected {
                    p.rect_filled(rect, 0.0, theme::ROW_SELECT());
                } else if resp.hovered() {
                    p.rect_filled(rect, 0.0, theme::BG_HOVER());
                }
                let done = r.status.as_deref() == Some("Completed");
                let color = if done { theme::TEXT_MUTED() } else { theme::TEXT() };
                let font = skin::sans(12.5);
                let mut x = rect.left();
                let texts = [r.subject.clone(), size(r.total_bytes as f64), size(r.remaining_bytes), String::new(), r.files.to_string(), r.group.clone()];
                for (i, t) in texts.iter().enumerate() {
                    let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(widths[i], ROW_H));
                    if i == 3 {
                        let bar = cell.shrink2(vec2(4.0, 2.0));
                        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(bar));
                        theme::progress_bar(&mut child, r.progress, &status_label(r), done);
                    } else {
                        let right = matches!(i, 1 | 2 | 4);
                        let clip = ui.painter().with_clip_rect(cell.shrink2(vec2(5.0, 0.0)));
                        let (pos, align) = if right {
                            (pos2(cell.right() - 6.0, cell.center().y), Align2::RIGHT_CENTER)
                        } else {
                            (pos2(cell.left() + 6.0, cell.center().y), Align2::LEFT_CENTER)
                        };
                        clip.text(pos, align, t, font.clone(), color);
                    }
                    x += widths[i];
                }
                if resp.clicked() || resp.secondary_clicked() {
                    self.selected = Some(r.id);
                }
                let id = r.id;
                let paused = r.status.as_deref() == Some("Paused");
                skin::context_menu(&resp, |ui| {
                    let mut pick: Option<&str> = None;
                    if skin::item(ui, false, if paused { "Resume" } else { "Pause" }).clicked() {
                        pick = Some(if paused { "resume" } else { "pause" });
                    }
                    ui.separator();
                    for (label, a) in [("Move to top", "top"), ("Move up", "up"), ("Move down", "down"), ("Move to bottom", "bottom")] {
                        if skin::item(ui, false, label).clicked() {
                            pick = Some(a);
                        }
                    }
                    ui.separator();
                    if skin::item(ui, false, "Unpack now").clicked() {
                        pick = Some("unpack");
                    }
                    if skin::item(ui, false, "Set archive password…").clicked() {
                        pick = Some("password");
                    }
                    ui.separator();
                    if skin::item(ui, false, "Delete").clicked() {
                        pick = Some("delete");
                    }
                    if let Some(a) = pick {
                        ui.close();
                        match a {
                            "password" => self.password_for = Some((id, String::new())),
                            "delete" => self.confirm_delete = Some(id),
                            a => self.act(&ctx, id, a),
                        }
                    }
                });
            }
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.confirm_delete {
            let mut close = false;
            egui::Window::new("Delete")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("Remove the selected item from the queue?");
                    ui.add_space(8.0);
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
        if let Some((id, mut draft)) = self.password_for.take() {
            let mut keep = true;
            egui::Window::new("Archive password")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("Password for the archive:");
                    let r = skin::text_edit(ui, 280.0, egui::TextEdit::singleline(&mut draft).password(true));
                    r.request_focus();
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if (skin::button(ui, "OK").clicked() || enter) && !draft.trim().is_empty() {
                            self.post(ctx, &format!("/api/queue/{id}/password"), draft.trim().as_bytes().to_vec());
                            keep = false;
                        }
                        if skin::button(ui, "Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            if keep {
                self.password_for = Some((id, draft));
            }
        }
    }

    fn login(&mut self, root: &mut Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::BG()))
            .show(root, |ui| {
                ui.add_space(ui.available_height() * 0.3);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("NewsQueue").font(theme::semibold(20.0)));
                    ui.add_space(6.0);
                    ui.label("This server needs an API key.");
                    ui.add_space(8.0);
                    let r = skin::text_edit(ui, 260.0, egui::TextEdit::singleline(&mut self.key_draft).password(true).hint_text("API key"));
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.add_space(8.0);
                    if skin::button(ui, "Sign in").clicked() || enter {
                        self.key = self.key_draft.trim().to_string();
                        save_key(&self.key);
                        let mut s = self.shared.lock().unwrap();
                        s.unauthorized = false;
                        s.error = None;
                        self.last_poll = -10.0;
                    }
                });
            });
    }
}

fn status_bar(ui: &mut Ui, snap: &Snapshot, error: Option<&str>) {
    let rect = ui.max_rect();
    ui.painter().line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, theme::BORDER()));
    ui.horizontal_centered(|ui| {
        ui.add_space(8.0);
        let dot = if snap.connected { theme::GREEN() } else { theme::TEXT_MUTED() };
        let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 4.0, dot);
        let font = skin::sans(12.0);
        let mut parts = vec![
            if snap.connected { "Connected".to_string() } else { "Not connected".to_string() },
            format!("{}/s{}", size(snap.speed_bps), if snap.paused { " (paused)" } else { "" }),
            format!("{} left in {} item(s)", size(snap.queue_remaining_bytes), snap.queue.len()),
        ];
        if snap.speed_limit_bps > 0.0 {
            parts.push(format!("limit {}/s", size(snap.speed_limit_bps)));
        }
        ui.label(egui::RichText::new(parts.join("   ·   ")).font(font.clone()));
        if let Some(e) = error {
            ui.add_space(16.0);
            ui.label(egui::RichText::new(e).font(font).color(theme::RED()));
        }
    });
}

fn log(ui: &mut Ui, snap: &Snapshot) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
        ui.add_space(4.0);
        for l in &snap.log_tail {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                let mono = skin::mono(12.0);
                ui.label(egui::RichText::new(&l.time).font(mono.clone()).color(theme::TEXT_MUTED()));
                ui.add_sized([70.0, 16.0], egui::Label::new(egui::RichText::new(&l.level).font(mono.clone()).color(Color32::from_gray(140))));
                ui.label(egui::RichText::new(&l.message).font(mono));
            });
        }
    });
}
