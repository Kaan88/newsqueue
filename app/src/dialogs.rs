use eframe::egui::{
    self, Align, Align2, Color32, FontId, Label, Layout, RichText, ScrollArea, Stroke, TextEdit,
    Ui, Vec2,
};

use crate::model::*;
use crate::theme::{self, Icon};
use crate::App;

const SETUP_PAGES: [&str; 17] = [
    "Connection",
    "Download",
    "Download #2",
    "Log",
    "Miscellaneous",
    "Miscellaneous #2",
    "Miscellaneous #3",
    "NZB",
    "NZB #2",
    "NZB Play",
    "Par2",
    "Proxy",
    "Scheduler",
    "Servers",
    "Unpack",
    "User interface",
    "User interface #2",
];

pub fn draw(app: &mut App, ctx: &egui::Context) {
    if app.show_setup {
        setup_window(app, ctx);
    }
    if app.show_server_edit {
        server_edit_window(app, ctx);
    }
    if app.show_categories {
        categories_window(app, ctx);
    }
    if app.show_stats {
        stats_window(app, ctx);
    }
    if app.show_about {
        about_window(app, ctx);
    }
    if app.show_notes {
        notes_window(app, ctx);
    }
    if app.show_columns {
        columns_window(app, ctx);
    }
    if app.show_rename {
        rename_window(app, ctx);
    }
    if app.show_pause_for {
        pause_window(app, ctx);
    }
    if app.show_shutdown {
        shutdown_window(app, ctx);
    }
    if app.show_search {
        search_window(app, ctx);
    }
    if app.show_speed_limit {
        speed_limit_window(app, ctx);
    }
    if app.show_filter_help {
        filter_help_window(app, ctx);
    }
    if app.show_rss_feeds {
        rss_feeds_window(app, ctx);
    }
    if app.show_explorer_rename {
        explorer_rename_window(app, ctx);
    }
    if app.show_execute_command {
        execute_command_window(app, ctx);
    }
    if app.show_split_file {
        split_file_window(app, ctx);
    }
    if app.show_message.is_some() {
        message_window(app, ctx);
    }
}

fn setup_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_setup;
    let mut draft = std::mem::take(&mut app.setup_draft);
    let mut ok = false;
    let mut cancel = false;
    egui::Window::new("Setup")
        .default_pos([245.0, 95.0])
        .default_size([790.0, 620.0])
        .min_size([650.0, 470.0])
        .resizable(true)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            let height = (ui.available_height() - 38.0).max(200.0);
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(205.0, height),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.painter().rect_filled(
                            ui.max_rect(),
                            0.0,
                            Color32::from_rgb(246, 249, 253),
                        );
                        ScrollArea::vertical()
                            .id_salt("setup_tree_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                for (i, name) in SETUP_PAGES.iter().enumerate() {
                                    let r = ui.selectable_label(
                                        app.setup_page == i,
                                        format!("[+] {name}"),
                                    );
                                    if r.clicked() {
                                        app.setup_page = i;
                                    }
                                }
                            });
                    },
                );
                ui.separator();
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), height),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ScrollArea::vertical()
                            .id_salt("setup_content_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(SETUP_PAGES[app.setup_page])
                                        .size(14.0)
                                        .strong()
                                        .color(theme::TEXT_GROUP),
                                );
                                ui.separator();
                                match app.setup_page {
                                    0 => page_connection(ui, &mut draft),
                                    1 => page_download(ui, &mut draft),
                                    2 => page_download2(ui, &mut draft),
                                    3 => page_log(ui, &mut draft),
                                    4 => page_misc(ui, &mut draft),
                                    5 => page_misc2(ui, &mut draft),
                                    6 => page_misc3(ui, &mut draft),
                                    7 => page_nzb(ui, &mut draft),
                                    8 => page_nzb2(ui, &mut draft),
                                    9 => page_nzb_play(ui, &mut draft),
                                    10 => page_par2(ui, &mut draft),
                                    11 => page_proxy(ui, &mut draft),
                                    12 => page_scheduler(ui, &mut draft),
                                    13 => page_servers(ui, app, &mut draft),
                                    14 => page_unpack(ui, &mut draft),
                                    15 => page_ui(ui, &mut draft),
                                    _ => page_ui2(ui, &mut draft),
                                }
                            });
                    },
                );
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Language").size(9.5));
                let mut lang = "English".to_string();
                egui::ComboBox::from_id_salt("setup_language")
                    .selected_text(lang.clone())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut lang, "English".into(), "English");
                    });
                ui.add_space(12.0);
                if ui.button("Clear history").clicked() {
                    app.log("Setup", "Search history cleared.");
                }
                if ui.button("Categories...").clicked() {
                    app.category_draft = draft.categories.clone();
                    app.show_categories = true;
                }
                if ui.button("Columns...").clicked() {
                    app.column_visible_draft = app.settings.column_visible;
                    app.column_width_draft = app.settings.column_widths;
                    app.show_columns = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("OK").clicked() {
                        ok = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        });
    app.setup_draft = draft.clone();
    if ok {
        let old_download = app.settings.download_dir.clone();
        app.settings = draft.clone();
        if !app.settings.show_tabs.iter().any(|v| *v) {
            app.settings.show_tabs[0] = true;
        }
        app.setup_draft = app.settings.clone();
        ensure_dirs(&app.settings);
        if app.explorer_path == std::path::PathBuf::from(&old_download) {
            app.explorer_path = std::path::PathBuf::from(&app.settings.download_dir);
        }
        app.show_threads = app.settings.thread_list_visible;
        app.thread_height = app.settings.thread_list_height;
        app.status_mode_speed = app.settings.speed_bytes_mode;
        app.status_mode_queue = app.settings.queue_bytes_mode;
        app.status_mode_transfered = app.settings.transfered_bytes_mode;
        app.settings.save();
        app.show_setup = false;
        app.log("Setup", "Options saved.");
        app.toast("Options saved");
        return;
    }
    if cancel || !open {
        app.show_setup = false;
    }
}

fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(2.0);
    ui.label(RichText::new(text).size(10.0).strong());
    ui.add_space(2.0);
}

fn page_connection(ui: &mut Ui, s: &mut Settings) {
    heading(ui, "Connection");
    egui::Grid::new("connection_grid")
        .num_columns(2)
        .spacing([12.0, 7.0])
        .show(ui, |ui| {
            ui.label("Maximum download speed");
            ui.horizontal(|ui| {
                let old = s.max_speed_kbps;
                ui.add(
                    egui::DragValue::new(&mut s.max_speed_kbps)
                        .speed(1.0)
                        .range(0.0..=10_000.0)
                        .suffix(" KB/s"),
                );
                if (s.max_speed_kbps - old).abs() > 0.0001 {
                    s.max_speed_mbps = s.max_speed_kbps * 8.0 / 1000.0;
                }
                ui.label(
                    RichText::new(format!("({:.2} Mbit/s)", s.max_speed_mbps))
                        .size(9.0)
                        .color(theme::TEXT_MUTED),
                );
            });
            ui.end_row();

            ui.label("Maximum download speed");
            ui.horizontal(|ui| {
                let old = s.max_speed_mbps;
                ui.add(
                    egui::DragValue::new(&mut s.max_speed_mbps)
                        .speed(0.01)
                        .range(0.0..=100.0)
                        .suffix(" Mbit/s"),
                );
                if (s.max_speed_mbps - old).abs() > 0.0001 {
                    s.max_speed_kbps = s.max_speed_mbps * 1000.0 / 8.0;
                }
                ui.label(
                    RichText::new(format!("({:.0} KB/s)", s.max_speed_kbps))
                        .size(9.0)
                        .color(theme::TEXT_MUTED),
                );
            });
            ui.end_row();

            ui.label("Nr. of connections per thread");
            ui.add(egui::DragValue::new(&mut s.connections_per_thread).range(1..=200));
            ui.end_row();

            ui.label("Global connection timeout");
            ui.add(
                egui::DragValue::new(&mut s.connection_timeout)
                    .range(1..=999)
                    .suffix(" s"),
            );
            ui.end_row();

            ui.label("Delay between connection retries");
            ui.add(
                egui::DragValue::new(&mut s.connection_retry_delay)
                    .range(1..=999)
                    .suffix(" s"),
            );
            ui.end_row();

            ui.label("XPAT connection timeout");
            ui.add(
                egui::DragValue::new(&mut s.xpat_timeout)
                    .range(1..=9999)
                    .suffix(" s"),
            );
            ui.end_row();

            ui.label("Retry count");
            ui.add(egui::DragValue::new(&mut s.retry_count).range(0..=999));
            ui.end_row();

            ui.label("Delay between retries");
            ui.add(
                egui::DragValue::new(&mut s.retry_delay)
                    .range(0..=1440)
                    .suffix(" s"),
            );
            ui.end_row();

            ui.label("Grace period");
            ui.add(
                egui::DragValue::new(&mut s.grace_period)
                    .range(0..=1440)
                    .suffix(" min"),
            );
            ui.end_row();

            ui.label("Connected while idle");
            ui.checkbox(&mut s.connected_while_idle, "");
            ui.end_row();
        });
    ui.add_space(8.0);
    ui.label(
        RichText::new("The original profile saved 8 connections with SSL on port 563. SSL switches the default port between 563 and 119. The limits are 200 connections per server and 8 servers per level.")
            .size(9.0)
            .color(theme::TEXT_MUTED),
    );
}

