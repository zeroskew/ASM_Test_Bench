//! Small custom widgets: painted toolbar icons, cards, chips, tabs.

use egui::{Color32, CornerRadius, Pos2, Rect, Response, RichText, Sense, Shape, Stroke, Ui, Vec2};

use super::theme::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Build,
    Run,
    Pause,
    Stop,
    Reset,
    StepInto,
    StepOver,
    StepOut,
    StepBack,
}

fn arc(c: Pos2, r: f32, a0: f32, a1: f32, n: usize) -> Vec<Pos2> {
    (0..=n).map(|i| {
        let t = a0 + (a1 - a0) * i as f32 / n as f32;
        Pos2::new(c.x + r * t.cos(), c.y + r * t.sin())
    }).collect()
}

fn arrow_head(p: &egui::Painter, tip: Pos2, dir: Vec2, size: f32, col: Color32) {
    let d = dir.normalized();
    let n = Vec2::new(-d.y, d.x);
    let base = tip - d * size;
    p.add(Shape::convex_polygon(vec![tip, base + n * size * 0.7, base - n * size * 0.7], col, Stroke::NONE));
}

pub fn paint_icon(p: &egui::Painter, icon: Icon, c: Pos2, s: f32, col: Color32) {
    let st = Stroke::new(1.8, col);
    let h = s / 2.0;
    use std::f32::consts::PI;
    match icon {
        Icon::Run => {
            p.add(Shape::convex_polygon(vec![Pos2::new(c.x - h * 0.6, c.y - h * 0.8), Pos2::new(c.x + h * 0.8, c.y), Pos2::new(c.x - h * 0.6, c.y + h * 0.8)], col, Stroke::NONE));
        }
        Icon::Pause => {
            for dx in [-h * 0.4, h * 0.4] {
                p.rect_filled(Rect::from_center_size(Pos2::new(c.x + dx, c.y), Vec2::new(h * 0.4, h * 1.5)), 1.0, col);
            }
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_center_size(c, Vec2::splat(h * 1.4)), 2.0, col);
        }
        Icon::Reset => {
            let pts = arc(c, h * 0.75, -PI * 0.35, PI * 1.45, 24);
            p.add(Shape::line(pts.clone(), st));
            let end = pts[0];
            arrow_head(p, end + Vec2::new(h * 0.15, h * 0.25), Vec2::new(0.5, 0.9), h * 0.55, col);
        }
        Icon::StepInto | Icon::StepOut => {
            let dot = Pos2::new(c.x, c.y + h * 0.75);
            p.circle_filled(dot, h * 0.22, col);
            if icon == Icon::StepInto {
                p.line_segment([Pos2::new(c.x, c.y - h * 0.9), Pos2::new(c.x, c.y + h * 0.05)], st);
                arrow_head(p, Pos2::new(c.x, c.y + h * 0.35), Vec2::new(0.0, 1.0), h * 0.5, col);
            } else {
                p.line_segment([Pos2::new(c.x, c.y + h * 0.35), Pos2::new(c.x, c.y - h * 0.4)], st);
                arrow_head(p, Pos2::new(c.x, c.y - h * 0.95), Vec2::new(0.0, -1.0), h * 0.5, col);
            }
        }
        Icon::StepOver | Icon::StepBack => {
            let dot = Pos2::new(c.x, c.y + h * 0.75);
            p.circle_filled(dot, h * 0.22, col);
            let ac = Pos2::new(c.x, c.y + h * 0.1);
            let (a0, a1) = if icon == Icon::StepOver { (PI * 1.05, PI * 1.9) } else { (PI * 1.95, PI * 1.1) };
            let pts = arc(ac, h * 0.8, a0, a1, 18);
            let end = *pts.last().unwrap();
            p.add(Shape::line(pts, st));
            let dir = if icon == Icon::StepOver { Vec2::new(0.35, 1.0) } else { Vec2::new(-0.35, 1.0) };
            arrow_head(p, end + dir * h * 0.35, dir, h * 0.5, col);
        }
        Icon::Build => {
            // A box with a check mark: "assemble / check".
            let r = Rect::from_center_size(c, Vec2::splat(h * 1.6));
            p.rect_stroke(r, 3.0, Stroke::new(1.5, col), egui::StrokeKind::Inside);
            p.add(Shape::line(vec![Pos2::new(c.x - h * 0.45, c.y), Pos2::new(c.x - h * 0.1, c.y + h * 0.35), Pos2::new(c.x + h * 0.5, c.y - h * 0.35)], Stroke::new(1.8, col)));
        }
    }
}

