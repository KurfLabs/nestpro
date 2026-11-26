#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod appicon;
mod backup;
mod arrange_ui;
mod bands;
mod blm;
mod startup;
mod iconfile;
mod cli;
mod cnc;
mod engine;
mod filewatch;
mod gas;
mod feedbackui;
mod grainui;
mod smoothui;
mod mcp;
mod ct1;
mod cutpanel;
mod cyclesui;
mod orderpick;
mod leadmenu;
mod jointpics;
mod joints;
mod kerf;
mod leads;
mod xmlimport;
mod jobtree;
mod filename;
mod dock;
mod cutoff;
mod dropsui;
mod cuttime;
mod techtable;
mod dbbrowse;
mod dbconn;
mod dbquery;
mod dbui;
mod drawing;
mod erpui;
mod integrations;
mod font;
mod ghost;
mod manual;
mod radial;
mod settings;
mod settings_ui;
mod sheetlistui;
mod sheetsim;
mod textedit;
mod dxf;
mod formats;
mod importui;
mod inuseui;
mod jobfile;
mod materials;
mod matpick;
mod jobs;
mod libui;
mod library;
mod folders;
mod libtree;
mod refreshui;
mod rolloutui;
mod shared;
mod shop;
mod region;
mod remline;
mod remmark;
mod remnant;
mod stock;
mod stockui;
mod stockpics;
mod machine;
mod nest;
mod omax;
mod post;
mod progimport;
mod qualities;
mod partview;
mod techui;
mod testdir;
mod theme;
mod turnbar;
mod ui_parts;
mod units;
mod updates;
mod v44;

use eframe::egui;
use egui::{Align2, Color32, CornerRadius, FontId, Key, Pos2, Rect, RichText, Sense, Shape as EShape, Stroke, Vec2};
use nest::{NestResult, Part, Placed, Shape, SheetSpec, P};
use std::path::PathBuf;
use theme::th;
use ui_parts::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Home,
    Sheet,
    Technology,
    View,
    /// Drawing module (a part opened on its own) only:
    Machining,
    Simulation,
    Notes,
    Utilities,
}

#[derive(Clone, Copy, PartialEq)]
pub enum BottomTab {
    PartsList,
    PartsView,
}

/// The two main screens: the jobs manager (what the original program is)
/// and the nesting screen it hands sheets to.
#[derive(Clone, Copy, PartialEq)]
pub enum Workspace {
    Jobs,
    Nesting,
    /// One part opened on its own to apply technology (the original opens it in
    /// its Drawing module, rule O6).
    Part,
}

#[derive(Clone, Copy, PartialEq)]
pub enum SheetTab {
    View,
    List,
}

/// Which parts open together with the chosen one (original PIEZADIB.SYS modes,
/// rule O3): the selection only, every part of the job, parts on
/// the same machine, or parts with the same machine, material and thickness.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OpenScope {
    Selected,
    All,
    SameMachine,
    SameTechnology,
}

impl OpenScope {
    pub const ALL: [OpenScope; 4] = [OpenScope::Selected, OpenScope::All, OpenScope::SameMachine, OpenScope::SameTechnology];
    pub fn label(self) -> &'static str {
        match self {
            OpenScope::Selected => "This part only",
            OpenScope::All => "All parts of the job",
            OpenScope::SameMachine => "Same machine",
            OpenScope::SameTechnology => "Same machine, material & thickness",
        }
    }
}

/// A question waiting for the person's answer.
#[derive(Clone)]
pub enum Pending {
    /// Geometry edit on a part that has its own technology.
    TechPartEdit { idx: usize, draft: Part },
    /// Material or thickness change while parts are nested.
    NestedSpecChange { new_spec: SheetSpec },
    /// The open job's file was changed outside the app while it also changed here.
    JobFileChanged { path: PathBuf },
    /// Export CNC: what will be written, where and for which machine, shown
    /// before writing; with the sheets that have overlapping parts or parts off
    /// the sheet: (sheet index, overlapping pairs, parts off the sheet).
    /// `checks`: Verify machining's findings per sheet (sheet, problems).
    /// `also`: the other machines in Settings it can write the same sheets
    /// for (EXP4); `earlier`: this job's programs other machines' folders
    /// still hold, removed on export (EXP6).
    ExportReview { problems: Vec<(usize, usize, usize)>, checks: Vec<(usize, usize)>, also: Vec<AlsoRow>, earlier: Vec<engine::overflow::Earlier> },
}

/// EXP4: one other machine in the Export review: where its programs go,
/// whether it can take the nest as it is (else why not), and ticked.
#[derive(Clone)]
pub struct AlsoRow {
    pub machine: usize,
    pub dir: PathBuf,
    pub ready: Result<(), String>,
    pub ticked: bool,
}

/// Answers to "this part has technology" (the original's choices, rule PT2).
#[derive(Clone, Copy, PartialEq)]
pub enum TechChoice {
    /// Keep the original with its technology and put the change on a copy.
    EditCopy,
    /// Change the part itself; its technology is removed.
    EditPart,
}

/// One work order: its products and the sheets they were nested onto.
#[derive(Clone)]
pub struct Job {
    pub week: String,
    pub name: String,
    pub parts: Vec<Part>,
    pub spec: SheetSpec,
    pub result: NestResult,
    pub stale: bool,
    /// The job's record in the library when it has been saved there (or opened from it).
    pub lib_id: Option<u64>,
    /// The revision of that record, and of each library part in the job, as
    /// this window read or last saved them (LB7).
    pub lib_read: libui::LibRead,
    /// Free cut lines / remnant lines and contours cut by hand, per sheet.
    pub cut_lines: Vec<Vec<cutoff::CutLine>>,
    pub manual_cuts: Vec<Vec<cnc::ManualCut>>,
    /// When the job's nestings last changed (the original's CAM last date of the
    /// job, rule ND1), seconds since 1970.
    pub cam_changed: Option<u64>,
    /// The job's file on disk (opened from or saved to), if any.
    pub file: Option<filewatch::JobLink>,
    /// Sheets this job has taken from stock, per stock reference, as of its
    /// last save (stock.rs; reconciled on every save).
    pub held: std::collections::BTreeMap<String, i32>,
    /// The ERP/MES work this job was brought in from and sent back to.
    pub erp: Option<integrations::ErpJob>,
    /// The job as last saved or opened (`job_sig`); None = never saved. It
    /// has unsaved changes when it no longer matches (jobtree.rs).
    pub saved: Option<u64>,
    /// The Library folder a job saved into the Library for the first time
    /// goes into (JX15): the folder of the library job it was opened or
    /// copied from. None: by its folder's name.
    pub lib_folder: Option<u64>,
    /// The material names the job read, by id (MA17): the one it was
    /// opened with, and that of a stock sheet whose material it took. Saved
    /// back with the job's material while the list doesn't have it, so the
    /// job keeps its id and name (invariant 3).
    pub material_names: engine::material::ReadNames,
    /// Why the job is read-only here (UP9): a newer version saved it. Shown
    /// above the job; Save and Save to library are refused, saying so.
    pub read_only: Option<String>,
}

pub struct App {
    pub job_name: String,
    pub parts: Vec<Part>,
    pub sel_part: Option<usize>,
    pub spec: SheetSpec,
    pub result: NestResult,
    pub plans: Vec<cnc::SheetPlan>,
    pub program: String,
    pub stale: bool,
    pub cur_sheet: usize,
    pub sel_placed: Option<usize>,
    /// More selected parts on the sheet besides `sel_placed` (arrange_ui.rs).
    pub sel_more: Vec<usize>,
    pub drag: Option<arrange_ui::DragState>,
    /// A turn of the selection by its handle (turnbar.rs, HM15).
    pub turn_drag: Option<turnbar::TurnDrag>,
    /// The turn bar's angle box while typed in, its own step, and Ctrl+T
    /// asking for the box (HM17).
    pub turn_text: String,
    pub turn_custom: f64,
    pub turn_focus: bool,
    /// A selection window being drawn on the sheet: where the drag started and
    /// where it is now (mm; the direction says what it takes, SW1), and
    /// whether it adds to the selection.
    pub window_sel: Option<(P, P, bool)>,
    /// The movement value window (Ctrl+R) is open.
    pub step_popup: bool,
    panning: bool,
    pub zoom: f32,
    pub pan: Vec2,
    pub tab: Tab,
    pub bottom: BottomTab,
    pub collision: bool,
    /// Pairs of parts on the shown sheet that touch or overlap (QR6), worked
    /// out on every change.
    pub overlaps: Vec<(usize, usize)>,
    /// The last export (folder, file names, warning, and what the programs
    /// are for, GS16: "3 programs written: Mild steel 12.7 mm, Oxygen"),
    /// shown on the sheet until closed so the files can be found.
    pub last_export: Option<(PathBuf, Vec<String>, Option<String>, String)>,
    /// EXP4, EXP6: what the last export did for the other machines, a line
    /// each (true = went as asked), shown on its card.
    pub export_also_lines: Vec<(String, bool)>,
    /// A placed part to bring to the middle of the view (Show next problem).
    pub focus_placed: Option<usize>,
    /// Which problem part "Show next" shows next.
    pub problem_cursor: usize,
    /// Copies on the pointer from the quick ring's Copy (QR4).
    pub carry: Option<arrange_ui::Carry>,
    pub show_path: bool,
    pub show_numbers: bool,
    pub show_grid: bool,
    pub show_program: bool,
    pub dock: dock::Dock,
    pub show_times: bool,
    pub status: String,
    pub cursor_mm: Option<P>,
    hover_name: Option<String>,
    shot: Option<PathBuf>,
    frames: u32,
    pub drag_test_on: bool,
    pub drag_test: dock::DragTest,
    color_seq: usize,
    pub undo: Vec<Snapshot>,
    pub redo: Vec<Snapshot>,
    last_edit: f64,
    pub search: String,
    toasts: Vec<(String, f64)>,
    last_status: String,
    anim_pending: bool,
    anim_start: f64,
    pub workspace: Workspace,
    pub sheet_tab: SheetTab,
    pub jobs: Vec<Job>,
    pub cur_job: usize,
    pub job_search: String,
    /// Original option: append the labor ticket to the product status line.
    pub show_labor_ticket: bool,
    /// Which list the Jobs status line describes (last one clicked).
    pub jobs_pick_sheet: bool,
    pub pending: Option<Pending>,
    /// Closing the window with unsaved jobs: their names while asking, and
    /// whether the person chose to close anyway (jobtree.rs `quit_guard`).
    pub quit_ask: Option<Vec<String>>,
    pub quit_confirmed: bool,
    /// S2: closing waits for this backup or restore of the Library to end.
    pub quit_wait: Option<updates::Work>,
    /// The Library folder being typed in its box (settings), until the
    /// typing is done.
    pub lib_dir_edit: Option<String>,
    /// A folder picked in Settings that holds no Library, while asking
    /// whether to make a new Library there (LB12).
    pub lib_new_ask: Option<String>,
    /// SH9, SH16: the first-run screen, or Settings → Shop's Join a shop,
    /// while it is open.
    pub firstrun: Option<startup::FirstRun>,
    /// Original option: warn before changing a part that has technology.
    pub warn_tech: bool,
    /// "Use this answer for the rest" given in the technology question.
    pub tech_answer_all: Option<TechChoice>,
    tech_all_box: bool,
    /// Technology tab: lead type and keypad position for Automatic, and "All parts".
    pub lead_choice: nest::LeadType,
    /// Screenshots only (--lead-menu=<type>): the Type picker shown open on that type.
    pub lead_menu_demo: Option<nest::LeadType>,
    pub lead_pos: u8,
    /// Which cuts Auto lead-ins and Delete lead-ins act on (LE12).
    pub lead_scope: nest::LeadScope,
    pub tech_all: bool,
    /// The scope picked by hand on the sheet (Nesting): None until then,
    /// when the technology commands work on all part types.
    pub tech_nest_all: Option<bool>,
    /// Screenshots only (--open-job-menu): show the Open job list as if opened.
    pub demo_open_menu: bool,
    /// Parts View scrolls this part's tile into view once (e.g. after import).
    pub scroll_to_part: Option<usize>,
    /// What the last import or automatic technology command did, shown as a
    /// card on the drawing with the next steps, until closed.
    pub result_card: Option<ResultCard>,
    pub auto_joints: nest::AutoJoints,
    pub show_joint_sizes: bool,
    /// Micro-joints window: the tab shown, 0 = Joint, 1 = How many (kept while the app runs).
    pub joint_tab: u8,
    /// Cut Technology → By size window.
    pub show_cut_ranges: bool,
    /// Nest parts grouped by their Group attribute (CT1 A785 sets the start value).
    pub group_nest: bool,
    /// Job Explorer: empty folders, a name being typed, a job waiting for delete confirmation.
    pub job_folders: Vec<String>,
    pub tree_edit: Option<jobtree::TreeEdit>,
    pub confirm_delete_job: Option<usize>,
    /// An empty sheet added with Add nesting, kept while it is the current sheet.
    pub empty_sheet_kept: Option<usize>,
    /// The open Sheet cycles window (SR6).
    pub cycles_dlg: Option<cyclesui::CyclesDlg>,
    /// The open job's last nesting change (see Job::cam_changed).
    pub cam_changed: Option<u64>,
    /// What the open job holds from stock (see Job::held).
    pub held: std::collections::BTreeMap<String, i32>,
    /// The semi-automatic remnant tool's line type.
    pub semi_mode: remline::SemiMode,
    /// A save waiting for the new remnants' references, and the names given.
    pub naming: Option<stockui::RemnantNaming>,
    pub remnant_names: Option<std::collections::BTreeMap<(usize, usize), String>>,
    /// Parts opened together in the Part view (the original's PIEZADIB.SYS list) and which one is shown.
    pub open_set: Vec<usize>,
    pub open_idx: usize,
    pub open_scope: OpenScope,
    pub part_zoom: f32,
    /// Part view: how far the drawing has been moved (screen px).
    pub part_pan: Vec2,
    /// Earlier Part views (zoom, pan) for "Previous", and when the wheel last moved.
    pub part_view_hist: Vec<(f32, Vec2)>,
    pub part_view_last: f64,
    /// Part view: the extent the view is framed on while a lead end is dragged
    /// (part index, extent), so the part doesn't move under the pointer.
    pub part_ext_lock: Option<(usize, [f64; 4])>,
    /// Part view: All the same contours for leads moved by hand (LE8): a
    /// dragged lead goes on every hole of the same shape.
    pub lead_same: bool,
    /// Part view: a lead just moved on one hole alone, offered to the holes
    /// of its shape (part, contour, how many) (LE8).
    pub lead_offer: Option<(usize, usize, usize)>,
    /// Part view: the lead clicked on (part, contour), whose card changes
    /// its shape (LE9).
    pub sel_lead: Option<(usize, usize)>,
    /// Screenshots only (--lead-card-menu): the selected lead's shape menu open.
    pub lead_card_demo: bool,
    /// Where that card sits (part, contour; top left), worked out from its
    /// size the frame before.
    pub lead_card_at: Option<((usize, usize), egui::Pos2)>,
    /// Follow Windows' light/dark setting (the original's default style takes
    /// the system colours).
    pub follow_system: bool,
    /// The Options menu is open.
    pub options_open: bool,
    /// Where the open job was last saved or opened from.
    pub job_path: Option<PathBuf>,
    /// What the window last read from or wrote to that file (to notice when
    /// an agent or script changes it), and when it last looked.
    pub job_link: Option<filewatch::JobLink>,
    pub disk_poll: f64,
    /// The Library window, and the open job's record number in the library.
    pub libui: libui::LibUi,
    pub importui: importui::ImportUi,
    pub stockui: stockui::StockUi,
    pub job_lib_id: Option<u64>,
    /// What the open job read from the Library (LB7).
    pub lib_read: libui::LibRead,
    /// Jobs in use: this window's holds, other PCs' jobs in use, and what
    /// opening one asks (CU5–CU11).
    pub inuse: inuseui::InUseUi,
    /// Who is working at this window, and the clock it goes by: what it
    /// saves to the Library records them (LB5). This PC's user and clock;
    /// tests set their own to play two PCs.
    pub who: shared::Who,
    pub clock: shared::Clock,
    /// Lists and open jobs that update by themselves (JX8, LB14–LB16).
    pub autorefresh: refreshui::AutoRefresh,
    /// The Feedback window and the note after a crash (FB1–FB8).
    pub feedback: feedbackui::FeedbackUi,
    /// The Add sheet size window for the job's own sheet list (JS1).
    pub sheet_list_ui: sheetlistui::SheetListUi,
    /// The Drawing module's view state (axes, colours, simulation, measuring…).
    pub dv: drawing::DrawView,
    /// The cut technology panel on the part's drawing screen (CS6).
    pub cut_panel: cutpanel::CutPanel,
    /// Picking a part's cut order on its drawing screen (CO11).
    pub order_pick: orderpick::OrderPick,
    /// Simulation of the current sheet's program (Nesting → Simulation tab).
    pub ssim: Option<sheetsim::SheetSim>,
    /// Saved settings (connections, machines, materials, folders), what was
    /// last saved, the Settings window, and the machine last applied.
    pub cfg: settings::AppSettings,
    pub cfg_saved: settings::AppSettings,
    pub setui: settings_ui::SettingsUi,
    /// Updates in a shop: the shop version, a trial and roll out (UP15–UP22).
    pub rollout: rolloutui::RolloutUi,
    /// Where a chosen update waits for the program to close: the program's
    /// own (a test gives one of its own).
    pub handoff: &'static updates::Handoff,
    pub applied_machine: Option<settings::Machine>,
    /// The machine's post-processor settings (block numbers, units, extension…).
    pub post: post::PostConfig,
    /// Read-only browser for Expert's database.
    pub dbb: dbbrowse::DbBrowser,
    /// BLM LC5 output (Siemens 840D SPF set + XML + zip, like Lantek's BLM post).
    pub blm: blm::BlmSettings,
    /// The nesting Verify Machining panel.
    pub nverify: v44::NestVerify,
    /// The Drop layouts window (standard drops, SD8).
    pub dropsui: dropsui::DropsUi,
    /// Part view: selected elements (by index), the part they belong to, the
    /// start of a box selection, and details of the selection for the ribbon.
    pub sel_elems: Vec<usize>,
    pub elem_part: Option<usize>,
    pub box_start: Option<egui::Pos2>,
    pub sel_elem_info: Vec<(usize, nest::ElemKind, f64, P)>,
    /// Where the element was clicked (part coordinates).
    pub sel_pick: Option<(usize, P)>,
    /// Texts selected in the Part view (window selection).
    pub sel_texts: Vec<usize>,
    // Text being added to a part (Technology tab, Text group).
    pub text_draft: String,
    pub text_height: f64,
    pub text_kind: nest::TextKind,
    pub placing_text: bool,
    /// Arc radius for new texts (0 = straight; see `PartText::curve`).
    pub text_curve: f64,
    /// Manual machining: contours picked by hand, per sheet, in cutting order.
    pub manual_cuts: Vec<Vec<cnc::ManualCut>>,
    pub manual_mode: bool,
    pub manual_reverse: bool,
    /// A sweep across the sheet's parts in progress in Manual cutting (MM8),
    /// with which parts the picks before it finish.
    pub manual_sweep: Option<(engine::sheetpick::PartSweep, Vec<bool>)>,
    /// A box being drawn in Manual cutting from this corner (screen; MM9).
    pub manual_box: Option<Pos2>,
    /// The radial (gesture) menu, when open.
    pub radial: Option<crate::radial::Radial>,
    /// The quick text editor, when open.
    pub text_editor: Option<crate::textedit::TextEditor>,
    /// A part picked in the Parts View, following the pointer until dropped.
    pub ghost: Option<crate::ghost::Ghost>,
    /// Cut lines on each sheet that belong to no part (remnant line, free paths).
    pub cut_lines: Vec<Vec<cutoff::CutLine>>,
    pub cut_tool: Option<cutoff::CutTool>,
    pub cut_draft: Vec<P>,
    pub sel_cut: Option<usize>,
    /// Moving a straight cut line: (index, grab point, its points before).
    pub cut_drag: Option<(usize, P, Vec<P>)>,
    pub round_edge: bool,
    /// The open job's ERP/MES link (see Job::erp).
    pub erp_job: Option<integrations::ErpJob>,
    /// The open job's material names as read (`Job::material_names`).
    pub job_material_names: engine::material::ReadNames,
    /// ERP/MES window and background requests (erpui.rs).
    pub erp: erpui::ErpUi,
}

/// Everything an undo step needs to restore.
#[derive(Clone)]
pub struct Snapshot {
    parts: Vec<Part>,
    spec: SheetSpec,
    result: NestResult,
    cur_sheet: usize,
    stale: bool,
    /// Each sheet's cut lines and hand-picked cut order, so undoing a nest
    /// brings back the lines that went with the old layout.
    cut_lines: Vec<Vec<cutoff::CutLine>>,
    manual_cuts: Vec<Vec<cnc::ManualCut>>,
}

