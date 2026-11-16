//! The part Technology tab, laid out like the original's drawing module:
//! Lead-in/Lead-outs, Micro-Joints, Loops and Cut Technology, each with
//! Automatic and Delete. The rules it follows are in MECHANICS.md.

use crate::engine::TechEdit;
use crate::nest::PartTech;
use crate::theme::th;
use crate::ui_parts::*;
use crate::App;
use eframe::egui;
use egui::{Color32, CornerRadius, Rect, RichText, Sense, Stroke, Vec2};

impl App {
    /// Parts the Technology buttons act on: the selected one, or all.
    /// Run an automatic technology command and say what it changed (and,
    /// when it changed one part type of several, offer it for all of them).
    pub fn run_tech(&mut self, k: TechRerun) {
        match k {
            TechRerun::AutoLeads => self.auto_leads(),
            TechRerun::StandardLeads => self.apply_machine_leads(),
            TechRerun::AutoJoints => self.auto_joints_now(),
            TechRerun::AutoLoops => self.auto_loops(),
        }
        let targets = self.tech_targets();
        let pcs: u32 = targets.iter().filter_map(|&i| self.parts.get(i)).map(|p| p.qty).sum();
        let what = k.label();
        let (title, again) = if self.tech_all || targets.len() != 1 {
            (format!("{what} on all {} part types ({pcs} pcs)", targets.len()), None)
        } else {
            let name = self.parts.get(targets[0]).map_or(String::new(), |p| p.name.clone());
            (format!("{what} on 1 part type: {name} ({pcs} pcs)"), (self.parts.len() > 1).then_some(k))
        };
        self.result_card = Some(crate::ResultCard {
            title,
            lines: vec![],
            warn: false,
            actions: again.map(|k| vec![(format!("Apply to all {} part types", self.parts.len()), crate::CardAct::TechAll(k))]).unwrap_or_default(),
        });
    }

    pub(crate) fn tech_targets(&self) -> Vec<usize> {
        if self.tech_all {
            (0..self.parts.len()).collect()
        } else {
            self.sel_part.filter(|&i| i < self.parts.len()).into_iter().collect()
        }
    }

    /// Automatic lead-ins: placed by contour class (leads.rs), with the chosen
    /// type (by default the machine's tables) and position.
    pub fn auto_leads(&mut self) {
        let e = TechEdit::AutoLeads { lead_type: self.lead_choice, position: self.lead_pos };
        self.edit_tech("Automatic lead-ins", |t, b| e.apply(t, b));
    }

    /// The active machine's technology file (CT1), or the BLM file's values,
    /// for the job's material and thickness (its fitting section, MTL1–MTL7).
    pub fn machine_tech(&self) -> crate::ct1::MachineTech {
        crate::ct1::for_spec(&self.spec)
    }

    pub fn machine_tech_tip(&self) -> String {
        let mt = self.machine_tech();
        let q = mt.leads.quality(1);
        let classes: Vec<String> = crate::leads::CLASS_NAMES.iter().zip(q.recs.iter()).map(|(n, r)| format!("{n}: type {} ({} / {} mm)", r.kind, r.d_in, r.d_out)).collect();
        format!(
            "Standard leads: the machine's own lead-in and lead-out on every contour, whatever its shape — {} mm in, {} mm out, {} mm overlap past the start, position {} ({}). (Auto lead-ins instead picks by contour class.)\n{}.\nTables by contour class (quality 1): {}",
            mt.lead_in,
            mt.lead_out,
            mt.overlap,
            mt.zone,
            mt.source.as_ref().map_or("built-in BLM values".to_string(), |p| p.display().to_string()),
            mt.table_label(),
            classes.join(", ")
        )
    }

    /// Lead-in, lead-out, overlap and position from the machine's CT1
    /// (E044, E045, B156, E000).
    pub fn apply_machine_leads(&mut self) {
        let mt = self.machine_tech();
        self.lead_pos = mt.zone;
        let e = TechEdit::MachineLeads(mt);
        self.edit_tech("Machine lead-ins", |t, b| e.apply(t, b));
    }

    /// Automatic micro-joints: one per the machine's maximum distance (F150, by
    /// the part's quality) on every contour the joint type reaches, holes too,
    /// within the size class's fewest/most (Settings → count by size).
    pub fn auto_joints_now(&mut self) {
        let mt = self.machine_tech();
        let e = TechEdit::auto_joints(&mt, None);
        self.edit_tech("Automatic micro-joints", |t, b| e.apply(t, b));
    }

    /// Loops at sharp outer corners (toggles them).
    pub fn auto_loops(&mut self) {
        let on = self.tech_targets().first().and_then(|&i| self.parts[i].tech).is_some_and(|t| t.loops);
        let e = TechEdit::Loops(!on);
        self.edit_tech(if on { "Loops removed" } else { "Loops added at sharp corners" }, |t, b| e.apply(t, b));
    }

    pub(crate) fn edit_tech(&mut self, what: &str, f: impl Fn(&mut Option<PartTech>, (f64, f64))) {
        let targets = self.tech_targets();
        if targets.is_empty() {
            self.status = "Select a part first (or tick 'All parts').".into();
            return;
        }
        self.checkpoint();
        for &i in &targets {
            let bbox = self.parts[i].bbox();
            f(&mut self.parts[i].tech, bbox);
        }
        self.refresh();
        let who = if targets.len() == 1 { format!("'{}'", self.parts[targets[0]].name) } else { format!("{} parts", targets.len()) };
        self.status = format!("{what} for {who}.");
    }

