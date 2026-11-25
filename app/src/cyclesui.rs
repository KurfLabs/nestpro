//! Sheet cycles (SR1, SR6–SR10): how many times a nested sheet is cut. One
//! layout run as several identical sheets: its parts count as nested and its
//! stock sheet as taken that many times over, and an edit to it changes every
//! cycle. The window asks for the new number between 1 and the most the parts
//! left and the stock allow; agents use `sheet-cycles`.

use crate::engine::{self, stockflow, CyclesLimit};
use crate::theme::th;
use crate::App;
use eframe::egui;
use egui::{Key, RichText};

/// The open Cycles window: which sheet, the number being typed, and the
/// limits worked out when it opened (the stock is read once).
#[derive(Clone, Debug)]
pub struct CyclesDlg {
    pub sheet: usize,
    pub n: usize,
    pub limit: CyclesLimit,
    /// The sheet's stock item's sheets free for it, when it is a stock sheet.
    pub in_stock: Option<usize>,
    /// The job, the sheet's layout and the number of sheets it was opened on:
    /// the window closes when any changes, so OK never sets another sheet's
    /// cycles.
    pub job: usize,
    pub layout: Vec<crate::nest::Placed>,
    pub sheets: usize,
}

impl App {
    /// How many times sheet `k` can be cut (SR3, SR4).
    pub fn cycles_limit_at(&self, k: usize) -> CyclesLimit {
        let cap = stockflow::stock_cap(&self.library_dir(), &self.cfg.stock, &self.held, &self.result.formats, self.result.sheets.len(), k);
        engine::cycles_limit(&self.parts, &self.result.sheets, &self.result.formats, k, cap)
    }

    /// Open the Cycles window for sheet `k` (Sheets → Sheet cycles, or the
    /// Nesting Explorer's right-click). The new number starts at the most
    /// the sheet can be cut when the parts left fill it again, else at what
    /// it is now (SR6).
    pub fn open_cycles(&mut self, k: usize) {
        if k >= self.result.sheets.len() {
            self.status = "Nest first — there is no sheet to cut again.".into();
            return;
        }
        let limit = self.cycles_limit_at(k);
        let n = if self.result.sheets[k].is_empty() { limit.now } else { limit.most().max(limit.now) };
        let in_stock = {
            let f = self.result.format(k, &self.spec);
            (!f.stock_ref.is_empty()).then(|| stockflow::stock_cap(&self.library_dir(), &crate::stock::StockSettings { allow_no_stock: false, ..self.cfg.stock.clone() }, &self.held, &self.result.formats, self.result.sheets.len(), k)).flatten()
        };
        self.cycles_dlg = Some(CyclesDlg { sheet: k, n, limit, in_stock, job: self.cur_job, layout: self.result.sheets[k].clone(), sheets: self.result.sheets.len() });
    }

    /// Cut sheet `k` `n` times (SR8–SR10), with undo. The stock follows on
    /// the next save (SS10).
    pub fn set_sheet_cycles(&mut self, k: usize, n: usize) -> bool {
        let limit = self.cycles_limit_at(k);
        if let Some(why) = limit.refusal(k, n) {
            self.status = why;
            return false;
        }
        if n == limit.now {
            self.status = format!("Sheet {} is already cut {n} time(s).", k + 1);
            return true;
        }
        self.checkpoint();
        let len = self.result.sheets.len();
        match engine::set_cycles(&mut self.result.formats, &self.spec, len, k, n, &limit) {
            Ok(m) => {
                self.mark_cam_changed();
                self.refresh();
                self.status = format!("{m} The job is cut on {}.", engine::sheets_text(&self.result.formats, self.result.sheets.len()));
                true
            }
            Err(e) => {
                self.status = e;
                false
            }
        }
    }

