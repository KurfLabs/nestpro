//! Smooth on the Part screen (SM21): the Edit geometry group's button and its
//! small window. The rule itself is `engine::smooth`; agents use `smooth`.

use crate::engine::smooth::{self, Smooth};
use crate::theme::th;
use crate::units::LenUnit;
use crate::App;
use eframe::egui;
use egui::RichText;

/// A tolerance box: mm to 3 decimals, inches to 4.
pub(crate) fn tol_drag(mm: &mut f64, unit: LenUnit) -> egui::DragValue<'_> {
    let speed = match unit {
        LenUnit::Mm => 0.005,
        LenUnit::Inch => 0.0254 / 4.0,
    };
    egui::DragValue::new(mm)
        .range(0.001..=smooth::MAX_TOL)
        .speed(speed)
        .custom_formatter(move |v, _| match unit {
            LenUnit::Mm => format!("{} mm", trim(format!("{v:.3}"))),
            LenUnit::Inch => format!("{} in", trim(format!("{:.4}", v / crate::units::MM_PER_INCH))),
        })
        .custom_parser(move |s| unit.parse(s))
}

fn trim(s: String) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

impl App {
    /// Smooth the open part's drawing with `o` (SM21), as one undo step; the
    /// values are kept for next time.
    pub fn smooth_open_part(&mut self, o: Smooth) {
        let Some(i) = self.shown_part() else { return };
        let mut parts = self.parts.clone();
        let mut sheets = self.result.sheets.clone();
        match smooth::smooth_part(&mut parts, &mut sheets, i, None, &o) {
            Ok(done) => {
                if self.cfg.smooth.command != o {
                    self.cfg.smooth.command = o;
                    let _ = self.cfg.save();
                }
                if !done.done.contours.is_empty() {
                    self.checkpoint();
                    self.parts = parts;
                    self.result.sheets = sheets;
                    // The elements are new: a selected one or a pick on the
                    // old drawing would name a different piece now.
                    self.sel_elems.clear();
                    self.sel_elem_info.clear();
                    self.sel_pick = None;
                    if done.nested {
                        self.stale = true;
                    }
                    self.refresh();
                }
                self.status = done.message;
            }
            Err(e) => self.status = e,
        }
    }

    /// The Part screen's Edit geometry group: Smooth, and (when the
    /// original's missing commands are shown) the drafting menus still to
    /// come.
    pub fn edit_geometry_group(&mut self, ui: &mut egui::Ui) {
        use crate::drawing::{dropdown, soon_menu};
        use crate::ui_parts::{ribbon_button, ribbon_group, Icon};
        let custom = self.shown_part().is_some_and(|i| matches!(self.parts[i].shape, crate::nest::Shape::Custom) && self.parts[i].custom.is_some());
        let unbuilt = self.dv.show_unbuilt;
        ribbon_group(ui, "Edit geometry", |ui| {
            let r = ribbon_button(ui, Icon::Smooth, "Smooth", self.dv.smooth.is_some(), custom)
                .on_hover_text("Smooth the drawing: radii and round holes drawn as many short straight lines become true arcs, so they cut smooth instead of in facets. Straight edges and sharp corners stay as they are.")
                .on_disabled_hover_text("Smooth is for imported drawings: a built-in shape's arcs are exact already.");
            if r.clicked() {
                self.dv.smooth = if self.dv.smooth.is_some() { None } else { Some(self.cfg.smooth.command) };
            }
            if unbuilt {
                let r = dropdown(ui, Icon::Joint, "Modify", true);
                soon_menu(&r, "Modify", &["Fillets", "Chamfer", "Offset", "Lengthen elements", "Join elements", "Fitting: By points, Make tangent, Reduce overlappings", "Shorten-lengthen", "Join and fillet"]);
                let r = dropdown(ui, Icon::Rotate, "Transform", true);
                soon_menu(&r, "Transform", &["Copy", "Move", "Stretch", "Rotate", "Scale", "Place Near", "Mirror", "Break Parts", "Union", "Subtraction", "Intersection", "Change Cutting Direction", "Contour Ordering", "Transform One-Touch Figures"]);
                let r = dropdown(ui, Icon::Grid, "Figures", true);
                soon_menu(&r, "One-touch Figures", &["One-touch figures 1–12"]);
            }
        });
    }

    /// The Smooth window: the tolerance, largest radius and fewest lines per
    /// arc, then Smooth (the open part) or Close.
    pub fn smooth_window(&mut self, ctx: &egui::Context) {
        let Some(mut o) = self.dv.smooth else { return };
        let unit = self.cfg.nesting.units;
        let mut open = true;
        let mut run = false;
        let mut close = false;
        egui::Window::new("Smooth")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(340.0)
            .show(ctx, |ui| {
                ui.label(RichText::new("Runs of short straight lines that lie on one circle become true arcs, each less than a half circle and ending on the drawing's own points. Every line, its middle included, must stay within the tolerance of its arc.").size(12.5).color(th().text_dim));
                ui.add_space(6.0);
                egui::Grid::new("smooth_grid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("Tolerance");
                    ui.add(tol_drag(&mut o.tol, unit)).on_hover_text("The largest gap allowed between a line and its arc. The drawing's lines sag between their points; a coarse radius needs more.");
                    ui.end_row();
                    ui.label("Largest radius");
                    ui.add(crate::units::len_drag(&mut o.max_r, unit, 1.0, 1_000_000.0)).on_hover_text("Lines that would need a larger radius stay straight lines.");
                    ui.end_row();
                    ui.label("Fewest lines per arc");
                    ui.add(egui::DragValue::new(&mut o.min_lines).range(0..=smooth::MAX_MIN_LINES).speed(0.2)).on_hover_text("An arc replaces at least this many lines (never fewer than 2).");
                    ui.end_row();
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Smooth the part").on_hover_text("Smooth every contour of the open part (Ctrl+Z takes it back)").clicked() {
                        run = true;
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });
        self.dv.smooth = (open && !close).then_some(o);
        if run {
            self.smooth_open_part(o);
        }
    }
}