    pub fn ribbon_part_tech(&mut self, ui: &mut egui::Ui) {
        // On the sheet the commands work on all part types until the user
        // picks "This part type" by hand; that choice is then kept. (Picking a
        // part on the sheet does not change it.) An opened part's commands work
        // on that part. With no part picked, "this part" means nothing.
        if self.workspace == crate::Workspace::Nesting {
            self.tech_all = self.tech_nest_all.unwrap_or(true);
        }
        if !self.tech_all && self.sel_part.is_none_or(|i| i >= self.parts.len()) && !self.parts.is_empty() {
            self.tech_all = true;
        }
        let has = !self.tech_targets().is_empty();
        let n_scope = self.tech_targets().len();
        let in_part = self.workspace == crate::Workspace::Part;
        let picked = in_part && !self.sel_elem_info.is_empty();
        // What the commands work on comes first, so it is always in view
        // (the ribbon folds groups from the right when the window is narrow).
        ribbon_group(ui, "Apply to", |ui| {
            ui.vertical(|ui| {
                // Two clear choices with what each changes: one part (all its
                // pieces) or every part in the job.
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.add_space(4.0);
                let sel = self.sel_part.and_then(|i| self.parts.get(i));
                let pieces: u32 = self.parts.iter().map(|p| p.qty).sum();
                let one = match sel {
                    Some(p) => format!("This part type: {} · all {} pcs", p.name, p.qty),
                    None => "This part type (pick one)".to_string(),
                };
                let all = format!("All {} part types · {} pcs", self.parts.len(), pieces);
                let width = 220.0;
                // Two choices; the chosen one is filled in the accent colour
                // with a tick, the other stays plain.
                let choice = |text: String, on: bool| {
                    if on {
                        egui::Button::new(RichText::new(format!("✔  {text}")).color(Color32::WHITE).strong()).fill(th().accent).min_size(Vec2::new(width, 26.0))
                    } else {
                        egui::Button::new(RichText::new(text).color(th().text)).min_size(Vec2::new(width, 26.0))
                    }
                };
                if ui.add_enabled(sel.is_some(), choice(one, !self.tech_all)).on_hover_text("The technology commands change the selected part type — every piece of it on every sheet (technology belongs to the part, not to one placed piece)").clicked() {
                    self.tech_all = false;
                    if self.workspace == crate::Workspace::Nesting {
                        self.tech_nest_all = Some(false);
                    }
                }
                if ui.add(choice(all, self.tech_all)).on_hover_text("The technology commands change every part type in the job").clicked() {
                    self.tech_all = true;
                    if self.workspace == crate::Workspace::Nesting {
                        self.tech_nest_all = Some(true);
                    }
                }
            });
        });
        ribbon_group(ui, "Lead-ins", |ui| {
            if scoped(ribbon_button_ex(ui, Icon::LeadIn, "Auto\nlead-ins", false, has, true), n_scope, self.tech_all)
                .on_hover_text("Leads by contour class, from the machine's tables by default: round holes and parts at the position's angle, rectangles at its corner or nearest side, other shapes at the nearest valid corner, else the middle of the nearest long-enough line")
                .clicked()
            {
                self.run_tech(TechRerun::AutoLeads);
            }
            if scoped(ribbon_button(ui, Icon::Remove, "Delete\nlead-ins", false, has), n_scope, self.tech_all).clicked() {
                self.edit_tech("Lead-ins removed", |t, b| TechEdit::DeleteLeads.apply(t, b));
            }
            if scoped(ribbon_button(ui, Icon::StandardLeads, "Standard\nleads", false, has), n_scope, self.tech_all)
                .on_hover_text(self.machine_tech_tip())
                .clicked()
            {
                self.run_tech(TechRerun::StandardLeads);
            }
            if in_part
                && ribbon_button(ui, Icon::OnElement, "On element", false, picked)
                    .on_hover_text("Click a line or arc on the part, then put the lead-in in its middle")
                    .clicked()
            {
                self.lead_on_element();
            }
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.add_space(6.0);
                ui.label(RichText::new("Type").size(11.0).color(th().text_dim));
                crate::leadmenu::picker(ui, &mut self.lead_choice, self.lead_menu_demo);
            });
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Position").size(11.0).color(th().text_dim));
                self.position_pad(ui);
            });
        });

        ribbon_group(ui, "Micro-joints", |ui| {
            if scoped(ribbon_button(ui, Icon::Joint, "Auto\njoints", false, has), n_scope, self.tech_all)
                .on_hover_text("One joint per the machine's maximum distance (100 mm by default) on every contour, holes too, rounded up to an even number, each centred on its line or arc; within the size class's fewest/most")
                .clicked()
            {
                self.run_tech(TechRerun::AutoJoints);
            }
            if scoped(ribbon_button(ui, Icon::Remove, "Delete\njoints", false, has), n_scope, self.tech_all).clicked() {
                self.edit_tech("Micro-joints removed", |t, b| TechEdit::DeleteJoints.apply(t, b));
            }
            if in_part
                && ribbon_button(ui, Icon::OnElement, "On element", false, picked)
                    .on_hover_text("A micro-joint in the middle of each selected line or arc")
                    .clicked()
            {
                self.joint_on_elements();
            }
            if ribbon_button(ui, Icon::Gear, "Joint\nsettings", self.show_joint_sizes, true).on_hover_text("Joint type (plain gap, with leads, at corners), gap width, lead distances and angle, and the automatic count by size").clicked() {
                if !self.show_joint_sizes {
                    self.auto_joints = crate::ct1::for_spec(&self.spec).joint_sizes;
                }
                self.show_joint_sizes = !self.show_joint_sizes;
            }
        });
        ribbon_group(ui, "Loops", |ui| {
            if scoped(ribbon_button(ui, Icon::Loop, "Auto\nloops", false, has), n_scope, self.tech_all)
                .on_hover_text("Small loops outside sharp outer corners (90° or less)")
                .clicked()
            {
                self.run_tech(TechRerun::AutoLoops);
            }
            if scoped(ribbon_button(ui, Icon::Remove, "Delete\nloops", false, has), n_scope, self.tech_all).clicked() {
                self.edit_tech("Loops removed", |t, b| TechEdit::Loops(false).apply(t, b));
            }
        });
        ribbon_group(ui, "Cut technology", |ui| {
            if scoped(ribbon_button(ui, Icon::CutTech, "Own\ntechnology", false, has), n_scope, self.tech_all)
                .on_hover_text("Give the part its own technology with the defaults")
                .clicked()
            {
                self.edit_tech("Own cut technology", |t, b| TechEdit::Own.apply(t, b));
            }
            if scoped(ribbon_button(ui, Icon::Remove, "Remove own\ntechnology", false, has), n_scope, self.tech_all)
                .on_hover_text("Back to the sheet's automatic technology")
                .clicked()
            {
                self.edit_tech("Technology removed (automatic again)", |t, b| TechEdit::Delete.apply(t, b));
            }
            if ribbon_button(ui, Icon::SizeBands, "Quality\nby size", self.show_cut_ranges, true)
                .on_hover_text("Cut quality by contour size: e.g. small holes with fine quality (the original's automatic cut technology list)")
                .clicked()
            {
                self.show_cut_ranges = !self.show_cut_ranges;
            }
            // The cut technology panel (CS6): on an opened part only.
            if in_part {
                let custom = self.shown_part().is_some_and(|i| self.parts[i].shape == crate::nest::Shape::Custom);
                let tip = if custom {
                    "Open the cut technology panel: choose Cut (or one of the machine's qualities), Mark, No cut, Bend line or Leave out, then click, box or sweep over contours"
                } else {
                    "A built-in shape's contours are always cut: open an imported drawing to paint its contours"
                };
                if ribbon_button(ui, Icon::Brush, "Paint\ncontours", self.cut_panel.open, custom).on_hover_text(tip).clicked() {
                    self.cut_panel.open = !self.cut_panel.open;
                    if !self.cut_panel.open {
                        self.cut_panel.row = None;
                    }
                }
            }
        });
        // Texts: in the original a text is an element with a type (marking,
        // special or plain note) and gets the marking technology; see MECHANICS.md.
        ribbon_group(ui, "Text", |ui| {
            ui.vertical(|ui| {
                // Three compact rows fill exactly a big button's height, so
                // this group's title lines up with the others.
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.spacing_mut().interact_size.y = 21.0;
                ui.spacing_mut().button_padding.y = 1.0;
                ui.add(egui::TextEdit::singleline(&mut self.text_draft).desired_width(118.0).hint_text("Text or #REF"))
                    .on_hover_text(format!("Special texts fill in: {}", crate::nest::TEXT_PARAMS.iter().map(|(k, v)| format!("{k} = {v}")).collect::<Vec<_>>().join(", ")));
                egui::ComboBox::from_id_salt("text_kind").width(118.0).selected_text(short_kind(self.text_kind)).show_ui(ui, |ui| {
                    for k in crate::nest::TextKind::ALL {
                        ui.selectable_value(&mut self.text_kind, k, k.label());
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Height").size(11.0).color(th().text_dim));
                    crate::units::drag_len_in(ui, &mut self.text_height, self.cfg.nesting.units, 2.0, 100.0);
                    ui.label(RichText::new("Arc").size(11.0).color(th().text_dim));
                    ui.add(egui::DragValue::new(&mut self.text_curve).range(-2000.0..=2000.0).speed(0.5).suffix(" r"))
                        .on_hover_text("Bend the text along an arc of this radius (mm). 0 = straight, positive = over the top of a circle, negative = round the bottom. Or select a line, arc or circle and press On element.");
                });
            });
            let in_part = self.workspace == crate::Workspace::Part;
            let on_elem = in_part && !self.sel_elems.is_empty();
            let tip = if on_elem {
                "Put the text next to the selected line (along it) or arc/circle (curved round it), inside the material"
            } else if in_part {
                "Click on the part to place the text (tip: select a line or arc first to place it relative to it)"
            } else {
                "Add the text along the bottom of the part"
            };
            let label = if on_elem { "On element" } else if in_part { "Place" } else { "Add" };
            if scoped(ribbon_button(ui, Icon::Text, label, self.placing_text, has && !self.text_draft.trim().is_empty()), n_scope, self.tech_all)
                .on_hover_text(tip)
                .clicked()
            {
                if on_elem {
                    self.text_on_element();
                } else if in_part {
                    self.placing_text = !self.placing_text;
                    self.status = if self.placing_text { "Click on the part where the text should start.".into() } else { String::new() };
                } else {
                    let (txt, h, kind) = (self.text_draft.clone(), self.text_height, self.text_kind);
                    self.add_text_to_targets(&txt, h, kind);
                }
            }
            if scoped(ribbon_button(ui, Icon::Remove, "Delete\ntexts", false, has), n_scope, self.tech_all).on_hover_text("Remove all texts from the part").clicked() {
                let targets = self.tech_targets();
                self.checkpoint();
                for i in targets {
                    self.parts[i].texts.clear();
                }
                self.refresh();
                self.status = "Texts removed.".into();
            }
        });
    }

    /// 3 × 3 keypad for the lead position (7 8 9 / 4 5 6 / 1 2 3).
    fn position_pad(&mut self, ui: &mut egui::Ui) {
        let cell = 15.0;
        let (r, _) = ui.allocate_exact_size(Vec2::splat(cell * 3.0 + 4.0), Sense::hover());
        let p = ui.painter();
        p.rect_stroke(r, CornerRadius::same(4), Stroke::new(1.0, th().border), egui::StrokeKind::Inside);
        for row in 0..3 {
            for col in 0..3 {
                let pos = (2 - row) as u8 * 3 + col as u8 + 1;
                let cr = Rect::from_min_size(r.min + Vec2::new(2.0 + col as f32 * cell, 2.0 + row as f32 * cell), Vec2::splat(cell));
                let resp = ui.interact(cr, ui.scope_id().with(("pos", pos)), Sense::click());
                let on = self.lead_pos == pos;
                let fill = if on { th().accent } else if resp.hovered() { th().hover } else { Color32::TRANSPARENT };
                ui.painter().rect_filled(cr.shrink(1.5), CornerRadius::same(3), fill);
                ui.painter().circle_filled(cr.center(), 2.0, if on { Color32::WHITE } else { th().text_dim });
                if resp.on_hover_text(format!("Position {pos}")).clicked() {
                    self.lead_pos = pos;
                }
            }
        }
    }

    /// The Micro-joints window: a *Joint* tab (type and sizes, set on a
    /// drawing of one joint) and a *How many* tab (spacing and the count by
    /// contour size, rule P7).
    pub fn joint_sizes_window(&mut self, ctx: &egui::Context) {
        self.cut_ranges_window(ctx);
        if !self.show_joint_sizes {
            return;
        }
        let mut open = true;
        let targets = self.tech_targets();
        let cur = targets.first().and_then(|&i| self.parts[i].tech).unwrap_or_default();
        let mut edit = cur;
        let mt = self.machine_tech();
        let sizes_before = self.auto_joints;
        let unit = self.cfg.nesting.units;
        let applies = if targets.is_empty() {
            "Select a part first".to_string()
        } else if self.tech_all {
            format!("For all {} parts", targets.len())
        } else {
            "For the selected part".to_string()
        };
        egui::Window::new("Micro-joints").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.set_width(JW);
            let rules_on = self.auto_joints != crate::nest::AutoJoints::default();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                if joint_tab(ui, "Joint", self.joint_tab == 0, false).on_hover_text("Joint type, gap and leads").clicked() {
                    self.joint_tab = 0;
                }
                let tip = if rules_on { "Spacing and the count by contour size (size rules are on)" } else { "Spacing and the count by contour size" };
                if joint_tab(ui, "How many", self.joint_tab == 1, rules_on).on_hover_text(tip).clicked() {
                    self.joint_tab = 1;
                }
            });
            ui.add_space(2.0);
            if self.joint_tab == 0 {
                ui.add_enabled_ui(!targets.is_empty(), |ui| joint_type_ui(ui, &mut edit, unit));
            } else {
                joint_count_ui(ui, &mut edit, &mut self.auto_joints, unit, !targets.is_empty());
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new(&applies).size(11.0).color(th().text_dim));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let src = mt.source.as_ref().map_or("built-in BLM values".to_string(), |p| p.display().to_string());
                    if self.joint_tab == 0 {
                        let tip = format!("From the machine's technology file: type {}, gap {} mm, lead-in {} mm at {}°, lead-out {} mm ({src})", mt.joint_type, mt.joint_gap, mt.joint_lead, mt.joint_angle, mt.joint_lead_out);
                        if ui.add_enabled(!targets.is_empty(), egui::Button::new("Machine values")).on_hover_text(tip).clicked() {
                            edit.joint_width = mt.joint_gap;
                            edit.joint_lead = mt.joint_lead;
                            edit.joint_lead_out = Some(mt.joint_lead_out);
                            edit.joint_angle = mt.joint_angle;
                            if let Some(jt) = crate::nest::JointType::from_code(mt.joint_type) {
                                edit.joint_type = jt;
                            }
                        }
                    } else {
                        let tip = format!("From the machine's technology file: one joint per {} mm and its size classes (F121, F122, F136–F149) ({src})", mt.joint_every);
                        if ui.button("Machine values").on_hover_text(tip).clicked() {
                            self.auto_joints = mt.joint_sizes;
                            if !targets.is_empty() {
                                edit.joint_every = mt.joint_every;
                            }
                        }
                        if ui.button("No size rules").on_hover_text("Every contour medium, any number of joints (the original's defaults)").clicked() {
                            self.auto_joints = crate::nest::AutoJoints::default();
                        }
                    }
                });
            });
        });
        // The size rules edited here act on the active machine's tables.
        if self.auto_joints != sizes_before {
            crate::ct1::set_joint_sizes(self.auto_joints);
        }
        if !open {
            self.show_joint_sizes = false;
        }
        if edit != cur && !targets.is_empty() {
            let (jt, w, l, lo, a, every) = (edit.joint_type, edit.joint_width, edit.joint_lead, edit.joint_lead_out, edit.joint_angle, edit.joint_every);
            // A gap or spacing set here is the programmer's: a gas change
            // leaves it (GS14).
            let (own_w, own_every) = (w != cur.joint_width, every != cur.joint_every);
            self.edit_tech(&format!("Micro-joints: {}", jt.label().to_lowercase()), |t, _| {
                let mut x = t.unwrap_or_default();
                x.joint_type = jt;
                x.joint_width = w;
                x.joint_lead = l;
                x.joint_lead_out = lo;
                x.joint_angle = a;
                x.joint_every = every;
                if own_w {
                    x.joint_width_machine = None;
                }
                if own_every {
                    x.joint_every_machine = None;
                }
                *t = Some(x);
            });
        }
    }
}

