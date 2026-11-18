//! Classic widgets drawn with egui: 3D bevels, push buttons, check boxes,
//! sunken fields, spin boxes, drop-down lists, menus, a tab control, list
//! view headers, captioned dialogs and a status bar.

use crate::theme::{self, BODY, DKSHADOW, FACE, GRAY_TEXT, HILIGHT, Icon, LIGHT, SEL, SEL_TEXT, SHADOW, TEXT, WINDOW};
use egui::{Align2, Color32, CursorIcon, Id, Pos2, Rect, Response, Sense, Shape, Stroke, TextEdit, Ui, Vec2, pos2, vec2};

// ------------------------------------------------------------- drawing ---

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    /// Buttons, dialogs, the page under a tab.
    Raised,
    /// Fields and lists.
    Sunken,
    /// One-pixel raised: hot toolbar buttons, status-bar grips.
    RaisedThin,
    /// One-pixel sunken: status-bar panes, pressed toolbar buttons.
    SunkenThin,
    /// The groove of group boxes and separators.
    Etched,
    /// A pressed push button: a flat dark frame.
    Pressed,
}

/// One-pixel strips: crisp at any scale, no anti-aliased lines.
fn strip(p: &egui::Painter, r: Rect, c: Color32) {
    p.rect_filled(r, 0.0, c);
}

fn frame(p: &egui::Painter, r: Rect, t: f32, tl: Color32, br: Color32) {
    strip(p, Rect::from_min_max(r.min, pos2(r.max.x - t, r.min.y + t)), tl);
    strip(p, Rect::from_min_max(r.min, pos2(r.min.x + t, r.max.y - t)), tl);
    strip(p, Rect::from_min_max(pos2(r.min.x, r.max.y - t), r.max), br);
    strip(p, Rect::from_min_max(pos2(r.max.x - t, r.min.y), r.max), br);
}

/// One physical pixel, in points.
pub fn px(ctx: &egui::Context) -> f32 {
    1.0 / ctx.pixels_per_point()
}

/// `r` with its edges on the pixel grid.
pub fn snap(ctx: &egui::Context, r: Rect) -> Rect {
    let k = ctx.pixels_per_point();
    let f = |v: f32| (v * k).round() / k;
    Rect::from_min_max(pos2(f(r.min.x), f(r.min.y)), pos2(f(r.max.x), f(r.max.y)))
}

pub fn edge(p: &egui::Painter, r: Rect, kind: Edge) {
    let r = snap(p.ctx(), r);
    // One point, but at least one physical pixel.
    let t = 1.0f32.max(px(p.ctx()));
    match kind {
        Edge::Raised => {
            frame(p, r, t, LIGHT(), DKSHADOW());
            frame(p, r.shrink(t), t, HILIGHT(), SHADOW());
        }
        Edge::Sunken => {
            frame(p, r, t, SHADOW(), HILIGHT());
            frame(p, r.shrink(t), t, DKSHADOW(), LIGHT());
        }
        Edge::RaisedThin => frame(p, r, t, HILIGHT(), SHADOW()),
        Edge::SunkenThin => frame(p, r, t, SHADOW(), HILIGHT()),
        Edge::Etched => {
            frame(p, r, t, SHADOW(), HILIGHT());
            frame(p, r.shrink(t), t, HILIGHT(), SHADOW());
        }
        Edge::Pressed => {
            frame(p, r, t, DKSHADOW(), DKSHADOW());
            frame(p, r.shrink(t), t, SHADOW(), SHADOW());
        }
    }
}

/// A horizontal groove (menus, dialogs above their buttons).
pub fn hline(p: &egui::Painter, x0: f32, x1: f32, y: f32) {
    let t = 1.0f32.max(px(p.ctx()));
    let y = (y * p.ctx().pixels_per_point()).round() / p.ctx().pixels_per_point();
    strip(p, Rect::from_min_max(pos2(x0, y), pos2(x1, y + t)), SHADOW());
    strip(p, Rect::from_min_max(pos2(x0, y + t), pos2(x1, y + 2.0 * t)), HILIGHT());
}

pub fn vline(p: &egui::Painter, x: f32, y0: f32, y1: f32) {
    let t = 1.0f32.max(px(p.ctx()));
    let x = (x * p.ctx().pixels_per_point()).round() / p.ctx().pixels_per_point();
    strip(p, Rect::from_min_max(pos2(x, y0), pos2(x + t, y1)), SHADOW());
    strip(p, Rect::from_min_max(pos2(x + t, y0), pos2(x + 2.0 * t, y1)), HILIGHT());
}

/// The dotted keyboard-focus rectangle.
pub fn focus_rect(p: &egui::Painter, r: Rect) {
    let st = Stroke::new(1.0, TEXT());
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    for w in pts.windows(2) {
        p.extend(Shape::dashed_line(&[w[0], w[1]], st, 1.0, 1.0));
    }
}

/// Text drawn embossed, as classic controls show disabled captions.
pub fn text_disabled(p: &egui::Painter, pos: Pos2, align: Align2, text: &str, font: egui::FontId) {
    p.text(pos + vec2(1.0, 1.0), align, text, font.clone(), HILIGHT());
    p.text(pos, align, text, font, SHADOW());
}

fn caption_text(p: &egui::Painter, pos: Pos2, align: Align2, text: &str, enabled: bool) {
    if enabled {
        p.text(pos, align, text, theme::font(BODY), TEXT());
    } else {
        text_disabled(p, pos, align, text, theme::font(BODY));
    }
}

