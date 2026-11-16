//! The Drop layouts window (Sheet tab → Remnants & cut lines): the ways the
//! shop's standard drops can cut the shown sheet's leftover, each drawn on the
//! sheet; clicking one cuts the sheet that way (SD3, SD8). The rule itself is
//! `engine::drops`.

use crate::cutoff::CutKind;
use crate::engine::drops::{self, Layout, Outcome};
use crate::theme::th;
use crate::ui_parts::chip;
use crate::App;
use eframe::egui;
use egui::{Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};

/// The window's state; the layouts are worked out again only when the sheet
/// or the settings change.
#[derive(Default)]
pub struct DropsUi {
    pub open: bool,
    cache: Option<(u64, Result<Vec<Layout>, Outcome>)>,
}

/// Size of a card's drawing.
const PIC: Vec2 = Vec2::new(220.0, 96.0);

impl App {
    fn drops_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.cur_sheet.hash(&mut h);
        let spec = self.cur_spec();
        for v in [spec.w, spec.h, crate::kerf::kerf_mm(&spec), self.cfg.stock.min_remnant_m2] {
            v.to_bits().hash(&mut h);
        }
        for p in self.result.sheets.get(self.cur_sheet).map_or(&[][..], |v| v.as_slice()) {
            for v in [p.x, p.y, p.w, p.h] {
                v.to_bits().hash(&mut h);
            }
        }
        format!("{:?}{:?}", self.cfg.stock.line, self.cfg.stock.drops.sizes).hash(&mut h);
        h.finish()
    }

    /// The layouts for the shown sheet (cached).
    fn drop_layouts(&mut self) -> Result<Vec<Layout>, Outcome> {
        let key = self.drops_key();
        if let Some((k, v)) = &self.dropsui.cache {
            if *k == key {
                return v.clone();
            }
        }
        let sheet = self.result.sheets.get(self.cur_sheet).cloned().unwrap_or_default();
        let st = &self.cfg.stock;
        let v = drops::layouts(&sheet, &self.cur_spec(), &st.line, &st.drops.sizes, st.min_remnant_m2);
        self.dropsui.cache = Some((key, v.clone()));
        v
    }

    /// Cut the shown sheet's leftover as this layout (undo brings back the old lines).
    pub fn apply_drop_layout(&mut self, l: &Layout) {
        let what = format!("Standard drops, {}", l.summary(self.unit()));
        self.set_remnant_lines(l.lines.clone(), &what);
    }

    pub fn drops_window(&mut self, ctx: &egui::Context) {
        if !self.dropsui.open {
            return;
        }
        let mut open = true;
        let unit = self.unit();
        let res = self.drop_layouts();
        let mut pick: Option<Layout> = None;
        let mut options = false;
        let current: Vec<crate::cutoff::CutLine> = self.cut_list().iter().filter(|c| c.kind == CutKind::Remnant).cloned().collect();
        let sheet = self.result.sheets.get(self.cur_sheet).cloned().unwrap_or_default();
        let spec = self.cur_spec();
        let mut on = self.cfg.stock.drops.on;
        egui::Window::new("Drop layouts").open(&mut open).anchor(egui::Align2::RIGHT_TOP, [-16.0, 140.0]).default_width(480.0).collapsible(false).show(ctx, |ui| {
            ui.label(RichText::new(format!("Sheet {} — ways to cut the leftover into your standard drops. Click one to cut the sheet that way; Ctrl+Z puts the old line back.", self.cur_sheet + 1)).color(th().text_dim));
            ui.horizontal(|ui| {
                ui.checkbox(&mut on, "Cut off leftover uses standard drops").on_hover_text("On: Cut off leftover (and the line made after nesting) cuts the biggest drops first. Off: one remnant of whatever is left.");
                if ui.small_button("Drop sizes…").on_hover_text("Sheet Remnants options: the standard drop sizes").clicked() {
                    options = true;
                }
            });
            ui.separator();
            match &res {
                Err(why) => {
                    let msg = match why {
                        _ if self.cfg.stock.drops.sizes.is_empty() => "No standard drop sizes yet — add them under Drop sizes….".to_string(),
                        Outcome::NotStrip => format!("Standard drops work with the strip and L shapes of the remnant line; the line shape now is {}.", self.cfg.stock.line.shape.label()),
                        Outcome::NoRoom => "Nothing past the parts is wide enough for a remnant.".to_string(),
                        Outcome::NoneFits(l, w) => format!("No standard drop fits the {} left past the parts — it goes with the skeleton.", unit.size(*l, *w, 0)),
                    };
                    ui.label(RichText::new(msg).color(th().warn_fg));
                }
                Ok(v) => {
                    egui::ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
                        egui::Grid::new("drop_layouts").num_columns(2).spacing([10.0, 10.0]).show(ui, |ui| {
                            for (i, l) in v.iter().enumerate() {
                                let chosen = !current.is_empty() && current == l.lines;
                                if drop_card(ui, i, l, &sheet, &spec, unit, chosen).clicked() {
                                    pick = Some(l.clone());
                                }
                                if i % 2 == 1 {
                                    ui.end_row();
                                }
                            }
                        });
                    });
                }
            }
        });
        if on != self.cfg.stock.drops.on {
            self.cfg.stock.drops.on = on;
            let _ = self.cfg.save();
        }
        if let Some(l) = pick {
            self.apply_drop_layout(&l);
        }
        if options {
            self.stockui.open = true;
            self.stockui.show_options = true;
            self.stockui.loaded = false;
        }
        if !open {
            self.dropsui.open = false;
        }
    }
}