/// Width of the Micro-joints window's content.
const JW: f32 = 340.0;

/// A tab title with an underline when it is shown, and an amber dot when
/// something on it is switched on.
fn joint_tab(ui: &mut egui::Ui, text: &str, on: bool, dot: bool) -> egui::Response {
    let col = if on { th().text_strong } else { th().text_dim };
    let r = ui.add(egui::Button::new(RichText::new(text).size(13.0).color(col)).frame(false));
    if on {
        ui.painter().line_segment([r.rect.left_bottom() + Vec2::new(0.0, 3.0), r.rect.right_bottom() + Vec2::new(0.0, 3.0)], Stroke::new(2.0, th().accent));
    }
    if dot {
        ui.painter().circle_filled(r.rect.right_top() + Vec2::new(4.0, 4.0), 3.0, crate::jointpics::MEDIUM);
    }
    r
}

/// A section title with an (i) that explains it on hover.
fn joint_section(ui: &mut egui::Ui, title: &str, tip: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).strong());
        if !tip.is_empty() {
            let (r, resp) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
            let c = if resp.hovered() { th().text } else { th().text_dim };
            ui.painter().circle_stroke(r.center(), 6.0, Stroke::new(1.0, c));
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "i", egui::FontId::proportional(9.5), c);
            resp.on_hover_text(tip);
        }
    });
}

