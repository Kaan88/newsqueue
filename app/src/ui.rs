use eframe::egui::{
    self, Align, Align2, Color32, FontId, Label, Layout, RichText, Sense, Stroke, TextEdit, Ui,
    Vec2,
};
use egui_extras::{Column, TableBuilder};

use crate::model::*;
use crate::theme::{self, Icon};
use crate::{App, MoveDir, Selection, UiAction};

#[derive(Clone, Copy, Debug)]
enum QueueRowRef {
    Collection(usize),
    File(usize, usize),
}

#[derive(Clone)]
struct ExplorerEntry {
    name: String,
    path: std::path::PathBuf,
    is_dir: bool,
    size: u64,
    kind: String,
    modified: String,
    accessed: String,
    created: String,
    readonly: bool,
}

enum ExplorerAction {
    OpenParent(std::path::PathBuf),
    NewFolder,
    Properties(std::path::PathBuf),
    Delete(std::path::PathBuf),
}

pub fn draw(app: &mut App, ctx: &egui::Context) {
    normalize_tab(app);

    egui::TopBottomPanel::top("main_toolbar")
        .exact_height(TOOLBAR_HEIGHT)
        .frame(egui::Frame::none().fill(theme::BG))
        .show(ctx, |ui| toolbar(app, ui));

    egui::TopBottomPanel::top("main_tabs")
        .exact_height(TAB_BAR_HEIGHT + 1.0)
        .frame(egui::Frame::none().fill(theme::BG))
        .show(ctx, |ui| tabs(app, ui));

    egui::TopBottomPanel::bottom("status_bar")
        .exact_height(STATUS_BAR_HEIGHT)
        .frame(egui::Frame::none().fill(theme::BG))
        .show(ctx, |ui| status_bar(app, ui));

    if app.show_threads {
        let mut height = app.thread_height.clamp(80.0, 520.0);
        let panel = egui::TopBottomPanel::bottom("threads_panel")
            .resizable(true)
            .min_height(70.0)
            .max_height(520.0)
            .default_height(height)
            .frame(
                egui::Frame::none()
                    .fill(theme::PANEL_WHITE)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER)),
            );
        let resp = panel.show(ctx, |ui| {
            thread_panel(app, ui);
        });
        if (resp.response.rect.height() - height).abs() > 1.0 {
            height = resp.response.rect.height();
            app.thread_height = height;
            app.settings.thread_list_height = height;
        }
    }

    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(theme::PANEL_WHITE))
        .show(ctx, |ui| match app.active_tab {
            0 => queue_tab(app, ui),
            1 => par2_tab(app, ui),
            2 => explorer_tab(app, ui),
            3 => log_tab(app, ui),
            _ => rss_tab(app, ui),
        });
}

fn normalize_tab(app: &mut App) {
    let visible = [
        app.settings.show_tabs[0],
        app.settings.show_tabs[1],
        app.settings.show_tabs[2],
        app.settings.show_tabs[3],
        app.settings.show_tabs[4],
    ];
    if !visible.get(app.active_tab).copied().unwrap_or(false) {
        if let Some((idx, _)) = visible.iter().enumerate().find(|(_, v)| **v) {
            app.active_tab = idx;
        } else {
            app.settings.show_tabs[0] = true;
            app.active_tab = 0;
        }
    }
}

fn toolbar(app: &mut App, ui: &mut Ui) {
    let rect = ui.max_rect();
    theme::paint_toolbar_gradient(ui, rect);
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    ui.horizontal(|ui| {
        ui.add_space(2.0);
        let r = theme::tool_button(ui, "Connect", Icon::Connect, app.connected);
        if r.clicked() {
            app.toggle_connect();
        }
        r.on_hover_text("Connect to / Disconnect from usenet server");
        toolbar_sep(ui);
        let r = theme::tool_button(ui, "Pause", Icon::Pause, app.paused);
        if r.clicked() {
            app.toggle_pause();
        }
        r.on_hover_text("Pause/Resume download threads [F1]");
        let r = theme::tool_button(ui, "Pause for", Icon::PauseFor, false);
        if r.clicked() {
            app.show_pause_for = true;
        }
        r.on_hover_text("Pause for a chosen number of minutes");
        toolbar_sep(ui);
        let r = theme::tool_button(ui, "NZB", Icon::Nzb, false);
        if r.clicked() {
            app.import_nzb();
        }
        r.on_hover_text("Import NZB files\n[hold SHIFT for paused state]");
        toolbar_sep(ui);
        let r = theme::tool_button(ui, "Download", Icon::Download, false);
        if r.clicked() {
            app.open_download_folder();
        }
        r.on_hover_text("Open the download folder\n[hold SHIFT for unrar folder]");
        toolbar_sep(ui);
        let r = theme::tool_button(ui, "Cleanup", Icon::Cleanup, false);
        if r.clicked() {
            app.cleanup();
        }
        r.on_hover_text("Remove downloaded entries from the queue and completed PAR2 sets");
        toolbar_sep(ui);
        let r = theme::tool_button(ui, "Search", Icon::Search, false);
        if r.clicked() {
            app.show_search = true;
        }
        r.on_hover_text("Global search [CTRL+F]");
        toolbar_sep(ui);
        let r = theme::tool_button(
            ui,
            "Scheduler",
            Icon::Scheduler,
            app.settings.scheduler_enabled,
        );
        if r.clicked() {
            app.settings.scheduler_enabled = !app.settings.scheduler_enabled;
            app.log(
                "Scheduler",
                if app.settings.scheduler_enabled {
                    "Scheduler enabled."
                } else {
                    "Scheduler disabled."
                },
            );
        }
        r.on_hover_text("Enable/Disable scheduler");
        toolbar_sep(ui);
        let r = theme::tool_button(ui, "Shutdown", Icon::Shutdown, app.shutdown_armed);
        if r.clicked() {
            app.show_shutdown = true;
        }
        r.on_hover_text("Shutdown when all tasks are done");
        toolbar_sep(ui);
        let r = theme::tool_button(ui, "Setup", Icon::Setup, false);
        if r.clicked() {
            app.setup_draft = app.settings.clone();
            app.show_setup = true;
            app.setup_page = 0;
        }
        r.on_hover_text("Setup options");
        toolbar_sep(ui);
        let r = theme::tool_button(ui, "Home", Icon::Home, false);
        if r.clicked() {
            app.toast("Home link is not available in this build");
        }
        r.on_hover_text("Project home");
        let r = theme::tool_button(ui, "Forum", Icon::Forum, false);
        if r.clicked() {
            app.toast("Forum link is not available in this build");
        }
        r.on_hover_text("Community forum");
        let r = theme::tool_button(ui, "Version?", Icon::Version, false);
        if r.clicked() {
            app.show_message = Some((
                "Version".into(),
                format!("{APP_NAME} {VERSION}\nThis is the latest build in this project."),
            ));
        }
        r.on_hover_text("Check for a new version");
        let r = theme::tool_button(ui, "Donate", Icon::Donate, false);
        if r.clicked() {
            app.toast("This free build does not take donations");
        }
        r.on_hover_text("Donate");
        let r = theme::tool_button(ui, "About", Icon::About, false);
        if r.clicked() {
            app.show_about = true;
        }
        r.on_hover_text("About...");
        let r = theme::tool_button(ui, "Notes", Icon::Notes, false);
        if r.clicked() {
            app.notes_draft = app.settings.notes.clone();
            app.show_notes = true;
        }
        r.on_hover_text("Notes");
        let r = theme::tool_button(ui, "Stats", Icon::Stats, false);
        if r.clicked() {
            app.show_stats = true;
        }
        r.on_hover_text("Statistics and download history");
        let r = theme::tool_button(ui, "To tray", Icon::Tray, false);
        if r.clicked() {
            app.toast("Window would be minimized to the tray");
        }
        r.on_hover_text("Minimize to tray");
    });
}

fn toolbar_sep(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(2.0, 53.0), Sense::hover());
    ui.painter().line_segment(
        [
            egui::pos2(rect.center().x, rect.top() + 6.0),
            egui::pos2(rect.center().x, rect.bottom() - 6.0),
        ],
        Stroke::new(1.0_f32, Color32::from_rgb(210, 223, 240)),
    );
}

