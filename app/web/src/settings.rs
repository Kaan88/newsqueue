//! The Settings tab: the engine's public settings grouped in pages, plus
//! the server list and RSS feeds (their own endpoints, passwords hidden).

use crate::net::{self, Client};
use crate::{skin, theme};
use egui::{RichText, TextEdit, Ui, vec2};
use serde_json::{Map, Value};

pub const PAGES: [&str; 8] = ["Servers", "Folders", "Download", "Connection", "PAR2 & unpack", "Scheduler", "RSS", "Log & history"];

/// How one setting is edited.
enum Kind {
    Check,
    Number { min: f64, max: f64, suffix: &'static str },
    Text,
    Long,
    Choice(&'static [(&'static str, &'static str)]),
    Level(&'static [&'static str]),
}

use Kind::*;

const fn num(min: f64, max: f64, suffix: &'static str) -> Kind {
    Number { min, max, suffix }
}

/// (key, label, editor, hint) per page; pages 0 (servers) and 5/6 add
/// their own editors around these.
fn fields(page: usize) -> Vec<(&'static str, &'static str, Kind, &'static str)> {
    match page {
        1 => vec![
            ("download_dir", "Download folder", Text, ""),
            ("temp_dir", "Temporary folder", Text, ""),
            ("unrar_dir", "Unpack folder", Text, "Empty: unpack into the download folder."),
            ("nzb_dir", "NZB folder", Text, ""),
            ("auto_import_folder", "Watch folder", Text, "NZB files dropped here are queued."),
            ("auto_import_paused", "Queue watched NZBs paused", Check, ""),
            ("delete_nzb_after_import", "Delete watched NZBs after import", Check, ""),
            ("sort_folder", "Move finished jobs to", Text, "Empty: leave them where they are."),
            ("sort_pattern", "Folder pattern", Text, "{name}, {year}, {group}, {date} …"),
            ("skip_existing", "Skip files that already exist", Check, ""),
            ("duplicate_check", "Duplicate NZBs", Choice(&[("0", "Add them"), ("1", "Warn"), ("2", "Add them paused")]), ""),
            ("low_free_space_mb", "Pause below free space", num(0.0, 1e7, " MB"), ""),
        ],
        2 => vec![
            ("max_speed_mbps", "Line speed", num(0.0, 100000.0, " Mbit/s"), "The 100 % of the speed-limit steps."),
            ("retry_count", "Retries per article", num(0.0, 100.0, ""), ""),
            ("retry_delay", "Retry delay", num(0.0, 600.0, " s"), ""),
            ("article_cache_mb", "Article cache", num(0.0, 4096.0, " MB"), ""),
            ("cache_cleanup_minutes", "Cache clean-up every", num(0.0, 1440.0, " min"), ""),
            ("precheck_articles", "Check articles exist before downloading (STAT)", Check, ""),
            ("abort_hopeless", "Stop jobs PAR2 cannot repair", Check, ""),
            ("startup_connect", "Connect when the server starts", Check, ""),
            ("default_execute", "Run after each job", Text, "NQ_FOLDER, NQ_NAME and NQ_FILE are set."),
        ],
        3 => vec![
            ("proxy_enabled", "Use a proxy", Check, ""),
            ("proxy_kind", "Proxy type", Choice(&[("http", "HTTP CONNECT"), ("socks5", "SOCKS5")]), ""),
            ("proxy_address", "Proxy address", Text, ""),
            ("proxy_port", "Proxy port", num(0.0, 65535.0, ""), ""),
            ("proxy_auth", "Proxy needs a login", Check, "The login itself is set on the desktop."),
        ],
        4 => vec![
            ("par2_checking", "Verify with PAR2", Check, ""),
            ("par2_repairing", "Repair with PAR2", Check, ""),
            ("par2_on_demand", "Fetch PAR2 volumes only when needed", Check, ""),
            ("cleanup_after_repair", "Delete PAR2 files after a repair", Check, ""),
            ("auto_unrar", "Unpack finished jobs", Check, ""),
            ("direct_unpack", "Unpack while downloading", Check, ""),
            ("delete_archives", "Delete archives after unpacking", Check, ""),
            ("overwrite_existing", "Overwrite existing files", Check, ""),
            ("rename_existing", "Rename existing files", Check, ""),
            ("strip_root", "Drop the archive's top folder", Check, ""),
            ("create_dir_archive_name", "Subfolder per archive", Check, ""),
            ("create_dir_group_name", "Subfolder per group", Check, ""),
            ("create_dir_nzb_name", "Subfolder per NZB", Check, ""),
            ("auto_rename_to_collection", "Name output after the job", Check, ""),
            ("delete_empty_folder", "Delete empty folders", Check, ""),
            ("pause_while_unraring", "Pause downloads while unpacking", Check, ""),
            ("move_files_to_unrar", "Move these files along", Text, "Patterns, e.g. *.nfo;*.srt"),
            ("delete_files_after_unrar", "Delete these files", Text, "Patterns, e.g. *.sfv;*.url"),
        ],
        5 => vec![("scheduler_enabled", "Only download in the green hours", Check, "")],
        6 => vec![
            ("rss_refresh_minutes", "Refresh every", num(0.0, 1440.0, " min"), "0 = only on request."),
            ("case_insensitive_filters", "Filters ignore case", Check, ""),
            ("rss_filters", "Filters", Long, "One per line: a word or pattern an item must match."),
        ],
        7 => vec![
            ("log_enabled", "Keep a log", Check, ""),
            ("log_to_file", "Write the log to files", Check, ""),
            ("detailed_log", "Detailed log", Check, ""),
            ("keep_log_days", "Keep log files", num(0.0, 3650.0, " days"), ""),
            ("max_log_lines", "Lines in the Log tab", num(10.0, 100000.0, ""), ""),
            ("autosave_minutes", "Save the queue every", num(0.0, 1440.0, " min"), ""),
            ("clear_history_on_exit", "Clear the history on exit", Check, ""),
        ],
        _ => Vec::new(),
    }
}

const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const SLOTS: usize = 48;

#[derive(Default)]
pub struct SettingsPage {
    pub page: usize,
    /// Edited copy of the public settings; None until first shown.
    draft: Option<Map<String, Value>>,
    /// The settings the draft started from, to send only changes.
    base: Map<String, Value>,
    servers: Option<Vec<Value>>,
    servers_base: Vec<Value>,
    rss: Option<Vec<String>>,
    rss_base: Vec<String>,
    asked_servers: bool,
    asked_rss: bool,
    /// Scheduler painting: the value being painted while dragging.
    paint: Option<bool>,
}

fn label_cell(ui: &mut Ui, label: &str, hint: &str, width: f32) {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), egui::Sense::hover());
    ui.painter().with_clip_rect(rect).text(
        egui::pos2(rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        skin::sans(13.0),
        theme::TEXT(),
    );
    if !hint.is_empty() {
        resp.on_hover_text(hint);
    }
}