fn joint_hint(ui: &mut egui::Ui, s: &str) {
    ui.label(RichText::new(s).size(11.0).color(th().text_dim));
}

/// The Joint tab: the four types as pictures, then one joint drawn large
/// with its size boxes on the sizes they set.
fn joint_type_ui(ui: &mut egui::Ui, edit: &mut PartTech, unit: crate::units::LenUnit) {
    use crate::jointpics as jp;
    use crate::nest::JointType;
    joint_section(ui, "Type", "");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let tw = (JW - 18.0) / 4.0;
        for (jt, name) in JointType::ALL.into_iter().zip(["Plain gap", "Square leads", "Angled leads", "Corner"]) {
            let (r, resp) = ui.allocate_exact_size(Vec2::new(tw, tw / 2.0 + 24.0), Sense::click());
            let on = edit.joint_type == jt;
            let p = ui.painter();
            let fill = if on { th().accent_soft } else if resp.hovered() { th().hover } else { th().card };
            p.rect_filled(r, CornerRadius::same(7), fill);
            p.rect_stroke(r, CornerRadius::same(7), Stroke::new(if on { 1.5 } else { 1.0 }, if on { th().accent } else { th().border }), egui::StrokeKind::Inside);
            jp::type_tile(p, Rect::from_min_size(r.min + Vec2::splat(4.0), Vec2::new(tw - 8.0, (tw - 8.0) / 2.0)), jt);
            p.text(r.center_bottom() - Vec2::new(0.0, 5.0), egui::Align2::CENTER_BOTTOM, name, egui::FontId::proportional(11.0), if on { th().text_strong } else { th().text });
            if resp.on_hover_text(jt.label()).clicked() {
                edit.joint_type = jt;
            }
        }
    });
    ui.add_space(4.0);
    joint_section(
        ui,
        "Sizes",
        "Type a size in the box on the drawing. Each gap sits on one line or arc longer than the gap; a joint next to a corner goes on the corner.",
    );
    let jt = edit.joint_type;
    let r = Rect::from_min_size(ui.cursor().min, Vec2::new(JW, jp::joint_height(jt, JW)));
    let m = jp::joint(ui.painter(), r, jt);
    // A child ui over the drawing: boxes placed on it leave the rest of the
    // window where it was.
    ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
        let fw = if unit == crate::units::LenUnit::Inch { 74.0 } else { 66.0 };
        let at = |c: egui::Pos2, w: f32| Rect::from_center_size(c, Vec2::new(w, 18.0));
        ui.put(at(m.gap, fw), crate::units::len_drag(&mut edit.joint_width, unit, 0.05, 10.0)).on_hover_text("Gap: the bit left uncut that holds the part in the sheet");
        if let Some(c) = m.lead_out {
            let mut out = edit.joint_lead_out.unwrap_or(edit.joint_lead);
            let field = crate::units::len_drag_word(&mut out, unit, 0.0, 50.0, "None", |v| v <= 0.0, 0.0);
            if ui.put(at(c, fw), field).on_hover_text("Lead-out: how far the cut runs out into the scrap before the gap, square to the contour. None = the beam just stops at the gap").changed() {
                edit.joint_lead_out = Some(out);
            }
        }
        if let Some(c) = m.lead_in {
            ui.put(at(c, fw), crate::units::len_drag(&mut edit.joint_lead, unit, 0.0, 50.0)).on_hover_text("Lead-in: how far off the contour the cut re-pierces after the gap, square to the contour");
        }
        if let Some(c) = m.angle {
            ui.put(at(c, 48.0), egui::DragValue::new(&mut edit.joint_angle).range(1.0..=90.0).speed(0.5).suffix("°")).on_hover_text("The leads' angle to the contour: 90° is square to it, less leans them further away from the gap");
        }
    });
    ui.advance_cursor_after_rect(r);
    ui.add_space(2.0);
    jp::legend(ui, jt.has_leads());
    joint_hint(
        ui,
        match jt {
            JointType::Gap => "The beam goes off across the gap and back on after it. No leads.",
            JointType::Corner => "Joints go on the outside corners first. Drag the red handle on a joint in the Part view to bend its leads.",
            _ => "Cuts out into the scrap, stops, re-pierces and cuts back in. Drag the red handle on a joint in the Part view to bend its leads.",
        },
    );
}