fn tabs(app: &mut App, ui: &mut Ui) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, theme::BG);
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0_f32, theme::BORDER),
    );
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    ui.horizontal(|ui| {
        let defs = [
            ("Download queue", Icon::Queue, 130.0),
            ("PAR2", Icon::Shield, 78.0),
            ("Explorer", Icon::Folder, 100.0),
            ("Log", Icon::Page, 72.0),
            ("RSS", Icon::Rss, 72.0),
        ];
        for (i, (caption, icon, width)) in defs.iter().enumerate() {
            if !app.settings.show_tabs[i] {
                continue;
            }
            let r = theme::tab_button(ui, caption, *icon, app.active_tab == i, *width);
            if r.clicked() {
                app.active_tab = i;
            }
        }
        // The tab strip is only as wide as the captions; leave the rest empty.
        ui.allocate_space(Vec2::new(ui.available_width(), TAB_BAR_HEIGHT));
    });
}

fn queue_tab(app: &mut App, ui: &mut Ui) {
    let bar = egui::TopBottomPanel::top("dq_toolbar")
        .exact_height(DQ_TOOLBAR_HEIGHT)
        .frame(egui::Frame::none().fill(theme::BG_PALE))
        .show_inside(ui, |ui| {
            ui.painter().line_segment(
                [ui.max_rect().left_bottom(), ui.max_rect().right_bottom()],
                Stroke::new(1.0_f32, theme::BORDER_LIGHT),
            );
            ui.spacing_mut().item_spacing = Vec2::new(1.0, 0.0);
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                let mut action = None;
                dq_button(ui, Icon::Play, "Unpause selected files", || {
                    action = Some(UiAction::PauseSelected(false))
                });
                dq_button(ui, Icon::Pause, "Pause selected files", || {
                    action = Some(UiAction::PauseSelected(true))
                });
                dq_sep(ui);
                dq_button(ui, Icon::Top, "Move selected to top", || {
                    action = Some(UiAction::Move(MoveDir::Top))
                });
                dq_button(ui, Icon::Up, "Move selected up", || {
                    action = Some(UiAction::Move(MoveDir::Up))
                });
                dq_button(ui, Icon::Down, "Move selected down", || {
                    action = Some(UiAction::Move(MoveDir::Down))
                });
                dq_button(ui, Icon::Bottom, "Move selected to bottom", || {
                    action = Some(UiAction::Move(MoveDir::Bottom))
                });
                dq_sep(ui);
                dq_button(ui, Icon::Join, "Join", || action = Some(UiAction::None));
                dq_button(ui, Icon::Split, "Split", || action = Some(UiAction::None));
                dq_sep(ui);
                dq_button(ui, Icon::Collapse, "Collapse all", || {
                    for c in &mut app.queue {
                        c.expanded = false;
                    }
                });
                dq_button(ui, Icon::Expand, "Expand all", || {
                    for c in &mut app.queue {
                        c.expanded = true;
                    }
                });
                dq_sep(ui);
                dq_button(ui, Icon::Unrar, "Unrar", || {
                    action = Some(UiAction::ToggleUnrar(true))
                });
                dq_button(ui, Icon::NoUnrar, "Do not unrar", || {
                    action = Some(UiAction::ToggleUnrar(false))
                });
                dq_button(ui, Icon::Key, "Set unrar password", || {
                    action = Some(UiAction::None)
                });
                dq_button(ui, Icon::Folder, "Change unrar folder", || {
                    action = Some(UiAction::ChangeFolder)
                });
                dq_sep(ui);
                dq_button(ui, Icon::Rename, "Rename collection", || {
                    action = Some(UiAction::Rename)
                });
                dq_button(ui, Icon::Export, "Export selected to NZB", || {
                    action = Some(UiAction::Export)
                });
                dq_button(ui, Icon::Export, "Export selected to multiple NZBs", || {
                    action = Some(UiAction::Export)
                });
                dq_button(ui, Icon::Folder, "Change download folder", || {
                    action = Some(UiAction::ChangeFolder)
                });
                dq_sep(ui);
                dq_button(ui, Icon::Play, "Play media file while downloading", || {
                    action = Some(UiAction::Play)
                });
                dq_button(ui, Icon::Key, "Change execute command", || {
                    action = Some(UiAction::None)
                });
                dq_button(ui, Icon::Eta, "ETA on selected", || {
                    action = Some(UiAction::Eta)
                });
                if let Some(a) = action {
                    app.apply_ui_action(a);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(4.0);
                    let mut filter = app.filter.clone();
                    let resp = ui.add_sized(
                        [122.0, 20.0],
                        TextEdit::singleline(&mut filter).hint_text("Filter subjects"),
                    );
                    if resp.changed() {
                        app.filter = filter;
                    }
                    resp.on_hover_text("Filter subjects");
                    ui.label(RichText::new("Filter").size(9.5));
                });
            });
        });
    let _ = bar;

    let rows = build_rows(app);
    let queue = &app.queue;
    let selected = app.selected;
    let visible = app.settings.column_visible;
    let widths = app.settings.column_widths;
    let mut new_selection: Option<Option<Selection>> = None;
    let mut toggle: Option<usize> = None;
    let mut action: Option<UiAction> = None;
    let mut double_toggle = false;

    let col_map: Vec<usize> = visible
        .iter()
        .enumerate()
        .filter(|(_, v)| **v)
        .map(|(i, _)| i)
        .collect();
    if col_map.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new(
                    "No queue columns are selected. Open Setup > Columns... to enable one.",
                )
                .size(10.0),
            );
        });
        return;
    }
    let mut table = TableBuilder::new(ui)
        .striped(false)
        .resizable(true)
        // Rows must sense clicks, otherwise the row response can never report
        // a click, a double click or the right-click that opens the menu.
        .sense(egui::Sense::click())
        .cell_layout(Layout::left_to_right(Align::Center))
        .min_scrolled_height(0.0);
    for &i in &col_map {
        let mut col = Column::initial(widths[i])
            .at_least(28.0)
            .clip(true)
            .resizable(true);
        if i == 0 {
            col = col.at_least(120.0);
        }
        table = table.column(col);
    }
    table
        .header(20.0, |mut header| {
            for &ci in &col_map {
                header.col(|ui| {
                    ui.painter().rect_filled(ui.max_rect(), 0.0, theme::BG);
                    ui.painter().line_segment(
                        [ui.max_rect().left_bottom(), ui.max_rect().right_bottom()],
                        Stroke::new(1.0_f32, theme::BORDER_LIGHT),
                    );
                    let align = match ci {
                        1..=3 => Align::Max,
                        6 | 8 => Align::Center,
                        _ => Align::Min,
                    };
                    let layout = match align {
                        Align::Max => Layout::right_to_left(Align::Center),
                        Align::Center => {
                            Layout::centered_and_justified(egui::Direction::LeftToRight)
                        }
                        Align::Min => Layout::left_to_right(Align::Center),
                    };
                    ui.with_layout(layout, |ui| {
                        ui.add(
                            Label::new(RichText::new(QUEUE_COLUMN_NAMES[ci]).size(9.5)).truncate(),
                        );
                    });
                });
            }
        })
        .body(|body| {
            body.rows(21.0, rows.len(), |mut row| {
                let ri = row.index();
                let row_ref = rows[ri];
                // A transparent click-catcher is added after the cell contents so
                // right-clicks on text, labels and progress bars reach the row too.
                let mut row_click: Option<egui::Response> = None;
                match row_ref {
                    QueueRowRef::Collection(ci) => {
                        let c = &queue[ci];
                        let selected_here = selected
                            == Some(Selection {
                                collection: ci,
                                file: None,
                            });
                        row.set_selected(selected_here);
                        for &col in &col_map {
                            row.col(|ui| {
                                match col {
                                    0 => {
                                        ui.spacing_mut().item_spacing.x = 2.0;
                                        if !c.files.is_empty() {
                                            let label = if c.expanded { "-" } else { "+" };
                                            let btn = ui.add_sized(
                                                [14.0, 14.0],
                                                egui::Button::new(RichText::new(label).size(9.0))
                                                    .frame(false),
                                            );
                                            if btn.clicked() {
                                                toggle = Some(ci);
                                            }
                                        } else {
                                            ui.add_space(14.0);
                                        }
                                        let (r, _) = ui
                                            .allocate_exact_size(Vec2::new(15.0, 15.0), Sense::hover());
                                        theme::paint_icon(ui.painter(), r, Icon::Folder, theme::TEXT);
                                        ui.add(
                                            Label::new(
                                                RichText::new(&c.subject)
                                                    .size(10.0)
                                                    .color(theme::TEXT_GROUP),
                                            )
                                            .truncate(),
                                        );
                                    }
                                    1 => {
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            ui.label(
                                                RichText::new(format_size_short(c.total_bytes))
                                                    .size(9.5),
                                            );
                                        });
                                    }
                                    2 => {
                                        ui.with_layout(
                                            Layout::centered_and_justified(
                                                egui::Direction::LeftToRight,
                                            ),
                                            |ui| {
                                                ui.label(
                                                    RichText::new(format!("{}p", c.parts)).size(9.5),
                                                );
                                            },
                                        );
                                    }
                                    3 => {
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            ui.label(RichText::new(format_age(c.age_days)).size(9.5));
                                        });
                                    }
                                    4 => {
                                        ui.add(
                                            Label::new(RichText::new(&c.group).size(9.5)).truncate(),
                                        );
                                    }
                                    5 => {
                                        ui.add(
                                            Label::new(RichText::new(&c.poster).size(9.5)).truncate(),
                                        );
                                    }
                                    6 => {
                                        let p = c.progress();
                                        match c.status {
                                            ItemStatus::Queued => {
                                                ui.with_layout(
                                                    Layout::centered_and_justified(
                                                        egui::Direction::LeftToRight,
                                                    ),
                                                    |ui| {
                                                        ui.label(RichText::new("Queued").size(9.0));
                                                    },
                                                );
                                            }
                                            ItemStatus::Paused => {
                                                theme::progress_bar(
                                                    ui,
                                                    p,
                                                    &format!("Paused {:.0}%", p * 100.0),
                                                    false,
                                                );
                                            }
                                            ItemStatus::Error => {
                                                ui.with_layout(
                                                    Layout::centered_and_justified(
                                                        egui::Direction::LeftToRight,
                                                    ),
                                                    |ui| {
                                                        ui.label(
                                                            RichText::new("Error!")
                                                                .size(9.0)
                                                                .color(theme::RED),
                                                        );
                                                    },
                                                );
                                            }
                                            _ => {
                                                theme::progress_bar(
                                                    ui,
                                                    p,
                                                    &format!("{:.0}%", p * 100.0),
                                                    c.status == ItemStatus::Completed,
                                                );
                                            }
                                        }
                                    }
                                    7 => {
                                        ui.add(
                                            Label::new(RichText::new(&c.folder).size(8.8)).truncate(),
                                        );
                                    }
                                    8 => {
                                        ui.with_layout(
                                            Layout::centered_and_justified(
                                                egui::Direction::LeftToRight,
                                            ),
                                            |ui| {
                                                let (txt, color) = if c.unrar {
                                                    ("Yes", theme::GREEN_DARK)
                                                } else {
                                                    ("-", theme::TEXT_MUTED)
                                                };
                                                ui.label(RichText::new(txt).size(9.0).color(color));
                                            },
                                        );
                                    }
                                    9 => {
                                        ui.add(
                                            Label::new(
                                                RichText::new(if c.execute.is_empty() {
                                                    "-"
                                                } else {
                                                    &c.execute
                                                })
                                                .size(9.0),
                                            )
                                            .truncate(),
                                        );
                                    }
                                    10 => {
                                        ui.with_layout(
                                            Layout::centered_and_justified(
                                                egui::Direction::LeftToRight,
                                            ),
                                            |ui| {
                                                ui.label(
                                                    RichText::new(if c.delete_archives {
                                                        "Yes"
                                                    } else {
                                                        "-"
                                                    })
                                                    .size(9.0),
                                                );
                                            },
                                        );
                                    }
                                    _ => {
                                        ui.add(
                                            Label::new(RichText::new(&c.posted_on).size(9.0))
                                                .truncate(),
                                        );
                                    }
                                }
                                row_click_overlay(ui, col, &mut row_click);
                            });
                        }
                    }
                    QueueRowRef::File(ci, fi) => {
                        let c = &queue[ci];
                        let f = &c.files[fi];
                        let selected_here = selected
                            == Some(Selection {
                                collection: ci,
                                file: Some(fi),
                            });
                        row.set_selected(selected_here);
                        for &col in &col_map {
                            row.col(|ui| {
                                match col {
                                    0 => {
                                        ui.add_space(18.0);
                                        let (r, _) = ui
                                            .allocate_exact_size(Vec2::new(13.0, 13.0), Sense::hover());
                                        theme::paint_icon(ui.painter(), r, Icon::File, theme::TEXT);
                                        ui.add(Label::new(RichText::new(&f.name).size(9.2)).truncate());
                                    }
                                    1 => {
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            ui.label(
                                                RichText::new(format_size_short(f.bytes)).size(9.2),
                                            );
                                        });
                                    }
                                    2 => {
                                        ui.with_layout(
                                            Layout::centered_and_justified(
                                                egui::Direction::LeftToRight,
                                            ),
                                            |ui| {
                                                ui.label(
                                                    RichText::new(format!("{}p", f.parts)).size(9.2),
                                                );
                                            },
                                        );
                                    }
                                    6 => {
                                        let p = if f.bytes == 0 {
                                            0.0
                                        } else {
                                            (f.downloaded / f.bytes as f64) as f32
                                        };
                                        if p >= 1.0 {
                                            theme::progress_bar(ui, 1.0, "100%", true);
                                        } else if p > 0.0 {
                                            theme::progress_bar(
                                                ui,
                                                p,
                                                &format!("{:.0}%", p * 100.0),
                                                false,
                                            );
                                        } else {
                                            ui.with_layout(
                                                Layout::centered_and_justified(
                                                    egui::Direction::LeftToRight,
                                                ),
                                                |ui| {
                                                    ui.label(RichText::new("Queued").size(9.0));
                                                },
                                            );
                                        }
                                    }
                                    _ => {}
                                }
                                row_click_overlay(ui, col, &mut row_click);
                            });
                        }
                    }
                }
                let resp = row_click.unwrap_or_else(|| row.response());
                if resp.clicked() {
                    new_selection = Some(Some(match row_ref {
                        QueueRowRef::Collection(ci) => Selection {
                            collection: ci,
                            file: None,
                        },
                        QueueRowRef::File(ci, fi) => Selection {
                            collection: ci,
                            file: Some(fi),
                        },
                    }));
                }
                if resp.secondary_clicked() {
                    new_selection = Some(Some(match row_ref {
                        QueueRowRef::Collection(ci) => Selection {
                            collection: ci,
                            file: None,
                        },
                        QueueRowRef::File(ci, fi) => Selection {
                            collection: ci,
                            file: Some(fi),
                        },
                    }));
                }
                if resp.double_clicked() {
                    double_toggle = true;
                }
                resp.context_menu(|ui| {
                    if ui.button("Unpause").clicked() {
                        action = Some(UiAction::PauseSelected(false));
                        new_selection = Some(Some(match row_ref {
                            QueueRowRef::Collection(ci) => Selection {
                                collection: ci,
                                file: None,
                            },
                            QueueRowRef::File(ci, fi) => Selection {
                                collection: ci,
                                file: Some(fi),
                            },
                        }));
                        ui.close_menu();
                    }
                    if ui.button("Pause").clicked() {
                        action = Some(UiAction::PauseSelected(true));
                        new_selection = Some(Some(match row_ref {
                            QueueRowRef::Collection(ci) => Selection {
                                collection: ci,
                                file: None,
                            },
                            QueueRowRef::File(ci, fi) => Selection {
                                collection: ci,
                                file: Some(fi),
                            },
                        }));
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.menu_button("Move", |ui| {
                        if ui.button("Move to top").clicked() {
                            action = Some(UiAction::Move(MoveDir::Top));
                            ui.close_menu();
                        }
                        if ui.button("Move up").clicked() {
                            action = Some(UiAction::Move(MoveDir::Up));
                            ui.close_menu();
                        }
                        if ui.button("Move down").clicked() {
                            action = Some(UiAction::Move(MoveDir::Down));
                            ui.close_menu();
                        }
                        if ui.button("Move to bottom").clicked() {
                            action = Some(UiAction::Move(MoveDir::Bottom));
                            ui.close_menu();
                        }
                    });
                    if ui.button("Join").clicked() {
                        action = Some(UiAction::None);
                        ui.close_menu();
                    }
                    if ui.button("Split").clicked() {
                        action = Some(UiAction::None);
                        ui.close_menu();
                    }
                    if ui.button("Collapse all").clicked() {
                        action = Some(UiAction::CollapseAll);
                        ui.close_menu();
                    }
                    if ui.button("Expand all").clicked() {
                        action = Some(UiAction::ExpandAll);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Rename collection").clicked() {
                        action = Some(UiAction::Rename);
                        ui.close_menu();
                    }
                    if ui.button("Export to NZB").clicked() {
                        action = Some(UiAction::Export);
                        ui.close_menu();
                    }
                    if ui.button("Change download folder").clicked() {
                        action = Some(UiAction::ChangeFolder);
                        ui.close_menu();
                    }
                    if ui.button("Open").clicked() {
                        action = Some(UiAction::OpenFolder);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Unrar").clicked() {
                        action = Some(UiAction::ToggleUnrar(true));
                        ui.close_menu();
                    }
                    if ui.button("Do not Unrar").clicked() {
                        action = Some(UiAction::ToggleUnrar(false));
                        ui.close_menu();
                    }
                    if ui.button("Unpack now").clicked() {
                        action = Some(UiAction::UnpackNow);
                        ui.close_menu();
                    }
                    if ui.button("Change unrar folder").clicked() {
                        action = Some(UiAction::ChangeUnrarFolder);
                        ui.close_menu();
                    }
                    if ui.button("Set password").clicked() {
                        action = Some(UiAction::None);
                        ui.close_menu();
                    }
                    if ui.button("Set category").clicked() {
                        action = Some(UiAction::SetCategory);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Delete").clicked() {
                        action = Some(UiAction::DeleteSelected);
                        ui.close_menu();
                    }
                });
            });
        });

    if let Some(sel) = new_selection {
        app.selected = sel;
    }
    if let Some(ci) = toggle {
        app.toggle_expand(ci);
    }
    if double_toggle {
        if app.settings.dblclick_expand {
            if let Some(sel) = app.selected {
                app.toggle_expand(sel.collection);
            }
        } else {
            let pause = app
                .selected
                .and_then(|s| app.queue.get(s.collection))
                .map(|c| c.status != ItemStatus::Paused)
                .unwrap_or(true);
            app.pause_selected(pause);
        }
    }
    if let Some(a) = action {
        app.apply_ui_action(a);
    }
}

