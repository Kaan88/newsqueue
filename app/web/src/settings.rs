//! The Options dialog: the engine's public settings grouped in pages, plus
//! the server list and RSS feeds (their own endpoints, passwords hidden).

use crate::net::{self, Client};
use crate::skin::{self, Edge};
use crate::theme::{self, BODY, FACE, GRAY_TEXT, Icon, SEL, SEL_TEXT, TEXT, WINDOW};
use egui::{Align2, Rect, Sense, TextEdit, Ui, pos2, vec2};
use serde_json::{Map, Value};

pub const PAGES: [&str; 8] = ["Servers", "Folders", "Download", "Connection", "PAR2 & unpack", "Scheduler", "RSS feeds", "Log & history"];
const PAGE_ICONS: [Icon; 8] = [Icon::Connect, Icon::Folder, Icon::Nzb, Icon::Link, Icon::Par2, Icon::PauseFor, Icon::Rss, Icon::Log];
pub const RSS_PAGE: usize = 6;

/// How one setting is edited.
enum Kind {
    Check,
    Number { min: f64, max: f64, suffix: &'static str },
    Text,
    Long,
    Choice(&'static [(&'static str, &'static str)]),
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
            ("unrar_path", "unrar program", Text, "Empty: found on PATH."),
            ("par2_path", "par2 program", Text, "Empty: found on PATH."),
            ("sevenzip_path", "7-Zip program", Text, "Empty: found on PATH."),
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
/// Height of the page area inside the dialog.
const PAGE_H: f32 = 520.0;

#[derive(Default)]
pub struct SettingsPage {
    pub page: usize,
    /// Edited copy of the public settings; None until first shown.
    draft: Option<Map<String, Value>>,
    /// The settings the draft started from, to send only changes.
    base: Map<String, Value>,
    servers: Option<Vec<Value>>,
    servers_base: Vec<Value>,
    server_sel: usize,
    rss: Option<Vec<String>>,
    rss_base: Vec<String>,
    asked_servers: bool,
    asked_rss: bool,
    close_after_servers: bool,
    /// Scheduler painting: the value being painted while dragging.
    paint: Option<bool>,
}

impl SettingsPage {
    /// Forget the drafts, so the dialog opens on what the server has now.
    pub fn reset(&mut self) {
        let page = self.page;
        *self = Self { page, ..Self::default() };
    }

    pub fn dirty(&self) -> bool {
        self.draft.as_ref().is_some_and(|d| *d != self.base)
            || self.servers.as_ref().is_some_and(|s| *s != self.servers_base)
            || self.rss.as_ref().is_some_and(|r| *r != self.rss_base)
    }

    /// Shows the dialog; true when it should close.
    pub fn dialog(&mut self, ctx: &egui::Context, client: &Client, snap: &net::Snapshot, narrow: bool) -> bool {
        let mut saved_and_close = false;
        // Take what the server sent.
        {
            let mut s = net::lock(&client.shared);
            if let Some(result) = s.servers_saved.take() {
                saved_and_close = self.close_after_servers && result.is_ok();
                self.close_after_servers = false;
                if let Ok(list) = result {
                    self.servers_base = list.clone();
                    self.server_sel = self.server_sel.min(list.len().saturating_sub(1));
                    self.servers = Some(list);
                    s.servers = None;
                }
            }
            if let Some(list) = s.servers.take().filter(|_|!s.servers_saving) {
                self.servers_base = list.clone();
                self.server_sel = self.server_sel.min(list.len().saturating_sub(1));
                self.servers = Some(list);
            }
            if let Some(list) = s.rss.take() {
                self.rss_base = list.clone();
                self.rss = Some(list);
            }
        }
        if saved_and_close { return true; }
        if self.draft.is_none() {
            if let Some(map) = snap.settings.as_object().filter(|m| !m.is_empty()) {
                self.base = map.clone();
                self.draft = Some(map.clone());
            }
        }
        if !self.asked_servers {
            self.asked_servers = true;
            client.load_servers(ctx);
        }
        if !self.asked_rss {
            self.asked_rss = true;
            client.load_rss(ctx);
        }
        let mut close = false;
        let width = if narrow { ctx.content_rect().width() } else { 680.0 };
        let page_h = if narrow { (ctx.content_rect().height() - 110.0).max(220.0) } else { PAGE_H.min(ctx.content_rect().height() - 90.0).max(260.0) };
        let (_, x) = skin::dialog(ctx, "options", "Options", width, |ui| {
            if narrow {
                ui.horizontal(|ui| {
                    skin::label(ui, "Page:");
                    let w = ui.available_width();
                    if let Some(Some(i)) = skin::combo(ui, "options-page", PAGES[self.page], w, |ui| {
                        let mut pick = None;
                        for (i, name) in PAGES.iter().enumerate() {
                            if skin::list_item(ui, self.page == i, name).clicked() {
                                pick = Some(i);
                            }
                        }
                        pick
                    }) {
                        self.page = i;
                    }
                });
                self.page_ui(ui, client, ctx, page_h, narrow);
            } else {
                ui.horizontal_top(|ui| {
                    self.page_list(ui, page_h);
                    ui.add_space(4.0);
                    ui.vertical(|ui| self.page_ui(ui, client, ctx, page_h, narrow));
                });
            }
            skin::dialog_buttons(ui, |ui| {
                let dirty = self.dirty();
                let saving = net::lock(&client.shared).servers_saving;
                if ui.add_enabled_ui(dirty && !saving, |ui| skin::button(ui, "Apply")).inner.clicked() {
                    self.save(client, ctx);
                }
                if skin::button(ui, "Cancel").clicked() {
                    close = true;
                }
                if ui.add_enabled_ui(!saving, |ui|skin::default_button(ui, "OK")).inner.clicked() {
                    let servers_dirty = self.servers.as_ref().is_some_and(|s|*s != self.servers_base);
                    self.save(client, ctx);
                    self.close_after_servers = servers_dirty;
                    close = !servers_dirty;
                }
                if dirty {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| skin::note(ui, if saving { "Saving servers..." } else { "Changes are not saved yet." }));
                }
            });
        });
        (close || x) && !net::lock(&client.shared).servers_saving
    }

    /// The list of pages on the left, like a property sheet's tree.
    fn page_list(&mut self, ui: &mut Ui, height: f32) {
        let (rect, _) = ui.allocate_exact_size(vec2(150.0, height), Sense::hover());
        ui.painter().rect_filled(rect, 0.0, WINDOW());
        skin::edge(ui.painter(), rect, Edge::Sunken);
        for (i, name) in PAGES.iter().enumerate() {
            let r = Rect::from_min_size(pos2(rect.left() + 2.0, rect.top() + 3.0 + i as f32 * 22.0), vec2(rect.width() - 4.0, 22.0));
            let resp = ui.interact(r, ui.scope_id().with(("optpage", i)), Sense::click());
            crate::audit::item(ui, PAGES[i], r);
            let tw = skin::text_width(ui, name, theme::font(BODY));
            let p = ui.painter();
            theme::paint_icon(p, Rect::from_min_size(pos2(r.left() + 4.0, r.center().y - 8.0), vec2(16.0, 16.0)), PAGE_ICONS[i], true);
            let tr = Rect::from_min_size(pos2(r.left() + 24.0, r.top() + 2.0), vec2(tw + 6.0, r.height() - 4.0));
            let sel = self.page == i;
            if sel {
                p.rect_filled(tr, 0.0, SEL());
            }
            p.text(pos2(tr.left() + 3.0, r.center().y), Align2::LEFT_CENTER, *name, theme::font(BODY), if sel { SEL_TEXT() } else { TEXT() });
            if resp.clicked() {
                self.page = i;
            }
        }
    }

    fn page_ui(&mut self, ui: &mut Ui, client: &Client, ctx: &egui::Context, height: f32, narrow: bool) {
        // A heading band like a property page title.
        let (band, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
        if theme::is_caustica() {
            crate::caustica::band_ground(ui.painter(), band);
        } else { skin::gradient(ui.painter(), band, theme::CAPTION_A(), FACE()); }
        ui.painter().text(pos2(band.left() + 6.0, band.center().y), Align2::LEFT_CENTER, PAGES[self.page], theme::bold(BODY), if theme::is_caustica() { TEXT() } else { egui::Color32::WHITE });
        ui.add_space(2.0);
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height - 30.0), Sense::hover());
        skin::scroll_view(ui, rect, ("options-scroll", self.page), [false, true], false, 20.0, |area, ui| {
            area.show(ui, |ui| {
                ui.set_width(ui.available_width() - 4.0);
                ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
                match self.page {
                    0 => { let saving=net::lock(&client.shared).servers_saving; ui.add_enabled_ui(!saving, |ui|self.servers_ui(ui, narrow)); },
                    5 => {
                        self.fields_ui(ui, narrow);
                        skin::group(ui, "Download hours", |ui| self.scheduler_ui(ui));
                    }
                    6 => {
                        self.rss_ui(ui, client, ctx);
                        skin::group(ui, "Fetching", |ui| self.fields_ui(ui, narrow));
                    }
                    _ => self.fields_ui(ui, narrow),
                }
            })
        });
    }

    fn save(&mut self, client: &Client, ctx: &egui::Context) {
        if let Some(draft) = &self.draft {
            let changed: Map<String, Value> =
                draft.iter().filter(|(k, v)| self.base.get(*k) != Some(*v)).map(|(k, v)| (k.clone(), v.clone())).collect();
            if !changed.is_empty() {
                let body = serde_json::to_vec(&Value::Object(changed)).unwrap_or_default();
                client.post(ctx, "/api/settings", body, Some("Options saved".into()));
                self.base = draft.clone();
            }
        }
        if let Some(servers) = &self.servers {
            if *servers != self.servers_base {
                if !net::lock(&client.shared).servers_saving {
                    client.save_servers(ctx,servers.clone());
                }
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
            skin::note(ui, "Waiting for the server...");
            return;
        };
        let label_w = if narrow { 0.0 } else { 220.0 };
        let field_w = if narrow { ui.available_width() } else { (ui.available_width() - label_w - 8.0).clamp(140.0, 300.0) };
        for (key, label, kind, hint) in fields(self.page) {
            let Some(value) = draft.get_mut(key) else { continue };
            match &kind {
                Check => {
                    let mut on = value.as_bool().unwrap_or(false);
                    let r = skin::checkbox(ui, &mut on, label);
                    if !hint.is_empty() {
                        r.on_hover_text(hint);
                    }
                    *value = Value::Bool(on);
                }
                _ if narrow => {
                    skin::label(ui, &format!("{label}:"));
                    edit_value(ui, value, &kind, field_w, key);
                    if !hint.is_empty() {
                        skin::note(ui, hint);
                    }
                }
                _ => {
                    ui.horizontal_top(|ui| {
                        row_label(ui, label, label_w);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            edit_value(ui, value, &kind, field_w, key);
                            if !hint.is_empty() {
                                skin::note(ui, hint);
                            }
                        });
                    });
                }
            }
        }
    }