/// The How many tab: the spacing, the size map with its limits and the
/// fewest / most joints for each size class (rule P7).
fn joint_count_ui(ui: &mut egui::Ui, edit: &mut PartTech, aj: &mut crate::nest::AutoJoints, unit: crate::units::LenUnit, has_part: bool) {
    use crate::jointpics as jp;
    joint_section(ui, "Spacing", "Automatic joints: one per this length of each contour, holes too (the machine's maximum distance, CT1 F150). Off = the fixed number of joints on the outside.");
    let (r, _) = ui.allocate_exact_size(Vec2::new(JW, 80.0 * JW / 336.0), Sense::hover());
    jp::spacing(ui.painter(), r);
    ui.add_enabled_ui(has_part, |ui| {
        ui.horizontal(|ui| {
            ui.label("One joint every");
            ui.add_sized([84.0, 18.0], crate::units::len_drag_word(&mut edit.joint_every, unit, 0.0, 5000.0, "Off", |v| v <= 0.0, 0.0));
        });
    });
    ui.separator();
    joint_section(
        ui,
        "Size classes",
        "Each contour is sorted by its box: small if it fits inside the small size, large if it reaches the large size both ways, otherwise medium. Holes and parts each have their own counts.",
    );
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::new(104.0, 92.0), Sense::hover());
        jp::size_map(ui.painter(), r, aj);
        ui.vertical(|ui| {
            let none = |v: f64| v <= 0.0;
            let pair = |ui: &mut egui::Ui, name: &str, c: Color32, v: &mut [f64; 2], large: bool| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    ui.add_sized([70.0, 18.0], egui::Label::new(RichText::new(name).color(c)));
                    for (k, axis) in ["Width", "Height"].into_iter().enumerate() {
                        if k == 1 {
                            ui.label("×");
                        }
                        let f = if large {
                            crate::units::len_drag_word(&mut v[k], unit, 0.0, jp::NO_LIMIT_MM, "No limit", jp::is_no_limit, jp::NO_LIMIT_MM)
                        } else {
                            crate::units::len_drag_word(&mut v[k], unit, 0.0, jp::NO_LIMIT_MM, "None", none, 0.0)
                        };
                        ui.add_sized([70.0, 18.0], f).on_hover_text(axis);
                    }
                });
            };
            pair(ui, "Small up to", jp::SMALL, &mut aj.small, false);
            pair(ui, "Large from", jp::LARGE, &mut aj.large, true);
            ui.label(RichText::new("Medium = everything else").size(11.0).color(jp::MEDIUM));
        });
    });
    ui.add_space(4.0);
    joint_section(ui, "Joints per contour", "Click a box to set the fewest and most joints for that size. Grey = it follows Every contour.");
    let general = *aj;
    egui::Grid::new("aj_counts").num_columns(4).spacing([6.0, 5.0]).show(ui, |ui| {
        ui.label("");
        for (name, c) in [("Small", jp::SMALL), ("Medium", jp::MEDIUM), ("Large", jp::LARGE)] {
            ui.add_sized([82.0, 16.0], egui::Label::new(RichText::new(name).strong().size(11.5).color(c)));
        }
        ui.end_row();
        for (row, hole) in [("Parts", false), ("Holes", true)] {
            ui.label(row);
            for k in 0..3 {
                let pair = if hole { &mut aj.holes[k] } else { &mut aj.parts[k] };
                count_cell(ui, pair, &general, if hole { "Holes" } else { "Parts" }, ["small", "medium", "large"][k]);
            }
            ui.end_row();
        }
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Every contour").color(th().text_dim)).on_hover_text("The limits a size class follows when it has none of its own");
        ui.label("fewest");
        ui.add(egui::DragValue::new(&mut aj.min).range(0..=999));
        ui.label("most");
        ui.add(egui::DragValue::new(&mut aj.max).range(0..=99999).custom_formatter(|v, _| if v >= jp::NO_LIMIT_COUNT as f64 { "No limit".into() } else { format!("{v:.0}") }).custom_parser(no_limit_count));
    });
}