impl SettingsPage {
    pub fn dirty(&self) -> bool {
        self.draft.as_ref().is_some_and(|d| *d != self.base)
            || self.servers.as_ref().is_some_and(|s| *s != self.servers_base)
            || self.rss.as_ref().is_some_and(|r| *r != self.rss_base)
    }

    pub fn ui(&mut self, ui: &mut Ui, client: &Client, snap: &net::Snapshot, narrow: bool) {
        let ctx = ui.ctx().clone();
        // Take what the server sent.
        {
            let mut s = net::lock(&client.shared);
            if let Some(list) = s.servers.take() {
                self.servers_base = list.clone();
                self.servers = Some(list);
            }
            if let Some(list) = s.rss.take() {
                self.rss_base = list.clone();
                self.rss = Some(list);
            }
        }
        if self.draft.is_none() {
            if let Some(map) = snap.settings.as_object() {
                self.base = map.clone();
                self.draft = Some(map.clone());
            }
        }
        if !self.asked_servers {
            self.asked_servers = true;
            client.load_servers(&ctx);
        }
        if !self.asked_rss {
            self.asked_rss = true;
            client.load_rss(&ctx);
        }

        // Footer with Save / Revert.
        egui::Panel::bottom("settings_footer")
            .exact_size(44.0)
            .frame(egui::Frame::NONE.fill(theme::BG()))
            .show(ui, |ui| {
                ui.painter().line_segment([ui.max_rect().left_top(), ui.max_rect().right_top()], egui::Stroke::new(1.0, theme::BORDER()));
                ui.horizontal_centered(|ui| {
                    ui.add_space(12.0);
                    let dirty = self.dirty();
                    if ui.add_enabled_ui(dirty, |ui| skin::button(ui, "Save")).inner.clicked() {
                        self.save(client, &ctx);
                    }
                    if ui.add_enabled_ui(dirty, |ui| skin::button(ui, "Revert")).inner.clicked() {
                        self.draft = Some(self.base.clone());
                        self.servers = Some(self.servers_base.clone());
                        self.rss = Some(self.rss_base.clone());
                    }
                    ui.add_space(8.0);
                    let note = if dirty { "Unsaved changes" } else { "Saved on the server" };
                    ui.label(RichText::new(note).color(theme::TEXT_MUTED()).font(skin::sans(12.0)));
                });
            });

        if narrow {
            egui::Panel::top("settings_pages")
                .exact_size(40.0)
                .frame(egui::Frame::NONE.fill(theme::BG()).inner_margin(egui::Margin::symmetric(10, 6)))
                .show(ui, |ui| {
                    skin::ComboBox::from_id_salt("settings_page_combo")
                        .selected_text(PAGES[self.page])
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (i, name) in PAGES.iter().enumerate() {
                                skin::selectable_value(ui, &mut self.page, i, *name);
                            }
                        });
                });
        } else {
            egui::Panel::left("settings_pages")
                .exact_size(190.0)
                .resizable(false)
                .frame(egui::Frame::NONE.fill(theme::PANEL_WHITE()).inner_margin(egui::Margin::symmetric(6, 8)))
                .show(ui, |ui| {
                    ui.painter().line_segment([ui.max_rect().right_top(), ui.max_rect().right_bottom()], egui::Stroke::new(1.0, theme::BORDER()));
                    skin::heading(ui, "Settings");
                    ui.add_space(4.0);
                    for (i, name) in PAGES.iter().enumerate() {
                        if skin::selectable(ui, self.page == i, *name).clicked() {
                            self.page = i;
                        }
                    }
                });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::BG()).inner_margin(egui::Margin::symmetric(if narrow { 12 } else { 20 }, 14)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.label(RichText::new(PAGES[self.page]).font(theme::semibold(16.0)).color(theme::TEXT_GROUP()));
                    ui.add_space(10.0);
                    match self.page {
                        0 => self.servers_ui(ui, narrow),
                        5 => {
                            self.fields_ui(ui, narrow);
                            ui.add_space(8.0);
                            self.scheduler_ui(ui);
                        }
                        6 => {
                            self.rss_ui(ui, client, &ctx);
                            ui.add_space(12.0);
                            self.fields_ui(ui, narrow);
                        }
                        _ => self.fields_ui(ui, narrow),
                    }
                    ui.add_space(20.0);
                });
            });
    }

    fn save(&mut self, client: &Client, ctx: &egui::Context) {
        if let Some(draft) = &self.draft {
            let changed: Map<String, Value> =
                draft.iter().filter(|(k, v)| self.base.get(*k) != Some(*v)).map(|(k, v)| (k.clone(), v.clone())).collect();
            if !changed.is_empty() {
                let body = serde_json::to_vec(&Value::Object(changed)).unwrap_or_default();
                client.post(ctx, "/api/settings", body, Some("Settings saved".into()));
                self.base = draft.clone();
            }
        }
        if let Some(servers) = &self.servers {
            if *servers != self.servers_base {
                let body = serde_json::to_vec(servers).unwrap_or_default();
                client.post(ctx, "/api/servers", body, Some("Servers saved".into()));
                // Re-read them, so indices and "password set" are current.
                self.asked_servers = false;
            }
        }
        if let Some(rss) = &self.rss {
            if *rss != self.rss_base {
                let body = serde_json::to_vec(rss).unwrap_or_default();
                client.post(ctx, "/api/rss", body, Some("Feeds saved".into()));
                self.rss_base = rss.clone();
            }
        }
    }

    fn fields_ui(&mut self, ui: &mut Ui, narrow: bool) {
        let Some(draft) = self.draft.as_mut() else {
            ui.label(RichText::new("Waiting for the server…").color(theme::TEXT_MUTED()));
            return;
        };
        let label_w = if narrow { ui.available_width() } else { 250.0 };
        let field_w = if narrow { ui.available_width() - 4.0 } else { (ui.available_width() - label_w - 12.0).clamp(160.0, 420.0) };
        for (key, label, kind, hint) in fields(self.page) {
            let Some(value) = draft.get_mut(key) else { continue };
            let changed = self.base.get(key) != Some(value);
            let mut row = |ui: &mut Ui| match &kind {
                Check => {
                    let mut on = value.as_bool().unwrap_or(false);
                    let r = skin::checkbox_label(ui, &mut on, label);
                    if !hint.is_empty() {
                        r.on_hover_text(hint);
                    }
                    *value = Value::Bool(on);
                }
                _ => {
                    if narrow {
                        ui.label(RichText::new(label).color(theme::TEXT()));
                    } else {
                        label_cell(ui, label, hint, label_w);
                    }
                    edit_value(ui, value, &kind, field_w, key);
                }
            };
            if narrow && !matches!(kind, Check) {
                ui.vertical(|ui| row(ui));
            } else {
                ui.horizontal(|ui| {
                    row(ui);
                    if changed {
                        ui.label(RichText::new("•").color(theme::ACCENT()));
                    }
                });
            }
            if narrow && !hint.is_empty() && !matches!(kind, Check) {
                ui.label(RichText::new(hint).font(skin::sans(11.5)).color(theme::TEXT_MUTED()));
            }
            ui.add_space(if narrow { 8.0 } else { 3.0 });
        }
    }

    fn servers_ui(&mut self, ui: &mut Ui, narrow: bool) {
        let Some(servers) = self.servers.as_mut() else {
            ui.label(RichText::new("Loading servers…").color(theme::TEXT_MUTED()));
            return;
        };
        ui.label(
            RichText::new("Passwords are never sent to this page. Leave the field empty to keep the stored one.")
                .color(theme::TEXT_MUTED())
                .font(skin::sans(12.0)),
        );
        ui.add_space(8.0);
        let mut remove = None;
        let width = ui.available_width().min(640.0);
        for (i, server) in servers.iter_mut().enumerate() {
            skin::panel_frame().inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                ui.set_width(width - 26.0);
                ui.horizontal(|ui| {
                    let name = server["name"].as_str().unwrap_or("").to_string();
                    let address = server["address"].as_str().unwrap_or("").to_string();
                    let title = if name.is_empty() { address.clone() } else { name };
                    ui.label(RichText::new(if title.is_empty() { "New server".into() } else { title }).font(theme::semibold(14.0)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if skin::button(ui, "Remove").clicked() {
                            remove = Some(i);
                        }
                        let mut on = server["enabled"].as_bool().unwrap_or(true);
                        if skin::checkbox_label(ui, &mut on, "Enabled").changed() {
                            server["enabled"] = on.into();
                        }
                    });
                });
                ui.add_space(6.0);
                let lw = if narrow { 0.0 } else { 120.0 };
                let fw = if narrow { ui.available_width() } else { (ui.available_width() - lw - 8.0).max(120.0) };
                server_text(ui, server, "name", "Name", false, narrow, lw, fw);
                server_text(ui, server, "address", "Address", false, narrow, lw, fw);
                for (key, label, max, suffix) in [
                    ("port", "Port", 65535.0, ""),
                    ("connections", "Connections", 100.0, ""),
                    ("level", "Priority", 9.0, ""),
                    ("max_age", "Retention", 10000.0, " days"),
                    ("quota_gb", "Monthly quota", 100000.0, " GB"),
                ] {
                    ui.horizontal(|ui| {
                        label_cell(ui, label, "", if narrow { 110.0 } else { lw });
                        let mut v = server[key].as_f64().unwrap_or(0.0);
                        if skin::drag_value(ui, egui::DragValue::new(&mut v).range(0.0..=max).suffix(suffix)).changed() {
                            server[key] = (v.round() as u64).into();
                        }
                    });
                }
                ui.horizontal(|ui| {
                    let mut ssl = server["ssl"].as_bool().unwrap_or(true);
                    if skin::checkbox_label(ui, &mut ssl, "TLS").changed() {
                        server["ssl"] = ssl.into();
                    }
                    let mut auth = server["auth"].as_bool().unwrap_or(true);
                    if skin::checkbox_label(ui, &mut auth, "Login").changed() {
                        server["auth"] = auth.into();
                    }
                });
                if server["auth"].as_bool().unwrap_or(true) {
                    server_text(ui, server, "username", "Username", false, narrow, lw, fw);
                    server_text(ui, server, "password", "Password", true, narrow, lw, fw);
                }
                if let Some(used) = server["used_bytes"].as_f64().filter(|u| *u > 0.0) {
                    ui.label(
                        RichText::new(format!("Used this month: {}", crate::app::size(used)))
                            .font(skin::sans(11.5))
                            .color(theme::TEXT_MUTED()),
                    );
                }
            });
            ui.add_space(10.0);
        }
        if let Some(i) = remove {
            servers.remove(i);
            // Later servers keep pointing at their own stored passwords.
        }
        if skin::button(ui, "Add server").clicked() {
            servers.push(serde_json::json!({
                "name": "", "address": "", "port": 563, "connections": 8, "ssl": true,
                "auth": true, "username": "", "password": "", "enabled": true, "level": 0,
                "max_age": 0, "quota_gb": 0
            }));
        }
    }

    fn rss_ui(&mut self, ui: &mut Ui, client: &Client, ctx: &egui::Context) {
        let Some(feeds) = self.rss.as_mut() else {
            ui.label(RichText::new("Loading feeds…").color(theme::TEXT_MUTED()));
            return;
        };
        skin::heading(ui, "Feeds");
        ui.add_space(4.0);
        let width = (ui.available_width() - 90.0).clamp(160.0, 620.0);
        let mut remove = None;
        for (i, url) in feeds.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                skin::text_edit(ui, width, TextEdit::singleline(url).hint_text("https://…"));
                if skin::button(ui, "Remove").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            feeds.remove(i);
        }
        ui.horizontal(|ui| {
            if skin::button(ui, "Add feed").clicked() {
                feeds.push(String::new());
            }
            if skin::button(ui, "Refresh now").clicked() {
                client.post(ctx, "/api/rss/refresh", vec![], Some("RSS refresh started".into()));
            }
        });
    }

    /// 7 × 48 half-hour cells; click or drag to paint.
    fn scheduler_ui(&mut self, ui: &mut Ui) {
        let Some(draft) = self.draft.as_mut() else { return };
        let Some(days) = draft.get_mut("scheduler").and_then(|v| v.as_array_mut()) else { return };
        let label_w = 38.0;
        let cell = ((ui.available_width() - label_w - 4.0) / SLOTS as f32).clamp(5.0, 14.0);
        let h = 20.0;
        // Hour ruler.
        let (ruler, _) = ui.allocate_exact_size(vec2(label_w + cell * SLOTS as f32, 14.0), egui::Sense::hover());
        for hour in (0..24).step_by(if cell < 8.0 { 6 } else { 3 }) {
            let x = ruler.left() + label_w + hour as f32 * 2.0 * cell;
            ui.painter().text(egui::pos2(x, ruler.center().y), egui::Align2::LEFT_CENTER, format!("{hour}"), skin::sans(10.5), theme::TEXT_MUTED());
        }
        let pointer_down = ui.input(|i| i.pointer.primary_down());
        if !pointer_down {
            self.paint = None;
        }
        for (d, day) in days.iter_mut().enumerate() {
            let mut bits: Vec<u8> = day.as_str().unwrap_or("").bytes().collect();
            bits.resize(SLOTS, b'1');
            let (rect, _) = ui.allocate_exact_size(vec2(label_w + cell * SLOTS as f32, h), egui::Sense::hover());
            ui.painter().text(egui::pos2(rect.left(), rect.center().y), egui::Align2::LEFT_CENTER, DAYS[d], skin::sans(12.0), theme::TEXT());
            let mut changed = false;
            for (slot, bit) in bits.iter_mut().enumerate() {
                let r = egui::Rect::from_min_size(egui::pos2(rect.left() + label_w + slot as f32 * cell, rect.top() + 1.0), vec2(cell - 1.0, h - 2.0));
                let id = ui.scope_id().with(("sched", d, slot));
                let resp = ui.interact(r, id, egui::Sense::click_and_drag());
                let on = *bit == b'1';
                let hovered = ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| r.contains(p));
                if resp.drag_started() || resp.clicked() || (resp.is_pointer_button_down_on() && self.paint.is_none()) {
                    self.paint = Some(!on);
                }
                if let Some(target) = self.paint {
                    if pointer_down && hovered && on != target {
                        *bit = if target { b'1' } else { b'0' };
                        changed = true;
                    }
                }
                let fill = if *bit == b'1' { theme::GREEN() } else { theme::BG_ACTIVE() };
                ui.painter().rect_filled(r, 1.5, fill);
            }
            if changed {
                *day = Value::String(String::from_utf8_lossy(&bits).to_string());
            }
        }
        ui.add_space(4.0);
        ui.label(RichText::new("Green: downloading allowed. Click or drag to change.").font(skin::sans(11.5)).color(theme::TEXT_MUTED()));
    }
}