/// A small filled triangle: drop-down and spin arrows, submenu marks.
pub fn triangle(p: &egui::Painter, c: Pos2, size: f32, down: bool, color: Color32) {
    let h = size / 2.0;
    let pts = if down {
        vec![pos2(c.x - size / 2.0, c.y - h / 2.0), pos2(c.x + size / 2.0, c.y - h / 2.0), pos2(c.x, c.y + h / 2.0)]
    } else {
        vec![pos2(c.x - size / 2.0, c.y + h / 2.0), pos2(c.x + size / 2.0, c.y + h / 2.0), pos2(c.x, c.y - h / 2.0)]
    };
    p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

pub fn text_width(ui: &Ui, text: &str, font: egui::FontId) -> f32 {
    ui.painter().layout_no_wrap(text.to_string(), font, TEXT()).size().x
}

// ------------------------------------------------------------- buttons ---

/// A push button, at least 75 × 23 like the dialog-unit default.
pub fn button(ui: &mut Ui, text: &str) -> Response {
    push(ui, text, None, 75.0, false)
}

/// The dialog's default button (Enter): an extra dark frame.
pub fn default_button(ui: &mut Ui, text: &str) -> Response {
    push(ui, text, None, 75.0, true)
}

pub fn button_w(ui: &mut Ui, text: &str, width: f32) -> Response {
    push(ui, text, None, width, false)
}

pub fn icon_button(ui: &mut Ui, text: &str, icon: Icon, width: f32) -> Response {
    push(ui, text, Some(icon), width, false)
}

fn push(ui: &mut Ui, text: &str, icon: Option<Icon>, min_w: f32, default: bool) -> Response {
    let tw = text_width(ui, text, theme::font(BODY)) + if icon.is_some() { 22.0 } else { 0.0 };
    let size = vec2((tw + 20.0).max(min_w), 23.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let enabled = ui.is_enabled();
    let resp = resp.on_hover_cursor(CursorIcon::Default);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let pressed = enabled && resp.is_pointer_button_down_on() && resp.hovered();
        let focused = resp.has_focus();
        let mut r = snap(ui.ctx(), rect);
        p.rect_filled(r, 0.0, FACE());
        if default || focused {
            edge(p, r, Edge::Pressed);
            frame(p, r, 1.0, DKSHADOW(), DKSHADOW());
            r = r.shrink(1.0);
        }
        edge(p, r, if pressed { Edge::Pressed } else { Edge::Raised });
        let d = if pressed { 1.0 } else { 0.0 };
        let mut x = rect.center().x + d;
        if let Some(icon) = icon {
            let start = rect.center().x - tw / 2.0;
            theme::paint_icon(p, Rect::from_min_size(pos2(start + d, rect.center().y - 8.0 + d), vec2(16.0, 16.0)), icon, enabled);
            x += 11.0;
        }
        caption_text(p, pos2(x, rect.center().y + d), Align2::CENTER_CENTER, text, enabled);
        if focused {
            focus_rect(p, rect.shrink(4.0));
        }
    }
    if enabled && resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Space)) {
        // Space presses a focused button, as on the desktop.
        let mut r = resp.clone();
        r.flags |= egui::response::Flags::FAKE_PRIMARY_CLICKED;
        return r;
    }
    resp
}

/// A flat toolbar button with a 24 px icon over its caption (`text` empty:
/// icon only). Raised when hot, sunken when pressed or checked.
pub fn tool_button(ui: &mut Ui, text: &str, icon: Icon, checked: bool, compact: bool) -> Response {
    let w = if compact || text.is_empty() { 30.0 } else { (text_width(ui, text, theme::font(theme::SMALL)) + 14.0).max(52.0) };
    let h = if compact || text.is_empty() { 30.0 } else { 46.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    let enabled = ui.is_enabled();
    let p = ui.painter();
    let hot = enabled && resp.hovered();
    let down = enabled && resp.is_pointer_button_down_on() && hot;
    if checked {
        p.rect_filled(rect, 0.0, theme::TRACK());
    }
    if down || checked {
        edge(p, rect, Edge::SunkenThin);
    } else if hot {
        edge(p, rect, Edge::RaisedThin);
    }
    let d = if down || checked { 1.0 } else { 0.0 };
    let isz = if compact || text.is_empty() { 20.0 } else { 24.0 };
    let icon_c = if compact || text.is_empty() { rect.center() } else { pos2(rect.center().x, rect.top() + 16.0) };
    theme::paint_icon(p, Rect::from_center_size(icon_c + vec2(d, d), vec2(isz, isz)), icon, enabled);
    if !(compact || text.is_empty()) {
        let pos = pos2(rect.center().x + d, rect.bottom() - 9.0 + d);
        if enabled {
            p.text(pos, Align2::CENTER_CENTER, text, theme::font(theme::SMALL), TEXT());
        } else {
            text_disabled(p, pos, Align2::CENTER_CENTER, text, theme::font(theme::SMALL));
        }
    }
    resp
}

/// The groove between toolbar button groups.
pub fn tool_separator(ui: &mut Ui, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(8.0, height), Sense::hover());
    vline(ui.painter(), rect.center().x - 1.0, rect.top() + 3.0, rect.bottom() - 3.0);
}

/// The two ridges at the start of a classic toolbar band.
pub fn gripper(ui: &mut Ui, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(8.0, height), Sense::hover());
    let r = Rect::from_min_max(pos2(rect.left() + 2.0, rect.top() + 2.0), pos2(rect.left() + 5.0, rect.bottom() - 2.0));
    edge(ui.painter(), r, Edge::RaisedThin);
}

// ---------------------------------------------------------- check boxes ---

pub fn checkbox(ui: &mut Ui, on: &mut bool, text: &str) -> Response {
    let tw = text_width(ui, text, theme::font(BODY));
    let (rect, mut resp) = ui.allocate_exact_size(vec2(13.0 + 6.0 + tw + 2.0, 19.0), Sense::click());
    let enabled = ui.is_enabled();
    if resp.clicked() && enabled {
        *on = !*on;
        resp.mark_changed();
    }
    let p = ui.painter();
    let b = snap(ui.ctx(), Rect::from_min_size(pos2(rect.left(), rect.center().y - 6.5), vec2(13.0, 13.0)));
    let pressed = resp.is_pointer_button_down_on() && resp.hovered();
    p.rect_filled(b, 0.0, if enabled && !pressed { WINDOW() } else { FACE() });
    edge(p, b, Edge::Sunken);
    if *on {
        let c = if enabled { TEXT() } else { SHADOW() };
        let pts = vec![pos2(b.left() + 3.0, b.top() + 6.0), pos2(b.left() + 5.5, b.top() + 8.5), pos2(b.left() + 10.0, b.top() + 4.0)];
        p.add(Shape::line(pts, Stroke::new(2.0, c)));
    }
    let tpos = pos2(b.right() + 6.0, rect.center().y);
    caption_text(p, tpos, Align2::LEFT_CENTER, text, enabled);
    if resp.has_focus() && !text.is_empty() {
        focus_rect(p, Rect::from_min_size(tpos - vec2(2.0, 8.0), vec2(tw + 4.0, 16.0)));
    }
    resp
}

pub fn radio(ui: &mut Ui, selected: bool, text: &str) -> Response {
    let tw = text_width(ui, text, theme::font(BODY));
    let (rect, resp) = ui.allocate_exact_size(vec2(12.0 + 6.0 + tw + 2.0, 19.0), Sense::click());
    let enabled = ui.is_enabled();
    let p = ui.painter();
    let c = pos2(rect.left() + 6.0, rect.center().y);
    // The sunken ring: dark above-left, light below-right.
    p.circle_filled(c, 6.0, HILIGHT());
    p.circle_filled(c + vec2(-0.5, -0.5), 5.5, SHADOW());
    p.circle_filled(c, 5.0, LIGHT());
    p.circle_filled(c + vec2(-0.3, -0.3), 4.6, DKSHADOW());
    p.circle_filled(c, 4.0, if enabled { WINDOW() } else { FACE() });
    if selected {
        p.circle_filled(c, 2.0, if enabled { TEXT() } else { SHADOW() });
    }
    caption_text(p, pos2(rect.left() + 18.0, rect.center().y), Align2::LEFT_CENTER, text, enabled);
    resp
}