/// "no limit" typed in a count box.
fn no_limit_count(s: &str) -> Option<f64> {
    let t = s.trim().to_ascii_lowercase();
    if t.is_empty() || (t.len() >= 2 && "no limit".starts_with(&t)) { Some(99999.0) } else { t.parse().ok() }
}

/// One size class's count, as words ("any", "0–2"); a click opens its
/// fewest / most.
fn count_cell(ui: &mut egui::Ui, pair: &mut [i32; 2], general: &crate::nest::AutoJoints, what: &str, size: &str) {
    use crate::jointpics as jp;
    let (text, follows) = jp::range_text(*pair, general);
    let resp = ui.add_sized([82.0, 20.0], egui::Button::new(RichText::new(text).color(if follows { th().text_dim } else { th().text_strong })));
    let resp = resp.on_hover_text(format!("{what}, {size}: click to set the fewest and most joints"));
    egui::Popup::menu(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_min_width(190.0);
        ui.label(RichText::new(format!("{what}, {size}")).strong());
        let same = |v: f64, _: std::ops::RangeInclusive<usize>| if v < 0.0 { "as Every contour".into() } else if v >= jp::NO_LIMIT_COUNT as f64 { "No limit".into() } else { format!("{v:.0}") };
        egui::Grid::new("cell").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("Fewest");
            ui.add(egui::DragValue::new(&mut pair[0]).range(-1..=999).custom_formatter(same));
            ui.end_row();
            ui.label("Most");
            ui.add(egui::DragValue::new(&mut pair[1]).range(-1..=99999).custom_formatter(same).custom_parser(no_limit_count));
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if ui.small_button("None").on_hover_text("No joints on these contours").clicked() {
                *pair = [0, 0];
            }
            if ui.small_button("Any").on_hover_text("No limit either way").clicked() {
                *pair = [0, jp::NO_LIMIT_COUNT];
            }
            if ui.small_button("As Every contour").clicked() {
                *pair = [-1, -1];
            }
        });
    });
}