fn demo_parts() -> Vec<Part> {
    let p = |name: &str, shape, w, h, qty, color| Part { name: name.into(), shape, w, h, qty, rotate: true, color, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
    vec![
        p("810", Shape::Strip, 980.0, 60.0, 4, 0),
        p("1210", Shape::Strip, 1440.0, 60.0, 4, 1),
        p("Square", Shape::Rectangle, 190.0, 190.0, 2, 2),
        p("Square hole", Shape::PlateHole, 190.0, 190.0, 2, 3),
        p("Ring", Shape::Disc, 200.0, 200.0, 8, 4),
        p("Bracket", Shape::LBracket, 150.0, 120.0, 12, 5),
        p("Gusset", Shape::Triangle, 120.0, 120.0, 14, 6),
        p("Clip", Shape::Flange, 110.0, 70.0, 16, 7),
        p("Angle", Shape::LBracket, 60.0, 45.0, 40, 8),
    ]
    .into_iter()
    .map(|mut part| {
        // Two sample parts carry their own technology.
        part.tech = match part.name.as_str() {
            "Square hole" => Some(nest::PartTech { lead_in: 4.0, lead_out: 2.0, micro_joints: 4, joint_width: 0.6, joint_type: nest::JointType::Leads, quality: nest::CutQuality::new(1), corner_start: true, lead_type: nest::LeadType::LineArc, ..Default::default() }),
            "1210" => Some(nest::PartTech { lead_in: 6.0, lead_out: 0.0, micro_joints: 6, joint_width: 0.8, quality: None, ..Default::default() }),
            _ => None,
        };
        // The holed plate carries its reference as a special marking text.
        if part.name == "Square hole" {
            // Curved over the hole, placed relative to it like "Text on element".
            let g = part.geometry();
            let elems = nest::elements_of(&g);
            if let Some(t) = elems.iter().find(|e| e.contour == 0).and_then(|e| partview::text_for_element(&g, e, "#REF", 11.0, nest::TextKind::Special)) {
                part.texts.push(t);
            }
            part.texts.push(nest::PartText { text: "QTY #QTY".into(), at: [52.0, 14.0], height: 9.0, angle: 0.0, curve: 0.0, kind: nest::TextKind::Special });
        }
        part
    })
    .chain(dxf::read(dxf::SAMPLE_DXF).ok().and_then(|d| dxf::build(&d, &dxf::sample_roles()).ok()).map(|(g, _)| Part {
        name: "Plate-R60 (DXF)".into(),
        shape: Shape::Custom,
        w: g.w,
        h: g.h,
        qty: 3,
        rotate: true,
        color: 9,
        tech: None,
        texts: g.texts.clone(),
        group: String::new(),
        erp: vec![],
        custom: Some(std::sync::Arc::new(g)),
        rots: Default::default(),
    }))
    .collect()
}

/// A few sample work orders grouped by week, each already nested.
fn demo_jobs(first: &App) -> Vec<Job> {
    let spec = first.spec.clone();
    let p = |name: &str, shape, w, h, qty, color| Part { name: name.into(), shape, w, h, qty, rotate: true, color, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
    let mk = |week: &str, name: &str, parts: Vec<Part>, spec: SheetSpec| {
        let result = nest::nest(&parts, &spec);
        Job { week: week.into(), name: name.into(), parts, spec, result, stale: false, lib_id: None, cut_lines: vec![], manual_cuts: vec![], cam_changed: None, held: Default::default(), erp: None, file: None, saved: None, lib_read: Default::default(), lib_folder: None, material_names: vec![], read_only: None }
    };
    let mut steel5 = spec.clone();
    steel5.thickness = 5.0;
    let mut alu = spec.clone();
    alu.material = 2;
    alu.thickness = 2.0;
    alu.w = 2500.0;
    alu.h = 1250.0;
    vec![
        Job {
            week: "W09_10".into(),
            name: first.job_name.clone(),
            parts: first.parts.clone(),
            spec: first.spec.clone(),
            result: first.result.clone(),
            stale: false,
            lib_id: None,
            lib_read: Default::default(),
            cut_lines: vec![],
            manual_cuts: vec![],
            cam_changed: None,
            file: None,
            saved: None,
            lib_folder: None,
            material_names: vec![],
            held: Default::default(),
            erp: None,
            read_only: None,
        },
        mk(
            "W09_10",
            "Brackets-0311",
            vec![p("BR-100", Shape::LBracket, 220.0, 180.0, 24, 5), p("BR-101", Shape::LBracket, 160.0, 120.0, 30, 1), p("Gusset-S", Shape::Triangle, 140.0, 140.0, 40, 6)],
            steel5,
        ),
        mk(
            "W11",
            "Covers-0312",
            vec![p("Cover-L", Shape::SlottedPlate, 900.0, 300.0, 6, 4), p("Cover-S", Shape::SlottedPlate, 450.0, 200.0, 12, 9), p("Plate-H", Shape::PlateHole, 250.0, 250.0, 10, 3)],
            alu,
        ),
        mk(
            "W11",
            "Frames-A",
            vec![p("Rail-1400", Shape::Strip, 1400.0, 80.0, 12, 1), p("Rail-900", Shape::Strip, 900.0, 80.0, 16, 0), p("Flange-200", Shape::Flange, 200.0, 200.0, 20, 7)],
            spec.clone(),
        ),
        mk(
            "W12",
            "Discs-Batch",
            vec![p("Disc-300", Shape::Disc, 300.0, 300.0, 18, 4), p("Disc-180", Shape::Disc, 180.0, 180.0, 30, 2), p("Washer-90", Shape::Disc, 90.0, 90.0, 60, 9)],
            spec,
        ),
    ]
}

impl App {
    fn new(shot: Option<PathBuf>) -> Self {
        let mut a = App {
            job_name: "Sample job 1-3.0000".into(),
            parts: demo_parts(),
            sel_part: Some(1),
            spec: SheetSpec { w: 2000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: None, pick: Default::default(), sheet_list: vec![] },
            result: NestResult::default(),
            plans: vec![],
            program: String::new(),
            stale: false,
            cur_sheet: 0,
            sel_placed: None,
            sel_more: vec![],
            drag: None,
            turn_drag: None,
            turn_text: String::new(),
            turn_custom: turnbar::CUSTOM_STEP_DEFAULT,
            turn_focus: false,
            window_sel: None,
            step_popup: false,
            panning: false,
            zoom: 1.0,
            pan: Vec2::ZERO,
            tab: Tab::Home,
            bottom: BottomTab::PartsView,
            collision: true,
            overlaps: vec![],
            last_export: None,
            export_also_lines: vec![],
            focus_placed: None,
            problem_cursor: 0,
            carry: None,
            show_path: false,
            show_numbers: false,
            show_grid: false,
            show_program: false,
            dock: dock::Dock::default(),
            show_times: false,
            status: String::new(),
            cursor_mm: None,
            hover_name: None,
            shot,
            frames: 0,
            drag_test_on: false,
            drag_test: dock::DragTest::default(),
            color_seq: 9,
            undo: vec![],
            redo: vec![],
            last_edit: -10.0,
            search: String::new(),
            toasts: vec![],
            last_status: String::new(),
            anim_pending: false,
            anim_start: 0.0,
            workspace: Workspace::Nesting,
            sheet_tab: SheetTab::View,
            jobs: vec![],
            cur_job: 0,
            job_search: String::new(),
            show_labor_ticket: false,
            jobs_pick_sheet: false,
            pending: None,
            quit_ask: None,
            quit_confirmed: false,
            quit_wait: None,
            lib_dir_edit: None,
            lib_new_ask: None,
            firstrun: None,
            warn_tech: true,
            tech_answer_all: None,
            tech_all_box: false,
            lead_choice: nest::LeadType::Machine,
            lead_menu_demo: None,
            lead_pos: 1,
            lead_scope: nest::LeadScope::All,
            tech_all: false,
            tech_nest_all: None,
            demo_open_menu: false,
            scroll_to_part: None,
            result_card: None,
            auto_joints: nest::AutoJoints::default(),
            show_joint_sizes: false,
            joint_tab: 0,
            show_cut_ranges: false,
            group_nest: crate::ct1::active().group_parts,
            job_folders: vec![],
            tree_edit: None,
            confirm_delete_job: None,
            empty_sheet_kept: None,
            cycles_dlg: None,
            open_set: vec![],
            open_idx: 0,
            open_scope: OpenScope::All,
            part_zoom: 1.0,
            part_pan: Vec2::ZERO,
            part_view_hist: vec![],
            part_ext_lock: None,
            lead_same: false,
            lead_offer: None,
            sel_lead: None,
            lead_card_demo: false,
            lead_card_at: None,
            part_view_last: 0.0,
            follow_system: false,
            options_open: false,
            job_path: None,
            job_link: None,
            disk_poll: -10.0,
            libui: Default::default(),
            importui: Default::default(),
            stockui: Default::default(),
            job_lib_id: None,
            lib_read: Default::default(),
            inuse: Default::default(),
            who: shared::Who::this_pc(),
            clock: shared::Clock::pc(),
            autorefresh: Default::default(),
            feedback: Default::default(),
            sheet_list_ui: Default::default(),
            dv: drawing::DrawView::default(),
            cut_panel: Default::default(),
            order_pick: Default::default(),
            ssim: None,
            cfg: settings::AppSettings::load(),
            cfg_saved: settings::AppSettings::default(),
            setui: Default::default(),
            rollout: Default::default(),
            handoff: &updates::HANDOFF,
            applied_machine: None,
            post: post::PostConfig::default(),
            dbb: Default::default(),
            blm: blm::BlmSettings::for_machine(""),
            nverify: v44::NestVerify { check_safe: true, value: 5.0, ..Default::default() },
            dropsui: Default::default(),
            sel_elems: vec![],
            elem_part: None,
            box_start: None,
            sel_elem_info: vec![],
            sel_pick: None,
            sel_texts: vec![],
            text_draft: "#REF".into(),
            text_height: 8.0,
            text_kind: nest::TextKind::Special,
            placing_text: false,
            text_curve: 0.0,
            manual_cuts: vec![],
            cam_changed: None,
            held: Default::default(),
            semi_mode: Default::default(),
            naming: None,
            remnant_names: None,
            manual_mode: false,
            manual_reverse: false,
            manual_sweep: None,
            manual_box: None,
            radial: None,
            text_editor: None,
            ghost: None,
            cut_lines: vec![],
            cut_tool: None,
            cut_draft: vec![],
            sel_cut: None,
            cut_drag: None,
            round_edge: true,
            erp_job: None,
            job_material_names: vec![],
            erp: Default::default(),
        };
        // --library-dir=<folder> (screenshots, scripts): the Library this run
        // works on, set before anything reads one, so the configured Library
        // is never read or joined (MA28) in its place.
        if let Some(d) = library_dir_arg() {
            a.cfg.library_dir = d;
        }
        // --shop-demo (screenshot runs only): a temporary Library of its own,
        // never the configured one or --library-dir's (SH5).
        if a.shot.is_some() && settings_ui::shop_demo_arg().is_some() {
            // No temporary folder: stop before any Library is read, so a
            // screenshot can never show the configured (real) shop.
            let Some(d) = settings_ui::shop_demo_dir() else {
                eprintln!("--shop-demo: couldn't make a temporary folder for the demo Library; nothing was read.");
                std::process::exit(2);
            };
            a.cfg.library_dir = d.display().to_string();
        }
        // The material list (MA14): the Library's, or this PC's own.
        a.reload_materials(true);
        a.run_nesting();
        a.undo.clear();
        a.jobs = demo_jobs(&a);
        // The examples count as saved: nothing to lose when closing them.
        for i in 0..a.jobs.len() {
            a.mark_job_saved(i);
        }
        a
    }

    /// Keep the open job's copy in the job list up to date.
    pub fn store_job(&mut self) {
        if let Some(j) = self.jobs.get_mut(self.cur_job) {
            j.name = self.job_name.clone();
            j.parts = self.parts.clone();
            j.spec = self.spec.clone();
            j.result = self.result.clone();
            j.stale = self.stale;
            j.lib_id = self.job_lib_id;
            j.lib_read = self.lib_read.clone();
            j.cut_lines = self.cut_lines.clone();
            j.manual_cuts = self.manual_cuts.clone();
            j.cam_changed = self.cam_changed;
            j.held = self.held.clone();
            j.file = self.job_link.clone();
            j.erp = self.erp_job.clone();
            j.material_names = self.job_material_names.clone();
        }
    }

    /// Open another job: its products, sheet settings and nested sheets.
    pub fn open_job(&mut self, i: usize) {
        // The Cycles window belongs to the job it was opened on.
        self.cycles_dlg = None;
        if i >= self.jobs.len() || i == self.cur_job {
            return;
        }
        self.store_job();
        let j = self.jobs[i].clone();
        self.cur_job = i;
        self.job_name = j.name;
        self.parts = j.parts;
        self.spec = j.spec;
        self.result = j.result;
        self.stale = j.stale;
        self.job_lib_id = j.lib_id;
        self.lib_read = j.lib_read;
        // The job's own cut lines and hand-cut contours (not the previous job's).
        self.cut_lines = j.cut_lines;
        self.manual_cuts = j.manual_cuts;
        self.cam_changed = j.cam_changed;
        self.held = j.held;
        // The job's own file (not the previous job's).
        self.job_path = j.file.as_ref().map(|l| l.path.clone());
        self.job_link = j.file;
        self.erp_job = j.erp;
        self.job_material_names = j.material_names;
        self.ghost = None;
        self.sel_cut = None;
        self.cur_sheet = 0;
        self.sel_placed = None;
        self.sel_part = if self.parts.is_empty() { None } else { Some(0) };
        self.undo.clear();
        self.redo.clear();
        // A question about the last job's library part goes with it (LB9):
        // answered here, it would save this job's part of that name.
        if matches!(self.libui.question, Some(crate::libui::SaveQuestion::Part { .. })) {
            self.libui.question = None;
        }
        self.jobs_reindexed();
        self.refresh();
        self.status = format!("Opened job {}.", self.job_name);
        // The job in front is in use; the one left keeps its hold only with
        // unsaved changes (CU5).
        self.sync_holds();
    }

    /// Open a nesting from machine programs (.zip / .XML / .SPF, see
    /// progimport.rs) as a new job, ready to edit in the Nesting module.
    pub fn open_nesting_file(&mut self, path: &std::path::Path) {
        let pj = match engine::imports::program_job(&self.cfg, &self.spec, path, self.color_seq) {
            Ok(pj) => pj,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        self.color_seq += pj.parts.len();
        let name = self.unique_job_name(&pj.name).unwrap_or_else(|| pj.name.clone());
        let spec = pj.spec.clone();
        let n_parts = pj.sheet.len();
        let notes = pj.notes.clone();
        let cuts = pj.cuts.clone();
        self.store_job();
        self.jobs.push(Job {
            week: "Opened programs".into(),
            name: name.clone(),
            parts: pj.parts,
            spec: spec.clone(),
            result: nest::NestResult { sheets: vec![pj.sheet], formats: vec![nest::SheetFormat::rect(spec.w, spec.h)], method: "opened from machine program".into(), tried: 0 },
            stale: false,
            lib_id: None,
            lib_read: Default::default(),
            cut_lines: vec![],
            manual_cuts: vec![],
            cam_changed: None,
            held: Default::default(),
            file: None,
            saved: None,
            lib_folder: None,
            material_names: vec![],
            erp: None,
            read_only: None,
        });
        let i = self.jobs.len() - 1;
        self.open_job(i);
        // Cut-offs come back as remnant lines (edge to edge) or free cuts.
        self.cut_lines = vec![cuts];
        self.workspace = Workspace::Nesting;
        self.refresh();
        let extra = if notes.is_empty() { String::new() } else { format!(" ({})", notes.join("; ")) };
        self.status = format!("Opened {name}: {n_parts} parts of {} kinds on a {} sheet — move, rotate, add or remove parts, then export.{extra}", self.parts.len(), self.unit().size(spec.w, spec.h, 0));
    }

    /// Save the open job (Save as… asks where; Save reuses the last place).
    pub fn save_job(&mut self, ask: bool) {
        let safe: String = self.job_name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '.' || c == ' ' { c } else { '_' }).collect();
        let path = match (&self.job_path, ask) {
            (Some(p), false) => p.clone(),
            _ => {
                let dir = if !self.cfg.export_dir.is_empty() { PathBuf::from(&self.cfg.export_dir) } else { library::documents_dir() };
                let _ = std::fs::create_dir_all(&dir);
                match rfd::FileDialog::new().add_filter("Sheet nesting job", &[jobfile::EXT]).set_directory(&dir).set_file_name(format!("{safe}.{}", jobfile::EXT)).save_file() {
                    Some(p) => p,
                    None => return,
                }
            }
        };
        self.save_job_to(&path);
    }

    pub fn save_job_to(&mut self, path: &std::path::Path) {
        // UP9: a job a newer version saved is never saved by this one.
        if let Some(why) = self.read_only() {
            self.status = format!("Not saved: this job is read-only on this PC. {why}");
            return;
        }
        // Someone else (an agent) changed the file since the window last read
        // or wrote it: don't write over their work without asking which to keep.
        if let Some(link) = self.job_link.clone().filter(|l| l.path == path) {
            let m = filewatch::mtime(path);
            if m.is_some() && m != link.mtime && jobfile::load(path).is_ok_and(|d| engine::imports::job_sig(&d) != link.disk_sig) {
                self.pending = Some(Pending::JobFileChanged { path: path.to_path_buf() });
                self.status = "Not saved: the job's file was changed by another program — choose which version to keep first.".into();
                return;
            }
        }
        // UP7: never over a file a newer version saved: asked before any
        // question, and again under the write lock below.
        let versions = self.versions();
        if let Err(e) = jobfile::may_save(path, &versions) {
            self.status = format!("Couldn't save the job: {e}");
            return;
        }
        crate::shared::pause("window save: checked");
        // A Library job's sheets are booked when it's saved to the Library,
        // after its revision check (LB8): saved to a file, it leaves the
        // stock alone, and the file is a copy holding none of its sheets or
        // remnants (as LB9's copy), so it never gives back what the record
        // holds (LB13).
        let library_job = self.job_lib_id.is_some();
        if !library_job && !self.ready_to_save(stockui::SaveTarget::File(path.to_path_buf())) {
            return;
        }
        // UP7: from the version check through the write, under the
        // Library's write lock, and checked before any sheet is booked, so a
        // newer version's save in between is never written over and a
        // refused save books nothing.
        // The shop version is read again under the lock: a roll back that
        // landed while this save waited makes this PC's release the one
        // left, and the shop read-only on it (UP26).
        let mut versions = versions;
        let lock = match jobfile::save_lock(&self.library_dir()).and_then(|l| {
            versions = self.versions();
            jobfile::may_save(path, &versions).map(|_| l)
        }) {
            Ok(l) => l,
            Err(e) => {
                self.status = format!("Couldn't save the job: {e}");
                return;
            }
        };
        let stock_note = if library_job {
            self.result.normalise(&self.spec);
            " — its sheets are booked when it's saved to the Library.".to_string()
        } else {
            self.stock_on_save()
        };
        self.store_job();
        let (mut formats, mut held) = (self.result.formats.clone(), self.held.clone());
        if library_job {
            engine::stockflow::forget_booking(&mut formats, &mut held);
        }
        let week = self.jobs.get(self.cur_job).map_or(String::new(), |j| j.week.clone());
        let j = jobfile::JobFile {
            // As the file records it once saved (UP5), so the file watcher
            // takes the file as this save's.
            program_version: Some(updates::VERSION.to_string()),
            format: jobfile::FORMAT.into(),
            version: jobfile::VERSION,
            name: self.job_name.clone(),
            week,
            parts: self.parts.clone(),
            spec: self.spec.clone(),
            sheets: self.result.sheets.clone(),
            formats,
            held,
            cut_lines: self.cut_lines.clone(),
            method: self.result.method.clone(),
            manual_cuts: self.manual_cuts.clone(),
            stale: self.stale,
            // The window books the stock on every save (stock_on_save), a
            // Library job's only when saved to the Library.
            booked: !library_job,
            erp: self.erp_job.clone(),
            saved: None,
            library: None,
            // The library parts it read, and at which revision (LB7).
            library_parts: self.lib_read.parts.clone(),
            material_name: engine::material::read_name(self.spec.material, &self.job_material_names),
        };
        let saved = jobfile::save_under(path, &j, &versions, lock.as_ref());
        drop(lock);
        match saved {
            Ok(()) => {
                if library_job {
                    // The file is a copy (LB13): the window doesn't follow
                    // it, or an agent's change to it, reloaded, would drop
                    // the job's booking and the next Save to library would
                    // take its sheets again. Nor does the job keep it as its
                    // own file: Save asks where, as for any Library job.
                    self.job_path = None;
                    self.job_link = None;
                    self.store_job();
                } else {
                    self.link_job_file(path, &j);
                }
                self.mark_job_saved(self.cur_job);
                // An agent's one-step undo copy is from before this save: undoing
                // to it would drop what was just saved.
                let _ = std::fs::remove_file(engine::session::backup_path(path));
                self.note_recent_job(path);
                self.status = format!("Saved job {} to {}{stock_note}", self.job_name, path.display());
            }
            Err(e) => self.status = format!("Couldn't save the job: {e}"),
        }
    }

    /// When the program opens: settle which Library this PC works in (UP17:
    /// a trial the shop moved on from ends, back to the shop folder; UP18:
    /// the tried release's one-time changes on the copy), then reopen the
    /// active job from that Library (AJ2), so no job of the old one stays
    /// open (S6).
    pub fn open_at_start(&mut self) {
        self.resolve_trial_at_start();
        if let Some(id) = crate::jobtree::active_job_id(&self.cfg.active_job) {
            self.open_library_job(id);
            if self.job_lib_id != Some(id) {
                self.cfg.active_job.clear();
            } else {
                self.status = format!("Reopened the active job {}.", self.job_name);
            }
        }
    }

    /// Open a saved job as a job of its own (a second copy gets " (1)").
    pub fn open_job_file(&mut self, path: &std::path::Path) {
        // N1: no job is opened while a roll out runs.
        if self.rollout.rolling {
            self.status = rolloutui::ROLLING.into();
            return;
        }
        let (j, written) = match jobfile::load_versioned(path) {
            Ok(j) => j,
            Err(e) => {
                self.status = format!("Couldn't open {}: {e}", path.display());
                return;
            }
        };
        let disk = j.clone();
        let name = self.unique_job_name(&j.name).unwrap_or_else(|| j.name.clone());
        self.store_job();
        let tried = 0;
        self.jobs.push(Job { week: if j.week.is_empty() { "Opened jobs".into() } else { j.week }, name, parts: j.parts, spec: j.spec.clone(), result: { let mut r = nest::NestResult { sheets: j.sheets, formats: j.formats, method: j.method, tried }; r.normalise(&j.spec); r }, stale: j.stale, lib_id: None, cut_lines: vec![], manual_cuts: vec![], cam_changed: None, held: j.held.clone(), erp: None, file: None, saved: None, lib_read: libui::LibRead { revision: 0, restore: 0, parts: j.library_parts }, lib_folder: None, material_names: engine::material::names_read(j.spec.material, &j.material_name), read_only: self.versions().at(path).newer(written.as_deref()).map(|n| n.message()) });
        let i = self.jobs.len() - 1;
        self.open_job(i);
        self.cut_lines = j.cut_lines;
        self.manual_cuts = j.manual_cuts;
        self.stale = j.stale;
        self.erp_job = j.erp;
        self.job_lib_id = None;
        self.refresh();
        self.link_job_file(path, &disk);
        self.store_job();
        self.mark_job_saved(self.cur_job);
        self.note_recent_job(path);
        self.status = format!("Opened job {} from {} — {} product(s), {}.", self.job_name, path.display(), self.parts.len(), engine::sheets_text(&self.result.formats, self.result.sheets.len()));
        if let Some(why) = self.read_only() {
            self.status = format!("Opened job {} read-only. {why}", self.job_name);
        }
    }

    /// UP7, UP8: the versions this window's files are judged by: this
    /// program's, and the shop's when its Library is a shop.
    pub fn versions(&self) -> shared::Versions {
        shop::versions(&self.library_dir(), updates::VERSION)
    }

    /// UP9: why the job in front is read-only here (a newer version saved
    /// it), or None.
    pub fn read_only(&self) -> Option<String> {
        self.jobs.get(self.cur_job).and_then(|j| j.read_only.clone())
    }

    pub fn open_job_dialog(&mut self) {
        // Starts where jobs are saved by default.
        let dir = self.cfg.recent_jobs.first().and_then(|p| PathBuf::from(p).parent().map(|d| d.to_path_buf())).unwrap_or_else(|| self.export_dir());
        if let Some(f) = rfd::FileDialog::new().add_filter("Sheet nesting job", &[jobfile::EXT]).set_directory(&dir).pick_file() {
            self.open_job_file(&f);
        }
    }

    /// A job file just opened or saved goes to the top of the recent list.
    pub fn note_recent_job(&mut self, path: &std::path::Path) {
        let p = path.display().to_string();
        self.cfg.recent_jobs.retain(|r| r != &p);
        self.cfg.recent_jobs.insert(0, p);
        self.cfg.recent_jobs.truncate(8);
        // A screenshot run keeps it in memory only: it never saves the
        // settings (its flags, such as --machine-gas, change them for the
        // picture alone).
        if !self.shot_mode_now() {
            let _ = self.cfg.save();
        }
    }

    /// The Open job menu: recent job files, the library, or any file.
    pub fn open_job_menu(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(380.0);
        if !self.libui.loaded {
            self.lib_refresh();
        }
        // One list of the jobs saved lately, wherever they were saved — job
        // files and the Library's jobs — newest first.
        enum Src {
            File(PathBuf),
            Lib(u64),
        }
        let mut rows: Vec<(u64, String, String, Src, bool)> = vec![];
        for r in &self.cfg.recent_jobs {
            let p = PathBuf::from(r);
            let name = p.file_stem().map_or(r.clone(), |n| n.to_string_lossy().to_string());
            let folder = p.parent().map(|d| d.display().to_string()).unwrap_or_default();
            let t = std::fs::metadata(&p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
            let exists = p.exists();
            rows.push((t, name, folder, Src::File(p), exists));
        }
        for e in self.libui.entries.iter().filter(|e| e.kind == library::Kind::Job) {
            // Its folder as the Library's folder tree shows it (JX10).
            let folder = self.libui.tree.path(self.libui.tree.folder_of(e.id));
            let what = format!("{} · {} sheet(s){}", self.mat_thk(e.material, e.thickness), e.count_b, if folder.is_empty() { String::new() } else { format!(" · {folder}") });
            rows.push((e.updated, e.name.clone(), what, Src::Lib(e.id), true));
        }
        rows.sort_by(|a, b| b.0.cmp(&a.0));
        ui.label(RichText::new("Recent jobs").strong().color(th().text_strong));
        ui.label(RichText::new("Job files and Library jobs together, newest first").size(11.5).color(th().text_dim));
        // The Library's jobs are missing because it can't be reached (LB11).
        if let Some(e) = self.libui.error.clone() {
            let (again, folder) = crate::libui::unreachable_note(ui, &format!("Library jobs aren't listed: {e}"));
            if again {
                self.lib_retry();
            }
            if folder {
                self.open_settings(crate::settings_ui::Section::Folders);
                ui.close();
            }
        }
        if rows.is_empty() {
            ui.label(RichText::new("None yet — jobs you save or open show up here.").size(12.0).color(th().text_dim));
        }
        let id = egui::Id::unique("open_job_search");
        let mut needle = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_default();
        if rows.len() > 6 {
            ui.add(egui::TextEdit::singleline(&mut needle).hint_text("🔍 Find a job…").desired_width(f32::INFINITY));
            ui.data_mut(|d| d.insert_temp(id, needle.clone()));
        }
        let n = needle.to_lowercase();
        let mut open_file = None;
        let mut open_lib = None;
        let when = |t: u64| {
            if t == 0 {
                return String::new();
            }
            let days = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()).saturating_sub(t)) / 86_400;
            match days {
                0 => "today".to_string(),
                1 => "yesterday".to_string(),
                d => format!("{d} days ago"),
            }
        };
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for (t, name, detail, src, exists) in rows.iter().filter(|r| n.is_empty() || r.1.to_lowercase().contains(&n) || r.2.to_lowercase().contains(&n)).take(14) {
                let (badge, tip) = match src {
                    Src::File(p) => ("File", if *exists { p.display().to_string() } else { format!("{}\n(moved or deleted)", p.display()) }),
                    Src::Lib(_) => ("Library", format!("Saved to the Library ({})", self.library_dir().display())),
                };
                ui.horizontal(|ui| {
                    let resp = ui.add_enabled(*exists, egui::Button::new(RichText::new(name).strong()).frame(false)).on_hover_text(tip);
                    ui.label(RichText::new(badge).size(10.5).color(th().text_dim).background_color(th().panel));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(when(*t)).size(11.0).color(th().text_dim));
                    });
                    if resp.clicked() {
                        match src {
                            Src::File(p) => open_file = Some(p.clone()),
                            Src::Lib(id) => open_lib = Some(*id),
                        }
                    }
                });
                ui.label(RichText::new(detail).size(11.5).color(th().text_dim));
                // Another PC has it in use (CU10).
                if let Some(n) = match src {
                    Src::Lib(id) => self.in_use_note(*id),
                    Src::File(_) => None,
                } {
                    crate::inuseui::in_use_chip(ui, &n);
                }
                ui.add_space(2.0);
            }
        });
        ui.separator();
        if ui_parts::menu_row(ui, "📂", "Browse for a job file…", "Ctrl + O") {
            self.open_job_dialog();
        }
        if ui_parts::menu_row(ui, "📚", "Library…", "all saved jobs and products") {
            self.libui.open = true;
            self.libui.tab = 0;
        }
        if let Some(p) = open_file {
            self.open_job_file(&p);
        }
        if let Some(id) = open_lib {
            self.open_library_job(id);
        }
    }

    /// The next part colour.
    pub fn color_seq_next(&mut self) -> usize {
        self.color_seq += 1;
        self.color_seq - 1
    }

    /// Name rule from the original (MECHANICS.md first table, #9): if the
    /// name is taken, try "Name (1)" … "Name (9)"; give up after 9 tries or when the
    /// name is longer than 250 characters.
    pub fn unique_job_name(&self, base: &str) -> Option<String> {
        let taken = |n: &str| self.jobs.iter().enumerate().any(|(i, j)| if i == self.cur_job { self.job_name == n } else { j.name == n });
        if base.chars().count() > 250 {
            return None;
        }
        if !taken(base) {
            return Some(base.to_string());
        }
        (1..10).map(|k| format!("{base} ({k})")).find(|n| !taken(n))
    }

    pub fn new_job_in_week(&mut self, week: String) {
        let Some(name) = self.unique_job_name("New job") else {
            self.status = "Couldn't create the job: 'New job (1)' to '(9)' are all taken.".into();
            return;
        };
        self.store_job();
        self.jobs.push(Job { week, name, parts: vec![], spec: engine::with_prefs(&self.spec, &self.cfg), result: NestResult::default(), stale: false, lib_id: None, cut_lines: vec![], manual_cuts: vec![], cam_changed: None, held: Default::default(), erp: None, file: None, saved: None, lib_read: Default::default(), lib_folder: None, material_names: vec![], read_only: None });
        let i = self.jobs.len() - 1;
        self.open_job(i);
        self.status = format!("Created job {} — add products, then nest.", self.job_name);
    }

    /// Copy the open job, products and sheets included, under the same name rule.
    pub fn copy_job(&mut self) {
        let base = self.job_name.clone();
        let Some(name) = self.unique_job_name(&base) else {
            self.status = format!("Couldn't copy: '{base} (1)' to '(9)' are all taken.");
            return;
        };
        self.store_job();
        let mut j = self.jobs[self.cur_job].clone();
        j.name = name;
        // Not the original's library record or file, or saving the copy would
        // overwrite it; and never saved.
        j.lib_id = None;
        j.file = None;
        j.saved = None;
        // Nor its stock: the copy takes its own sheets when it's saved, and the
        // original's remnants stay the original's.
        j.held.clear();
        for f in j.result.formats.iter_mut() {
            f.made.clear();
            f.made_from = 0;
        }
        // Nor its work order: the copy's nest would replace the original's.
        if let Some(e) = j.erp.as_mut() {
            e.work_order = None;
            e.sent = None;
        }
        self.jobs.insert(self.cur_job + 1, j);
        self.jobs_reindexed();
        let i = self.cur_job + 1;
        self.open_job(i);
        self.status = format!("Copied job to {}.", self.job_name);
    }

    /// Identical sheets folded together, in nesting order (original: rows with
    /// CopyIndx > 1 are hidden and Quantity counts the copies — MECHANICS.md #4).
    /// Returns (first sheet index, number of identical sheets).
    pub fn sheet_groups(&self) -> Vec<(usize, usize)> {
        engine::sheet_groups(&self.result, &self.spec)
    }

    /// Add part drawings as parts (dropped on the window, picked in the
    /// system's file picker). A drawing with several layer rows, or rows that
    /// start as Leave out (DX3), opens the Import drawings window instead,
    /// picked and with its cut technology panel open, so its layers are set
    /// before it comes in (IM12).
    pub fn import_drawings(&mut self, paths: Vec<PathBuf>) {
        // Lantek import files (xsd\Importers: <Import><Product …>) go to the XML importer.
        let (xml, paths): (Vec<PathBuf>, Vec<PathBuf>) = paths.into_iter().partition(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml")));
        for x in &xml {
            self.import_xml(x);
        }
        if paths.is_empty() {
            return;
        }
        let mut added = 0;
        let mut notes: Vec<String> = vec![];
        let mut first = true;
        let mut layered: Vec<PathBuf> = vec![];
        for path in paths {
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Drawing".into());
            let reads = match formats::read_file(&path, None, &self.cfg) {
                Ok(p) => formats::incoming(&stem, p),
                Err(e) => {
                    notes.push(e);
                    continue;
                }
            };
            if reads.iter().any(|r| r.part.drawing.layers().len() > 1 || !r.part.drawing.hidden.is_empty()) {
                layered.push(path);
                continue;
            }
            added += self.add_drawing(&reads, &Default::default(), &mut notes, &mut first);
        }
        if !layered.is_empty() {
            self.review_drawings(layered);
        }
        self.finish_import(added, notes);
    }

    /// Open the Import drawings window on drawings whose layers need a look
    /// (IM12): picked, the first shown, with the cut technology panel open.
    fn review_drawings(&mut self, files: Vec<PathBuf>) {
        let Some(dir) = files.first().and_then(|f| f.parent()).map(std::path::Path::to_path_buf) else { return };
        let mut dirs = vec![dir];
        dirs.extend(self.cfg.import_dirs.iter().map(PathBuf::from));
        self.importui.show(&dirs);
        self.importui.picked = files.iter().cloned().collect();
        self.importui.focus = files.first().cloned();
        self.importui.panel.open = true;
        let names: Vec<String> = files.iter().filter_map(|f| f.file_name()).map(|n| n.to_string_lossy().to_string()).collect();
        self.status = format!("{} {} layers: set them in Cut technology, then Import.", names.join(", "), if names.len() == 1 { "has several" } else { "have several" });
    }

    /// Import drawings from the Import drawings window (IM11): each with the
    /// drawing as its preview read it and what was set for it there, or read
    /// now with the layer roles it starts with. A drawing set in the preview
    /// whose file changed since comes in as it starts, and the notes say so.
    pub fn import_chosen(&mut self, files: Vec<(PathBuf, importui::Chosen)>) {
        let (xml, files): (Vec<_>, Vec<_>) = files.into_iter().partition(|(p, _)| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml")));
        for (x, _) in &xml {
            self.import_xml(x);
        }
        if files.is_empty() {
            return;
        }
        let mut added = 0;
        let mut notes: Vec<String> = vec![];
        let mut first = true;
        for (path, chosen) in files {
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Drawing".into());
            let (reads, choices) = match chosen {
                importui::Chosen::Read(d, c) => (d.reads.clone(), c),
                read_now => {
                    if matches!(read_now, importui::Chosen::Changed) {
                        notes.push(format!("{stem}: the file or the import settings changed after its layers and contours were set in the preview, so it came in as it starts; set them again in Cut technology"));
                    }
                    match formats::read_file(&path, None, &self.cfg) {
                        Ok(p) => (formats::incoming(&stem, p), Default::default()),
                        Err(e) => {
                            notes.push(e);
                            continue;
                        }
                    }
                }
            };
            added += self.add_drawing(&reads, &choices, &mut notes, &mut first);
        }
        self.finish_import(added, notes);
    }

    /// Make each part of one drawing file with the layers and contours chosen
    /// for it (IM11; `engine::imports::chosen_part`, as `import-drawing`) and
    /// add it to the job; how many were added.
    fn add_drawing(&mut self, reads: &[formats::Incoming], choices: &engine::imports::DrawingChoices, notes: &mut Vec<String>, first: &mut bool) -> usize {
        let roles = choices.roles(reads);
        let mut added = 0;
        for (ix, inc) in reads.iter().enumerate() {
            match engine::imports::chosen_part(&self.cfg, &self.spec, inc, &roles, choices.of(ix), self.color_seq) {
                Ok((part, n)) => {
                    if *first {
                        self.checkpoint();
                        *first = false;
                    }
                    notes.extend(n);
                    self.parts.push(part);
                    self.color_seq += 1;
                    added += 1;
                }
                Err(e) => notes.push(e),
            }
        }
        added
    }

    fn finish_import(&mut self, added: usize, notes: Vec<String>) {
        if added > 0 {
            self.sel_part = Some(self.parts.len() - 1);
            self.stale = true;
            self.refresh();
        }
        if added == 0 && notes.is_empty() {
            return; // the Import drawings window shows the rest (IM12)
        }
        self.status = if notes.is_empty() {
            format!("Imported {added} drawing(s). Set quantities, then Nest all.")
        } else {
            format!("Imported {added} drawing(s). {}", notes.join(" · "))
        };
        // The same, as a card on the drawing with the next steps.
        let first = self.parts.len() - added;
        let new = &self.parts[first..];
        let pcs: u32 = new.iter().map(|p| p.qty).sum();
        let mut names: Vec<String> = new.iter().take(4).map(|p| format!("{} × {}", p.name, p.qty)).collect();
        if added > 4 {
            names.push(format!("and {} more", added - 4));
        }
        let mut lines = vec![
            names.join(" · "),
            format!("Material {} — the job's; change it before nesting if it's wrong.", self.mat_thk(self.spec.material, self.spec.thickness)),
        ];
        lines.extend(notes.iter().take(4).cloned());
        if added > 0 {
            self.scroll_to_part = Some(first);
        }
        self.result_card = Some(ResultCard {
            title: if added == 0 { "Nothing imported".to_string() } else { format!("Imported {added} drawing(s) — {added} new part type(s), {pcs} pcs") },
            lines: if added == 0 { notes.clone() } else { lines },
            warn: added == 0 || !notes.is_empty(),
            actions: if added == 0 { vec![] } else { vec![("Nest all".to_string(), CardAct::NestAll), ("Show new parts".to_string(), CardAct::ShowPart(first)), ("Material and sheet".to_string(), CardAct::SheetTab)] },
        });
    }

    /// Finish drawings that waited for the layer question.
    /// Save the selected part as DXF (outline and holes on CUT, marking on MARK),
    /// with a one-line text file of the same name beside it — the way the original
    /// hands a part to its bending program (rule DF1).
    pub fn export_part_dxf(&mut self, idx: usize) {
        let Some(p) = self.parts.get(idx) else { return };
        let g = p.geometry();
        let marks = p.custom.as_ref().map_or(vec![], |c| c.marks.clone());
        let text = dxf::write(&g.outline, &g.holes, &marks);
        let safe = |s: &str| s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect::<String>();
        let dir = library::documents_dir()
            .join(safe(&self.job_name))
            .join("parts");
        let base = safe(&p.name);
        let info = format!("{} / {} / {} mm", p.name, crate::engine::material::name_or(self.spec.material, "?"), self.spec.thickness);
        let res = (|| -> std::io::Result<PathBuf> {
            std::fs::create_dir_all(&dir)?;
            let f = dir.join(format!("{base}.dxf"));
            std::fs::write(&f, text)?;
            std::fs::write(dir.join(format!("{base}.txt")), info + "\n")?;
            Ok(f)
        })();
        self.status = match res {
            Ok(f) => format!("Saved {} as DXF: {}", p.name, f.display()),
            Err(e) => format!("Couldn't save the DXF: {e}"),
        };
    }

    /// Carry out the answer to "this part has technology".
    pub fn apply_tech_choice(&mut self, idx: usize, draft: Part, choice: TechChoice) {
        if idx >= self.parts.len() {
            return;
        }
        self.checkpoint();
        match choice {
            TechChoice::EditCopy => {
                // Original inserts a copy of the part (rule PT2).
                let mut copy = draft;
                copy.tech = None;
                let base = self.parts[idx].name.clone();
                copy.name = engine::copy_name(&self.parts, &base);
                copy.color = self.color_seq;
                self.color_seq += 1;
                // Placed indices keep pointing at the same parts.
                engine::insert_part(&mut self.parts, &mut self.result.sheets, idx + 1, copy);
                self.sel_part = Some(idx + 1);
                self.status = format!("Kept '{}' with its technology; the change went to '{}'.", base, self.parts[idx + 1].name);
            }
            TechChoice::EditPart => {
                let mut p = draft;
                p.tech = None;
                self.status = format!("Changed '{}'; its technology was removed.", p.name);
                self.parts[idx] = p;
            }
        }
        self.stale = true;
        self.refresh();
    }

    /// Material/thickness change while parts are nested (original asks once:
    /// keep the nestings, remove them and free their sheets, or cancel — rule PT4).
    pub fn apply_spec_change(&mut self, new_spec: SheetSpec, remove_nestings: bool) {
        self.checkpoint();
        self.spec = new_spec;
        if remove_nestings {
            let n = self.result.sheets.len();
            self.result = NestResult::default();
            self.sel_placed = None;
            self.status = format!("Removed {n} nested sheet(s) and freed them. Nest again for the new material.");
        } else {
            self.stale = true;
            self.status = "Kept the nested sheets; they're marked Modified.".into();
        }
        self.refresh();
    }

    /// The question dialog, when one is waiting.
    fn pending_dialog(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pending.clone() else { return };
        if let Pending::JobFileChanged { path } = &p {
            if self.job_file_dialog(ctx, path) {
                self.pending = None;
            }
            return;
        }
        let mut close = false;
        // EXP4: a machine picked in the Export review, or a machine ticked.
        let review_machine = self.cfg.active_machine;
        let (mut rebuild, mut flip): (bool, Option<(usize, bool)>) = (false, None);
        let title = match &p {
            Pending::TechPartEdit { .. } => "Part with technology",
            Pending::NestedSpecChange { .. } => "Nested parts will change",
            Pending::JobFileChanged { .. } => "Job changed on disk",
            Pending::ExportReview { .. } => "Export CNC programs",
        };
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                match p {
                    Pending::ExportReview { problems, checks, also, earlier } => {
                        // What will be written, for which machine and where.
                        let n = self.result.sheets.len();
                        let dir = self.export_dir().join(engine::safe_name(&self.job_name));
                        let names: Vec<String> = (0..n).map(|k| self.cnc_name(k + 1)).collect();
                        let shown = names.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
                        let more = if n > 3 { format!(" and {} more", n - 3) } else { String::new() };
                        egui::Grid::new("export_review").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                            ui.label(RichText::new("Sheets").color(th().text_dim));
                            // SR1, SR21: one program per nesting, the sheets cut counted with their cycles.
                            ui.add(egui::Label::new(RichText::new(format!("all {} of {}", engine::sheets_text(&self.result.formats, n), self.job_name)).strong()).wrap());
                            ui.end_row();
                            // The machine can be changed here, without leaving the review;
                            // its program format comes with it (MK1).
                            ui.label(RichText::new("Machine").color(th().text_dim));
                            if self.machine_combo(ui, 200.0) {
                                self.refresh();
                            }
                            // EXP4: the other machines' rows follow the machine picked.
                            if self.cfg.active_machine != review_machine {
                                rebuild = true;
                            }
                            // Machines changed in Settings would leave the rows behind:
                            // the review closes; Export CNC shows it again.
                            if self.setui.open {
                                close = true;
                            }
                            ui.end_row();
                            ui.label(RichText::new("Program format").color(th().text_dim));
                            ui.label(self.cfg.machines.get(self.cfg.active_machine).map_or("—", |m| crate::settings_ui::output_name(m.output)));
                            ui.end_row();
                            ui.label(RichText::new("Files").color(th().text_dim));
                            ui.label(format!("{shown}{more}, and a parts report (.csv)"));
                            ui.end_row();
                            ui.label(RichText::new("Folder").color(th().text_dim));
                            ui.label(RichText::new(dir.display().to_string()).monospace().size(12.0));
                            ui.end_row();
                        });
                        // EXP4: the same sheets for other machines, each into its own folder.
                        if !also.is_empty() {
                            ui.add_space(6.0);
                            ui.label(RichText::new("Also write for").color(th().text_dim)).on_hover_text("When this machine is busy: the same sheets for another machine, with its own lead-ins, micro-joints and post, into its own programs folder");
                            for (i, row) in also.iter().enumerate() {
                                let Some(m) = self.cfg.machines.get(row.machine) else { continue };
                                let mut on = row.ticked;
                                let text = format!("{}  ·  {}", m.name, crate::settings_ui::output_name(m.output));
                                let r = ui.add_enabled(row.ready.is_ok(), egui::Checkbox::new(&mut on, text));
                                if r.changed() {
                                    flip = Some((i, on));
                                }
                                match &row.ready {
                                    Ok(()) => {
                                        ui.horizontal(|ui| {
                                            ui.add_space(24.0);
                                            ui.label(RichText::new(row.dir.display().to_string()).monospace().size(11.5).color(th().text_dim));
                                        });
                                    }
                                    Err(why) => {
                                        ui.horizontal(|ui| {
                                            ui.add_space(24.0);
                                            ui.add(egui::Label::new(RichText::new(format!("Can't take this nest: {why}.")).size(11.5).color(th().text_dim)).wrap());
                                        });
                                    }
                                }
                            }
                        }
                        ui.add_space(4.0);
                        ui.label(RichText::new("The programs follow each sheet's machining as Cutting path shows it — lead-ins, cut order, and your hand-picked order where you set one.").size(12.0).color(th().text_dim));
                        // Verify machining, run for this review on every sheet.
                        if checks.is_empty() {
                            ui.label(RichText::new(format!("✔ Verify machining: all {n} sheet(s) pass (overlaps, part gap, off the sheet, lead-ins)")).size(12.5).color(th().ok_fg));
                        } else {
                            let total: usize = checks.iter().map(|c| c.1).sum();
                            let which = checks.iter().map(|c| (c.0 + 1).to_string()).collect::<Vec<_>>().join(", ");
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(format!("⚠ Verify machining: {total} problem(s) on sheet {which}")).size(12.5).color(th().warn_fg));
                                if ui.link("Show them").clicked() {
                                    self.nverify.all_sheets = true;
                                    self.verify_nest();
                                    close = true;
                                }
                            });
                        }
                        // Files of an earlier export of this job that will be replaced, said plainly.
                        let old = std::fs::read_dir(&dir).map_or(0, |rd| rd.flatten().filter(|e| e.path().is_file()).count());
                        if old > 0 {
                            ui.label(RichText::new(format!("⚠ This folder already has {old} file(s) from an earlier export of this job — they will be replaced.")).size(12.5).color(th().warn_fg));
                        }
                        // EXP6: said before, so nobody is surprised a program went.
                        for e in &earlier {
                            ui.add(egui::Label::new(RichText::new(format!("⚠ {}", engine::overflow::earlier_line(&self.cfg, e))).size(12.5).color(th().warn_fg)).wrap());
                        }
                        let ticked: Vec<usize> = also.iter().filter(|r| r.ticked).map(|r| r.machine).collect();
                        if !problems.is_empty() {
                            ui.add_space(6.0);
                            ui.label(RichText::new("These sheets would cut parts into each other or past the sheet:").color(Color32::from_rgb(185, 28, 28)));
                            for &(k, pairs, off) in &problems {
                                let mut w = vec![];
                                if pairs > 0 {
                                    w.push(format!("{pairs} overlap(s)"));
                                }
                                if off > 0 {
                                    w.push(format!("{off} part(s) off the sheet"));
                                }
                                ui.label(RichText::new(format!("Sheet {}: {}", k + 1, w.join(", "))).strong().color(Color32::from_rgb(185, 28, 28)));
                            }
                        }
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if problems.is_empty() {
                                let old = std::fs::read_dir(&dir).map_or(0, |rd| rd.flatten().filter(|e| e.path().is_file()).count());
                                let label = match (old > 0, ticked.len()) {
                                    (_, t) if t > 0 => format!("Export {n} program(s) for {} machines", t + 1),
                                    (true, _) => format!("Replace and export {n} program(s)"),
                                    (false, _) => format!("Export {n} program(s)"),
                                };
                                let primary = egui::Button::new(RichText::new(label).color(Color32::WHITE).strong()).fill(th().accent);
                                if ui.add(primary).clicked() {
                                    self.export_with(&ticked);
                                    close = true;
                                }
                            } else {
                                let first = problems.first().map_or(0, |s| s.0);
                                let primary = egui::Button::new(RichText::new(format!("Show sheet {}", first + 1)).color(Color32::WHITE).strong()).fill(th().accent);
                                if ui.add(primary).on_hover_text("Go to the sheet; the parts in the way are drawn in red").clicked() {
                                    self.workspace = Workspace::Nesting;
                                    self.select_sheet(first);
                                    close = true;
                                }
                                if ui.button("Export anyway").clicked() {
                                    self.export_with(&ticked);
                                    close = true;
                                }
                            }
                            // EXP3: the machine's own programs folder, when it has one.
                            let own = self.cfg.machines.get(self.cfg.active_machine).is_some_and(|m| !m.programs_dir.trim().is_empty());
                            if ui.button("Change folder…").on_hover_text(if own { "Settings → Machines: this machine's programs folder" } else { "Settings → Folders" }).clicked() {
                                if own {
                                    self.setui.sel_machine = self.cfg.active_machine;
                                    self.open_settings(settings_ui::Section::Machines);
                                } else {
                                    self.open_settings(settings_ui::Section::Folders);
                                }
                                close = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    }
                    Pending::TechPartEdit { idx, draft } => {
                        let (name, items) = self.parts.get(idx).map_or((String::new(), 0), |q| (q.name.clone(), q.tech.map_or(0, |t| t.items())));
                        ui.label(format!(
                            "'{name}' has its own technology ({items} item(s): lead-ins, micro-joints, cut quality). Changing its shape or size can't keep it."
                        ));
                        ui.add_space(6.0);
                        ui.checkbox(&mut self.tech_all_box, "Use this answer for the rest of this session");
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            let primary = egui::Button::new(RichText::new("Edit a copy").color(Color32::WHITE).strong()).fill(th().accent);
                            if ui.add(primary).on_hover_text("Keep this part and its technology; make a copy with the change").clicked() {
                                if self.tech_all_box {
                                    self.tech_answer_all = Some(TechChoice::EditCopy);
                                }
                                self.apply_tech_choice(idx, draft.clone(), TechChoice::EditCopy);
                                close = true;
                            }
                            if ui.button("Edit this part (remove technology)").clicked() {
                                if self.tech_all_box {
                                    self.tech_answer_all = Some(TechChoice::EditPart);
                                }
                                self.apply_tech_choice(idx, draft.clone(), TechChoice::EditPart);
                                close = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    }
                    Pending::NestedSpecChange { new_spec } => {
                        ui.label(format!(
                            "{} sheet(s) are nested in {}. Changing to {} affects them.",
                            self.result.sheets.len(),
                            self.mat_thk(self.spec.material, self.spec.thickness),
                            self.mat_thk(new_spec.material, new_spec.thickness)
                        ));
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("Keep nested sheets").clicked() {
                                self.apply_spec_change(new_spec.clone(), false);
                                close = true;
                            }
                            let primary = egui::Button::new(RichText::new("Remove nestings").color(Color32::WHITE).strong()).fill(th().accent);
                            if ui.add(primary).on_hover_text("Take the parts off the sheets and free the sheets").clicked() {
                                self.apply_spec_change(new_spec.clone(), true);
                                close = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    }
                    Pending::JobFileChanged { .. } => {}
                }
            });
        if close {
            self.pending = None;
            self.tech_all_box = false;
        } else if rebuild {
            // The machine picked, its technology in use, before the review is built again.
            self.apply_settings(false);
            self.export_checked();
        } else if let (Some((i, on)), Some(Pending::ExportReview { also, .. })) = (flip, self.pending.as_mut()) {
            if let Some(r) = also.get_mut(i) {
                r.ticked = on && r.ready.is_ok();
            }
            // EXP4: two machines can't write into one folder: ticking one
            // unticks another that shares it.
            if let Some(dir) = also.get(i).filter(|r| r.ticked).map(|r| r.dir.clone()) {
                for (j, r) in also.iter_mut().enumerate() {
                    if j != i && engine::overflow::same_folder(&r.dir, &dir) {
                        r.ticked = false;
                    }
                }
            }
            self.review_earlier();
        }
    }

    /// Furthest X and Y any part reaches on a sheet.
    pub fn sheet_extent(&self, i: usize) -> (f64, f64) {
        self.result.sheets.get(i).map_or((0.0, 0.0), |s| {
            (s.iter().map(|p| p.x + p.w).fold(0.0, f64::max), s.iter().map(|p| p.y + p.h).fold(0.0, f64::max))
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            parts: self.parts.clone(),
            spec: self.spec.clone(),
            result: self.result.clone(),
            cur_sheet: self.cur_sheet,
            stale: self.stale,
            cut_lines: self.cut_lines.clone(),
            manual_cuts: self.manual_cuts.clone(),
        }
    }

    /// Remember the current state so the next change can be undone.
    pub fn checkpoint(&mut self) {
        // Any other change ends a copy carried on the pointer (QR4); a
        // pattern goes along with the sheet as the change leaves it (HM21).
        if !self.pattern_on() {
            self.carry = None;
        }
        let s = self.snapshot();
        self.push_undo(s);
    }

    fn push_undo(&mut self, s: Snapshot) {
        self.undo.push(s);
        if self.undo.len() > 60 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn push_undo_coalesced(&mut self, before: Snapshot, now: f64) {
        if now - self.last_edit > 1.0 {
            self.push_undo(before);
        }
        self.last_edit = now;
    }

    fn restore(&mut self, s: Snapshot) {
        let mut result = s.result;
        // Remnants stored since are not rolled back: follow the stock (RN6).
        if result.formats.iter().any(|f| !f.made.is_empty()) {
            if let Ok(stock) = crate::stock::Stock::load(&self.library_dir()) {
                engine::stockflow::follow_stock(&stock, &mut result.formats);
            }
        }
        self.parts = s.parts;
        self.spec = s.spec;
        self.result = result;
        self.cur_sheet = s.cur_sheet;
        self.stale = s.stale;
        if self.cut_lines != s.cut_lines || self.manual_cuts != s.manual_cuts {
            self.mark_cam_changed();
        }
        self.cut_lines = s.cut_lines;
        self.manual_cuts = s.manual_cuts;
        self.sel_cut = None;
        self.sel_placed = None;
        self.drag = None;
        self.turn_drag = None;
        // A part on the pointer may be one the restored job doesn't have.
        self.ghost = None;
        if self.sel_part.is_some_and(|i| i >= self.parts.len()) {
            self.sel_part = None;
        }
        // A Break elements tool's clicks and marks were on the drawing
        // before; they go, but Cut several keeps its base while it is still
        // on a line (BE1).
        let base = self.dv.modify_picks.first().copied().filter(|_| self.dv.modify == Some(crate::drawing::Modify::CutSeveral));
        self.dv.modify_picks.clear();
        self.dv.modify_marks.clear();
        self.dv.modify_note = None;
        self.refresh();
        if let (Some(b), Some(i)) = (base, self.shown_part()) {
            if crate::engine::breakelems::element_at(&self.parts[i], b, 1e-6, &[]).is_some() {
                self.dv.modify_picks.push(b);
            }
        }
    }

    /// With a Break elements tool in hand, its note says the drawing was
    /// put back (or forward), in place of what the last click did.
    fn modify_note_after(&mut self, what: &str) {
        if self.dv.modify.is_some() {
            self.dv.modify_note = Some(crate::drawing::ModifyNote { text: what.into(), ok: true, spot: None });
        }
    }

    pub fn undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            let cur = self.snapshot();
            self.redo.push(cur);
            self.restore(s);
            self.result_card = None;
            self.status = "Undone.".into();
            self.modify_note_after("Undone: the drawing is back as it was. Ctrl+Y puts the change back.");
        }
    }

    pub fn redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            let cur = self.snapshot();
            self.undo.push(cur);
            self.restore(s);
            self.result_card = None;
            self.status = "Redone.".into();
            self.modify_note_after("Redone.");
        }
    }

    /// Slide every part on the current sheet left, then down, as far as it
    /// goes without touching another part — leaving one large remnant.
    pub fn compact(&mut self) {
        let ci = self.cur_sheet;
        if ci >= self.result.sheets.len() {
            return;
        }
        if self.result.format(ci, &self.spec).outline.is_some() {
            self.status = engine::COMPACT_REMNANT.into();
            return;
        }
        self.checkpoint();
        let before = self.sheet_usage(ci).2;
        // Follow the real contours, not the boxes (see MECHANICS.md "True-shape").
        let cs = self.sheet_spec(ci);
        nest::compact_contours(&self.parts, &mut self.result.sheets[ci], &cs);
        self.refresh();
        let after = self.sheet_usage(ci).2;
        let u = self.unit();
        self.status = format!("Compacted — remnant now starts at X {} ({} gained).", u.len(after, 0), u.len((before - after).max(0.0), 0));
    }

    /// Place a copy of the selected parts in the first free spot (left first),
    /// keeping their layout; it uses pieces still to place first (HM10).
    pub fn duplicate_selected(&mut self) {
        let sel = self.selection();
        let ci = self.cur_sheet;
        if sel.is_empty() || ci >= self.result.sheets.len() {
            return;
        }
        let f = self.result.format(ci, &self.spec);
        match engine::arrange::copy_spot(&self.result.sheets[ci], &sel, &self.spec, &f) {
            Some(d) => {
                self.checkpoint();
                let n = self.result.sheets[ci].len();
                let copies = engine::arrange::copies(&self.result.sheets[ci], &sel, d);
                let extras = engine::pattern::put(&mut self.parts, &mut self.result.sheets, &self.result.formats, ci, &copies);
                self.set_selection((n..self.result.sheets[ci].len()).collect());
                self.refresh();
                self.status = self.copied_status(n, extras);
            }
            None => self.status = "No free spot on this sheet for a copy.".into(),
        }
    }

    /// Write every sheet's machine program plus a CSV parts report.
    /// BLM settings with this job's name, material and thickness filled in.
    pub fn blm_now(&self) -> blm::BlmSettings {
        engine::blm_for(&self.spec, &self.blm, &self.job_name)
    }

    /// The program file name shown and written for a sheet.
    pub fn cnc_name(&self, sheet: usize) -> String {
        if self.blm.on {
            format!("{}.zip", blm::nest_name(&self.job_name, sheet))
        } else {
            self.post.file_name(&self.job_name, sheet)
        }
    }

    /// Where Export CNC writes, for its tooltip: all sheets, the folder, the format.
    pub fn export_dir(&self) -> PathBuf {
        // EXP3: the machine's own programs folder, else Settings → Folders.
        engine::overflow::programs_base(&self.cfg, self.cfg.active_machine)
    }

    /// The open job as an export writes it (EXP4).
    pub fn job_ref(&self) -> engine::overflow::JobRef<'_> {
        engine::overflow::JobRef { name: &self.job_name, parts: &self.parts, sheets: &self.result.sheets, formats: &self.result.formats, spec: &self.spec, manual: &self.manual_cuts, cuts: &self.cut_lines }
    }

    /// EXP4: the Export review's other machines: each checked with its own
    /// technology (read now, once per review; the machine in use is back
    /// after), ticked again when it was and still can take the nest.
    fn also_rows(&self, ticked: &[usize]) -> Vec<AlsoRow> {
        let here = self.export_dir().join(engine::safe_name(&self.job_name));
        let active = self.cfg.active_machine;
        (0..self.cfg.machines.len())
            .filter(|&k| k != active)
            .map(|k| {
                let dir = engine::overflow::job_dir(&self.cfg, k, &self.job_name);
                let ready = if engine::overflow::same_folder(&dir, &here) {
                    Err(format!("it writes into the same folder as {} ({}), where each export removes the other's programs. Give it its own programs folder in Settings → Machines", engine::overflow::machine_name(&self.cfg, active), here.display()))
                } else {
                    engine::overflow::check(&self.cfg, k, self.job_ref())
                };
                AlsoRow { machine: k, dir, ticked: ticked.contains(&k) && ready.is_ok(), ready }
            })
            .collect()
    }

    /// EXP6: the review's list of this job's programs in other machines'
    /// folders that the export, as ticked now, would remove.
    fn review_earlier(&mut self) {
        let Some(Pending::ExportReview { also, .. }) = self.pending.as_ref() else { return };
        let mut written = vec![self.export_dir().join(engine::safe_name(&self.job_name))];
        written.extend(also.iter().filter(|r| r.ticked).map(|r| r.dir.clone()));
        let list = engine::overflow::earlier_elsewhere(&self.cfg, &self.job_name, &self.spec, &written);
        if let Some(Pending::ExportReview { earlier, .. }) = self.pending.as_mut() {
            *earlier = list;
        }
    }

    /// Export CNC from the ribbon: a review first — the sheets, the machine and
    /// output, the folder and file names — and every sheet checked, so a
    /// program that cuts parts into each other is never written unnoticed.
    pub fn export_checked(&mut self) {
        let mut bad = vec![];
        for (k, sheet) in self.result.sheets.iter().enumerate() {
            let spec = self.sheet_spec(k);
            let pairs = engine::overlapping(&self.parts, sheet, &spec).len();
            let off = sheet.iter().filter(|p| !spec.fits(p)).count();
            if pairs > 0 || off > 0 {
                bad.push((k, pairs, off));
            }
        }
        if self.result.sheets.is_empty() {
            self.status = "Nothing to export — nest first.".into();
            return;
        }
        // Verify machining on every sheet, so the review can say it passed.
        self.refresh();
        let need = self.safe_need();
        let checks: Vec<(usize, usize)> = (0..self.result.sheets.len())
            .filter_map(|k| {
                let (sheet, plan) = (self.result.sheets.get(k)?, self.plans.get(k)?);
                let n = engine::verify_sheet(&self.parts, sheet, &self.sheet_spec(k), plan, self.nverify.check_safe.then_some(need)).len();
                (n > 0).then_some((k, n))
            })
            .collect();
        // EXP4: machines ticked before (a machine picked in the review) stay ticked.
        // A machine whose folder still holds this job's programs starts ticked,
        // so exporting again doesn't take them away unasked (EXP6).
        let ticked: Vec<usize> = match &self.pending {
            Some(Pending::ExportReview { also, .. }) => also.iter().filter(|r| r.ticked).map(|r| r.machine).collect(),
            _ => {
                let here = self.export_dir().join(engine::safe_name(&self.job_name));
                engine::overflow::earlier_elsewhere(&self.cfg, &self.job_name, &self.spec, &[here]).into_iter().filter(|e| !e.files.is_empty()).flat_map(|e| e.machines).collect()
            }
        };
        let mut also = self.also_rows(&ticked);
        // Never two ticked on one folder.
        for i in 0..also.len() {
            if also[i].ticked && also[..i].iter().any(|r| r.ticked && engine::overflow::same_folder(&r.dir, &also[i].dir)) {
                also[i].ticked = false;
            }
        }
        self.pending = Some(Pending::ExportReview { problems: bad, checks, also, earlier: vec![] });
        self.review_earlier();
    }

    pub fn export(&mut self) {
        self.export_with(&[]);
    }

    /// Export CNC for the machine in use and, EXP4, the same sheets for the
    /// other machines `also` (each into its own programs folder, checked with
    /// its own technology first); then, EXP6, this job's programs left in
    /// any other machine's folder go.
    pub fn export_with(&mut self, also: &[usize]) {
        // Nothing nested: engine::export says so (and removes the job's old programs).
        // EXP3: the machine's programs folder, else Settings → Folders, or
        // Documents\Sheet Nesting.
        let base = self.export_dir();
        // UP16: in a trial nothing reaches the shop folder, programs neither.
        let refused = self.versions().trial_of.and_then(|shop| shared::trial_may_write(&base, &shop, "The programs").err());
        let res = match refused {
            Some(e) => Err(e),
            None => {
                // EXP2: post files that couldn't be read are read again first, so a
                // shop folder that is back is picked up; still missing, export refuses.
                if !self.post.missing.is_empty() {
                    self.apply_settings(true);
                }
                // Every sheet's program (or BLM LC5 set) and the CSV parts report.
                let blm = self.blm_now();
                let marks: Vec<Vec<Vec<nest::P>>> = (0..self.result.sheets.len()).map(|k| self.remnant_marks(k)).collect();
                engine::export(&base, &self.job_name, &self.parts, &self.result.sheets, &self.result.formats, &self.plans, &self.spec, &self.cut_lines, &marks, &self.post, Some(&blm), self.cfg.stock.line.edge_offset)
            }
        };
        let n = self.result.sheets.len();
        let mut wrote = false;
        let status = match res {
            Ok(ex) => {
                // GS16: the message names the material, thickness and gas.
                let names = ex.files.iter().filter_map(|f| f.file_name().map(|n| n.to_string_lossy().to_string())).collect();
                let what = engine::programs_written(n, &self.spec, &self.blm, self.unit());
                self.last_export = Some((ex.dir.clone(), names, ex.warning.clone(), what));
                wrote = true;
                engine::export_note(&ex, n, &self.spec, &self.blm, self.unit())
            }
            Err(e) => {
                // A refused export takes the job's programs with it (BL39).
                self.last_export = None;
                e
            }
        };
        // EXP4: the machines ticked are tried even when this one failed.
        let (mut lines, first) = if n > 0 { self.export_also(base.join(engine::safe_name(&self.job_name)), wrote, also) } else { (vec![], None) };
        self.status = lines.iter().fold(status.clone(), |s, l| format!("{s} {}", l.0));
        // A machine written while this one's export stopped still gets the
        // card, with this one's reason first.
        if let (false, Some((dir, files, warning))) = (wrote, first) {
            let what = engine::programs_written(n, &self.spec, &self.blm, self.unit());
            self.last_export = Some((dir, files, warning, what));
            lines.insert(0, (format!("Not written for {}: {status}", engine::overflow::machine_name(&self.cfg, self.cfg.active_machine)), false));
        }
        self.export_also_lines = lines;
    }

    /// EXP4, EXP6: after the machine in use's export into `here` (`wrote`:
    /// it went), the same sheets for each of `also`, then, when any machine
    /// was written, this job's programs left in the other machines' folders
    /// removed. What happened, a line each (true = as asked).
    /// With the first other machine written: its folder, file names and warning.
    #[allow(clippy::type_complexity)]
    fn export_also(&self, here: PathBuf, wrote: bool, also: &[usize]) -> (Vec<(String, bool)>, Option<(PathBuf, Vec<String>, Option<String>)>) {
        use engine::overflow;
        let mut said: Vec<(String, bool)> = vec![];
        let mut first = None;
        // The machine in use's folder is cleaned here only when its export
        // stopped (a post file it can't read is said, EXP2): export did the rest.
        let mut written = if wrote { vec![here] } else { vec![] };
        let mut names = if wrote { vec![overflow::machine_name(&self.cfg, self.cfg.active_machine)] } else { vec![] };
        let trial = self.versions().trial_of;
        let n = self.result.sheets.len();
        for &k in also {
            let name = overflow::machine_name(&self.cfg, k);
            let base = overflow::programs_base(&self.cfg, k);
            let dir = base.join(engine::safe_name(&self.job_name));
            if written.iter().any(|w| overflow::same_folder(w, &dir)) {
                said.push((format!("Not written for {name}: it writes into the same folder as another machine here ({}).", dir.display()), false));
                continue;
            }
            // UP16: in a trial nothing reaches the shop folder (its folder and its job folder judged).
            if let Some(e) = trial.as_ref().and_then(|shop| shared::trial_may_write(&base, shop, "The programs").and_then(|_| shared::trial_may_write(&dir, shop, "The programs")).err()) {
                said.push((format!("Not written for {name}: {e}"), false));
                continue;
            }
            let marks = || (0..n).map(|i| self.remnant_marks(i)).collect();
            match overflow::write(&self.cfg, k, self.job_ref(), &base, &marks, self.cfg.stock.line.edge_offset) {
                Ok((ex, blm)) => {
                    said.push((format!("{name}: {}", engine::export_note(&ex, n, &self.spec, &blm, self.unit())), true));
                    if first.is_none() {
                        let files = ex.files.iter().filter_map(|f| f.file_name().map(|n| n.to_string_lossy().to_string())).collect();
                        first = Some((ex.dir.clone(), files, ex.warning.clone()));
                    }
                    written.push(ex.dir);
                    names.push(name);
                }
                Err(e) => said.push((format!("Not written for {name}: {e}."), false)),
            }
        }
        let once = overflow::cut_once_note(&names);
        if !once.is_empty() {
            said.push((once.trim().to_string(), false));
        }
        if names.is_empty() {
            return (said, first);
        }
        let earlier: Vec<_> = overflow::earlier_elsewhere(&self.cfg, &self.job_name, &self.spec, &written)
            .into_iter()
            .filter(|e| trial.as_ref().is_none_or(|shop| shared::trial_may_write(&e.dir, shop, "The programs").is_ok()))
            .collect();
        let gone = overflow::remove_earlier(&self.cfg, &earlier);
        if !gone.is_empty() {
            let ok = earlier.iter().all(|e| e.unread.is_none()) && !gone.contains("couldn't");
            said.push((gone.trim().to_string(), ok));
        }
        (said, first)
    }

    pub fn run_nesting(&mut self) {
        self.checkpoint();
        self.anim_pending = true;
        // Groups in their own zones; else from the sheet stock when it has
        // sheets for this job, else the job's sheet size; then contour
        // compaction on rectangular sheets.
        // Hand work that a new nesting replaces: said in the status line, so
        // nobody loses it without knowing (Ctrl+Z brings it back).
        let lost_order = self.manual_cuts.iter().filter(|s| !s.is_empty()).count();
        let lost_lines: usize = self.cut_lines.iter().map(|s| s.len()).sum();
        // JS4: the job's sheet list as this nesting reads it (its own
        // remnants are those of the nesting it replaces).
        let list_notes = engine::stockflow::list_slots(&self.spec, Some(&self.stock_use())).1;
        self.result = engine::nest_all(&self.parts, &self.spec, self.group_nest, Some(&self.stock_use()), self.cfg.stock.equal_sheets);
        self.manual_cuts.clear();
        self.ssim = None;
        self.cut_lines.clear();
        if self.cfg.stock.lines_after_nesting {
            self.auto_lines_all_sheets();
        }
        self.mark_cam_changed();
        self.sel_cut = None;
        self.cur_sheet = 0;
        self.sel_placed = None;
        self.drag = None;
        self.turn_drag = None;
        self.stale = false;
        self.refresh();
        let need: u32 = self.parts.iter().map(|p| p.qty).sum();
        let placed = self.total_placed();
        self.status = if placed as u32 == need {
            format!("Nesting done: {} parts on {} — level {}, best of {} methods.", placed, engine::sheets_text(&self.result.formats, self.result.sheets.len()), self.spec.nest_level().name, self.result.tried)
        } else {
            format!("{} of {} parts nested — some are larger than the sheet.", placed, need)
        };
        // From the stock, but the Library can't be reached: said so (LB11).
        if !(self.group_nest && nest::part_groups(&self.parts).len() > 1) {
            if let Some(n) = engine::stockflow::stock_unreachable_note(&self.stock_use()) {
                self.status += &n;
            }
            // JS4: a line of the job's sheet list that couldn't be used.
            for n in list_notes {
                self.status += &format!(" {n}");
            }
        } else if !self.spec.sheet_list.is_empty() {
            self.status += " The job's sheet list wasn't used: groups nest in their own zones on the job's sheet size.";
        }
        let mut lost = vec![];
        if lost_order > 0 {
            lost.push(format!("the hand-picked cut order on {lost_order} sheet(s)"));
        }
        if lost_lines > 0 {
            lost.push(format!("{lost_lines} cut / remnant line(s)"));
        }
        if !lost.is_empty() {
            self.status += &format!(" Removed {} — Ctrl+Z brings them back.", lost.join(" and "));
        }
    }

    /// Nest only the sheet on screen again (RN1): the parts already on it, on
    /// the same sheet; the other sheets keep their layout. Refused, sheet
    /// unchanged, when they don't all fit back (RN2).
    pub fn renest_sheet(&mut self) {
        let ci = self.cur_sheet;
        if ci >= self.result.sheets.len() {
            return;
        }
        let f = self.result.format(ci, &self.spec);
        match engine::renest_sheet(&self.parts, &self.spec, &self.result.sheets[ci], &f, self.group_nest) {
            Ok(placed) => {
                self.checkpoint();
                self.anim_pending = true;
                let before = self.sheet_usage(ci).2;
                self.result.sheets[ci] = placed;
                self.result.normalise(&self.spec);
                engine::stockflow::remnants_outdated(&mut self.result.formats[ci]);
                if let Some(l) = self.cut_lines.get_mut(ci) {
                    l.clear();
                }
                if let Some(m) = self.manual_cuts.get_mut(ci) {
                    m.clear();
                }
                self.ssim = None;
                self.sel_cut = None;
                self.sel_placed = None;
                self.drag = None;
                self.turn_drag = None;
                if self.cfg.stock.lines_after_nesting {
                    self.auto_lines_sheet(ci);
                }
                self.mark_cam_changed();
                self.refresh();
                let (usage, _, after) = self.sheet_usage(ci);
                let u = self.unit();
                self.status = format!(
                    "Sheet {} nested again: its {} parts, usage {:.1} %, remnant from X {} (was {}). Other sheets unchanged.",
                    ci + 1,
                    self.result.sheets[ci].len(),
                    usage,
                    u.len(after, 0),
                    u.num_dp(before, 0)
                );
            }
            Err(e) => self.status = e,
        }
    }

    /// Every piece nested, each sheet as often as it is cut (SR1).
    pub fn total_placed(&self) -> usize {
        nest::pieces_on(&self.result.sheets, &self.result.formats)
    }

    /// How many of a part are nested, each sheet as often as it is cut (SR1).
    pub fn placed_of(&self, part: usize) -> usize {
        nest::placed_on(&self.result.sheets, &self.result.formats, part)
    }

    pub fn refresh(&mut self) {
        // Empty sheets go, except one just added with Add nesting while it is shown.
        let keep = self.empty_sheet_kept.filter(|&k| k == self.cur_sheet);
        // Per-sheet cut lines and hand cuts go with their sheet, so later
        // sheets don't pick up their neighbour's lines.
        if engine::drop_empty_sheets(&mut self.result.sheets, &mut self.result.formats, &self.spec, &mut self.cut_lines, &mut self.manual_cuts, keep) {
            self.mark_cam_changed();
        }
        if keep.is_none() {
            self.empty_sheet_kept = None;
        }
        if self.cur_sheet >= self.result.sheets.len() {
            self.cur_sheet = self.result.sheets.len().saturating_sub(1);
        }
        self.plans = engine::plans(&self.parts, &self.result.sheets, &self.result.formats, &self.spec, &self.manual_cuts);
        self.overlaps = self.result.sheets.get(self.cur_sheet).map_or(vec![], |s| engine::overlapping(&self.parts, s, &self.cur_spec()));
        self.program = match (self.result.sheets.get(self.cur_sheet), self.plans.get(self.cur_sheet)) {
            // EXP2: no program while a post file can't be read.
            (Some(_), Some(_)) if self.post.refusal().is_some() => format!("( NO PROGRAM: {} )", self.post.refusal().unwrap_or_default().replace(['(', ')'], "")),
            (Some(s), Some(pl)) => engine::program(self.cur_sheet + 1, &self.parts, s, &self.cur_spec(), pl, self.cut_list(), &self.remnant_marks(self.cur_sheet), &self.post, self.cfg.stock.line.edge_offset),
            _ => "( NO SHEET NESTED )".into(),
        };
    }

    pub fn select_sheet(&mut self, i: usize) {
        if i < self.result.sheets.len() && i != self.cur_sheet {
            // A copy on the pointer belongs to the sheet it was made on; a
            // pattern goes along to the next (HM21).
            if !self.pattern_on() {
                self.carry = None;
            }
            self.cur_sheet = i;
            self.ssim = None;
            self.sel_placed = None;
            self.refresh();
        }
    }

    pub fn clear(&mut self) {
        self.checkpoint();
        self.clear_sheets();
        self.status = "Sheets cleared.".into();
    }

    fn clear_sheets(&mut self) {
        self.result = NestResult::default();
        self.cut_lines.clear();
        self.sel_cut = None;
        self.sel_placed = None;
        self.drag = None;
        self.turn_drag = None;
        self.refresh();
    }

    pub fn new_job(&mut self) {
        // UP9: a job read-only here is never emptied in place (Undo would
        // bring the newer version's job back as this one's to save): a new
        // job of its own starts instead, with its own undo.
        if self.read_only().is_some() {
            let week = self.jobs.get(self.cur_job).map_or_else(String::new, |j| j.week.clone());
            return self.new_job_in_week(week);
        }
        self.checkpoint();
        self.parts.clear();
        self.sel_part = None;
        self.job_name = "New job".into();
        self.spec = engine::with_prefs(&self.spec, &self.cfg);
        self.clear_sheets();
        self.status = "New job — add parts, then press Nest all.".into();
    }

    pub fn add_part(&mut self, shape: Shape) {
        self.checkpoint();
        self.parts.push(engine::shape_part(shape, self.parts.len() + 1, self.color_seq));
        self.color_seq += 1;
        self.sel_part = Some(self.parts.len() - 1);
        self.stale = true;
    }

    pub fn remove_part(&mut self, idx: usize) {
        self.checkpoint();
        engine::remove_part(&mut self.parts, &mut self.result.sheets, idx);
        // Placements were renumbered: hand picks would land on other parts
        // (as the agents' remove-part does); the part on the pointer may be gone.
        self.manual_cuts.iter_mut().for_each(Vec::clear);
        self.ghost = None;
        self.sel_part = if self.parts.is_empty() { None } else { Some(idx.min(self.parts.len() - 1)) };
        self.sel_placed = None;
        self.refresh();
        self.stale = true;
    }

    /// R: one part turns to its next allowed quarter turn, several turn 90°
    /// together — about their centre, moved to the nearest free spot when
    /// they'd land on another part (HM5).
    pub fn rotate_selected(&mut self) {
        self.step_turn_selected(false);
    }

    /// The quick ring's Turn back: one part to its previous allowed turn,
    /// several 90° the other way (QR3).
    pub fn turn_back_selected(&mut self) {
        self.step_turn_selected(true);
    }

    /// Whether Turn (or Turn back) can turn the selection: one part needs
    /// another allowed turn that way, several must all be allowed 90° (QR2).
    pub fn can_step_turn(&self, back: bool) -> bool {
        let sel = self.selection();
        let Some(sheet) = self.result.sheets.get(self.cur_sheet) else { return false };
        match sel[..] {
            [] => false,
            [i] => engine::step_turn(&self.parts[sheet[i].part], sheet[i], back).is_ok(),
            _ => engine::arrange::can_turn(&self.parts, sheet, &sel, if back { 3 } else { 1 }),
        }
    }

    fn step_turn_selected(&mut self, back: bool) {
        let sel = self.selection();
        let s = self.cur_sheet;
        if sel.is_empty() || s >= self.result.sheets.len() {
            return;
        }
        let q = if let [i] = sel[..] {
            let old = self.result.sheets[s][i];
            match engine::step_turn(&self.parts[old.part], old, back) {
                Ok(c) => (c.turns + 4 - old.turns) % 4,
                Err(e) => {
                    self.status = e.into();
                    return;
                }
            }
        } else if back {
            3
        } else {
            1
        };
        self.turn_selected(q);
    }

    pub fn remove_selected(&mut self) {
        let sel = self.selection();
        if sel.is_empty() || self.cur_sheet >= self.result.sheets.len() {
            return;
        }
        self.checkpoint();
        for &i in sel.iter().rev() {
            self.result.sheets[self.cur_sheet].remove(i);
        }
        // The sheet's placements were renumbered: its hand picks would land on
        // other parts (as the agents' operations do, sheet_changed).
        if let Some(m) = self.manual_cuts.get_mut(self.cur_sheet) {
            m.clear();
        }
        self.set_selection(vec![]);
        self.refresh();
        self.status = if sel.len() == 1 { "Part taken off the sheet. Press Nest all to place it again.".into() } else { format!("{} parts taken off the sheet.", sel.len()) };
    }

    /// (usage of whole sheet %, usage up to the remnant cut line %, X of that cut line)
    pub fn sheet_usage(&self, i: usize) -> (f64, f64, f64) {
        let Some(s) = self.result.sheets.get(i) else { return (0.0, 0.0, 0.0) };
        engine::sheet_usage(&self.parts, s, &self.spec, &self.result.format(i, &self.spec))
    }

    /// The job's settings with sheet `i`'s own size.
    pub fn sheet_spec(&self, i: usize) -> SheetSpec {
        self.spec.for_format(&self.result.format(i, &self.spec))
    }

    /// The real area of all sheets in mm² (remnants count by their shape),
    /// each sheet as often as it is cut (SR1).
    pub fn total_sheet_area(&self) -> f64 {
        (0..self.result.sheets.len()).map(|i| self.result.format(i, &self.spec).area_mm2() * self.runs(i) as f64).sum()
    }

    /// How many times sheet `i` is cut (SR1).
    pub fn runs(&self, i: usize) -> usize {
        nest::runs_at(&self.result.formats, i)
    }

    /// The sheets the job is cut on: every sheet as often as it is cut (SR1).
    pub fn sheets_cut(&self) -> usize {
        nest::sheets_cut(&self.result.formats, self.result.sheets.len())
    }

    /// The net area of every part nested, each sheet as often as it is cut.
    pub fn parts_area_cut(&self) -> f64 {
        self.result.sheets.iter().enumerate().map(|(k, s)| s.iter().map(|p| self.parts[p.part].net_area()).sum::<f64>() * self.runs(k) as f64).sum()
    }

    /// The machine time of the whole job: each sheet's as often as it is cut.
    pub fn job_seconds(&self) -> f64 {
        self.plans.iter().enumerate().map(|(k, p)| p.seconds * self.runs(k) as f64).sum()
    }

    /// A sheet's size as shown in lists, with its stock reference if it has one.
    pub fn sheet_label(&self, i: usize, decimals: usize) -> String {
        let f = self.result.format(i, &self.spec);
        let k = self.unit().factor();
        let size = format!("{:.d$}x{:.d$}", f.w / k, f.h / k, d = decimals);
        // SR17: a sheet cut several times says how many.
        let times = if f.runs() > 1 { format!(" (×{})", f.runs()) } else { String::new() };
        match (f.remnant, f.stock_ref.is_empty()) {
            (true, _) => format!("{size} remnant {}{times}", f.stock_ref),
            (false, false) => format!("{size} [{}]{times}", f.stock_ref),
            _ => format!("{size}{times}"),
        }
    }

    /// The unit lengths are shown and typed in (Settings → Nesting → Units;
    /// the app keeps mm inside).
    pub fn unit(&self) -> units::LenUnit {
        self.cfg.nesting.units
    }

    /// The name of the shop machine the job is cut on (Settings → Machines &
    /// output, the one in use; MK1).
    pub fn machine_name(&self) -> String {
        self.cfg.machines.get(self.cfg.active_machine).map_or_else(|| self.spec.machine.label().to_string(), |m| m.name.clone())
    }

    /// "Mild steel 3 mm" / "Mild steel 0.1875 in (3/16")".
    pub fn mat_thk(&self, material: usize, thickness: f64) -> String {
        format!("{} {}", crate::engine::material::nearest_name(material), self.unit().thickness(thickness))
    }

    /// Is a part inside sheet `i`: within its margin and, on a remnant, inside
    /// its real shape?
    pub fn fits_sheet(&self, c: &Placed, i: usize) -> bool {
        engine::part_fits_format(&self.parts, &self.spec, &self.result.format(i, &self.spec), c)
    }

    /// The settings for the sheet on screen.
    pub fn cur_spec(&self) -> SheetSpec {
        self.sheet_spec(self.cur_sheet)
    }

    fn canvas(&mut self, ui: &mut egui::Ui) {
        if self.stale {
            egui::Frame::new()
                .fill(th().warn_bg)
                .stroke(Stroke::new(1.0, with_alpha(th().warn_fg, 90)))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(10, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("⚠ Parts or sheet changed since the last nest").color(th().warn_fg));
                        let b = egui::Button::new(RichText::new("Nest all again").color(Color32::WHITE).strong()).fill(th().accent).corner_radius(CornerRadius::same(6));
                        if ui.add(b).clicked() {
                            self.run_nesting();
                        }
                    });
                });
            ui.add_space(3.0);
        }
        let now = ui.input(|i| i.time);
        if self.anim_pending {
            self.anim_pending = false;
            // No animation for the automatic screenshot, so it shows the full sheet.
            self.anim_start = if self.shot.is_some() { now - 100.0 } else { now };
        }
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        // Horizontal navy-to-blue gradient background.
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(rect.left_top(), th().canvas_a);
        mesh.colored_vertex(rect.right_top(), th().canvas_b);
        mesh.colored_vertex(rect.right_bottom(), th().canvas_b);
        mesh.colored_vertex(rect.left_bottom(), th().canvas_a);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(EShape::mesh(mesh));

        let view = rect;
        let cspec = self.cur_spec();
        let cformat = self.result.format(self.cur_sheet, &self.spec);
        let fit = ((view.width() - 40.0) / cspec.w as f32).min((view.height() - 24.0) / cspec.h as f32).max(0.0001);
        // Show next problem: zoom in a little and put that part in the middle.
        if let Some(i) = self.focus_placed.take() {
            if let Some(p) = self.result.sheets.get(self.cur_sheet).and_then(|sh| sh.get(i)) {
                self.zoom = self.zoom.max(2.5);
                let s = fit * self.zoom;
                let (cx, cy) = ((p.x + p.w / 2.0) as f32, (p.y + p.h / 2.0) as f32);
                self.pan = Vec2::new(cspec.w as f32 * s / 2.0 - cx * s, cy * s - cspec.h as f32 * s / 2.0);
                self.sel_placed = Some(i);
                self.sel_more.clear();
            }
        }
        let mut s = fit * self.zoom;
        let origin_for = |s: f32, pan: Vec2, spec: &SheetSpec| view.center() + pan + Vec2::new(-(spec.w as f32) * s / 2.0, spec.h as f32 * s / 2.0);
        let mut o = origin_for(s, self.pan, &cspec);

        if let Some(hp) = resp.hover_pos() {
            let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                let wx = (hp.x - o.x) / s;
                let wy = (o.y - hp.y) / s;
                self.zoom = (self.zoom * partview::wheel_factor(scroll)).clamp(0.3, 40.0);
                s = fit * self.zoom;
                let o2 = origin_for(s, self.pan, &cspec);
                self.pan += hp - Pos2::new(o2.x + wx * s, o2.y - wy * s);
                o = origin_for(s, self.pan, &cspec);
            }
        }
        let to_s = |p: P| Pos2::new(o.x + p[0] as f32 * s, o.y - p[1] as f32 * s);
        let to_w = |q: Pos2| -> P { [((q.x - o.x) / s) as f64, ((o.y - q.y) / s) as f64] };

        self.cursor_mm = resp.hover_pos().map(to_w);
        let sheet_idx = self.cur_sheet;
        let has_sheet = sheet_idx < self.result.sheets.len();
        // Right-click (or S): the radial tool menu, for the part under the pointer.
        let ghosting = self.ghost.is_some() || self.cut_tool.is_some();
        if let Some(hp) = resp.hover_pos().filter(|_| !ghosting) {
            if self.radial_trigger(ui) {
                // On a part: the part ring for the selection; on empty sheet: the sheet ring (QR1).
                let w = to_w(hp);
                let under = self.result.sheets.get(sheet_idx).and_then(|sh| sh.iter().rposition(|p| nest::placed_contains(&self.parts, p, w)));
                if let Some(i) = under {
                    if !self.is_selected(i) {
                        self.set_selection(vec![i]);
                    }
                }
                self.open_radial(ui.ctx(), hp, if under.is_some() { radial::Ring::OnPart } else { radial::Ring::Sheet });
                if let Some(r) = self.radial.as_mut() {
                    r.at = Some(w);
                }
            }
        }

        if ghosting {
            // Dropping parts: clicks drop the ghost (see ghost.rs); dragging pans.
            if resp.dragged_by(egui::PointerButton::Primary) && self.ghost.is_none_or(|g| !g.dragging) {
                self.pan += resp.drag_delta();
            }
        } else if has_sheet {
            // Select, drag (snap and slide), Ctrl+drag copies, window, pan (arrange_ui.rs).
            self.sheet_pointer(ui, &resp, &to_w, s);
        } else if resp.dragged() {
            self.pan += resp.drag_delta();
        }

        let srect = Rect::from_two_pos(to_s([0.0, 0.0]), to_s([cspec.w, cspec.h]));
        match cformat.outline.as_ref().filter(|o| o.outer.len() >= 3) {
            // A remnant: its box dimmed, its real shape as the sheet.
            Some(ol) => {
                painter.rect_filled(srect, CornerRadius::ZERO, th().sheet.gamma_multiply(0.25));
                for r in region::free_rects(ol) {
                    painter.rect_filled(Rect::from_two_pos(to_s([r.x, r.y]), to_s([r.x + r.w, r.y + r.h])), CornerRadius::ZERO, th().sheet);
                }
                let edge = Stroke::new(1.5, th().text_dim);
                for ring in std::iter::once(&ol.outer).chain(ol.holes.iter()) {
                    let mut pts: Vec<Pos2> = ring.iter().map(|&p| to_s(p)).collect();
                    pts.extend(pts.first().copied());
                    painter.add(EShape::line(pts, edge));
                }
            }
            None => {
                painter.rect_filled(srect, CornerRadius::ZERO, th().sheet);
            }
        }
        if self.show_grid {
            let grid = Color32::from_rgba_unmultiplied(255, 255, 255, 18);
            let step = self.unit().grid_step();
            let mut x = step;
            while x < cspec.w {
                painter.line_segment([to_s([x, 0.0]), to_s([x, cspec.h])], Stroke::new(1.0, grid));
                x += step;
            }
            let mut y = step;
            while y < cspec.h {
                painter.line_segment([to_s([0.0, y]), to_s([cspec.w, y])], Stroke::new(1.0, grid));
                y += step;
            }
        }
        painter.rect_stroke(srect, CornerRadius::ZERO, Stroke::new(1.0, th().sheet_edge), egui::StrokeKind::Middle);

        self.hover_name = None;
        if has_sheet {
            let hover_w = resp.hover_pos().map(to_w);
            let sheet = &self.result.sheets[sheet_idx];
            let mt = crate::ct1::for_spec(&self.spec);
            let n = sheet.len().max(1) as f64;
            let stagger = (0.7 / n).min(0.03);
            let mut animating = false;
            // Selection glows are gathered into this slot, under every part, so
            // one part's glow never paints over the parts placed before it.
            let halo_slot = painter.add(EShape::Noop);
            let mut halos: Vec<EShape> = vec![];
            let machined = self.sheet_shows_machined();
            for (i, pl) in sheet.iter().enumerate() {
                let part = &self.parts[pl.part];
                let base = part_color(part.color);
                let selected = self.is_selected(i);
                let hovered = hover_w.is_some_and(|w| nest::placed_contains(&self.parts, pl, w));
                if hovered {
                    let u = self.cfg.nesting.units;
                    self.hover_name = Some(format!(
                        "{}  ·  #{}\n{}{}\nX {}   Y {}",
                        part.name,
                        i + 1,
                        u.size(pl.w, pl.h, 0),
                        &if pl.angle() == 0.0 && !pl.mirror { String::new() } else { format!("  ·  turned {}{}", nest::angle_text(pl.angle()), if pl.mirror { ", mirrored" } else { "" }) },
                        u.num_dp(pl.x, 0),
                        u.num_dp(pl.y, 0)
                    ));
                }
                // Parts drop onto the sheet one after another after a nest.
                let t = (((now - self.anim_start) - i as f64 * stagger) / 0.25).clamp(0.0, 1.0) as f32;
                if t < 1.0 {
                    animating = true;
                }
                if t <= 0.0 {
                    continue;
                }
                let ease = 1.0 - (1.0 - t) * (1.0 - t);
                let drop = (1.0 - ease) * 14.0;
                let fill = if hovered && !selected { base.gamma_multiply(1.2) } else { base };
                let fill = if t < 1.0 { fill.gamma_multiply(ease) } else { fill };
                let invalid = self.drag.as_ref().is_some_and(|d| !d.valid && d.sel.contains(&i)) || self.turn_drag.as_ref().is_some_and(|d| !d.valid && d.sel.contains(&i));
                let edge = if invalid {
                    Stroke::new(2.5, Color32::from_rgb(248, 80, 90))
                } else if selected {
                    Stroke::new(2.5, Color32::WHITE)
                } else {
                    Stroke::new(1.0, th().part_edge.gamma_multiply(ease))
                };
                if selected {
                    // Soft glow round the selected part's own profile.
                    halos.extend(selection_halo(part, pl, |q| to_s(q) - Vec2::new(0.0, drop)));
                }
                paint_part_placed(&painter, part, pl, |q| to_s(q) - Vec2::new(0.0, drop), fill, edge, th().sheet);
                // Every contour in its cut quality's or state's colour (CS7);
                // cut contours only once the sheet is machined (CS8).
                let plain = (!machined).then(|| darker(base, 0.42).gamma_multiply(ease));
                crate::cutpanel::paint_contour_colours(&painter, part, &mt, &|q| to_s(pl.map(q)) - Vec2::new(0.0, drop), 1.2, false, plain);
                // Micro-joints as the cut plan places them (automatic ones too, holes
                // included): small uncut bridges on the contours.
                let planned: Vec<(&[P], (f64, f64))> = if !self.moving_parts() {
                    self.plans.get(sheet_idx).map_or(vec![], |plan| {
                        plan.contours.iter().filter(|c| c.part_no == i + 1).flat_map(|c| c.joints.iter().map(move |&j| (c.points.as_slice(), j))).collect()
                    })
                } else {
                    vec![]
                };
                for (pts, (a, b)) in planned {
                    let half = ((b - a) / 2.0).max(3.5 / s as f64);
                    let mid = (a + b) / 2.0;
                    {
                        let pa = to_s(cnc::point_at(pts, mid - half)) - Vec2::new(0.0, drop);
                        let pb = to_s(cnc::point_at(pts, mid + half)) - Vec2::new(0.0, drop);
                        painter.line_segment([pa, pb], Stroke::new(3.0, th().sheet));
                        let m = pa + (pb - pa) * 0.5;
                        painter.circle_filled(m, 2.2, Color32::from_rgb(250, 204, 21));
                    }
                }
                if self.show_numbers {
                    let size = (pl.w.min(pl.h) as f32 * s * 0.35).clamp(0.0, 15.0);
                    if size >= 7.0 {
                        let c = if matches!(part.shape, Shape::Triangle | Shape::LBracket) {
                            to_s([pl.x + pl.w * 0.3, pl.y + pl.h * 0.3])
                        } else {
                            to_s([pl.x + pl.w / 2.0, pl.y + pl.h / 2.0])
                        };
                        painter.text(c, Align2::CENTER_CENTER, format!("{}", i + 1), FontId::proportional(size), Color32::BLACK);
                    }
                }
            }
            painter.set(halo_slot, EShape::Vec(halos));
            if animating {
                ui.ctx().request_repaint();
            }
            if self.show_path && !self.moving_parts() {
                if let Some(plan) = self.plans.get(sheet_idx) {
                    let mut pos = to_s([0.0, 0.0]);
                    let rapid = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 120, 120, 200));
                    for c in &plan.contours {
                        let li = to_s(c.lead_in);
                        painter.extend(EShape::dashed_line(&[pos, li], rapid, 4.0, 4.0));
                        if let Some(pu) = &c.punch {
                            // Punched: a cross per hit (one for a standard shape, a row when nibbling).
                            let pc = Color32::from_rgb(255, 140, 60);
                            for &h in &pu.hits {
                                let q = to_s(h);
                                painter.line_segment([q - Vec2::new(3.0, 3.0), q + Vec2::new(3.0, 3.0)], Stroke::new(1.3, pc));
                                painter.line_segment([q - Vec2::new(3.0, -3.0), q + Vec2::new(3.0, -3.0)], Stroke::new(1.3, pc));
                            }
                            pos = to_s(*pu.hits.last().unwrap_or(&c.points[0]));
                            continue;
                        }
                        let st = to_s(c.points[0]);
                        // System compensation: the beam's centre path beside the edge (KC8).
                        if c.comp.system {
                            let mut pts: Vec<Pos2> = c.points.iter().map(|&q| to_s(q)).collect();
                            pts.push(pts[0]);
                            painter.add(EShape::line(pts, Stroke::new(1.0, crate::partview::KERF_PATH)));
                        }
                        // Lead-in as cut: line, line + arc, arc, or from a hole's centre.
                        let lp: Vec<Pos2> = c.lead_path.iter().map(|&q| to_s(q)).collect();
                        painter.add(EShape::line(lp, Stroke::new(1.6, Color32::WHITE)));
                        painter.circle_filled(li, 2.0, Color32::WHITE);
                        // Loops at sharp corners.
                        for (vi, l) in &c.loops {
                            let mut pts = vec![to_s(c.points[*vi])];
                            pts.extend(l.iter().map(|&q| to_s(q)));
                            painter.add(EShape::line(pts, Stroke::new(1.4, Color32::from_rgb(250, 204, 21))));
                        }
                        if let Some(lo) = c.lead_out {
                            painter.line_segment([st, to_s(lo)], Stroke::new(1.4, Color32::from_rgb(129, 140, 248)));
                        }
                        pos = st;
                    }
                }
            }
        }
        if has_sheet {
            self.arrange_canvas(&painter, &to_s);
            self.carry_canvas(&painter, &to_s);
            self.turn_canvas(&painter, &to_s, s);
        }
        // Hover card next to the pointer.
        if let (Some(info), Some(hp), None, false, None, None) = (&self.hover_name, resp.hover_pos(), &self.drag, ghosting, &self.carry, &self.turn_drag) {
            let galley = painter.layout(info.clone(), FontId::proportional(12.5), th().text, 260.0);
            let size = galley.size() + Vec2::new(20.0, 14.0);
            let mut pos = hp + Vec2::new(18.0, 18.0);
            if pos.x + size.x > rect.right() - 6.0 {
                pos.x = hp.x - size.x - 12.0;
            }
            if pos.y + size.y > rect.bottom() - 6.0 {
                pos.y = hp.y - size.y - 12.0;
            }
            let card = Rect::from_min_size(pos, size);
            painter.rect_filled(card.translate(Vec2::new(0.0, 3.0)), CornerRadius::same(8), Color32::from_black_alpha(90));
            painter.rect_filled(card, CornerRadius::same(8), th().card);
            painter.rect_filled(Rect::from_min_size(card.min + Vec2::new(0.0, 8.0), Vec2::new(3.0, size.y - 16.0)), CornerRadius::same(2), th().accent);
            painter.galley(card.min + Vec2::new(11.0, 7.0), galley, th().text);
        }

        // Manual cutting: badges on picked contours, and picking while the mode is on.
        if has_sheet {
            if self.manual_mode {
                self.manual_canvas(ui, &painter, &resp, &to_s, &to_w, s);
            }
            self.manual_badges(&painter, &to_s);
        }
        if has_sheet && self.ssim.is_some() {
            self.sheet_sim_canvas(ui, &painter, rect, &to_s);
        }
        if has_sheet {
            self.verify_canvas(&painter, &to_s);
            self.overlap_canvas(&painter, rect, &to_s);
            // The warning leads to each part in the way, one click at a time.
            let bad = self.problem_parts();
            if !bad.is_empty() {
                let b = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 50.0), Vec2::new(170.0, 26.0));
                let k = self.problem_cursor % bad.len();
                let label = format!("Show problem {} of {} ▸", k + 1, bad.len());
                if ui.put(b, egui::Button::new(RichText::new(label).strong())).on_hover_text("Zoom to the next part that overlaps another or sticks out past the sheet, and select it").clicked() {
                    self.focus_placed = Some(bad[k]);
                    self.problem_cursor = k + 1;
                }
            }
        }
        if has_sheet || self.cut_tool.is_some() {
            self.cut_canvas(ui, &painter, &resp, &to_s, &to_w, s);
        }
        if self.ghost.is_some() {
            self.ghost_canvas(ui, &painter, rect, &resp, &to_s, &to_w, s);
        }
        // Turn the selection: step buttons, a typed angle, the step (HM17).
        if has_sheet {
            self.turn_bar(ui, rect, &to_s);
        }

        self.canvas_overlays(ui, rect, now);
    }

    /// Floating zoom / sheet bar, usage chip and pop-up notices on the canvas.
    fn canvas_overlays(&mut self, ui: &mut egui::Ui, rect: Rect, now: f64) {
        let painter = ui.painter_at(rect);
        let n = self.result.sheets.len();
        // --- floating upright toolbar in the right-hand margin ---
        // (zoom in, zoom %, zoom out, fit, previous sheet, sheet n/m, next sheet)
        let items: [(&str, f32); 7] = [("+", 32.0), ("", 22.0), ("−", 32.0), ("Fit", 32.0), ("‹", 30.0), ("", 22.0), ("›", 30.0)];
        let total: f32 = items.iter().map(|i| i.1).sum::<f32>() + 12.0;
        let bw = 48.0;
        let bar = Rect::from_min_size(Pos2::new(rect.right() - bw - 12.0, rect.center().y - total / 2.0), Vec2::new(bw, total));
        painter.rect_filled(bar.translate(Vec2::new(0.0, 3.0)), CornerRadius::same(14), Color32::from_black_alpha(60));
        painter.rect_filled(bar, CornerRadius::same(14), th().overlay);
        painter.rect_stroke(bar, CornerRadius::same(14), Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 40)), egui::StrokeKind::Inside);
        let mut y = bar.top() + 6.0;
        for (k, (label, h)) in items.iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(bar.left() + 4.0, y), Vec2::new(bw - 8.0, *h));
            y += h;
            let text = match k {
                1 => format!("{:.0}%", self.zoom * 100.0),
                5 => format!("{} / {}", if n > 0 { self.cur_sheet + 1 } else { 0 }, n),
                _ => label.to_string(),
            };
            let clickable = !matches!(k, 1 | 5);
            let enabled = match k {
                4 => self.cur_sheet > 0,
                6 => self.cur_sheet + 1 < n,
                _ => true,
            };
            if k == 4 {
                painter.line_segment(
                    [Pos2::new(bar.left() + 10.0, r.top() - 1.0), Pos2::new(bar.right() - 10.0, r.top() - 1.0)],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 60)),
                );
            }
            let mut col = if enabled { Color32::WHITE } else { Color32::from_rgba_unmultiplied(255, 255, 255, 70) };
            if clickable && enabled {
                let resp = ui.interact(r, ui.scope_id().with(("ov", k)), Sense::click());
                let h = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
                if h > 0.0 {
                    painter.rect_filled(r.shrink(1.0), CornerRadius::same(10), Color32::from_rgba_unmultiplied(255, 255, 255, (h * 45.0) as u8));
                }
                if resp.hovered() {
                    col = Color32::from_rgb(199, 210, 254);
                }
                if resp.clicked() {
                    match k {
                        0 => self.zoom = (self.zoom * 1.25).min(40.0),
                        2 => self.zoom = (self.zoom / 1.25).max(0.3),
                        3 => {
                            self.zoom = 1.0;
                            self.pan = Vec2::ZERO;
                        }
                        4 => self.select_sheet(self.cur_sheet - 1),
                        6 => self.select_sheet(self.cur_sheet + 1),
                        _ => {}
                    }
                }
            }
            let size = if matches!(k, 0 | 2 | 4 | 6) { 18.0 } else { 11.5 };
            painter.text(r.center(), Align2::CENTER_CENTER, text, FontId::proportional(size), col);
        }

        // --- compact usage card in the left-hand margin ---
        if n > 0 {
            let (u, _, cut_x) = self.sheet_usage(self.cur_sheet);
            let card = Rect::from_min_size(Pos2::new(rect.left() + 10.0, rect.bottom() - 70.0), Vec2::new(104.0, 58.0));
            painter.rect_filled(card.translate(Vec2::new(0.0, 3.0)), CornerRadius::same(10), Color32::from_black_alpha(60));
            painter.rect_filled(card, CornerRadius::same(10), th().overlay);
            painter.text(card.left_top() + Vec2::new(10.0, 7.0), Align2::LEFT_TOP, "Usage", FontId::proportional(11.0), Color32::from_rgb(161, 161, 170));
            painter.text(card.right_top() + Vec2::new(-10.0, 5.0), Align2::RIGHT_TOP, format!("{u:.1}%"), FontId::proportional(14.0), Color32::WHITE);
            let track = Rect::from_min_size(card.left_top() + Vec2::new(10.0, 27.0), Vec2::new(card.width() - 20.0, 5.0));
            painter.rect_filled(track, CornerRadius::same(3), Color32::from_rgba_unmultiplied(255, 255, 255, 50));
            let mut f = track;
            f.set_width(track.width() * (u as f32 / 100.0).clamp(0.0, 1.0));
            painter.rect_filled(f, CornerRadius::same(3), Color32::from_rgb(129, 140, 248));
            painter.text(
                card.left_bottom() + Vec2::new(10.0, -8.0),
                Align2::LEFT_BOTTOM,
                format!("Free from X {}", self.unit().num_dp(cut_x, 0)),
                FontId::proportional(10.5),
                Color32::from_rgb(161, 161, 170),
            );
        }

        // --- what the active tool wants next, in view (not only in the status bar) ---
        let hint = if self.ghost.is_some() {
            Some("Click to drop the part  ·  R or Space turns it  ·  M mirrors  ·  Enter finds a free spot  ·  Esc or right-click cancels")
        } else if self.manual_mode {
            Some("Click the contours in the order to cut them  ·  drag across parts, or box them, to take them whole  ·  Esc ends")
        } else {
            match self.cut_tool {
                Some(cutoff::CutTool::Straight) => Some("Click where the cut line starts, then where it ends  ·  Esc stops"),
                Some(cutoff::CutTool::Path) => Some("Click the corners of the cut path  ·  double-click or Enter finishes  ·  Esc stops"),
                Some(cutoff::CutTool::Semi(_)) => Some("Click an empty area to make it a remnant  ·  Esc stops"),
                None => None,
            }
        };
        if let Some(h) = hint {
            let galley = painter.layout_no_wrap(h.to_string(), FontId::proportional(13.0), Color32::WHITE);
            let size = galley.size() + Vec2::new(28.0, 14.0);
            // At the top of the drawing, out of the way of the work; when the
            // pointer comes near it, it moves to the bottom.
            let top = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 12.0 + size.y / 2.0), size);
            let near = ui.ctx().pointer_hover_pos().is_some_and(|p| top.expand(40.0).contains(p));
            let pill = if near { Rect::from_center_size(Pos2::new(rect.center().x, rect.bottom() - 24.0 - size.y / 2.0), size) } else { top };
            painter.rect_filled(pill, CornerRadius::same(10), th().overlay);
            painter.galley(pill.min + Vec2::new(14.0, 7.0), galley, Color32::WHITE);
        }

        // --- what the last import / technology command did, with next steps ---
        let mut tech_card_h = 0.0;
        if let Some(card) = self.result_card.clone() {
            let mut close = false;
            let mut act = None;
            let r = egui::Area::new(egui::Id::unique("result_card")).order(egui::Order::Foreground).fixed_pos(rect.left_top() + Vec2::new(12.0, 12.0)).show(ui.ctx(), |ui| {
                egui::Frame::new().fill(th().card).stroke(Stroke::new(1.0, th().border)).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                    ui.set_max_width(360.0);
                    ui.horizontal(|ui| {
                        let (mark, col) = if card.warn { ("⚠", th().warn_fg) } else { ("✔", th().ok_fg) };
                        ui.label(RichText::new(format!("{mark} {}", card.title)).strong().color(col));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✖").on_hover_text("Close").clicked() {
                                close = true;
                            }
                        });
                    });
                    for l in &card.lines {
                        ui.label(RichText::new(l).size(12.0).color(th().text_dim));
                    }
                    if !card.actions.is_empty() {
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            for (k, (label, a)) in card.actions.iter().enumerate() {
                                let b = if k == 0 { egui::Button::new(RichText::new(label).color(Color32::WHITE).strong()).fill(th().accent) } else { egui::Button::new(label) };
                                if ui.add(b).on_hover_text(a.tip()).clicked() {
                                    act = Some(*a);
                                }
                            }
                        });
                    }
                });
            });
            tech_card_h = r.response.rect.height() + 8.0;
            if close {
                self.result_card = None;
            }
            if let Some(a) = act {
                self.result_card = None;
                match a {
                    CardAct::TechAll(k) => {
                        self.tech_all = true;
                        if self.workspace == Workspace::Nesting {
                            self.tech_nest_all = Some(true);
                        }
                        self.run_tech(k);
                    }
                    CardAct::NestAll => self.run_nesting(),
                    CardAct::ShowPart(i) => {
                        self.sel_part = Some(i);
                        self.scroll_to_part = Some(i);
                        // Keep the card: it still says what came in.
                        self.result_card = Some(card.clone());
                    }
                    CardAct::SheetTab => self.tab = Tab::Sheet,
                }
            }
        }

        // --- the last export: where the files went, until closed ---
        if let Some((dir, files, warning, what)) = self.last_export.clone() {
            let mut close = false;
            egui::Area::new(egui::Id::unique("last_export")).order(egui::Order::Foreground).fixed_pos(rect.left_top() + Vec2::new(12.0, 12.0 + tech_card_h)).show(ui.ctx(), |ui| {
                egui::Frame::new().fill(th().card).stroke(Stroke::new(1.0, th().border)).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                    ui.set_max_width(330.0);
                    ui.horizontal(|ui| {
                        let (ok_col, title) = if warning.is_some() { (th().warn_fg, "Programs written — with a warning") } else { (th().ok_fg, "Programs written") };
                        ui.label(RichText::new(format!("✔ {title}")).strong().color(ok_col));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✖").on_hover_text("Close").clicked() {
                                close = true;
                            }
                        });
                    });
                    // What they're for: the material, thickness and gas (GS16).
                    ui.label(RichText::new(&what).strong()).on_hover_text("The programs are cut in this material and thickness, with this gas. A gas marked \"the machine's\" is the machine's gas from Settings → Machines: this job has no gas of its own.");
                    let shown: Vec<&String> = files.iter().take(4).collect();
                    for f in &shown {
                        ui.label(RichText::new(f.as_str()).monospace().size(12.0));
                    }
                    if files.len() > shown.len() {
                        ui.label(RichText::new(format!("and {} more", files.len() - shown.len())).size(12.0).color(th().text_dim));
                    }
                    ui.label(RichText::new(dir.display().to_string()).size(11.5).color(th().text_dim));
                    if let Some(w) = &warning {
                        ui.label(RichText::new(w).size(12.0).color(th().warn_fg));
                    }
                    // EXP4, EXP6: each other machine, written or not and why.
                    for (line, ok) in &self.export_also_lines {
                        ui.label(RichText::new(line).size(12.0).color(if *ok { th().text_dim } else { th().warn_fg }));
                    }
                    ui.add_space(4.0);
                    if ui.button("📂 Open folder").clicked() {
                        open_folder(&dir);
                    }
                });
            });
            if close {
                self.last_export = None;
            }
        }

        // --- pop-up notices (bottom-right), fade out after a few seconds ---
        // Long messages (an export's folder, what a nesting removed) stay longer.
        // --keep-notices (screenshots): they stay until the picture is taken.
        let keep = self.shot.is_some() && std::env::args().any(|a| a == "--keep-notices");
        let life = |m: &str| if keep { f64::INFINITY } else { (4.0 + m.len() as f64 / 40.0).min(9.0) };
        self.toasts.retain(|t| now - t.1 < life(&t.0));
        let mut y = rect.bottom() - 14.0;
        for (msg, t0) in self.toasts.iter().rev().take(3) {
            let age = (now - t0) as f32;
            let end = life(msg) as f32;
            let alpha = if age < 0.2 { age / 0.2 } else if age > end - 0.6 { ((end - age) / 0.6).max(0.0) } else { 1.0 };
            let galley = painter.layout(msg.clone(), FontId::proportional(12.5), Color32::WHITE, 420.0);
            let size = galley.size() + Vec2::new(28.0, 14.0);
            let slide = (1.0 - alpha) * 12.0;
            let card = Rect::from_min_size(Pos2::new(rect.right() - size.x - 14.0 + slide, y - size.y), size);
            painter.rect_filled(card.translate(Vec2::new(0.0, 3.0)), CornerRadius::same(10), Color32::from_black_alpha((70.0 * alpha) as u8));
            painter.rect_filled(card, CornerRadius::same(10), Color32::from_rgba_unmultiplied(24, 24, 30, (240.0 * alpha) as u8));
            painter.circle_filled(card.left_center() + Vec2::new(11.0, 0.0), 3.5, Color32::from_rgba_unmultiplied(129, 140, 248, (255.0 * alpha) as u8));
            painter.galley(card.min + Vec2::new(20.0, 7.0), galley, Color32::from_rgba_unmultiplied(255, 255, 255, (255.0 * alpha) as u8));
            y -= size.y + 8.0;
        }
        if !self.toasts.is_empty() {
            ui.ctx().request_repaint();
        }
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if self.drag_test_on {
            self.drag_test_input(ctx, raw, self.frames + 1);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frames += 1;
        self.quit_guard(&ctx);
        // The active job (ORDENACT.SYS in the original): the open job's library
        // record, so it opens again next time. Not while taking screenshots.
        if self.shot.is_none() {
            let want = self.job_lib_id.map(|i| i.to_string()).unwrap_or_default();
            if self.cfg.active_job != want {
                self.cfg.active_job = want;
            }
        }
        let speed = 40.0 * self.cfg.scroll_speed;
        if (ctx.options(|o| o.input_options.line_scroll_speed) - speed).abs() > 0.01 {
            ctx.options_mut(|o| o.input_options.line_scroll_speed = speed);
        }
        if self.shot.is_none() {
            let z = settings::ui_zoom(self.cfg.ui_size);
            if (ctx.zoom_factor() - z).abs() > 1e-3 {
                ctx.set_zoom_factor(z);
            }
        }
        // An agent or script may change the open job's file (CLI / MCP).
        self.watch_job_file(&ctx);
        // Other PCs' saves to the shared Library (JX8, LB14–LB16).
        self.auto_refresh(&ctx);
        if ctx.input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::S)) {
            self.save_job(false);
        }
        if ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::S)) {
            self.save_job(true);
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::O)) {
            self.open_job_dialog();
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Comma)) {
            self.open_settings(settings_ui::Section::General);
        }
        self.settings_window(&ctx);
        self.new_library_window(&ctx);
        self.first_run_window(&ctx);
        self.shop_admin_window(&ctx);
        self.restore_window(&ctx);
        // UP11: releases for a PC of its own, checked off the window's thread.
        self.update_tick(&ctx);
        // UP21: a PC in a shop installs the shop version from the shop
        // folder; UP16: a trial on the admin PC.
        self.shop_update_tick(&ctx);
        self.rollout_window(&ctx);
        self.rollback_window(&ctx);
        self.whats_new_window(&ctx);
        self.library_window(&ctx);
        self.import_window(&ctx);
        self.stock_window(&ctx);
        if self.demo_open_menu {
            egui::Area::new(egui::Id::unique("demo_open_menu")).order(egui::Order::Foreground).fixed_pos(Pos2::new(8.0, 138.0)).show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| self.open_job_menu(ui));
            });
        }
        self.erp_window(&ctx);
        self.naming_window(&ctx);
        self.save_question_window(&ctx);
        self.in_use_window(&ctx);
        self.taken_over_window(&ctx);
        // Holds on the jobs in use here, renewed about every 30 s (CU5, CU6).
        // Not for screenshots, which never write to a Library by themselves.
        if self.shot.is_none() {
            self.hold_frame(&ctx);
        }
        self.reload_note_card(&ctx);
        self.crash_card(&ctx);
        self.feedback_window(&ctx);
        self.restore_note_card(&ctx);
        self.unknown_material_card(&ctx);
        self.db_browser(&ctx);
        self.nest_verify_window(&ctx);
        self.drops_window(&ctx);
        self.sheet_list_window(&ctx);
        if self.follow_system {
            if let Some(t) = ctx.system_theme() {
                let dark = t == egui::Theme::Dark;
                if dark != theme::is_dark() {
                    theme::set_dark(&ctx, dark);
                }
            }
        }
        if self.shot.is_some() && self.frames == 60 {
            self.shot_arrange();
        }
        if let Some(path) = &self.shot {
            ctx.request_repaint();
            if self.frames == 120 {
                let path = path.clone();
                ctx.request_screenshot(move |img| {
                    let [w, h] = img.size;
                    let mut bytes = Vec::with_capacity(w * h * 4);
                    for c in &img.pixels {
                        bytes.extend_from_slice(&[c.r(), c.g(), c.b(), 255]);
                    }
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let code = match image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                        Some(im) => match im.save(&path) {
                            Ok(_) => 0,
                            Err(e) => {
                                eprintln!("screenshot save failed: {e}");
                                1
                            }
                        },
                        None => 1,
                    };
                    std::process::exit(code);
                });
            }
            if self.frames > 900 {
                eprintln!("screenshot timed out");
                std::process::exit(2);
            }
        }

        // Drawings dropped onto the window become products.
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| formats::is_drawing(p) || p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml")))
                .collect()
        });
        if !dropped.is_empty() {
            self.import_drawings(dropped);
        }
        // A copy on the pointer ends when its sheet changes, the screen
        // changes or another tool starts (QR4); a pattern follows the sheet
        // shown while it can (HM21).
        self.check_carry();
        if self.workspace == Workspace::Nesting {
            // Alt/Ctrl + arrows, Ctrl+Q/W, M, Ctrl+E… on the selected parts.
            self.sheet_keys(&ctx);
        }
        let typing = ctx.memory(|m| m.focused().is_some());
        if !typing {
            let (undo, redo, dup) = ctx.input_mut(|i| {
                let redo = i.consume_key(egui::Modifiers::COMMAND, Key::Y)
                    || i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::Z);
                (i.consume_key(egui::Modifiers::COMMAND, Key::Z), redo, i.consume_key(egui::Modifiers::COMMAND, Key::D))
            });
            if undo {
                self.undo();
            }
            if redo {
                self.redo();
            }
            // With a pattern on the pointer the keys for the selected parts
            // wait (the selection is the copy dropped last, not what is held).
            let held = self.pattern_on();
            if dup && !held {
                self.duplicate_selected();
            }
            let (f5, r, del) = ctx.input(|i| (i.key_pressed(Key::F5), i.key_pressed(Key::R) && i.modifiers.is_none() && !held, i.key_pressed(Key::Delete) && !held));
            if f5 {
                self.run_nesting();
            }
            if r {
                self.rotate_selected();
            }
            if del {
                self.remove_selected();
            }
        }
        if self.status != self.last_status {
            self.last_status = self.status.clone();
            if !self.status.is_empty() {
                let now = ctx.input(|i| i.time);
                self.toasts.push((self.status.clone(), now));
            }
        }

        egui::Panel::top("title")
            .frame(egui::Frame::new().fill(th().chrome).inner_margin(egui::Margin::symmetric(6, 4)))
            .show_separator_line(false)
            .show(ui, |ui| self.title_row(ui));
        egui::Panel::top("tabs")
            .frame(egui::Frame::new().fill(th().chrome).inner_margin(egui::Margin { left: 0, right: 0, top: 3, bottom: 0 }))
            .show_separator_line(false)
            .show(ui, |ui| self.ribbon_tabs(ui));
        // One fixed height for every tab, so the drawing never jumps when
        // switching tabs.
        egui::Panel::top("ribbon")
            .exact_size(crate::ui_parts::RIBBON_H)
            .frame(
                egui::Frame::new()
                    .fill(th().chrome)
                    .inner_margin(egui::Margin::symmetric(4, 4))
                    .stroke(Stroke::new(1.0, th().border)),
            )
            .show(ui, |ui| self.ribbon(ui));
        // UP9: the job in front is read-only (a newer version saved it):
        // said above it, on every screen, with what to do.
        if let Some(why) = self.read_only() {
            egui::Panel::top("read_only")
                .frame(egui::Frame::new().fill(th().warn_bg).inner_margin(egui::Margin::symmetric(10, 5)).stroke(Stroke::new(1.0, th().border)))
                .show_separator_line(false)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("🔒 {} is read-only on this PC.", self.job_name)).strong().color(th().warn_fg));
                        ui.label(RichText::new(why).color(th().warn_fg));
                    })
                    .response
                    .on_hover_text("A newer version of the program saved this job. This PC can show it and make programs from it, but never saves over it, so nothing the newer version saved is lost. Saving it is refused until this PC is updated.");
                });
        }
        // UP13: "Version 0.5.0 is out" while a newer release is offered.
        self.update_banner(ui);
        // UP16, UP21: a trial, or the shop version ready to install.
        self.shop_update_banner(ui);
        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(th().chrome).stroke(Stroke::new(1.0, th().border)).inner_margin(egui::Margin::symmetric(8, 3)))
            .show(ui, |ui| {
                match self.workspace {
                    Workspace::Jobs => self.jobs_status_bar(ui),
                    Workspace::Part => self.part_status_bar(ui),
                    Workspace::Nesting => self.status_bar(ui),
                }
            });
        if self.workspace == Workspace::Part {
            self.part_workspace(ui);
            self.radial_ui(&ctx);
            self.text_editor_ui(&ctx);
            self.dock_overlay(&ctx);
            self.pending_dialog(&ctx);
            self.joint_sizes_window(&ctx);
            return;
        }
        if self.workspace == Workspace::Jobs {
            self.jobs_workspace(ui);
            self.dock_overlay(&ctx);
            self.times_window(&ctx);
            self.pending_dialog(&ctx);
            self.joint_sizes_window(&ctx);
            return;
        }
        egui::Panel::bottom("bottom_tabs")
            .frame(egui::Frame::new().fill(th().pane_header).inner_margin(egui::Margin::symmetric(6, 2)))
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.selectable_label(self.bottom == BottomTab::PartsList, "≡ Parts List").clicked() {
                        self.bottom = BottomTab::PartsList;
                    }
                    if ui.selectable_label(self.bottom == BottomTab::PartsView, "▣ Parts View").clicked() {
                        self.bottom = BottomTab::PartsView;
                    }
                    ui.add_space(16.0);
                    self.parts_search_bar(ui);
                });
            });
        // Sized to one row of part cards; drag the edge for more.
        egui::Panel::bottom("parts_view")
            .resizable(true)
            .default_size(142.0)
            .min_size(120.0)
            .frame(egui::Frame::new().fill(th().panel))
            .show(ui, |ui| self.parts_view(ui));
        if self.show_program {
            egui::Panel::right("program")
                .resizable(true)
                .default_size(320.0)
                .frame(egui::Frame::new().fill(th().panel))
                .show(ui, |ui| self.program_panel(ui));
        }
        self.dock_panels(ui);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(th().bg).inner_margin(egui::Margin::same(3)))
            .show(ui, |ui| self.canvas(ui));
        self.radial_ui(&ctx);
        self.dock_overlay(&ctx);
        self.times_window(&ctx);
        self.step_window(&ctx);
        self.cycles_window(&ctx);
        self.pattern_window(&ctx);
        self.pending_dialog(&ctx);
        self.joint_sizes_window(&ctx);
    }
}