// --------------------------------------------------------------- fields ---

/// A sunken edit field around `edit`; `rows` > 1 for a multi-line one.
pub fn field(ui: &mut Ui, width: f32, rows: usize, edit: TextEdit<'_>) -> Response {
    let line = 16.0;
    let h = if rows <= 1 { 21.0 } else { rows as f32 * line + 8.0 };
    let (rect, _) = ui.allocate_exact_size(vec2(width, h), Sense::hover());
    let enabled = ui.is_enabled();
    ui.painter().rect_filled(rect, 0.0, if enabled { WINDOW() } else { FACE() });
    edge(ui.painter(), rect, Edge::Sunken);
    let inner = rect.shrink2(vec2(3.0, if rows <= 1 { 2.0 } else { 3.0 }));
    let edit = edit
        .frame(egui::Frame::NONE)
        .margin(egui::Margin::symmetric(1, if rows <= 1 { 1 } else { 0 }))
        .font(theme::font(BODY))
        .text_color(if enabled { TEXT() } else { GRAY_TEXT() })
        .desired_width(inner.width() - 2.0);
    if rows <= 1 {
        // A child ui, so the parent's cursor stays after the whole field.
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)));
        child.add(edit)
    } else {
        scroll_view(ui, inner, ("field", rect.min.x as i32, rect.min.y as i32), [false, true], false, 14.0, |area, ui| {
            let w = ui.available_width();
            area.show(ui, |ui| ui.add_sized([w, inner.height() - 2.0], edit.desired_rows(rows)))
        })
        .inner
    }
}

/// A spin box: a number field with up/down arrows. Typing is committed on
/// Enter or when the field loses focus; out-of-range values are clamped.
pub fn spin(ui: &mut Ui, id_salt: impl std::hash::Hash + std::fmt::Debug, value: &mut f64, min: f64, max: f64, decimals: usize, width: f32) -> Response {
    let id = ui.scope_id().with(("spin", id_salt));
    let (rect, _) = ui.allocate_exact_size(vec2(width, 21.0), Sense::hover());
    let enabled = ui.is_enabled();
    let p = ui.painter().clone();
    p.rect_filled(rect, 0.0, if enabled { WINDOW() } else { FACE() });
    edge(&p, rect, Edge::Sunken);
    let arrows = Rect::from_min_max(pos2(rect.right() - 18.0, rect.top() + 2.0), pos2(rect.right() - 2.0, rect.bottom() - 2.0));
    let fmt = |v: f64| format!("{v:.decimals$}");
    let mut text: String = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| fmt(*value));
    let edit_rect = Rect::from_min_max(pos2(rect.left() + 3.0, rect.top() + 2.0), pos2(arrows.left() - 1.0, rect.bottom() - 2.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(edit_rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let r = child.add(
        TextEdit::singleline(&mut text)
            .id(id.with("edit"))
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::symmetric(1, 1))
            .font(theme::font(BODY))
            .text_color(if enabled { TEXT() } else { GRAY_TEXT() })
            .desired_width(edit_rect.width() - 2.0),
    );
    let mut changed = false;
    let commit = |text: &str, value: &mut f64| -> bool {
        match text.trim().replace(',', ".").parse::<f64>() {
            Ok(v) => {
                let v = v.clamp(min, max);
                let v = if decimals == 0 { v.round() } else { v };
                if v != *value {
                    *value = v;
                    return true;
                }
                false
            }
            Err(_) => false,
        }
    };
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            changed |= commit(&fmt((*value + 1.0).min(max)), value);
            ui.data_mut(|d| d.insert_temp(id, fmt(*value)));
        }
        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            changed |= commit(&fmt((*value - 1.0).max(min)), value);
            ui.data_mut(|d| d.insert_temp(id, fmt(*value)));
        }
    }
    if r.lost_focus() {
        changed |= commit(&text, value);
        ui.data_mut(|d| d.remove::<String>(id));
    }
    // The arrow buttons.
    let half = arrows.height() / 2.0;
    for (k, up) in [(0.0, true), (1.0, false)] {
        let br = Rect::from_min_size(pos2(arrows.left(), arrows.top() + k * half), vec2(arrows.width(), half));
        let resp = ui.interact(br, id.with(("arrow", up)), Sense::click());
        let down = enabled && resp.is_pointer_button_down_on() && resp.hovered();
        p.rect_filled(br, 0.0, FACE());
        edge(&p, br, if down { Edge::SunkenThin } else { Edge::Raised });
        triangle(&p, br.center() + if down { vec2(1.0, 1.0) } else { Vec2::ZERO }, 6.0, !up, if enabled { TEXT() } else { SHADOW() });
        if enabled && resp.clicked() {
            let next = if up { (*value + 1.0).min(max) } else { (*value - 1.0).max(min) };
            changed |= commit(&fmt(next), value);
            ui.data_mut(|d| d.remove::<String>(id));
        }
    }
    let mut out = r;
    if changed {
        out.mark_changed();
    }
    out
}

// ------------------------------------------------------- drop-down list ---

/// A drop-down list: the current text in a sunken field with an arrow
/// button; `add` lists the choices with [`list_item`].
pub fn combo<R>(ui: &mut Ui, id_salt: impl std::hash::Hash, current: &str, width: f32, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 21.0), Sense::click());
    let enabled = ui.is_enabled();
    let p = ui.painter();
    p.rect_filled(rect, 0.0, if enabled { WINDOW() } else { FACE() });
    edge(p, rect, Edge::Sunken);
    let btn = Rect::from_min_max(pos2(rect.right() - 18.0, rect.top() + 2.0), pos2(rect.right() - 2.0, rect.bottom() - 2.0));
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
    let down = open || (resp.is_pointer_button_down_on() && resp.hovered());
    p.rect_filled(btn, 0.0, FACE());
    edge(p, btn, if down { Edge::SunkenThin } else { Edge::Raised });
    triangle(p, btn.center() + if down { vec2(1.0, 1.0) } else { Vec2::ZERO }, 7.0, true, if enabled { TEXT() } else { SHADOW() });
    let text_r = Rect::from_min_max(pos2(rect.left() + 4.0, rect.top() + 3.0), pos2(btn.left() - 2.0, rect.bottom() - 3.0));
    if resp.has_focus() {
        p.rect_filled(text_r, 0.0, SEL());
    }
    p.with_clip_rect(text_r).text(
        pos2(text_r.left() + 1.0, text_r.center().y),
        Align2::LEFT_CENTER,
        current,
        theme::font(BODY),
        if !enabled { GRAY_TEXT() } else if resp.has_focus() { SEL_TEXT() } else { TEXT() },
    );
    let _ = id_salt;
    let shown = egui::Popup::from_toggle_button_response(&resp)
        .width(width)
        .gap(0.0)
        .frame(egui::Frame::NONE.fill(WINDOW()).stroke(Stroke::new(1.0, DKSHADOW())).inner_margin(1))
        .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
        .show(|ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            ui.set_min_width(width - 2.0);
            add(ui)
        });
    shown.map(|s| s.inner)
}