/// Icon-only toolbar button with a tooltip.
pub fn icon_button(ui: &mut Ui, icon: Icon, color: Color32, enabled: bool, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(30.0, 28.0), if enabled { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    if enabled && resp.hovered() {
        p.rect_filled(rect, 5.0, if resp.is_pointer_button_down_on() { Color32::from_rgb(62, 66, 76) } else { CARD_HI });
    }
    let col = if enabled { color } else { Color32::from_rgb(78, 82, 92) };
    paint_icon(p, icon, rect.center(), 16.0, col);
    let resp = resp.on_hover_text(tip);
    if enabled { resp } else { resp.on_hover_cursor(egui::CursorIcon::NotAllowed) }
}

/// Filled pill button with an icon and text (the primary Run button).
pub fn primary_button(ui: &mut Ui, icon: Icon, text: &str, fill: Color32, enabled: bool, tip: &str) -> Response {
    let font = egui::FontId::new(13.5, bold());
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, Color32::WHITE);
    let size = Vec2::new(galley.size().x + 40.0, 28.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let fill = if !enabled {
        Color32::from_rgb(55, 58, 66)
    } else if resp.is_pointer_button_down_on() {
        fill.gamma_multiply(0.8)
    } else if resp.hovered() {
        fill.gamma_multiply(1.15)
    } else {
        fill
    };
    let p = ui.painter();
    p.rect_filled(rect, 6.0, fill);
    let tc = if enabled { Color32::WHITE } else { TEXT_FAINT };
    paint_icon(p, icon, Pos2::new(rect.left() + 16.0, rect.center().y), 13.0, tc);
    p.galley(Pos2::new(rect.left() + 28.0, rect.center().y - galley.size().y / 2.0), galley, tc);
    resp.on_hover_text(tip)
}

/// A rounded card with a small uppercase title.
pub fn card<R>(ui: &mut Ui, title: &str, right: impl FnOnce(&mut Ui), body: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin { left: 12, right: 12, top: 8, bottom: 10 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(title.to_uppercase()).size(11.0).family(bold()).color(TEXT_DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
            });
            ui.add_space(2.0);
            body(ui)
        })
        .inner
}

/// Small rounded label (status pills, tags).
pub fn pill(ui: &mut Ui, text: &str, fg: Color32, bg: Color32) -> Response {
    let font = egui::FontId::new(12.0, bold());
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, fg);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + Vec2::new(18.0, 6.0), Sense::hover());
    ui.painter().rect_filled(rect, rect.height() / 2.0, bg);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, fg);
    resp
}

/// A flag chip: lit when set, outlined in orange when it changed in the last step.
pub fn flag_chip(ui: &mut Ui, name: &str, set: bool, changed: bool, active: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(40.0, 34.0), Sense::hover());
    let p = ui.painter();
    let (bg, fg) = if !active {
        (Color32::from_rgb(40, 42, 48), TEXT_FAINT)
    } else if set {
        (Color32::from_rgb(28, 92, 160), Color32::WHITE)
    } else {
        (Color32::from_rgb(40, 42, 48), TEXT_DIM)
    };
    p.rect_filled(rect, 5.0, bg);
    if changed && active {
        p.rect_stroke(rect, 5.0, Stroke::new(1.5, CHANGED), egui::StrokeKind::Inside);
    }
    p.text(Pos2::new(rect.center().x, rect.top() + 11.0), egui::Align2::CENTER_CENTER, name, egui::FontId::new(11.5, bold()), fg);
    p.text(Pos2::new(rect.center().x, rect.bottom() - 9.0), egui::Align2::CENTER_CENTER, if set { "1" } else { "0" }, egui::FontId::new(10.0, code()), fg.gamma_multiply(0.85));
    resp
}

/// Underlined tab header row. Returns the clicked tab.
pub fn tabs<T: Copy + PartialEq>(ui: &mut Ui, current: &mut T, items: &[(T, String)]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (t, label) in items {
            let sel = *current == *t;
            let font = egui::FontId::new(13.0, if sel { bold() } else { egui::FontFamily::Proportional });
            let galley = ui.painter().layout_no_wrap(label.clone(), font, if sel { TEXT } else { TEXT_DIM });
            let (rect, resp) = ui.allocate_exact_size(galley.size() + Vec2::new(22.0, 12.0), Sense::click());
            let p = ui.painter();
            if resp.hovered() && !sel {
                p.rect_filled(rect.shrink2(Vec2::new(2.0, 2.0)), 4.0, CARD_HI);
            }
            let tc = if sel || resp.hovered() { TEXT } else { TEXT_DIM };
            p.galley_with_override_text_color(rect.center() - galley.size() / 2.0, galley, tc);
            if sel {
                p.line_segment([Pos2::new(rect.left() + 6.0, rect.bottom() - 1.0), Pos2::new(rect.right() - 6.0, rect.bottom() - 1.0)], Stroke::new(2.0, ACCENT_HI));
            }
            if resp.clicked() {
                *current = *t;
            }
        }
    });
}
