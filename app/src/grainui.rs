//! Grain on the Part screen (GR3–GR6): turning the open part's drawing, its
//! grain lock, and the grain callout drawn above a part kept to the grain.
//! The rules themselves are in `engine::grain`.

use crate::engine::grain::{self, Lock};
use crate::nest::P;
use crate::theme::th;
use crate::App;
use eframe::egui;
use egui::{Align2, FontId, Pos2, Stroke, Vec2};

impl App {
    /// Turn the open part's drawing `deg`° counter-clockwise (GR3), as one
    /// undo step.
    pub fn turn_open_part(&mut self, deg: i32) {
        let Some(i) = self.shown_part() else { return };
        let mut parts = self.parts.clone();
        let mut sheets = self.result.sheets.clone();
        match grain::turn_part(&mut parts, &mut sheets, i, deg) {
            Ok(done) => {
                self.checkpoint();
                self.parts = parts;
                self.result.sheets = sheets;
                if done.nested {
                    self.stale = true;
                }
                self.refresh();
                self.status = done.message;
            }
            Err(e) => self.status = e,
        }
    }

    /// Set the open part's grain lock (GR2), as one undo step.
    pub fn set_open_grain(&mut self, lock: Lock) {
        let Some(i) = self.shown_part() else { return };
        if grain::lock_of(&self.parts[i]) == lock {
            return;
        }
        self.checkpoint();
        grain::set_lock(&mut self.parts[i], lock);
        self.stale = true;
        self.refresh();
        self.status = format!("'{}': {}. {}", self.parts[i].name, lock.label(), lock.help());
    }

    /// The Part screen's grain group: turn the drawing left or right, and the
    /// grain lock.
    pub fn grain_group(&mut self, ui: &mut egui::Ui) {
        use crate::drawing::dropdown;
        use crate::ui_parts::{ribbon_button, ribbon_group, Icon};
        let has = self.shown_part().is_some();
        let lock = self.shown_part().map_or(Lock::Free, |i| grain::lock_of(&self.parts[i]));
        ribbon_group(ui, "Grain", |ui| {
            if ribbon_button(ui, Icon::RotateLeft, "Turn\nleft", false, has).on_hover_text("Turn the part's drawing 90° counter-clockwise, with its leads, micro-joints and texts, so its X runs along the sheet's grain (the sheet's length). Keeps the part to the grain.").clicked() {
                self.turn_open_part(90);
            }
            if ribbon_button(ui, Icon::Rotate, "Turn\nright", false, has).on_hover_text("Turn the part's drawing 90° clockwise, with its leads, micro-joints and texts, so its X runs along the sheet's grain (the sheet's length). Keeps the part to the grain.").clicked() {
                self.turn_open_part(-90);
            }
            let r = dropdown(ui, Icon::Grain, "Lock", has).on_hover_text(format!("Grain lock: {}. {}", lock.label(), lock.help()));
            egui::Popup::menu(&r).show(|ui| {
                ui.label(egui::RichText::new("The sheet's grain runs along its length").strong().color(th().text_strong));
                for l in Lock::ALL {
                    if ui.selectable_label(lock == l, l.label()).on_hover_text(l.help()).clicked() {
                        self.set_open_grain(l);
                    }
                }
            });
        });
    }
}

/// The grain callout above a part kept to the grain (GR6): a double arrow
/// along its X, the length of the part, with "GRAIN" and the angles the
/// nester may use.
pub fn paint_grain_callout(painter: &egui::Painter, part: &crate::nest::Part, to_s: &dyn Fn(P) -> Pos2) {
    let lock = grain::lock_of(part);
    if lock == Lock::Free {
        return;
    }
    let (w, h) = part.bbox();
    let col = th().warn_fg;
    let st = Stroke::new(1.6, col);
    let up = Vec2::new(0.0, -22.0);
    let (a, b) = (to_s([0.0, h]) + up, to_s([w, h]) + up);
    painter.line_segment([a, b], st);
    for (tip, dir) in [(a, 1.0f32), (b, -1.0)] {
        painter.line_segment([tip, tip + Vec2::new(8.0 * dir, -4.0)], st);
        painter.line_segment([tip, tip + Vec2::new(8.0 * dir, 4.0)], st);
    }
    let words = match lock {
        Lock::Fixed => "GRAIN  ·  fixed at 0°",
        _ => "GRAIN  ·  0° or 180°",
    };
    painter.text(Pos2::new((a.x + b.x) / 2.0, a.y - 4.0), Align2::CENTER_BOTTOM, words, FontId::proportional(12.0), col);
}