/// One choice in a drop-down list: highlighted under the pointer.
pub fn list_item(ui: &mut Ui, selected: bool, text: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::click());
    let hot = resp.hovered() || (selected && !ui.ctx().is_pointer_over_egui());
    let p = ui.painter();
    if hot {
        p.rect_filled(rect, 0.0, SEL());
    }
    p.text(pos2(rect.left() + 4.0, rect.center().y), Align2::LEFT_CENTER, text, theme::font(BODY), if hot { SEL_TEXT() } else { TEXT() });
    resp
}

// ---------------------------------------------------------------- menus ---

/// The frame of every popup menu: face colour, room for the raised edge.
pub fn menu_frame() -> egui::Frame {
    egui::Frame::NONE.fill(FACE()).inner_margin(egui::Margin::same(3)).shadow(egui::Shadow {
        offset: [2, 2],
        blur: 0,
        spread: 0,
        color: Color32::from_black_alpha(70),
    })
}

/// Draws the raised edge round a popup menu once it is laid out.
pub fn menu_edge(ctx: &egui::Context, shown: &egui::Response) {
    edge(&ctx.layer_painter(shown.layer_id), shown.rect, Edge::Raised);
}

/// A menu command. `shortcut` is shown right-aligned and greyed items do
/// nothing. Returns true when chosen; the caller closes the menu.
pub fn menu_item(ui: &mut Ui, text: &str, shortcut: &str, enabled: bool) -> bool {
    menu_row(ui, text, shortcut, enabled, None)
}

/// A menu item with a tick (`radio`: a dot) when `on`.
pub fn menu_check(ui: &mut Ui, text: &str, on: bool, radio: bool) -> bool {
    menu_row(ui, text, "", true, Some((on, radio)))
}

/// A menu item with a tick and a shortcut.
pub fn menu_check_key(ui: &mut Ui, text: &str, shortcut: &str, on: bool) -> bool {
    menu_row(ui, text, shortcut, true, Some((on, false)))
}

/// The width of the menu `ui` is in: its widest row, as measured over the
/// last frame, so all rows (and the highlight) share one width.
fn menu_width(ui: &Ui, desired: f32) -> f32 {
    let key = ui.scope_id().with("menu-width");
    let pass = ui.ctx().cumulative_pass_nr();
    let (seen, this, last): (u64, f32, f32) = ui.data(|d| d.get_temp(key)).unwrap_or((pass, 0.0, 0.0));
    let (this, last) = if seen == pass { (this, last) } else { (0.0, this) };
    let this = this.max(desired);
    ui.data_mut(|d| d.insert_temp(key, (pass, this, last)));
    desired.max(last).max(150.0)
}

fn menu_row(ui: &mut Ui, text: &str, shortcut: &str, enabled: bool, check: Option<(bool, bool)>) -> bool {
    let tw = text_width(ui, text, theme::font(BODY));
    let sw = if shortcut.is_empty() { 0.0 } else { text_width(ui, shortcut, theme::font(BODY)) + 28.0 };
    let w = menu_width(ui, tw + sw + 46.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 19.0), Sense::click());
    let p = ui.painter();
    let hot = resp.hovered();
    if hot {
        p.rect_filled(rect, 0.0, SEL());
    }
    let color = if hot { SEL_TEXT() } else { TEXT() };
    let tpos = pos2(rect.left() + 20.0, rect.center().y);
    if enabled {
        p.text(tpos, Align2::LEFT_CENTER, text, theme::font(BODY), color);
        if !shortcut.is_empty() {
            p.text(pos2(rect.right() - 12.0, rect.center().y), Align2::RIGHT_CENTER, shortcut, theme::font(BODY), color);
        }
    } else if hot {
        p.text(tpos, Align2::LEFT_CENTER, text, theme::font(BODY), SHADOW().gamma_multiply(1.2));
    } else {
        text_disabled(p, tpos, Align2::LEFT_CENTER, text, theme::font(BODY));
        if !shortcut.is_empty() {
            text_disabled(p, pos2(rect.right() - 12.0, rect.center().y), Align2::RIGHT_CENTER, shortcut, theme::font(BODY));
        }
    }
    if let Some((true, radio)) = check {
        let c = pos2(rect.left() + 10.0, rect.center().y);
        if radio {
            p.circle_filled(c, 3.0, color);
        } else {
            p.add(Shape::line(vec![c + vec2(-3.5, 0.0), c + vec2(-1.0, 2.5), c + vec2(3.5, -2.5)], Stroke::new(1.8, color)));
        }
    }
    enabled && resp.clicked()
}

pub fn menu_separator(ui: &mut Ui) {
    let w = menu_width(ui, 0.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, 8.0), Sense::hover());
    hline(ui.painter(), rect.left() + 1.0, rect.right() - 1.0, rect.center().y - 1.0);
}

/// A right-click menu on `resp`, in the classic frame.
pub fn context_menu(resp: &Response, add: impl FnOnce(&mut Ui)) {
    let shown = egui::Popup::context_menu(resp).frame(menu_frame()).show(|ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        add(ui)
    });
    if let Some(s) = shown {
        menu_edge(&resp.ctx, &s.response);
    }
}