fn row_click_overlay(ui: &mut Ui, col: usize, acc: &mut Option<egui::Response>) {
    let mut rect = ui.max_rect();
    if col == 0 {
        // Keep the expand/collapse button on the left clickable.
        rect.min.x += 15.0;
    }
    let response = ui.interact(rect, ui.id().with("row_click"), Sense::click());
    *acc = Some(match acc.take() {
        Some(previous) => previous.union(response),
        None => response,
    });
}

fn dq_button(ui: &mut Ui, icon: Icon, hint: &str, mut clicked: impl FnMut()) {
    let r = theme::small_button(ui, icon, false);
    if r.clicked() {
        clicked();
    }
    r.on_hover_text(hint);
}

fn dq_sep(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(5.0, 24.0), Sense::hover());
    ui.painter().line_segment(
        [
            egui::pos2(rect.center().x, rect.top() + 4.0),
            egui::pos2(rect.center().x, rect.bottom() - 4.0),
        ],
        Stroke::new(1.0_f32, theme::BORDER),
    );
}

fn build_rows(app: &App) -> Vec<QueueRowRef> {
    let f = app.filter.to_lowercase();
    let mut rows = Vec::new();
    for (ci, c) in app.queue.iter().enumerate() {
        let subject_match = f.is_empty() || c.subject.to_lowercase().contains(&f);
        let any_file = c.files.iter().any(|x| x.name.to_lowercase().contains(&f));
        if !f.is_empty() && !subject_match && !any_file {
            continue;
        }
        rows.push(QueueRowRef::Collection(ci));
        if c.expanded {
            for (fi, file) in c.files.iter().enumerate() {
                if f.is_empty() || subject_match || file.name.to_lowercase().contains(&f) {
                    rows.push(QueueRowRef::File(ci, fi));
                }
            }
        }
    }
    rows
}

