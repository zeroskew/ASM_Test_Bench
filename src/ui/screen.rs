//! The Screen window: shows the memory-mapped 128x128 display and sends keys to the program.

use egui::{Color32, ColorImage, Event, EventFilter, Key, Pos2, Rect, RichText, Sense, Stroke, TextureHandle, TextureOptions, Vec2};

use super::theme::*;
use crate::asm::{palette, SCREEN_BASE, SCREEN_H, SCREEN_W};
use crate::cpu::Machine;

pub struct ScreenView {
    tex: Option<TextureHandle>,
    palette: [Color32; 256],
}

/// Key code for an egui key (see the KEY_* built-in constants), if the program can see it.
pub fn key_code(k: Key) -> Option<u32> {
    Some(match k {
        Key::ArrowLeft => 128,
        Key::ArrowRight => 129,
        Key::ArrowUp => 130,
        Key::ArrowDown => 131,
        Key::Space => 32,
        Key::Enter => 13,
        Key::Escape => 27,
        Key::Backspace => 8,
        Key::Tab => 9,
        _ => {
            let name = k.name();
            let c = name.chars().next()?;
            if name.len() == 1 && (c.is_ascii_uppercase() || c.is_ascii_digit()) {
                c as u32
            } else {
                return None;
            }
        }
    })
}

impl ScreenView {
    pub fn new() -> Self {
        let mut p = [Color32::BLACK; 256];
        for (i, c) in p.iter_mut().enumerate() {
            let [r, g, b] = palette(i as u8);
            *c = Color32::from_rgb(r, g, b);
        }
        ScreenView { tex: None, palette: p }
    }

    pub fn id() -> egui::Id {
        egui::Id::unique("screen_view")
    }

    /// Draws the window. `m` is None when no program is loaded (a blank screen is shown).
    pub fn show(&mut self, ctx: &egui::Context, open: &mut bool, m: Option<&mut Machine>, running: bool) {
        let id = Self::id();
        let pixels: Vec<Color32> = match &m {
            Some(m) => m.mem[SCREEN_BASE as usize..(SCREEN_BASE + SCREEN_W * SCREEN_H) as usize].iter().map(|&b| self.palette[b as usize]).collect(),
            None => vec![Color32::BLACK; (SCREEN_W * SCREEN_H) as usize],
        };
        let img = ColorImage::new([SCREEN_W as usize, SCREEN_H as usize], pixels);
        match &mut self.tex {
            Some(t) => t.set(img, TextureOptions::NEAREST),
            None => self.tex = Some(ctx.load_texture("screen", img, TextureOptions::NEAREST)),
        }
        let tex = self.tex.as_ref().unwrap().id();
        let mut m = m;

        // First placement: top-right of the editor, left of the inspector panel.
        let area = ctx.content_rect();
        egui::Window::new(RichText::new("Screen").family(bold()))
            .id(egui::Id::unique("screen_window"))
            .default_pos(egui::pos2((area.right() - 400.0 - 430.0).max(area.left() + 20.0), area.top() + 100.0))
            .open(open)
            .resizable(true)
            .default_size([420.0, 470.0])
            .min_size([200.0, 230.0])
            .show(ctx, |ui| {
                let avail = ui.available_size() - Vec2::new(0.0, 26.0);
                // Integer scale when it fits (crisp pixels), else fit to the window.
                let fit = (avail.x / SCREEN_W as f32).min(avail.y / SCREEN_H as f32).max(0.5);
                let scale = if fit >= 1.0 { fit.floor() } else { fit };
                let size = Vec2::new(SCREEN_W as f32, SCREEN_H as f32) * scale;
                let (outer, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), size.y + 8.0), Sense::hover());
                let rect = Rect::from_center_size(outer.center(), size);
                let resp = ui.interact(rect, id, Sense::click());
                if resp.clicked() {
                    ui.memory_mut(|mem| mem.request_focus(id));
                }
                let focused = ui.memory(|mem| mem.has_focus(id));
                if focused {
                    // Arrow keys and Tab go to the program, not to focus navigation.
                    ui.memory_mut(|mem| mem.set_focus_lock_filter(id, EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: false }));
                }
                let p = ui.painter();
                p.rect_filled(rect.expand(3.0), 3.0, Color32::from_rgb(10, 10, 12));
                p.image(tex, rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                let border = if focused { Stroke::new(2.0, ACCENT_HI) } else { Stroke::new(1.0, BORDER) };
                p.rect_stroke(rect.expand(3.0), 3.0, border, egui::StrokeKind::Inside);

                // Keys go to the program while the screen has focus.
                if focused {
                    if let Some(m) = m.as_deref_mut() {
                        let events: Vec<(Key, bool)> = ui.input(|i| i.events.iter().filter_map(|e| if let Event::Key { key, pressed, .. } = e { Some((*key, *pressed)) } else { None }).collect());
                        for (k, down) in events {
                            if let Some(code) = key_code(k) {
                                m.key_event(code, down);
                            }
                        }
                    }
                }

                // Footer: hovered pixel, or a hint.
                ui.horizontal(|ui| {
                    if let Some(pos) = resp.hover_pos() {
                        let x = ((pos.x - rect.left()) / scale).floor().clamp(0.0, SCREEN_W as f32 - 1.0) as u32;
                        let y = ((pos.y - rect.top()) / scale).floor().clamp(0.0, SCREEN_H as f32 - 1.0) as u32;
                        let addr = SCREEN_BASE + y * SCREEN_W + x;
                        let c = m.as_deref().map(|m| m.mem[addr as usize]).unwrap_or(0);
                        let (sw, _) = ui.allocate_exact_size(Vec2::new(12.0, 12.0), Sense::hover());
                        ui.painter().rect_filled(sw, 2.0, self.palette[c as usize]);
                        ui.label(RichText::new(format!("x {x}, y {y}  ·  colour {c}  ·  [0x{addr:04X}]")).family(code()).size(12.0).color(TEXT_DIM));
                    } else if focused {
                        ui.label(RichText::new("Keyboard → program (arrows, letters, Space…). Click elsewhere to stop.").size(12.0).color(ACCENT_HI));
                    } else {
                        let hint = if running { "Click the screen to send keys to the program." } else { "128×128 pixels at SCREEN (0xC000), one byte = one palette colour." };
                        ui.label(RichText::new(hint).size(12.0).color(TEXT_DIM));
                    }
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_codes() {
        assert_eq!(key_code(Key::ArrowLeft), Some(128));
        assert_eq!(key_code(Key::A), Some('A' as u32));
        assert_eq!(key_code(Key::Num7), Some('7' as u32));
        assert_eq!(key_code(Key::F5), None);
    }
}