fn dir_row(ui: &mut Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.horizontal(|ui| {
        ui.add_sized([340.0, 20.0], TextEdit::singleline(value));
        if ui.button("Browse...").clicked() {
            if let Some(p) = rfd::FileDialog::new().pick_folder() {
                *value = p.to_string_lossy().to_string();
            }
        }
    });
    ui.end_row();
}

fn page_download(ui: &mut Ui, s: &mut Settings) {
    heading(ui, "Download");
    egui::Grid::new("download_grid")
        .num_columns(2)
        .spacing([12.0, 7.0])
        .show(ui, |ui| {
            dir_row(ui, "Default download folder", &mut s.download_dir);
            dir_row(ui, "Default NZB folder", &mut s.nzb_dir);
            dir_row(ui, "Temporary folder", &mut s.temp_dir);
            dir_row(ui, "Unrar folder", &mut s.unrar_dir);

            ui.label("Maximal download path length");
            ui.add(
                egui::DragValue::new(&mut s.max_path_length)
                    .range(0..=100_000)
                    .suffix(" characters"),
            );
            ui.end_row();

            ui.label("If free space on destination drive gets below");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut s.low_free_space_mb)
                        .range(0..=1_000_000)
                        .suffix(" MB"),
                );
                ui.label(RichText::new("pause downloading").size(9.0));
            });
            ui.end_row();

            ui.label("Autosave download queue every");
            ui.add(
                egui::DragValue::new(&mut s.autosave_minutes)
                    .range(1..=999)
                    .suffix(" min"),
            );
            ui.end_row();

            ui.label("Cleanup");
            ui.horizontal(|ui| {
                ui.checkbox(&mut s.cleanup_queue, "Download queue");
                ui.checkbox(&mut s.par2_checking, "Par2 list");
            });
            ui.end_row();

            ui.label("Skip if file is already downloaded");
            ui.checkbox(&mut s.skip_existing, "");
            ui.end_row();

            ui.label("Prompt for name when importing");
            ui.checkbox(&mut s.prompt_for_name, "");
            ui.end_row();

            ui.label("Dblclick toggles expand/collapse");
            ui.checkbox(&mut s.dblclick_expand, "");
            ui.end_row();
        });
    ui.add_space(8.0);
    ui.label(RichText::new("Use the Categories and Columns buttons at the bottom of the Setup window to edit categories and the queue columns.").size(9.0).color(theme::TEXT_MUTED));
}

fn page_download2(ui: &mut Ui, s: &mut Settings) {
    heading(ui, "Download #2");
    egui::Grid::new("download2_grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Always add to top");
            ui.checkbox(&mut s.add_to_top, "Add new collection as first in list");
            ui.end_row();
            ui.label("Sort by subject while importing");
            ui.checkbox(&mut s.sort_nzb, "");
            ui.end_row();
            ui.label("Sort rars while importing");
            ui.checkbox(&mut s.sort_rars, "");
            ui.end_row();
            ui.label("Merge collections with same name");
            ui.checkbox(&mut s.merge_nzb, "");
            ui.end_row();
            ui.label("Delete NZB file after importing");
            ui.checkbox(&mut s.delete_nzb_after_import, "");
            ui.end_row();
            ui.label("Import as paused");
            ui.checkbox(&mut s.auto_import_paused, "");
            ui.end_row();
            ui.label("Case insensitive subject filters");
            ui.checkbox(&mut s.case_insensitive_filters, "");
            ui.end_row();
        });
    ui.add_space(8.0);
    heading(ui, "Pause downloading");
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut s.pause_nzb, "nzb files");
        ui.checkbox(&mut s.pause_samples, "sample files");
        ui.checkbox(&mut s.pause_par2, "par2 recovery files");
    });
}

fn page_log(ui: &mut Ui, s: &mut Settings) {
    heading(ui, "Log");
    egui::Grid::new("log_grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Enable logging");
            ui.checkbox(&mut s.log_enabled, "");
            ui.end_row();
            ui.label("Log to file");
            ui.checkbox(&mut s.log_to_file, "");
            ui.end_row();
            ui.label("Keep log files for previous");
            ui.add(
                egui::DragValue::new(&mut s.keep_log_days)
                    .range(0..=999)
                    .suffix(" days"),
            );
            ui.end_row();
            ui.label("Maximum visible lines in Log tab");
            ui.add(egui::DragValue::new(&mut s.max_log_lines).range(0..=100_000));
            ui.end_row();
            ui.label("Detailed logging");
            ui.checkbox(&mut s.detailed_log, "");
            ui.end_row();
            ui.label("Download history");
            ui.checkbox(&mut s.download_history, "");
            ui.end_row();
            ui.label("Speed and transfered bytes");
            ui.checkbox(&mut s.speed_and_bytes, "");
            ui.end_row();
            ui.label("Keep download history for last");
            ui.add(
                egui::DragValue::new(&mut s.keep_log_days)
                    .range(0..=999)
                    .suffix(" days"),
            );
            ui.end_row();
        });
    ui.label(
        RichText::new("The log keeps 200 visible lines in this build and writes one file per day.")
            .size(9.0)
            .color(theme::TEXT_MUTED),
    );
}

fn page_misc(ui: &mut Ui, s: &mut Settings) {
    heading(ui, "Miscellaneous");
    egui::Grid::new("misc_grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Minimize to tray");
            ui.checkbox(&mut s.minimize_to_tray, "");
            ui.end_row();
            ui.label("Tray balloon hints");
            ui.checkbox(&mut s.tray_balloon, "");
            ui.end_row();
            ui.label("Display speed in tray");
            ui.checkbox(&mut s.speed_in_tray, "");
            ui.end_row();
            ui.label("Progress on taskbar");
            ui.checkbox(&mut s.progress_taskbar, "");
            ui.end_row();
            ui.label("Check for new version every");
            ui.add(
                egui::DragValue::new(&mut s.check_version_minutes)
                    .range(1..=9999)
                    .suffix(" min"),
            );
            ui.end_row();
            ui.label("Enable status in XML file");
            ui.checkbox(&mut s.status_xml, "");
            ui.end_row();
            ui.label("Clear history on exit");
            ui.checkbox(&mut s.clear_history_on_exit, "");
            ui.end_row();
            ui.label("Tray single click restores");
            ui.checkbox(&mut s.tray_single_click, "");
            ui.end_row();
        });
}

fn page_misc2(ui: &mut Ui, s: &mut Settings) {
    heading(ui, "Miscellaneous #2");
    egui::Grid::new("misc2_grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Article cache size");
            ui.add(
                egui::DragValue::new(&mut s.article_cache_mb)
                    .range(0..=500)
                    .suffix(" MB"),
            );
            ui.end_row();
            ui.label("Cache cleanup time");
            ui.add(
                egui::DragValue::new(&mut s.cache_cleanup_minutes)
                    .range(1..=60)
                    .suffix(" min"),
            );
            ui.end_row();
            ui.label("Decoder sleep cycle lines");
            ui.add(egui::DragValue::new(&mut s.decoder_sleep_lines).range(1..=4000));
            ui.end_row();
            ui.label("Download thread priority");
            ui.add(egui::DragValue::new(&mut s.download_priority).range(0..=5));
            ui.end_row();
            ui.label("Decoder thread priority");
            ui.add(egui::DragValue::new(&mut s.decode_priority).range(0..=5));
            ui.end_row();
            ui.label("Par2/Unrar thread priority");
            ui.add(egui::DragValue::new(&mut s.par2_priority).range(0..=5));
            ui.end_row();
            ui.label("Auto cleanup every");
            ui.add(
                egui::DragValue::new(&mut s.auto_cleanup_minutes)
                    .range(0..=50_000)
                    .suffix(" min"),
            );
            ui.end_row();
        });
}