fn par2_tab(app: &mut App, ui: &mut Ui) {
    egui::TopBottomPanel::top("par2_toolbar")
        .exact_height(30.0)
        .frame(egui::Frame::none().fill(theme::BG_PALE))
        .show_inside(ui, |ui| {
            ui.painter().line_segment(
                [ui.max_rect().left_bottom(), ui.max_rect().right_bottom()],
                Stroke::new(1.0_f32, theme::BORDER_LIGHT),
            );
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                let r = theme::small_button(ui, Icon::Shield, false);
                if r.clicked() {
                    if let Some(i) = app.selected_par2 {
                        if let Some(p) = app.par2.get_mut(i) {
                            p.status = "Repairing".into();
                            p.info = "Repairing...".into();
                            app.repair_until = Some((
                                i,
                                std::time::Instant::now() + std::time::Duration::from_secs(6),
                            ));
                            let name = p.name.clone();
                            app.log("PAR2", &format!("Repair started: {name}"));
                        }
                    } else {
                        app.toast("Select a PAR2 set first");
                    }
                }
                r.on_hover_text("Repair PAR2 set");
                let r = theme::small_button(ui, Icon::Unrar, false);
                if r.clicked() {
                    if let Some(i) = app.selected_par2 {
                        if let Some(p) = app.par2.get_mut(i) {
                            p.status = "Unpacking".into();
                            p.info = "Queued for unpack".into();
                            let name = p.name.clone();
                            app.log("Unpack", &format!("Unpack queued: {name}"));
                        }
                    }
                }
                r.on_hover_text("Unpack PAR2/SFV set");
                let r = theme::small_button(ui, Icon::Add, false);
                if r.clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("PAR2/SFV files", &["par2", "PAR2", "sfv", "SFV"])
                        .pick_file()
                    {
                        let name = path
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "imported.par2".into());
                        let size = std::fs::metadata(&path)
                            .map(|m| m.len() as f64 / MB)
                            .unwrap_or(0.0);
                        app.par2.push(Par2Set {
                            name: name.clone(),
                            size_mb: size,
                            blocks: 0,
                            info: "Queued".into(),
                            unrar: false,
                            progress: 0.0,
                            status: "Queued".into(),
                        });
                        app.log("PAR2", &format!("Imported PAR2/SFV: {name}"));
                    }
                }
                r.on_hover_text("Import PAR2/SFV file(s)");
                let r = theme::small_button(ui, Icon::Trash, false);
                if r.clicked() {
                    if let Some(i) = app.selected_par2 {
                        if i < app.par2.len() {
                            let p = app.par2.remove(i);
                            app.log("PAR2", &format!("Deleted: {}", p.name));
                            app.selected_par2 = None;
                        }
                    }
                }
                r.on_hover_text("Delete selected PAR2 set");
                let r = theme::small_button(ui, Icon::Up, false);
                if r.clicked() {
                    if let Some(i) = app.selected_par2 {
                        if i > 0 {
                            app.par2.swap(i, i - 1);
                            app.selected_par2 = Some(i - 1);
                        }
                    }
                }
                r.on_hover_text("Move up");
                let r = theme::small_button(ui, Icon::Down, false);
                if r.clicked() {
                    if let Some(i) = app.selected_par2 {
                        if i + 1 < app.par2.len() {
                            app.par2.swap(i, i + 1);
                            app.selected_par2 = Some(i + 1);
                        }
                    }
                }
                r.on_hover_text("Move down");
                ui.add_space(10.0);
                ui.label(
                    RichText::new("PAR2 checking and repairing are enabled")
                        .size(9.5)
                        .color(theme::TEXT_MUTED),
                );
            });
        });

    let selected = app.selected_par2;
    let par2 = &app.par2;
    let mut new_sel: Option<Option<usize>> = None;
    let table = TableBuilder::new(ui)
        .striped(false)
        .resizable(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(
            Column::initial(300.0)
                .at_least(120.0)
                .clip(true)
                .resizable(true),
        )
        .column(
            Column::initial(90.0)
                .at_least(50.0)
                .clip(true)
                .resizable(true),
        )
        .column(
            Column::initial(75.0)
                .at_least(40.0)
                .clip(true)
                .resizable(true),
        )
        .column(
            Column::remainder()
                .at_least(100.0)
                .clip(true)
                .resizable(true),
        )
        .column(
            Column::initial(45.0)
                .at_least(30.0)
                .clip(true)
                .resizable(true),
        )
        .column(
            Column::initial(110.0)
                .at_least(60.0)
                .clip(true)
                .resizable(true),
        );
    let table = table.header(20.0, |mut header| {
        for text in [
            "Par2/sfv set",
            "Size",
            "Blocks",
            "Info",
            "Unrar",
            "Progress",
        ] {
            header.col(|ui| {
                ui.painter().rect_filled(ui.max_rect(), 0.0, theme::BG);
                ui.painter().line_segment(
                    [ui.max_rect().left_bottom(), ui.max_rect().right_bottom()],
                    Stroke::new(1.0_f32, theme::BORDER_LIGHT),
                );
                ui.add(Label::new(RichText::new(text).size(9.5)).truncate());
            });
        }
    });
    table.body(|body| {
        body.rows(22.0, par2.len(), |mut row| {
            let i = row.index();
            let p = &par2[i];
            row.set_selected(selected == Some(i));
            row.col(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), Sense::hover());
                theme::paint_icon(ui.painter(), r, Icon::Shield, theme::TEXT);
                ui.add(Label::new(RichText::new(&p.name).size(9.5)).truncate());
            });
            row.col(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(format!("{:.0} MB", p.size_mb)).size(9.5));
                });
            });
            row.col(|ui| {
                ui.with_layout(
                    Layout::centered_and_justified(egui::Direction::LeftToRight),
                    |ui| {
                        ui.label(
                            RichText::new(if p.blocks > 0 {
                                p.blocks.to_string()
                            } else {
                                "-".into()
                            })
                            .size(9.5),
                        );
                    },
                );
            });
            row.col(|ui| {
                ui.add(Label::new(RichText::new(&p.info).size(9.5)).truncate());
            });
            row.col(|ui| {
                ui.with_layout(
                    Layout::centered_and_justified(egui::Direction::LeftToRight),
                    |ui| {
                        ui.label(
                            RichText::new(if p.unrar { "Yes" } else { "-" })
                                .size(9.0)
                                .color(if p.unrar {
                                    theme::GREEN_DARK
                                } else {
                                    theme::TEXT_MUTED
                                }),
                        );
                    },
                );
            });
            row.col(|ui| {
                theme::progress_bar(
                    ui,
                    p.progress,
                    &format!("{:.0}%", p.progress * 100.0),
                    p.progress >= 1.0,
                );
            });
            let resp = row.response();
            if resp.clicked() || resp.secondary_clicked() {
                new_sel = Some(Some(i));
            }
        });
    });
    if let Some(s) = new_sel {
        app.selected_par2 = s;
    }
}