#[allow(clippy::too_many_arguments)]
fn server_text(ui: &mut Ui, server: &mut Value, key: &str, label: &str, password: bool, narrow: bool, lw: f32, fw: f32) {
    let mut layout = |ui: &mut Ui| {
        let mut s = server[key].as_str().unwrap_or("").to_string();
        let mut edit = TextEdit::singleline(&mut s);
        if password {
            let hint = if server["password_set"].as_bool() == Some(true) { "stored – type to replace" } else { "not set" };
            edit = edit.password(true).hint_text(hint);
        }
        if skin::text_edit(ui, fw, edit).changed() {
            server[key] = s.into();
        }
    };
    if narrow {
        ui.label(label);
        layout(ui);
    } else {
        ui.horizontal(|ui| {
            label_cell(ui, label, "", lw);
            layout(ui);
        });
    }
}

fn edit_value(ui: &mut Ui, value: &mut Value, kind: &Kind, width: f32, key: &str) {
    match kind {
        Number { min, max, suffix } => {
            let integer = value.is_u64() || value.is_i64();
            let mut v = value.as_f64().unwrap_or(0.0);
            let mut dv = egui::DragValue::new(&mut v).range(*min..=*max).suffix(*suffix);
            if integer {
                dv = dv.speed(1.0).fixed_decimals(0);
            }
            if skin::drag_value_sized(ui, width.min(140.0), dv).changed() {
                *value = if integer { Value::from(v.round() as u64) } else { Value::from(v) };
            }
        }
        Text => {
            let mut s = value.as_str().unwrap_or("").to_string();
            if skin::text_edit(ui, width, TextEdit::singleline(&mut s)).changed() {
                *value = s.into();
            }
        }
        Long => {
            let mut s = value.as_str().unwrap_or("").to_string();
            if skin::text_edit(ui, width, TextEdit::multiline(&mut s).desired_rows(5)).changed() {
                *value = s.into();
            }
        }
        Choice(options) => {
            let current = match &*value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            let shown = options.iter().find(|o| o.0.eq_ignore_ascii_case(&current)).map(|o| o.1).unwrap_or(current.as_str()).to_string();
            skin::ComboBox::from_id_salt(key).selected_text(shown).width(width.min(240.0)).show_ui(ui, |ui| {
                for (v, label) in options.iter() {
                    if skin::selectable(ui, v.eq_ignore_ascii_case(&current), *label).clicked() {
                        *value = if value.is_number() { Value::from(v.parse::<u64>().unwrap_or(0)) } else { Value::from(*v) };
                    }
                }
            });
        }
        Level(names) => {
            let i = value.as_u64().unwrap_or(0) as usize;
            skin::ComboBox::from_id_salt(key).selected_text(names.get(i).copied().unwrap_or("?")).width(width.min(240.0)).show_ui(ui, |ui| {
                for (n, label) in names.iter().enumerate() {
                    if skin::selectable(ui, n == i, *label).clicked() {
                        *value = Value::from(n as u64);
                    }
                }
            });
        }
        Check => {}
    }
}