/// `--library-dir=<folder>` on the command line: the Library this run works
/// on instead of the one in the settings.
fn library_dir_arg() -> Option<String> {
    std::env::args().find_map(|a| a.strip_prefix("--library-dir=").map(str::to_string))
}

impl App {
    fn times_window(&mut self, ctx: &egui::Context) {
        if self.show_times {
            let mut open = true;
            egui::Window::new("Times and costs")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::RIGHT_TOP, Vec2::new(-24.0, 176.0))
                .show(ctx, |ui| self.results(ui));
            if !open {
                self.show_times = false;
            }
        }
    }
}

/// Show a folder in the computer's file manager (Explorer on Windows).
pub fn open_folder(dir: &std::path::Path) {
    let prog = if cfg!(windows) { "explorer" } else if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = std::process::Command::new(prog).arg(dir).spawn();
}

/// The Export CNC tooltip: what is written (every sheet), in which format and
/// where, so nobody has to guess before or after exporting.
pub fn export_tip(app: &App) -> String {
    let n = app.result.sheets.len();
    let what = if app.blm.on { "BLM LC5 program set" } else { "CNC program" };
    // SR21: one program per nesting, run as often as the sheet is cut.
    let each = if app.sheets_cut() == n { format!("each of the {n} sheet(s)") } else { format!("each of the {n} nesting(s) ({}; a sheet cut several times runs its program again)", engine::sheets_text(&app.result.formats, n)) };
    format!("Writes a {what} for {each}, plus a parts report, to\n{}\n(Settings → Folders, or the machine's own programs folder in Settings → Machines, to change it).", app.export_dir().display())
}