    pub fn cycles_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.cycles_dlg.clone() else { return };
        if d.job != self.cur_job || d.sheets != self.result.sheets.len() || self.result.sheets.get(d.sheet) != Some(&d.layout) {
            self.cycles_dlg = None;
            return;
        }
        let most = d.limit.most().max(d.limit.now);
        let mut open = true;
        let (mut ok, mut cancel) = (false, false);
        let title = format!("Cycles — sheet {}", d.sheet + 1);
        egui::Window::new(RichText::new(title).strong())
            .id(egui::Id::unique("sheet_cycles"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_TOP, egui::Vec2::new(0.0, 120.0))
            .show(ctx, |ui| {
                ui.label(format!("{}W {}", d.sheet + 1, self.sheet_label(d.sheet, 0)));
                ui.label(RichText::new("The same layout cut several times: its parts and its stock sheets count once for every cycle, and a change to the sheet changes every cycle.").size(11.5).color(th().text_dim));
                ui.add_space(4.0);
                egui::Grid::new("cycles_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                    if let Some(free) = d.in_stock {
                        ui.label(RichText::new("Free in stock").color(th().text_dim));
                        ui.label(format!("{free}"));
                        ui.end_row();
                    }
                    ui.label(RichText::new("Cut now").color(th().text_dim));
                    ui.label(format!("{}", d.limit.now));
                    ui.end_row();
                    ui.label(RichText::new("Most").color(th().text_dim));
                    ui.label(format!("{most}")).on_hover_text(self.cycles_most_why(&d.limit));
                    ui.end_row();
                    ui.label(RichText::new("Cut").strong());
                    // SR7: more than the most can't be typed.
                    ui.add(egui::DragValue::new(&mut d.n).range(1..=most).speed(0.1).suffix(" time(s)"));
                    ui.end_row();
                });
                ui.label(RichText::new(self.cycles_most_why(&d.limit)).size(11.5).color(th().text_dim));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                        ok = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if ok {
            let n = d.n.clamp(1, most);
            self.cycles_dlg = None;
            self.set_sheet_cycles(d.sheet, n);
        } else if !open || cancel || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.cycles_dlg = None;
        } else {
            self.cycles_dlg = Some(d);
        }
    }

    /// What limits the cycles, in a line under the number.
    fn cycles_most_why(&self, l: &CyclesLimit) -> String {
        let by_stock = l.by_stock.filter(|&s| s < l.by_parts);
        match by_stock {
            Some(s) => format!("The stock has {s} of this sheet free for it; add sheets to stock to cut it more."),
            None if l.by_parts > l.now => format!("The parts still to nest fill it {} more time(s).", l.by_parts - l.now),
            None => "The parts still to nest don't fill it again: raise their quantities to cut it more. Fewer cycles give parts back.".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::nest::{Part, Shape};
    use crate::App;

    fn app(qty: u32) -> App {
        let mut a = App::new(None);
        a.parts = vec![Part { name: "P".into(), shape: Shape::Rectangle, w: 900.0, h: 900.0, qty, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        a.cfg.stock.equal_sheets = false;
        a.cfg.stock.use_stock = false;
        a.run_nesting();
        a
    }

    /// SR6–SR8: the window cuts a sheet more times as far as the parts left
    /// fill it, refuses beyond, and undo takes it back.
    #[test]
    fn cycles_raise_as_far_as_the_parts_fill_and_undo() {
        let mut a = app(6);
        let per = a.result.sheets[0].len();
        assert!(per < 6, "{per} on a sheet");
        a.cycles_dlg = None;
        a.open_cycles(0);
        let d = a.cycles_dlg.clone().expect("the window opens");
        assert_eq!((d.limit.now, d.limit.most()), (1, 1));
        assert!(!a.set_sheet_cycles(0, 2), "nothing left to fill it again");
        assert!(a.status.contains("raise their quantities"), "{}", a.status);
        a.parts[0].qty += per as u32;
        a.open_cycles(0);
        assert_eq!(a.cycles_dlg.as_ref().map(|d| d.n), Some(2), "starts at the most");
        assert!(a.set_sheet_cycles(0, 2), "{}", a.status);
        assert_eq!(a.total_placed(), 6 + per);
        assert!(a.sheet_label(0, 0).ends_with("(×2)"), "{}", a.sheet_label(0, 0));
        a.undo();
        assert_eq!((a.runs(0), a.total_placed()), (1, 6));
    }
}
