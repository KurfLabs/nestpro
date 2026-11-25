//! The job's own sheet list on screen (JS1–JS6): its rows in the Nesting
//! Explorer (drawn sheets, how many, drag to reorder, click or right-click
//! for the line's menu) and the Add sheets window (stock cards, or a size of
//! its own with a preview). The rules are `engine::sheetlist`, which the
//! agents' `sheet-list` operation calls too.

use crate::engine::sheetlist;
use crate::nest::{ListSheet, SheetSpec};
use crate::stock::{self, Item, Stock};
use crate::stockpics;
use crate::theme::th;
use crate::App;
use eframe::egui;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, RichText, Sense, Stroke};

/// Which half of the Add sheets window is shown.
#[derive(Clone, Copy, Default, PartialEq)]
enum Tab {
    #[default]
    Stock,
    Size,
}

/// Which stock cards are shown.
#[derive(Clone, Copy, Default, PartialEq)]
enum Filter {
    #[default]
    All,
    Remnants,
    Sheets,
}

/// What the Add sheets window holds between frames.
#[derive(Default)]
pub struct SheetListUi {
    pub open: bool,
    /// The stock's sheets, remnants and drops for the job's material and
    /// thickness, read when the window opens (not locked ones, SS4), each
    /// with what is free of it for this job.
    items: Vec<Item>,
    /// Why the stock couldn't be read, if it couldn't.
    stock_note: Option<String>,
    /// The job it was opened for: switching jobs closes it, so its stock
    /// cards never go on another job's list.
    job: usize,
    tab: Tab,
    filter: Filter,
    /// The stock item picked on the stock half.
    pick_ref: Option<String>,
    /// The size of its own on the size half, mm.
    w: f64,
    h: f64,
    qty: u32,
    as_needed: bool,
    /// What the last Add said (text, it worked).
    said: Option<(String, bool)>,
}

/// A line's menu choice.
enum Act {
    Qty(Option<u32>),
    Move(usize),
    Remove,
}

/// Dragging a line of the list to another place.
#[derive(Clone, Copy)]
struct LineDrag(usize);

/// A line drawn as the stock window draws a sheet: the job's material, a
/// remnant's real shape and amber edge.
fn line_item(l: &ListSheet, spec: &SheetSpec) -> Item {
    let mut it = Item::sheet(if l.stock_ref.is_empty() { "size" } else { &l.stock_ref }, spec.material, spec.thickness, l.w, l.h, 1);
    if l.remnant {
        it.kind = stock::Kind::UserRemnant;
        it.outline = l.outline.clone();
    }
    it
}

/// Draw `it` to fit inside `r`, centred.
fn paint_fit(p: &egui::Painter, r: Rect, it: &Item) {
    let s = stockpics::common_scale([it], r.size());
    let d = vec2(it.length as f32 * s, it.width as f32 * s);
    let origin = pos2(r.center().x - d.x / 2.0, r.center().y + d.y / 2.0);
    stockpics::paint_item(&p.with_clip_rect(r.expand(1.0)), origin, s, it);
}

/// A rounded pill of text, right-aligned at `right`; returns its box.
fn pill(p: &egui::Painter, right: egui::Pos2, text: &str, bg: Color32, fg: Color32, size: f32) -> Rect {
    let g = p.layout_no_wrap(text.to_string(), FontId::proportional(size), fg);
    let r = Rect::from_min_size(pos2(right.x - g.size().x - 12.0, right.y - g.size().y / 2.0 - 2.0), g.size() + vec2(12.0, 4.0));
    p.rect_filled(r, 9.0, bg);
    p.galley(r.min + vec2(6.0, 2.0), g, fg);
    r
}

/// "×2", or "all" for as many as needed.
fn qty_pill(q: Option<u32>) -> String {
    q.map_or("all".into(), |n| format!("×{n}"))
}

/// What the line is, in two lines: the stock reference or the size, then
/// what kind of sheet.
fn line_texts(l: &ListSheet, u: crate::units::LenUnit) -> (String, String, Color32) {
    if l.stock_ref.is_empty() {
        (u.size(l.w, l.h, 0), "Size of its own".into(), th().text_dim)
    } else if l.remnant {
        // Amber like its edge says remnant; the menu and tooltip say it in words.
        (l.stock_ref.clone(), u.size(l.w, l.h, 0), stockpics::remnant_edge())
    } else {
        (l.stock_ref.clone(), u.size(l.w, l.h, 0), th().text_dim)
    }
}