fn main() -> eframe::Result {
    // UP2: the installer's hook runs before anything else (Windows only).
    updates::startup_hook();
    std::panic::set_hook(Box::new(|info| eprintln!("panic: {info}")));
    // `nestpro <operation> …` and `nestpro mcp`: the command line and the MCP
    // server for agents and scripts, without opening the window.
    let argv: Vec<String> = std::env::args().collect();
    if cli::is_cli(&argv) {
        std::process::exit(cli::main(argv));
    }
    // Letting the graphics layer pick among every backend at once closes the
    // window silently on some Windows PCs; one explicit backend is reliable.
    #[cfg(windows)]
    if std::env::var_os("WGPU_BACKEND").is_none() {
        // SAFETY: called at start-up before any other thread exists.
        unsafe { std::env::set_var("WGPU_BACKEND", "dx12") };
    }
    let args: Vec<String> = std::env::args().collect();
    let shot = args.iter().position(|a| a == "--screenshot").and_then(|i| args.get(i + 1)).map(PathBuf::from);
    // FB7, FB8: an error that closes the window is remembered in this PC's
    // settings folder, so the next start can offer to send a note about it.
    // Screenshot runs never write it.
    if shot.is_none() {
        let dir = feedbackui::pc_dir();
        std::panic::set_hook(Box::new(move |info| {
            eprintln!("panic: {info}");
            if engine::feedback::closes_window() {
                let (message, place) = engine::feedback::panic_message(info);
                let when = shared::Clock::pc().now();
                engine::feedback::record_crash(&dir, &engine::feedback::Crash { when, version: updates::VERSION.into(), message, place });
            }
        }));
    }
    // SH9: a PC with no settings yet starts at the first-run screen. Looked
    // at before anything could save them; never in a screenshot run, nor
    // with a Library folder given on the command line.
    let first_run = shot.is_none() && library_dir_arg().is_none() && startup::first_run(&settings::AppSettings::file(), &library::default_dir());
    let want_dark = args.iter().any(|a| a == "--dark");
    let want_light = args.iter().any(|a| a == "--light");
    let want_jobs = args.iter().any(|a| a == "--jobs");
    let want_tech = args.iter().any(|a| a == "--tech");
    let want_part = args.iter().any(|a| a == "--part");
    let want_float = args.iter().any(|a| a == "--float-panel");
    let want_radial = args.iter().any(|a| a == "--radial");
    let want_manual = args.iter().any(|a| a == "--manual");
    // --manual-sweep="x,y;x,y;…": a sweep across sheet 1's parts along that path, sheet mm (screenshots).
    let manual_sweep = args.iter().find_map(|a| a.strip_prefix("--manual-sweep=").map(|s| s.to_string()));
    let want_ghost = args.iter().any(|a| a == "--ghost");
    let want_ghost_ring = args.iter().any(|a| a == "--ghost-ring");
    let want_cutoff = args.iter().any(|a| a == "--cutoff");
    let want_part_pan = args.iter().any(|a| a == "--part-pan");
    // --machine=laser|plasma|oxyfuel|waterjet|laserpunch|punch (screenshots).
    let want_sim = args.iter().any(|a| a == "--sim");
    let want_times = args.iter().any(|a| a == "--times");
    // --cycles: the Sheet cycles window on the current sheet (screenshots).
    let want_cycles = args.iter().any(|a| a == "--cycles");
    let want_db = args.iter().any(|a| a == "--db");
    let want_post = args.iter().any(|a| a == "--post");
    let want_dbb = args.iter().any(|a| a == "--dbbrowse");
    let want_lib = args.iter().any(|a| a == "--library");
    let job_save = args.iter().find_map(|a| a.strip_prefix("--save-job=").map(PathBuf::from));
    let job_open = args.iter().find_map(|a| a.strip_prefix("--open-job=").map(PathBuf::from));
    let open_nest = args.iter().find_map(|a| a.strip_prefix("--open-nest=").map(PathBuf::from));
    // Another program hands parts over the way it would to Expert: a Lantek
    // import XML (DXF Import / Parts Import schema).
    let import_xml = args.iter().find_map(|a| a.strip_prefix("--import-xml=").map(PathBuf::from));
    let want_verify = args.iter().any(|a| a == "--verify");
    let machine_arg = args.iter().find_map(|a| a.strip_prefix("--machine=").map(|s| s.to_lowercase()));
    // --part-tab=machining|view|simulation|notes|utilities|technology (screenshots).
    let part_tab = args.iter().find_map(|a| a.strip_prefix("--part-tab=").map(|s| s.to_string()));
    let want_edit_text = args.iter().any(|a| a == "--edit-text");
    let want_drag_test = args.iter().any(|a| a == "--drag-test");
    let (icon, _icon_file) = iconfile::window_icon();
    // Like the original's start-up (rules SU3, SU4): the first
    // plain word may name a file to open, and the window opens maximized
    // (SW_SHOWMAXIMIZED) — except for screenshots, which need a fixed size.
    // "/hide" isn't followed: a hidden window could never be reached here.
    let first = startup::first_word(&args);
    let start_file = startup::start_file(first, jobfile::EXT);
    let maximized = shot.is_none() && first != Some(startup::HIDE_SWITCH) && startup::show_state(first) == startup::SW_SHOWMAXIMIZED;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 880.0])
            .with_min_inner_size([900.0, 600.0])
            .with_maximized(maximized)
            .with_title("Nesting and Machining - Sheet Nesting")
            // The black icon (assets/icon.png) for the title bar and taskbar,
            // or nestpro.ico / nestpro.png when one is there.
            .with_icon(std::sync::Arc::new(egui::IconData { rgba: icon.rgba, width: icon.width, height: icon.height })),
        ..Default::default()
    };
    let res = eframe::run_native(
        "Sheet Nesting",
        options,
        Box::new(move |cc| {
            // Start in the mode asked for on the command line, otherwise follow
            // the computer's own light/dark setting (light if unknown).
            let dark = if want_dark {
                true
            } else if want_light || shot.is_some() {
                false
            } else {
                cc.egui_ctx.system_theme() == Some(egui::Theme::Dark)
            };
            theme::set_dark(&cc.egui_ctx, dark);
            let load_layout = shot.is_none();
            let mut app = App::new(shot);
            // SH9: until the first-run screen is answered, nothing saves this
            // PC's settings: a person who closes the program first gets the
            // screen again.
            app.cfg.setup_pending = first_run;
            // No mode asked for: keep following Windows, like the original's
            // default style (rule V5).
            app.follow_system = !want_dark && !want_light && load_layout;
            // Without the example jobs: one empty job (not for screenshots).
            if load_layout && !app.cfg.examples {
                let spec = app.spec.clone();
                app.jobs = vec![Job { week: String::new(), name: "New job".into(), parts: vec![], spec, result: nest::NestResult::default(), stale: false, lib_id: None, cut_lines: vec![], manual_cuts: vec![], cam_changed: None, held: Default::default(), erp: None, file: None, saved: None, lib_read: Default::default(), lib_folder: None, material_names: vec![], read_only: None }];
                app.cur_job = 0;
                app.job_name = "New job".into();
                app.parts.clear();
                app.result = nest::NestResult::default();
                app.sel_part = None;
                app.cur_sheet = 0;
                app.cut_lines.clear();
                app.refresh();
            }
            // S6: the Library this PC works in is settled first (a trial
            // the shop moved on from ends here), then the active job is
            // reopened from it (rule AJ2: no record = no active job).
            if load_layout {
                app.open_at_start();
            }
            if first_run {
                app.firstrun = Some(startup::FirstRun::default());
            }
            // --first-run[=join] (screenshots): the first-run screen (SH9).
            if let Some(what) = std::env::args().find_map(|a| if a == "--first-run" { Some(String::new()) } else { a.strip_prefix("--first-run=").map(str::to_string) }) {
                app.demo_first_run(&what);
            }
            // SH4: this PC records itself in its shop when the program opens.
            // Never in a screenshot run, which works on made-up PCs.
            if !app.shot_mode_now() {
                app.record_in_shop();
                // BK2: the day's backup, if no PC has taken it yet today.
                app.start_backup(crate::backup::Why::Daily);
            }
            // UP11: a PC of its own checks for releases (a screenshot only
            // with a made-up list, --fake-releases=<file>).
            let fake = std::env::args().find_map(|a| a.strip_prefix("--fake-releases=").map(PathBuf::from)).or_else(|| std::env::var_os("NESTPRO_FAKE_RELEASES").map(PathBuf::from));
            let chosen = fake.is_some() && std::env::args().any(|a| a == "--update-chosen");
            app.start_update_checks(fake);
            // --update-chosen / --whats-new (screenshots, with --fake-releases):
            // Update when I close pressed, or What's new open, once the check
            // has answered.
            app.setui.upd.demo = (chosen, std::env::args().any(|a| a == "--whats-new"));
            // Saved preferences (Settings → General / Appearance).
            app.warn_tech = app.cfg.warn_tech;
            app.dv.show_unbuilt = app.cfg.show_unbuilt;
            if !want_dark && !want_light && load_layout {
                match app.cfg.theme.as_str() {
                    "light" | "dark" => {
                        app.follow_system = false;
                        theme::set_dark(&cc.egui_ctx, app.cfg.theme == "dark");
                    }
                    _ => app.follow_system = true,
                }
            }
            if std::env::args().any(|a| a == "--options") {
                app.options_open = true;
            }
            // FB7: the crash this PC remembers from last time; screenshots
            // never read this PC's own (--crash-note shows a made-up one).
            if app.shot.is_none() {
                app.feedback.crash_card = engine::feedback::last_crash(&feedbackui::pc_dir());
            }
            if std::env::args().any(|a| a == "--crash-note") {
                app.demo_crash_note();
            }
            // --machine-gas=<number>:<name>: the active machine saved with
            // that gas number and name, as settings from before the gas was
            // a pick may hold (screenshots of GS17's flag). Only with
            // --screenshot, which never saves the settings: launched by hand
            // it's ignored, so the real machine's gas is never overwritten.
            let machine_gas = app.shot.as_ref().and_then(|_| std::env::args().find_map(|a| a.strip_prefix("--machine-gas=").and_then(|v| v.split_once(':').map(|(n, s)| (n.parse::<u32>().ok(), s.to_string())))));
            if let Some((n, name)) = machine_gas {
                if let (Some(n), Some(m)) = (n, app.cfg.machines.get_mut(app.cfg.active_machine)) {
                    m.gas_id = n;
                    m.gas_name = name;
                    app.apply_settings(true);
                }
            }
            if let Some(s) = std::env::args().find_map(|a| a.strip_prefix("--settings=").map(str::to_string)) {
                use settings_ui::Section::*;
                let sec = match s.as_str() {
                    "shop" | "shop-admin" => Shop,
                    "appearance" => Appearance,
                    "shortcuts" => Shortcuts,
                    "about" => About,
                    "connections" => Connections,
                    "machines" | "machines-order" | "machines-qualities" | "machines-sizes" => Machines,
                    "materials" => Materials,
                    "erp" | "integrations" => Integrations,
                    "folders" => Folders,
                    "nesting" => Nesting,
                    _ => General,
                };
                app.open_settings(sec);
                // Screenshots of a machine's cut order scroll down to it.
                app.setui.show_order = s == "machines-order";
                app.setui.show_qualities = s == "machines-qualities";
                app.setui.show_sizes = s == "machines-sizes";
                // Screenshots of Settings → Materials with a material picked.
                if let Some(n) = std::env::args().find_map(|a| a.strip_prefix("--settings-material=").map(str::to_string)) {
                    // The page lists the materials in the list's order.
                    app.setui.mat_sel = engine::material::current().materials.iter().position(|m| m.name == n);
                }
            }
            // Load the saved "ExpertLayout" (the original does this at start, D6).
            if load_layout {
                if let Some(d) = dock::Dock::load() {
                    app.dock = d;
                }
            }
            if want_part {
                if let Some(i) = app.parts.iter().position(|p| p.name == "Square hole") {
                    app.open_part(i);
                    // Show a picked element: the first line of the outer contour.
                    let el = nest::elements_of(&app.parts[i].geometry());
                    if let Some(k) = el.iter().position(|e| e.contour == app.parts[i].geometry().holes.len()) {
                        app.elem_part = Some(i);
                        app.sel_elems = vec![k];
                    }
                }
            }
            if let Some(t) = &part_tab {
                app.tab = match t.as_str() {
                    "technology" => Tab::Technology,
                    "machining" => Tab::Machining,
                    "view" => Tab::View,
                    "simulation" => {
                        app.dv.sim = Some(0.0);
                        Tab::Simulation
                    }
                    "notes" => Tab::Notes,
                    "utilities" => {
                        app.dv.show_unbuilt = true;
                        Tab::Utilities
                    }
                    "missing" => {
                        app.dv.show_missing = true;
                        Tab::Home
                    }
                    _ => Tab::Home,
                };
            }
            // --smooth-window: the part screen's Smooth window open (SM21).
            if std::env::args().any(|a| a == "--smooth-window") {
                app.dv.smooth = Some(app.cfg.smooth.command);
            }
            app.apply_settings(true);
            // --part-sim-at=<share>: the part's simulation that far through its program.
            if let Some(f) = args.iter().find_map(|a| a.strip_prefix("--part-sim-at=").and_then(|v| v.parse::<f64>().ok())) {
                let total: f64 = app.part_sim_ops().0.iter().map(|o| o.secs).sum();
                app.dv.sim = Some(total * f.clamp(0.0, 1.0));
                app.dv.sim_follow = true;
            }
            // --part-cleared: the part after Clear machining, its cut contours
            // in the plain colour (CS8).
            if args.iter().any(|a| a == "--part-cleared") {
                app.dv.machining = false;
            }
            // --part-zoom=<z>: the part view zoomed in (with --part-sim-at, on the head).
            if let Some(z) = args.iter().find_map(|a| a.strip_prefix("--part-zoom=").and_then(|v| v.parse::<f32>().ok())) {
                app.part_zoom = z.clamp(crate::partview::PART_ZOOM_MIN, crate::partview::PART_ZOOM_MAX);
            }
            // The database's jobs at start-up (Settings → General).
            if load_layout && app.cfg.auto_db && app.db_ready() {
                app.db_open_now();
            }
            // --export: write the programs at start (checks the BLM set).
            if std::env::args().any(|a| a == "--export") {
                app.export();
            }
            if want_verify {
                app.tab = Tab::Machining;
                app.nverify.all_sheets = true;
                app.verify_nest();
            }
            if let Some(p) = &job_save {
                app.save_job_to(p);
            }
            if let Some(p) = &job_open {
                app.open_job_file(p);
                // With --part, the opened job's first part in the Drawing module.
                if want_part {
                    app.open_part(0);
                }
            }
            if let Some(p) = &import_xml {
                app.import_xml(p);
                app.workspace = Workspace::Jobs;
            }
            if let Some(p) = &open_nest {
                app.open_nesting_file(p);
            }
            // A file named on the command line (double-click), opened once the
            // window is up, as the original does with a ".NXP" (SU3).
            match &start_file {
                Some(startup::StartFile::Nesting(p)) => app.open_nesting_file(p),
                Some(startup::StartFile::Job(p)) => app.open_job_file(p),
                None => {}
            }
            // --library-dir=<folder> was set when the window was made
            // (`library_dir_arg`); --lib-save: screenshots of a filled library.
            if let Some(what) = std::env::args().find_map(|a| a.strip_prefix("--save-question=").map(str::to_string)) {
                app.demo_save_question(&what);
            }
            // --shop-demo[=none|unrecorded] (screenshots): a made-up shop and
            // its PCs in a temporary Library of its own (SH5);
            // --settings=shop-admin: with the admin PC question open (SH7).
            if let Some(what) = settings_ui::shop_demo_arg() {
                app.demo_shop(&what);
                // --rollout-demo=warning|ready|trial|trial-stop|rollback|rollback-refused
                // (with --fake-releases): updates in that shop (UP16, UP20,
                // UP21, UP23, UP24; trial-stop: Stop trying refused while a
                // job from the trial is open, S3).
                if let Some(w) = std::env::args().find_map(|a| a.strip_prefix("--rollout-demo=").map(str::to_string)) {
                    app.demo_rollout(&w);
                }
            }
            // --in-use=note|ask|warning|taken (screenshots, with --library-dir=
            // a new, empty folder): jobs in use on made-up PCs (CU8–CU10).
            if let Some(what) = std::env::args().find_map(|a| a.strip_prefix("--in-use=").map(str::to_string)) {
                app.demo_in_use(&what);
            }
            // --reload-note (screenshots, with --library-dir= a new, empty
            // folder): an open library job another PC saved reloads by
            // itself, with its note (LB15).
            // --reload-note=ask: it has unsaved changes here, so it asks.
            // --feedback (screenshots): the Feedback window open (FB1).
            if std::env::args().any(|a| a == "--feedback") {
                app.demo_feedback();
            }
            if let Some(a) = std::env::args().find(|a| a == "--reload-note" || a == "--reload-note=ask") {
                app.demo_reload_note(a.ends_with("=ask"));
            }
            // --closing-restore (screenshots): the window closed while a
            // restore runs: it waits, saying so (S2). The made-up restore
            // never ends; the screenshot ends the program.
            if std::env::args().any(|a| a == "--closing-restore") {
                std::mem::forget(app.handoff.begin(updates::Work::Restore));
                app.quit_wait = Some(updates::Work::Restore);
            }
            // --restore-note (screenshots, with --library-dir= a new, empty
            // folder): another PC restored the shop; this one is told (BK13).
            if std::env::args().any(|a| a == "--restore-note") {
                app.demo_restore_note();
            }
            // --unknown-material (screenshots, with --library-dir= a new,
            // empty folder): a job whose material isn't in the list (MA30).
            if std::env::args().any(|a| a == "--unknown-material") {
                app.demo_unknown_material();
            }
            // --erp-demo (screenshots, with --jobs): a made-up Fulcrum work
            // order cut from two materials opened as jobs (F35, JX21).
            // --erp-demo-tree: the same, with the window closed (the Job Explorer).
            if std::env::args().any(|a| a == "--erp-demo" || a == "--erp-demo-tree") {
                app.demo_erp_two_materials();
                app.erp.open = !std::env::args().any(|a| a == "--erp-demo-tree");
            }
            // --refresh-minutes=N (screenshots): the auto-refresh setting (JX8).
            if let Some(m) = std::env::args().find_map(|a| a.strip_prefix("--refresh-minutes=").and_then(|v| v.parse::<i64>().ok())) {
                app.cfg.refresh_minutes = engine::refresh::period(m);
            }
            // --lib-tree-open: every Library folder opened out, and the Job
            // Explorer the whole height (screenshots).
            if std::env::args().any(|a| a == "--lib-tree-open") {
                app.libui.reveal_all = true;
                app.libui.loaded = false;
                app.dock.close(Workspace::Jobs, dock::PaneId::JobSummary);
            }
            // --library-pick=<folder> (screenshots): that folder picked in
            // Settings → Folders, asking first when it holds no Library (LB12).
            if let Some(d) = std::env::args().find_map(|a| a.strip_prefix("--library-pick=").map(str::to_string)) {
                app.pick_library_folder(d);
            }
            if std::env::args().any(|a| a == "--lib-save") {
                app.save_job_to_library();
                app.add_products_to_library();
            }
            // --lib-save-job: the open job only, its save message left in the
            // status line (screenshots of what a save says, SV10).
            if std::env::args().any(|a| a == "--lib-save-job") {
                app.save_job_to_library();
            }
            if want_lib {
                app.workspace = Workspace::Jobs;
                app.libui.open = true;
                // --library-products: on its products tab.
                if std::env::args().any(|a| a == "--library-products") {
                    app.libui.tab = 1;
                }
            }
            // --import=<folder> [--import-pick=<file name>]: the Import drawings
            // window there, with that file picked and previewed (screenshots).
            if let Some(d) = std::env::args().find_map(|a| a.strip_prefix("--import=").map(PathBuf::from)).filter(|d| !d.is_file()) {
                app.importui.sync = true;
                app.importui.show(std::slice::from_ref(&d));
                if let Some(f) = std::env::args().find_map(|a| a.strip_prefix("--import-pick=").map(|n| d.join(n))) {
                    app.importui.focus = Some(f.clone());
                    app.importui.picked.insert(f);
                }
                // --import-panel (screenshots): the preview's cut technology panel
                // (IM11). --import-layer=<layer>=<cut|mark|ignore> sets a layer row
                // and --import-paint="7=mark; 1=leave-out" paints contours, both
                // through the panel's own functions; --import-row=<state> chooses a row.
                if std::env::args().any(|a| a == "--import-panel") {
                    app.importui.panel.open = true;
                    let (cfg, spec) = (app.cfg.clone(), app.spec.clone());
                    app.importui.want_preview(&cfg, &spec, &egui::Context::default());
                    let state = |w: &str| dxf::ContourState::ALL.into_iter().find(|s| s.key() == w.trim());
                    let row = |st: dxf::ContourState| match st {
                        dxf::ContourState::Cut => cutpanel::Row::Cut(None),
                        dxf::ContourState::Mark => cutpanel::Row::Mark,
                        dxf::ContourState::NoCut => cutpanel::Row::NoCut,
                        dxf::ContourState::BendLine => cutpanel::Row::BendLine,
                        dxf::ContourState::LeaveOut => cutpanel::Row::LeaveOut,
                    };
                    for a in std::env::args().filter_map(|a| a.strip_prefix("--import-layer=").map(str::to_string)) {
                        if let Some((l, r)) = a.rsplit_once('=') {
                            let role = dxf::LayerRole::ALL.into_iter().find(|x| x.label().eq_ignore_ascii_case(r) || (r == "ignore" && *x == dxf::LayerRole::Ignore));
                            if let Some(role) = role {
                                app.preview_set_layer(l, role);
                            }
                        }
                    }
                    if let Some(p) = std::env::args().find_map(|a| a.strip_prefix("--import-paint=").map(str::to_string)) {
                        for (k, st) in p.split(';').filter_map(|c| c.split_once('=')).filter_map(|(k, w)| Some((k.trim().parse::<usize>().ok()?.checked_sub(1)?, state(w)?))) {
                            app.preview_paint(&[k], row(st), false);
                        }
                    }
                    app.importui.panel.row = std::env::args().find_map(|a| a.strip_prefix("--import-row=").and_then(state)).map(row);
                }
            }
            // --nest-stock: nest again from the library's sheet stock;
            // --stock: show the Sheet stock window (screenshots).
            if std::env::args().any(|a| a == "--nest-stock") {
                app.run_nesting();
                app.anim_pending = false;
            }
            // --units=inch: lengths in inches (screenshots; --select is applied in `ui`).
            if std::env::args().any(|a| a == "--units=inch") {
                app.cfg.nesting.units = units::LenUnit::Inch;
            }
            // --remnant-mark: the automatic mark on remnants on (screenshots).
            if std::env::args().any(|a| a == "--remnant-mark") {
                app.cfg.stock.mark = remmark::MarkSettings { on: true, text: "#300 #301x#302".into(), height: 18.0, ..Default::default() };
            }
            // --remline=<code>: the automatic remnant line in that shape (screenshots).
            if let Some(c) = std::env::args().find_map(|a| a.strip_prefix("--remline=").and_then(|v| v.parse::<u8>().ok())) {
                if let Some(s) = remline::LineShape::ALL.into_iter().find(|s| s.code() == c) {
                    app.tab = Tab::Sheet;
                    app.cur_sheet = app.result.sheets.len().saturating_sub(1);
                    app.cfg.stock.line.shape = s;
                    app.cut_lines.clear();
                    app.auto_remnant_edge();
                }
            }
            // --drops (screenshots): standard drops on with made-up sizes, the
            // leftover cut off and the Drop layouts window open.
            if std::env::args().any(|a| a == "--drops") {
                app.tab = Tab::Sheet;
                app.cur_sheet = app.result.sheets.len().saturating_sub(1);
                app.cfg.stock.drops = engine::drops::DropSettings { on: true, sizes: vec![[1828.8, 1219.2], [1219.2, 1219.2], [1219.2, 609.6], [609.6, 609.6]], keep_odd: false };
                if !std::env::args().any(|a| a.starts_with("--remline=")) {
                    app.cfg.stock.line.shape = remline::LineShape::VerticalStrip;
                }
                app.cut_lines.clear();
                app.auto_remnant_edge();
                app.dropsui.open = true;
                // --drops-layout=N: that layout cut, the window closed.
                if let Some(n) = std::env::args().find_map(|a| a.strip_prefix("--drops-layout=").and_then(|v| v.parse::<usize>().ok())) {
                    let sheet = app.result.sheets.get(app.cur_sheet).cloned().unwrap_or_default();
                    let st = app.cfg.stock.clone();
                    if let Some(l) = engine::drops::layouts(&app.parts, &sheet, &app.result.format(app.cur_sheet, &app.spec), &app.cur_spec(), &st.line, &st.drops.sizes, st.min_remnant_m2).ok().and_then(|v| v.into_iter().nth(n.saturating_sub(1))) {
                        app.apply_drop_layout(&l);
                    }
                    app.dropsui.open = false;
                }
            }
            // --stock-demo (screenshots): two made-up sheets in the stock, the
            // smaller one picked, so the window shows a real choice.
            if std::env::args().any(|a| a == "--stock-demo") {
                let (m, t) = (app.spec.material, app.spec.thickness);
                let _ = stock::Stock::update(&app.library_at(), |s| {
                    s.add_sheet(stock::Item::sheet("DEMO-S1", m, t, 2000.0, 1000.0, 5), true)?;
                    let mut r = stock::Item::sheet("DEMO-R1", m, t, 1200.0, 1000.0, 1);
                    r.kind = stock::Kind::UserRemnant;
                    s.add_sheet(r, true)?;
                    // More made-up stock, so the material shelf and the cards
                    // have something to show: another size, an L-shaped
                    // remnant, another thickness and the other materials.
                    s.add_sheet(stock::Item::sheet("DEMO-S2", m, t, 3048.0, 1219.2, 8), true)?;
                    let mut l = stock::Item::sheet("DEMO-001-R1", m, t, 2032.0, 1067.0, 1);
                    l.kind = stock::Kind::NestRemnant;
                    l.origin = "3".into();
                    l.from_job = "DEMO-001".into();
                    l.outline = Some(stock::Outline { outer: vec![[0.0, 0.0], [2032.0, 0.0], [2032.0, 457.0], [1016.0, 457.0], [1016.0, 1067.0], [0.0, 1067.0]], holes: vec![] });
                    s.add_sheet(l, true)?;
                    s.add_sheet(stock::Item::sheet("DEMO-S3", m, 5.0, 3657.6, 1524.0, 3), true)?;
                    s.add_sheet(stock::Item::sheet("DEMO-SS2", (m + 1) % crate::engine::material::count(), 2.0, 2438.4, 1219.2, 4), true)?;
                    s.add_sheet(stock::Item::sheet("DEMO-AL1", (m + 2) % crate::engine::material::count(), 3.0, 3048.0, 1524.0, 1), true)
                });
                app.stockui.sel = Some("DEMO-R1".into());
            }
            // --sheet-list (screenshots): the job's own sheet list (JS5) with a
            // made-up remnant from --stock-demo and two sizes, nested on;
            // --sheet-list-add: the Add sheet size window open too (JS1).
            if std::env::args().any(|a| a == "--sheet-list") {
                app.workspace = Workspace::Nesting;
                if let Some(it) = stock::Stock::load(&app.library_dir()).ok().and_then(|s| s.get("DEMO-R1").cloned()) {
                    let _ = app.add_sheet_list_line(crate::nest::ListSheet::stock(&it, Some(1)));
                }
                let _ = app.add_sheet_list_line(crate::nest::ListSheet::size(1219.2, 609.6, Some(2)));
                let _ = app.add_sheet_list_line(crate::nest::ListSheet::size(1524.0, 1219.2, None));
                if !app.parts.is_empty() {
                    app.run_nesting();
                }
                if std::env::args().any(|a| a == "--sheet-list-add") {
                    app.open_sheet_list_add();
                    app.sheet_list_ui_pick_demo();
                    if std::env::args().any(|a| a == "--sheet-list-size") {
                        app.sheet_list_ui_size_demo();
                    }
                }
            }
            // --stock-other (screenshots): a made-up sheet of another material,
            // picked, so the window offers to switch the job to it (ST16).
            if std::env::args().any(|a| a == "--stock-other") {
                let (m, t) = ((app.spec.material + 1) % crate::engine::material::count(), app.spec.thickness);
                let _ = stock::Stock::update(&app.library_at(), |s| s.add_sheet(stock::Item::sheet("DEMO-SS1", m, t, 1486.0, 724.0, 1), true));
                app.stockui.sel = Some("DEMO-SS1".into());
                app.stockui.shelf = Some(crate::stockui::Shelf::Thickness(m, t));
            }
            if std::env::args().any(|a| a == "--stock") {
                app.stockui.open = true;
                app.stockui.show_options = std::env::args().any(|a| a == "--stock-options");
                // --stock-cards / --stock-new: the cards view, or the New sheet form open.
                app.stockui.cards = std::env::args().any(|a| a == "--stock-cards");
                if std::env::args().any(|a| a == "--stock-new") {
                    app.stockui.creating = true;
                    app.stockui.new_item.material = app.spec.material;
                    app.stockui.new_item.thickness = app.spec.thickness;
                    app.stockui.new_item.length = 3048.0;
                    app.stockui.new_item.width = 1219.2;
                    app.stockui.new_item.qty = 10;
                }
            }
            // --erp: the ERP window filled with made-up work (screenshots, no network).
            if std::env::args().any(|a| a == "--erp") {
                app.workspace = Workspace::Jobs;
                app.erp_demo();
                // With --settings=erp the page is what is shown.
                app.erp.open = !std::env::args().any(|a| a == "--settings=erp");
            }
            if want_dbb {
                app.open_db_browser();
            }
            if want_post {
                app.tab = Tab::Machining;
                app.open_settings(settings_ui::Section::Machines);
                app.show_program = true;
            }
            if want_times {
                app.show_times = true;
            }
            if want_cycles {
                app.open_cycles(app.cur_sheet);
            }
            if want_db {
                app.workspace = Workspace::Jobs;
                app.open_db_window();
            }
            if want_sim {
                // Halfway through the first sheet's program, Simulation tab open.
                app.tab = Tab::Simulation;
                let mut s = sheetsim::SheetSim::default();
                if let Some(plan) = app.plans.first() {
                    let names: Vec<String> = app.result.sheets[0].iter().map(|p| app.parts[p.part].name.clone()).collect();
                    let total: f64 = sheetsim::ops(plan, &names).iter().map(|o| o.secs).sum();
                    s.t = total * 0.45;
                }
                app.ssim = Some(s);
            }
            if let Some(mk) = &machine_arg {
                if let Some(k) = machine::MachineKind::ALL.into_iter().find(|k| k.label().to_lowercase().replace(['-', ' '], "").starts_with(mk.as_str())) {
                    app.spec.machine = k;
                    app.dv.std_shapes = true;
                    app.refresh();
                }
            }
            if want_part_pan {
                // Zoomed in and moved, as after scrolling and middle-dragging.
                app.part_zoom = 2.4;
                app.part_pan = Vec2::new(-260.0, 140.0);
            }
            app.drag_test_on = want_drag_test;
            if want_edit_text {
                if let Some(i) = app.shown_part() {
                    if app.parts[i].texts.len() > 1 {
                        app.edit_text_at(egui::Pos2::new(1150.0, 860.0), i, 1);
                    }
                }
            }
            if want_manual && !app.result.sheets.is_empty() {
                // Pick contours is on the Machining tab, with Undo pick and Clear.
                app.tab = Tab::Machining;
                app.manual_mode = true;
                let picks: Vec<nest::P> = app.result.sheets[0].iter().take(3).map(|p| [p.x + p.w * 0.5, p.y]).collect();
                for (k, p) in picks.into_iter().enumerate() {
                    app.manual_reverse = k == 1;
                    app.manual_pick(p, 1.0);
                }
                app.manual_reverse = false;
            }
            if let (Some(path), false) = (&manual_sweep, app.result.sheets.is_empty()) {
                app.tab = Tab::Machining;
                app.manual_mode = true;
                let pts: Vec<nest::P> = path.split(';').filter_map(|t| t.split_once(',').and_then(|(x, y)| Some([x.trim().parse().ok()?, y.trim().parse().ok()?]))).collect();
                app.manual_sweep_along(&pts);
            }
            if want_ghost {
                // A part picked in the Parts View, hovering near a free spot.
                if let Some(i) = app.parts.iter().position(|p| p.name == "Square") {
                    app.start_ghost(i, false);
                    if let (Some(c), Some(g)) = (app.ghost_free_spot(), app.ghost.as_mut()) {
                        g.at = Some([c.x + c.w / 2.0 + 6.0, c.y + c.h / 2.0 + 30.0]);
                    }
                }
            }
            if want_ghost_ring {
                // A ring hovering by the pocket between two nested rings.
                if let Some(i) = app.parts.iter().position(|p| p.shape == Shape::Disc) {
                    // Lift the last ring off the sheet so there's a gap to drop it back into.
                    let lifted = app.result.sheets.first_mut().and_then(|s| s.iter().rposition(|p| p.part == i).map(|k| s.remove(k)));
                    app.refresh();
                    app.start_ghost(i, false);
                    let rings: Vec<(P, f64)> = app.result.sheets.first().map_or(vec![], |s| s.iter().filter_map(|o| nest::placed_circle(&app.parts, o)).collect());
                    let (lc, r) = nest::part_circle(&app.parts[i]).unwrap_or(([0.0, 0.0], 0.0));
                    let (fw, fh) = ghost::footprint(&app.parts[i], 0);
                    let mut spot = None;
                    'find: for a in 0..rings.len() {
                        for b in a + 1..rings.len() {
                            for p in nest::pocket_points(rings[a].0, rings[a].1, rings[b].0, rings[b].1, r, app.spec.gap) {
                                let c = Placed { part: i, x: p[0] - lc[0], y: p[1] - lc[1], w: fw, h: fh, turns: 0, mirror: false, spin: Default::default() };
                                if app.ghost_ok(&c) {
                                    spot = Some(p);
                                    break 'find;
                                }
                            }
                        }
                    }
                    let spot = spot.or(lifted.map(|l| [l.x + lc[0], l.y + lc[1]]));
                    if let (Some(p), Some(g)) = (spot, app.ghost.as_mut()) {
                        g.at = Some([p[0] + 9.0, p[1] + 7.0]);
                    }
                }
            }
            if want_cutoff {
                // A stepped remnant line hugging the parts, a free cut path, and
                // the straight-line tool half-drawn.
                app.tab = Tab::Sheet;
                app.exact_remnant_edge();
                let (w, h) = (app.cur_spec().w, app.cur_spec().h);
                let x = app.cut_list().first().map_or(w * 0.8, |l| l.pts.iter().map(|p| p[0]).fold(0.0, f64::max));
                let path = vec![[x + 60.0, h * 0.62], [w - 60.0, h * 0.62], [w - 60.0, h * 0.9]];
                app.add_cut_line(cutoff::CutLine { pts: path, kind: cutoff::CutKind::Free });
                app.sel_cut = Some(0);
                if std::env::args().any(|a| a == "--export") {
                    app.export();
                }
            }
            if args.iter().any(|a| a == "--overlap") {
                // Collision checking off and the second part pushed onto the
                // first: the red outlines and the badge (QR5, QR6).
                app.collision = false;
                if let Some(s) = app.result.sheets.first_mut().filter(|s| s.len() > 1) {
                    s[1].x = s[0].x + s[0].w * 0.5;
                    s[1].y = s[0].y + s[0].h * 0.3;
                }
                app.refresh();
            }
            // --window="x,y;x,y" (screenshots): a selection window on sheet 1
            // being dragged from the first corner to the second, sheet mm, with
            // the parts it takes selected (HM7, SW1).
            if let Some(w) = args.iter().find_map(|a| a.strip_prefix("--window=")) {
                let c: Vec<P> = w.split(';').filter_map(|q| q.split_once(',')).filter_map(|(x, y)| Some([x.trim().parse().ok()?, y.trim().parse().ok()?])).collect();
                if let (2, Some(sheet)) = (c.len(), app.result.sheets.first()) {
                    let sel = engine::arrange::window_select(&app.parts, sheet, &engine::selection::Window::dragged(c[0], c[1]));
                    app.set_selection(sel);
                    app.window_sel = Some((c[0], c[1], false));
                }
            }
            if want_radial {
                // The Part view's ring, or on the sheet the part ring with a
                // selection (--select=…) and the sheet ring without.
                let ring = if app.workspace == Workspace::Part {
                    radial::Ring::Part
                } else if args.iter().any(|a| a.starts_with("--select=")) {
                    radial::Ring::OnPart
                } else {
                    radial::Ring::Sheet
                };
                app.radial = Some(radial::Radial::new(egui::Pos2::new(1060.0, 640.0), ring));
            }
            if want_float {
                // Show a floating, a collapsed and a docked-right pane.
                let ws = app.workspace;
                let first = dock::PaneId::for_workspace(ws)[1];
                app.dock.apply_drop(ws, first, dock::Drop::Float(Pos2::new(1200.0, 300.0)));
                app.dock.toggle_auto_hide(dock::PaneId::for_workspace(ws)[0]);
            }
            if want_tech {
                app.tab = Tab::Technology;
                app.show_path = true;
                app.zoom = 3.0;
            }
            // --path (screenshots): the sheet's cut path, at the zoom it opens with.
            if std::env::args().any(|a| a == "--path") {
                app.show_path = true;
            }
            // --about-qualities (screenshots): "About cut qualities" starts open (CQ8).
            if std::env::args().any(|a| a == "--about-qualities") {
                cutpanel::ABOUT_OPEN.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            // --demo-qualities (screenshots): the active machine writes LC5
            // programs and the shop has named qualities 1, 2, 3 and 9 and said
            // what each is for (CQ7), as Settings → Machines keeps them.
            if std::env::args().any(|a| a == "--demo-qualities") {
                let k = app.cfg.active_machine.min(app.cfg.machines.len().saturating_sub(1));
                if let Some(m) = app.cfg.machines.get_mut(k) {
                    m.name = "Demo LC5".into();
                    m.output = settings::Output::BlmSiemens;
                    for (q, name, note) in [
                        (1, "Small holes", "Holes under about 10 mm that must come out round"),
                        (2, "Medium holes", "Slots and holes up to about 30 mm"),
                        (3, "Outsides", "Outlines and big cut-outs, fastest"),
                        (9, "Marking", "Etched part numbers and bend marks"),
                    ] {
                        m.qualities.names[q - 1] = name.into();
                        m.qualities.notes[q - 1] = note.into();
                    }
                    // And picks small holes at 1, medium ones at 2 on Auto (CQ9).
                    let rule = |r_from: f64, r_to: f64, q: usize| crate::qualities::SizeRule { r_from, r_to, quality: nest::CutQuality::new(q).unwrap_or(nest::CutQuality::FALLBACK), ..Default::default() };
                    m.sizes = vec![rule(0.0, 5.0, 1), rule(5.0, 15.0, 2)];
                }
                app.apply_settings(true);
            }
            // --cut-ranges (screenshots): Quality by size on the plate with holes,
            // its rows and each contour's quality (CQ1) by the machine's names
            // and colours (CQ4, CQ5): rows at qualities 1, 2 and 3; on a job
            // opened with --open-job, its first part.
            if std::env::args().any(|a| a == "--cut-ranges") {
                app.tab = Tab::Technology;
                app.sel_part = app.parts.iter().position(|p| p.name == "Square hole").or((!app.parts.is_empty()).then_some(0));
                app.tech_nest_all = Some(false);
                app.show_cut_ranges = true;
                if let Some(t) = app.sel_part.and_then(|i| app.parts[i].tech.as_mut()) {
                    for (k, (from, to)) in [(0.0, 5.0), (5.0, 50.0), (50.0, 150.0)].into_iter().enumerate() {
                        if let Some(q) = crate::nest::CutQuality::new(k + 1) {
                            t.cut_ranges[k] = crate::nest::CutRange { on: true, min_r: from, max_r: to, quality: q };
                        }
                    }
                }
            }
            // --joints=joint|count (screenshots): the Micro-joints window on that
            // tab for the plate with holes; --joint-type=gap|square|angled|corner
            // picks the part's joint type, --joint-rules sets demo size rules.
            if let Some(tab) = std::env::args().find_map(|a| a.strip_prefix("--joints=").map(|s| s.to_string())) {
                app.tab = Tab::Technology;
                app.sel_part = app.parts.iter().position(|p| p.name == "Square hole").or((!app.parts.is_empty()).then_some(0));
                app.tech_nest_all = Some(false);
                app.show_joint_sizes = true;
                app.joint_tab = u8::from(tab == "count");
                app.auto_joints = crate::ct1::for_spec(&app.spec).joint_sizes;
                if std::env::args().any(|a| a == "--joint-rules") {
                    app.auto_joints = nest::AutoJoints { small: [50.0, 50.0], parts: [[0, 2], [-1, -1], [-1, -1]], holes: [[0, 0], [-1, -1], [-1, -1]], ..nest::AutoJoints::default() };
                }
                let jt = match std::env::args().find_map(|a| a.strip_prefix("--joint-type=").map(|s| s.to_string())).as_deref() {
                    Some("square") => nest::JointType::Leads,
                    Some("angled") => nest::JointType::LeadsInclined,
                    Some("corner") => nest::JointType::Corner,
                    _ => nest::JointType::Gap,
                };
                if let Some(i) = app.sel_part {
                    let mut t = app.parts[i].tech.unwrap_or_default();
                    t.joint_type = jt;
                    t.joint_every = 100.0;
                    app.parts[i].tech = Some(t);
                }
            }
            // --paint-panel / --paint-contours (screenshots): the cut technology
            // panel on the first imported part with Slow (1) chosen and All the
            // same contours on; --paint-contours also paints, through the same
            // engine function as contour-tech, its bend lines Mark, its first
            // hole Leave out and every hole equal to its second Slow (1) (CS6,
            // CS7). Without --part the sheet shows the result. --paint-card=<row>
            // (cut, slow, mark, no-cut, bend-line, leave-out) shows a row's hover card.
            let paint_panel = std::env::args().any(|a| a == "--paint-panel" || a == "--paint-contours");
            if let Some(i) = (0..app.parts.len()).find(|&i| app.parts[i].shape == Shape::Custom).filter(|_| paint_panel) {
                use crate::cutpanel::Row;
                let slow = Row::Cut(nest::CutQuality::new(1));
                if std::env::args().any(|a| a == "--paint-contours") {
                    let list = engine::contours::list(&app.parts[i]);
                    let bends: Vec<usize> = (0..list.len()).filter(|&k| list[k].state == dxf::ContourState::BendLine).collect();
                    app.paint_contours(i, &bends, Row::Mark, false);
                    app.paint_contours(i, &[0], Row::LeaveOut, false);
                    app.paint_contours(i, &[1], slow, true);
                }
                app.cut_panel.open = true;
                app.cut_panel.same = true;
                app.cut_panel.row = Some(slow);
                app.cut_panel.card = std::env::args().find_map(|a| a.strip_prefix("--paint-card=").map(str::to_string)).and_then(|r| match r.as_str() {
                    "cut" => Some(Row::Cut(None)),
                    "slow" => Some(slow),
                    "mark" => Some(Row::Mark),
                    "no-cut" => Some(Row::NoCut),
                    "bend-line" => Some(Row::BendLine),
                    "leave-out" => Some(Row::LeaveOut),
                    _ => None,
                });
                if app.workspace == Workspace::Part {
                    app.tab = Tab::Technology;
                }
            }
            // --lead-offer / --lead-same (screenshots): the Clip (a flange plate
            // with four equal corner holes) in the Part view with its first
            // hole's lead-in moved by hand through the engine (LE8): alone,
            // with Apply to them offered, or with All the same contours on, so
            // all four holes take it.
            let (lead_offer, lead_same) = (std::env::args().any(|a| a == "--lead-offer"), std::env::args().any(|a| a == "--lead-same"));
            if let Some(i) = app.parts.iter().position(|p| p.name == "Clip").filter(|_| lead_offer || lead_same) {
                app.open_part(i);
                app.lead_same = lead_same;
                let spec = app.spec.clone();
                if let Ok(m) = engine::set_lead_end(&mut app.parts, i, &spec, 0, engine::LeadEnd::In, Some(3.0), Some(40.0), lead_same) {
                    app.status = m;
                }
                if lead_offer {
                    app.lead_offer = Some((i, 0, engine::same_holes(&app.parts[i], 0).len()));
                }
                app.refresh();
            }
            // --sheet-tab (screenshots): the Sheet tab, e.g. its machine-table line.
            if std::env::args().any(|a| a == "--sheet-tab") {
                app.tab = Tab::Sheet;
            }
            // --material-open=<picker>[:<text>] (screenshots): a Material ▾
            // open (sheet_tab, new_sheet), with <text> typed in its find
            // box; --material-find=<text> types in the stock shelf's.
            if let Some(v) = std::env::args().find_map(|a| a.strip_prefix("--material-open=").map(str::to_string)) {
                let (salt, text) = v.split_once(':').unwrap_or((v.as_str(), ""));
                crate::matpick::open_for_shot(&cc.egui_ctx, salt, text);
            }
            if let Some(v) = std::env::args().find_map(|a| a.strip_prefix("--material-find=").map(str::to_string)) {
                crate::matpick::type_for_shot(&cc.egui_ctx, "shelf", &v);
            }
            if want_jobs {
                app.workspace = Workspace::Jobs;
            }
            // --export-review / --part-check (screenshots): the Export CNC
            // review, or Verify machining's result on the opened part.
            // --machines-demo (screenshots): made-up machines, each with its own
            // programs folder, for the review's Also write for (EXP4).
            if app.shot.is_some() && std::env::args().any(|a| a == "--machines-demo") {
                let m = |name: &str, output, kind, dir: &str| settings::Machine { name: name.into(), output, kind, programs_dir: dir.into(), ..Default::default() };
                use crate::machine::MachineKind::{Laser, Waterjet};
                app.cfg.machines = vec![
                    m("BLM 1", settings::Output::BlmSiemens, Laser, r"\\srv\cnc\BLM 1"),
                    m("BLM 2", settings::Output::BlmSiemens, Laser, r"\\srv\cnc\BLM 2"),
                    m("Fiber laser", settings::Output::Iso, Laser, ""),
                    m("Waterjet", settings::Output::Omax, Waterjet, r"\\srv\cnc\Waterjet"),
                ];
                app.cfg.export_dir = r"\\srv\cnc\Programs".into();
                app.cfg.active_machine = 0;
                app.apply_settings(true);
            }
            if std::env::args().any(|a| a == "--export-review") {
                app.export_checked();
            }
            if std::env::args().any(|a| a == "--part-check") {
                app.verify_machining_now();
            }
            // --import-card (screenshots): the card after importing the last
            // two parts' drawings. --open-job-menu: the Open job list, with a
            // job file and a Library job saved under the (temporary) home.
            // --import=<drawing>: import it as if dropped on the window (the
            // Import drawings window opens on a drawing with several rows, IM12).
            if let Some(p) = std::env::args().find_map(|a| a.strip_prefix("--import=").map(PathBuf::from)).filter(|p| p.is_file()) {
                app.import_drawings(vec![p]);
            }
            if std::env::args().any(|a| a == "--import-card") {
                // Two new drawings (made-up references), not yet nested.
                for (k, name) in ["DEMO-101", "DEMO-102"].into_iter().enumerate() {
                    if let Some(mut p) = app.parts.get(k * 3).cloned() {
                        p.name = name.into();
                        p.qty = [6, 4][k];
                        p.color = app.color_seq_next();
                        app.parts.push(p);
                    }
                }
                app.finish_import(2.min(app.parts.len()), vec![]);
            }
            if std::env::args().any(|a| a == "--open-job-menu") {
                let dir = library::documents_dir();
                let _ = std::fs::create_dir_all(&dir);
                app.save_job_to(&dir.join("DEMO-002 base plates.snjob"));
                app.save_job_to_library();
                app.demo_open_menu = true;
            }
            // Screenshots of a part's cut order (CO11–CO13), part mm, on the
            // part opened with --part: --order-pick turns Manual Contours on;
            // --order-click=x,y;x,y, --order-box=x0,y0;x1,y1 and
            // --order-sweep=x,y;x,y;… (sweeps apart with |) pick through the
            // screen's own functions, in that order (--order-sweeping leaves
            // the last sweep held); --order-menu shows the
            // Contours menu open, --order-quality its quality list and
            // --order-card=<manual|automatic|parts|all-marks|all-cut|quality>
            // an item's hover card.
            if let Some(i) = app.shown_part().filter(|_| app.workspace == Workspace::Part) {
                let arg = |name: &str| std::env::args().find_map(|a| a.strip_prefix(name).map(str::to_string));
                let flag = |name: &str| std::env::args().any(|a| a == name);
                let pts = |s: &str| -> Vec<[f64; 2]> {
                    s.split(';')
                        .filter_map(|q| {
                            let (x, y) = q.split_once(',')?;
                            Some([x.trim().parse().ok()?, y.trim().parse().ok()?])
                        })
                        .collect()
                };
                if part_tab.as_deref() == Some("machining") {
                    app.tab = Tab::Machining;
                }
                let (click, sweep, boxed) = (arg("--order-click="), arg("--order-sweep="), arg("--order-box="));
                if flag("--order-pick") || click.is_some() || sweep.is_some() || boxed.is_some() {
                    app.order_pick_start();
                }
                for q in click.as_deref().map(pts).unwrap_or_default() {
                    app.order_click(i, q, 1.0);
                }
                if let Some(q) = boxed.as_deref().map(pts).filter(|q| q.len() == 2) {
                    app.order_box(i, &crate::engine::selection::Window::dragged(q[0], q[1]));
                }
                let sweeps: Vec<Vec<[f64; 2]>> = sweep.as_deref().map_or(vec![], |s| s.split('|').map(pts).collect());
                for (k, q) in sweeps.iter().enumerate() {
                    let Some((first, rest)) = q.split_first() else { continue };
                    app.order_sweep_start(i, *first, 1.0);
                    for &q in rest {
                        app.order_sweep_to(q, 1.0);
                    }
                    if k + 1 < sweeps.len() || !flag("--order-sweeping") {
                        app.order_sweep_end();
                    }
                }
                app.order_pick.demo_quality = flag("--order-quality");
                app.order_pick.demo_card = arg("--order-card=");
                app.order_pick.demo_menu = flag("--order-menu") || app.order_pick.demo_quality || app.order_pick.demo_card.is_some();
            }
            // Placing leads by hand (screenshots, LE10–LE12), on the part shown
            // (--part-name=<name> opens that one first), through the engine as
            // the clicks do: --lead-scope=<all|holes|outline> picks the ribbon's
            // row, --lead-delete runs Delete lead-ins with it on that part,
            // --lead-slide=<contour>:<x>,<y> drags a lead's foot there,
            // --lead-add=<contour>:<x>,<y> is the ring's Lead here, and
            // --lead-remove=<contour> its card's Remove lead. Each shows the
            // Technology ribbon.
            {
                let arg = |k: &str| std::env::args().find_map(|a| a.strip_prefix(k).map(str::to_string));
                if let Some(i) = arg("--part-name=").and_then(|n| app.parts.iter().position(|p| p.name == n)) {
                    app.open_part(i);
                    app.sel_part = Some(i);
                    if let Some(z) = arg("--part-zoom=").and_then(|v| v.parse::<f32>().ok()) {
                        app.part_zoom = z.clamp(crate::partview::PART_ZOOM_MIN, crate::partview::PART_ZOOM_MAX);
                    }
                }
                let at = |v: &str| -> Option<(usize, P)> {
                    let (c, xy) = v.split_once(':')?;
                    let (x, y) = xy.split_once(',')?;
                    Some((c.parse().ok()?, [x.parse().ok()?, y.parse().ok()?]))
                };
                if ["--lead-scope=", "--lead-slide=", "--lead-add=", "--lead-remove="].iter().any(|k| arg(k).is_some()) {
                    app.tab = Tab::Technology;
                }
                // --modify=<break|cut1|cut-several|join|bridge> picks up a Break
                // elements tool on the part shown; --modify-click=x,y;x,y clicks
                // the part there (part mm) as the canvas does (BE1);
                // --modify-width=mm sets the bridge width first.
                if let Some(m) = arg("--modify=") {
                    use crate::drawing::Modify;
                    let tool = match m.as_str() {
                        "break" => Some(Modify::Break),
                        "cut1" => Some(Modify::Cut1),
                        "cut-several" => Some(Modify::CutSeveral),
                        "join" => Some(Modify::Join),
                        "bridge" => Some(Modify::Bridge),
                        _ => None,
                    };
                    app.set_modify(tool);
                    if let Some(w) = arg("--modify-width=").and_then(|v| v.parse::<f64>().ok()) {
                        app.dv.bridge_width = w;
                    }
                    let clicks = arg("--modify-click=").unwrap_or_default();
                    for xy in clicks.split(';').filter(|t| !t.trim().is_empty()) {
                        if let Some((x, y)) = xy.split_once(',').and_then(|(x, y)| Some((x.trim().parse::<f64>().ok()?, y.trim().parse::<f64>().ok()?))) {
                            app.modify_click([x, y], 1.0);
                        }
                    }
                    // --modify-hover=x,y: the pointer is taken to be there, for
                    // the preview of the next click (BE15).
                    app.dv.shot_hover = arg("--modify-hover=").and_then(|v| v.split_once(',').and_then(|(x, y)| Some([x.trim().parse::<f64>().ok()?, y.trim().parse::<f64>().ok()?])));
                }
                // --part-pan-px=<x>,<y>: the Part view moved by that many pixels.
                if let Some((x, y)) = arg("--part-pan-px=").and_then(|v| v.split_once(',').and_then(|(x, y)| Some((x.parse::<f32>().ok()?, y.parse::<f32>().ok()?)))) {
                    app.part_pan = Vec2::new(x, y);
                }
                if let Some(sc) = arg("--lead-scope=") {
                    app.lead_scope = match sc.as_str() {
                        "holes" => nest::LeadScope::Holes,
                        "outline" => nest::LeadScope::Outline,
                        _ => nest::LeadScope::All,
                    };
                    app.tab = Tab::Technology;
                }
                if let Some(idx) = app.shown_part() {
                    let spec = app.spec.clone();
                    let u = app.unit();
                    if std::env::args().any(|a| a == "--lead-delete") {
                        app.tech_all = false;
                        app.tech_nest_all = Some(false);
                        let scope = app.lead_scope;
                        app.edit_tech("Lead-ins removed", |t, h| engine::TechEdit::DeleteLeads(scope).apply(t, h));
                    }
                    if let Some((ci, p)) = arg("--lead-slide=").and_then(|v| at(&v)) {
                        app.status = engine::move_lead_start(&mut app.parts, idx, &spec, ci, p, 0.0, false, u).unwrap_or_else(|e| e);
                    }
                    if let Some((ci, p)) = arg("--lead-add=").and_then(|v| at(&v)) {
                        app.status = engine::add_lead_at(&mut app.parts, idx, &spec, ci, p, false, u).unwrap_or_else(|e| e);
                    }
                    if let Some(ci) = arg("--lead-remove=").and_then(|v| v.parse::<usize>().ok()) {
                        app.status = engine::remove_lead(&mut app.parts, idx, &spec, ci, false).unwrap_or_else(|e| e);
                    }
                    app.refresh();
                }
            }
            // --lead-menu=<machine|perpendicular|line|arc|line-arc|corner>
            // (screenshots): the Technology tab with the lead Type picker
            // open, previewing that type.
            if let Some(t) = std::env::args().find_map(|a| a.strip_prefix("--lead-menu=").and_then(leadmenu::named)) {
                app.tab = Tab::Technology;
                app.lead_menu_demo = Some(t);
            }
            // --lead-pick=<contour> (screenshots, with --part): that cut
            // contour's lead clicked on, its card shown (LE9); with
            // --lead-shape=<type> given that shape through the engine first,
            // with --lead-card-menu its shape menu open.
            if let (Some(idx), Some(ci)) = (app.shown_part(), std::env::args().find_map(|a| a.strip_prefix("--lead-pick=").and_then(|v| v.parse::<usize>().ok()))) {
                if let Some(k) = std::env::args().find_map(|a| a.strip_prefix("--lead-shape=").and_then(leadmenu::named)) {
                    let spec = app.spec.clone();
                    match engine::set_lead_type(&mut app.parts, idx, &spec, ci, k, false) {
                        Ok(m) | Err(m) => app.status = m,
                    }
                    app.refresh();
                }
                app.sel_lead = Some((idx, ci));
                app.lead_card_demo = std::env::args().any(|a| a == "--lead-card-menu");
                app.tab = Tab::Technology;
            }
            // --tech-result (screenshots): Auto lead-ins run on one part type,
            // with its result card and "Apply to all".
            if std::env::args().any(|a| a == "--tech-result") {
                app.tab = Tab::Technology;
                app.sel_part = Some(1.min(app.parts.len().saturating_sub(1)));
                app.tech_all = false;
                app.tech_nest_all = Some(false);
                app.run_tech(techui::TechRerun::AutoLeads);
            }
            Ok(Box::new(app))
        }),
    );
    if let Err(e) = &res {
        eprintln!("window error: {e}");
    }
    // UP14: the window has closed, and every job with it: an update chosen
    // with Update when I close installs now.
    updates::install_on_close();
    res
}