/// The menu bar. `menus` holds the titles; `add(ui, index)` fills the
/// open one and returns true to close it.
pub fn menu_bar(ui: &mut Ui, titles: &[&str], mut add: impl FnMut(&mut Ui, usize) -> bool) {
    let id = ui.scope_id().with("menubar");
    let mut open: Option<usize> = ui.data(|d| d.get_temp(id)).flatten();
    let mut rects = Vec::new();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(2.0);
        for (i, t) in titles.iter().enumerate() {
            let w = text_width(ui, t, theme::font(BODY)) + 14.0;
            let (rect, resp) = ui.allocate_exact_size(vec2(w, 19.0), Sense::click());
            rects.push(rect);
            let p = ui.painter();
            if open == Some(i) {
                edge(p, rect, Edge::SunkenThin);
            } else if resp.hovered() {
                edge(p, rect, Edge::RaisedThin);
            }
            let d = if open == Some(i) { 1.0 } else { 0.0 };
            p.text(rect.center() + vec2(d, d), Align2::CENTER_CENTER, t, theme::font(BODY), TEXT());
            if resp.clicked() {
                open = if open == Some(i) { None } else { Some(i) };
            } else if open.is_some() && open != Some(i) && resp.hovered() {
                // With one menu open, pointing at another opens that one.
                open = Some(i);
            }
        }
    });
    let ctx = ui.ctx().clone();
    if let Some(i) = open {
        let anchor = rects[i];
        let area = egui::Area::new(id.with(("menu", i)))
            .kind(egui::UiKind::Menu)
            .order(egui::Order::Foreground)
            .fixed_pos(anchor.left_bottom() + vec2(0.0, 1.0))
            .constrain(true);
        let shown = area.show(&ctx, |ui| {
            menu_frame()
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    ui.vertical(|ui| add(ui, i)).inner
                })
                .inner
        });
        menu_edge(&ctx, &shown.response);
        let clicked_outside = ctx.input(|inp| inp.pointer.any_pressed())
            && ctx.input(|inp| inp.pointer.interact_pos()).is_some_and(|p| !shown.response.rect.contains(p) && !rects.iter().any(|r| r.contains(p)));
        if shown.inner || clicked_outside || ctx.input(|inp| inp.key_pressed(egui::Key::Escape)) {
            open = None;
        }
    }
    ui.data_mut(|d| d.insert_temp(id, open));
}

// ----------------------------------------------------------------- tabs ---

/// A tab strip; the selected tab is raised and joins the page below it.
/// Returns the page rectangle under the tabs.
pub fn tab_strip(ui: &mut Ui, selected: &mut usize, titles: &[(&str, Icon)]) -> Rect {
    let h = 22.0;
    let (strip_rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    let mut x = strip_rect.left() + 2.0;
    let mut sel_rect = None;
    for (i, (t, icon)) in titles.iter().enumerate() {
        let w = text_width(ui, t, theme::font(BODY)) + 34.0;
        let is_sel = *selected == i;
        let r = if is_sel {
            Rect::from_min_max(pos2(x - 2.0, strip_rect.top()), pos2(x + w + 2.0, strip_rect.bottom() + 1.0))
        } else {
            Rect::from_min_max(pos2(x, strip_rect.top() + 2.0), pos2(x + w, strip_rect.bottom()))
        };
        let resp = ui.interact(r, ui.scope_id().with(("tab", i)), Sense::click());
        if resp.clicked() {
            *selected = i;
        }
        if is_sel {
            sel_rect = Some(r);
        } else {
            paint_tab(ui.painter(), r, t, *icon, false, resp.hovered());
        }
        x += w;
    }
    if let Some(r) = sel_rect {
        let i = *selected;
        // The page's top edge, broken under the selected tab.
        let p = ui.painter();
        strip(p, Rect::from_min_max(pos2(strip_rect.left(), strip_rect.bottom()), pos2(r.left(), strip_rect.bottom() + 1.0)), HILIGHT());
        strip(p, Rect::from_min_max(pos2(r.right(), strip_rect.bottom()), pos2(strip_rect.right(), strip_rect.bottom() + 1.0)), HILIGHT());
        paint_tab(p, r, titles[i].0, titles[i].1, true, false);
    } else {
        strip(ui.painter(), Rect::from_min_max(strip_rect.left_bottom(), pos2(strip_rect.right(), strip_rect.bottom() + 1.0)), HILIGHT());
    }
    strip_rect
}

fn paint_tab(p: &egui::Painter, r: Rect, text: &str, icon: Icon, selected: bool, hot: bool) {
    let r = snap(p.ctx(), r);
    p.rect_filled(r, 0.0, FACE());
    // Left and top light, right dark; rounded corners by one pixel.
    strip(p, Rect::from_min_max(pos2(r.left(), r.top() + 2.0), pos2(r.left() + 1.0, r.bottom())), HILIGHT());
    strip(p, Rect::from_min_max(pos2(r.left() + 2.0, r.top()), pos2(r.right() - 2.0, r.top() + 1.0)), HILIGHT());
    strip(p, Rect::from_min_max(pos2(r.left() + 1.0, r.top() + 1.0), pos2(r.left() + 2.0, r.top() + 2.0)), HILIGHT());
    strip(p, Rect::from_min_max(pos2(r.right() - 1.0, r.top() + 2.0), pos2(r.right(), r.bottom())), DKSHADOW());
    strip(p, Rect::from_min_max(pos2(r.right() - 2.0, r.top() + 1.0), pos2(r.right() - 1.0, r.top() + 2.0)), DKSHADOW());
    strip(p, Rect::from_min_max(pos2(r.right() - 2.0, r.top() + 2.0), pos2(r.right() - 1.0, r.bottom())), SHADOW());
    let d = if selected { 0.0 } else { 1.0 };
    theme::paint_icon(p, Rect::from_min_size(pos2(r.left() + 7.0, r.center().y - 8.0 + d - 0.5), vec2(16.0, 16.0)), icon, true);
    p.text(pos2(r.left() + 27.0, r.center().y + d - 0.5), Align2::LEFT_CENTER, text, theme::font(BODY), TEXT());
    if hot && !selected {
        p.line_segment([pos2(r.left() + 3.0, r.top() + 1.5), pos2(r.right() - 3.0, r.top() + 1.5)], Stroke::new(1.0, theme::ORANGE()));
    }
}

// ------------------------------------------------------------ list view ---

/// Column layout of a list view: widths that the header can resize.
#[derive(Clone)]
pub struct Columns {
    pub titles: Vec<&'static str>,
    pub widths: Vec<f32>,
    pub right: Vec<bool>,
    /// Remembered in the browser under this key.
    pub key: &'static str,
}

impl Columns {
    pub fn new(key: &'static str, cols: &[(&'static str, f32, bool)]) -> Self {
        let mut c = Self { titles: cols.iter().map(|c| c.0).collect(), widths: cols.iter().map(|c| c.1).collect(), right: cols.iter().map(|c| c.2).collect(), key };
        if let Some(saved) = crate::app::stored(&format!("nq-cols-{key}")) {
            let w: Vec<f32> = saved.split(',').filter_map(|v| v.parse().ok()).collect();
            if w.len() == c.widths.len() {
                c.widths = w;
            }
        }
        c
    }
    pub fn total(&self) -> f32 {
        self.widths.iter().sum()
    }
    /// Left edges, from `x0`.
    pub fn lefts(&self, x0: f32) -> Vec<f32> {
        let mut x = x0;
        self.widths
            .iter()
            .map(|w| {
                let l = x;
                x += w;
                l
            })
            .collect()
    }
    fn save(&self) {
        let v: Vec<String> = self.widths.iter().map(|w| format!("{w:.0}")).collect();
        crate::app::store(&format!("nq-cols-{}", self.key), &v.join(","));
    }
}

