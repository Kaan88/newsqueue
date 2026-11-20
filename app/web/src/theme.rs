//! Independent web skin and brightness preferences, palettes, fonts and icons.
//! Classic retains its original renderer and DejaVu metrics. Caustica uses the
//! Instrument recipes and Inter without changing the app layout.

use egui::{Color32, FontFamily, FontId, Painter, Pos2, Rect, Shape, Stroke, pos2};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme { Classic, Caustica, CausticaCopper }
impl Theme {
    pub fn from_saved(value: Option<&str>) -> Self { match value { Some("classic") => Self::Classic, Some("caustica-copper") => Self::CausticaCopper, _ => Self::Caustica } }
    pub fn key(self) -> &'static str { match self { Self::Classic => "classic", Self::Caustica => "caustica", Self::CausticaCopper => "caustica-copper" } }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Brightness { System, Light, Dark }
impl Brightness {
    pub fn from_saved(value: Option<&str>) -> Self { match value { Some("light") => Self::Light, Some("dark") => Self::Dark, _ => Self::System } }
    pub fn key(self) -> &'static str { match self { Self::System => "system", Self::Light => "light", Self::Dark => "dark" } }
    pub fn dark(self, system: bool) -> bool { match self { Self::System => system, Self::Light => false, Self::Dark => true } }
}
static CAUSTICA: AtomicBool = AtomicBool::new(false);
pub fn is_caustica() -> bool { CAUSTICA.load(Ordering::Relaxed) }
pub fn configure(ctx: &egui::Context, theme: Theme, dark: bool) {
    let palette_changed = crate::caustica_icons::configure(theme == Theme::CausticaCopper);
    let changed = CAUSTICA.swap(theme != Theme::Classic, Ordering::Relaxed) != (theme != Theme::Classic);
    crate::caustica::set_light(!dark);
    if changed || palette_changed { install(ctx, dark); } else { set_dark(ctx, dark); }
}
const CAUSTICA_LIGHT_WARNING: Color32 = rgb(0x98,0x52,0x00);
fn color(name: &str, light: Color32, dark: Color32) -> Color32 {
    if !is_caustica() { return pick(light, dark); }
    if let Some(c) = crate::caustica_icons::color(name) { return c; }
    use crate::caustica as c;
    match name {
        "FACE" => c::PANEL(), "HILIGHT" | "LIGHT" => c::LIGHT_LINE(),
        "SHADOW" | "DKSHADOW" => c::EDGE(), "WINDOW" => pick(rgb(0xfa,0xfa,0xfc),rgb(0x1a,0x1a,0x1c)),
        "TEXT" => c::TX(), "GRAY_TEXT" => c::FAINT(), "NOTE_TEXT" => c::DIM(),
        "SEL" => pick(rgb(0xc6,0xe8,0xed),rgb(0x1d,0x45,0x4c)), "SEL_TEXT" => c::TX(),
        "CAPTION_A" => pick(rgb(0x16,0x62,0x70),rgb(0x2b,0x2b,0x2f)),
        "CAPTION_B" => pick(rgb(0x22,0x76,0x82),rgb(0x22,0x22,0x25)),
        "TRACK" => c::DEEP(), "BAR" | "LINK" => c::ACCENT(),
        "ORANGE" => pick(CAUSTICA_LIGHT_WARNING, dark),
        _ => pick(light, dark),
    }
}

static DARK: AtomicBool = AtomicBool::new(false);

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

/// `light` normally, `dark` with dark colours on.
pub fn pick(light: Color32, dark: Color32) -> Color32 {
    if is_dark() { dark } else { light }
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

macro_rules! colors {
    ($($(#[$m:meta])* $name:ident = $light:expr, $dark:expr;)*) => {
        $( $(#[$m])* #[allow(non_snake_case)] pub fn $name() -> Color32 { color(stringify!($name), $light, $dark) } )*
    };
}

colors! {
    /// 3D face: dialogs, bars, buttons.
    FACE = rgb(0xD4, 0xD0, 0xC8), rgb(0x3C, 0x3C, 0x3E);
    /// Outer highlight of raised edges.
    HILIGHT = rgb(0xFF, 0xFF, 0xFF), rgb(0x66, 0x66, 0x6A);
    /// Inner highlight of raised edges.
    LIGHT = rgb(0xE4, 0xE1, 0xDA), rgb(0x4C, 0x4C, 0x50);
    SHADOW = rgb(0x80, 0x80, 0x80), rgb(0x26, 0x26, 0x28);
    DKSHADOW = rgb(0x40, 0x40, 0x40), rgb(0x0E, 0x0E, 0x10);
    /// Lists and edit fields.
    WINDOW = rgb(0xFF, 0xFF, 0xFF), rgb(0x1E, 0x1E, 0x20);
    TEXT = rgb(0x00, 0x00, 0x00), rgb(0xE8, 0xE8, 0xE8);
    GRAY_TEXT = rgb(0x80, 0x80, 0x80), rgb(0x8E, 0x8E, 0x92);
    // Explanatory notes: secondary but readable on the face (gray text is
    // for disabled controls).
    NOTE_TEXT = rgb(0x50, 0x50, 0x50), rgb(0xB0, 0xB0, 0xB4);
    SEL = rgb(0x0A, 0x24, 0x6A), rgb(0x2F, 0x5A, 0x9E);
    SEL_TEXT = rgb(0xFF, 0xFF, 0xFF), rgb(0xFF, 0xFF, 0xFF);
    CAPTION_A = rgb(0x0A, 0x24, 0x6A), rgb(0x1C, 0x3A, 0x72);
    CAPTION_B = rgb(0xA6, 0xCA, 0xF0), rgb(0x4A, 0x72, 0xAE);
    /// Scroll bar tracks and checked toolbar buttons (the classic dither).
    TRACK = rgb(0xEC, 0xEA, 0xE6), rgb(0x2C, 0x2C, 0x2E);
    INFO = rgb(0xFF, 0xFF, 0xE1), rgb(0x4A, 0x48, 0x30);
    LINK = rgb(0x00, 0x00, 0xCC), rgb(0x8C, 0xB4, 0xFF);
    GREEN = rgb(0x00, 0x80, 0x00), rgb(0x6C, 0xC4, 0x6C);
    RED = rgb(0xC0, 0x00, 0x00), rgb(0xF0, 0x70, 0x70);
    ORANGE = rgb(0xCC, 0x78, 0x00), rgb(0xE8, 0xA0, 0x48);
    BLUE = rgb(0x00, 0x50, 0xA0), rgb(0x78, 0xA8, 0xE8);
    /// The fill of a running progress bar.
    BAR = rgb(0x0A, 0x24, 0x6A), rgb(0x3A, 0x6A, 0xB4);
}

pub const BODY: f32 = 12.0;
pub const SMALL: f32 = 11.0;

pub fn font(size: f32) -> FontId {
    if is_caustica() { crate::caustica::sans(size + 2.0) } else { FontId::proportional(size) }
}

pub fn bold(size: f32) -> FontId {
    if is_caustica() { crate::caustica::semibold(size + 2.0) } else { FontId::new(size, FontFamily::Name("bold".into())) }
}

pub fn mono(size: f32) -> FontId {
    if is_caustica() { crate::caustica::mono(size) } else { FontId::monospace(size) }
}

fn fonts() -> egui::FontDefinitions {
    use std::sync::Arc;
    let mut f = egui::FontDefinitions::default();
    let add = |f: &mut egui::FontDefinitions, name: &str, bytes: &'static [u8]| {
        f.font_data.insert(name.into(), Arc::new(egui::FontData::from_static(bytes)));
    };
    add(&mut f, "dejavu", include_bytes!("../../assets/fonts/DejaVuSans.ttf"));
    add(&mut f, "dejavu-bold", include_bytes!("../../assets/fonts/DejaVuSans-Bold.ttf"));
    add(&mut f, "dejavu-mono", include_bytes!("../../assets/fonts/DejaVuSansMono.ttf"));
    f.families.entry(FontFamily::Proportional).or_default().insert(0, "dejavu".into());
    f.families.entry(FontFamily::Monospace).or_default().insert(0, "dejavu-mono".into());
    let mut b = vec!["dejavu-bold".to_string()];
    b.extend(f.families[&FontFamily::Proportional].iter().cloned());
    f.families.insert(FontFamily::Name("bold".into()), b);
    f
}

pub fn install(ctx: &egui::Context, dark: bool) {
    let mut definitions = fonts();
    if is_caustica() { crate::caustica::fonts(&mut definitions); }
    ctx.set_fonts(definitions);
    DARK.store(dark, Ordering::Relaxed);
    apply(ctx);
}

/// Switches the colours when `dark` differs from the ones in use.
pub fn set_dark(ctx: &egui::Context, dark: bool) {
    if dark != is_dark() {
        DARK.store(dark, Ordering::Relaxed);
        apply(ctx);
        ctx.request_repaint();
    }
}

fn apply(ctx: &egui::Context) {
    use egui::{CornerRadius, Margin, Shadow, Vec2, style::WidgetVisuals};
    let mut style = (*ctx.global_style()).clone();
    let mut v = if is_dark() { egui::Visuals::dark() } else { egui::Visuals::light() };
    let flat = |fill: Color32| WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: Stroke::NONE,
        corner_radius: CornerRadius::ZERO,
        fg_stroke: Stroke::new(1.0, TEXT()),
        expansion: 0.0,
    };
    v.widgets.noninteractive = flat(FACE());
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, SHADOW());
    v.widgets.inactive = flat(FACE());
    v.widgets.hovered = flat(FACE());
    v.widgets.active = flat(FACE());
    v.widgets.open = flat(FACE());
    v.override_text_color = Some(TEXT());
    v.panel_fill = FACE();
    v.window_fill = FACE();
    v.window_stroke = Stroke::NONE;
    v.window_corner_radius = CornerRadius::ZERO;
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow { offset: [2, 2], blur: 0, spread: 0, color: Color32::from_black_alpha(70) };
    v.menu_corner_radius = CornerRadius::ZERO;
    v.extreme_bg_color = WINDOW();
    v.faint_bg_color = WINDOW();
    v.code_bg_color = WINDOW();
    v.selection.bg_fill = SEL();
    v.selection.stroke = Stroke::new(1.0, SEL_TEXT());
    v.hyperlink_color = LINK();
    v.text_cursor.stroke = Stroke::new(1.0, TEXT());
    v.text_cursor.blink = true;
    v.collapsing_header_frame = false;
    v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.5 };
    v.striped = false;
    style.visuals = v;
    style.text_styles = [
        (egui::TextStyle::Small, font(SMALL)),
        (egui::TextStyle::Body, font(BODY)),
        (egui::TextStyle::Button, font(BODY)),
        (egui::TextStyle::Heading, bold(14.0)),
        (egui::TextStyle::Monospace, mono(BODY)),
    ]
    .into();
    let sp = &mut style.spacing;
    sp.item_spacing = Vec2::new(6.0, 4.0);
    sp.button_padding = Vec2::new(6.0, 2.0);
    sp.interact_size = Vec2::new(40.0, 21.0);
    sp.window_margin = Margin::same(3);
    sp.menu_margin = Margin::same(3);
    sp.icon_width = 13.0;
    sp.icon_spacing = 5.0;
    sp.scroll = egui::style::ScrollStyle {
        floating: false,
        bar_width: 16.0,
        handle_min_length: 16.0,
        bar_inner_margin: 0.0,
        bar_outer_margin: 0.0,
        floating_width: 16.0,
        floating_allocated_width: 16.0,
        foreground_color: false,
        dormant_background_opacity: 1.0,
        active_background_opacity: 1.0,
        interact_background_opacity: 1.0,
        dormant_handle_opacity: 1.0,
        active_handle_opacity: 1.0,
        interact_handle_opacity: 1.0,
        fade: egui::style::ScrollFadeStyle { strength: 0.0, size: 0.0 },
        ..Default::default()
    };
    style.interaction.selectable_labels = false;
    style.interaction.tooltip_delay = 0.6;
    style.animation_time = 0.0;
    ctx.set_global_style(style);
}

// ------------------------------------------------------------------ icons ---

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Nzb,
    Connect,
    Pause,
    PauseFor,
    Speed,
    Cleanup,
    Top,
    Up,
    Down,
    Bottom,
    Delete,
    Unpack,
    Setup,
    Folder,
    File,
    History,
    Log,
    Par2,
    Rss,
    Play,
    Stats,
    Info,
    Key,
    Rename,
    Question,
    Warning,
    Error,
    Check,
    Queue,
    Refresh,
    Link,
}

/// Maps a 16-unit design grid onto `r`.
struct Grid {
    o: Pos2,
    k: f32,
}

impl Grid {
    fn p(&self, x: f32, y: f32) -> Pos2 {
        pos2(self.o.x + x * self.k, self.o.y + y * self.k)
    }
    fn r(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_min_max(self.p(x0, y0), self.p(x1, y1))
    }
    fn line(&self) -> f32 {
        (self.k * 0.9).max(1.0)
    }
}

fn poly(p: &mut Pen, g: &Grid, pts: &[(f32, f32)], fill: Color32, outline: Color32) {
    let v: Vec<Pos2> = pts.iter().map(|(x, y)| g.p(*x, *y)).collect();
    p.add(Shape::convex_polygon(v.clone(), fill, Stroke::NONE));
    p.add(Shape::closed_line(v, Stroke::new(g.line(), outline)));
}

fn boxed(p: &mut Pen, g: &Grid, r: Rect, fill: Color32, outline: Color32) {
    p.rect_filled(r, 0.0, fill);
    p.rect_stroke(r, 0.0, Stroke::new(g.line(), outline), egui::StrokeKind::Inside);
}

fn shade(c: Color32, k: f32) -> Color32 {
    let f = |v: u8| (v as f32 * k).clamp(0.0, 255.0) as u8;
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

/// The arrow of Up/Down/Top/Bottom, pointing up when `up`.
fn arrow(p: &mut Pen, g: &Grid, up: bool, y_shift: f32) {
    let blue = rgb(0x1E, 0x5A, 0xC8);
    let dark = rgb(0x0A, 0x24, 0x6A);
    let s = if up { 1.0 } else { -1.0 };
    let y = |v: f32| if up { v + y_shift } else { 16.0 - v - y_shift };
    let _ = s;
    let pts = [(8.0, y(2.0)), (14.0, y(8.0)), (10.5, y(8.0)), (10.5, y(14.0)), (5.5, y(14.0)), (5.5, y(8.0)), (2.0, y(8.0))];
    // Two convex parts: the head and the shaft.
    poly(p, g, &[pts[0], pts[1], pts[6]], blue, dark);
    let shaft = [pts[2], pts[3], pts[4], pts[5]];
    poly(p, g, &shaft, blue, dark);
    p.add(Shape::convex_polygon(vec![g.p(5.9, y(8.4)), g.p(10.1, y(8.4)), g.p(10.1, y(7.2)), g.p(5.9, y(7.2))], blue, Stroke::NONE));
}

pub fn paint_icon(p: &Painter, rect: Rect, icon: Icon, enabled: bool) {
    if is_caustica() { crate::caustica_icons::icon(p, rect, icon, enabled); return; }
    let s = rect.width().min(rect.height());
    let g = Grid { o: rect.center() - egui::vec2(s / 2.0, s / 2.0), k: s / 16.0 };
    let mut pen = Pen { p, out: Vec::new() };
    paint_color(&mut pen, &g, icon);
    if enabled {
        p.extend(pen.out);
        return;
    }
    // Classic disabled look: the silhouette embossed, white under grey.
    let recolor = |shapes: &[Shape], c: Color32, d: f32| -> Vec<Shape> {
        shapes
            .iter()
            .cloned()
            .map(|mut s| {
                egui::epaint::shape_transform::adjust_colors(&mut s, move |col| {
                    if col.a() > 0 {
                        *col = c;
                    }
                });
                s.translate(egui::vec2(d, d));
                s
            })
            .collect()
    };
    p.extend(recolor(&pen.out, HILIGHT(), g.k.max(1.0)));
    p.extend(recolor(&pen.out, SHADOW(), 0.0));
}

/// Collects an icon's shapes, so a disabled icon can be recoloured.
struct Pen<'a> {
    p: &'a Painter,
    out: Vec<Shape>,
}

impl Pen<'_> {
    fn add(&mut self, s: Shape) {
        self.out.push(s);
    }
    fn rect_filled(&mut self, r: Rect, radius: f32, c: Color32) {
        self.out.push(Shape::rect_filled(r, radius, c));
    }
    fn rect_stroke(&mut self, r: Rect, radius: f32, st: Stroke, kind: egui::StrokeKind) {
        self.out.push(Shape::rect_stroke(r, radius, st, kind));
    }
    fn line_segment(&mut self, pts: [Pos2; 2], st: Stroke) {
        self.out.push(Shape::line_segment(pts, st));
    }
    fn circle_filled(&mut self, c: Pos2, r: f32, col: Color32) {
        self.out.push(Shape::circle_filled(c, r, col));
    }
    fn circle_stroke(&mut self, c: Pos2, r: f32, st: Stroke) {
        self.out.push(Shape::circle_stroke(c, r, st));
    }
    fn text(&mut self, pos: Pos2, align: egui::Align2, t: &str, font: FontId, col: Color32) {
        let galley = self.p.layout_no_wrap(t.to_string(), font, col);
        let r = align.anchor_size(pos, galley.size());
        self.out.push(Shape::galley(r.min, galley, col));
    }
}