fn explorer_tab(app: &mut App, ui: &mut Ui) {
    egui::TopBottomPanel::top("explorer_toolbar")
        .exact_height(34.0)
        .frame(egui::Frame::none().fill(theme::BG_PALE))
        .show_inside(ui, |ui| {
            ui.painter().line_segment(
                [ui.max_rect().left_bottom(), ui.max_rect().right_bottom()],
                Stroke::new(1.0_f32, theme::BORDER_LIGHT),
            );
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                let r = theme::small_button(ui, Icon::Back, false);
                if r.clicked() {
                    app.explorer_back();
                }
                r.on_hover_text("Back");
                let r = theme::small_button(ui, Icon::Forward, false);
                if r.clicked() {
                    app.explorer_forward();
                }
                r.on_hover_text("Forward");
                let r = theme::small_button(ui, Icon::Up, false);
                if r.clicked() {
                    app.explorer_up();
                }
                r.on_hover_text("Up");
                let r = theme::small_button(ui, Icon::Refresh, false);
                if r.clicked() {
                    app.explorer_selected = None;
                    app.toast("Folder refreshed");
                }
                r.on_hover_text("Refresh");
                let r = theme::small_button(ui, Icon::Folder, app.settings.exp_folders_visible);
                if r.clicked() {
                    app.settings.exp_folders_visible = !app.settings.exp_folders_visible;
                }
                r.on_hover_text("Show/Hide folders panel");
                ui.add_space(6.0);

                let mut path_text = app.explorer_path_edit.clone();
                let response = ui.add_sized(
                    [480.0, 20.0],
                    TextEdit::singleline(&mut path_text).hint_text("Path"),
                );
                response
                    .clone()
                    .on_hover_text(app.explorer_path.display().to_string());
                if response.changed() {
                    app.explorer_path_edit = path_text.clone();
                }
                let enter = response.lost_focus()
                    && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if enter {
                    app.explorer_navigate(std::path::PathBuf::from(path_text.trim()));
                }
                if ui.small_button("Go").clicked() {
                    app.explorer_navigate(std::path::PathBuf::from(path_text.trim()));
                }
                ui.menu_button("v", |ui| {
                    let roots = [
                        ("Download", app.settings.download_dir.clone()),
                        ("NZB", app.settings.nzb_dir.clone()),
                        ("Temp", app.settings.temp_dir.clone()),
                        ("Unrar", app.settings.unrar_dir.clone()),
                    ];
                    for (label, path) in roots {
                        if ui.button(label).clicked() {
                            app.explorer_navigate(std::path::PathBuf::from(path));
                            ui.close_menu();
                        }
                    }
                    if !app.settings.explorer_favorites.is_empty() {
                        ui.separator();
                        for path in app.settings.explorer_favorites.clone() {
                            if ui.button(&path).clicked() {
                                app.explorer_navigate(std::path::PathBuf::from(path));
                                ui.close_menu();
                            }
                        }
                    }
                    ui.separator();
                    for letter in b'A'..=b'Z' {
                        let drive = format!("{}:\\", letter as char);
                        if std::path::Path::new(&drive).is_dir()
                            && ui.button(&drive).clicked()
                        {
                            app.explorer_navigate(std::path::PathBuf::from(drive));
                            ui.close_menu();
                        }
                    }
                });
                let r = theme::small_button(ui, Icon::Add, false);
                if r.clicked() {
                    let path = app.explorer_path.to_string_lossy().to_string();
                    if !app.settings.explorer_favorites.contains(&path) {
                        app.settings.explorer_favorites.push(path);
                        app.toast("Added to favourites");
                    }
                }
                r.on_hover_text("Add current folder to favourites");
                let r = theme::small_button(ui, Icon::Trash, false);
                if r.clicked() {
                    let path = app.explorer_path.to_string_lossy().to_string();
                    app.settings.explorer_favorites.retain(|item| item != &path);
                    app.toast("Removed from favourites");
                }
                r.on_hover_text("Remove current folder from favourites");

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(4.0);
                    ui.add_sized(
                        [150.0, 20.0],
                        TextEdit::singleline(&mut app.explorer_filter)
                            .hint_text("Filter files/folders"),
                    );
                    ui.label(RichText::new("Filter").size(9.5));
                });
            });
        });

    if app.settings.exp_folders_visible {
        let panel = egui::SidePanel::left("explorer_folders")
            .resizable(true)
            .default_width(app.settings.exp_folders_width)
            .width_range(140.0..=400.0)
            .show_inside(ui, |ui| {
                ui.painter()
                    .rect_filled(ui.max_rect(), 0.0, Color32::from_rgb(246, 249, 253));
                ui.add_space(4.0);
                ui.label(RichText::new("Folders").size(10.0).strong());
                ui.separator();
                let roots = [
                    ("Download", app.settings.download_dir.clone()),
                    ("NZB", app.settings.nzb_dir.clone()),
                    ("Temp", app.settings.temp_dir.clone()),
                    ("Unrar", app.settings.unrar_dir.clone()),
                ];
                for (label, path) in roots {
                    let path = std::path::PathBuf::from(path);
                    let selected = app.explorer_path == path;
                    if ui
                        .selectable_label(selected, format!("[+] {label}"))
                        .clicked()
                    {
                        app.explorer_navigate(path);
                    }
                }
                if !app.settings.explorer_favorites.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new("Favourites").size(10.0).strong());
                    for path in app.settings.explorer_favorites.clone() {
                        let p = std::path::PathBuf::from(&path);
                        let selected = app.explorer_path == p;
                        let label = p
                            .file_name()
                            .map(|name| name.to_string_lossy().to_string())
                            .unwrap_or(path);
                        if ui.selectable_label(selected, format!(" {label}")).clicked() {
                            app.explorer_navigate(p);
                        }
                    }
                }
                ui.add_space(4.0);
                ui.label(RichText::new("This folder").size(10.0).strong());
                if let Ok(rd) = std::fs::read_dir(&app.explorer_path) {
                    let mut dirs: Vec<_> = rd
                        .flatten()
                        .filter(|entry| entry.path().is_dir())
                        .collect();
                    dirs.sort_by_key(|entry| entry.file_name());
                    for entry in dirs {
                        let path = entry.path();
                        let name = entry.file_name().to_string_lossy().to_string();
                        let selected = app.explorer_path == path;
                        if ui
                            .selectable_label(selected, format!("    {name}"))
                            .clicked()
                        {
                            app.explorer_navigate(path);
                        }
                    }
                }
            });
        app.settings.exp_folders_width = panel.response.rect.width();
    }

    let columns: Vec<usize> = app
        .settings
        .explorer_columns
        .iter()
        .enumerate()
        .filter(|(_, visible)| **visible)
        .map(|(index, _)| index)
        .collect();
    if columns.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("No explorer columns are selected. Enable one in Setup > Columns.")
                    .size(10.0),
            );
        });
        return;
    }

    let entries = read_explorer(&app.explorer_path, &app.explorer_filter);
    let widths = app.settings.explorer_column_widths;
    let selected = app.explorer_selected.clone();
    let mut new_selected: Option<std::path::PathBuf> = None;
    let mut open_entry: Option<(std::path::PathBuf, bool)> = None;
    let mut action: Option<ExplorerAction> = None;

    let mut table = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .sense(egui::Sense::click())
        .cell_layout(Layout::left_to_right(Align::Center));
    for &index in &columns {
        let mut column = Column::initial(widths[index])
            .at_least(50.0)
            .clip(true)
            .resizable(true);
        if index == 0 {
            column = column.at_least(140.0);
        }
        table = table.column(column);
    }

    table
        .header(20.0, |mut header| {
            for &index in &columns {
                header.col(|ui| {
                    ui.painter().rect_filled(ui.max_rect(), 0.0, theme::BG);
                    ui.painter().line_segment(
                        [ui.max_rect().left_bottom(), ui.max_rect().right_bottom()],
                        Stroke::new(1.0_f32, theme::BORDER_LIGHT),
                    );
                    let layout = if index == 1 {
                        Layout::right_to_left(Align::Center)
                    } else {
                        Layout::left_to_right(Align::Center)
                    };
                    ui.with_layout(layout, |ui| {
                        ui.add(
                            Label::new(RichText::new(explorer_column_name(index)).size(9.5))
                                .truncate(),
                        );
                    });
                });
            }
        })
        .body(|body| {
            body.rows(21.0, entries.len(), |mut row| {
                let index = row.index();
                let entry = &entries[index];
                row.set_selected(selected.as_deref() == Some(entry.path.as_path()));
                // Labels consume clicks; a transparent overlay makes the whole
                // row clickable, including the Name/Type/Size text.
                let mut row_click: Option<egui::Response> = None;
                for &column in &columns {
                    row.col(|ui| {
                        match column {
                        0 => {
                            let (rect, _) =
                                ui.allocate_exact_size(Vec2::new(14.0, 14.0), Sense::hover());
                            theme::paint_icon(
                                ui.painter(),
                                rect,
                                if entry.is_dir { Icon::Folder } else { Icon::File },
                                theme::TEXT,
                            );
                            ui.add(
                                Label::new(RichText::new(&entry.name).size(9.5)).truncate(),
                            );
                        }
                        1 => {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(if entry.is_dir {
                                        String::new()
                                    } else {
                                        format_size_short(entry.size)
                                    })
                                    .size(9.2),
                                );
                            });
                        }
                        2 => {
                            ui.add(
                                Label::new(RichText::new(&entry.kind).size(9.2)).truncate(),
                            );
                        }
                        3 => {
                            ui.add(
                                Label::new(RichText::new(&entry.modified).size(9.0)).truncate(),
                            );
                        }
                        4 => {
                            ui.label(
                                RichText::new(if entry.readonly { "R" } else { "-" }).size(9.0),
                            );
                        }
                        5 => {
                            ui.add(
                                Label::new(RichText::new(&entry.accessed).size(9.0)).truncate(),
                            );
                        }
                        6 => {
                            ui.add(
                                Label::new(RichText::new(&entry.created).size(9.0)).truncate(),
                            );
                        }
                        7 => {
                            ui.add(Label::new(RichText::new(&entry.name).size(9.0)).truncate());
                        }
                        _ => {
                            ui.add(
                                Label::new(
                                    RichText::new(entry.path.to_string_lossy()).size(8.5),
                                )
                                .truncate(),
                            );
                        }
                        }
                        row_click_overlay(ui, column + 1, &mut row_click);
                    });
                }
                let response = row_click.unwrap_or_else(|| row.response());
                if response.clicked() || response.secondary_clicked() {
                    new_selected = Some(entry.path.clone());
                }
                if response.double_clicked() {
                    open_entry = Some((entry.path.clone(), entry.is_dir));
                }
                response.context_menu(|ui| {
                    if ui.button("Open").clicked() {
                        open_entry = Some((entry.path.clone(), entry.is_dir));
                        ui.close_menu();
                    }
                    if ui.button("Open containing folder").clicked() {
                        action = Some(ExplorerAction::OpenParent(entry.path.clone()));
                        ui.close_menu();
                    }
                    if ui.button("Copy path").clicked() {
                        ui.ctx()
                            .copy_text(entry.path.to_string_lossy().to_string());
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("New folder").clicked() {
                        action = Some(ExplorerAction::NewFolder);
                        ui.close_menu();
                    }
                    if ui.button("Properties").clicked() {
                        action = Some(ExplorerAction::Properties(entry.path.clone()));
                        ui.close_menu();
                    }
                    if ui.button("Delete").clicked() {
                        action = Some(ExplorerAction::Delete(entry.path.clone()));
                        ui.close_menu();
                    }
                });
            });
        });

    if let Some(path) = new_selected {
        app.explorer_selected = Some(path);
    }
    if let Some((path, is_dir)) = open_entry {
        app.explorer_open_entry(path, is_dir);
    }
    match action {
        Some(ExplorerAction::OpenParent(path)) => app.explorer_open_containing(path),
        Some(ExplorerAction::NewFolder) => app.explorer_new_folder(),
        Some(ExplorerAction::Properties(path)) => app.explorer_properties(path),
        Some(ExplorerAction::Delete(path)) => app.explorer_delete(path),
        None => {}
    }
}