pub const HEADER_H: f32 = 20.0;

/// Paints the header at `top`, scrolled by `scroll_x`, and lets the user
/// drag the column dividers.
pub fn list_header(ui: &mut Ui, cols: &mut Columns, area: Rect, scroll_x: f32) {
    let top = area.top();
    let clip = Rect::from_min_max(area.min, pos2(area.right(), top + HEADER_H));
    let p = ui.painter().with_clip_rect(clip);
    let lefts = cols.lefts(area.left() - scroll_x);
    let mut changed = false;
    for (i, l) in lefts.iter().enumerate() {
        let r = Rect::from_min_size(pos2(*l, top), vec2(cols.widths[i], HEADER_H));
        p.rect_filled(r, 0.0, FACE());
        edge(&p, r, Edge::Raised);
        let text_r = r.shrink2(vec2(6.0, 0.0));
        let tp = if cols.right[i] { pos2(text_r.right(), r.center().y) } else { pos2(text_r.left(), r.center().y) };
        p.with_clip_rect(text_r.intersect(clip)).text(tp, if cols.right[i] { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER }, cols.titles[i], theme::font(BODY), TEXT());
        // The divider at the right edge resizes this column.
        let grip = Rect::from_center_size(pos2(r.right(), r.center().y), vec2(8.0, HEADER_H)).intersect(clip);
        if grip.width() > 0.0 {
            let g = ui.interact(grip, ui.scope_id().with(("colgrip", cols.key, i)), Sense::drag()).on_hover_cursor(CursorIcon::ResizeColumn);
            if g.dragged() {
                cols.widths[i] = (cols.widths[i] + g.drag_delta().x).max(24.0);
                changed = true;
            }
            if g.drag_stopped() {
                cols.save();
            }
        }
    }
    // The empty header space right of the last column.
    let end = lefts.last().map(|l| l + cols.widths.last().copied().unwrap_or(0.0)).unwrap_or(area.left());
    if end < clip.right() {
        let r = Rect::from_min_max(pos2(end, top), pos2(clip.right() + 2.0, top + HEADER_H));
        p.rect_filled(r, 0.0, FACE());
        edge(&p, r, Edge::Raised);
    }
    if changed {
        ui.ctx().request_repaint();
    }
}

/// A smooth progress bar in a thin sunken well with two-tone centred text.
pub fn progress(p: &egui::Painter, r: Rect, fraction: f32, text: &str, fill: Color32) {
    let r = snap(p.ctx(), r);
    p.rect_filled(r, 0.0, WINDOW());
    edge(p, r, Edge::SunkenThin);
    let inner = r.shrink(1.0);
    let fill_r = Rect::from_min_size(inner.min, vec2((inner.width() * fraction.clamp(0.0, 1.0)).round(), inner.height()));
    if fill_r.width() > 0.0 {
        p.rect_filled(fill_r, 0.0, fill);
    }
    let font = theme::font(theme::SMALL);
    p.with_clip_rect(fill_r).text(inner.center(), Align2::CENTER_CENTER, text, font.clone(), Color32::WHITE);
    let rest = Rect::from_min_max(pos2(fill_r.right(), inner.top()), inner.max);
    p.with_clip_rect(rest).text(inner.center(), Align2::CENTER_CENTER, text, font, TEXT());
}

// --------------------------------------------------------------- dialogs ---

/// A modal dialog with a classic caption bar (drag it to move the dialog)
/// and a close box. Returns the content's value and whether the dialog
/// asked to close (close box or Escape).
pub fn dialog<R>(ctx: &egui::Context, id: &str, title: &str, width: f32, add: impl FnOnce(&mut Ui) -> R) -> (R, bool) {
    let id = Id::unique(("dialog", id));
    let offset: Vec2 = ctx.data(|d| d.get_temp(id.with("offset"))).unwrap_or_default();
    let screen = ctx.content_rect();
    let width = width.min(screen.width() - 8.0);
    let mut close = false;
    let area = egui::Modal::default_area(id).anchor(Align2::CENTER_CENTER, offset).constrain(true);
    let shown = egui::Modal::new(id)
        .area(area)
        .backdrop_color(Color32::TRANSPARENT)
        .frame(egui::Frame::NONE.fill(FACE()).inner_margin(egui::Margin::same(4)))
        .show(ctx, |ui| {
            ui.set_width(width - 8.0);
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            // Caption.
            let (cap, cap_resp) = ui.allocate_exact_size(vec2(width - 8.0, 18.0), Sense::drag());
            gradient(ui.painter(), cap, theme::CAPTION_A(), theme::CAPTION_B());
            ui.painter().with_clip_rect(cap.shrink2(vec2(4.0, 0.0))).text(
                pos2(cap.left() + 4.0, cap.center().y),
                Align2::LEFT_CENTER,
                title,
                theme::bold(BODY),
                Color32::WHITE,
            );
            if cap_resp.dragged() {
                let o = offset + cap_resp.drag_delta();
                ui.data_mut(|d| d.insert_temp(id.with("offset"), o));
            }
            let xr = Rect::from_min_size(pos2(cap.right() - 18.0, cap.top() + 2.0), vec2(16.0, 14.0));
            let x = ui.interact(xr, id.with("close"), Sense::click());
            let down = x.is_pointer_button_down_on() && x.hovered();
            ui.painter().rect_filled(xr, 0.0, FACE());
            edge(ui.painter(), xr, if down { Edge::Pressed } else { Edge::Raised });
            let c = xr.center() + if down { vec2(1.0, 1.0) } else { Vec2::ZERO } + vec2(-0.5, 0.0);
            let st = Stroke::new(1.6, TEXT());
            ui.painter().line_segment([c + vec2(-3.5, -3.0), c + vec2(3.5, 3.0)], st);
            ui.painter().line_segment([c + vec2(3.5, -3.0), c + vec2(-3.5, 3.0)], st);
            if x.clicked() {
                close = true;
            }
            ui.add_space(4.0);
            egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
                ui.set_width(width - 24.0);
                add(ui)
            })
            .inner
        });
    edge(&ctx.layer_painter(shown.response.layer_id), shown.response.rect, Edge::Raised);
    if shown.is_top_modal && !shown.any_popup_open && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        close = true;
    }
    (shown.inner, close)
}

/// The row of buttons at the bottom of a dialog, after a groove. `add`
/// lays them out right to left: add Cancel before OK.
pub fn dialog_buttons(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.add_space(2.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 2.0), Sense::hover());
    hline(ui.painter(), r.left(), r.right(), r.top());
    ui.add_space(2.0);
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 23.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        add(ui);
    });
}