/// One line of the list in the Nesting Explorer: its place, the sheet
/// drawn, what it is, and how many.
fn list_row(ui: &mut egui::Ui, i: usize, l: &ListSheet, spec: &SheetSpec, u: crate::units::LenUnit) -> egui::Response {
    let w = ui.available_width().max(160.0);
    let (rect, r) = ui.allocate_exact_size(vec2(w, 36.0), Sense::click_and_drag());
    let p = ui.painter();
    if r.hovered() || r.dragged() || egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&r)) {
        p.rect_filled(rect, 5.0, th().hover);
    }
    let c = rect.center().y;
    p.text(pos2(rect.left() + 6.0, c), Align2::CENTER_CENTER, format!("{}", i + 1), FontId::proportional(11.0), th().text_dim);
    let pic = Rect::from_center_size(pos2(rect.left() + 29.0, c), vec2(30.0, 22.0));
    paint_fit(p, pic, &line_item(l, spec));
    let right = pos2(rect.right() - 6.0, c);
    let pr = pill(p, right, &qty_pill(l.qty), th().accent_soft, th().accent_fg, 12.0);
    let (a, b, bc) = line_texts(l, u);
    let tp = p.with_clip_rect(Rect::from_min_max(pos2(pic.right() + 8.0, rect.top()), pos2(pr.left() - 4.0, rect.bottom())));
    tp.text(pos2(pic.right() + 7.0, c - 7.5), Align2::LEFT_CENTER, a, FontId::proportional(13.0), th().text_strong);
    tp.text(pos2(pic.right() + 7.0, c + 8.0), Align2::LEFT_CENTER, b, FontId::proportional(11.0), bc);
    if r.hovered() && !r.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

/// The line's menu: how many, move, remove.
fn line_menu(ui: &mut egui::Ui, i: usize, n: usize, l: &ListSheet, u: crate::units::LenUnit) -> Option<Act> {
    let mut act = None;
    ui.set_min_width(220.0);
    let (a, b, bc) = line_texts(l, u);
    ui.label(RichText::new(a).strong().color(th().text_strong));
    let kind = match (l.stock_ref.is_empty(), l.remnant) {
        (true, _) => String::new(),
        (false, true) => "Remnant from stock · ".into(),
        (false, false) => "Stock sheet · ".into(),
    };
    ui.label(RichText::new(if l.stock_ref.is_empty() { b } else { format!("{kind}{b}") }).size(11.0).color(bc));
    ui.add_space(4.0);
    let mut as_needed = l.qty.is_none();
    let mut q = l.qty.unwrap_or(1);
    ui.horizontal(|ui| {
        ui.label("How many");
        ui.add_enabled_ui(!as_needed, |ui| {
            if let Some(v) = stepper(ui, &mut q) {
                act = Some(Act::Qty(Some(v)));
            }
        });
    });
    if ui.checkbox(&mut as_needed, "As many as needed").on_hover_text("Nest all puts on this sheet everything that fits it; the lines after it get only what doesn't").changed() {
        act = Some(Act::Qty(if as_needed { None } else { Some(q) }));
    }
    ui.separator();
    if ui.add_enabled(i > 0, egui::Button::new("⬆  Move up")).clicked() {
        act = Some(Act::Move(i - 1));
        ui.close();
    }
    if ui.add_enabled(i + 1 < n, egui::Button::new("⬇  Move down")).clicked() {
        act = Some(Act::Move(i + 1));
        ui.close();
    }
    if ui.button(RichText::new("🗑  Remove from the list").color(th().warn_fg)).clicked() {
        act = Some(Act::Remove);
        ui.close();
    }
    act
}

/// − [n] +, 1 to 999; Some(new value) when it changed.
fn stepper(ui: &mut egui::Ui, q: &mut u32) -> Option<u32> {
    let before = *q;
    ui.spacing_mut().item_spacing.x = 2.0;
    if ui.add_enabled(*q > 1, egui::Button::new("−").min_size(vec2(24.0, 22.0))).on_hover_text("One sheet fewer").clicked() {
        *q -= 1;
    }
    ui.add(egui::DragValue::new(q).range(1..=sheetlist::MAX_QTY).speed(0.2));
    if ui.add_enabled(*q < sheetlist::MAX_QTY, egui::Button::new("+").min_size(vec2(24.0, 22.0))).on_hover_text("One sheet more").clicked() {
        *q += 1;
    }
    (*q != before).then_some(*q)
}

