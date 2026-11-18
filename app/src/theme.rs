use crate::model::{TOOLBAR_BUTTON_HEIGHT, TOOLBAR_BUTTON_WIDTH};
use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Response, CornerRadius, Sense, Stroke, Ui,
    Vec2,
};
#[allow(non_snake_case)]
pub fn BG() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0x22, 0x22, 0x25), Color32::from_rgb(0xf3, 0xf3, 0xf5))
}
#[allow(non_snake_case)]
pub fn BG_LIGHT() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0x2b, 0x2b, 0x2f), Color32::from_rgb(0xe7, 0xe7, 0xeb))
}
#[allow(non_snake_case)]
pub fn BG_PALE() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0x1a, 0x1a, 0x1c), Color32::from_rgb(0xe6, 0xe6, 0xea))
}
#[allow(non_snake_case)]
#[allow(dead_code)]
pub fn BG_HOVER() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0x2c, 0x2c, 0x30), Color32::from_rgb(0xec, 0xec, 0xf0))
}
#[allow(non_snake_case)]
#[allow(dead_code)]
pub fn BG_ACTIVE() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0x34, 0x34, 0x3a), Color32::from_rgb(0xdd, 0xdd, 0xe2))
}
/// The ground of lists and tables (recessed, darker than panels).
#[allow(non_snake_case)]
pub fn PANEL_WHITE() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0x1a, 0x1a, 0x1c), Color32::from_rgb(0xff, 0xff, 0xff))
}
#[allow(non_snake_case)]
#[allow(dead_code)]
pub fn ROW_ALT() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0x1e, 0x1e, 0x21), Color32::from_rgb(0xf7, 0xf7, 0xf9))
}
#[allow(dead_code)]
#[allow(non_snake_case)]
pub fn ROW_GROUP() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0x1f, 0x1f, 0x22), Color32::from_rgb(0xf4, 0xf4, 0xf6))
}
#[allow(dead_code)]
#[allow(non_snake_case)]
pub fn ROW_SELECT() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0x1d, 0x45, 0x4c), Color32::from_rgb(0xcd, 0xec, 0xf1))
}
#[allow(non_snake_case)]
pub fn TEXT() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0xe5, 0xe5, 0xe7), Color32::from_rgb(0x1c, 0x1c, 0x1f))
}
#[allow(non_snake_case)]
pub fn TEXT_MUTED() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0xa8, 0xa8, 0xaf), Color32::from_rgb(0x50, 0x50, 0x58))
}
#[allow(non_snake_case)]
pub fn TEXT_GROUP() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0x9a, 0xdf, 0xe8), Color32::from_rgb(0x0d, 0x6f, 0x7d))
}
#[allow(non_snake_case)]
pub fn ACCENT() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0x60, 0xca, 0xd8), Color32::from_rgb(0x12, 0x8c, 0x9e))
}
#[allow(non_snake_case)]
pub fn BORDER() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0x0c, 0x0c, 0x0d), Color32::from_rgb(0xa4, 0xa5, 0xac))
}
#[allow(non_snake_case)]
pub fn BORDER_LIGHT() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0x38, 0x38, 0x3d), Color32::from_rgb(0xd6, 0xd6, 0xdb))
}
#[allow(non_snake_case)]
pub fn GREEN() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0x6c, 0xc4, 0x6c), Color32::from_rgb(0x3d, 0x9a, 0x4a))
}
#[allow(non_snake_case)]
pub fn GREEN_DARK() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0x4c, 0xa0, 0x58), Color32::from_rgb(0x2f, 0x7d, 0x3a))
}
#[allow(non_snake_case)]
pub fn ORANGE() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0xe0, 0x9a, 0x48), Color32::from_rgb(0xc0, 0x7a, 0x20))
}
#[allow(non_snake_case)]
pub fn RED() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0xe0, 0x60, 0x60), Color32::from_rgb(0xc0, 0x3b, 0x3b))
}
#[allow(non_snake_case)]
pub fn BLUE() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0x6a, 0xa8, 0xe8), Color32::from_rgb(0x2a, 0x6f, 0xbe))
}
#[allow(non_snake_case)]
pub fn YELLOW() -> Color32 {
    crate::skin::pick(Color32::from_rgb(0xf0, 0xc8, 0x50), Color32::from_rgb(0xd9, 0xa5, 0x20))
}
pub fn semibold(size: f32) -> FontId {
    crate::skin::semibold(size)
}