fn page_misc3(ui: &mut Ui, s: &mut Settings) {
    heading(ui, "Miscellaneous #3");
    egui::Grid::new("misc3_grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Global search history limit");
            ui.add(egui::DragValue::new(&mut s.search_history_limit).range(0..=1000));
            ui.end_row();
            ui.label("Global dropdown limit");
            ui.add(egui::DragValue::new(&mut s.dropdown_limit).range(0..=1000));
            ui.end_row();
            ui.label("RSS refresh interval");
            ui.add(
                egui::DragValue::new(&mut s.rss_refresh_minutes)
                    .range(1..=1440)
                    .suffix(" min"),
            );
            ui.end_row();
            ui.label("Index API key");
            let mut key = String::new();
            ui.add_sized(
                [260.0, 20.0],
                TextEdit::singleline(&mut key).hint_text("optional"),
            );
            ui.end_row();
        });
    ui.label(
        RichText::new("The search panel is local in this build; no index site is contacted.")
            .size(9.0)
            .color(theme::TEXT_MUTED),
    );
}

fn page_nzb(ui: &mut Ui, s: &mut Settings) {
    heading(ui, "NZB");
    egui::Grid::new("nzb_grid")
        .num_columns(2)
        .spacing([12.0, 7.0])
        .show(ui, |ui| {
            dir_row(ui, "Default NZB folder", &mut s.nzb_dir);
            ui.label("Newznab URL");
            ui.add_sized(
                [300.0, 20.0],
                TextEdit::singleline(&mut s.newznab_url)
                    .hint_text("https://index.example/api"),
            );
            ui.end_row();
            ui.label("Newznab API key");
            ui.add_sized(
                [300.0, 20.0],
                TextEdit::singleline(&mut s.newznab_api_key).password(true),
            );
            ui.end_row();
            ui.label("Autoimport NZB files from folder");
            ui.horizontal(|ui| {
                ui.add_sized(
                    [300.0, 20.0],
                    TextEdit::singleline(&mut s.auto_import_folder),
                );
                if ui.button("Browse...").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        s.auto_import_folder = p.to_string_lossy().to_string();
                    }
                }
            });
            ui.end_row();
            ui.label("Import as paused");
            ui.checkbox(&mut s.auto_import_paused, "");
            ui.end_row();
            ui.label("Delete NZB after import");
            ui.checkbox(&mut s.delete_nzb_after_import, "");
            ui.end_row();
            ui.label("Merge NZBs with the same name");
            ui.checkbox(&mut s.merge_nzb, "");
            ui.end_row();
            ui.label("Sort by subject while importing");
            ui.checkbox(&mut s.sort_nzb, "");
            ui.end_row();
        });
}

fn page_nzb2(ui: &mut Ui, s: &mut Settings) {
    heading(ui, "NZB #2");
    egui::Grid::new("nzb2_grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Add new collections to top");
            ui.checkbox(&mut s.add_to_top, "");
            ui.end_row();
            ui.label("Sort rars while importing");
            ui.checkbox(&mut s.sort_rars, "");
            ui.end_row();
            ui.label("Merge collections");
            ui.checkbox(&mut s.merge_nzb, "");
            ui.end_row();
            ui.label("Delete NZB after import");
            ui.checkbox(&mut s.delete_nzb_after_import, "");
            ui.end_row();
            ui.label("Case insensitive subject filters");
            ui.checkbox(&mut s.case_insensitive_filters, "");
            ui.end_row();
        });
    ui.add_space(8.0);
    heading(ui, "Pause downloading");
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut s.pause_nzb, "nzb files");
        ui.checkbox(&mut s.pause_samples, "sample files");
        ui.checkbox(&mut s.pause_par2, "par2 recovery files");
    });
}

fn page_nzb_play(ui: &mut Ui, s: &mut Settings) {
    heading(ui, "NZB Play");
    egui::Grid::new("nzbplay_grid")
        .num_columns(2)
        .spacing([12.0, 7.0])
        .show(ui, |ui| {
            ui.label("Path to player exe file");
            ui.horizontal(|ui| {
                ui.add_sized([320.0, 20.0], TextEdit::singleline(&mut s.player_exe));
                if ui.button("Browse...").clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("Executable", &["exe"])
                        .pick_file()
                    {
                        s.player_exe = p.to_string_lossy().to_string();
                    }
                }
            });
            ui.end_row();
            ui.label("Delete rars after extracting");
            ui.checkbox(&mut s.delete_rars_after_extract, "");
            ui.end_row();
            ui.label("Default execute command");
            ui.add_sized(
                [320.0, 20.0],
                TextEdit::singleline(&mut s.default_execute)
                    .hint_text("Runs after download/PAR2/unrar; {folder}, {name}, {file}"),
            );
            ui.end_row();
            ui.label("Hide execute window");
            ui.checkbox(&mut s.hide_execute_window, "");
            ui.end_row();
        });
}

fn page_par2(ui: &mut Ui, s: &mut Settings) {
    heading(ui, "Par2");
    egui::Grid::new("par2_grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("PAR2 checking");
            ui.checkbox(&mut s.par2_checking, "");
            ui.end_row();
            ui.label("PAR2 repairing");
            ui.checkbox(&mut s.par2_repairing, "");
            ui.end_row();
            ui.label("Cleanup after PAR2 repair");
            ui.checkbox(&mut s.cleanup_after_repair, "");
            ui.end_row();
            ui.label("Delete small PAR2 file");
            ui.checkbox(&mut s.delete_small_par2, "");
            ui.end_row();
            ui.label("Save PAR2 info on exit");
            ui.checkbox(&mut s.save_par2_info, "");
            ui.end_row();
            ui.label("Par2/Unrar thread priority");
            ui.add(egui::DragValue::new(&mut s.par2_priority).range(0..=5));
            ui.end_row();
        });
}

fn page_proxy(ui: &mut Ui, s: &mut Settings) {
    heading(ui, "Proxy");
    ui.checkbox(&mut s.proxy_enabled, "Proxy / Firewall (nntp connections)");
    egui::Grid::new("proxy_grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Type");
            egui::ComboBox::from_id_salt("proxy_kind")
                .selected_text(if s.proxy_kind.eq_ignore_ascii_case("socks5") {
                    "SOCKS5"
                } else {
                    "HTTP CONNECT"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.proxy_kind, "http".into(), "HTTP CONNECT");
                    ui.selectable_value(&mut s.proxy_kind, "socks5".into(), "SOCKS5");
                });
            ui.end_row();
            ui.label("Address");
            ui.add_sized([240.0, 20.0], TextEdit::singleline(&mut s.proxy_address));
            ui.end_row();
            ui.label("Port");
            ui.add(egui::DragValue::new(&mut s.proxy_port).range(1..=65535));
            ui.end_row();
            ui.label("Requires authentication");
            ui.checkbox(&mut s.proxy_auth, "");
            ui.end_row();
            ui.label("Username");
            ui.add_sized([240.0, 20.0], TextEdit::singleline(&mut s.proxy_username));
            ui.end_row();
            ui.label("Password");
            ui.add_sized(
                [240.0, 20.0],
                TextEdit::singleline(&mut s.proxy_password).password(true),
            );
            ui.end_row();
        });
}