    fn servers_ui(&mut self, ui: &mut Ui, narrow: bool) {
        let Some(servers) = self.servers.as_mut() else {
            skin::note(ui, "Loading servers...");
            return;
        };
        // The list and its buttons.
        let mut add = false;
        let mut remove = false;
        let sel_now = self.server_sel;
        let mut clicked = None;
        ui.horizontal_top(|ui| {
            let w = ui.available_width() - 90.0;
            let (rect, _) = ui.allocate_exact_size(vec2(w, 94.0), Sense::hover());
            ui.painter().rect_filled(rect, 0.0, WINDOW());
            skin::edge(ui.painter(), rect, Edge::Sunken);
            if servers.is_empty() {
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, "No servers. Add one.", theme::font(BODY), GRAY_TEXT());
            }
            for (i, s) in servers.iter().enumerate() {
                let r = Rect::from_min_size(pos2(rect.left() + 2.0, rect.top() + 2.0 + i as f32 * 18.0), vec2(rect.width() - 4.0, 18.0));
                if r.bottom() > rect.bottom() {
                    break;
                }
                let resp = ui.interact(r, ui.scope_id().with(("srv", i)), Sense::click());
                let sel = sel_now == i;
                let p = ui.painter();
                if sel {
                    p.rect_filled(r, 0.0, SEL());
                }
                let enabled = s["enabled"].as_bool().unwrap_or(true);
                theme::paint_icon(p, Rect::from_min_size(pos2(r.left() + 2.0, r.center().y - 8.0), vec2(16.0, 16.0)), Icon::Connect, enabled);
                let name = s["name"].as_str().filter(|n| !n.is_empty()).or(s["address"].as_str()).filter(|n| !n.is_empty()).unwrap_or("New server");
                let text = format!(
                    "{name}  ({}:{}, {} connections{})",
                    s["address"].as_str().unwrap_or(""),
                    s["port"].as_u64().unwrap_or(0),
                    s["connections"].as_u64().unwrap_or(0),
                    if enabled { "" } else { ", off" }
                );
                let color = if sel { SEL_TEXT() } else if enabled { TEXT() } else { GRAY_TEXT() };
                let text = crate::app::elide(p, &text, theme::font(BODY), r.width() - 26.0);
                p.with_clip_rect(r).text(pos2(r.left() + 22.0, r.center().y), Align2::LEFT_CENTER, text, theme::font(BODY), color);
                if resp.clicked() {
                    clicked = Some(i);
                }
            }
            ui.vertical(|ui| {
                if skin::button_w(ui, "Add", 84.0).clicked() {
                    add = true;
                }
                if ui.add_enabled_ui(!servers.is_empty(), |ui| skin::button_w(ui, "Remove", 84.0)).inner.clicked() {
                    remove = true;
                }
            });
        });
        if let Some(i) = clicked {
            self.server_sel = i;
        }
        if add {
            servers.push(serde_json::json!({
                "name": "", "address": "", "port": 563, "connections": 8, "ssl": true,
                "auth": true, "username": "", "password": "", "enabled": true, "level": 0,
                "max_age": 0, "quota_gb": 0
            }));
            self.server_sel = servers.len() - 1;
        }
        if remove && self.server_sel < servers.len() {
            // Later servers keep pointing at their own stored passwords.
            servers.remove(self.server_sel);
            self.server_sel = self.server_sel.min(servers.len().saturating_sub(1));
        }
        let Some(server) = servers.get_mut(self.server_sel) else { return };
        let lw = if narrow { 96.0 } else { 120.0 };
        skin::group(ui, "Server", |ui| {
            let fw = (ui.available_width() - lw - 8.0).max(100.0);
            let mut on = server["enabled"].as_bool().unwrap_or(true);
            if skin::checkbox(ui, &mut on, "Use this server").changed() {
                server["enabled"] = on.into();
            }
            server_text(ui, server, "name", "Name", false, lw, fw);
            server_text(ui, server, "address", "Address", false, lw, fw);
            for (key, label, max, unit) in [
                ("port", "Port", 65535.0, ""),
                ("connections", "Connections", 100.0, ""),
                ("level", "Priority", 9.0, if narrow { "0 = main" } else { "0 = main, higher = backup" }),
                ("max_age", "Retention", 10000.0, if narrow { "days" } else { "days (0 = unknown)" }),
                ("quota_gb", "Monthly quota", 100000.0, if narrow { "GB" } else { "GB (0 = none)" }),
            ] {
                ui.horizontal(|ui| {
                    row_label(ui, label, lw);
                    let mut v = server[key].as_f64().unwrap_or(0.0);
                    if skin::spin(ui, ("srv", key), &mut v, 0.0, max, 0, if narrow { 70.0 } else { 80.0 }).changed() {
                        server[key] = (v.round() as u64).into();
                    }
                    if !unit.is_empty() {
                        skin::label(ui, unit);
                    }
                });
            }
            let mut ssl = server["ssl"].as_bool().unwrap_or(true);
            if skin::checkbox(ui, &mut ssl, "Secure connection (TLS)").changed() {
                server["ssl"] = ssl.into();
            }
            if let Some(used) = server["used_bytes"].as_f64().filter(|u| *u > 0.0) {
                skin::note(ui, &format!("Used this month: {}", crate::app::size(used)));
            }
        });
        skin::group(ui, "Login", |ui| {
            let fw = (ui.available_width() - lw - 8.0).max(100.0);
            let mut auth = server["auth"].as_bool().unwrap_or(true);
            if skin::checkbox(ui, &mut auth, "The server needs a login").changed() {
                server["auth"] = auth.into();
            }
            ui.add_enabled_ui(auth, |ui| {
                server_text(ui, server, "username", "User name", false, lw, fw);
                server_text(ui, server, "password", "Password", true, lw, fw);
            });
            skin::note(ui, "Passwords are never sent to this page. Leave the field empty to keep the stored one.");
        });
    }

    fn rss_ui(&mut self, ui: &mut Ui, client: &Client, ctx: &egui::Context) {
        let Some(feeds) = self.rss.as_mut() else {
            skin::note(ui, "Loading feeds...");
            return;
        };
        let mut remove = None;
        let mut add = false;
        skin::group(ui, "Feeds", |ui| {
            if feeds.is_empty() {
                skin::note(ui, "No feeds yet.");
            }
            for (i, url) in feeds.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    let w = ui.available_width() - 84.0;
                    skin::field(ui, w, 1, TextEdit::singleline(url).hint_text("https://.../rss"));
                    if skin::button_w(ui, "Remove", 78.0).clicked() {
                        remove = Some(i);
                    }
                });
            }
            ui.horizontal(|ui| {
                if skin::button_w(ui, "Add feed", 84.0).clicked() {
                    add = true;
                }
                if skin::button_w(ui, "Refresh now", 90.0).clicked() {
                    client.post(ctx, "/api/rss/refresh", vec![], Some("RSS refresh started".into()));
                }
            });
        });
        if let Some(i) = remove {
            feeds.remove(i);
        }
        if add {
            feeds.push(String::new());
        }
    }

    /// 7 x 48 half-hour cells; click or drag to paint.
    fn scheduler_ui(&mut self, ui: &mut Ui) {
        let Some(draft) = self.draft.as_mut() else { return };
        let Some(days) = draft.get_mut("scheduler").and_then(|v| v.as_array_mut()) else { return };
        let label_w = 34.0;
        let cell = ((ui.available_width() - label_w - 6.0) / SLOTS as f32).floor().clamp(5.0, 12.0);
        let h = 16.0;
        let grid_w = cell * SLOTS as f32;
        // Hour ruler.
        let (ruler, _) = ui.allocate_exact_size(vec2(label_w + grid_w, 14.0), Sense::hover());
        for hour in (0..24).step_by(if cell < 8.0 { 6 } else { 3 }) {
            let x = ruler.left() + label_w + 2.0 + hour as f32 * 2.0 * cell;
            ui.painter().text(pos2(x, ruler.center().y), Align2::LEFT_CENTER, format!("{hour}"), theme::font(theme::SMALL), TEXT());
        }
        let pointer_down = ui.input(|i| i.pointer.primary_down());
        if !pointer_down {
            self.paint = None;
        }
        let top = ui.cursor().top();
        let grid = Rect::from_min_size(pos2(ruler.left() + label_w, top), vec2(grid_w + 4.0, h * 7.0 + 4.0));
        ui.painter().rect_filled(grid, 0.0, WINDOW());
        skin::edge(ui.painter(), grid, Edge::Sunken);
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add_space(2.0);
        for (d, day) in days.iter_mut().enumerate() {
            let mut bits: Vec<u8> = day.as_str().unwrap_or("").bytes().collect();
            bits.resize(SLOTS, b'1');
            let (rect, _) = ui.allocate_exact_size(vec2(label_w + grid_w + 4.0, h), Sense::hover());
            ui.painter().text(pos2(rect.left(), rect.center().y), Align2::LEFT_CENTER, DAYS[d], theme::font(BODY), TEXT());
            let mut changed = false;
            for (slot, bit) in bits.iter_mut().enumerate() {
                let r = Rect::from_min_size(pos2(rect.left() + label_w + 2.0 + slot as f32 * cell, rect.top()), vec2(cell, h));
                let resp = ui.interact(r, ui.scope_id().with(("sched", d, slot)), Sense::click_and_drag());
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
                let p = ui.painter();
                if *bit == b'1' {
                    p.rect_filled(r, 0.0, if theme::is_caustica() {
                        theme::pick(egui::Color32::from_rgb(0x52,0x86,0x65),egui::Color32::from_rgb(0x43,0x7b,0x58))
                    } else { theme::GREEN() });
                }
                // Grid lines: every hour darker.
                let line = if theme::is_caustica() && *bit == b'1' {
                    if slot % 2 == 0 { theme::pick(egui::Color32::from_rgb(0x2e,0x55,0x3f),egui::Color32::from_rgb(0x24,0x4a,0x34)) }
                    else { theme::pick(egui::Color32::from_rgb(0x6d,0x9f,0x7c),egui::Color32::from_rgb(0x54,0x8d,0x69)) }
                } else { match (*bit == b'1', slot % 2 == 0) {
                    (true, true) => egui::Color32::from_rgb(0x00, 0x50, 0x00),
                    (true, false) => egui::Color32::from_rgb(0x20, 0x98, 0x20),
                    (false, true) => theme::SHADOW(),
                    (false, false) => theme::TRACK(),
                }};
                p.line_segment([r.left_top(), r.left_bottom()], egui::Stroke::new(1.0, line));
                p.line_segment([r.left_bottom(), r.right_bottom()], egui::Stroke::new(1.0, theme::SHADOW()));
            }
            if changed {
                *day = Value::String(String::from_utf8_lossy(&bits).to_string());
            }
        }
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.add_space(6.0);
        skin::note(ui, "Green: downloading allowed. Click or drag to change.");
    }
}

