use crate::model::{TOOLBAR_BUTTON_HEIGHT, TOOLBAR_BUTTON_WIDTH};
use eframe::egui::{
    self, Align2, Color32, FontFamily, FontId, Pos2, Rect, Response, Rounding, Sense, Stroke, Ui,
    Vec2,
};
pub const BG: Color32 = Color32::from_rgb(214, 226, 242);
pub const BG_LIGHT: Color32 = Color32::from_rgb(230, 238, 248);
pub const BG_PALE: Color32 = Color32::from_rgb(242, 246, 252);
pub const BG_HOVER: Color32 = Color32::from_rgb(220, 234, 252);
pub const BG_ACTIVE: Color32 = Color32::from_rgb(196, 222, 250);
pub const PANEL_WHITE: Color32 = Color32::from_rgb(255, 255, 255);
pub const ROW_ALT: Color32 = Color32::from_rgb(247, 250, 255);
#[allow(dead_code)]
pub const ROW_GROUP: Color32 = Color32::from_rgb(239, 245, 252);
#[allow(dead_code)]
pub const ROW_SELECT: Color32 = Color32::from_rgb(204, 228, 255);
pub const TEXT: Color32 = Color32::from_rgb(26, 30, 38);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(90, 100, 115);
pub const TEXT_GROUP: Color32 = Color32::from_rgb(0, 0, 102);
pub const ACCENT: Color32 = Color32::from_rgb(0, 103, 192);
pub const BORDER: Color32 = Color32::from_rgb(170, 188, 212);
pub const BORDER_LIGHT: Color32 = Color32::from_rgb(214, 224, 238);
pub const GREEN: Color32 = Color32::from_rgb(92, 190, 92);
pub const GREEN_DARK: Color32 = Color32::from_rgb(52, 135, 62);
pub const ORANGE: Color32 = Color32::from_rgb(214, 138, 45);
pub const RED: Color32 = Color32::from_rgb(196, 64, 64);
pub const BLUE: Color32 = Color32::from_rgb(54, 118, 190);
pub const YELLOW: Color32 = Color32::from_rgb(240, 196, 64);
/// Segoe UI from the Windows font folder, falling back to egui's built-in
/// font when it is missing. Adds a `semibold` family for headings.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let dir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let load = |file: &str| std::fs::read(std::path::Path::new(&dir).join("Fonts").join(file)).ok();
    let mut add = |name: &str, file: &str| -> bool {
        match load(file) {
            Some(bytes) => {
                fonts
                    .font_data
                    .insert(name.into(), egui::FontData::from_owned(bytes));
                true
            }
            None => false,
        }
    };
    let regular = add("segoe", "segoeui.ttf");
    let semibold = add("segoe-semibold", "seguisb.ttf");
    let mono = add("consolas", "consola.ttf");
    let prop = fonts.families.entry(FontFamily::Proportional).or_default();
    if regular {
        prop.insert(0, "segoe".into());
    }
    let fallback = prop.clone();
    let mut bold = fallback.clone();
    if semibold {
        bold.insert(0, "segoe-semibold".into());
    }
    fonts.families.insert(FontFamily::Name("semibold".into()), bold);
    if mono {
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, "consolas".into());
    }
    ctx.set_fonts(fonts);
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