/// A stock item as a card to pick: the sheet drawn to a shared scale, its
/// reference, kind, size, what is free and what the list already asks of it.
fn stock_card(ui: &mut egui::Ui, it: &Item, scale: f32, on: bool, listed: Option<Option<u32>>, u: crate::units::LenUnit) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(vec2(170.0, 122.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 7.0, if on { th().accent_soft } else if r.hovered() { th().hover } else { th().card });
    let edge = if on { Stroke::new(2.0, th().accent) } else if r.hovered() { Stroke::new(1.0, th().accent_border) } else { Stroke::new(1.0, th().border) };
    p.rect_stroke(rect, 7.0, edge, egui::StrokeKind::Inside);
    let band = Rect::from_min_size(rect.min + vec2(10.0, 10.0), vec2(rect.width() - 20.0, 54.0));
    let d = vec2(it.length as f32 * scale, it.width as f32 * scale);
    let origin = pos2(band.left(), band.center().y + d.y / 2.0);
    stockpics::paint_item(&p.with_clip_rect(band.expand(1.0)), origin, scale, it);
    pill(p, pos2(band.right(), band.top() + 9.0), &format!("{} free", it.qty), Color32::from_black_alpha(170), Color32::WHITE, 11.0);
    let x = rect.left() + 10.0;
    let mut y = band.bottom() + 8.0;
    p.text(pos2(x, y), Align2::LEFT_TOP, &it.reference, FontId::proportional(13.5), th().text_strong);
    let (kind, kc) = if it.kind.is_remnant() { ("Remnant", stockpics::remnant_edge()) } else { ("Sheet", th().text_dim) };
    p.text(pos2(rect.right() - 10.0, y + 1.0), Align2::RIGHT_TOP, kind, FontId::proportional(11.0), kc);
    y += 19.0;
    p.text(pos2(x, y), Align2::LEFT_TOP, u.size(it.length, it.width, 0), FontId::proportional(11.5), th().text);
    if let Some(q) = listed {
        pill(p, pos2(rect.right() - 8.0, y + 8.0), &format!("on list {}", qty_pill(q)), th().ok_bg, th().ok_fg, 10.5);
    }
    if on {
        p.circle_filled(rect.left_top() + vec2(12.0, 12.0), 8.0, th().accent);
        p.text(rect.left_top() + vec2(12.0, 12.0), Align2::CENTER_CENTER, "✔", FontId::proportional(10.0), Color32::WHITE);
    }
    r.on_hover_text("Click to pick it, double-click to add it straight away")
}

/// A size to pick in one click: "1524 × 1219".
fn size_chip(ui: &mut egui::Ui, on: bool, text: String) -> egui::Response {
    let fill = if on { th().accent_soft } else { th().card };
    let edge = if on { th().accent } else { th().border };
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, edge))
        .corner_radius(12)
        .inner_margin(egui::Margin::symmetric(10, 3))
        .show(ui, |ui| ui.add(egui::Label::new(RichText::new(text).size(12.0).color(if on { th().accent_fg } else { th().text })).extend()))
        .response
        .interact(Sense::click())
}

/// A tab of the window: label and a count beside it.
fn tab(ui: &mut egui::Ui, on: bool, label: &str, count: Option<usize>) -> egui::Response {
    let text = match count {
        Some(n) => format!("{label}  {n}"),
        None => label.to_string(),
    };
    let fill = if on { th().accent } else { th().card };
    let fg = if on { Color32::WHITE } else { th().text };
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, if on { th().accent } else { th().border }))
        .corner_radius(14)
        .inner_margin(egui::Margin::symmetric(14, 5))
        .show(ui, |ui| ui.add(egui::Label::new(RichText::new(text).size(13.0).strong().color(fg)).extend()))
        .response
        .interact(Sense::click())
}