fn explorer_column_name(index: usize) -> &'static str {
    EXPLORER_COLUMN_NAMES
        .get(index)
        .copied()
        .unwrap_or("Path")
}

fn read_explorer(path: &std::path::Path, filter: &str) -> Vec<ExplorerEntry> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(path) else {
        return out;
    };
    let f = filter.to_lowercase();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !f.is_empty() && !name.to_lowercase().contains(&f) {
            continue;
        }
        let p = e.path();
        let md = e.metadata().ok();
        let is_dir = md.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let size = md.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified = md
            .as_ref()
            .and_then(|m| m.modified().ok())
            .map(|t| {
                chrono::DateTime::<chrono::Local>::from(t)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        let accessed = md
            .as_ref()
            .and_then(|m| m.accessed().ok())
            .map(|t| {
                chrono::DateTime::<chrono::Local>::from(t)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        let created = md
            .as_ref()
            .and_then(|m| m.created().ok())
            .map(|t| {
                chrono::DateTime::<chrono::Local>::from(t)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        let readonly = md
            .as_ref()
            .map(|m| m.permissions().readonly())
            .unwrap_or(false);
        let kind = if is_dir {
            "Folder".to_string()
        } else {
            p.extension()
                .map(|s| format!("{} file", s.to_string_lossy().to_uppercase()))
                .unwrap_or_else(|| "File".into())
        };
        out.push(ExplorerEntry {
            name,
            path: p,
            is_dir,
            size,
            kind,
            modified,
            accessed,
            created,
            readonly,
        });
    }
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

fn log_tab(app: &mut App, ui: &mut Ui) {
    egui::TopBottomPanel::top("log_toolbar")
        .exact_height(30.0)
        .frame(egui::Frame::none().fill(theme::BG_PALE))
        .show_inside(ui, |ui| {
            ui.painter().line_segment(
                [ui.max_rect().left_bottom(), ui.max_rect().right_bottom()],
                Stroke::new(1.0_f32, theme::BORDER_LIGHT),
            );
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                if ui.button("Clear").clicked() {
                    app.logs.clear();
                }
                if ui.button("Copy").clicked() {
                    let text = app
                        .logs
                        .iter()
                        .filter(|e| {
                            app.log_filter.is_empty()
                                || e.message
                                    .to_lowercase()
                                    .contains(&app.log_filter.to_lowercase())
                        })
                        .map(|e| format!("[{}] [{}] {}", e.time, e.level, e.message))
                        .collect::<Vec<_>>()
                        .join("\n");
                    ui.output_mut(|o| o.copied_text = text);
                    app.toast("Log copied");
                }
                ui.add_space(8.0);
                ui.label(RichText::new("Filter").size(9.5));
                ui.add_sized(
                    [180.0, 20.0],
                    TextEdit::singleline(&mut app.log_filter).hint_text("Filter log lines"),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("{} lines", app.logs.len()))
                            .size(9.0)
                            .color(theme::TEXT_MUTED),
                    );
                });
            });
        });
    let f = app.log_filter.to_lowercase();
    egui::ScrollArea::vertical()
        .id_salt("log_scroll")
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for e in app
                .logs
                .iter()
                .filter(|e| f.is_empty() || e.message.to_lowercase().contains(&f))
            {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(
                        RichText::new(&e.time)
                            .monospace()
                            .size(9.0)
                            .color(theme::TEXT_MUTED),
                    );
                    let color = match e.level.as_str() {
                        "Error" => theme::RED,
                        "Connection" => theme::BLUE,
                        "Scheduler" => theme::ORANGE,
                        _ => theme::TEXT_MUTED,
                    };
                    ui.add_sized(
                        [62.0, 14.0],
                        Label::new(RichText::new(&e.level).size(9.0).color(color)),
                    );
                    ui.add(Label::new(RichText::new(&e.message).size(9.5)));
                });
            }
        });
}