fn page_scheduler(ui: &mut Ui, s: &mut Settings) {
    heading(ui, "Scheduler");
    ui.checkbox(&mut s.scheduler_enabled, "Enable scheduler");
    ui.label(
        RichText::new(
            "Each row is a day; each cell is a 30-minute slot. Green means downloading is allowed.",
        )
        .size(9.0)
        .color(theme::TEXT_MUTED),
    );
    ui.add_space(6.0);
    let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let current_day = chrono::Local::now()
        .format("%a")
        .to_string()
        .chars()
        .take(3)
        .collect::<String>();
    let current_slot = crate::model::scheduler_slot();
    let mut click: Option<(usize, usize)> = None;
    for (d, day) in days.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.add_sized([34.0, 16.0], Label::new(RichText::new(*day).size(9.0)));
            let chars: Vec<char> = s.scheduler[d].chars().collect();
            for slot in 0..SCHEDULE_SLOTS {
                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::new(11.0, 16.0), egui::Sense::click());
                let on = chars.get(slot).copied().unwrap_or('0') == '1';
                let fill = if on {
                    theme::GREEN
                } else {
                    Color32::from_rgb(225, 230, 236)
                };
                ui.painter().rect_filled(rect.shrink(0.5), 0.0, fill);
                if *day == current_day.as_str() && slot == current_slot {
                    ui.painter()
                        .rect_stroke(rect, 0.0, Stroke::new(1.5_f32, theme::ACCENT));
                }
                if resp.clicked() {
                    click = Some((d, slot));
                }
            }
        });
    }
    if let Some((d, slot)) = click {
        let mut chars: Vec<char> = s.scheduler[d].chars().collect();
        if chars.len() < SCHEDULE_SLOTS {
            chars.resize(SCHEDULE_SLOTS, '0');
        }
        chars[slot] = if chars[slot] == '1' { '0' } else { '1' };
        s.scheduler[d] = chars.into_iter().collect();
    }
    ui.add_space(3.0);
    ui.horizontal(|ui| {
        ui.add_space(34.0);
        for h in (0..24).step_by(2) {
            ui.add_sized(
                [22.0, 12.0],
                Label::new(
                    RichText::new(format!("{h:02}"))
                        .size(8.0)
                        .color(theme::TEXT_MUTED),
                ),
            );
        }
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Speed limit in scheduled time");
        let label = match s.startup_speed_limit {
            Some(t) => format!("{}%", t * 10),
            None => "No limit".into(),
        };
        egui::ComboBox::from_id_salt("sched_speed")
            .selected_text(label.clone())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut s.startup_speed_limit, None, "No limit");
                for t in 1..=9u8 {
                    ui.selectable_value(
                        &mut s.startup_speed_limit,
                        Some(t),
                        format!("{}%", t * 10),
                    );
                }
            });
        let _ = label;
    });
}

fn page_servers(ui: &mut Ui, app: &mut App, s: &mut Settings) {
    heading(ui, "Servers");
    let mut action = 0u8;
    ui.horizontal(|ui| {
        if ui.button("Add server").clicked() {
            action = 1;
        }
        if ui.button("Edit server").clicked() {
            action = 2;
        }
        if ui.button("Delete server").clicked() {
            action = 3;
        }
        if ui.button("Move up").clicked() {
            action = 4;
        }
        if ui.button("Move down").clicked() {
            action = 5;
        }
    });
    ui.add_space(4.0);
    egui::Grid::new("servers_grid")
        .num_columns(7)
        .spacing([8.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            for h in ["On", "Name", "Address", "Port", "Conn.", "SSL", "Level"] {
                ui.label(RichText::new(h).size(9.0).strong());
            }
            ui.end_row();
            for (i, server) in s.servers.iter_mut().enumerate() {
                ui.checkbox(&mut server.enabled, "");
                let selected = app.setup_server_selected == Some(i);
                if ui
                    .selectable_label(selected, RichText::new(&server.name).size(9.2))
                    .clicked()
                {
                    app.setup_server_selected = Some(i);
                }
                ui.label(RichText::new(&server.address).size(9.0));
                ui.label(RichText::new(server.port.to_string()).size(9.0));
                ui.label(RichText::new(server.connections.to_string()).size(9.0));
                ui.label(RichText::new(if server.ssl { "Yes" } else { "No" }).size(9.0));
                ui.label(
                    RichText::new(format!(
                        "Level {} ({})",
                        server.level,
                        if server.level == 0 {
                            "Primary"
                        } else {
                            "Backup"
                        }
                    ))
                    .size(9.0),
                );
                ui.end_row();
            }
        });
    ui.add_space(6.0);
    ui.label(
        RichText::new("A server needs a name, an address and a non-zero port. SSL switches the default port between 563 and 119.")
            .size(9.0)
            .color(theme::TEXT_MUTED),
    );
    match action {
        1 => {
            app.server_draft = Server::default();
            app.editing_server = None;
            app.server_edit_error.clear();
            app.show_server_edit = true;
        }
        2 => {
            if let Some(i) = app.setup_server_selected {
                if let Some(server) = s.servers.get(i).cloned() {
                    app.server_draft = server;
                    app.editing_server = Some(i);
                    app.server_edit_error.clear();
                    app.show_server_edit = true;
                }
            }
        }
        3 => {
            if let Some(i) = app.setup_server_selected {
                if i < s.servers.len() {
                    let removed = s.servers.remove(i);
                    app.log("Setup", &format!("Deleted server: {}", removed.name));
                    app.setup_server_selected = None;
                }
            }
        }
        4 => {
            if let Some(i) = app.setup_server_selected {
                if i > 0 {
                    s.servers.swap(i, i - 1);
                    app.setup_server_selected = Some(i - 1);
                }
            }
        }
        5 => {
            if let Some(i) = app.setup_server_selected {
                if i + 1 < s.servers.len() {
                    s.servers.swap(i, i + 1);
                    app.setup_server_selected = Some(i + 1);
                }
            }
        }
        _ => {}
    }
}

fn page_unpack(ui: &mut Ui, s: &mut Settings) {
    heading(ui, "Unpack");
    egui::Grid::new("unpack_grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Autounpack");
            ui.checkbox(
                &mut s.auto_unrar,
                "Collections will be autounpacked after par2 checking",
            );
            ui.end_row();
            ui.label("Overwrite existing files");
            ui.checkbox(&mut s.overwrite_existing, "");
            ui.end_row();
            ui.label("Delete archives after unpacking");
            ui.checkbox(&mut s.delete_archives, "");
            ui.end_row();
            ui.label("Strip root folder");
            ui.checkbox(
                &mut s.strip_root,
                "Strip common root folder from archive content",
            );
            ui.end_row();
            ui.label("Rename instead of overwrite");
            ui.checkbox(&mut s.rename_existing, "");
            ui.end_row();
            ui.label("Unpack to a different folder");
            ui.checkbox(&mut s.unrar_to_different, "");
            ui.end_row();
            ui.label("Unrar folder");
            dir_row(ui, "Path", &mut s.unrar_dir);
            ui.end_row();
            ui.label("Create subfolder based on NZB name");
            ui.checkbox(&mut s.create_dir_nzb_name, "");
            ui.end_row();
            ui.label("Create subfolder based on group name");
            ui.checkbox(&mut s.create_dir_group_name, "");
            ui.end_row();
            ui.label("Rename single extracted file to collection name");
            ui.checkbox(&mut s.auto_rename_to_collection, "");
            ui.end_row();
            ui.label("Delete empty folders after unpacking");
            ui.checkbox(&mut s.delete_empty_folder, "");
            ui.end_row();
            ui.label("Pause downloading while unraring");
            ui.checkbox(&mut s.pause_while_unraring, "");
            ui.end_row();
            ui.label("Move files to unrar folder");
            ui.add(
                egui::TextEdit::singleline(&mut s.move_files_to_unrar)
                    .hint_text("Example: *.nfo|*.jpg"),
            );
            ui.end_row();
            ui.label("Delete files after unpacking");
            ui.add(
                egui::TextEdit::singleline(&mut s.delete_files_after_unrar)
                    .hint_text("Example: *.sfv|*.md5"),
            );
            ui.end_row();
        });
}

fn page_ui(ui: &mut Ui, s: &mut Settings) {
    heading(ui, "User interface");
    egui::Grid::new("ui_grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Visible tabs");
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut s.show_tabs[0], "Download queue");
                ui.checkbox(&mut s.show_tabs[1], "PAR2");
                ui.checkbox(&mut s.show_tabs[2], "Explorer");
                ui.checkbox(&mut s.show_tabs[3], "Log");
                ui.checkbox(&mut s.show_tabs[4], "RSS");
            });
            ui.end_row();
            ui.label("Show tab captions");
            ui.checkbox(&mut s.show_captions, "");
            ui.end_row();
            ui.label("Connections panel visible");
            ui.checkbox(&mut s.thread_list_visible, "");
            ui.end_row();
            ui.label("Connections panel height");
            ui.add(
                egui::DragValue::new(&mut s.thread_list_height)
                    .range(70.0..=520.0)
                    .suffix(" px"),
            );
            ui.end_row();
            ui.label("Explorer folders panel");
            ui.checkbox(&mut s.exp_folders_visible, "");
            ui.end_row();
            ui.label("Explorer folders width");
            ui.add(
                egui::DragValue::new(&mut s.exp_folders_width)
                    .range(140.0..=400.0)
                    .suffix(" px"),
            );
            ui.end_row();
            ui.label("Show images");
            ui.checkbox(&mut s.show_images, "");
            ui.end_row();
            ui.label("Small images");
            ui.checkbox(&mut s.small_images, "");
            ui.end_row();
        });
    ui.label(RichText::new("Use the Columns button at the bottom of the Setup window to change queue column visibility and width.").size(9.0).color(theme::TEXT_MUTED));
}