impl App {
    /// Nesting Explorer → Add sheet size… (JS1).
    pub fn open_sheet_list_add(&mut self) {
        let mut u = SheetListUi { open: true, job: self.cur_job, w: self.spec.w, h: self.spec.h, qty: 1, ..Default::default() };
        match Stock::load(&self.library_dir()) {
            Ok(st) => {
                let free = |it: &Item| it.qty + self.held.get(&it.reference).copied().unwrap_or(0);
                u.items = st.items.iter().filter(|it| sheetlist::check_stock_item(&self.spec, it).is_ok() && (free(it) > 0 || self.cfg.stock.allow_no_stock)).cloned().collect();
                for it in u.items.iter_mut() {
                    it.qty = free(it);
                }
                // Drops and remnants first (they're why the list is there),
                // then the sheets; biggest first within each.
                u.items.sort_by(|a, b| b.kind.is_remnant().cmp(&a.kind.is_remnant()).then((b.length * b.width).total_cmp(&(a.length * a.width))));
            }
            Err(e) => u.stock_note = Some(format!("The sheet stock can't be read: {e}")),
        }
        u.tab = if u.items.is_empty() { Tab::Size } else { Tab::Stock };
        self.sheet_list_ui = u;
    }

    /// Screenshots: the first stock item picked in the open window.
    pub fn sheet_list_ui_pick_demo(&mut self) {
        self.sheet_list_ui.pick_ref = self.sheet_list_ui.items.first().map(|i| i.reference.clone());
        self.sheet_list_ui.qty = 1;
    }

    /// Screenshots: the size half of the open window, on a standard drop.
    pub fn sheet_list_ui_size_demo(&mut self) {
        self.cfg.stock.drops.sizes = vec![[1828.8, 1219.2], [1219.2, 1219.2], [1219.2, 609.6], [609.6, 609.6]];
        self.sheet_list_ui.tab = Tab::Size;
        self.sheet_list_ui.w = 1219.2;
        self.sheet_list_ui.h = 609.6;
        self.sheet_list_ui.qty = 2;
    }

    /// Add a line to the job's sheet list (JS1): one undo step.
    pub fn add_sheet_list_line(&mut self, l: ListSheet) -> Result<String, String> {
        let before = self.snapshot();
        let m = sheetlist::add(&mut self.spec, l)?;
        self.push_undo(before);
        self.sheet_list_changed();
        Ok(m)
    }

    /// Take line `i` off the job's sheet list.
    pub fn remove_sheet_list_line(&mut self, i: usize) {
        let before = self.snapshot();
        if let Ok(l) = sheetlist::remove(&mut self.spec, i) {
            self.push_undo(before);
            self.sheet_list_changed();
            self.status = format!("Took {} off the job's sheet list (Ctrl+Z puts it back).", sheetlist::describe(&l));
        }
    }

    /// Move line `i` to place `to` (the order Nest all takes them in, JS3).
    pub fn move_sheet_list_line(&mut self, i: usize, to: usize) {
        let before = self.snapshot();
        if i != to && sheetlist::move_to(&mut self.spec, i, to).is_ok() {
            self.push_undo(before);
            self.sheet_list_changed();
            self.status = format!("Line {} moved to {}: Nest all takes the list top to bottom.", i + 1, to + 1);
        }
    }

    /// Empty the job's sheet list.
    pub fn clear_sheet_list(&mut self) {
        if self.spec.sheet_list.is_empty() {
            return;
        }
        self.checkpoint();
        self.spec.sheet_list.clear();
        self.sheet_list_changed();
        self.status = "The job's sheet list is empty: Nest all uses the stock and the job's sheet size again (Ctrl+Z puts it back).".into();
    }

    fn sheet_list_changed(&mut self) {
        if !self.result.sheets.is_empty() {
            self.stale = true;
        }
        self.refresh();
    }