/// `fixed_scale` pins one pixel per point (deterministic screenshots);
/// otherwise the monitor's DPI scale is used.
pub fn install(ctx: &egui::Context, fixed_scale: bool) {
    install_fonts(ctx);
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = BG_LIGHT;
    style.visuals.extreme_bg_color = PANEL_WHITE;
    style.visuals.faint_bg_color = ROW_ALT;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.window_rounding = Rounding::same(6.0);
    style.visuals.menu_rounding = Rounding::same(4.0);
    style.visuals.window_stroke = Stroke::new(1.0_f32, BORDER);
    style.visuals.window_shadow = egui::epaint::Shadow { offset: Vec2::new(0.0, 4.0), blur: 16.0, spread: 0.0, color: Color32::from_black_alpha(40) };
    style.visuals.popup_shadow = egui::epaint::Shadow { offset: Vec2::new(0.0, 2.0), blur: 8.0, spread: 0.0, color: Color32::from_black_alpha(30) };
    for w in [&mut style.visuals.widgets.noninteractive, &mut style.visuals.widgets.inactive, &mut style.visuals.widgets.hovered, &mut style.visuals.widgets.active, &mut style.visuals.widgets.open] { w.rounding = Rounding::same(3.0); }
    style.visuals.selection.bg_fill = Color32::from_rgb(153, 200, 245);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, TEXT);
    style.visuals.widgets.noninteractive.bg_fill = BG;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER_LIGHT);
    style.visuals.widgets.inactive.bg_fill = BG_LIGHT;
    style.visuals.widgets.inactive.weak_bg_fill = BG_LIGHT;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    style.visuals.widgets.hovered.bg_fill = BG_HOVER;
    style.visuals.widgets.hovered.weak_bg_fill = BG_HOVER;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.active.bg_fill = BG_ACTIVE;
    style.visuals.widgets.active.weak_bg_fill = BG_ACTIVE;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.open.bg_fill = BG_PALE;
    style.spacing.item_spacing = Vec2::new(6.0, 4.0);
    style.spacing.button_padding = Vec2::new(10.0, 3.0);
    style.spacing.window_margin = egui::Margin::same(10.0);
    style.interaction.selectable_labels = false;
    style.spacing.interact_size.y = 22.0;
    style.text_styles.insert(
        egui::TextStyle::Body,
        FontId::new(12.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        FontId::new(12.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        FontId::new(10.5, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Heading,
        semibold(15.0),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        FontId::new(11.5, FontFamily::Monospace),
    );
    ctx.set_style(style);
    if fixed_scale {
        ctx.set_pixels_per_point(1.0);
    }
}
pub fn paint_toolbar_gradient(ui: &Ui, rect: Rect) {
    let p = ui.painter();
    let top = Color32::from_rgb(246, 249, 254);
    let mid = Color32::from_rgb(232, 240, 250);
    let bottom = BG;
    let steps = 18;
    for i in 0..steps {
        let t = i as f32 / (steps - 1) as f32;
        let color = if t < 0.5 {
            lerp_color(top, mid, t * 2.0)
        } else {
            lerp_color(mid, bottom, (t - 0.5) * 2.0)
        };
        let y0 = rect.top() + rect.height() * i as f32 / steps as f32;
        let y1 = rect.top() + rect.height() * (i + 1) as f32 / steps as f32;
        p.rect_filled(
            Rect::from_min_max(Pos2::new(rect.left(), y0), Pos2::new(rect.right(), y1)),
            0.0,
            color,
        );
    }
    p.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0_f32, BORDER),
    );
    p.line_segment(
        [
            rect.left_top() + Vec2::new(0.0, 1.0),
            rect.right_top() + Vec2::new(0.0, 1.0),
        ],
        Stroke::new(1.0_f32, Color32::WHITE),
    );
}
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
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(rect.shrink(1.0), Rounding::same(3.0), BG_HOVER);
        painter.rect_stroke(
            rect.shrink(1.0),
            Rounding::same(3.0),
            Stroke::new(1.0_f32, Color32::from_rgb(220, 235, 252)),
        );
    }
    if active {
        painter.rect_filled(rect.shrink(1.0), Rounding::same(3.0), BG_ACTIVE);
        painter.line_segment(
            [
                Pos2::new(rect.left() + 6.0, rect.bottom() - 2.0),
                Pos2::new(rect.right() - 6.0, rect.bottom() - 2.0),
            ],
            Stroke::new(2.0_f32, ACCENT),
        );
    }
    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.center().x, rect.top() + 18.0),
        Vec2::new(24.0, 24.0),
    );
    paint_icon(painter, icon_rect, icon, TEXT);
    let caption_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 1.0, rect.top() + 33.0),
        Pos2::new(rect.right() - 1.0, rect.bottom() - 2.0),
    );
    painter.text(
        caption_rect.center(),
        Align2::CENTER_CENTER,
        caption,
        FontId::proportional(11.0),
        TEXT,
    );
    resp
}
pub fn small_button(ui: &mut Ui, icon: Icon, active: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(26.0, 24.0), Sense::click());
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(rect.shrink(1.0), Rounding::same(3.0), BG_HOVER);
        painter.rect_stroke(
            rect.shrink(1.0),
            Rounding::same(3.0),
            Stroke::new(1.0_f32, BORDER_LIGHT),
        );
    }
    if active {
        painter.rect_filled(rect.shrink(1.0), Rounding::same(3.0), BG_ACTIVE);
    }
    paint_icon(
        painter,
        Rect::from_center_size(rect.center(), Vec2::new(16.0, 16.0)),
        icon,
        TEXT,
    );
    resp
}
pub fn tab_button(ui: &mut Ui, caption: &str, icon: Icon, selected: bool, width: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 26.0), Sense::click());
    let painter = ui.painter();
    let fill = if selected {
        Color32::from_rgb(232, 240, 250)
    } else if resp.hovered() {
        BG_HOVER
    } else {
        BG
    };
    painter.rect_filled(rect, Rounding::ZERO, fill);
    if selected {
        painter.line_segment(
            [
                Pos2::new(rect.left() + 2.0, rect.top() + 1.0),
                Pos2::new(rect.right() - 2.0, rect.top() + 1.0),
            ],
            Stroke::new(2.0_f32, Color32::from_rgb(245, 250, 255)),
        );
        painter.line_segment(
            [
                Pos2::new(rect.left(), rect.top()),
                Pos2::new(rect.left(), rect.bottom() - 1.0),
            ],
            Stroke::new(1.0_f32, BORDER_LIGHT),
        );
        painter.line_segment(
            [
                Pos2::new(rect.right(), rect.top()),
                Pos2::new(rect.right(), rect.bottom() - 1.0),
            ],
            Stroke::new(1.0_f32, BORDER_LIGHT),
        );
    }
    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 15.0, rect.center().y),
        Vec2::new(15.0, 15.0),
    );
    paint_icon(painter, icon_rect, icon, TEXT);
    painter.text(
        Pos2::new(rect.left() + 27.0, rect.center().y),
        Align2::LEFT_CENTER,
        caption,
        if selected { semibold(12.0) } else { FontId::proportional(12.0) },
        TEXT,
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
            painter.circle_filled(Pos2::new(c.x + s * 0.33, c.y + s * 0.33), s * 0.14, GREEN);
        }
        Icon::Pause => {
            let w = s * 0.14;
            let h = s * 0.48;
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(c.x - s * 0.16, c.y), Vec2::new(w, h)),
                Rounding::same(1.0),
                color,
            );
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(c.x + s * 0.16, c.y), Vec2::new(w, h)),
                Rounding::same(1.0),
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
            painter.rect_stroke(r, Rounding::same(1.0), stroke);
            let fold = s * 0.18;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(r.right() - fold, r.top()),
                    Pos2::new(r.right(), r.top() + fold),
                    Pos2::new(r.right() - fold, r.top() + fold),
                ],
                BG_LIGHT,
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
            painter.rect_stroke(r, Rounding::same(1.5), stroke);
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
                Rounding::same(1.0),
                stroke,
            );
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
            let dir = if up { -1.0 } else { 1.0 };
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
            let dir = if matches!(icon, Icon::Up) { -1.0 } else { 1.0 };
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
            painter.rect_stroke(r, Rounding::same(1.0), thin);
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
            painter.rect_stroke(r, Rounding::same(1.0), thin);
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
                    Stroke::new(stroke.width * 1.5, RED),
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
            painter.rect_filled(r, Rounding::same(1.5), YELLOW);
            painter.rect_stroke(
                r,
                Rounding::same(1.5),
                Stroke::new(1.0_f32, Color32::from_rgb(180, 140, 30)),
            );
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(r.left() + s * 0.03, r.top() - s * 0.10),
                    Pos2::new(r.left() + s * 0.28, r.top() + s * 0.02),
                ),
                Rounding::same(1.5),
                YELLOW,
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
                Rounding::same(1.0),
                stroke,
            );
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
                Stroke::new(stroke.width * 1.4, RED),
            );
            painter.line_segment(
                [
                    Pos2::new(c.x + s * 0.22, c.y - s * 0.22),
                    Pos2::new(c.x - s * 0.22, c.y + s * 0.22),
                ],
                Stroke::new(stroke.width * 1.4, RED),
            );
        }
        Icon::Open => {
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(0.0, s * 0.08), Vec2::new(s * 0.54, s * 0.40)),
                1.0,
                stroke,
            );
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
                    if k == 0 { ACCENT } else { color },
                );
            }
        }
        Icon::Lock => {
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(0.0, s * 0.10), Vec2::new(s * 0.40, s * 0.32)),
                1.0,
                stroke,
            );
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
    if color == TEXT {
        GREEN_DARK
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
    let p = ui.painter();
    let rect = rect.shrink2(Vec2::new(0.0, 1.0));
    p.rect_filled(rect, Rounding::same(3.0), Color32::from_rgb(238, 242, 248));
    let fill = if complete {
        Color32::from_rgb(200, 212, 226)
    } else {
        Color32::from_rgb(134, 196, 132)
    };
    let mut inner = rect;
    inner.set_width((inner.width() * fraction.clamp(0.0, 1.0)).max(0.0));
    if inner.width() > 0.5 {
        p.rect_filled(inner, Rounding::same(3.0), fill);
    }
    p.rect_stroke(
        rect,
        Rounding::same(3.0),
        Stroke::new(1.0_f32, Color32::from_rgb(186, 198, 214)),
    );
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(10.5),
        TEXT,
    );
}
#[allow(dead_code)]
pub fn section_header(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 20.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, BG);
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0_f32, BORDER_LIGHT),
    );
    ui.painter().text(
        Pos2::new(rect.left() + 6.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(10.5),
        TEXT,
    );
}
#[allow(dead_code)]
pub fn thin_separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, ui.available_height()), Sense::hover());
    ui.painter().line_segment(
        [rect.center_top(), rect.center_bottom()],
        Stroke::new(1.0_f32, BORDER_LIGHT),
    );
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