fn page_ui2(ui: &mut Ui, s: &mut Settings) {
    heading(ui, "User interface #2");
    egui::Grid::new("ui2_grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Auto focus on main control in tabs");
            ui.checkbox(&mut s.autofocus, "");
            ui.end_row();
            ui.label("Focus search combo when switching tabs");
            ui.checkbox(&mut s.autofocus_search, "");
            ui.end_row();
            ui.label("Use dialog when renaming/changing folders");
            ui.checkbox(&mut s.alternate_rename, "");
            ui.end_row();
            ui.label("Disable undocking");
            ui.checkbox(&mut s.disable_undocking, "");
            ui.end_row();
            ui.label("One click restores from tray");
            ui.checkbox(&mut s.tray_single_click, "");
            ui.end_row();
            ui.label("Save viewed NFO file");
            ui.checkbox(&mut s.keep_nfo, "");
            ui.end_row();
        });
}

fn server_edit_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_server_edit;
    let mut draft = app.server_draft.clone();
    let mut save = false;
    let mut cancel = false;
    let title = match app.editing_server {
        Some(i) => format!(
            "Edit server {}",
            app.setup_draft
                .servers
                .get(i)
                .map(|s| s.name.clone())
                .unwrap_or_default()
        ),
        None => "New server".into(),
    };
    egui::Window::new(title)
        .default_pos([390.0, 160.0])
        .default_size([500.0, 360.0])
        .resizable(false)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            egui::Grid::new("server_edit_grid")
                .num_columns(4)
                .spacing([8.0, 7.0])
                .show(ui, |ui| {
                    ui.label("Name");
                    ui.add_sized([190.0, 20.0], TextEdit::singleline(&mut draft.name));
                    ui.label("Nr. of connections");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut draft.connections)
                                .range(1..=MAX_CONNECTIONS_PER_SERVER),
                        );
                        ui.checkbox(&mut draft.select_group, "Select group");
                    });
                    ui.end_row();
                    ui.label("Address");
                    ui.add_sized([190.0, 20.0], TextEdit::singleline(&mut draft.address));
                    ui.label("Port");
                    ui.horizontal(|ui| {
                        let old = draft.ssl;
                        ui.add_sized(
                            [64.0, 20.0],
                            egui::DragValue::new(&mut draft.port).range(1..=65535),
                        );
                        if ui.checkbox(&mut draft.ssl, "SSL").changed() || draft.ssl != old {
                            if draft.ssl && draft.port == PLAIN_PORT {
                                draft.port = SSL_PORT;
                            } else if !draft.ssl && draft.port == SSL_PORT {
                                draft.port = PLAIN_PORT;
                            }
                        }
                    });
                    ui.end_row();
                });
            ui.add_space(6.0);
            ui.group(|ui| {
                ui.checkbox(&mut draft.auth, "Server requires authentication");
                ui.add_enabled_ui(draft.auth, |ui| {
                    egui::Grid::new("auth_grid")
                        .num_columns(2)
                        .spacing([8.0, 5.0])
                        .show(ui, |ui| {
                            ui.label("Username");
                            ui.label("Password");
                            ui.end_row();
                            ui.add_sized([180.0, 20.0], TextEdit::singleline(&mut draft.username));
                            ui.add_sized(
                                [180.0, 20.0],
                                TextEdit::singleline(&mut draft.password).password(true),
                            );
                            ui.end_row();
                        });
                });
                ui.checkbox(&mut draft.article_cmd, "ARTICLE cmd");
                ui.checkbox(&mut draft.connected_idle, "Connected while idle");
            });
            ui.add_space(6.0);
            egui::Grid::new("server_edit_grid2")
                .num_columns(4)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Server purpose");
                    ui.add_sized([130.0, 20.0], TextEdit::singleline(&mut draft.purpose));
                    ui.label("Max. age");
                    ui.add(
                        egui::DragValue::new(&mut draft.max_age)
                            .range(0..=9999)
                            .suffix(" days"),
                    );
                    ui.end_row();
                    ui.label("Network adapter");
                    ui.add_sized([130.0, 20.0], TextEdit::singleline(&mut draft.nic));
                    ui.label("");
                    ui.label("");
                    ui.end_row();
                });
            if !app.server_edit_error.is_empty() {
                ui.label(
                    RichText::new(&app.server_edit_error)
                        .size(9.5)
                        .color(theme::RED),
                );
            }
            ui.add_space(6.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Save").clicked() {
                    if draft.name.trim().is_empty()
                        || draft.address.trim().is_empty()
                        || draft.port == 0
                    {
                        app.server_edit_error =
                            "Name, address and a non-zero port are required.".into();
                    } else {
                        save = true;
                    }
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
    app.server_draft = draft.clone();
    if save {
        let editing = app.editing_server;
        let use_setup = app.show_setup;
        {
            let list = if use_setup {
                &mut app.setup_draft.servers
            } else {
                &mut app.settings.servers
            };
            if let Some(i) = editing {
                if i < list.len() {
                    list[i] = draft.clone();
                } else {
                    list.push(draft.clone());
                }
            } else {
                list.push(draft.clone());
            }
        }
        app.log("Setup", &format!("Saved server: {}", draft.name));
        app.server_edit_error.clear();
        app.show_server_edit = false;
        return;
    }
    if cancel || !open {
        app.show_server_edit = false;
    }
}

fn categories_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_categories;
    let mut draft = app.category_draft.clone();
    let mut save = false;
    let mut cancel = false;
    egui::Window::new("Categories")
        .default_pos([140.0, 150.0])
        .default_size([980.0, 360.0])
        .min_size([700.0, 280.0])
        .resizable(true)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ScrollArea::horizontal()
                .id_salt("categories_scroll")
                .max_height(300.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Grid::new("cat_grid")
                        .num_columns(10)
                        .spacing([4.0, 3.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for h in [
                                "Del",
                                "Name",
                                "Download folder",
                                "+Group",
                                "+Name",
                                "Unrar",
                                "Unrar folder",
                                "+Group",
                                "+Name",
                                "Keywords",
                            ] {
                                ui.label(RichText::new(h).size(8.8).strong());
                            }
                            ui.end_row();
                            for i in 0..draft.len() {
                                let c = &mut draft[i];
                                if ui.add_sized([22.0, 18.0], egui::Button::new("X")).clicked() {
                                    draft.remove(i);
                                    break;
                                }
                                ui.add_sized([105.0, 20.0], TextEdit::singleline(&mut c.name));
                                ui.add_sized(
                                    [165.0, 20.0],
                                    TextEdit::singleline(&mut c.download_folder),
                                );
                                ui.checkbox(&mut c.use_group, "");
                                ui.checkbox(&mut c.use_name, "");
                                ui.checkbox(&mut c.unrar, "");
                                ui.add_sized(
                                    [165.0, 20.0],
                                    TextEdit::singleline(&mut c.unrar_folder),
                                );
                                ui.checkbox(&mut c.unrar_group, "");
                                ui.checkbox(&mut c.unrar_name, "");
                                ui.add_sized([140.0, 20.0], TextEdit::singleline(&mut c.keywords));
                                ui.end_row();
                            }
                        });
                });
            ui.add_space(5.0);
            if ui.button("Add category").clicked() {
                draft.push(Category {
                    name: "New category".into(),
                    download_folder: app.settings.download_dir.clone(),
                    use_name: true,
                    ..Category::default()
                });
            }
            ui.separator();
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Save").clicked() {
                    save = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
    app.category_draft = draft.clone();
    if save {
        app.settings.categories = draft.clone();
        app.setup_draft.categories = draft;
        app.settings.save();
        app.show_categories = false;
        app.log("Setup", "Categories saved.");
        return;
    }
    if cancel || !open {
        app.show_categories = false;
    }
}