    /// The job's sheet list in the Nesting Explorer (JS5): a header with
    /// Add, then one drawn row per line. Click or right-click a row for its
    /// menu; drag it to change the order.
    pub(crate) fn sheet_list_node(&mut self, ui: &mut egui::Ui) {
        let u = self.unit();
        let n = self.spec.sheet_list.len();
        if n == 0 {
            // Empty: one dashed row that says what it's for.
            let w = ui.available_width().max(160.0);
            let (rect, r) = ui.allocate_exact_size(vec2(w, 30.0), Sense::click());
            let p = ui.painter();
            if r.hovered() {
                p.rect_filled(rect.shrink(2.0), 5.0, th().hover);
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            stockpics::dashed_rect(p, rect.shrink(2.0), Stroke::new(1.0, th().accent_border));
            p.text(rect.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, "+  Add sheet sizes or drops", FontId::proportional(12.5), th().accent_fg);
            if r.on_hover_text("Pick the sizes, stock sheets, remnants or drops this job nests on: Nest all fills them first, top to bottom").clicked() {
                self.open_sheet_list_add();
            }
            return;
        }
        let mut act: Option<(usize, Act)> = None;
        let mut add = false;
        let mut clear = false;
        let id = ui.make_persistent_id("sheet_list_node");
        let spec = self.spec.clone();
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
            .show_header(ui, |ui| {
                let h = ui.label(RichText::new("Sheets to nest on").strong().color(th().text_strong));
                h.on_hover_text("This job's own sheets: Nest all fills them first, top to bottom, then goes on with the stock and the job's sheet size as the Do All options say. Drag a row to change the order.").context_menu(|ui| {
                    if ui.button("Add sheet size…").clicked() {
                        add = true;
                        ui.close();
                    }
                    if ui.button("Clear the list").on_hover_text("Nest all goes back to the stock and the job's sheet size").clicked() {
                        clear = true;
                        ui.close();
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let b = ui.add(egui::Button::new(RichText::new("+ Add").size(12.0).color(th().accent_fg)).frame(false));
                    if b.on_hover_text("Add a sheet size, stock sheet, remnant or drop").clicked() {
                        add = true;
                    }
                });
            })
            .body(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for (i, l) in spec.sheet_list.iter().enumerate() {
                    let r = list_row(ui, i, l, &spec, u);
                    let tip = format!("{}. {}\nClick or right-click to change how many, move or remove it. Drag to change the order.", i + 1, sheetlist::describe(l));
                    // Drag to reorder: a line between rows shows where it lands.
                    if r.drag_started() {
                        egui::DragAndDrop::set_payload(ui.ctx(), LineDrag(i));
                    }
                    if r.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    }
                    let below = ui.ctx().pointer_interact_pos().is_some_and(|p| p.y > r.rect.center().y);
                    if let Some(from) = r.dnd_hover_payload::<LineDrag>().filter(|f| f.0 != i) {
                        let _ = from;
                        let y = if below { r.rect.bottom() + 1.0 } else { r.rect.top() - 1.0 };
                        ui.painter().line_segment([pos2(r.rect.left() + 4.0, y), pos2(r.rect.right() - 4.0, y)], Stroke::new(2.5, th().accent));
                    }
                    if let Some(from) = r.dnd_release_payload::<LineDrag>() {
                        let before = if below { i + 1 } else { i };
                        let to = if from.0 < before { before - 1 } else { before };
                        if to != from.0 {
                            act = Some((from.0, Act::Move(to)));
                        }
                    }
                    let n = spec.sheet_list.len();
                    if egui::DragAndDrop::has_any_payload(ui.ctx()) {
                        continue;
                    }
                    let r = r.on_hover_text(tip);
                    // One menu, opened by a click or a right-click.
                    let mut menu = egui::Popup::menu(&r);
                    if r.secondary_clicked() {
                        menu = menu.open_memory(Some(egui::SetOpenCommand::Bool(true)));
                    }
                    menu.show(|ui| {
                        if let Some(a) = line_menu(ui, i, n, l, u) {
                            act = Some((i, a));
                        }
                    });
                }
            });
        match act {
            Some((i, Act::Qty(q))) => {
                let before = self.snapshot();
                let now = ui.input(|inp| inp.time);
                if sheetlist::set_qty(&mut self.spec, i, q).is_ok() {
                    // Stepping or dragging the number is one undo step.
                    self.push_undo_coalesced(before, now);
                    self.sheet_list_changed();
                }
            }
            Some((i, Act::Move(to))) => self.move_sheet_list_line(i, to),
            Some((i, Act::Remove)) => self.remove_sheet_list_line(i),
            None => {}
        }
        if add {
            self.open_sheet_list_add();
        }
        if clear {
            self.clear_sheet_list();
        }
    }