/// A short "what just happened" card on the drawing, with the next steps.
#[derive(Clone, Debug)]
pub struct ResultCard {
    pub title: String,
    pub lines: Vec<String>,
    /// Something went wrong or needs a look (orange, not green).
    pub warn: bool,
    /// Buttons: the first is the main one.
    pub actions: Vec<(String, CardAct)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CardAct {
    /// Run the technology command again on every part type.
    TechAll(techui::TechRerun),
    NestAll,
    /// Pick this part and bring its tile into view in Parts View.
    ShowPart(usize),
    /// The Sheet tab: material, thickness, sheet.
    SheetTab,
}

impl CardAct {
    fn tip(self) -> &'static str {
        match self {
            CardAct::TechAll(_) => "Run the same command on every part type in the job (Ctrl + Z undoes it)",
            CardAct::NestAll => "Nest all the job's parts (F5)",
            CardAct::ShowPart(_) => "Pick the first new part and show its tile in Parts View",
            CardAct::SheetTab => "Open the Sheet tab: material, thickness and sheet size",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_clashes(a: &App) {
        for s in &a.result.sheets {
            for (i, p) in s.iter().enumerate() {
                assert!(a.spec.fits(p), "part leaves sheet");
                for q in &s[i + 1..] {
                    assert!(!nest::clash_exact(&a.parts, p, q, a.spec.gap - 1e-6), "parts overlap");
                }
            }
        }
    }

    /// EXP3, EXP4, EXP6: the Export review's Also write for, as a person
    /// ticks it: the same sheets for a second laser in its own folder; a
    /// waterjet greyed out; the next export for one machine only takes the
    /// other's programs away, said in the review first.
    #[test]
    fn the_export_review_writes_the_same_sheets_for_a_ticked_machine_too() {
        let d = crate::testdir::TestDir::new("also_write_for");
        let mut a = App::new(None);
        std::fs::create_dir_all(d.join("Lantek").join("MchFiles")).unwrap();
        std::fs::write(d.join("Lantek").join("MchFiles").join("PstAdi06.CT1"), "# Demo laser\nB056 1.0\n").unwrap();
        let folder = |n: &str| d.join(n).to_string_lossy().into_owned();
        a.cfg.lantek_dir = folder("Lantek");
        a.cfg.export_dir = folder("shared");
        // Each laser on a post of its own with its technology file beside it.
        std::fs::create_dir_all(d.join("posts")).unwrap();
        let m = |name: &str, dir: String| {
            let post = d.join("posts").join(format!("{}.CFF", name.replace(' ', "_")));
            std::fs::write(&post, "").unwrap();
            std::fs::write(post.with_extension("CT1"), "# Demo laser\nB056 1.0\n").unwrap();
            settings::Machine { name: name.into(), programs_dir: dir, post_lib: post.to_string_lossy().into_owned(), ..Default::default() }
        };
        let waterjet = settings::Machine { name: "Demo waterjet".into(), kind: crate::machine::MachineKind::Waterjet, output: settings::Output::Omax, programs_dir: folder("m4"), ..Default::default() };
        a.cfg.machines = vec![m("Demo laser 1", folder("m1")), m("Demo laser 2", folder("m2")), m("Demo laser 3", String::new()), waterjet];
        // The controls' folders are there; one that isn't would be said (EXP6).
        for n in ["m1", "m2", "m4", "shared"] {
            std::fs::create_dir_all(d.join(n)).unwrap();
        }
        a.cfg.active_machine = 0;
        a.job_name = "DEMO-001".into();
        a.parts = vec![Part { name: "A".into(), shape: Shape::Rectangle, w: 300.0, h: 200.0, qty: 2, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        a.run_nesting();
        let programs = |n: &str| std::fs::read_dir(d.join(n).join("DEMO-001")).map_or(0, |rd| rd.flatten().filter(|f| f.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("cnc"))).count());
        a.export_checked();
        let Some(Pending::ExportReview { also, earlier, .. }) = a.pending.clone() else { panic!("no review") };
        assert_eq!(also.iter().map(|r| r.machine).collect::<Vec<_>>(), [1, 2, 3]);
        assert!(also[0].ready.is_ok() && also[1].ready.is_ok(), "{:?}", also.iter().map(|r| r.ready.clone()).collect::<Vec<_>>());
        assert!(also[2].ready.as_ref().unwrap_err().contains("waterjet"));
        assert!(earlier.is_empty());
        a.export_with(&[1]);
        assert!(programs("m1") > 0 && programs("m2") > 0 && programs("shared") == 0, "{}", a.status);
        assert!(a.status.contains("Demo laser 2:") && a.status.contains("one machine only"), "{}", a.status);
        assert!(a.export_also_lines.iter().any(|(l, ok)| *ok && l.starts_with("Demo laser 2:")), "{:?}", a.export_also_lines);
        // Opened again, the second laser starts ticked: its folder has the
        // job's programs. Unticked, the review says they will go.
        a.pending = None;
        a.export_checked();
        let Some(Pending::ExportReview { also, earlier, .. }) = a.pending.clone() else { panic!("no review") };
        assert!(also[0].ticked && !also[1].ticked && earlier.is_empty());
        if let Some(Pending::ExportReview { also, .. }) = a.pending.as_mut() {
            also[0].ticked = false;
        }
        a.review_earlier();
        let Some(Pending::ExportReview { earlier, .. }) = a.pending.clone() else { panic!("no review") };
        assert_eq!(earlier.iter().map(|e| e.machines.clone()).collect::<Vec<_>>(), [vec![1]]);
        a.export();
        assert!(programs("m1") > 0 && programs("m2") == 0, "{}", a.status);
        assert!(a.status.contains("Removed this job's") && a.status.contains("Demo laser 2's folder"), "{}", a.status);
    }

    #[test]
    fn copy_compact_undo_keep_layout_valid() {
        let mut a = App::new(None);
        assert!(a.undo.is_empty());
        // A half-empty sheet, so the copy surely has room.
        a.parts = vec![
            Part { name: "A".into(), shape: Shape::Rectangle, w: 300.0, h: 200.0, qty: 3, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] },
            Part { name: "B".into(), shape: Shape::Disc, w: 150.0, h: 150.0, qty: 4, rotate: true, color: 1, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] },
        ];
        a.run_nesting();
        a.compact();
        no_clashes(&a);
        let placed = a.total_placed();
        a.cur_sheet = 0;
        a.sel_placed = Some(0);
        a.duplicate_selected();
        no_clashes(&a);
        assert_eq!(a.total_placed(), placed + 1, "the copy found a free spot");
        a.undo();
        assert_eq!(a.total_placed(), placed);
        a.redo();
        assert_eq!(a.total_placed(), placed + 1);
        no_clashes(&a);
    }