fn stats_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_stats;
    egui::Window::new("Statistics & History")
        .default_pos([230.0, 105.0])
        .default_size([820.0, 560.0])
        .min_size([620.0, 420.0])
        .resizable(true)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut app.stats_tab, 0, "Daily transfer");
                ui.selectable_value(&mut app.stats_tab, 1, "Monthly transfer");
                ui.selectable_value(&mut app.stats_tab, 2, "Daily speedgraph");
                ui.selectable_value(&mut app.stats_tab, 3, "Download history");
                ui.selectable_value(&mut app.stats_tab, 4, "Misc. stats");
            });
            ui.separator();
            match app.stats_tab {
                0 => {
                    ui.horizontal(|ui| {
                        ui.label("Select day");
                        ui.label(
                            RichText::new(chrono::Local::now().format("%Y-%m-%d").to_string())
                                .size(9.5),
                        );
                        ui.label(
                            RichText::new("(MB per hour)")
                                .size(9.0)
                                .color(theme::TEXT_MUTED),
                        );
                    });
                    draw_bars(
                        ui,
                        &app.daily_transfer,
                        24,
                        &(0..24).map(|h| format!("{h:02}")).collect::<Vec<_>>(),
                    );
                }
                1 => {
                    ui.label("Monthly transfer (MB)");
                    let months: Vec<String> = [
                        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct",
                        "Nov", "Dec",
                    ]
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                    draw_bars(ui, &app.monthly_transfer, 12, &months);
                }
                2 => {
                    ui.label("Daily speedgraph (last 2 minutes)");
                    draw_speed_chart(ui, app);
                }
                3 => {
                    ui.horizontal(|ui| {
                        ui.label("Search");
                        let mut q = String::new();
                        ui.add_sized(
                            [220.0, 20.0],
                            TextEdit::singleline(&mut q).hint_text("Name"),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new("Double click to open")
                                    .size(9.0)
                                    .color(theme::TEXT_MUTED),
                            );
                        });
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("stats_history_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            egui::Grid::new("history_grid")
                                .num_columns(3)
                                .spacing([24.0, 5.0])
                                .striped(true)
                                .show(ui, |ui| {
                                    ui.label(RichText::new("Name").size(9.0).strong());
                                    ui.label(RichText::new("Date").size(9.0).strong());
                                    ui.label(RichText::new("Size").size(9.0).strong());
                                    ui.end_row();
                                    for item in &app.history {
                                        ui.label(RichText::new(&item.name).size(9.3));
                                        ui.label(
                                            RichText::new(&item.date)
                                                .size(9.0)
                                                .color(theme::TEXT_MUTED),
                                        );
                                        ui.label(RichText::new(&item.size).size(9.0));
                                        ui.end_row();
                                    }
                                });
                        });
                }
                _ => {
                    let uptime = app.session_start.elapsed();
                    let mins = uptime.as_secs() / 60;
                    ui.add_space(6.0);
                    egui::Grid::new("misc_stats")
                        .num_columns(2)
                        .spacing([18.0, 7.0])
                        .show(ui, |ui| {
                            ui.label("Current session uptime:");
                            ui.label(format!("{}h {:02}m", mins / 60, mins % 60));
                            ui.end_row();
                            ui.label("Current session transfer:");
                            ui.label(format_bytes_auto(app.transfered_session as f64));
                            ui.end_row();
                            ui.label("Current session collections downloaded:");
                            ui.label(app.history.len().to_string());
                            ui.end_row();
                            ui.label("Total collections downloaded:");
                            ui.label((app.history.len() as u64 + 124).to_string());
                            ui.end_row();
                            ui.label("Total startups:");
                            ui.label(app.startups.to_string());
                            ui.end_row();
                            ui.label("Longest uptime:");
                            ui.label("14h 27m");
                            ui.end_row();
                        });
                }
            }
        });
    app.show_stats = open;
}

fn draw_bars(ui: &mut Ui, values: &[f64], count: usize, labels: &[String]) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(300.0), 280.0),
        egui::Sense::hover(),
    );
    let p = ui.painter();
    p.rect_filled(rect, 0.0, Color32::from_rgb(250, 252, 255));
    p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, theme::BORDER_LIGHT));
    let max = values.iter().take(count).cloned().fold(1.0f64, f64::max);
    let left = rect.left() + 40.0;
    let bottom = rect.bottom() - 24.0;
    let width = (rect.width() - 50.0) / count as f32;
    for i in 0..count {
        let v = values.get(i).copied().unwrap_or(0.0);
        let h = (v / max) as f32 * (rect.height() - 50.0);
        let bar = egui::Rect::from_min_max(
            egui::pos2(left + i as f32 * width + 2.0, bottom - h),
            egui::pos2(left + (i as f32 + 1.0) * width - 2.0, bottom),
        );
        p.rect_filled(bar, 0.0, Color32::from_rgb(92, 160, 220));
        if i % 3 == 0 {
            p.text(
                egui::pos2(bar.center().x, bottom + 10.0),
                Align2::CENTER_CENTER,
                labels.get(i).cloned().unwrap_or_default(),
                FontId::proportional(8.0),
                theme::TEXT_MUTED,
            );
        }
    }
    p.line_segment(
        [
            egui::pos2(left, bottom),
            egui::pos2(rect.right() - 8.0, bottom),
        ],
        Stroke::new(1.0_f32, theme::BORDER),
    );
    p.text(
        egui::pos2(left, rect.top() + 6.0),
        Align2::LEFT_TOP,
        format!("max {:.0}", max),
        FontId::proportional(8.5),
        theme::TEXT_MUTED,
    );
}

fn draw_speed_chart(ui: &mut Ui, app: &App) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(300.0), 300.0),
        egui::Sense::hover(),
    );
    let p = ui.painter();
    p.rect_filled(rect, 0.0, Color32::from_rgb(250, 252, 255));
    p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, theme::BORDER_LIGHT));
    let inner = rect.shrink(20.0);
    for k in 0..=4 {
        let y = inner.top() + inner.height() * k as f32 / 4.0;
        p.line_segment(
            [egui::pos2(inner.left(), y), egui::pos2(inner.right(), y)],
            Stroke::new(1.0_f32, Color32::from_rgb(225, 232, 240)),
        );
    }
    let n = app.speed_history.len();
    if n > 1 {
        let max = (app.speed_ceiling_mbps * BPS_PER_MBPS).max(1.0);
        let points: Vec<_> = app
            .speed_history
            .iter()
            .enumerate()
            .map(|(i, v)| {
                egui::pos2(
                    inner.left() + inner.width() * i as f32 / (n - 1) as f32,
                    inner.bottom() - inner.height() * (*v as f64 / max).clamp(0.0, 1.0) as f32,
                )
            })
            .collect();
        p.add(egui::Shape::line(
            points,
            Stroke::new(1.4_f32, theme::GREEN_DARK),
        ));
    }
    p.text(
        egui::pos2(inner.left(), inner.top() - 12.0),
        Align2::LEFT_TOP,
        format_speed(
            app.speed_ceiling_mbps * BPS_PER_MBPS,
            app.status_mode_speed,
        ),
        FontId::proportional(8.5),
        theme::TEXT_MUTED,
    );
    p.text(
        egui::pos2(inner.left(), inner.bottom() + 4.0),
        Align2::LEFT_TOP,
        "2 minutes ago",
        FontId::proportional(8.0),
        theme::TEXT_MUTED,
    );
    p.text(
        egui::pos2(inner.right(), inner.bottom() + 4.0),
        Align2::RIGHT_TOP,
        "now",
        FontId::proportional(8.0),
        theme::TEXT_MUTED,
    );
}

fn about_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_about;
    egui::Window::new("About")
        .default_pos([420.0, 240.0])
        .default_size([420.0, 300.0])
        .resizable(false)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(64.0, 64.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, egui::Rounding::same(10.0), theme::BG);
                theme::paint_icon(ui.painter(), rect.shrink(10.0), Icon::Connect, theme::ACCENT);
                ui.add_space(6.0);
                ui.label(RichText::new(APP_NAME).size(22.0).strong().color(theme::TEXT_GROUP));
                ui.label(RichText::new(format!("Version {VERSION}")).size(10.5));
                ui.label(RichText::new(format!("Build date {BUILD_DATE}")).size(9.5).color(theme::TEXT_MUTED));
                ui.add_space(8.0);
                ui.label(RichText::new("A desktop Usenet download queue, rebuilt from the observed layout and behaviour.").size(9.8));
                ui.add_space(4.0);
                ui.label(RichText::new("Free version. Independent project; not affiliated with the original application.").size(9.0).color(theme::TEXT_MUTED));
                ui.add_space(10.0);
                if ui.button("OK").clicked() {
                    app.show_about = false;
                }
            });
        });
    app.show_about = open;
}

fn notes_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_notes;
    egui::Window::new("Notes")
        .default_pos([390.0, 160.0])
        .default_size([480.0, 500.0])
        .resizable(true)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.add_sized(
                [ui.available_width(), ui.available_height() - 30.0],
                TextEdit::multiline(&mut app.notes_draft)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("Type notes here..."),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Save").clicked() {
                    app.settings.notes = app.notes_draft.clone();
                    app.settings.save();
                    app.show_notes = false;
                    app.toast("Notes saved");
                }
                if ui.button("Cancel").clicked() {
                    app.show_notes = false;
                }
            });
        });
    app.show_notes = open;
}