/// One layout: the sheet with its parts and the drops, what they are and how
/// much of the leftover they use.
fn drop_card(ui: &mut egui::Ui, i: usize, l: &Layout, sheet: &[crate::nest::Placed], spec: &crate::nest::SheetSpec, unit: crate::units::LenUnit, chosen: bool) -> egui::Response {
    let fill = if chosen { th().accent_soft } else { th().card };
    let stroke = if chosen { Stroke::new(1.5, th().accent) } else { Stroke::new(1.0, th().border) };
    let inner = egui::Frame::new().fill(fill).stroke(stroke).corner_radius(CornerRadius::same(6)).inner_margin(8.0).show(ui, |ui| ui.vertical(|ui| {
        ui.set_width(PIC.x);
        let (rect, _) = ui.allocate_exact_size(PIC, Sense::hover());
        draw_layout(ui.painter(), rect, l, sheet, spec, unit);
        ui.horizontal_wrapped(|ui| {
            if l.auto {
                chip(ui, "Auto", th().ok_bg, th().ok_fg).on_hover_text("What Cut off leftover makes: the biggest drops first");
            }
            ui.label(RichText::new(l.summary(unit)).strong());
        });
        ui.label(RichText::new(format!("{} drop{} · {:.0}% of the leftover · {} cut{}", l.drops.len(), if l.drops.len() == 1 { "" } else { "s" }, l.used() * 100.0, l.lines.len(), if l.lines.len() == 1 { "" } else { "s" })).size(11.5).color(th().text_dim));
    }));
    ui.interact(inner.response.rect, egui::Id::unique(("drop_card", i)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Cut the sheet this way")
}

/// The sheet scaled into `rect`: parts as grey boxes, drops filled with their
/// size, the cuts in red.
fn draw_layout(p: &egui::Painter, rect: Rect, l: &Layout, sheet: &[crate::nest::Placed], spec: &crate::nest::SheetSpec, unit: crate::units::LenUnit) {
    let s = ((rect.width() - 4.0) / spec.w as f32).min((rect.height() - 4.0) / spec.h as f32);
    let o = Pos2::new(rect.center().x - spec.w as f32 * s / 2.0, rect.center().y + spec.h as f32 * s / 2.0);
    let at = |x: f64, y: f64| Pos2::new(o.x + x as f32 * s, o.y - y as f32 * s);
    let bx = |x0: f64, y0: f64, x1: f64, y1: f64| Rect::from_two_pos(at(x0, y0), at(x1, y1));
    p.rect(bx(0.0, 0.0, spec.w, spec.h), CornerRadius::ZERO, th().sheet, Stroke::new(1.0, th().sheet_edge), StrokeKind::Inside);
    for q in sheet {
        p.rect_filled(bx(q.x, q.y, q.x + q.w, q.y + q.h), CornerRadius::ZERO, th().part_edge.gamma_multiply(0.35));
    }
    let drop_fill = Color32::from_rgba_unmultiplied(70, 150, 230, 90);
    for (d, size) in &l.drops {
        let r = bx(d[0], d[1], d[2], d[3]);
        p.rect(r, CornerRadius::ZERO, drop_fill, Stroke::new(1.0, Color32::from_rgb(40, 110, 200)), StrokeKind::Inside);
        if r.width() > 28.0 && r.height() > 12.0 {
            p.text(r.center(), egui::Align2::CENTER_CENTER, unit.dims(size[0], size[1], 0).replace(' ', ""), FontId::proportional(10.0), th().text_strong);
        }
    }
    for c in &l.lines {
        for w in c.pts.windows(2) {
            p.line_segment([at(w[0][0].clamp(0.0, spec.w), w[0][1].clamp(0.0, spec.h)), at(w[1][0].clamp(0.0, spec.w), w[1][1].clamp(0.0, spec.h))], Stroke::new(1.2, Color32::from_rgb(220, 60, 60)));
        }
    }
}