fn row_label(ui: &mut Ui, label: &str, width: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(width, 21.0), Sense::hover());
    let text = format!("{label}:");
    let p = ui.painter().with_clip_rect(r);
    if ui.is_enabled() {
        p.text(pos2(r.left(), r.center().y), Align2::LEFT_CENTER, text, theme::font(BODY), TEXT());
    } else {
        skin::text_disabled(&p, pos2(r.left(), r.center().y), Align2::LEFT_CENTER, &text, theme::font(BODY));
    }
}

fn server_text(ui: &mut Ui, server: &mut Value, key: &str, label: &str, password: bool, lw: f32, fw: f32) {
    ui.horizontal(|ui| {
        row_label(ui, label, lw);
        let mut s = server[key].as_str().unwrap_or("").to_string();
        let mut edit = TextEdit::singleline(&mut s);
        if password {
            let hint = if server["password_set"].as_bool() == Some(true) { "(stored - type to replace)" } else { "(not set)" };
            edit = edit.password(true).hint_text(hint);
        }
        if skin::field(ui, fw, 1, edit).changed() {
            server[key] = s.into();
        }
    });
}

fn edit_value(ui: &mut Ui, value: &mut Value, kind: &Kind, width: f32, key: &str) {
    match kind {
        Number { min, max, suffix } => {
            let integer = value.is_u64() || value.is_i64();
            let mut v = value.as_f64().unwrap_or(0.0);
            ui.horizontal(|ui| {
                if skin::spin(ui, key, &mut v, *min, *max, if integer { 0 } else { 1 }, 90.0).changed() {
                    *value = if integer { Value::from(v.round() as u64) } else { Value::from(v) };
                }
                let unit = suffix.trim();
                if !unit.is_empty() {
                    skin::label(ui, unit);
                }
            });
        }
        Text => {
            let mut s = value.as_str().unwrap_or("").to_string();
            if skin::field(ui, width, 1, TextEdit::singleline(&mut s)).changed() {
                *value = s.into();
            }
        }
        Long => {
            let mut s = value.as_str().unwrap_or("").to_string();
            if skin::field(ui, width, 5, TextEdit::multiline(&mut s)).changed() {
                *value = s.into();
            }
        }
        Choice(options) => {
            let current = match &*value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            let shown = options.iter().find(|o| o.0.eq_ignore_ascii_case(&current)).map(|o| o.1).unwrap_or(current.as_str()).to_string();
            let picked = skin::combo(ui, key, &shown, width.min(220.0), |ui| {
                let mut pick = None;
                for (v, label) in options.iter() {
                    if skin::list_item(ui, v.eq_ignore_ascii_case(&current), label).clicked() {
                        pick = Some(*v);
                    }
                }
                pick
            });
            if let Some(Some(v)) = picked {
                *value = if value.is_number() { Value::from(v.parse::<u64>().unwrap_or(0)) } else { Value::from(v) };
            }
        }
        Check => {}
    }
}