fn rss_tab(app: &mut App, ui: &mut Ui) {
    egui::TopBottomPanel::top("rss_toolbar")
        .exact_height(30.0)
        .frame(egui::Frame::none().fill(theme::BG_PALE))
        .show_inside(ui, |ui| {
            ui.painter().line_segment(
                [ui.max_rect().left_bottom(), ui.max_rect().right_bottom()],
                Stroke::new(1.0_f32, theme::BORDER_LIGHT),
            );
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                let r = theme::small_button(ui, Icon::Refresh, false);
                if r.clicked() {
                    let stamp = now_datetime();
                    for item in &mut app.rss {
                        item.date = stamp.clone();
                    }
                    app.log("RSS", "Feed list refreshed.");
                    app.toast("RSS refreshed");
                }
                r.on_hover_text("Refresh list");
                let r = theme::small_button(ui, Icon::Scheduler, app.rss_timer);
                if r.clicked() {
                    app.rss_timer = !app.rss_timer;
                    app.log(
                        "RSS",
                        if app.rss_timer {
                            "RSS timer enabled."
                        } else {
                            "RSS timer disabled."
                        },
                    );
                }
                r.on_hover_text("Enable/disable timer");
                let r = theme::small_button(ui, Icon::Filter, false);
                if r.clicked() {
                    app.show_filter_help = true;
                }
                r.on_hover_text("Show/hide filters");
                ui.add_space(8.0);
                ui.label(RichText::new("Filter").size(9.5));
                ui.add_sized(
                    [180.0, 20.0],
                    TextEdit::singleline(&mut app.rss_filter).hint_text("Filter names"),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Press Refresh to update the feed list")
                            .size(9.0)
                            .color(theme::TEXT_MUTED),
                    );
                });
            });
        });
    let f = app.rss_filter.to_lowercase();
    let items: Vec<_> = app
        .rss
        .iter()
        .filter(|i| {
            f.is_empty()
                || i.name.to_lowercase().contains(&f)
                || i.description.to_lowercase().contains(&f)
        })
        .cloned()
        .collect();
    let table = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(
            Column::initial(260.0)
                .at_least(100.0)
                .clip(true)
                .resizable(true),
        )
        .column(
            Column::initial(140.0)
                .at_least(80.0)
                .clip(true)
                .resizable(true),
        )
        .column(Column::remainder().at_least(150.0).clip(true));
    let table = table.header(20.0, |mut header| {
        for text in ["Name", "Date", "Description"] {
            header.col(|ui| {
                ui.painter().rect_filled(ui.max_rect(), 0.0, theme::BG);
                ui.add(Label::new(RichText::new(text).size(9.5)).truncate());
            });
        }
    });
    table.body(|body| {
        body.rows(22.0, items.len(), |mut row| {
            let i = row.index();
            let item = &items[i];
            row.col(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), Sense::hover());
                theme::paint_icon(ui.painter(), r, Icon::Rss, theme::TEXT);
                ui.add(Label::new(RichText::new(&item.name).size(9.5)).truncate());
            });
            row.col(|ui| {
                ui.add(Label::new(RichText::new(&item.date).size(9.2)).truncate());
            });
            row.col(|ui| {
                ui.add(Label::new(RichText::new(&item.description).size(9.2)).truncate());
            });
        });
    });
}