fn paint_color(p: &mut Pen, g: &Grid, icon: Icon) {
    let ink = rgb(0x30, 0x30, 0x30);
    let paper = rgb(0xFF, 0xFF, 0xFF);
    let yellow = rgb(0xF6, 0xD3, 0x5C);
    let yellow_dark = rgb(0xA8, 0x7C, 0x10);
    let green = rgb(0x2E, 0xA0, 0x2E);
    let green_dark = rgb(0x0E, 0x5E, 0x0E);
    let red = rgb(0xD8, 0x2A, 0x20);
    let red_dark = rgb(0x80, 0x10, 0x08);
    let blue = rgb(0x1E, 0x5A, 0xC8);
    let blue_dark = rgb(0x0A, 0x24, 0x6A);
    let w = g.line();
    let page = |p: &mut Pen| {
        poly(p, g, &[(3.0, 1.0), (10.0, 1.0), (13.0, 4.0), (13.0, 15.0), (3.0, 15.0)], paper, ink);
        poly(p, g, &[(10.0, 1.0), (13.0, 4.0), (10.0, 4.0)], rgb(0xD8, 0xD8, 0xD8), ink);
    };
    match icon {
        Icon::Nzb => {
            page(p);
            for y in [6.0, 8.0, 10.0] {
                p.line_segment([g.p(5.0, y), g.p(9.0, y)], Stroke::new(w, rgb(0x90, 0x90, 0x90)));
            }
            // Green plus: add.
            p.rect_filled(g.r(9.0, 9.0, 16.0, 16.0), 0.0, green_dark);
            p.rect_filled(g.r(11.5, 10.0, 13.5, 15.0), 0.0, rgb(0x7C, 0xE0, 0x7C));
            p.rect_filled(g.r(10.0, 11.5, 15.0, 13.5), 0.0, rgb(0x7C, 0xE0, 0x7C));
        }
        Icon::Connect => {
            let c = g.p(8.0, 8.0);
            p.circle_filled(c, 6.5 * g.k, rgb(0x4A, 0x8C, 0xE0));
            // Land masses.
            poly(p, g, &[(4.0, 5.0), (7.0, 3.5), (8.5, 6.0), (6.0, 8.5), (3.5, 7.5)], rgb(0x5C, 0xB8, 0x4C), rgb(0x5C, 0xB8, 0x4C));
            poly(p, g, &[(9.0, 9.0), (12.5, 8.5), (12.0, 12.0), (9.5, 13.0)], rgb(0x5C, 0xB8, 0x4C), rgb(0x5C, 0xB8, 0x4C));
            p.circle_stroke(c, 6.5 * g.k, Stroke::new(w, blue_dark));
        }
        Icon::Pause | Icon::PauseFor => {
            boxed(p, g, g.r(3.0, 2.0, 7.0, 14.0), blue, blue_dark);
            boxed(p, g, g.r(9.0, 2.0, 13.0, 14.0), blue, blue_dark);
            if icon == Icon::PauseFor {
                p.circle_filled(g.p(12.0, 12.0), 4.0 * g.k, paper);
                p.circle_stroke(g.p(12.0, 12.0), 4.0 * g.k, Stroke::new(w, ink));
                p.line_segment([g.p(12.0, 12.0), g.p(12.0, 9.5)], Stroke::new(w, ink));
                p.line_segment([g.p(12.0, 12.0), g.p(14.0, 12.0)], Stroke::new(w, ink));
            }
        }
        Icon::Speed => {
            let c = g.p(8.0, 10.0);
            p.circle_filled(c, 7.0 * g.k, paper);
            p.circle_stroke(c, 7.0 * g.k, Stroke::new(w, ink));
            p.rect_filled(g.r(0.0, 11.0, 16.0, 16.0), 0.0, Color32::TRANSPARENT);
            for (i, col) in [(0, green), (1, yellow), (2, red)] {
                let a0 = std::f32::consts::PI * (1.0 + i as f32 / 3.0);
                let a1 = a0 + std::f32::consts::PI / 3.0;
                let pts: Vec<Pos2> = (0..=6).map(|k| a0 + (a1 - a0) * k as f32 / 6.0).map(|a| c + egui::vec2(a.cos(), a.sin()) * 5.5 * g.k).collect();
                p.add(Shape::line(pts, Stroke::new(2.0 * g.k, col)));
            }
            p.line_segment([c, g.p(12.0, 5.0)], Stroke::new(1.5 * g.k, red_dark));
            p.circle_filled(c, 1.3 * g.k, ink);
        }
        Icon::Cleanup => {
            p.line_segment([g.p(12.5, 1.0), g.p(7.0, 9.0)], Stroke::new(2.0 * g.k, rgb(0x8B, 0x5A, 0x2B)));
            poly(p, g, &[(4.0, 8.0), (9.5, 10.0), (8.0, 15.0), (1.0, 13.5)], yellow, yellow_dark);
            for x in [3.0, 5.0, 7.0] {
                p.line_segment([g.p(x + 0.5, 10.0), g.p(x - 0.5, 14.5)], Stroke::new(w * 0.8, yellow_dark));
            }
        }
        Icon::Top | Icon::Bottom => {
            let up = icon == Icon::Top;
            let bar = if up { g.r(2.0, 1.0, 14.0, 3.5) } else { g.r(2.0, 12.5, 14.0, 15.0) };
            boxed(p, g, bar, blue, blue_dark);
            arrow(p, g, up, 2.0);
        }
        Icon::Up | Icon::Down => arrow(p, g, icon == Icon::Up, 0.0),
        Icon::Delete => {
            let st = Stroke::new(3.2 * g.k, red_dark);
            p.line_segment([g.p(3.0, 3.0), g.p(13.0, 13.0)], st);
            p.line_segment([g.p(13.0, 3.0), g.p(3.0, 13.0)], st);
            let st = Stroke::new(2.0 * g.k, red);
            p.line_segment([g.p(3.0, 3.0), g.p(13.0, 13.0)], st);
            p.line_segment([g.p(13.0, 3.0), g.p(3.0, 13.0)], st);
        }
        Icon::Unpack => {
            // A brown box with a blue arrow coming out.
            boxed(p, g, g.r(1.5, 7.0, 14.5, 15.0), rgb(0xC8, 0x96, 0x5A), rgb(0x6A, 0x46, 0x1C));
            p.line_segment([g.p(1.5, 10.0), g.p(14.5, 10.0)], Stroke::new(w, rgb(0x6A, 0x46, 0x1C)));
            poly(p, g, &[(8.0, 0.5), (12.5, 5.0), (3.5, 5.0)], green, green_dark);
            boxed(p, g, g.r(6.5, 4.5, 9.5, 8.5), green, green_dark);
        }
        Icon::Setup => {
            let c = g.p(8.0, 8.0);
            let teeth: Vec<Pos2> = (0..32)
                .map(|i| {
                    let a = i as f32 * std::f32::consts::TAU / 32.0;
                    let rr = if (i / 2) % 2 == 0 { 7.2 } else { 5.4 };
                    c + egui::vec2(a.cos(), a.sin()) * rr * g.k
                })
                .collect();
            p.add(Shape::convex_polygon(teeth.clone(), rgb(0x9C, 0xA4, 0xB0), Stroke::NONE));
            p.add(Shape::closed_line(teeth, Stroke::new(w, ink)));
            p.circle_filled(c, 2.2 * g.k, FACE());
            p.circle_stroke(c, 2.2 * g.k, Stroke::new(w, ink));
        }
        Icon::Folder | Icon::Queue => {
            poly(p, g, &[(1.0, 3.0), (6.0, 3.0), (7.5, 5.0), (15.0, 5.0), (15.0, 14.0), (1.0, 14.0)], yellow, yellow_dark);
            p.line_segment([g.p(1.5, 6.5), g.p(14.5, 6.5)], Stroke::new(w, rgb(0xFF, 0xF0, 0xB0)));
        }
        Icon::File => page(p),
        Icon::History => {
            let c = g.p(8.0, 8.0);
            p.circle_filled(c, 7.0 * g.k, paper);
            p.circle_stroke(c, 7.0 * g.k, Stroke::new(w, blue_dark));
            p.line_segment([c, g.p(8.0, 3.5)], Stroke::new(1.5 * g.k, ink));
            p.line_segment([c, g.p(11.5, 9.5)], Stroke::new(1.5 * g.k, ink));
            p.circle_filled(c, 1.0 * g.k, red);
        }
        Icon::Log => {
            boxed(p, g, g.r(2.5, 1.5, 13.5, 15.0), paper, ink);
            p.rect_filled(g.r(2.5, 1.5, 13.5, 4.0), 0.0, blue);
            for y in [6.5, 8.5, 10.5, 12.5] {
                p.line_segment([g.p(4.5, y), g.p(11.5, y)], Stroke::new(w * 0.8, rgb(0x70, 0x70, 0x70)));
            }
        }
        Icon::Par2 => {
            poly(p, g, &[(8.0, 1.0), (14.0, 3.5), (13.0, 10.0), (8.0, 15.0), (3.0, 10.0), (2.0, 3.5)], green, green_dark);
            p.add(Shape::line(vec![g.p(5.0, 8.0), g.p(7.5, 10.5), g.p(11.5, 5.5)], Stroke::new(1.6 * g.k, paper)));
        }
        Icon::Rss => {
            p.rect_filled(g.r(1.0, 1.0, 15.0, 15.0), 2.0 * g.k, rgb(0xF0, 0x8A, 0x1C));
            p.rect_stroke(g.r(1.0, 1.0, 15.0, 15.0), 2.0 * g.k, Stroke::new(w, rgb(0x90, 0x48, 0x00)), egui::StrokeKind::Inside);
            p.circle_filled(g.p(4.5, 11.5), 1.5 * g.k, paper);
            for r in [5.0, 8.5] {
                let pts: Vec<Pos2> = (0..=8).map(|k| -std::f32::consts::FRAC_PI_2 * (1.0 - k as f32 / 8.0)).map(|a| g.p(4.5, 11.5) + egui::vec2(a.cos(), a.sin()) * r * g.k).collect();
                p.add(Shape::line(pts, Stroke::new(1.6 * g.k, paper)));
            }
        }
        Icon::Play => poly(p, g, &[(4.0, 2.0), (13.0, 8.0), (4.0, 14.0)], green, green_dark),
        Icon::Stats => {
            for (x, h, c) in [(1.5, 7.0, blue), (6.0, 11.0, green), (10.5, 5.0, red)] {
                boxed(p, g, g.r(x, 15.0 - h, x + 4.0, 15.0), c, shade(c, 0.5));
            }
        }
        Icon::Info | Icon::Question | Icon::Warning | Icon::Error | Icon::Check => {
            let (fill, edge, glyph) = match icon {
                Icon::Info => (blue, blue_dark, "i"),
                Icon::Question => (blue, blue_dark, "?"),
                Icon::Error => (red, red_dark, "×"),
                Icon::Check => (green, green_dark, "✓"),
                _ => (yellow, yellow_dark, "!"),
            };
            if icon == Icon::Warning {
                poly(p, g, &[(8.0, 0.5), (15.5, 15.0), (0.5, 15.0)], fill, edge);
                p.text(g.p(8.0, 10.0), egui::Align2::CENTER_CENTER, glyph, bold(10.0 * g.k), ink);
            } else {
                p.circle_filled(g.p(8.0, 8.0), 7.2 * g.k, fill);
                p.circle_stroke(g.p(8.0, 8.0), 7.2 * g.k, Stroke::new(w, edge));
                p.text(g.p(8.0, 8.3), egui::Align2::CENTER_CENTER, glyph, bold(10.5 * g.k), paper);
            }
        }
        Icon::Key => {
            p.circle_filled(g.p(5.0, 8.0), 3.8 * g.k, yellow);
            p.circle_stroke(g.p(5.0, 8.0), 3.8 * g.k, Stroke::new(w, yellow_dark));
            p.circle_filled(g.p(4.2, 8.0), 1.2 * g.k, FACE());
            boxed(p, g, g.r(8.5, 7.0, 15.0, 9.0), yellow, yellow_dark);
            boxed(p, g, g.r(12.5, 9.0, 14.5, 12.0), yellow, yellow_dark);
        }
        Icon::Rename => {
            poly(p, g, &[(11.0, 1.5), (14.5, 5.0), (5.0, 14.5), (1.5, 14.5), (1.5, 11.0)], yellow, yellow_dark);
            poly(p, g, &[(1.5, 11.0), (5.0, 14.5), (1.5, 14.5)], rgb(0xF0, 0xD0, 0xA0), ink);
            p.line_segment([g.p(9.5, 3.0), g.p(13.0, 6.5)], Stroke::new(w, yellow_dark));
        }
        Icon::Refresh => {
            let c = g.p(8.0, 8.0);
            let pts: Vec<Pos2> = (0..=20).map(|k| -0.6 + k as f32 * 0.25).map(|a| c + egui::vec2(a.cos(), a.sin()) * 5.5 * g.k).collect();
            p.add(Shape::line(pts, Stroke::new(2.2 * g.k, green)));
            poly(p, g, &[(10.0, 1.5), (15.0, 4.0), (10.5, 7.0)], green, green_dark);
        }
        Icon::Link => {
            p.circle_stroke(g.p(5.5, 8.0), 3.5 * g.k, Stroke::new(2.0 * g.k, blue));
            p.circle_stroke(g.p(10.5, 8.0), 3.5 * g.k, Stroke::new(2.0 * g.k, blue));
        }
    }
}