/// A left-to-right linear gradient (dialog captions, banners).
pub fn gradient(p: &egui::Painter, r: Rect, a: Color32, b: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(r.left_top(), a);
    mesh.colored_vertex(r.right_top(), b);
    mesh.colored_vertex(r.right_bottom(), b);
    mesh.colored_vertex(r.left_bottom(), a);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(Shape::mesh(mesh));
}

/// A group box: an etched frame with its caption on the face colour.
pub fn group<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let top = ui.cursor().top();
    let left = ui.cursor().left();
    let width = ui.available_width();
    ui.add_space(8.0);
    let inner = egui::Frame::NONE
        .inner_margin(egui::Margin { left: 10, right: 10, top: 10, bottom: 8 })
        .show(ui, |ui| {
            ui.set_width(width - 20.0);
            add(ui)
        });
    let r = Rect::from_min_max(pos2(left, top + 7.0), pos2(left + width, inner.response.rect.bottom()));
    let p = ui.painter();
    edge(p, r, Edge::Etched);
    let tw = text_width(ui, title, theme::font(BODY));
    let label = Rect::from_min_size(pos2(r.left() + 7.0, top), vec2(tw + 6.0, 14.0));
    p.rect_filled(label, 0.0, FACE());
    p.text(pos2(label.left() + 3.0, label.center().y), Align2::LEFT_CENTER, title, theme::font(BODY), TEXT());
    ui.add_space(6.0);
    inner.inner
}

/// A label that greys out with its control.
pub fn label(ui: &mut Ui, text: &str) -> Response {
    let enabled = ui.is_enabled();
    let w = text_width(ui, text, theme::font(BODY));
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 19.0), Sense::hover());
    caption_text(ui.painter(), pos2(rect.left(), rect.center().y), Align2::LEFT_CENTER, text, enabled);
    resp
}

/// Grey explanatory text that wraps.
pub fn note(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(egui::RichText::new(text).font(theme::font(theme::SMALL)).color(GRAY_TEXT())).wrap());
}

// ----------------------------------------------------------- status bar ---

/// A sunken status-bar pane of `width` (0: the rest) with its text.
pub fn status_pane(ui: &mut Ui, width: f32, text: &str, color: Color32, icon: Option<Color32>) -> Response {
    let w = if width <= 0.0 { ui.available_width() - 16.0 } else { width };
    let (rect, resp) = ui.allocate_exact_size(vec2(w.max(10.0), 18.0), Sense::click());
    let p = ui.painter();
    edge(p, rect, Edge::SunkenThin);
    let mut x = rect.left() + 5.0;
    if let Some(c) = icon {
        p.circle_filled(pos2(x + 4.0, rect.center().y), 3.5, c);
        p.circle_stroke(pos2(x + 4.0, rect.center().y), 3.5, Stroke::new(1.0, SHADOW()));
        x += 12.0;
    }
    p.with_clip_rect(rect.shrink(2.0)).text(pos2(x, rect.center().y), Align2::LEFT_CENTER, text, theme::font(BODY), color);
    resp
}

/// The diagonal grip in the bottom-right corner.
pub fn size_grip(p: &egui::Painter, corner: Pos2) {
    for k in 0..3 {
        let o = 3.0 + k as f32 * 4.0;
        p.line_segment([pos2(corner.x - o, corner.y - 1.0), pos2(corner.x - 1.0, corner.y - o)], Stroke::new(1.0, HILIGHT()));
        p.line_segment([pos2(corner.x - o + 1.0, corner.y - 1.0), pos2(corner.x - 1.0, corner.y - o + 1.0)], Stroke::new(1.0, SHADOW()));
        p.line_segment([pos2(corner.x - o + 2.0, corner.y - 1.0), pos2(corner.x - 1.0, corner.y - o + 2.0)], Stroke::new(1.0, SHADOW()));
    }
}

// ---------------------------------------------------------- scroll bars ---

/// Width of a classic scroll bar.
pub const SCROLL_BAR: f32 = 16.0;

/// A scroll area in `rect` with classic Windows scroll bars: arrow buttons
/// (held, they repeat), a dithered track that pages and a raised thumb.
/// The egui bars stay hidden; a bar appears only while its axis overflows.
/// `show` gets the configured `ScrollArea` and the viewport ui and must call
/// one of its `show` functions.
pub fn scroll_view<R>(
    ui: &mut Ui,
    rect: Rect,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    dirs: [bool; 2],
    stick_bottom: bool,
    step: f32,
    show: impl FnOnce(egui::ScrollArea, &mut Ui) -> egui::scroll_area::ScrollAreaOutput<R>,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    let key = ui.scope_id().with(("scroll-view", id_salt));
    let content: Vec2 = ui.data(|d| d.get_temp(key)).unwrap_or(Vec2::ZERO);
    // Which bars are needed (one bar can make the other necessary).
    let mut need = [false, false];
    for _ in 0..2 {
        let w = rect.width() - if need[1] { SCROLL_BAR } else { 0.0 };
        let h = rect.height() - if need[0] { SCROLL_BAR } else { 0.0 };
        need = [dirs[0] && content.x > w + 0.5, dirs[1] && content.y > h + 0.5];
    }
    let view = Rect::from_min_max(rect.min, pos2(rect.right() - if need[1] { SCROLL_BAR } else { 0.0 }, rect.bottom() - if need[0] { SCROLL_BAR } else { 0.0 }));
    let area_id: Option<Id> = ui.data(|d| d.get_temp(key.with("area")));
    let offset = area_id.and_then(|id| egui::scroll_area::State::load(ui.ctx(), id)).map(|s| s.clamped_offset()).unwrap_or(Vec2::ZERO);
    let max = (content - view.size()).max(Vec2::ZERO);
    let mut wanted = offset;
    for axis in 0..2 {
        if !need[axis] {
            continue;
        }
        let bar = if axis == 1 {
            Rect::from_min_max(pos2(view.right(), rect.top()), pos2(rect.right(), view.bottom()))
        } else {
            Rect::from_min_max(pos2(rect.left(), view.bottom()), pos2(view.right(), rect.bottom()))
        };
        let page = if axis == 1 { view.height() } else { view.width() };
        wanted[axis] = scroll_bar(ui, key.with(axis), bar, axis, offset[axis], max[axis], page, step);
    }
    if need[0] && need[1] {
        ui.painter().rect_filled(Rect::from_min_max(view.max, rect.max), 0.0, FACE());
    }
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(view).layout(egui::Layout::top_down(egui::Align::Min)));
    child.set_clip_rect(view.intersect(ui.clip_rect()));
    let mut area = egui::ScrollArea::new(dirs)
        .id_salt(key)
        .auto_shrink([false, false])
        .stick_to_bottom(stick_bottom)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    if wanted != offset {
        area = area.scroll_offset(wanted);
    }
    let out = show(area, &mut child);
    ui.data_mut(|d| {
        d.insert_temp(key, out.content_size);
        d.insert_temp(key.with("area"), out.id);
    });
    // Bars drawn for an older size or offset: draw again.
    if out.content_size != content || out.state.clamped_offset() != wanted {
        ui.ctx().request_repaint();
    }
    out
}