/// The Instrument skin (`skin.rs`): IBM Plex over egui's fonts, dark visuals.
/// `fixed_scale` pins one pixel per point (deterministic screenshots);
/// otherwise the monitor's DPI scale is used.
pub fn install(ctx: &egui::Context, fixed_scale: bool) {
    let mut fonts = egui::FontDefinitions::default();
    crate::skin::fonts(&mut fonts);
    ctx.set_fonts(fonts);
    apply_visuals(ctx);
    if fixed_scale {
        ctx.set_pixels_per_point(1.0);
    }
}

/// Whether `setting` ("system", "light", "dark") means the light variant now.
pub fn wants_light(setting: &str, ctx: &egui::Context) -> bool {
    match setting {
        "light" => true,
        "dark" => false,
        _ => ctx.system_theme() == Some(egui::Theme::Light),
    }
}

/// Switches to the light or dark variant when it differs from the one in use.
pub fn follow(ctx: &egui::Context, setting: &str) {
    let light = wants_light(setting, ctx);
    if light != crate::skin::is_light() {
        crate::skin::set_light(light);
        apply_visuals(ctx);
        ctx.request_repaint();
    }
}

/// The egui style for the variant in use (`skin::is_light`).
fn apply_visuals(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    style.visuals = if crate::skin::is_light() {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    crate::skin::style(&mut style);
    style.visuals.override_text_color = Some(TEXT());
    style.interaction.selectable_labels = false;
    style.spacing.window_margin = egui::Margin::same(12);
    ctx.set_global_style(style);
}
pub fn paint_toolbar_gradient(ui: &Ui, rect: Rect) {
    crate::skin::top_bar_ground(ui.painter(), rect);
}
#[allow(dead_code)]
fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
    )
}
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Connect,
    Pause,
    PauseFor,
    Nzb,
    Download,
    Cleanup,
    Scheduler,
    Shutdown,
    Setup,
    Home,
    Version,
    About,
    Notes,
    Stats,
    Tray,
    Threads,
    Play,
    Stop,
    Top,
    Up,
    Down,
    Back,
    Forward,
    Bottom,
    Collapse,
    Expand,
    Unrar,
    NoUnrar,
    Key,
    Folder,
    Rename,
    Export,
    Eta,
    Filter,
    Refresh,
    Trash,
    Add,
    Check,
    Cross,
    Open,
    File,
    Rss,
    Shield,
    Queue,
    Page,
    Lock,
    Link,
}
pub fn tool_button(ui: &mut Ui, caption: &str, icon: Icon, active: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(TOOLBAR_BUTTON_WIDTH, TOOLBAR_BUTTON_HEIGHT),
        Sense::click(),
    );
    let dy = crate::skin::button_chrome(ui, rect.shrink(2.0), &resp, true, active);
    let color = if active { ACCENT() } else { TEXT() };
    let painter = ui.painter();
    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.center().x, rect.top() + 18.0 + dy),
        Vec2::new(22.0, 22.0),
    );
    paint_icon(painter, icon_rect, icon, color);
    painter.text(
        Pos2::new(rect.center().x, rect.top() + 41.0 + dy),
        Align2::CENTER_CENTER,
        caption,
        crate::skin::sans(12.0),
        if active { ACCENT() } else { crate::skin::DIM() },
    );
    resp
}
pub fn small_button(ui: &mut Ui, icon: Icon, active: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(26.0, 24.0), Sense::click());
    let dy = crate::skin::button_chrome(ui, rect, &resp, true, active);
    paint_icon(
        ui.painter(),
        Rect::from_center_size(rect.center() + Vec2::new(0.0, dy), Vec2::new(15.0, 15.0)),
        icon,
        if active { ACCENT() } else { TEXT() },
    );
    resp
}
pub fn tab_button(ui: &mut Ui, caption: &str, icon: Icon, selected: bool, width: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 26.0), Sense::click());
    let (dy, color) = crate::skin::tab_chrome(ui, rect, &resp, selected, true);
    let painter = ui.painter();
    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 15.0, rect.center().y + dy),
        Vec2::new(14.0, 14.0),
    );
    paint_icon(painter, icon_rect, icon, color);
    painter.text(
        Pos2::new(rect.left() + 27.0, rect.center().y + dy),
        Align2::LEFT_CENTER,
        caption,
        if selected { semibold(13.5) } else { crate::skin::sans(13.5) },
        if selected { TEXT() } else { color },
    );
    resp
}
pub fn paint_icon(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    let stroke = Stroke::new((s * 0.075).max(1.0), color);
    let thin = Stroke::new((s * 0.06).max(1.0), color);
    match icon {
        Icon::Connect => {
            painter.circle_stroke(c, s * 0.36, stroke);
            painter.line_segment(
                [
                    Pos2::new(c.x - s * 0.36, c.y),
                    Pos2::new(c.x + s * 0.36, c.y),
                ],
                thin,
            );
            painter.line_segment(
                [
                    Pos2::new(c.x, c.y - s * 0.36),
                    Pos2::new(c.x, c.y + s * 0.36),
                ],
                thin,
            );
            for k in [-0.18, 0.18] {
                let x = c.x + s * k;
                let dy = (s * 0.36 * s * 0.36 - (s * k) * (s * k)).sqrt();
                painter.line_segment([Pos2::new(x, c.y - dy), Pos2::new(x, c.y + dy)], thin);
            }
            painter.circle_filled(Pos2::new(c.x + s * 0.33, c.y + s * 0.33), s * 0.14, GREEN());
        }
        Icon::Pause => {
            let w = s * 0.14;
            let h = s * 0.48;
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(c.x - s * 0.16, c.y), Vec2::new(w, h)),
                CornerRadius::same(1),
                color,
            );
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(c.x + s * 0.16, c.y), Vec2::new(w, h)),
                CornerRadius::same(1),
                color,
            );
        }
        Icon::PauseFor => {
            let w = s * 0.12;
            let h = s * 0.44;
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(c.x - s * 0.15, c.y - s * 0.06), Vec2::new(w, h)),
                1.0,
                color,
            );
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(c.x + s * 0.15, c.y - s * 0.06), Vec2::new(w, h)),
                1.0,
                color,
            );
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(c.x - s * 0.2, c.y + s * 0.27),
                    Pos2::new(c.x + s * 0.2, c.y + s * 0.27),
                    Pos2::new(c.x, c.y + s * 0.42),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Nzb | Icon::Page | Icon::Notes | Icon::Export | Icon::File => {
            let r = Rect::from_center_size(c, Vec2::new(s * 0.56, s * 0.68));
            painter.rect_stroke(r, CornerRadius::same(1), stroke, egui::StrokeKind::Inside);
            let fold = s * 0.18;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(r.right() - fold, r.top()),
                    Pos2::new(r.right(), r.top() + fold),
                    Pos2::new(r.right() - fold, r.top() + fold),
                ],
                BG_LIGHT(),
                thin,
            ));
            if matches!(icon, Icon::Nzb) {
                painter.text(
                    c + Vec2::new(0.0, s * 0.08),
                    Align2::CENTER_CENTER,
                    "N",
                    FontId::proportional(s * 0.30),
                    color,
                );
            } else if matches!(icon, Icon::Export) {
                painter.line_segment(
                    [
                        Pos2::new(c.x - s * 0.20, c.y + s * 0.10),
                        Pos2::new(c.x + s * 0.18, c.y + s * 0.10),
                    ],
                    thin,
                );
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        Pos2::new(c.x + s * 0.06, c.y - s * 0.02),
                        Pos2::new(c.x + s * 0.22, c.y + s * 0.10),
                        Pos2::new(c.x + s * 0.06, c.y + s * 0.22),
                    ],
                    color,
                    Stroke::NONE,
                ));
            } else if matches!(icon, Icon::Notes) {
                for k in 0..3 {
                    let y = c.y + s * (k as f32 - 1.0) * 0.13;
                    painter.line_segment(
                        [Pos2::new(c.x - s * 0.20, y), Pos2::new(c.x + s * 0.18, y)],
                        thin,
                    );
                }
            }
        }
        Icon::Download | Icon::Tray => {
            let r =
                Rect::from_center_size(c + Vec2::new(0.0, s * 0.10), Vec2::new(s * 0.60, s * 0.34));
            painter.rect_stroke(r, CornerRadius::same(1), stroke, egui::StrokeKind::Inside);
            painter.line_segment(
                [
                    Pos2::new(c.x, c.y - s * 0.34),
                    Pos2::new(c.x, c.y + s * 0.04),
                ],
                stroke,
            );
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(c.x - s * 0.16, c.y - s * 0.02),
                    Pos2::new(c.x + s * 0.16, c.y - s * 0.02),
                    Pos2::new(c.x, c.y + s * 0.18),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Cleanup => {
            painter.line_segment(
                [
                    Pos2::new(c.x + s * 0.22, c.y - s * 0.34),
                    Pos2::new(c.x - s * 0.04, c.y + s * 0.04),
                ],
                stroke,
            );
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(c.x - s * 0.24, c.y + s * 0.02),
                    Pos2::new(c.x + s * 0.08, c.y + s * 0.02),
                    Pos2::new(c.x + s * 0.18, c.y + s * 0.30),
                    Pos2::new(c.x - s * 0.14, c.y + s * 0.30),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Scheduler | Icon::Eta => {
            painter.circle_stroke(c, s * 0.34, stroke);
            painter.line_segment([c, Pos2::new(c.x, c.y - s * 0.20)], thin);
            painter.line_segment([c, Pos2::new(c.x + s * 0.16, c.y + s * 0.08)], thin);
        }
        Icon::Shutdown => {
            painter.circle_stroke(c + Vec2::new(0.0, s * 0.05), s * 0.30, stroke);
            painter.line_segment(
                [
                    Pos2::new(c.x, c.y - s * 0.36),
                    Pos2::new(c.x, c.y - s * 0.02),
                ],
                Stroke::new(stroke.width * 1.3, color),
            );
        }
        Icon::Setup => {
            painter.circle_stroke(c, s * 0.22, stroke);
            painter.circle_stroke(c, s * 0.09, thin);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                painter.line_segment(
                    [
                        Pos2::new(c.x + a.cos() * s * 0.30, c.y + a.sin() * s * 0.30),
                        Pos2::new(c.x + a.cos() * s * 0.40, c.y + a.sin() * s * 0.40),
                    ],
                    Stroke::new(stroke.width * 1.3, color),
                );
            }
        }
        Icon::Home => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(c.x - s * 0.36, c.y - s * 0.02),
                    Pos2::new(c.x, c.y - s * 0.34),
                    Pos2::new(c.x + s * 0.36, c.y - s * 0.02),
                ],
                color,
                Stroke::NONE,
            ));
            painter.rect_stroke(
                Rect::from_min_max(
                    Pos2::new(c.x - s * 0.24, c.y + s * 0.00),
                    Pos2::new(c.x + s * 0.24, c.y + s * 0.28),
                ),
                CornerRadius::same(1),
                stroke,
            egui::StrokeKind::Inside);
        }
        Icon::Version => {
            painter.text(
                c,
                Align2::CENTER_CENTER,
                "Z",
                FontId::proportional(s * 0.80),
                color,
            );
        }
        Icon::About => {
            painter.circle_stroke(c, s * 0.34, stroke);
            painter.text(
                c + Vec2::new(0.0, s * 0.02),
                Align2::CENTER_CENTER,
                "i",
                FontId::proportional(s * 0.50),
                color,
            );
        }
        Icon::Stats => {
            let base = c.y + s * 0.28;
            for (i, h) in [0.20, 0.34, 0.48].iter().enumerate() {
                let x = c.x + (i as f32 - 1.0) * s * 0.22;
                painter.rect_filled(
                    Rect::from_min_max(
                        Pos2::new(x - s * 0.07, base - s * h),
                        Pos2::new(x + s * 0.07, base),
                    ),
                    0.0,
                    color,
                );
            }
        }
        Icon::Threads => {
            for k in 0..3 {
                let y = c.y + (k as f32 - 1.0) * s * 0.18;
                painter.circle_filled(Pos2::new(c.x - s * 0.26, y), s * 0.04, color);
                painter.line_segment(
                    [Pos2::new(c.x - s * 0.14, y), Pos2::new(c.x + s * 0.28, y)],
                    thin,
                );
            }
        }
        Icon::Play => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(c.x - s * 0.20, c.y - s * 0.30),
                    Pos2::new(c.x + s * 0.30, c.y),
                    Pos2::new(c.x - s * 0.20, c.y + s * 0.30),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Stop => {
            painter.rect_filled(
                Rect::from_center_size(c, Vec2::new(s * 0.46, s * 0.46)),
                1.0,
                color,
            );
        }
        Icon::Top | Icon::Bottom => {
            let up = matches!(icon, Icon::Top);
            let dir = if up { 1.0 } else { -1.0 };
            painter.line_segment(
                [
                    Pos2::new(c.x, c.y - dir * s * 0.28),
                    Pos2::new(c.x, c.y + dir * s * 0.10),
                ],
                stroke,
            );
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(c.x - s * 0.16, c.y - dir * s * 0.06),
                    Pos2::new(c.x + s * 0.16, c.y - dir * s * 0.06),
                    Pos2::new(c.x, c.y - dir * s * 0.28),
                ],
                color,
                Stroke::NONE,
            ));
            let y = c.y - dir * s * 0.34;
            painter.line_segment(
                [Pos2::new(c.x - s * 0.22, y), Pos2::new(c.x + s * 0.22, y)],
                Stroke::new(stroke.width * 1.3, color),
            );
        }
        Icon::Up | Icon::Down => {
            let dir = if matches!(icon, Icon::Up) { 1.0 } else { -1.0 };
            painter.line_segment(
                [
                    Pos2::new(c.x, c.y - dir * s * 0.26),
                    Pos2::new(c.x, c.y + dir * s * 0.22),
                ],
                stroke,
            );
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(c.x - s * 0.17, c.y + dir * s * 0.02),
                    Pos2::new(c.x + s * 0.17, c.y + dir * s * 0.02),
                    Pos2::new(c.x, c.y - dir * s * 0.26),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Back | Icon::Forward => {
            let dir = if matches!(icon, Icon::Back) { -1.0 } else { 1.0 };
            painter.line_segment(
                [
                    Pos2::new(c.x - dir * s * 0.24, c.y),
                    Pos2::new(c.x + dir * s * 0.18, c.y),
                ],
                stroke,
            );
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(c.x - dir * s * 0.24, c.y),
                    Pos2::new(c.x + dir * s * 0.02, c.y - s * 0.17),
                    Pos2::new(c.x + dir * s * 0.02, c.y + s * 0.17),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Collapse | Icon::Expand => {
            let r = Rect::from_center_size(c, Vec2::new(s * 0.46, s * 0.46));
            painter.rect_stroke(r, CornerRadius::same(1), thin, egui::StrokeKind::Inside);
            painter.line_segment(
                [
                    Pos2::new(c.x - s * 0.14, c.y),
                    Pos2::new(c.x + s * 0.14, c.y),
                ],
                stroke,
            );
            if matches!(icon, Icon::Expand) {
                painter.line_segment(
                    [
                        Pos2::new(c.x, c.y - s * 0.14),
                        Pos2::new(c.x, c.y + s * 0.14),
                    ],
                    stroke,
                );
            }
        }
        Icon::Unrar | Icon::NoUnrar => {
            let r =
                Rect::from_center_size(c + Vec2::new(0.0, s * 0.08), Vec2::new(s * 0.50, s * 0.34));
            painter.rect_stroke(r, CornerRadius::same(1), thin, egui::StrokeKind::Inside);
            painter.line_segment(
                [
                    Pos2::new(c.x, c.y - s * 0.02),
                    Pos2::new(c.x, c.y - s * 0.30),
                ],
                stroke,
            );
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(c.x - s * 0.14, c.y - s * 0.16),
                    Pos2::new(c.x + s * 0.14, c.y - s * 0.16),
                    Pos2::new(c.x, c.y - s * 0.34),
                ],
                color,
                Stroke::NONE,
            ));
            if matches!(icon, Icon::NoUnrar) {
                painter.line_segment(
                    [
                        Pos2::new(c.x - s * 0.28, c.y + s * 0.28),
                        Pos2::new(c.x + s * 0.28, c.y - s * 0.28),
                    ],
                    Stroke::new(stroke.width * 1.5, RED()),
                );
            }
        }
        Icon::Key => {
            painter.circle_stroke(c + Vec2::new(-s * 0.14, 0.0), s * 0.12, stroke);
            painter.line_segment(
                [
                    Pos2::new(c.x - s * 0.02, c.y),
                    Pos2::new(c.x + s * 0.30, c.y),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(c.x + s * 0.20, c.y),
                    Pos2::new(c.x + s * 0.20, c.y + s * 0.12),
                ],
                thin,
            );
        }
        Icon::Folder => {
            let r =
                Rect::from_center_size(c + Vec2::new(0.0, s * 0.05), Vec2::new(s * 0.60, s * 0.40));
            painter.rect_filled(r, CornerRadius::same(1), YELLOW());
            painter.rect_stroke(
                r,
                CornerRadius::same(1),
                Stroke::new(1.0_f32, Color32::from_rgb(180, 140, 30)),
            egui::StrokeKind::Inside);
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(r.left() + s * 0.03, r.top() - s * 0.10),
                    Pos2::new(r.left() + s * 0.28, r.top() + s * 0.02),
                ),
                CornerRadius::same(1),
                YELLOW(),
            );
        }
        Icon::Rename => {
            painter.line_segment(
                [
                    Pos2::new(c.x - s * 0.28, c.y + s * 0.26),
                    Pos2::new(c.x + s * 0.22, c.y - s * 0.24),
                ],
                Stroke::new(stroke.width * 1.5, color),
            );
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(c.x - s * 0.32, c.y + s * 0.30),
                    Pos2::new(c.x - s * 0.22, c.y + s * 0.28),
                    Pos2::new(c.x - s * 0.28, c.y + s * 0.18),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Filter => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(c.x - s * 0.32, c.y - s * 0.24),
                    Pos2::new(c.x + s * 0.32, c.y - s * 0.24),
                    Pos2::new(c.x + s * 0.06, c.y + s * 0.02),
                    Pos2::new(c.x + s * 0.06, c.y + s * 0.28),
                    Pos2::new(c.x - s * 0.06, c.y + s * 0.22),
                    Pos2::new(c.x - s * 0.06, c.y + s * 0.02),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Refresh => {
            painter.circle_stroke(c, s * 0.28, stroke);
            painter.line_segment(
                [
                    Pos2::new(c.x + s * 0.18, c.y - s * 0.34),
                    Pos2::new(c.x + s * 0.34, c.y - s * 0.12),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(c.x + s * 0.18, c.y - s * 0.34),
                    Pos2::new(c.x + s * 0.02, c.y - s * 0.28),
                ],
                stroke,
            );
        }
        Icon::Trash => {
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(0.0, s * 0.10), Vec2::new(s * 0.38, s * 0.44)),
                CornerRadius::same(1),
                stroke,
            egui::StrokeKind::Inside);
            painter.line_segment(
                [
                    Pos2::new(c.x - s * 0.28, c.y - s * 0.14),
                    Pos2::new(c.x + s * 0.28, c.y - s * 0.14),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(c.x - s * 0.10, c.y - s * 0.26),
                    Pos2::new(c.x + s * 0.10, c.y - s * 0.26),
                ],
                stroke,
            );
            for k in [-0.12, 0.0, 0.12] {
                painter.line_segment(
                    [
                        Pos2::new(c.x + s * k, c.y - s * 0.04),
                        Pos2::new(c.x + s * k, c.y + s * 0.26),
                    ],
                    thin,
                );
            }
        }
        Icon::Add => {
            painter.line_segment(
                [
                    Pos2::new(c.x - s * 0.28, c.y),
                    Pos2::new(c.x + s * 0.28, c.y),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(c.x, c.y - s * 0.28),
                    Pos2::new(c.x, c.y + s * 0.28),
                ],
                stroke,
            );
        }
        Icon::Check => {
            painter.line_segment(
                [
                    Pos2::new(c.x - s * 0.26, c.y + s * 0.02),
                    Pos2::new(c.x - s * 0.05, c.y + s * 0.24),
                ],
                Stroke::new(stroke.width * 1.4, green_or(color)),
            );
            painter.line_segment(
                [
                    Pos2::new(c.x - s * 0.05, c.y + s * 0.24),
                    Pos2::new(c.x + s * 0.30, c.y - s * 0.24),
                ],
                Stroke::new(stroke.width * 1.4, green_or(color)),
            );
        }
        Icon::Cross => {
            painter.line_segment(
                [
                    Pos2::new(c.x - s * 0.22, c.y - s * 0.22),
                    Pos2::new(c.x + s * 0.22, c.y + s * 0.22),
                ],
                Stroke::new(stroke.width * 1.4, RED()),
            );
            painter.line_segment(
                [
                    Pos2::new(c.x + s * 0.22, c.y - s * 0.22),
                    Pos2::new(c.x - s * 0.22, c.y + s * 0.22),
                ],
                Stroke::new(stroke.width * 1.4, RED()),
            );
        }
        Icon::Open => {
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(0.0, s * 0.08), Vec2::new(s * 0.54, s * 0.40)),
                1.0,
                stroke,
            egui::StrokeKind::Inside);
            painter.line_segment(
                [
                    Pos2::new(c.x + s * 0.06, c.y - s * 0.30),
                    Pos2::new(c.x + s * 0.32, c.y - s * 0.06),
                ],
                stroke,
            );
            painter.add(egui::Shape::line(
                vec![
                    Pos2::new(c.x + s * 0.20, c.y - s * 0.30),
                    Pos2::new(c.x + s * 0.34, c.y - s * 0.30),
                    Pos2::new(c.x + s * 0.34, c.y - s * 0.16),
                ],
                stroke,
            ));
        }
        Icon::Rss => {
            painter.circle_filled(Pos2::new(c.x - s * 0.24, c.y + s * 0.24), s * 0.06, color);
            for r in [0.18, 0.34, 0.50] {
                draw_arc(
                    painter,
                    Pos2::new(c.x - s * 0.24, c.y + s * 0.24),
                    s * r,
                    -std::f32::consts::FRAC_PI_2,
                    0.0,
                    stroke,
                );
            }
        }
        Icon::Shield => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(c.x, c.y - s * 0.34),
                    Pos2::new(c.x + s * 0.30, c.y - s * 0.20),
                    Pos2::new(c.x + s * 0.22, c.y + s * 0.20),
                    Pos2::new(c.x, c.y + s * 0.34),
                    Pos2::new(c.x - s * 0.22, c.y + s * 0.20),
                    Pos2::new(c.x - s * 0.30, c.y - s * 0.20),
                ],
                Color32::TRANSPARENT,
                stroke,
            ));
            painter.line_segment(
                [
                    Pos2::new(c.x - s * 0.12, c.y),
                    Pos2::new(c.x - s * 0.02, c.y + s * 0.12),
                ],
                Stroke::new(stroke.width * 1.2, color),
            );
            painter.line_segment(
                [
                    Pos2::new(c.x - s * 0.02, c.y + s * 0.12),
                    Pos2::new(c.x + s * 0.16, c.y - s * 0.12),
                ],
                Stroke::new(stroke.width * 1.2, color),
            );
        }
        Icon::Queue => {
            for k in 0..3 {
                let y = c.y + (k as f32 - 1.0) * s * 0.18;
                painter.rect_filled(
                    Rect::from_min_max(
                        Pos2::new(c.x - s * 0.30, y - s * 0.05),
                        Pos2::new(c.x + s * 0.30, y + s * 0.05),
                    ),
                    0.0,
                    if k == 0 { ACCENT() } else { color },
                );
            }
        }
        Icon::Lock => {
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(0.0, s * 0.10), Vec2::new(s * 0.40, s * 0.32)),
                1.0,
                stroke,
            egui::StrokeKind::Inside);
            draw_arc(
                painter,
                Pos2::new(c.x, c.y - s * 0.04),
                s * 0.16,
                std::f32::consts::PI,
                std::f32::consts::TAU,
                stroke,
            );
            painter.circle_filled(c + Vec2::new(0.0, s * 0.10), s * 0.04, color);
        }
        Icon::Link => {
            painter.circle_stroke(c + Vec2::new(-s * 0.14, 0.0), s * 0.14, stroke);
            painter.circle_stroke(c + Vec2::new(s * 0.14, 0.0), s * 0.14, stroke);
            painter.line_segment(
                [
                    Pos2::new(c.x - s * 0.04, c.y),
                    Pos2::new(c.x + s * 0.04, c.y),
                ],
                stroke,
            );
        }
    }
}
fn green_or(color: Color32) -> Color32 {
    if color == TEXT() {
        GREEN_DARK()
    } else {
        color
    }
}
pub fn draw_arc(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    start: f32,
    end: f32,
    stroke: Stroke,
) {
    let mut points = Vec::new();
    let steps = 14;
    for i in 0..=steps {
        let t = start + (end - start) * i as f32 / steps as f32;
        points.push(Pos2::new(
            center.x + radius * t.cos(),
            center.y + radius * t.sin(),
        ));
    }
    painter.add(egui::Shape::line(points, stroke));
}
pub fn progress_bar(ui: &mut Ui, fraction: f32, text: &str, complete: bool) {
    let desired = Vec2::new(ui.available_width().max(20.0), 15.0);
    let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
    let rect = rect.shrink2(Vec2::new(0.0, 1.0));
    crate::skin::recess(ui, rect);
    let fill = if complete {
        crate::skin::pick(Color32::from_rgb(0x3a, 0x3a, 0x3f), Color32::from_rgb(0xc8, 0xc9, 0xcf))
    } else {
        crate::skin::accent_fill()
    };
    let mut inner = rect.shrink(1.0);
    inner.set_width((inner.width() * fraction.clamp(0.0, 1.0)).max(0.0));
    let p = ui.painter();
    if inner.width() > 0.5 {
        p.rect_filled(inner, CornerRadius::same(2), fill);
    }
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        crate::skin::mono(10.5),
        TEXT(),
    );
}
#[allow(dead_code)]
pub fn section_header(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
    crate::skin::band_ground(ui.painter(), rect);
    ui.painter().text(
        Pos2::new(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        text.to_uppercase(),
        semibold(12.0),
        crate::skin::DIM(),
    );
}
#[allow(dead_code)]
pub fn thin_separator(ui: &mut Ui) {
    crate::skin::rule_v(ui, ui.available_height());
}

/// The folder a Browse dialog opens in: the current value when it exists,
/// else its nearest existing parent.
pub fn start_dir(value: &str) -> std::path::PathBuf {
    let mut path = std::path::PathBuf::from(value.trim());
    while !path.as_os_str().is_empty() && !path.is_dir() {
        if !path.pop() {
            break;
        }
    }
    path
}