impl App {
    /// Cut quality by contour size (CT1): each contour's equivalent radius √(area / π) picks the first
    /// matching row; contours outside every row take the part's quality or the machine's size band (CQ1).
    pub fn cut_ranges_window(&mut self, ctx: &egui::Context) {
        if !self.show_cut_ranges {
            return;
        }
        let mut open = true;
        let targets = self.tech_targets();
        let cur = targets.first().and_then(|&i| self.parts[i].tech).unwrap_or_default();
        let mut edit = cur;
        // The first target's contours, to show the quality each one takes.
        let mt = self.machine_tech();
        // With the qualities set on its contours by hand (CQ6).
        let geo = targets.first().map(|&i| (self.parts[i].geometry(), self.parts[i].cut_qualities()));
        egui::Window::new("Cut technology by size").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label(RichText::new("Each contour's size is the radius of a circle with the same area. The first ticked row it falls in sets its quality (from ≤ r < up to).").size(12.0));
            ui.add_space(6.0);
            ui.add_enabled_ui(!targets.is_empty(), |ui| {
                egui::Grid::new("cut_ranges").num_columns(4).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("");
                    ui.label(RichText::new("From r").strong());
                    ui.label(RichText::new("Up to r").strong());
                    ui.label(RichText::new("Quality").strong());
                    ui.end_row();
                    for (k, row) in edit.cut_ranges.iter_mut().enumerate() {
                        ui.checkbox(&mut row.on, format!("Row {}", k + 1));
                        crate::units::drag_len_in(ui, &mut row.min_r, self.cfg.nesting.units, 0.0, 99999.0);
                        crate::units::drag_len_in(ui, &mut row.max_r, self.cfg.nesting.units, 0.0, 99999.0);
                        egui::ComboBox::from_id_salt(("cut_q", k)).width(120.0).selected_text(quality_text(ui, &mt.qualities, row.quality)).show_ui(ui, |ui| {
                            for q in offered_qualities(&mt.qualities) {
                                ui.selectable_value(&mut row.quality, q, quality_text(ui, &mt.qualities, q));
                            }
                        });
                        ui.end_row();
                    }
                });
            });
            if let Some((g, hand)) = &geo {
                ui.separator();
                for (k, (hole, pts)) in g.holes.iter().map(|h| (true, h)).chain(std::iter::once((false, &g.outline))).enumerate() {
                    let (q, from) = crate::cnc::contour_quality(&edit, &mt, pts, hand.get(k).copied().flatten());
                    let by_row = from == crate::cnc::QualityFrom::SizeRange;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(RichText::new(format!("{} r {:.1} mm →", if hole { "Hole" } else { "Outside" }, crate::cnc::contour_radius(pts))).size(11.5).color(th().text_dim));
                        ui.label(RichText::new("●").size(11.5).color(quality_colour(&mt.qualities, q)));
                        ui.label(RichText::new(format!("{} ({})", mt.qualities.label(q), from.label())).size(11.5).color(if by_row { th().text_strong } else { th().text_dim }));
                    });
                }
            }
            ui.add_space(4.0);
            let slowest = mt.qualities.label(crate::nest::CutQuality::new(1).unwrap_or(crate::nest::CutQuality::FALLBACK));
            if ui.button(format!("Example: small holes at {slowest}")).on_hover_text(format!("Row 1: holes up to r {} mm cut at {slowest}", crate::nest::SMALL_HOLE_RADIUS_EXAMPLE)).clicked() {
                edit.cut_ranges = Default::default();
                edit.cut_ranges[0].on = true;
            }
        });
        if !open {
            self.show_cut_ranges = false;
        }
        if edit.cut_ranges != cur.cut_ranges && !targets.is_empty() {
            let ranges = edit.cut_ranges;
            self.edit_tech("Cut technology by size", |t, _| {
                let mut x = t.unwrap_or_default();
                x.cut_ranges = ranges;
                *t = Some(x);
            });
        }
    }
}

