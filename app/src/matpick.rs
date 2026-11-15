//! The Material ▾ (MA23): the one way the window offers the shop's
//! materials, wherever a material is picked (the Sheet tab, the Home tab's
//! sheet line, the new stock sheet; the Sheet stock window's material shelf
//! uses its find box and groups). What it offers is
//! `engine::material::choose`: the materials whose name or other name holds
//! what is typed, grouped by kind; this file only draws it.

use crate::engine::material::{self, Choice, Choices, Id};
use crate::theme::th;
use egui::{RichText, Vec2};

/// The text typed into one picker, kept while the window runs.
fn find_id(salt: &str) -> egui::Id {
    egui::Id::unique(("material_find", salt))
}

/// The find box over a material list. Empty and focused each time the list
/// is opened (when `fresh`). Gives the text and whether Enter was pressed.
pub fn find_box(ui: &mut egui::Ui, salt: &str, fresh: bool, width: f32) -> (String, bool) {
    let id = find_id(salt);
    let mut text: String = ui.data_mut(|d| d.get_temp(id)).unwrap_or_default();
    if fresh {
        text.clear();
    }
    let r = ui.add(egui::TextEdit::singleline(&mut text).hint_text("Find a material…").desired_width(width).id(id.with("box"))).on_hover_text("Type part of a material's name or of one of its other names");
    if fresh {
        r.request_focus();
    }
    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    ui.data_mut(|d| d.insert_temp(id, text.clone()));
    (text, enter)
}

/// What the picker offers for `text` now.
pub fn groups(text: &str) -> Vec<Choices> {
    material::choose(&material::current(), text)
}

/// A kind's heading in a material list.
pub fn group_heading(ui: &mut egui::Ui, g: &Choices) {
    ui.add_space(3.0);
    ui.label(RichText::new(g.group.label()).size(11.0).strong().color(th().text_dim));
}

/// "No material matches …", under an empty list.
pub fn none_match(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(format!("No material matches '{}'. Try fewer letters, or another name.", text.trim())).size(11.5).color(th().text_dim));
}

/// One material row: swatch, name, the other name it was found by, and how
/// many sheets of it are in stock at the job's thickness. True when clicked.
fn row(ui: &mut egui::Ui, c: &Choice, on: bool, in_stock: Option<i32>, width: f32) -> bool {
    let h = 22.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, h), egui::Sense::click());
    let vis = ui.style().interact_selectable(&resp, on);
    if on || resp.hovered() {
        ui.painter().rect_filled(rect, 3.0, vis.weak_bg_fill);
    }
    let sw = egui::Rect::from_center_size(egui::pos2(rect.left() + 12.0, rect.center().y), Vec2::new(14.0, 14.0));
    crate::stockpics::paint_swatch(ui.painter(), sw, c.id);
    let font = egui::FontId::proportional(13.0);
    let name = ui.painter().text(egui::pos2(sw.right() + 7.0, rect.center().y), egui::Align2::LEFT_CENTER, &c.name, font, vis.text_color());
    if let Some(o) = &c.matched {
        ui.painter().text(egui::pos2(name.right() + 6.0, rect.center().y), egui::Align2::LEFT_CENTER, format!("· {o}"), egui::FontId::proportional(11.5), th().text_dim);
    }
    if let Some(n) = in_stock {
        ui.painter().text(egui::pos2(rect.right() - 6.0, rect.center().y), egui::Align2::RIGHT_CENTER, format!("{n} in stock"), egui::FontId::proportional(11.5), th().ok_fg);
    }
    let resp = match &c.matched {
        Some(o) => resp.on_hover_text(format!("Found by its other name {o}")),
        None => resp,
    };
    resp.clicked()
}

/// The list itself: the find box, then the materials by kind, the picked
/// one marked, each with its count in stock at the job's thickness
/// (`in_stock`, as `stockflow::in_stock_by_material` gives it). Enter picks
/// the first material shown. True when the material changed.
pub fn list(ui: &mut egui::Ui, salt: &str, material: &mut Id, in_stock: &[(Id, i32)], width: f32, max_height: f32) -> bool {
    // Freshly opened: not drawn the frame before.
    let seen = egui::Id::unique(("material_list_seen", salt));
    let frame = ui.ctx().cumulative_frame_nr();
    let last: Option<u64> = ui.data_mut(|d| d.get_temp(seen));
    let fresh = last.is_none_or(|f| f + 1 < frame);
    ui.data_mut(|d| d.insert_temp(seen, frame));
    ui.set_max_width(width);
    let (text, enter) = find_box(ui, salt, fresh, width);
    let gs = groups(&text);
    let before = *material;
    if enter {
        if let Some(c) = gs.first().and_then(|g| g.choices.first()) {
            *material = c.id;
        }
    }
    if gs.is_empty() {
        none_match(ui, &text);
    }
    egui::ScrollArea::vertical().id_salt(("material_list", salt)).max_height(max_height).auto_shrink([true, true]).show(ui, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = 1.0;
        for g in &gs {
            group_heading(ui, g);
            for c in &g.choices {
                let n = in_stock.iter().find(|x| x.0 == c.id).map(|x| x.1);
                if row(ui, c, *material == c.id, n, width) {
                    *material = c.id;
                }
            }
        }
    });
    *material != before
}

/// The Material ▾ as a drop-down field: the material's name (`?` for one the
/// list doesn't have, JOB-86); its list closes once a material is picked.
/// True when the material changed.
pub fn combo(ui: &mut egui::Ui, salt: &str, material: &mut Id, width: f32, in_stock: &[(Id, i32)]) -> bool {
    let mut changed = false;
    let list_w = width.max(260.0);
    let r = egui::ComboBox::from_id_salt(("material_combo", salt))
        .width(width)
        .height(460.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .selected_text(material::name_or(*material, "?"))
        .show_ui(ui, |ui| {
            if list(ui, salt, material, in_stock, list_w, 380.0) {
                changed = true;
                ui.close();
            }
        });
    // A screenshot shows the same list open under the field (a popup
    // can't be opened without a click).
    if shot_wanted(ui, salt) {
        egui::Area::new(egui::Id::unique(("material_shot_area", salt))).order(egui::Order::Foreground).fixed_pos(r.response.rect.left_bottom() + Vec2::new(0.0, 2.0)).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                list(ui, salt, material, in_stock, list_w, 380.0);
            });
        });
    }
    changed
}

fn shot_id(salt: &str) -> egui::Id {
    egui::Id::unique(("material_shot", salt))
}

/// A screenshot asked for this picker's list open (kept open while the
/// screenshot is taken).
pub fn shot_wanted(ui: &egui::Ui, salt: &str) -> bool {
    let w = ui.data(|d| d.get_temp::<bool>(shot_id(salt))).is_some();
    w
}

/// Screenshots (`--material-open=<salt>[:<text>]`): keep this picker's list
/// open, with `text` typed in its find box.
pub fn open_for_shot(ctx: &egui::Context, salt: &str, text: &str) {
    ctx.data_mut(|d| {
        d.insert_temp(shot_id(salt), true);
        d.insert_temp(find_id(salt), text.to_string());
        d.insert_temp(egui::Id::unique(("material_list_seen", salt)), u64::MAX / 2);
    });
}

/// Screenshots: `text` typed in a find box that is always shown (the stock
/// shelf's).
pub fn type_for_shot(ctx: &egui::Context, salt: &str, text: &str) {
    ctx.data_mut(|d| d.insert_temp(find_id(salt), text.to_string()));
}