    #[test]
    fn compact_undo_and_delete_leave_nothing_pointing_at_the_wrong_part() {
        let mut a = App::new(None);
        a.run_nesting();
        a.cur_sheet = 0;
        // Compact on a remnant would slide parts off its outline: refused.
        let before = a.result.sheets[0].clone();
        let (w, h) = (a.spec.w, a.spec.h);
        a.result.formats[0].outline = Some(crate::stock::Outline { outer: vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]], holes: vec![] });
        a.compact();
        assert!(a.status.contains("rectangular") && a.result.sheets[0] == before, "{}", a.status);
        a.result.formats[0].outline = None;
        // Undo with a just-added part still on the pointer: the pointer lets go.
        a.checkpoint();
        a.parts.push(a.parts[0].clone());
        a.start_ghost(a.parts.len() - 1, false);
        a.undo();
        assert!(a.ghost.is_none());
        // Taking a part off the sheet renumbers the rest: its hand picks go.
        a.manual_cuts = vec![vec![cnc::ManualCut { placed: 1, contour: 0, start: [0.0, 0.0], reverse: false, whole: None }]];
        a.set_selection(vec![0]);
        a.remove_selected();
        assert!(a.manual_cuts.iter().all(|m| m.is_empty()));
    }

    #[test]
    fn a_selection_moves_turns_copies_and_goes_by_hand() {
        // HM1–HM10 as the keys and buttons run them.
        let mut a = App::new(None);
        let r = |name: &str, color| Part { name: name.into(), shape: Shape::Rectangle, w: 200.0, h: 100.0, qty: 3, rotate: true, color, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        a.parts = vec![r("A", 0), r("B", 1)];
        a.run_nesting();
        assert_eq!(a.total_placed(), 6);
        a.cur_sheet = 0;
        a.set_selection(vec![0, 1]);
        assert_eq!(a.selection(), vec![0, 1]);
        let undo0 = a.undo.len();
        // Ctrl + Up: they sit under a third part, so they stay; Ctrl + Right:
        // to the right margin, together.
        let before: Vec<Placed> = a.result.sheets[0].clone();
        a.nudge(engine::arrange::Dir::Up, true, 10.0);
        assert_eq!(a.result.sheets[0], before, "{}", a.status);
        assert!(a.status.contains("they are at the part gap"), "{}", a.status);
        a.nudge(engine::arrange::Dir::Right, true, 10.0);
        let after = &a.result.sheets[0];
        let dx = after[0].x - before[0].x;
        assert!((after[0].x + after[0].w - (a.spec.w - a.spec.margin)).abs() < 1e-6, "{:?}", after[0]);
        assert!(dx > 0.0 && (after[1].x - before[1].x - dx).abs() < 1e-9, "moved together");
        no_clashes(&a);
        // Alt + Left twice in a row: one undo step, 2 × the step.
        let x = a.result.sheets[0][0].x;
        a.nudge(engine::arrange::Dir::Left, false, 20.0);
        a.nudge(engine::arrange::Dir::Left, false, 20.5);
        assert!((x - a.result.sheets[0][0].x - 2.0 * a.cfg.nesting.step).abs() < 1e-6);
        assert_eq!(a.undo.len(), undo0 + 2, "Ctrl+Right, then the Alt+Left run");
        // Turn, copy and remove the pair.
        a.turn_selected(1);
        assert!(a.selection().iter().all(|&i| a.result.sheets[0][i].turns == 1));
        no_clashes(&a);
        a.duplicate_selected();
        assert_eq!(a.total_placed(), 8);
        assert_eq!(a.selection(), vec![6, 7], "the copies are selected");
        no_clashes(&a);
        a.remove_selected();
        assert_eq!(a.total_placed(), 6);
        assert!(a.selection().is_empty());
        a.select_all_on_sheet();
        assert_eq!(a.selection().len(), 6);
    }

    #[test]
    fn alt_arrows_force_and_ctrl_arrows_always_keep_the_gap() {
        // HM2, HM3: the step move doesn't test for parts; the slide always does,
        // even with Collision off.
        let mut a = App::new(None);
        let r = Part { name: "A".into(), shape: Shape::Rectangle, w: 100.0, h: 100.0, qty: 2, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        a.parts = vec![r];
        a.spec.gap = 6.35;
        a.result = NestResult { sheets: vec![vec![Placed { part: 0, x: 100.0, y: 100.0, w: 100.0, h: 100.0, turns: 0, mirror: false, spin: Default::default() }, Placed { part: 0, x: 210.0, y: 100.0, w: 100.0, h: 100.0, turns: 0, mirror: false, spin: Default::default() }]], formats: vec![nest::SheetFormat::rect(2000.0, 1000.0)], method: String::new(), tried: 0 };
        a.cur_sheet = 0;
        a.collision = false;
        a.set_selection(vec![1]);
        a.nudge(engine::arrange::Dir::Left, true, 1.0);
        assert!((a.result.sheets[0][1].x - 206.35).abs() < 1e-6, "Ctrl+Left stops at the gap: {:?}", a.result.sheets[0][1]);
        a.collision = true;
        a.cfg.nesting.step = 5.0;
        a.nudge(engine::arrange::Dir::Left, false, 2.0);
        assert!((a.result.sheets[0][1].x - 201.35).abs() < 1e-6, "Alt+Left moves the whole step");
        assert!(a.status.starts_with('⚠') && a.status.contains("#1"), "{}", a.status);
        a.nudge(engine::arrange::Dir::Right, true, 3.0);
        assert!(a.result.sheets[0][1].x > 1500.0, "Ctrl+Right to the margin");
    }
}

#[cfg(test)]
mod job_tests {
    use super::*;

    /// A version one major release ahead of this program's, and a shop
    /// `Acme` on it in a new Library under `dir`.
    fn newer_shop(dir: &std::path::Path) -> (String, std::path::PathBuf) {
        let major: u64 = updates::VERSION.split('.').next().unwrap().parse().unwrap();
        let newer = format!("{}.0.0", major + 1);
        let _ = std::fs::remove_dir_all(dir);
        let lib = dir.join("library");
        crate::library::create(&lib).unwrap();
        engine::shop::start(&lib, "Acme", &shared::Who::new("sam", "PC-A"), &shared::Clock::at(1_794_216_000), &newer).unwrap();
        let f = lib.join(shop::FILE);
        let mut v: serde_json::Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
        v[shop::SHOP_VERSION] = serde_json::json!(newer);
        std::fs::write(&f, v.to_string()).unwrap();
        (newer, lib)
    }

    /// UP26 (review S1): the shop is rolled back while this window's save
    /// waits for the write lock: the shop version is read again under the
    /// lock, so the window, now on the release left, writes nothing into the
    /// shop folder and books nothing.
    #[test]
    fn a_roll_back_landing_while_a_save_waits_leaves_the_shop_untouched() {
        let dir = crate::testdir::TestDir::new("save_during_rollback");
        let lib = dir.join("Nesting");
        library::create(&lib).unwrap();
        let sam = shared::Who::new("sam", "PC-A");
        // The shop rolled out an older release, then this program's.
        let older = "0.0.9";
        engine::shop::start(&lib, "Acme", &sam, &shared::Clock::at(1_794_216_000), older).unwrap();
        engine::rollout::rollout(&lib, &engine::rollout::fake_release(&dir, older), &sam, older).unwrap();
        engine::rollout::rollout(&lib, &engine::rollout::fake_release(&dir, updates::VERSION), &sam, updates::VERSION).unwrap();
        let mut a = App::new(None);
        a.cfg.library_dir = lib.display().to_string();
        stock::Stock::update(&a.library_at(), |s| s.add_sheet(stock::Item::sheet("DEMO-S1", a.spec.material, a.spec.thickness, 3000.0, 1500.0, 3), true)).unwrap();
        a.add_part(Shape::Rectangle);
        a.run_nesting();
        let stock_before = std::fs::read(stock::Stock::file(&lib)).unwrap();
        let dest = lib.join("DEMO-001.snjob");
        let (l2, s2) = (lib.clone(), sam.clone());
        shared::hooks::set(move |step| {
            if step == "window save: checked" {
                engine::rollout::rollback(&l2, &s2, updates::VERSION).unwrap();
            }
        });
        a.save_job_to(&dest);
        shared::hooks::clear();
        assert_eq!(shop::read(&lib).unwrap().unwrap().shop_version(), Some(older));
        assert!(a.status.contains(&format!("Restart to go back to {older}")), "{}", a.status);
        assert!(!dest.exists(), "the window saved into the shop after it was rolled back");
        assert_eq!(std::fs::read(stock::Stock::file(&lib)).unwrap(), stock_before, "sheets were booked for a save that was refused");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_over_a_newer_job_file_books_no_sheets() {
        // S2 (UP7): the destination is checked before the stock is booked.
        let dir = crate::testdir::TestDir::new("newer_dest_books_nothing");
        let (newer, lib) = newer_shop(&dir);
        let mut a = App::new(None);
        a.cfg.library_dir = lib.display().to_string();
        stock::Stock::update(&a.library_at(), |s| s.add_sheet(stock::Item::sheet("DEMO-S1", a.spec.material, a.spec.thickness, 3000.0, 1500.0, 3), true)).unwrap();
        a.add_part(Shape::Rectangle);
        a.run_nesting();
        assert!(a.result.formats.iter().any(|f| f.stock_ref == "DEMO-S1"), "not nested from stock");
        let dest = dir.join("DEMO-001.snjob");
        jobfile::save_on(&dest, &jobfile::JobFile::new_job("DEMO-001", engine::default_spec()), &shared::Versions::this(&newer), &lib).unwrap();
        let (stock_before, dest_before) = (std::fs::read(stock::Stock::file(&lib)).unwrap(), std::fs::read(&dest).unwrap());
        a.save_job_to(&dest);
        assert!(a.status.contains("read-only on this PC"), "{}", a.status);
        assert_eq!(std::fs::read(stock::Stock::file(&lib)).unwrap(), stock_before, "sheets were booked for a save that was refused");
        assert_eq!(std::fs::read(&dest).unwrap(), dest_before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_job_from_a_read_only_job_starts_a_job_of_its_own() {
        // S3 (UP9): never emptied in place, so Undo can't bring the newer
        // version's job back as one this PC saves.
        let dir = crate::testdir::TestDir::new("read_only_new_job");
        let (newer, lib) = newer_shop(&dir);
        let src = dir.join("DEMO-001.snjob");
        let mut j = jobfile::JobFile::new_job("DEMO-001", engine::default_spec());
        j.parts.push(engine::shape_part(Shape::Rectangle, 1, 0));
        jobfile::save_on(&src, &j, &shared::Versions::this(&newer), &lib).unwrap();
        let mut a = App::new(None);
        a.cfg.library_dir = lib.display().to_string();
        a.open_job_file(&src);
        let (read_only, jobs) = (a.cur_job, a.jobs.len());
        a.new_job();
        assert_eq!(a.jobs.len(), jobs + 1, "the read-only job was emptied in place");
        assert!(a.parts.is_empty() && a.read_only().is_none());
        a.undo();
        assert!(a.parts.is_empty(), "undo brought the newer job's parts into the new job");
        assert!(a.jobs[read_only].read_only.is_some(), "the newer job isn't read-only any more");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_window_says_when_a_library_part_is_a_newer_versions_and_when_it_cant_add_parts() {
        // S4 (UP9, UP7).
        let dir = crate::testdir::TestDir::new("newer_part_window");
        let (newer, lib) = newer_shop(&dir);
        let at_newer = crate::library::Library::at(&lib, &shared::Who::new("sam", "PC-A"), &shared::Clock::at(1_794_216_000)).on_version(&newer);
        let id = at_newer.save_product(&engine::shape_part(Shape::Rectangle, 1, 0), "Acme", 0, 3.0, crate::library::SaveTo::New).unwrap().id;
        let mut a = App::new(None);
        a.cfg.library_dir = lib.display().to_string();
        a.add_library_product(id);
        let why = format!("Made by version {newer}. Update this PC: the shop is on {newer}.");
        assert!(a.status.contains("read-only on this PC") && a.status.contains(&why), "{}", a.status);
        // Adding the job's parts to a Library whose list the newer version
        // wrote: refused, and said.
        a.add_part(Shape::Rectangle);
        a.parts.last_mut().unwrap().name = "DEMO-NEW".into();
        a.add_products_to_library();
        assert!(a.status.contains("0 product(s) added") && a.status.contains("Not added: ") && a.status.contains(&why), "{}", a.status);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_newer_save_that_changes_only_the_version_makes_the_open_job_read_only() {
        // S7 (UP9): the file watcher takes the version from the same read as
        // the job.
        let dir = crate::testdir::TestDir::new("newer_stamp_watch");
        let (newer, lib) = newer_shop(&dir);
        let path = dir.join("DEMO-001.snjob");
        let mut a = App::new(None);
        a.cfg.library_dir = lib.display().to_string();
        a.save_job_to(&path);
        assert!(a.read_only().is_none(), "{}", a.status);
        // The window takes the file as it is now as its own.
        if let (Some(l), Ok(j)) = (a.job_link.as_mut(), jobfile::load(&path)) {
            l.disk_sig = engine::imports::job_sig(&j);
        }
        // A newer version saves it again, changing nothing but the version.
        std::thread::sleep(std::time::Duration::from_millis(30));
        let text = std::fs::read_to_string(&path).unwrap();
        let stamp = format!("\"program_version\": \"{}\"", updates::VERSION);
        assert!(text.contains(&stamp), "{text}");
        std::fs::write(&path, text.replacen(&stamp, &format!("\"program_version\": \"{newer}\""), 1)).unwrap();
        a.disk_poll = -10.0;
        a.watch_job_file(&egui::Context::default());
        assert!(a.read_only().is_some_and(|w| w.contains(&newer)), "{:?} {}", a.read_only(), a.status);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_job_a_newer_version_saved_opens_read_only_in_the_window_and_is_never_saved() {
        // UP7, UP8, UP9: the window on this PC's own version; the job and the
        // shop's version are one major release ahead.
        let major: u64 = updates::VERSION.split('.').next().unwrap().parse().unwrap();
        let newer = format!("{}.0.0", major + 1);
        let dir = crate::testdir::TestDir::new("read_only_window");
        let lib = dir.join("library");
        crate::library::create(&lib).unwrap();
        engine::shop::start(&lib, "Acme", &shared::Who::new("sam", "PC-A"), &shared::Clock::at(1_794_216_000), &newer).unwrap();
        let f = lib.join(shop::FILE);
        let mut v: serde_json::Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
        v[shop::SHOP_VERSION] = serde_json::json!(newer);
        std::fs::write(&f, v.to_string()).unwrap();
        let src = dir.join("DEMO-001.snjob");
        jobfile::save_on(&src, &jobfile::JobFile::new_job("DEMO-001", engine::default_spec()), &shared::Versions::this(&newer), &lib).unwrap();
        let bytes = std::fs::read(&src).unwrap();
        let mut a = App::new(None);
        a.cfg.library_dir = lib.display().to_string();
        a.open_job_file(&src);
        let why = format!("Made by version {newer}. Update this PC: the shop is on {newer}.");
        assert_eq!(a.read_only().as_deref(), Some(why.as_str()));
        assert!(a.status.contains("read-only") && a.status.contains(&why), "{}", a.status);
        // Save, Save as and Save to library are refused, saying why.
        a.save_job_to(&src);
        assert!(a.status.starts_with("Not saved: this job is read-only on this PC.") && a.status.contains(&why), "{}", a.status);
        a.save_job_to(&dir.join("DEMO-001 copy.snjob"));
        assert!(!dir.join("DEMO-001 copy.snjob").exists());
        a.save_job_to_library();
        assert!(a.status.contains("read-only") && a.status.contains(&why), "{}", a.status);
        assert!(crate::library::Library::at(&lib, &a.who, &a.clock).entries().is_empty(), "it was saved into the Library");
        assert_eq!(std::fs::read(&src).unwrap(), bytes, "the window wrote over the newer job");
        // Another job, opened after it, isn't read-only.
        a.new_job_in_week("W12".into());
        assert_eq!(a.read_only(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_job_whose_material_isnt_in_the_list_keeps_its_name_saved_from_the_window() {
        // Invariant 3, MA17: a job in a material this PC's list doesn't have
        // shows `?`, and the window saves it back with the name it came with,
        // to a file and to the Library, after switching jobs too.
        let dir = crate::testdir::TestDir::made("unknown_material");
        let src = dir.join("DEMO-904.snjob");
        let mut j = jobfile::JobFile::new_job("DEMO-904", engine::default_spec());
        j.spec.material = 41;
        j.material_name = "DEMO 904L".into();
        jobfile::save(&src, &j).unwrap();
        let mut a = App::new(None);
        a.open_job_file(&src);
        assert_eq!((a.spec.material, engine::material::name_or(a.spec.material, "?")), (41, "?".to_string()));
        let mine = a.cur_job;
        a.open_job(0);
        a.open_job(mine);
        let out = dir.join("DEMO-904-saved.snjob");
        a.save_job_to(&out);
        let saved = jobfile::load(&out).unwrap();
        assert_eq!((saved.spec.material, saved.material_name.as_str()), (41, "DEMO 904L"));
        assert_eq!(a.current_jobfile().material_name, "DEMO 904L", "the Library save's job");
        // Taking a stock sheet in another material the list doesn't have:
        // the job takes the sheet's material and its name, never the name
        // it was opened with.
        let lib = dir.join("library");
        crate::library::create(&lib).unwrap();
        a.cfg.library_dir = lib.display().to_string();
        let mut it = stock::Item::sheet("DEMO-2205-S1", 42, 4.0, 2000.0, 1000.0, 1);
        it.material_name = "DEMO 2205".into();
        stock::Stock::update(&a.library_at(), |s| s.add_sheet(it, true)).unwrap();
        a.use_stock_switching("DEMO-2205-S1");
        assert_eq!(a.spec.material, 42, "{}", a.status);
        a.save_job_to(&out);
        let saved = jobfile::load(&out).unwrap();
        assert_eq!((saved.spec.material, saved.material_name.as_str()), (42, "DEMO 2205"));
        // A job in a material the list has is saved with the list's name.
        a.spec.material = 1;
        a.save_job_to(&out);
        assert_eq!(jobfile::load(&out).unwrap().material_name, "Stainless steel");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// SH5: `--shop-demo` never touches a real Library: outside a screenshot
    /// run it does nothing at all, and its Library is always a temporary one
    /// of its own, never the configured one.
    #[test]
    fn the_shop_demo_never_touches_a_real_library() {
        let lib = crate::testdir::TestDir::new("shop_demo_real");
        crate::library::create(&lib).unwrap();
        let mut a = App::new(None);
        a.cfg.library_dir = lib.display().to_string();
        a.demo_shop("");
        assert!(!lib.join(crate::shop::FILE).exists() && !lib.join(crate::shop::PCS).exists(), "the demo made a shop in the configured Library");
        // Its own folder is made fresh for the run: never one already there.
        let demo = settings_ui::shop_demo_dir().unwrap();
        let fresh = demo.parent().unwrap().to_path_buf();
        assert!(fresh.starts_with(std::env::temp_dir()) && demo != *lib);
        assert_eq!(std::fs::read_dir(&fresh).unwrap().count(), 0, "the demo's folder wasn't made fresh");
        a.cfg.library_dir = demo.display().to_string();
        a.demo_shop("");
        assert!(!demo.exists(), "the demo ran outside a screenshot run");
        // Two allocations are two new, empty folders; one already there,
        // with someone's files in it, is never used or emptied.
        let (one, two) = (settings_ui::new_temp_folder("nestpro-test-alloc").unwrap(), settings_ui::new_temp_folder("nestpro-test-alloc").unwrap());
        assert_ne!(one, two);
        std::fs::write(one.join("keep.txt"), "someone's").unwrap();
        let three = settings_ui::new_temp_folder("nestpro-test-alloc").unwrap();
        assert!(three != one && one.join("keep.txt").is_file());
        for d in [&fresh, &one, &two, &three] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    /// A window as kim on PC-B whose Library is a shop Acme (admin PC-A),
    /// in a new folder named after `tag`.
    fn shop_window(tag: &str) -> (App, crate::testdir::TestDir) {
        use crate::shared::{Clock, Who};
        let d = crate::testdir::TestDir::new(&format!("add_pc_{tag}"));
        let lib = d.join("Nesting");
        crate::library::create(&lib).unwrap();
        crate::engine::shop::start(&lib, "Acme", &Who::new("sam", "PC-A"), &Clock::pc(), "0.1.0").unwrap();
        let mut a = App::new(None);
        a.cfg.library_dir = lib.display().to_string();
        a.who = Who::new("kim", "PC-B");
        (a, d)
    }

    /// Wait for this PC's background add to finish.
    fn wait_added(a: &mut App) {
        let t = std::time::Instant::now();
        while a.poll_add_this_pc() {
            assert!(t.elapsed() < std::time::Duration::from_secs(20), "adding this PC never finished");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// A folder this PC can only read (unix, not as root).
    #[cfg(unix)]
    fn read_only(p: &std::path::Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o555)).unwrap();
        std::fs::write(p.join("probe"), "").is_err()
    }

    #[cfg(unix)]
    fn writable(p: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// SH4: Add this PC never makes the window wait for the write lock: it
    /// runs in the background and its answer comes in later.
    #[test]
    fn add_this_pc_never_waits_for_the_lock() {
        let (mut a, d) = shop_window("lock");
        let lib = a.library_dir();
        let lock = lib.join(crate::shared::LOCK_FILE);
        std::fs::write(&lock, r#"{"user":"lee","pc":"PC-C","process":1,"since":0,"token":"PC-C-token"}"#).unwrap();
        let t = std::time::Instant::now();
        a.add_this_pc(true);
        assert!(t.elapsed() < std::time::Duration::from_secs(1), "Add this PC waited for the lock");
        assert!(a.poll_add_this_pc(), "it should still be waiting for the lock");
        std::fs::remove_file(&lock).unwrap();
        wait_added(&mut a);
        assert_eq!(a.setui.shop_unrecorded, None);
        assert_eq!(a.setui.shop_note.as_ref().map(|n| n.0), Some(true), "{:?}", a.setui.shop_note);
        assert!(crate::shop::records(&lib).unwrap().iter().any(|p| p.pc == "PC-B"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// SH4: when this PC can't bring its row up to date as the program opens
    /// (though it is listed from before), the failure is kept for the PCs
    /// card until a try works.
    #[cfg(unix)]
    #[test]
    fn a_failed_add_at_start_up_is_kept_until_a_try_works() {
        let (mut a, d) = shop_window("startup");
        let lib = a.library_dir();
        a.add_this_pc(false);
        wait_added(&mut a);
        assert!(crate::shop::records(&lib).unwrap().iter().any(|p| p.pc == "PC-B"));
        let pcs = lib.join(crate::shop::PCS);
        if read_only(&pcs) {
            a.record_in_shop();
            wait_added(&mut a);
            assert!(a.setui.shop_unrecorded.as_deref().is_some_and(|e| e.contains("PC-B couldn't be added")), "{:?}", a.setui.shop_unrecorded);
        }
        writable(&pcs);
        a.add_this_pc(true);
        wait_added(&mut a);
        assert_eq!(a.setui.shop_unrecorded, None, "a try that worked clears the failure");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// SH1, SH4: Start the shop in Settings keeps the failure too, when this
    /// PC can't be added to the new shop's PCs list.
    #[cfg(unix)]
    #[test]
    fn start_the_shop_keeps_why_this_pc_isnt_listed() {
        let d = crate::testdir::TestDir::new("start_here");
        let lib = d.join("Nesting");
        crate::library::create(&lib).unwrap();
        let pcs = lib.join(crate::shop::PCS);
        std::fs::create_dir(&pcs).unwrap();
        let mut a = App::new(None);
        a.cfg.library_dir = lib.display().to_string();
        a.who = crate::shared::Who::new("kim", "PC-B");
        if read_only(&pcs) {
            a.start_shop_here("Acme");
            assert!(lib.join(crate::shop::FILE).is_file(), "the shop wasn't started");
            assert!(a.setui.shop_unrecorded.as_deref().is_some_and(|e| e.contains("PC-B couldn't be added")), "{:?}", a.setui.shop_unrecorded);
        }
        writable(&pcs);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// SH4: Add this PC on a Library that is no shop (any more) says no
    /// success: only a record written counts.
    #[test]
    fn adding_this_pc_to_a_library_that_is_no_shop_says_no_success() {
        let (mut a, d) = shop_window("plain");
        let plain = d.join("Plain");
        crate::library::create(&plain).unwrap();
        a.cfg.library_dir = plain.display().to_string();
        a.add_this_pc(true);
        wait_added(&mut a);
        assert_eq!(a.setui.shop_note.as_ref().map(|n| n.0), Some(false), "{:?}", a.setui.shop_note);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn switching_jobs_keeps_each_jobs_products() {
        let mut a = App::new(None);
        assert!(a.jobs.len() >= 3);
        let first = a.parts.len();
        a.parts[0].qty = 7;
        a.open_job(1);
        assert_eq!(a.parts.len(), a.jobs[1].parts.len());
        assert!(!a.result.sheets.is_empty());
        a.open_job(0);
        assert_eq!(a.parts.len(), first);
        assert_eq!(a.parts[0].qty, 7);
        a.new_job_in_week("W12".into());
        assert!(a.parts.is_empty());
        assert_eq!(a.cur_job, a.jobs.len() - 1);
    }

    /// The window on one made-up machine saved with gas `gas` (number,
    /// name), its Library in a folder that isn't there.
    fn app_on_gas(tag: &str, gas: (u32, &str)) -> (App, crate::testdir::TestDir) {
        let d = crate::testdir::TestDir::made(&format!("newjob_gas_{tag}"));
        let mut a = App::new(None);
        a.cfg.library_dir = d.join("no-library").display().to_string();
        a.cfg.machines = vec![settings::Machine { name: "Demo laser".into(), gas_id: gas.0, gas_name: gas.1.into(), ..Default::default() }];
        a.cfg.active_machine = 0;
        (a, d)
    }

    #[test]
    fn every_job_made_on_screen_starts_with_the_machines_gas() {
        use gas::Gas;
        // GS12: New job (both commands), a job per material from an XML
        // import, and a job opened from programs that name no gas (GS18), on
        // a machine saved with oxygen: each is an oxygen job of its own.
        let (mut a, d) = app_on_gas("oxygen", (3, "Oxygen"));
        a.spec.gas = Some(Gas::Air);
        a.new_job_in_week("W12".into());
        assert_eq!(a.spec.gas, Some(Gas::Oxygen), "New job");
        a.spec.gas = Some(Gas::Air);
        a.new_job();
        assert_eq!(a.spec.gas, Some(Gas::Oxygen), "New job (Home)");
        let xml = d.join("DEMO-ORDER.xml");
        std::fs::write(&xml, "<Import>\n<Product Reference=\"DEMO-1\" Material=\"Mild steel\" Thickness=\"3\" Quantity=\"2\"/>\n<Product Reference=\"DEMO-2\" Material=\"AISI304\" Thickness=\"1.5\" Quantity=\"1\"/>\n</Import>\n").unwrap();
        let n = a.jobs.len();
        a.spec.gas = Some(Gas::Air);
        a.import_xml(&xml);
        a.store_job();
        assert_eq!(a.jobs.len(), n + 2, "one job per material: {}", a.status);
        assert_eq!(a.jobs[n..].iter().map(|j| (j.spec.material, j.spec.gas)).collect::<Vec<_>>(), [(0, Some(Gas::Oxygen)), (1, Some(Gas::Oxygen))], "XML import");
        let set = d.join("DEMONOGAS.SPF");
        std::fs::write(&set, ";ADILAS\r\n;STEEL,1000,500,3\r\n;IDGAS=\r\n;NOMEGAS=\r\nINITNEST\r\nORIG(10,10,0)\r\nN1 CALL\"/_n_wks_dir/_n_sheet_wpd/_n_DEMONOGAS701_spf\"\r\nENDNEST\r\n").unwrap();
        std::fs::write(d.join("DEMONOGAS701.SPF"), "NEXT_L(0,0,0,1,3)\r\nPROFILE\r\nG01 X100. Y0.\r\nG01 X100. Y50.\r\nG01 X0. Y50.\r\nLASTLINE(0,0,0,1,0.,50.,0.,0.,0,0)\r\nENDGEO\r\n").unwrap();
        a.spec.gas = Some(Gas::Air);
        a.open_nesting_file(&set);
        assert_eq!(a.spec.gas, Some(Gas::Oxygen), "programs naming no gas: {}", a.status);
        assert!(a.status.contains("the programs name no gas, so the job starts with the machine's gas, Oxygen"), "{}", a.status);
        // A set that names its gas gives the job that gas, whatever the machine's.
        std::fs::write(&set, ";ADILAS\r\n;STEEL,1000,500,3\r\n;IDGAS=5\r\n;NOMEGAS=H2O\r\nINITNEST\r\nORIG(10,10,0)\r\nN1 CALL\"/_n_wks_dir/_n_sheet_wpd/_n_DEMONOGAS701_spf\"\r\nENDNEST\r\n").unwrap();
        a.open_nesting_file(&set);
        assert_eq!(a.spec.gas, Some(Gas::Water), "programs naming 5 H2O: {}", a.status);
        let _ = std::fs::remove_dir_all(&d);

        // A machine saved as 2 named Oxygen (not the list's pair): new jobs
        // have no gas of their own, and are cut with the machine's as saved.
        let (mut a, d) = app_on_gas("off_list", (2, "Oxygen"));
        a.spec.gas = Some(Gas::Air);
        a.new_job_in_week("W12".into());
        assert_eq!(a.spec.gas, None, "New job");
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
mod rule_tests {
    use super::*;

    #[test]
    fn duplicate_job_names_follow_the_original_rule() {
        let mut a = App::new(None);
        let base = a.job_name.clone();
        a.copy_job();
        assert_eq!(a.job_name, format!("{base} (1)"));
        a.open_job(0);
        a.copy_job();
        assert_eq!(a.job_name, format!("{base} (2)"));
        // Fill (3)..(9); the tenth copy must be refused.
        for _ in 3..=9 {
            a.open_job(0);
            a.copy_job();
        }
        let before = a.jobs.len();
        a.open_job(0);
        a.copy_job();
        assert_eq!(a.jobs.len(), before);
        assert!(a.status.starts_with("Couldn't copy"));
    }

    #[test]
    fn identical_sheets_are_folded() {
        let mut a = App::new(None);
        a.parts = vec![Part { name: "P".into(), shape: Shape::Rectangle, w: 900.0, h: 900.0, qty: 6, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        a.cfg.stock.equal_sheets = false;
        a.run_nesting();
        let groups = a.sheet_groups();
        let total: usize = groups.iter().map(|g| g.1).sum();
        assert_eq!(total, a.result.sheets.len());
        assert!(groups.len() < a.result.sheets.len());
        // SR13: with Deal with equal sheets on, Nest all cuts them as the
        // cycles of one sheet; the folded count is the same.
        a.cfg.stock.equal_sheets = true;
        a.run_nesting();
        assert!(a.result.sheets.len() < total, "{} nestings", a.result.sheets.len());
        assert_eq!(a.sheets_cut(), total);
        assert_eq!(a.sheet_groups().iter().map(|g| g.1).sum::<usize>(), total);
        assert_eq!(a.total_placed(), 6);
        assert!(a.sheet_label(0, 0).ends_with(&format!("(×{})", a.runs(0))), "{}", a.sheet_label(0, 0));
    }
}

#[cfg(test)]
mod tech_tests {
    use super::*;

    fn tech_part_index(a: &App) -> usize {
        a.parts.iter().position(|p| p.tech.is_some()).expect("a sample part has technology")
    }

    #[test]
    fn micro_joints_reach_the_program_and_cut_length() {
        let a = App::new(None);
        assert!(a.program.contains("MICRO-JOINT"));
        let mut b = App::new(None);
        for p in &mut b.parts {
            p.tech = None;
        }
        b.refresh();
        assert!(!b.program.contains("MICRO-JOINT"));
    }

    #[test]
    fn editing_a_tech_part_copy_keeps_the_original() {
        let mut a = App::new(None);
        let i = tech_part_index(&a);
        let n = a.parts.len();
        let original = a.parts[i].clone();
        let mut draft = original.clone();
        draft.w += 20.0;
        a.apply_tech_choice(i, draft, TechChoice::EditCopy);
        assert_eq!(a.parts.len(), n + 1);
        assert_eq!(a.parts[i].w, original.w);
        assert!(a.parts[i].tech.is_some());
        assert!(a.parts[i + 1].tech.is_none());
        assert_eq!(a.parts[i + 1].w, original.w + 20.0);
        // Placed parts still point at the same products.
        for s in &a.result.sheets {
            for p in s {
                assert!(p.part < a.parts.len());
            }
        }
    }

    #[test]
    fn editing_a_tech_part_itself_removes_its_technology() {
        let mut a = App::new(None);
        let i = tech_part_index(&a);
        let mut draft = a.parts[i].clone();
        draft.h += 10.0;
        a.apply_tech_choice(i, draft, TechChoice::EditPart);
        assert!(a.parts[i].tech.is_none());
    }

    #[test]
    fn material_change_can_remove_or_keep_nestings() {
        let mut a = App::new(None);
        let mut s = a.spec.clone();
        s.thickness = 5.0;
        a.apply_spec_change(s.clone(), false);
        assert!(!a.result.sheets.is_empty() && a.stale);
        a.apply_spec_change(s, true);
        assert!(a.result.sheets.is_empty());
    }
}

#[cfg(test)]
mod part_tech_tests {
    use super::*;

    fn square_app(tech: nest::PartTech) -> App {
        let mut a = App::new(None);
        a.parts = vec![Part { name: "S".into(), shape: Shape::Rectangle, w: 200.0, h: 100.0, qty: 1, rotate: false, color: 0, tech: Some(tech), custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        a.run_nesting();
        a
    }

    #[test]
    fn automatic_lead_starts_at_the_corner_nearest_the_position() {
        // Position 9 = top right of the contour box; the machine's table puts a
        // corner lead (type 10) on rectangular parts.
        let a = square_app(nest::PartTech { corner_start: true, position: 9, lead_type: nest::LeadType::Machine, ..Default::default() });
        let c = &a.plans[0].contours[0];
        let pl = a.result.sheets[0][0];
        let top_right = [pl.x + pl.w, pl.y + pl.h];
        assert!((c.points[0][0] - top_right[0]).abs() < 1e-6 && (c.points[0][1] - top_right[1]).abs() < 1e-6);
        // Corrected (AL7): the lead extends the first side
        // (the top, cut right to left) backwards past the corner, not the bisector.
        let d = [c.lead_in[0] - c.points[0][0], c.lead_in[1] - c.points[0][1]];
        assert!(d[0] > 0.0 && d[1].abs() < 1e-6, "{d:?}");
        // Other lead types go in the middle of the side nearest the position.
        let a = square_app(nest::PartTech { corner_start: true, position: 9, ..Default::default() });
        let c = &a.plans[0].contours[0];
        assert!((c.points[0][0] - top_right[0]).abs() < 1e-6 && (c.points[0][1] - (pl.y + pl.h / 2.0)).abs() < 1e-6, "{:?}", c.points[0]);
    }

    #[test]
    fn arc_lead_ends_on_the_contour_and_round_holes_lead_from_the_zone_angle() {
        let mut a = App::new(None);
        a.parts = vec![Part { name: "F".into(), shape: Shape::Flange, w: 200.0, h: 200.0, qty: 1, rotate: false, color: 0, tech: Some(nest::PartTech { lead_type: nest::LeadType::Arc, corner_start: true, ..Default::default() }), custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        a.run_nesting();
        let plan = &a.plans[0];
        // Corrected: a round hole's lead starts on the circle at the position's
        // angle (225° for position 1), kept inside the hole and short of its
        // centre (AL4, AL10), not always pierced at the centre.
        let hole = plan.contours.iter().find(|c| c.inner).unwrap();
        let cc = nest::centroid(&hole.points);
        let r = hole.points.iter().map(|p| ((p[0] - cc[0]).powi(2) + (p[1] - cc[1]).powi(2)).sqrt()).fold(0.0, f64::max);
        let s = hole.points[0];
        let ang = (s[1] - cc[1]).atan2(s[0] - cc[0]).to_degrees().rem_euclid(360.0);
        assert!((ang - 225.0).abs() < 1.0, "{ang}");
        assert!(hole.lead_path.len() > 2, "an arc lead");
        assert!(hole.lead_path.iter().all(|p| ((p[0] - cc[0]).powi(2) + (p[1] - cc[1]).powi(2)).sqrt() <= r + 1e-6));
        let outer = plan.contours.iter().find(|c| !c.inner).unwrap();
        let end = *outer.lead_path.last().unwrap();
        assert!((end[0] - outer.points[0][0]).abs() < 1e-9 && (end[1] - outer.points[0][1]).abs() < 1e-9);
    }

    #[test]
    fn loops_go_on_sharp_corners_and_reach_the_program() {
        let a = square_app(nest::PartTech { loops: true, ..Default::default() });
        let outer = &a.plans[0].contours[0];
        assert_eq!(outer.loops.len(), 3); // four 90° corners; the start corner has the lead
        assert!(a.program.contains("( LOOP )"));
    }

    #[test]
    fn joints_by_size_class_follow_the_rule() {
        // Corrected (P7, AM2): each size class gives the fewest
        // and most joints (not counts along X and Y); −1 = the general F121/F122.
        let cfg = nest::AutoJoints { small: [50.0, 50.0], large: [300.0, 300.0], parts: [[0, 0], [-1, -1], [4, 6]], holes: [[1, 2], [-1, -1], [-1, -1]], min: 0, max: 99999 };
        assert_eq!(cfg.limits(40.0, 40.0, false), (0, 0));
        assert_eq!(cfg.limits(40.0, 40.0, true), (1, 2));
        assert_eq!(cfg.limits(200.0, 100.0, false), (0, 99999));
        // 160 mm small part: none allowed. 600 mm medium: one per 100 → 6.
        assert_eq!(cfg.count_for(160.0, 100.0, 40.0, 40.0, false), 0);
        assert_eq!(cfg.count_for(600.0, 100.0, 200.0, 100.0, false), 6);
        // 1500 mm large part: 15 → clamped to 6. Small hole 120 mm: 2 → at most 2.
        assert_eq!(cfg.count_for(1500.0, 100.0, 400.0, 350.0, false), 6);
        assert_eq!(cfg.count_for(120.0, 100.0, 40.0, 40.0, true), 2);
        assert_eq!(nest::AutoJoints::default().count_for(400.0, 100.0, 150.0, 50.0, false), 4);
    }
}

#[cfg(test)]
mod part_view_tests {
    use super::*;

    #[test]
    fn opening_a_part_builds_the_set_and_steps_through_it() {
        let mut a = App::new(None);
        let n = a.parts.len();
        a.open_scope = OpenScope::All;
        a.open_part(2);
        assert!(a.workspace == Workspace::Part);
        assert!(a.tab == Tab::Home, "the Drawing module opens on its Home tab");
        assert_eq!(a.open_set.len(), n);
        assert_eq!(a.shown_part(), Some(2));
        a.open_part_step(1);
        assert_eq!(a.sel_part, Some(3));
        a.open_part_step(-2);
        assert_eq!(a.sel_part, Some(1));
        a.open_scope = OpenScope::Selected;
        a.open_part(4);
        assert_eq!(a.open_set, vec![4]);
    }
}

#[cfg(test)]
mod element_tests {
    use super::*;

    fn rect_app() -> App {
        let mut a = App::new(None);
        a.parts = vec![Part { name: "R".into(), shape: Shape::Rectangle, w: 200.0, h: 100.0, qty: 1, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        a.run_nesting();
        a.open_scope = OpenScope::Selected;
        a.open_part(0);
        a
    }

    fn pick(a: &mut App, which: &[usize]) {
        let el = nest::elements_of(&a.parts[0].geometry());
        a.sel_elem_info = which.iter().map(|&i| (el[i].contour, el[i].kind, el[i].length, el[i].midpoint())).collect();
        a.sel_elems = which.to_vec();
    }

    #[test]
    fn a_rectangle_has_four_lines_and_a_disc_one_circle() {
        let a = rect_app();
        let el = nest::elements_of(&a.parts[0].geometry());
        assert_eq!(el.len(), 4);
        assert!(el.iter().all(|e| e.kind == nest::ElemKind::Line));
        assert!((el[0].length - 200.0).abs() < 1e-9 && (el[1].length - 100.0).abs() < 1e-9);
        assert_eq!(el[0].midpoint(), [100.0, 0.0]);
        let disc = Part { name: "D".into(), shape: Shape::Disc, w: 100.0, h: 100.0, qty: 1, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        let de = nest::elements_of(&disc.geometry());
        assert!(de.iter().any(|e| e.kind == nest::ElemKind::Circle));
    }

    #[test]
    fn capture_picks_the_nearest_and_breaks_ties_by_the_nearer_start() {
        let a = rect_app();
        let el = nest::elements_of(&a.parts[0].geometry());
        assert_eq!(partview::capture(&el, [60.0, 1.0], 2.0), Some(0));
        assert_eq!(partview::capture(&el, [60.0, 50.0], 2.0), None);
        // On the corner (200, 0) both the bottom and right lines are at distance 0:
        // the right line starts there, so it wins.
        assert_eq!(partview::capture(&el, [200.0, 0.0], 2.0), Some(1));
    }

    #[test]
    fn lead_on_element_starts_the_cut_in_the_middle_of_the_line() {
        let mut a = rect_app();
        pick(&mut a, &[0]);
        a.lead_on_element();
        let t = a.parts[0].tech.unwrap();
        assert_eq!(t.starts[0], Some((0, [100.0, 0.0])));
        let pl = a.result.sheets[0][0];
        let c = a.plans[0].contours.iter().find(|c| !c.inner).unwrap();
        assert!((c.points[0][0] - (pl.x + 100.0)).abs() < 1e-6 && (c.points[0][1] - pl.y).abs() < 1e-6, "{:?} vs {:?}", c.points[0], (pl.x, pl.y));
        // Picking again on the same contour replaces the start.
        pick(&mut a, &[1]);
        a.lead_on_element();
        let t = a.parts[0].tech.unwrap();
        assert_eq!(t.starts.iter().flatten().count(), 1);
        assert_eq!(t.starts[0], Some((0, [200.0, 50.0])));
    }

    #[test]
    fn lead_here_on_the_right_click_ring_starts_the_cut_exactly_there() {
        // LE10: not by thirds like On element (LP3), but at the spot itself.
        let mut a = rect_app();
        pick(&mut a, &[0]);
        a.sel_pick = Some((0, [37.0, 1.0]));
        a.lead_here();
        let plan = cnc::plan_part(&a.parts, 0, &a.spec);
        let c = plan.contours.iter().find(|c| !c.inner).unwrap();
        assert_eq!(c.lead_spots[0].frame.0, [37.0, 0.0], "{}", a.status);
        assert!(c.lead_path.len() > 1, "{}", a.status);
    }

    #[test]
    fn the_ribbons_holes_and_outline_choice_limits_auto_and_delete_lead_ins() {
        // LE12, from the ribbon: Delete lead-ins on the outline leaves the holes' leads.
        let mut a = rect_app();
        a.parts[0].shape = Shape::PlateHole;
        a.parts[0].h = 150.0;
        a.sel_part = Some(0);
        a.tech_all = false;
        a.lead_choice = nest::LeadType::Line;
        a.auto_leads();
        a.lead_scope = nest::LeadScope::Outline;
        a.edit_tech("Lead-ins removed", |t, h| crate::engine::TechEdit::DeleteLeads(nest::LeadScope::Outline).apply(t, h));
        let plan = cnc::plan_part(&a.parts, 0, &a.spec);
        assert!(plan.contours.iter().all(|c| (c.lead_path.len() > 1) == c.inner), "{:?}", a.parts[0].tech);
        a.auto_leads();
        let plan = cnc::plan_part(&a.parts, 0, &a.spec);
        assert!(plan.contours.iter().all(|c| c.lead_path.len() > 1));
        assert_eq!(a.parts[0].tech.unwrap().outline_leads.auto, Some((nest::LeadType::Line, 1)));
    }

    #[test]
    fn a_lead_goes_by_the_third_of_the_line_that_was_clicked() {
        // LP3: first third → 2·tol + overlap from the start,
        // last third → length − 2·tol − overlap, middle → (length − overlap) / 2.
        let mut a = rect_app();
        a.parts[0].tech = Some(nest::PartTech { overlap: 0.5, ..Default::default() });
        pick(&mut a, &[0]);
        a.sel_pick = Some((0, [20.0, 3.0]));
        a.lead_on_element();
        let s = a.parts[0].tech.unwrap().starts[0].unwrap().1;
        assert!((s[0] - 0.52).abs() < 1e-9 && s[1].abs() < 1e-9, "{s:?}");
        a.sel_pick = Some((0, [190.0, 0.0]));
        a.lead_on_element();
        let s = a.parts[0].tech.unwrap().starts[0].unwrap().1;
        assert!((s[0] - (200.0 - 0.02 - 0.5)).abs() < 1e-9, "{s:?}");
        a.sel_pick = Some((0, [100.0, 0.0]));
        a.lead_on_element();
        assert!((a.parts[0].tech.unwrap().starts[0].unwrap().1[0] - 99.75).abs() < 1e-9);
        // A click on another element (stale after a box selection) is ignored:
        // the lead goes in the element's middle (E4).
        a.sel_pick = Some((2, [20.0, 3.0]));
        a.lead_on_element();
        assert!((a.parts[0].tech.unwrap().starts[0].unwrap().1[0] - 100.0).abs() < 1e-9);
        // Too short: not longer than 4 × 0.01 + overlap.
        let e = nest::Element { contour: 0, kind: nest::ElemKind::Line, pts: vec![[0.0, 0.0], [0.5, 0.0]], length: 0.5 };
        assert_eq!(e.lead_point(Some([0.1, 0.0]), 0.46, nest::LEAD_LINEAR_TOL), None);
        assert!(e.lead_point(Some([0.1, 0.0]), 0.4, nest::LEAD_LINEAR_TOL).is_some());
    }

    #[test]
    fn short_elements_are_refused_and_joints_go_on_each_selected_element() {
        let mut a = rect_app();
        a.parts[0].tech = Some(nest::PartTech { lead_in: 150.0, ..Default::default() });
        pick(&mut a, &[1]); // 100 mm line, 150 mm lead
        a.lead_on_element();
        assert!(a.parts[0].tech.unwrap().starts.iter().all(|s| s.is_none()));
        pick(&mut a, &[0, 2]);
        a.joint_on_elements();
        assert_eq!(a.parts[0].tech.unwrap().joint_picks.iter().flatten().count(), 2);
        let c = a.plans[0].contours.iter().find(|c| !c.inner).unwrap();
        assert_eq!(c.joints.len(), 2);
    }
}

#[cfg(test)]
mod text_tests {
    use super::*;

    #[test]
    fn marking_texts_reach_the_plan_and_program_but_notes_do_not() {
        let mut a = App::new(None);
        let mk = |kind| nest::PartText { text: "AB1".into(), at: [10.0, 10.0], height: 10.0, angle: 0.0, curve: 0.0, kind };
        a.parts = vec![Part { name: "P".into(), shape: Shape::Rectangle, w: 200.0, h: 100.0, qty: 1, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![mk(nest::TextKind::Note)], group: String::new(), erp: vec![] }];
        a.run_nesting();
        assert!(a.plans[0].marks.is_empty(), "notes are not machined");
        a.parts[0].texts = vec![mk(nest::TextKind::Mark)];
        a.refresh();
        assert!(!a.plans[0].marks.is_empty());
        assert!(a.plans[0].mark_len > 0.0);
        assert!(a.program.contains("M10"));
        // Text added from the ribbon lands inside the part.
        a.sel_part = Some(0);
        a.add_text_to_targets("#REF", 8.0, nest::TextKind::Special);
        let t = a.parts[0].texts.last().unwrap();
        assert!(t.at[0] >= 0.0 && t.at[0] + font::width(&t.text, t.height) <= 200.0);
    }

    #[test]
    fn dxf_text_on_a_mark_layer_becomes_a_marking_text() {
        let src = "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n8\nCUT\n90\n4\n70\n1\n10\n0\n20\n0\n10\n100\n20\n0\n10\n100\n20\n50\n10\n0\n20\n50\n0\nTEXT\n8\nMARK\n10\n10\n20\n10\n40\n6\n1\nPN-7\n0\nTEXT\n8\nCUT\n10\n10\n20\n30\n40\n4\n1\nnote\n0\nENDSEC\n0\nEOF\n";
        let d = dxf::read(src).unwrap();
        let mut roles = std::collections::BTreeMap::new();
        roles.insert("CUT".to_string(), dxf::LayerRole::Cut);
        roles.insert("MARK".to_string(), dxf::LayerRole::Mark);
        let (g, _) = dxf::build(&d, &roles).unwrap();
        assert_eq!(g.texts.len(), 2);
        let mark = g.texts.iter().find(|t| t.text == "PN-7").unwrap();
        assert_eq!(mark.kind, nest::TextKind::Mark);
        assert_eq!(mark.height, 6.0);
        assert_eq!(g.texts.iter().find(|t| t.text == "note").unwrap().kind, nest::TextKind::Note);
    }
}

#[cfg(test)]
mod screenshot_settings_tests {
    use super::*;

    /// A screenshot run (`--screenshot`) changes settings in memory only:
    /// `--machine-gas` sets the machine's gas, and opening a job
    /// (`--open-job`) puts it on the recent list, but the settings file stays
    /// as it was (GS17). Without `--screenshot`, opening a job still saves
    /// the recent list.
    #[test]
    fn a_screenshot_run_never_saves_the_settings() {
        // Alone in a process of its own: every test of a run shares one
        // settings file, and another test could write it meanwhile.
        if std::env::var_os("NESTPRO_SHOT_SETTINGS_CHILD").is_none() {
            let name = "screenshot_settings_tests::a_screenshot_run_never_saves_the_settings";
            let out = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact", name, "--test-threads=1"]).env("NESTPRO_SHOT_SETTINGS_CHILD", "1").output().unwrap();
            let said = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            assert!(out.status.success() && said.contains("1 passed"), "{said}");
            return;
        }
        let file = settings::AppSettings::file();
        settings::AppSettings::default().save_to(&file).unwrap();
        let before = std::fs::read_to_string(&file).unwrap();
        let dir = file.parent().unwrap().join("jobs");
        std::fs::create_dir_all(&dir).unwrap();
        let job = dir.join("DEMO-SHOT.snjob");
        jobfile::save(&job, &jobfile::JobFile::new_job("DEMO-SHOT", engine::default_spec())).unwrap();
        // A screenshot run: --machine-gas=2:Oxygen, then --open-job.
        let mut a = App::new(Some(dir.join("shot.png")));
        let m = &mut a.cfg.machines[a.cfg.active_machine];
        m.gas_id = 2;
        m.gas_name = "Oxygen".into();
        a.apply_settings(true);
        a.open_job_file(&job);
        assert_eq!(a.cfg.recent_jobs.first(), Some(&job.display().to_string()), "the recent list changes in memory");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before, "a screenshot run wrote the settings file");
        // Launched for real, opening a job saves the recent list as before.
        let mut b = App::new(None);
        b.open_job_file(&job);
        let now = settings::AppSettings::read_from(&file).unwrap();
        assert_eq!(now.recent_jobs.first(), Some(&job.display().to_string()));
        assert_eq!((now.machines[0].gas_id, now.machines[0].gas_name.as_str()), (1, "Nitrogen"), "the machine's gas as saved");
    }
}