/// True on the press of `r` and then, while it is held, every 50 ms after
/// a 400 ms delay, like a held scroll arrow.
fn auto_repeat(ui: &Ui, r: &Response, key: Id) -> bool {
    if !r.is_pointer_button_down_on() {
        ui.data_mut(|d| d.remove::<f64>(key));
        return false;
    }
    let now = ui.input(|i| i.time);
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    match ui.data(|d| d.get_temp::<f64>(key)) {
        None => {
            ui.data_mut(|d| d.insert_temp(key, now + 0.4));
            true
        }
        Some(t) if now >= t => {
            ui.data_mut(|d| d.insert_temp(key, now + 0.05));
            true
        }
        _ => false,
    }
}

/// One classic scroll bar along `axis` (0 horizontal, 1 vertical) in `bar`.
/// Returns the offset the user asked for.
#[allow(clippy::too_many_arguments)]
fn scroll_bar(ui: &Ui, id: Id, bar: Rect, axis: usize, offset: f32, max: f32, page: f32, step: f32) -> f32 {
    let along = |r: Rect| if axis == 1 { r.height() } else { r.width() };
    let len = along(bar);
    let arrow = SCROLL_BAR.min(len / 2.0);
    let (a_rect, b_rect) = if axis == 1 {
        (Rect::from_min_size(bar.min, vec2(bar.width(), arrow)), Rect::from_min_max(pos2(bar.left(), bar.bottom() - arrow), bar.max))
    } else {
        (Rect::from_min_size(bar.min, vec2(arrow, bar.height())), Rect::from_min_max(pos2(bar.right() - arrow, bar.top()), bar.max))
    };
    let track = if axis == 1 {
        Rect::from_min_max(pos2(bar.left(), a_rect.bottom()), pos2(bar.right(), b_rect.top()))
    } else {
        Rect::from_min_max(pos2(a_rect.right(), bar.top()), pos2(b_rect.left(), bar.bottom()))
    };
    let track_len = along(track).max(0.0);
    let thumb_len = if max > 0.0 { (track_len * page / (page + max)).max(8.0).min(track_len) } else { track_len };
    let thumb_rect = |off: f32| {
        let t = if max > 0.0 { (track_len - thumb_len) * (off / max).clamp(0.0, 1.0) } else { 0.0 };
        if axis == 1 {
            Rect::from_min_size(pos2(track.left(), track.top() + t), vec2(track.width(), thumb_len))
        } else {
            Rect::from_min_size(pos2(track.left() + t, track.top()), vec2(thumb_len, track.height()))
        }
    };
    // The track first: the thumb is registered later, so it is on top.
    let tr = ui.interact(track, id.with("track"), Sense::click_and_drag());
    let a = ui.interact(a_rect, id.with("a"), Sense::click_and_drag());
    let b = ui.interact(b_rect, id.with("b"), Sense::click_and_drag());
    let show_thumb = track_len >= 8.0 && max > 0.0;
    let thumb = ui.interact(if show_thumb { thumb_rect(offset) } else { Rect::NOTHING }, id.with("thumb"), Sense::drag());
    let mut off = offset;
    if auto_repeat(ui, &a, id.with("ra")) {
        off -= step;
    }
    if auto_repeat(ui, &b, id.with("rb")) {
        off += step;
    }
    let mut paging = None;
    let pointer = ui.input(|i| i.pointer.interact_pos());
    match pointer.filter(|_| tr.is_pointer_button_down_on()) {
        Some(p) => {
            let t = thumb_rect(offset);
            let before = if axis == 1 { p.y < t.top() } else { p.x < t.left() };
            let after = if axis == 1 { p.y > t.bottom() } else { p.x > t.right() };
            if before || after {
                paging = Some(before);
                if auto_repeat(ui, &tr, id.with("rt")) {
                    off += if before { -page } else { page };
                }
            }
        }
        None => {
            auto_repeat(ui, &tr, id.with("rt"));
        }
    }
    if thumb.dragged() && track_len > thumb_len {
        off += thumb.drag_delta()[axis] * max / (track_len - thumb_len);
    }
    let off = off.clamp(0.0, max);
    // Paint.
    let p = ui.painter();
    p.rect_filled(track, 0.0, theme::TRACK());
    if let Some(before) = paging {
        // The pressed part of the track turns dark, as in Windows 2000.
        let t = thumb_rect(off);
        let part = match (axis, before) {
            (1, true) => Rect::from_min_max(track.min, pos2(track.right(), t.top())),
            (1, false) => Rect::from_min_max(pos2(track.left(), t.bottom()), track.max),
            (_, true) => Rect::from_min_max(track.min, pos2(t.left(), track.bottom())),
            (_, false) => Rect::from_min_max(pos2(t.right(), track.top()), track.max),
        };
        p.rect_filled(part, 0.0, DKSHADOW());
    }
    for (r, resp, back) in [(a_rect, &a, true), (b_rect, &b, false)] {
        let down = resp.is_pointer_button_down_on() && resp.hovered();
        p.rect_filled(r, 0.0, FACE());
        edge(p, r, if down { Edge::Pressed } else { Edge::Raised });
        let c = r.center() + if down { vec2(1.0, 1.0) } else { Vec2::ZERO };
        let color = if max > 0.0 { TEXT() } else { GRAY_TEXT() };
        let (s, h) = (3.5, 2.0);
        let pts = match (axis, back) {
            (1, true) => vec![pos2(c.x - s, c.y + h), pos2(c.x + s, c.y + h), pos2(c.x, c.y - h)],
            (1, false) => vec![pos2(c.x - s, c.y - h), pos2(c.x + s, c.y - h), pos2(c.x, c.y + h)],
            (_, true) => vec![pos2(c.x + h, c.y - s), pos2(c.x + h, c.y + s), pos2(c.x - h, c.y)],
            (_, false) => vec![pos2(c.x - h, c.y - s), pos2(c.x - h, c.y + s), pos2(c.x + h, c.y)],
        };
        p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
    }
    if show_thumb {
        let t = thumb_rect(off);
        p.rect_filled(t, 0.0, FACE());
        edge(p, t, Edge::Raised);
    }
    off
}