    /// The Add sheets window (JS1, JS6): stock cards or a size of its own,
    /// how many, Add. It stays open for the next one.
    pub fn sheet_list_window(&mut self, ctx: &egui::Context) {
        if !self.sheet_list_ui.open {
            return;
        }
        if self.sheet_list_ui.job != self.cur_job {
            self.sheet_list_ui.open = false;
            return;
        }
        let u = self.unit();
        let mut open = true;
        let mut add: Option<ListSheet> = None;
        let mut close = false;
        let mat = self.mat_thk(self.spec.material, self.spec.thickness);
        let (material, thickness) = (self.spec.material, self.spec.thickness);
        let drops = self.cfg.stock.drops.sizes.clone();
        let listed: Vec<ListSheet> = self.spec.sheet_list.clone();
        let job_size = (self.spec.w, self.spec.h);
        let s = &mut self.sheet_list_ui;
        let on_list = |r: &str| listed.iter().find(|l| l.stock_ref == r).map(|l| l.qty);
        egui::Window::new("Add sheets to nest on").open(&mut open).collapsible(false).resizable(false).min_width(580.0).max_width(580.0).anchor(Align2::CENTER_TOP, [0.0, 110.0]).show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            // What the sheets are for: the job's material, and the rule.
            ui.horizontal(|ui| {
                stockpics::swatch(ui, material, vec2(18.0, 18.0));
                ui.label(RichText::new(&mat).strong().color(th().text_strong));
                ui.label(RichText::new("· Nest all fills the job's list first, top to bottom").color(th().text_dim));
            });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if tab(ui, s.tab == Tab::Stock, "From stock", Some(s.items.len())).on_hover_text("Sheets, remnants and drops in stock for this material and thickness").clicked() {
                    s.tab = Tab::Stock;
                }
                if tab(ui, s.tab == Tab::Size, "Size of its own", None).on_hover_text("Type a size, or pick one of your standard drops or a common sheet size").clicked() {
                    s.tab = Tab::Size;
                }
            });
            ui.separator();
            // The body keeps one height, so switching halves doesn't make the window jump.
            let body_h = 290.0;
            ui.allocate_ui(vec2(ui.available_width(), body_h), |ui| {
                ui.set_min_height(body_h);
                match s.tab {
                    Tab::Stock => {
                        if let Some(n) = &s.stock_note {
                            ui.label(RichText::new(n).color(th().warn_fg));
                            return;
                        }
                        if s.items.is_empty() {
                            ui.add_space(60.0);
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new("Nothing in stock for this material and thickness.").color(th().text_dim));
                                if ui.button("Type a size instead").clicked() {
                                    s.tab = Tab::Size;
                                }
                            });
                            return;
                        }
                        let rem = s.items.iter().filter(|i| i.kind.is_remnant()).count();
                        if rem > 0 && rem < s.items.len() {
                            ui.horizontal(|ui| {
                                for (f, t) in [(Filter::All, format!("All {}", s.items.len())), (Filter::Remnants, format!("Remnants & drops {rem}")), (Filter::Sheets, format!("Sheets {}", s.items.len() - rem))] {
                                    if size_chip(ui, s.filter == f, t).clicked() {
                                        s.filter = f;
                                    }
                                }
                            });
                        }
                        let shown: Vec<&Item> = s
                            .items
                            .iter()
                            .filter(|i| match s.filter {
                                Filter::All => true,
                                Filter::Remnants => i.kind.is_remnant(),
                                Filter::Sheets => !i.kind.is_remnant(),
                            })
                            .collect();
                        let scale = stockpics::common_scale(shown.iter().copied(), vec2(150.0, 54.0));
                        let mut pick = None;
                        let mut quick = None;
                        egui::ScrollArea::vertical().max_height(body_h - 30.0).auto_shrink([false, false]).show(ui, |ui| {
                            egui::Grid::new("sheet_list_cards").spacing([10.0, 10.0]).show(ui, |ui| {
                                for (k, it) in shown.iter().enumerate() {
                                    let on = s.pick_ref.as_deref() == Some(it.reference.as_str());
                                    let r = stock_card(ui, it, scale, on, on_list(&it.reference), u);
                                    if r.clicked() {
                                        pick = Some(it.reference.clone());
                                    }
                                    if r.double_clicked() {
                                        quick = Some((*it).clone());
                                    }
                                    if k % 3 == 2 {
                                        ui.end_row();
                                    }
                                }
                            });
                        });
                        if let Some(p) = pick {
                            s.pick_ref = Some(p);
                        }
                        if let Some(it) = quick {
                            add = Some(ListSheet::stock(&it, (!s.as_needed).then_some(s.qty.max(1))));
                        }
                    }
                    Tab::Size => {
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width(300.0);
                                egui::Grid::new("sheet_list_size").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                                    ui.label("Length X");
                                    crate::units::drag_len_in(ui, &mut s.w, u, 1.0, 20000.0);
                                    ui.end_row();
                                    ui.label("Width Y");
                                    ui.horizontal(|ui| {
                                        crate::units::drag_len_in(ui, &mut s.h, u, 1.0, 20000.0);
                                        if ui.small_button("⇄").on_hover_text("Swap length and width").clicked() {
                                            std::mem::swap(&mut s.w, &mut s.h);
                                        }
                                    });
                                    ui.end_row();
                                });
                                ui.add_space(6.0);
                                let mut chips = |ui: &mut egui::Ui, title: &str, sizes: Vec<(f64, f64)>| {
                                    if sizes.is_empty() {
                                        return;
                                    }
                                    ui.label(RichText::new(title).size(11.5).color(th().text_dim));
                                    ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().item_spacing = vec2(5.0, 5.0);
                                        for (w, h) in sizes {
                                            let on = (s.w - w).abs() < crate::nest::SIZE_TOL && (s.h - h).abs() < crate::nest::SIZE_TOL;
                                            if size_chip(ui, on, u.dims(w, h, 0)).clicked() {
                                                s.w = w;
                                                s.h = h;
                                            }
                                        }
                                    });
                                    ui.add_space(4.0);
                                };
                                chips(ui, "Your standard drops", drops.iter().map(|d| (d[0].max(d[1]), d[0].min(d[1]))).collect());
                                chips(ui, "Common sheet sizes", u.sheet_presets().to_vec());
                                chips(ui, "This job's sheet", vec![job_size]);
                            });
                            ui.add_space(8.0);
                            // The size drawn with its dimensions, in the job's material.
                            let it = Item::sheet("size", material, thickness, s.w.max(1.0), s.h.max(1.0), 1);
                            ui.vertical(|ui| {
                                ui.add_space(14.0);
                                stockpics::item_dimensioned(ui, &it, vec2(230.0, 190.0), u);
                            });
                        });
                    }
                }
            });
            ui.separator();
            // How many, then Add.
            ui.horizontal(|ui| {
                ui.label(RichText::new("How many").strong().color(th().text_strong));
                ui.add_enabled_ui(!s.as_needed, |ui| {
                    stepper(ui, &mut s.qty);
                });
                ui.add_space(6.0);
                ui.checkbox(&mut s.as_needed, "As many as needed").on_hover_text("Nest all puts on this sheet everything that fits it; the lines after it get only what doesn't");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Done").clicked() {
                        close = true;
                    }
                    let q = (!s.as_needed).then_some(s.qty.max(1));
                    let picked = match s.tab {
                        Tab::Stock => s.pick_ref.as_ref().and_then(|r| s.items.iter().find(|i| &i.reference == r)).map(|it| ListSheet::stock(it, q)),
                        Tab::Size => Some(ListSheet::size(s.w, s.h, q)),
                    };
                    let text = match &picked {
                        Some(l) => format!("Add {} {}", line_texts(l, u).0, qty_pill(q)),
                        None => "Pick a sheet".to_string(),
                    };
                    let b = egui::Button::new(RichText::new(text).strong().color(Color32::WHITE)).fill(th().accent).min_size(vec2(150.0, 30.0)).corner_radius(6);
                    if ui.add_enabled(picked.is_some(), b).clicked() {
                        add = picked;
                    }
                });
            });
            if let Some((m, ok)) = &s.said {
                let (bg, fg, icon) = if *ok { (th().ok_bg, th().ok_fg, "✔") } else { (th().warn_bg, th().warn_fg, "⚠") };
                egui::Frame::new().fill(bg).corner_radius(6).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(format!("{icon}  {m}")).color(fg));
                });
            }
        });
        if let Some(l) = add {
            let r = self.add_sheet_list_line(l);
            if let Ok(m) = &r {
                self.status = format!("{m} Nest all to use it.");
            }
            self.sheet_list_ui.said = Some(match r {
                Ok(m) => (format!("{m} Add another, or Done."), true),
                Err(e) => (e, false),
            });
        }
        if !open || close {
            self.sheet_list_ui.open = false;
        }
    }
}