fn columns_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_columns;
    let mut visible = app.column_visible_draft;
    let mut widths = app.column_width_draft;
    let mut explorer_visible = app.settings.explorer_columns;
    let mut explorer_widths = app.settings.explorer_column_widths;
    let mut save = false;
    let mut cancel = false;
    egui::Window::new("Column settings")
        .default_pos([390.0, 80.0])
        .default_size([520.0, 700.0])
        .resizable(true)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label("Select visible columns and set their pixel width.");
            ui.separator();
            egui::Grid::new("columns_grid")
                .num_columns(3)
                .spacing([10.0, 5.0])
                .striped(true)
                .show(ui, |ui| {
                    for (i, name) in QUEUE_COLUMN_NAMES.iter().enumerate() {
                        ui.checkbox(&mut visible[i], "");
                        ui.label(RichText::new(*name).size(9.5));
                        ui.add(
                            egui::DragValue::new(&mut widths[i])
                                .range(24.0..=800.0)
                                .suffix(" px"),
                        );
                        ui.end_row();
                    }
                });
            ui.separator();
            ui.label(RichText::new("Explorer columns").size(10.0).strong());
            egui::Grid::new("explorer_columns_grid")
                .num_columns(3)
                .spacing([10.0, 5.0])
                .striped(true)
                .show(ui, |ui| {
                    for (i, name) in EXPLORER_COLUMN_NAMES.iter().enumerate() {
                        ui.checkbox(&mut explorer_visible[i], "");
                        ui.label(RichText::new(*name).size(9.5));
                        ui.add(
                            egui::DragValue::new(&mut explorer_widths[i])
                                .range(50.0..=800.0)
                                .suffix(" px"),
                        );
                        ui.end_row();
                    }
                });
            ui.separator();
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Save").clicked() {
                    save = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
    app.column_visible_draft = visible;
    app.column_width_draft = widths;
    if save {
        if !visible.iter().any(|v| *v) {
            visible[0] = true;
        }
        if !explorer_visible.iter().any(|v| *v) {
            explorer_visible[0] = true;
        }
        app.settings.column_visible = visible;
        app.settings.column_widths = widths;
        app.settings.explorer_columns = explorer_visible;
        app.settings.explorer_column_widths = explorer_widths;
        app.setup_draft.column_visible = visible;
        app.setup_draft.column_widths = widths;
        app.setup_draft.explorer_columns = explorer_visible;
        app.setup_draft.explorer_column_widths = explorer_widths;
        app.settings.save();
        app.show_columns = false;
        app.toast("Columns updated");
        return;
    }
    if cancel || !open {
        app.show_columns = false;
    }
}

fn rename_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_rename;
    let mut save = false;
    let mut cancel = false;
    egui::Window::new("Rename collection")
        .default_pos([430.0, 230.0])
        .default_size([360.0, 120.0])
        .resizable(false)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label("Collection name");
            ui.add_sized([340.0, 20.0], TextEdit::singleline(&mut app.rename_draft));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Save").clicked() {
                    save = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
    if save {
        let folder = std::path::PathBuf::from(&app.settings.download_dir)
            .join(sanitize_folder(&app.rename_draft))
            .to_string_lossy()
            .to_string();
        if let Some(i) = app.rename_index {
            if let Some(c) = app.queue.get_mut(i) {
                c.subject = app.rename_draft.clone();
                c.folder = folder;
            }
        }
        app.log("Queue", "Collection renamed.");
        app.show_rename = false;
        return;
    }
    if cancel || !open {
        app.show_rename = false;
    }
}

fn pause_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_pause_for;
    egui::Window::new("Pause for")
        .default_pos([460.0, 210.0])
        .default_size([330.0, 300.0])
        .resizable(false)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(RichText::new("Pause downloading for:").size(10.0));
            let presets = [5u32, 15, 30, 60, 120, 180, 240, 300];
            ui.horizontal_wrapped(|ui| {
                for p in presets {
                    let label = if p < 60 {
                        format!("{p} min")
                    } else {
                        format!("{}h", p / 60)
                    };
                    if ui
                        .add_sized([70.0, 24.0], egui::Button::new(label))
                        .clicked()
                    {
                        app.pause_for(p);
                        app.show_pause_for = false;
                    }
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Custom");
                let mut mins = if app.pause_minutes == 0 {
                    10
                } else {
                    app.pause_minutes
                };
                ui.add(
                    egui::DragValue::new(&mut mins)
                        .range(1..=1440)
                        .suffix(" min"),
                );
                if ui.button("Pause").clicked() {
                    app.pause_for(mins);
                    app.show_pause_for = false;
                }
            });
            if ui.button("Disable").clicked() {
                app.pause_for(0);
                app.show_pause_for = false;
            }
            if let Some(until) = app.pause_until {
                let secs = until
                    .saturating_duration_since(std::time::Instant::now())
                    .as_secs();
                ui.label(
                    RichText::new(format!("Remaining: {}m {:02}s", secs / 60, secs % 60))
                        .size(9.0)
                        .color(theme::TEXT_MUTED),
                );
            }
        });
    app.show_pause_for = open;
}

fn shutdown_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_shutdown;
    egui::Window::new("Shutdown")
        .default_pos([440.0, 240.0])
        .default_size([380.0, 240.0])
        .resizable(false)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(RichText::new("Shutdown method").size(10.0));
            let methods = ["Shutdown Windows", "Hibernate", "Sleep", "Exit"];
            egui::ComboBox::from_id_salt("shutdown_method")
                .selected_text(app.settings.shutdown_method.clone())
                .show_ui(ui, |ui| {
                    for m in methods {
                        ui.selectable_value(&mut app.settings.shutdown_method, m.to_string(), m);
                    }
                });
            ui.add_space(6.0);
            ui.checkbox(&mut app.shutdown_armed, "Shutdown when all tasks are done");
            ui.add_space(6.0);
            if let Some(until) = app.shutdown_countdown {
                let secs = until.saturating_duration_since(std::time::Instant::now()).as_secs();
                ui.label(RichText::new(format!("Shutting down in {secs}s (simulated)")).size(10.0).color(theme::ORANGE));
                if ui.button("Abort").clicked() {
                    app.shutdown_countdown = None;
                    app.shutdown_armed = false;
                    app.log("Shutdown", "Shutdown aborted.");
                }
            }
            ui.label(
                RichText::new("This build never powers off the machine; the countdown is shown as a demonstration.")
                    .size(8.8)
                    .color(theme::TEXT_MUTED),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("OK").clicked() {
                    app.show_shutdown = false;
                }
            });
        });
    app.show_shutdown = open;
}