fn thread_panel(app: &mut App, ui: &mut Ui) {
    ui.painter()
        .rect_filled(ui.max_rect(), 0.0, theme::PANEL_WHITE);
    if app.threads.is_empty() {
        // Header only, matching a disconnected session.
        let rect = ui.max_rect();
        let header = egui::Rect::from_min_size(rect.min, Vec2::new(rect.width(), 20.0));
        ui.painter().rect_filled(header, 0.0, theme::BG);
        let cols = [
            ("#".to_string(), 20.0),
            ("Action".into(), 150.0),
            ("Description".into(), 300.0),
            ("Status".into(), 200.0),
            ("Server".into(), 100.0),
        ];
        let mut x = rect.left();
        for (text, w) in cols {
            ui.painter().text(
                egui::pos2(x + 5.0, header.center().y),
                Align2::LEFT_CENTER,
                text,
                FontId::proportional(9.5),
                theme::TEXT,
            );
            x += w;
        }
        ui.allocate_space(Vec2::new(rect.width(), rect.height()));
        return;
    }
    let table = TableBuilder::new(ui)
        .striped(false)
        .resizable(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::initial(24.0).at_least(20.0))
        .column(Column::initial(150.0).at_least(60.0).clip(true))
        .column(Column::remainder().at_least(120.0).clip(true))
        .column(Column::initial(200.0).at_least(80.0).clip(true))
        .column(Column::initial(130.0).at_least(70.0).clip(true));
    let table = table.header(20.0, |mut header| {
        for text in ["#", "Action", "Description", "Status", "Server"] {
            header.col(|ui| {
                ui.painter().rect_filled(ui.max_rect(), 0.0, theme::BG);
                ui.add(Label::new(RichText::new(text).size(9.5)).truncate());
            });
        }
    });
    let threads = &app.threads;
    table.body(|body| {
        body.rows(20.0, threads.len(), |mut row| {
            let i = row.index();
            let t = &threads[i];
            row.col(|ui| {
                ui.label(RichText::new(t.number.to_string()).size(9.0));
            });
            row.col(|ui| {
                ui.add(Label::new(RichText::new(&t.action).size(9.2)).truncate());
            });
            row.col(|ui| {
                ui.add(Label::new(RichText::new(&t.description).size(9.2)).truncate());
            });
            row.col(|ui| {
                let color = if t.status == "Downloading" {
                    theme::GREEN_DARK
                } else {
                    theme::TEXT_MUTED
                };
                ui.label(RichText::new(&t.status).size(9.2).color(color));
            });
            row.col(|ui| {
                ui.add(Label::new(RichText::new(&t.server).size(9.0)).truncate());
            });
        });
    });
}

fn status_bar(app: &mut App, ui: &mut Ui) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, theme::BG);
    ui.painter().line_segment(
        [rect.left_top(), rect.right_top()],
        Stroke::new(1.0_f32, theme::BORDER),
    );
    let height = rect.height();
    let mut x = rect.left();
    let limit_text = match app.speed_limit_tag {
        Some(_) => format_speed(app.speed_limit_bps(), app.status_mode_speed),
        None => "No limit".into(),
    };
    let panels: [(&str, String, f32, &str); 8] = [
        (
            "limit",
            limit_text,
            90.0,
            "Current bandwidth limit\nClick to set limit",
        ),
        (
            "speed",
            format_speed(app.speed_bps, app.status_mode_speed),
            90.0,
            "Current speed\nDouble click to change display mode",
        ),
        ("graph", String::new(), 120.0, "Speed graph (2 min)"),
        (
            "transfered",
            format_transfered_bytes(
                app.transfered_session as f64,
                app.status_mode_transfered,
            ),
            100.0,
            "Transfered bytes\nRight click to reset\nDouble click to change display mode",
        ),
        ("eta", eta_text(app), 100.0, "ETA"),
        (
            "queue",
            format_queue_bytes(app.total_queue_size() as f64, app.status_mode_queue),
            100.0,
            "Queue size\nDouble click to change display mode",
        ),
        (
            "free_dl",
            format!("Free: {:.2} GB", free_gb(&app.settings.download_dir)),
            100.0,
            "Free space on download drive",
        ),
        (
            "free_ur",
            format!("Free: {:.2} GB", free_gb(&app.settings.unrar_dir)),
            100.0,
            "Free space on unrar drive",
        ),
    ];
    for (i, (id, text, w, hint)) in panels.iter().enumerate() {
        let arenas = egui::Rect::from_min_size(egui::pos2(x, rect.top()), Vec2::new(*w, height));
        ui.painter().rect_filled(arenas, 0.0, theme::BG_LIGHT);
        ui.painter().line_segment(
            [arenas.right_top(), arenas.right_bottom()],
            Stroke::new(1.0_f32, theme::BORDER),
        );
        if *id == "graph" {
            draw_speed_graph(ui, arenas.shrink2(Vec2::new(3.0, 3.0)), app);
        } else {
            ui.painter().text(
                arenas.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(9.2),
                theme::TEXT,
            );
        }
        let resp = ui.interact(arenas, egui::Id::new(("status_panel", i)), Sense::click());
        if resp.clicked() && *id == "limit" {
            app.show_speed_limit = true;
        }
        if resp.double_clicked() {
            match *id {
                "speed" => {
                    app.status_mode_speed = (app.status_mode_speed + 1) % 4;
                    app.settings.speed_bytes_mode = app.status_mode_speed;
                }
                "queue" => {
                    app.status_mode_queue = (app.status_mode_queue + 1) % 4;
                    app.settings.queue_bytes_mode = app.status_mode_queue;
                }
                "transfered" => {
                    app.status_mode_transfered = (app.status_mode_transfered + 1) % 4;
                    app.settings.transfered_bytes_mode = app.status_mode_transfered;
                }
                _ => {}
            }
        }
        if resp.secondary_clicked() && *id == "transfered" {
            app.transfered_session = 0;
            app.toast("Transfered bytes reset");
        }
        resp.on_hover_text(*hint);
        x += *w;
    }
    // Spring panel fills what is left, with the thread toggle at the far right.
    let spring = egui::Rect::from_min_size(
        egui::pos2(x, rect.top()),
        Vec2::new((rect.right() - 22.0 - x).max(0.0), height),
    );
    ui.painter().rect_filled(spring, 0.0, theme::BG_LIGHT);
    let thread_rect = egui::Rect::from_min_size(
        egui::pos2(rect.right() - 22.0, rect.top()),
        Vec2::new(22.0, height),
    );
    ui.painter().rect_filled(thread_rect, 0.0, theme::BG_LIGHT);
    theme::paint_icon(
        ui.painter(),
        egui::Rect::from_center_size(thread_rect.center(), Vec2::new(14.0, 14.0)),
        Icon::Threads,
        theme::TEXT,
    );
    let resp = ui.interact(thread_rect, egui::Id::new("thread_toggle"), Sense::click());
    if resp.clicked() {
        app.show_threads = !app.show_threads;
        app.settings.thread_list_visible = app.show_threads;
    }
    resp.on_hover_text("Show/Hide connections panel [F9]");
    if let Some((text, _)) = &app.toast {
        if spring.width() > 60.0 {
            ui.painter().text(
                egui::pos2(spring.left() + 6.0, spring.center().y),
                Align2::LEFT_CENTER,
                text,
                FontId::proportional(9.2),
                theme::ACCENT,
            );
        }
    }
}

fn eta_text(app: &App) -> String {
    if app.speed_bps <= 0.0 {
        "Unknown".into()
    } else {
        format_eta(app.queue_remaining() / app.speed_bps)
    }
}

fn draw_speed_graph(ui: &Ui, rect: egui::Rect, app: &App) {
    let p = ui.painter();
    p.rect_filled(rect, 0.0, Color32::from_rgb(235, 245, 235));
    p.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0_f32, Color32::from_rgb(170, 190, 170)),
    );
    let max = (app.speed_ceiling_mbps * BPS_PER_MBPS).max(1.0);
    let n = app.speed_history.len();
    if n < 2 {
        return;
    }
    let mut points = Vec::with_capacity(n + 2);
    for (i, v) in app.speed_history.iter().enumerate() {
        let x = rect.left() + rect.width() * i as f32 / (n - 1) as f32;
        let y = rect.bottom() - rect.height() * (*v as f64 / max).clamp(0.0, 1.0) as f32;
        points.push(egui::pos2(x, y));
    }
    let mut fill = points.clone();
    fill.push(egui::pos2(rect.right(), rect.bottom()));
    fill.push(egui::pos2(rect.left(), rect.bottom()));
    p.add(egui::Shape::convex_polygon(
        fill,
        Color32::from_rgba_unmultiplied(92, 190, 92, 55),
        Stroke::NONE,
    ));
    p.add(egui::Shape::line(
        points,
        Stroke::new(1.2_f32, theme::GREEN_DARK),
    ));
}