/// A part's own cut quality on screen: the machine's name for it (CQ4), or
/// "Auto (size bands)" when the machine's size bands decide (CQ1; agents' `auto`).
pub fn part_quality_label(q: Option<crate::nest::CutQuality>, names: &crate::qualities::CutQualities) -> String {
    q.map_or_else(|| "Auto (size bands)".to_string(), |q| names.label(q))
}

/// A cut quality's colour on screen (CQ5).
pub fn quality_colour(names: &crate::qualities::CutQualities, q: crate::nest::CutQuality) -> egui::Color32 {
    let [r, g, b] = names.colour(q);
    egui::Color32::from_rgb(r, g, b)
}

/// A cut quality as the pickers show it: a dot in its colour (CQ5), then the
/// machine's name for it (CQ4).
pub fn quality_text(ui: &egui::Ui, names: &crate::qualities::CutQualities, q: crate::nest::CutQuality) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let (font, valign) = (egui::FontSelection::Default, egui::Align::Center);
    RichText::new("● ").color(quality_colour(names, q)).append_to(&mut job, ui.style(), font.clone(), valign);
    RichText::new(names.label(q)).append_to(&mut job, ui.style(), font, valign);
    job
}

/// The qualities a picker lists: every one the machine doesn't hide (CQ4). A
/// hidden quality already chosen stays chosen and shows as the picker's text.
pub fn offered_qualities(names: &crate::qualities::CutQualities) -> Vec<crate::nest::CutQuality> {
    crate::nest::CutQuality::all().filter(|&q| names.offered(q)).collect()
}

#[cfg(test)]
mod quality_picker_tests {
    use crate::nest::CutQuality;

    #[test]
    fn a_hidden_quality_stays_out_of_the_picker_even_when_chosen() {
        let codes = [("A923".to_string(), "\"<HIDE>\"".to_string())].into_iter().collect();
        let names = crate::qualities::CutQualities::from_codes(&codes);
        let listed = super::offered_qualities(&names);
        assert_eq!(listed.len(), 11);
        assert!(!listed.iter().any(|q| q.number() == 2));
        assert!(listed.contains(&CutQuality::all().next().unwrap()));
    }
}

fn short_kind(k: crate::nest::TextKind) -> &'static str {
    match k {
        crate::nest::TextKind::Mark => "Marking text",
        crate::nest::TextKind::Special => "Special text",
        crate::nest::TextKind::Note => "Note only",
    }
}

impl App {
    /// Add a text along the bottom of each target part, centred (shrunk to fit).
    pub fn add_text_to_targets(&mut self, txt: &str, height: f64, kind: crate::nest::TextKind) {
        let targets = self.tech_targets();
        if targets.is_empty() {
            self.status = "Select a part first (or tick 'All parts').".into();
            return;
        }
        self.checkpoint();
        for &i in &targets {
            crate::engine::add_text(&mut self.parts[i], txt, height, kind);
        }
        self.refresh();
        self.status = format!("Text '{txt}' added to {} part(s) as {}.", targets.len(), kind.label().to_lowercase());
    }
}

/// A small count in the top corner of a technology button: how many part
/// types the command will change ("all 10", or "1" for the selected part), so
/// the scope is visible on the button itself, not only in "Apply to".
fn scoped(r: egui::Response, n: usize, all: bool) -> egui::Response {
    if n == 0 {
        return r;
    }
    let text = if all { format!("all {n}") } else if n == 1 { "1 type".to_string() } else { format!("{n} types") };
    let painter = r.ctx.layer_painter(r.layer_id);
    // Neutral grey, so it never reads as the blue "on" mark of a toggle.
    let galley = painter.layout_no_wrap(text, egui::FontId::proportional(10.5), th().text_dim);
    let size = galley.size() + Vec2::new(8.0, 2.0);
    let pill = egui::Rect::from_min_size(egui::Pos2::new(r.rect.right() - size.x - 1.0, r.rect.top() + 1.0), size);
    painter.rect_filled(pill, CornerRadius::same(7), th().panel);
    painter.rect_stroke(pill, CornerRadius::same(7), Stroke::new(1.0, th().border), egui::StrokeKind::Inside);
    painter.galley(pill.min + Vec2::new(4.0, 1.0), galley, th().text_dim);
    let what = if all { format!("Changes all {n} part types in the job.") } else { "Changes the selected part type only (every piece of it).".to_string() };
    r.on_hover_text(what)
}

/// The automatic technology commands that can be run again on all part types.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TechRerun {
    AutoLeads,
    StandardLeads,
    AutoJoints,
    AutoLoops,
}

impl TechRerun {
    pub fn label(self) -> &'static str {
        match self {
            TechRerun::AutoLeads => "Auto lead-ins",
            TechRerun::StandardLeads => "Standard leads",
            TechRerun::AutoJoints => "Auto joints",
            TechRerun::AutoLoops => "Auto loops",
        }
    }
}