#[cfg(test)]
mod preference_tests {
    use super::*;
    #[test]
    fn restored_skin_and_legacy_brightness_are_independent() {
        assert_eq!(Theme::from_saved(None), Theme::Caustica);
        assert_eq!(Theme::from_saved(Some("classic")), Theme::Classic);
        assert_eq!(Theme::from_saved(Some("caustica")), Theme::Caustica);
        assert_eq!(Theme::from_saved(Some("caustica-copper")), Theme::CausticaCopper);
        assert_eq!(Theme::from_saved(Some(Theme::CausticaCopper.key())), Theme::CausticaCopper);
        assert_eq!(Brightness::from_saved(Some("dark")), Brightness::Dark);
        assert_eq!(Brightness::from_saved(Some("light")), Brightness::Light);
        assert_eq!(Brightness::from_saved(None), Brightness::System);
        assert!(!Brightness::System.dark(false));
        assert!(Brightness::System.dark(true));
        assert!(!Brightness::Light.dark(true));
        assert!(Brightness::Dark.dark(false));
    }
    #[test]
    fn caustica_light_accent_and_warning_text_are_readable() {
        fn luminance(c: Color32) -> f64 {
            let v=[c.r(),c.g(),c.b()].map(|v| { let s=v as f64/255.0; if s<=0.04045 {s/12.92} else {((s+0.055)/1.055).powf(2.4)} });
            v[0]*0.2126+v[1]*0.7152+v[2]*0.0722
        }
        for (fg,bg) in [(crate::caustica::LIGHT_ACCENT,rgb(0xf3,0xf3,0xf5)),(CAUSTICA_LIGHT_WARNING,rgb(0xfa,0xfa,0xfc)),(rgb(0x60,0xca,0xd8),rgb(0x22,0x22,0x25))] {
            let (a,b)=(luminance(fg),luminance(bg));
            assert!((a.max(b)+0.05)/(a.min(b)+0.05)>=4.5);
        }
    }
}