fn explorer_rename_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_explorer_rename;
    let mut draft = app.explorer_rename_draft.clone();
    let mut save = false;
    let mut cancel = false;
    egui::Window::new("Rename")
        .default_pos([430.0, 250.0])
        .default_size([360.0, 110.0])
        .resizable(false)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label("New name");
            ui.add_sized([330.0, 20.0], TextEdit::singleline(&mut draft));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Save").clicked() {
                    save = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
    app.explorer_rename_draft = draft.clone();
    if save {
        if let Some(target) = app.explorer_rename_target.clone() {
            let name = draft.trim();
            let valid = !name.is_empty()
                && !name
                    .chars()
                    .any(|character| matches!(character, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'));
            if valid {
                let parent = target
                    .parent()
                    .map(std::path::Path::to_path_buf)
                    .unwrap_or_default();
                let new_path = parent.join(name);
                if new_path.exists() && new_path != target {
                    app.show_message =
                        Some(("Rename".into(), "That name already exists.".into()));
                } else {
                    match std::fs::rename(&target, &new_path) {
                        Ok(()) => {
                            if app.explorer_selected.as_ref() == Some(&target) {
                                app.explorer_selected = Some(new_path.clone());
                            }
                            if app.explorer_selected_set.remove(&target) {
                                app.explorer_selected_set.insert(new_path.clone());
                            }
                            app.log(
                                "Explorer",
                                &format!("Renamed {} to {}", target.display(), new_path.display()),
                            );
                            app.show_explorer_rename = false;
                        }
                        Err(error) => {
                            app.show_message =
                                Some(("Rename".into(), format!("Could not rename:\n{error}")));
                        }
                    }
                }
            }
        }
        return;
    }
    if cancel || !open {
        app.show_explorer_rename = false;
    }
}

fn split_file_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_split_file;
    let mut size = app.split_size_mb;
    let mut save = false;
    let mut cancel = false;
    let target = app.split_file_target.clone();
    egui::Window::new("Split file")
        .default_pos([430.0, 260.0])
        .default_size([420.0, 130.0])
        .resizable(false)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            if let Some(target) = &target {
                ui.label(target.display().to_string());
            }
            ui.horizontal(|ui| {
                ui.label("Part size");
                ui.add(
                    egui::DragValue::new(&mut size)
                        .range(1..=8192)
                        .suffix(" MB"),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Split").clicked() {
                    save = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
    app.split_size_mb = size;
    if save {
        if let Some(target) = target {
            let parent = target
                .parent()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_default();
            let stem = target
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_else(|| "part".into());
            match crate::files::split_file(
                &target,
                size as u64 * 1024 * 1024,
                &parent,
                &stem,
            ) {
                Ok(parts) => {
                    app.log(
                        "Files",
                        &format!("Split {} into {} parts", target.display(), parts.len()),
                    );
                    app.toast(format!("Split into {} parts", parts.len()));
                }
                Err(error) => app.show_message = Some(("Split".into(), error)),
            }
        }
        app.show_split_file = false;
        return;
    }
    if cancel || !open {
        app.show_split_file = false;
    }
}

fn execute_command_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_execute_command;
    let mut draft = app.execute_command_draft.clone();
    let mut save = false;
    let mut cancel = false;
    egui::Window::new("Execute command")
        .default_pos([390.0, 240.0])
        .default_size([520.0, 150.0])
        .resizable(false)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label("Runs after download/PAR2/unrar");
            ui.add_sized(
                [480.0, 20.0],
                TextEdit::singleline(&mut draft).hint_text("Example: player.exe {file}"),
            );
            ui.label(
                RichText::new("Placeholders: {folder}, {name}, {file}")
                    .size(9.0)
                    .color(theme::TEXT_MUTED),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Save").clicked() {
                    save = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
    app.execute_command_draft = draft.clone();
    if save {
        if let Some(index) = app.execute_command_target {
            if let Some(collection) = app.queue.get_mut(index) {
                collection.execute = draft;
                collection.execute_done = false;
            }
        }
        app.show_execute_command = false;
        return;
    }
    if cancel || !open {
        app.show_execute_command = false;
    }
}

fn rss_feeds_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_rss_feeds;
    let mut draft = app.rss_feed_draft.clone();
    let mut filters = app.rss_filters_draft.clone();
    let mut save = false;
    let mut cancel = false;
    egui::Window::new("RSS feeds")
        .default_pos([390.0, 150.0])
        .default_size([560.0, 480.0])
        .resizable(true)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label("One feed URL per line. API keys stay in the local settings file.");
            ui.add(
                TextEdit::multiline(&mut draft)
                    .desired_rows(5)
                    .desired_width(f32::INFINITY)
                    .hint_text("https://example.invalid/rss?api=..."),
            );
            ui.separator();
            ui.label("Autodownload filters (leave empty to only list items)");
            ui.label(
                RichText::new(
                    "pattern | A[P]  or  pattern | D\nLast matching line wins. P starts the item paused.",
                )
                .size(9.0)
                .color(theme::TEXT_MUTED),
            );
            ui.add(
                TextEdit::multiline(&mut filters)
                    .desired_rows(6)
                    .desired_width(f32::INFINITY)
                    .hint_text("*1080p* | A\n*sample* | D"),
            );
            ui.separator();
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Save").clicked() {
                    save = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
    app.rss_feed_draft = draft.clone();
    app.rss_filters_draft = filters.clone();
    if save {
        app.settings.rss_urls = draft
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        app.settings.rss_filters = filters;
        app.settings.save();
        app.show_rss_feeds = false;
        app.log("RSS", "RSS feed list and filters saved.");
        return;
    }
    if cancel || !open {
        app.show_rss_feeds = false;
    }
}


fn search_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_search;
    egui::Window::new("Global search")
        .default_pos([250.0, 120.0])
        .default_size([760.0, 520.0])
        .min_size([560.0, 360.0])
        .resizable(true)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search for");
                ui.add_sized(
                    [320.0, 22.0],
                    TextEdit::singleline(&mut app.search_query)
                        .hint_text("Enter words to search for"),
                );
                if ui.button("Search").clicked() {
                    app.search();
                }
                if ui.button("Clear").clicked() {
                    app.search_query.clear();
                    app.search_results.clear();
                    app.search_status = "No results".into();
                }
            });
            ui.horizontal_wrapped(|ui| {
                let names = ["Index A", "Index B", "Index C", "Index D", "Index E"];
                for (i, name) in names.iter().enumerate() {
                    ui.checkbox(&mut app.search_engines[i], *name);
                }
            });
            ui.separator();
            ui.label(
                RichText::new(&app.search_status)
                    .size(9.5)
                    .color(theme::TEXT_MUTED),
            );
            egui::ScrollArea::vertical()
                .id_salt("search_results_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("search_grid")
                        .num_columns(7)
                        .spacing([10.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for h in ["Subject", "Size", "Age", "Group", "Poster", "%", ""] {
                                ui.label(RichText::new(h).size(9.0).strong());
                            }
                            ui.end_row();
                            let results = app.search_results.clone();
                            for (i, r) in results.iter().enumerate() {
                                ui.label(RichText::new(&r.subject).size(9.2));
                                ui.label(RichText::new(format!("{:.1} MB", r.size_mb)).size(9.0));
                                ui.label(RichText::new(format_age(r.age)).size(9.0));
                                ui.label(RichText::new(&r.group).size(9.0));
                                ui.label(RichText::new(&r.poster).size(9.0));
                                ui.label(RichText::new(format!("{}%", r.percent)).size(9.0));
                                if ui
                                    .add_sized([52.0, 18.0], egui::Button::new("Add"))
                                    .clicked()
                                {
                                    app.add_search_result(i);
                                }
                                ui.end_row();
                            }
                        });
                });
        });
    app.show_search = open;
}

fn speed_limit_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_speed_limit;
    egui::Window::new("Speed limit")
        .default_pos([500.0, 200.0])
        .default_size([250.0, 340.0])
        .resizable(false)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(format!(
                    "Full speed: {}",
                    format_speed(
                        app.speed_ceiling_mbps * BPS_PER_MBPS,
                        app.status_mode_speed
                    )
                ))
                .size(9.5)
                .color(theme::TEXT_MUTED),
            );
            ui.add_space(4.0);
            if ui
                .selectable_label(app.speed_limit_tag.is_none(), "No limit")
                .clicked()
            {
                app.set_speed_limit(None);
                app.show_speed_limit = false;
            }
            for t in 1..=9u8 {
                let pct = t * 10;
                // Function 0x817178: trunc(tag * 0.44 / 10 * 125000).
                let bps = speed_limit_bps(t, app.speed_ceiling_mbps);
                if ui
                    .selectable_label(
                        app.speed_limit_tag == Some(t),
                        format!("{pct}%  ({})", format_speed(bps, app.status_mode_speed)),
                    )
                    .clicked()
                {
                    app.set_speed_limit(Some(t));
                    app.show_speed_limit = false;
                }
            }
            ui.separator();
            ui.label(
                RichText::new("The limit is applied to the total of all connections.")
                    .size(8.8)
                    .color(theme::TEXT_MUTED),
            );
        });
    app.show_speed_limit = open;
}

fn filter_help_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_filter_help;
    egui::Window::new("Filter help")
        .default_pos([400.0, 220.0])
        .default_size([460.0, 300.0])
        .resizable(false)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(RichText::new("RSS filters").size(13.0).strong());
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Each filter compares the item name with a wildcard pattern.\n\n\
                     pattern | A\nadds a match to the queue.\n\n\
                     pattern | D\ndenes a match.\n\n\
                     Optional letters after A: F (add as first), P (add as paused), R (use release name).\n\n\
                     The last matching filter wins. Lines beginning with ; are comments.",
                )
                .size(9.5),
            );
            ui.add_space(8.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Close").clicked() {
                    app.show_filter_help = false;
                }
            });
        });
    app.show_filter_help = open;
}

fn message_window(app: &mut App, ctx: &egui::Context) {
    let Some((title, text)) = app.show_message.clone() else {
        return;
    };
    let mut open = true;
    let mut close = false;
    egui::Window::new(title)
        .default_pos([400.0, 260.0])
        .default_size([440.0, 180.0])
        .resizable(false)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.label(RichText::new(text).size(10.0));
            ui.add_space(10.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("OK").clicked() {
                    close = true;
                }
            });
        });
    if close || !open {
        app.show_message = None;
    }
}
