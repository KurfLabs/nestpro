//! The Settings window: database connections, machines and how their
//! programs are written, material codes and folders. Every value is
//! editable and saved (settings.rs); Lantek Expert is only an import source.

use crate::settings::{AppSettings, Connection, Machine, Output};

/// A program format in a few words, for machine lists.
pub fn output_name(o: Output) -> &'static str {
    match o {
        Output::Iso => "ISO G-code",
        Output::BlmSiemens => "BLM / Siemens",
        Output::Omax => "OMAX waterjet",
    }
}
use crate::theme::th;
use crate::ui_parts::*;
use crate::App;
use eframe::egui;
use egui::{CornerRadius, RichText, Stroke};
use std::sync::mpsc::Receiver;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Section {
    /// SH5: the shop this PC's Library belongs to, at the top of the list.
    Shop,
    #[default]
    General,
    Appearance,
    Shortcuts,
    Connections,
    Machines,
    Nesting,
    Materials,
    Folders,
    Integrations,
    About,
}

impl Section {
    /// The navigation, in groups.
    const GROUPS: [(&'static str, &'static [Section]); 5] = [
        ("SHOP", &[Section::Shop]),
        ("APP", &[Section::General, Section::Appearance, Section::Shortcuts]),
        ("PRODUCTION", &[Section::Connections, Section::Machines, Section::Nesting, Section::Materials, Section::Folders]),
        ("INTEGRATIONS", &[Section::Integrations]),
        ("INFO", &[Section::About]),
    ];
    fn title(self) -> &'static str {
        match self {
            Section::Shop => "Shop",
            Section::General => "General",
            Section::Appearance => "Appearance",
            Section::Shortcuts => "Shortcuts",
            Section::Connections => "Connections",
            Section::Machines => "Machines & output",
            Section::Nesting => "Nesting",
            Section::Materials => "Materials",
            Section::Folders => "Folders",
            Section::Integrations => "ERP / MES",
            Section::About => "About",
        }
    }
    fn hint(self) -> &'static str {
        match self {
            Section::Shop => "This shop and its PCs",
            Section::General => "Warnings and commands",
            Section::Appearance => "Light, dark or Windows",
            Section::Shortcuts => "Keys and mouse",
            Section::Connections => "Databases to open",
            Section::Machines => "How programs are written",
            Section::Nesting => "Units (mm / inch), part gap, moving parts",
            Section::Materials => "The shop's material list",
            Section::Folders => "Imports and exports",
            Section::Integrations => "Fulcrum and other systems",
            Section::About => "Version and files",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Section::Shop => "🏭",
            Section::General => "🔧",
            Section::Appearance => "◐",
            Section::Shortcuts => "⌨",
            Section::Connections => "🗄",
            Section::Machines => "⚙",
            Section::Nesting => "▣",
            Section::Materials => "▦",
            Section::Folders => "📁",
            Section::Integrations => "🔗",
            Section::About => "ℹ",
        }
    }
}

/// The keyboard and mouse shortcuts (shown in Settings → Shortcuts).
pub const SHORTCUTS: &[(&str, &str)] = &[
    ("F5", "Nest all"),
    ("Ctrl + Z  /  Ctrl + Y", "Undo / redo"),
    ("Click / Shift or Ctrl + click", "Select a part / add it to the selection or take it out"),
    ("Drag right on empty sheet", "Select the parts wholly inside the window (Shift or Ctrl adds)"),
    ("Drag left on empty sheet", "Select every part the window touches (Shift or Ctrl adds)"),
    ("Ctrl + E  /  Ctrl + A", "Select every part on the sheet"),
    ("Alt + arrow keys", "Move the selected parts by the step (Settings → Nesting, or Ctrl + R); like the original, this move doesn't stop at other parts"),
    ("Ctrl + arrow keys", "Slide the selected parts to the next part (at the part gap) or the margin"),
    ("Arrow keys  /  Page Down, Page Up", "Look around the sheet / zoom in, out"),
    ("Ctrl + R", "Set the step (movement value)"),
    ("Drag a part", "Move the selection; it snaps one part gap from a neighbour and slides along parts it meets (hold Alt for no snap)"),
    ("Ctrl + drag a part", "Drag a copy of the selection"),
    ("Ctrl + D", "Copy the selected parts to the first free spot"),
    ("R", "Turn the selection to its next allowed quarter turn"),
    ("Ctrl + Q  /  Ctrl + W", "Turn the selection left / right by the turn step (the turn bar's Snap, or Settings → Nesting; 1° when Free)"),
    ("Ctrl + 1 / 5 / 0 / 3 / 4 / 9", "Turn the selection 1° / 5° / 10° / 30° / 45° / 90° (direction in Settings → Nesting)"),
    ("Ctrl + T", "Type an angle for the selection (the turn bar's angle box)"),
    ("Drag the round handle", "Turn the selection about its centre, snapping to the turn step (hold Alt to turn freely)"),
    ("M  /  Shift + M", "Mirror the selection left↔right / top↔bottom"),
    ("Del", "Remove the selected parts"),
    ("Esc", "Clear the selection"),
    ("Ctrl + S", "Save the job"),
    ("Ctrl + Shift + S", "Save the job as…"),
    ("Ctrl + O", "Open a saved job"),
    ("Ctrl + ,", "Settings"),
    ("Middle button / Space + drag", "Pan"),
    ("Mouse wheel", "Zoom"),
    ("Right-click (or S) on a part", "Quick ring: turn, turn back, copy on the pointer, mirror, delete, undo, fit (flick toward a tool, or click it)"),
    ("Right-click (or S) on empty sheet", "Quick ring: re-nest sheet, cut off leftover, sheet stock, next / previous sheet, undo, redo, fit"),
    ("Right-click (or S) in the Part view", "Quick ring: lead-ins, micro-joints, texts, loops, fit"),
];

#[derive(Default)]
pub struct SettingsUi {
    pub open: bool,
    pub section: Section,
    /// Connection / machine being edited.
    pub sel_conn: usize,
    pub sel_machine: usize,
    /// Password typed for the selected connection (never saved as text).
    pub pw_input: String,
    pub note: String,
    pub test: String,
    pub pending: Option<Receiver<String>>,
    /// Materials page: a name typed to see which material it means.
    pub mat_probe: String,
    /// Materials page: the list as the page is editing it (None until the
    /// page is shown, and again after a save or Undo changes).
    pub mat_draft: Option<crate::engine::materialedit::Draft>,
    /// Materials page: the row picked to edit.
    pub mat_sel: Option<usize>,
    /// Materials page: the catalogue standard Add from standard adds.
    pub mat_add: u32,
    /// Materials page: why the list can't be changed now (MA14), looked at
    /// when the page is read and on Try again; None: it can.
    pub mat_locked: Option<String>,
    /// Materials page: look again whether the list can be changed before
    /// it is next drawn (set on opening it and when the Library is looked
    /// at again).
    pub mat_recheck: bool,
    /// Materials page: the last result of Save or Delete (true: done), shown
    /// by the buttons.
    pub mat_note: Option<(bool, String)>,
    /// Machines page: the machine file's own sheet and hole order of the
    /// machine being edited, read once per file: (which files, its values,
    /// its start bottom is off, the file's name).
    pub order_file: Option<(String, crate::ct1::OrderSettings, bool, Option<String>)>,
    /// Scroll the Machines page down to the cut order (`--settings=machines-order`).
    pub show_order: bool,
    /// Shop page (SH5): the shop as last read: Err when the Library can't
    /// be reached, Ok(None) when it's no shop. None: read it when the page is
    /// next drawn (on opening Settings and on Try again, never every frame:
    /// a share that's down can take seconds to answer, LB11).
    pub shop: Option<Result<Option<crate::engine::shop::Status>, String>>,
    /// Shop page: Start a shop here was pressed, and the name typed for it.
    pub shop_start: Option<String>,
    /// Shop page: Make this PC the admin PC was pressed; the warning is open
    /// (SH7), about the shop folder shown then and the admin PC it named.
    /// Yes changes it only while both are still so.
    pub shop_admin_ask: Option<(std::path::PathBuf, String)>,
    /// Shop page: the last result of a change (true: done).
    pub shop_note: Option<(bool, String)>,
    /// Backups card (BK7): the backups as last read, with the shop (when
    /// the page is opened, on Try again and when a backup ends).
    pub backups: Option<Result<crate::backup::Overview, String>>,
    /// Backups card: Keep and the second place as typed, before Save
    /// backup settings.
    pub backup_edit: Option<crate::backup::Settings>,
    /// The backup running in the background, and how the last one went.
    pub backup_run: std::sync::Arc<std::sync::Mutex<BackupRun>>,
    /// The `BackupRun::ended` the card has seen.
    pub backup_seen: u64,
    /// BK8: Restore… was pressed: the list of backups is open, with the
    /// one picked, and whether it asks once more.
    pub restore: Option<RestoreAsk>,
    /// SH4: this PC adding itself to its shop in the background (at
    /// start-up, or Add this PC): the answer to come, and whether a person
    /// pressed Add this PC.
    pub shop_adding: Option<(Receiver<Result<bool, String>>, bool)>,
    /// SH4: why this PC's last try to add itself to the shop failed; shown on
    /// the PCs card with Add this PC until a try succeeds.
    pub shop_unrecorded: Option<String>,
    /// Updates on a PC of its own (UP11–UP14): the banner, What's new and
    /// the Updates card.
    pub upd: UpdateState,
}

/// UP11–UP14: what this window knows about releases.
#[derive(Default)]
pub struct UpdateState {
    /// Where releases come from. None: this window never checks (tests, and
    /// screenshots without a made-up list).
    pub source: Option<crate::updates::Source>,
    /// A check running off the window's thread.
    pub checking: Option<Receiver<crate::updates::Found>>,
    /// When the last check started (UP11: again a day later).
    pub checked_at: Option<std::time::Instant>,
    /// What the last check found.
    pub found: Option<crate::updates::Found>,
    /// UP12: the release offered, from the latest check.
    pub offer: Option<crate::engine::updates::Offer>,
    /// UP14: Update when I close.
    pub fetch: Fetch,
    /// The release chosen with Update when I close (the banner shows it
    /// while it downloads and waits for the program to close).
    pub chosen: Option<crate::engine::updates::Offer>,
    /// The Library folder this was found for: another one withdraws it all
    /// (S2).
    pub library: Option<std::path::PathBuf>,
    /// The banner was put away for this version (Later): it comes back for a
    /// newer one.
    pub later: Option<String>,
    /// What's new is open, with the notes of this release (the one the
    /// banner or the card showed when it was pressed).
    pub whats_new: Option<crate::engine::updates::Offer>,
    /// Screenshots: (press Update when I close, open What's new) once the
    /// check has answered.
    pub demo: (bool, bool),
}

impl UpdateState {
    /// S2: the Library folder these updates are for. When it changes (Settings
    /// → Folders, or joining a shop) everything found, offered and chosen
    /// for the other one is withdrawn, and this one is checked anew.
    pub fn follow_library(&mut self, dir: &std::path::Path, h: &crate::updates::Handoff) {
        if self.library.as_deref() == Some(dir) {
            return;
        }
        if self.library.is_some() {
            self.withdraw(h);
        }
        self.library = Some(dir.to_path_buf());
    }

    /// S2: forget what was found, offered and chosen (the Library changed or
    /// became a shop's): a running check's or download's answer is dropped,
    /// nothing installs on close, and the next frame checks again.
    pub fn withdraw(&mut self, h: &crate::updates::Handoff) {
        // S1: only a public release chosen here; a trial's or the shop's
        // (rolloutui) stays.
        h.withdraw_for(crate::updates::Why::Public);
        self.checking = None;
        self.checked_at = None;
        self.found = None;
        self.offer = None;
        self.fetch = Fetch::No;
        self.chosen = None;
        self.later = None;
        self.whats_new = None;
    }

    /// UP11, UP12: a check answered. A PC whose Library is a shop's now
    /// withdraws a release it chose from the public ones (S2).
    pub fn receive(&mut self, found: crate::updates::Found, h: &crate::updates::Handoff) {
        use crate::updates::Found;
        self.checking = None;
        self.offer = match &found {
            Found::Releases(l) => crate::engine::updates::offer(crate::updates::VERSION, l, None),
            _ => None,
        };
        // A public release chosen is withdrawn; the shop's own (UP21) is
        // left alone.
        if matches!(found, Found::InShop(_)) && self.chosen.is_some() {
            h.withdraw_for(crate::updates::Why::Public);
            self.fetch = Fetch::No;
            self.chosen = None;
            self.whats_new = None;
        }
        self.found = Some(found);
    }

    /// UP13: the release the banner is about: the chosen one while it
    /// downloads or waits for the program to close (even when a later check
    /// couldn't answer), else the one offered.
    pub fn banner_release(&self) -> Option<crate::engine::updates::Offer> {
        let fetching = !matches!(self.fetch, Fetch::No);
        (if fetching { self.chosen.clone() } else { None }).or_else(|| self.offer.clone())
    }

    /// S6: the newest release in the latest check.
    pub fn newest(&self) -> Option<&str> {
        match &self.found {
            Some(crate::updates::Found::Releases(l)) => crate::engine::updates::newest(l).map(|r| r.version.as_str()),
            _ => None,
        }
    }

    /// UP14: Update when I close: download the offered release in the
    /// background (`own`: this PC's own Library folder). It installs when the
    /// program closes, never before.
    pub fn choose(&mut self, own: std::path::PathBuf, h: &'static crate::updates::Handoff) {
        let (Some(src), Some(o), Some(dir)) = (self.source.clone(), self.offer.clone(), self.library.clone()) else { return };
        if matches!(self.fetch, Fetch::Running(..) | Fetch::Ready(_)) {
            return;
        }
        let c = crate::updates::Chosen { version: o.version.clone(), dir, own, src, why: crate::updates::Why::Public };
        self.fetch = Fetch::Running(o.version.clone(), crate::updates::download_in_background(h, c));
        self.chosen = Some(o);
    }

    /// UP14: pick up the download's answer. True while it runs.
    pub fn poll_fetch(&mut self) -> bool {
        let Fetch::Running(v, rx) = &self.fetch else { return false };
        match rx.try_recv() {
            Ok(Ok(())) => self.fetch = Fetch::Ready(v.clone()),
            Ok(Err(e)) => self.fetch = Fetch::Failed(v.clone(), e),
            Err(std::sync::mpsc::TryRecvError::Empty) => return true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.fetch = Fetch::Failed(v.clone(), format!("Couldn't download version {v}.")),
        }
        false
    }
}

/// UP14: the release chosen with Update when I close.
#[derive(Default)]
pub enum Fetch {
    #[default]
    No,
    /// Being downloaded, in the background.
    Running(String, Receiver<Result<(), String>>),
    /// Downloaded: it installs when the program closes.
    Ready(String),
    Failed(String, String),
}

/// A backup taken in the background (BK3, BK7): it's never taken on the
/// window's own thread, so a big Library never holds the window up.
#[derive(Default)]
pub struct BackupRun {
    /// Running since then (this PC's clock).
    pub since: Option<u64>,
    /// Counts the backups that ended.
    pub ended: u64,
    /// How the last one went (true: taken, or today's was there).
    pub note: Option<(bool, String)>,
    /// Save backup settings is waiting for the write lock (BK7).
    pub saving: bool,
    /// A restore running in the background since then (BK8).
    pub restoring: Option<u64>,
    /// What saving gave, for the window to take: whether they were the
    /// shop's, and this PC's own settings after it.
    pub saved: Option<(Result<bool, String>, crate::backup::Settings)>,
}

/// BK8: the Restore window: the backup picked, and whether the person
/// was asked once more (Restore pressed).
#[derive(Clone, Debug, Default)]
pub struct RestoreAsk {
    pub pick: Option<String>,
    pub sure: bool,
}

/// "lower left" → "Lower left".
fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
}

/// A small "changed" tag after a value changed for this machine, with the
/// machine file's value on hover (CO16, CO17).
fn changed_tag(ui: &mut egui::Ui, changed: bool, file: &str) {
    if changed {
        ui.label(RichText::new("changed").size(11.0).color(th().accent_fg)).on_hover_text(format!("The machine file gives: {file}. \"Use the machine file\" puts it back."));
    }
}

/// A rounded card with a title.
pub(crate) fn card<R>(ui: &mut egui::Ui, title: &str, sub: &str, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let t = th();
    egui::Frame::new()
        .fill(t.card)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).size(14.5).strong().color(t.text_strong));
            if !sub.is_empty() {
                ui.label(RichText::new(sub).size(12.5).color(t.text_dim));
            }
            ui.add_space(8.0);
            body(ui)
        })
        .inner
}

/// A button on a Settings page. `page` and `label` name it in
/// `engine::ops::GUI_MAP` ("Settings → Materials/Add blank"), so agents
/// keep up with it.
pub(crate) fn page_button(ui: &mut egui::Ui, page: &str, label: &str, enabled: bool) -> egui::Response {
    let _ = page;
    ui.add_enabled(enabled, egui::Button::new(label))
}

/// A labelled row inside a card.
pub(crate) fn row(ui: &mut egui::Ui, label: &str, body: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(150.0, 24.0), egui::Sense::hover());
        ui.painter().text(r.left_center(), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(13.0), th().text_dim);
        body(ui);
    });
    ui.add_space(2.0);
}

/// A file path box with Browse… and a "built-in" reset.
fn file_row(ui: &mut egui::Ui, label: &str, path: &mut String, filter: (&str, &[&str]), start: &str) -> bool {
    let mut changed = false;
    row(ui, label, |ui| {
        changed |= ui.add(egui::TextEdit::singleline(path).desired_width(360.0).hint_text("built-in values")).changed();
        if ui.button("Browse…").clicked() {
            let mut d = rfd::FileDialog::new().add_filter(filter.0, filter.1);
            if !start.is_empty() {
                d = d.set_directory(start);
            }
            if let Some(f) = d.pick_file() {
                *path = f.display().to_string();
                changed = true;
            }
        }
        if !path.is_empty() && ui.small_button("✖").on_hover_text("Use the built-in values").clicked() {
            path.clear();
            changed = true;
        }
    });
    changed
}

/// A clickable tile in a list of connections / machines.
pub(crate) fn tile(ui: &mut egui::Ui, selected: bool, active: bool, title: &str, sub: &str) -> bool {
    let t = th();
    let w = 210.0;
    let (r, resp) = ui.allocate_exact_size(egui::vec2(w, 54.0), egui::Sense::click());
    let hover = resp.hovered();
    let fill = if selected { t.accent_soft } else if hover { t.hover } else { t.card };
    ui.painter().rect(r, CornerRadius::same(9), fill, Stroke::new(if selected { 1.5 } else { 1.0 }, if selected { t.accent } else { t.border }), egui::StrokeKind::Inside);
    let clip = |s: &str, n: usize| if s.chars().count() > n { format!("{}…", s.chars().take(n).collect::<String>()) } else { s.to_string() };
    ui.painter().text(r.left_top() + egui::vec2(12.0, 10.0), egui::Align2::LEFT_TOP, clip(title, 24), egui::FontId::proportional(13.5), if selected { t.accent_fg } else { t.text_strong });
    ui.painter().text(r.left_top() + egui::vec2(12.0, 30.0), egui::Align2::LEFT_TOP, clip(sub, 30), egui::FontId::proportional(11.0), t.text_dim);
    if active {
        let c = r.right_top() + egui::vec2(-14.0, 14.0);
        ui.painter().circle_filled(c, 4.5, t.ok_fg);
    }
    resp.on_hover_text(format!("{title}\n{sub}{}", if active { "\n● in use" } else { "" })).clicked()
}

/// The connection string, as Expert builds it (DB2, CN1: no user → OLE DB with Windows
/// sign-in, otherwise the ODBC "SQL Server" driver), password masked.
pub fn conn_string(c: &Connection) -> String {
    let db = crate::dbconn::LantekDb {
        server: c.server.clone(),
        database: c.database.clone(),
        user: if c.windows { String::new() } else { c.user.clone() },
        has_password: !c.password_enc.is_empty(),
        ..Default::default()
    };
    db.connection_string()
}

impl App {
    pub fn open_settings(&mut self, s: Section) {
        self.setui.open = true;
        self.setui.section = s;
        self.setui.sel_conn = self.cfg.active_conn;
        self.setui.sel_machine = self.cfg.active_machine;
        // Read the machine files again: they may have changed.
        self.setui.order_file = None;
        // The Materials page looks again whether the list can be changed (MA21).
        self.setui.mat_recheck = true;
        // The Shop page reads the shop again (SH5).
        self.setui.shop = None;
    }

    /// Apply the settings: the active machine's post library and BLM post
    /// files, its output kind and gas; saves when anything changed.
    pub fn apply_settings(&mut self, force: bool) {
        let m = self.cfg.machine();
        if force || self.applied_machine.as_ref() != Some(&m) {
            let reload_post = force || self.applied_machine.as_ref().map(|a| &a.post_lib) != Some(&m.post_lib);
            if reload_post {
                self.post = crate::engine::load_post(&m);
            }
            // The machine's program format may have changed without its library.
            self.post.omax = m.output == Output::Omax;
            // EXP2: and with it which post files it needs.
            self.post.missing = crate::engine::unreadable_post_files(&m);
            // BLM post values and the machine's technology file: cutting zones
            // and their order (B110–B123), leads, joints, grouping.
            crate::engine::install_machine(&m, &self.cfg.lantek_dir);
            self.blm.on = m.output == Output::BlmSiemens;
            self.blm.machine = m.name.clone();
            self.blm.gas_id = m.gas_id;
            self.blm.gas_name = m.gas_name.clone();
            self.applied_machine = Some(m);
            self.refresh();
        }
        self.cfg.warn_tech = self.warn_tech;
        self.cfg.show_unbuilt = self.dv.show_unbuilt;
        self.cfg.theme = if self.follow_system { "system" } else if crate::theme::is_dark() { "dark" } else { "light" }.into();
        if self.cfg != self.cfg_saved {
            if !self.shot_mode_now() {
                if let Err(e) = self.cfg.save() {
                    self.setui.note = format!("Couldn't save settings: {e}");
                }
            }
            self.cfg_saved = self.cfg.clone();
        }
    }

    /// A screenshot run (--screenshot), which never saves the settings.
    pub(crate) fn shot_mode_now(&self) -> bool {
        self.shot.is_some() || std::env::args().any(|a| a == "--screenshot")
    }

    pub fn settings_window(&mut self, ctx: &egui::Context) {
        // SH4: this PC adding itself to its shop, whether Settings is open
        // or not (it starts when the program opens).
        if self.poll_add_this_pc() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        // BK7: Save backup settings' answer, whatever page is open or with
        // Settings closed, so this PC's own settings are always stored.
        self.backup_settings_saved();
        if self.setui.backup_run.lock().is_ok_and(|r| r.saving) {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        if let Some(rx) = &self.setui.pending {
            if let Ok(r) = rx.try_recv() {
                self.setui.test = r;
                self.setui.pending = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
        if !self.setui.open {
            self.apply_settings(false);
            return;
        }
        let mut open = true;
        let post_before = self.post.clone();
        egui::Window::new(RichText::new("Settings").size(16.0).strong())
            .id(egui::Id::unique("settings_window"))
            .pivot(egui::Align2::CENTER_TOP)
            .default_pos(crate::ui_parts::dialog_pos(ctx))
            .open(&mut open)
            // Fits between the ribbon and the status bar on a 1440 × 900 screen.
            .default_size([1040.0, (ctx.content_rect().height() - 176.0 - 96.0).clamp(400.0, 680.0)])
            .min_size([820.0, 400.0])
            .max_width(1200.0)
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    // Navigation.
                    ui.vertical(|ui| {
                        ui.set_width(200.0);
                        for (g, items) in Section::GROUPS {
                        ui.add_space(6.0);
                        ui.label(RichText::new(g).size(11.0).strong().color(th().text_dim));
                        ui.add_space(2.0);
                        for &s in items {
                            let sel = self.setui.section == s;
                            let (r, resp) = ui.allocate_exact_size(egui::vec2(196.0, 46.0), egui::Sense::click());
                            let t = th();
                            if sel || resp.hovered() {
                                ui.painter().rect_filled(r, CornerRadius::same(9), if sel { t.accent_soft } else { t.hover });
                            }
                            if sel {
                                ui.painter().rect_filled(egui::Rect::from_min_size(r.min + egui::vec2(0.0, 9.0), egui::vec2(3.0, 28.0)), CornerRadius::same(2), t.accent);
                            }
                            ui.painter().text(r.left_center() + egui::vec2(14.0, 0.0), egui::Align2::LEFT_CENTER, s.icon(), egui::FontId::proportional(18.0), if sel { t.accent_fg } else { t.text_dim });
                            ui.painter().text(r.left_top() + egui::vec2(44.0, 7.0), egui::Align2::LEFT_TOP, s.title(), egui::FontId::proportional(13.5), if sel { t.accent_fg } else { t.text_strong });
                            ui.painter().text(r.left_top() + egui::vec2(44.0, 26.0), egui::Align2::LEFT_TOP, s.hint(), egui::FontId::proportional(11.0), t.text_dim);
                            if resp.clicked() {
                                self.setui.section = s;
                                self.setui.mat_recheck = true;
                                self.setui.shop = None;
                            }
                            ui.add_space(2.0);
                        }
                        }
                        ui.add_space(12.0);
                        if self.setui.section == Section::Materials {
                            // MA21: the material list is kept only by Save materials.
                            ui.label(RichText::new("Kept only by Save materials").size(11.5).color(th().text_dim));
                        } else {
                            ui.label(RichText::new("Changes are saved automatically").size(11.5).color(th().text_dim)).on_hover_text(AppSettings::file().display().to_string());
                        }
                    });
                    ui.separator();
                    let out = egui::ScrollArea::vertical().id_salt("settings_body").auto_shrink([false, false]).show(ui, |ui| ui.vertical(|ui| {
                        ui.set_max_width(ui.available_width() - 8.0);
                        match self.setui.section {
                            Section::Shop => self.settings_shop(ui),
                            Section::General => self.settings_general(ui),
                            Section::Appearance => self.settings_appearance(ui),
                            Section::Shortcuts => self.settings_shortcuts(ui),
                            Section::About => self.settings_about(ui),
                            Section::Connections => self.settings_connections(ui),
                            Section::Machines => self.settings_machines(ui),
                            Section::Nesting => self.settings_nesting(ui),
                            Section::Materials => self.settings_materials(ui),
                            Section::Folders => self.settings_folders(ui),
                            Section::Integrations => self.settings_integrations(ui),
                        }
                        if !self.setui.note.is_empty() {
                            ui.add_space(6.0);
                            ui.label(RichText::new(&self.setui.note).size(11.5).color(th().accent_fg));
                        }
                    }));
                    crate::ui_parts::fade_if_more(ui, &out);
                });
            });
        if !open {
            self.setui.open = false;
        }
        if self.post != post_before {
            self.refresh();
        }
        self.apply_settings(false);
    }

    fn settings_general(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("General").size(18.0).strong().color(th().text_strong));
        ui.add_space(8.0);
        card(ui, "Part technology", "When you change a part that has its own technology (leads, micro-joints, texts).", |ui| {
            ui.checkbox(&mut self.warn_tech, "Ask before changing a part that has technology");
            if self.tech_answer_all.is_some() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("You chose an answer for the rest of the parts.").size(11.5).color(th().text_dim));
                    if ui.button("Ask again").clicked() {
                        self.tech_answer_all = None;
                    }
                });
            }
        });
        ui.add_space(10.0);
        // CU1: the original's Concurrency management, kept on this PC only.
        card(ui, "Jobs in use", "When several PCs share one Library, a job open on one of them is in use there: the others see who has it and since when. Kept on this PC only.", |ui| {
            ui.checkbox(&mut self.cfg.in_use_warn, "Warn when opening a job someone else has in use").on_hover_text("Also marks the jobs open here as in use for the other PCs. Off: this PC neither marks its jobs nor asks; a job someone else has in use still can't be deleted, renamed or moved from here.");
            // Allow is only shown while warn is on (CU1).
            if self.cfg.in_use_warn {
                ui.checkbox(&mut self.cfg.in_use_allow, "Allow opening it anyway").on_hover_text("Off: a job someone else has in use can only be taken over, after a warning, or left closed.");
            }
        });
        ui.add_space(10.0);
        card(ui, "Part data from the file name", "When DXF names follow a pattern, the reference and quantity are read from the name. Fields: [Reference] [Quantity] [Material] [Thickness], * skips text; the text between fields is the separator.", |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.cfg.name_format).hint_text("[Reference]_[Quantity]_[Material]").desired_width(320.0));
                if ui.button("Example").clicked() {
                    self.cfg.name_format = "[Reference]_[Quantity]_[Material]".into();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Reference");
                ui.add(egui::TextEdit::singleline(&mut self.cfg.ref_prefix).hint_text("prefix").desired_width(90.0));
                ui.label(RichText::new("+ name +").color(th().text_dim));
                ui.add(egui::TextEdit::singleline(&mut self.cfg.ref_suffix).hint_text("suffix").desired_width(90.0));
                ui.label(RichText::new(format!("(at most {} characters, the name is shortened)", crate::filename::REF_MAX_CHARS)).size(11.5).color(th().text_dim));
            });
            if !self.cfg.name_format.trim().is_empty() {
                match crate::filename::parse_format(&self.cfg.name_format) {
                    Ok(f) => {
                        let sample: Vec<String> = f.items.iter().map(|(fl, sep)| format!("{}{}", match fl {
                            crate::filename::Field::Reference => "PLATE-7",
                            crate::filename::Field::Quantity => "12",
                            crate::filename::Field::Material => "S235",
                            crate::filename::Field::Thickness => "3",
                            crate::filename::Field::Ignore => "JOB88",
                        }, sep)).collect();
                        ui.label(RichText::new(format!("✔ e.g. {}.dxf", sample.concat())).size(11.5).color(th().text_dim));
                    }
                    Err(e) => {
                        ui.label(RichText::new(format!("⚠ {e}")).size(11.5).color(th().warn_fg));
                    }
                }
            }
        });
        ui.add_space(10.0);
        card(ui, "Example jobs", "Five example jobs to try the app with. Turn off to start with one empty job.", |ui| {
            ui.checkbox(&mut self.cfg.examples, "Start with the example jobs (takes effect next start)");
        });
        ui.add_space(10.0);
        card(ui, "Database jobs", "The Job Explorer can list the jobs of the connection in use, next to this PC's jobs.", |ui| {
            ui.checkbox(&mut self.cfg.auto_db, "Load them at start-up when the connection can sign in by itself (Windows account or remembered password)");
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Connection in use: {}", self.cfg.conn().map_or("none".to_string(), |c| c.name.clone()))).size(11.5).color(th().text_dim));
                if ui.small_button("Connections…").clicked() {
                    self.setui.section = Section::Connections;
                }
            });
        });
        ui.add_space(10.0);
        card(ui, "Commands", "Some commands aren't available yet.", |ui| {
            ui.checkbox(&mut self.dv.show_unbuilt, "Show them greyed out in the ribbons");
            if ui.button("Drawing commands coming soon…").clicked() {
                self.dv.show_missing = true;
            }
        });
    }

    fn settings_appearance(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Appearance").size(18.0).strong().color(th().text_strong));
        ui.add_space(8.0);
        card(ui, "Theme", "Also on the sun / moon switch at the top right.", |ui| {
            ui.horizontal(|ui| {
                let cur = if self.follow_system { 2 } else if crate::theme::is_dark() { 1 } else { 0 };
                for (k, (icon, name, sub)) in [("☀", "Light", "Bright workshop screens"), ("🌙", "Dark", "Easy on the eyes"), ("◐", "Follow Windows", "Matches the system")].into_iter().enumerate() {
                    let t = th();
                    let (r, resp) = ui.allocate_exact_size(egui::vec2(190.0, 84.0), egui::Sense::click());
                    let sel = cur == k;
                    ui.painter().rect(r, CornerRadius::same(10), if sel { t.accent_soft } else if resp.hovered() { t.hover } else { t.card }, Stroke::new(if sel { 1.5 } else { 1.0 }, if sel { t.accent } else { t.border }), egui::StrokeKind::Inside);
                    ui.painter().text(r.left_top() + egui::vec2(14.0, 12.0), egui::Align2::LEFT_TOP, icon, egui::FontId::proportional(22.0), if sel { t.accent_fg } else { t.text_dim });
                    ui.painter().text(r.left_top() + egui::vec2(14.0, 42.0), egui::Align2::LEFT_TOP, name, egui::FontId::proportional(14.0), if sel { t.accent_fg } else { t.text_strong });
                    ui.painter().text(r.left_top() + egui::vec2(14.0, 61.0), egui::Align2::LEFT_TOP, sub, egui::FontId::proportional(11.0), t.text_dim);
                    if resp.clicked() {
                        match k {
                            0 | 1 => {
                                self.follow_system = false;
                                crate::theme::set_dark(ui.ctx(), k == 1);
                            }
                            _ => self.follow_system = true,
                        }
                    }
                    ui.add_space(6.0);
                }
            });
        });
        ui.add_space(10.0);
        self.settings_ui_size(ui);
    }

    fn settings_ui_size(&mut self, ui: &mut egui::Ui) {
        card(ui, "Scrolling", "How far one turn of the mouse wheel scrolls lists and panels.", |ui| {
            ui.add(egui::Slider::new(&mut self.cfg.scroll_speed, 1.0..=5.0).step_by(0.5).suffix("×"));
        });
        ui.add_space(10.0);
        card(ui, "Interface size", "Bigger text and buttons for shop-floor screens (Expert's ribbon font sizes).", |ui| {
            ui.horizontal(|ui| {
                for (k, name) in crate::settings::UI_SIZES.iter().enumerate() {
                    if ui.add(egui::Button::new(*name).selected(self.cfg.ui_size as usize == k).min_size(egui::vec2(120.0, 30.0))).clicked() {
                        self.cfg.ui_size = k as u8;
                    }
                }
            });
        });
    }

    fn settings_nesting(&mut self, ui: &mut egui::Ui) {
        use crate::units::{drag_len, LenUnit};
        ui.label(RichText::new("Nesting").size(18.0).strong().color(th().text_strong));
        ui.add_space(8.0);
        card(ui, "Units", "Every length in the app is shown and typed in this unit: sheet sizes, thickness, part sizes, gap and margin, the cursor position, the stock list and remnants. Inches can be typed as fractions (1/4, 1 1/2), decimals or steel gauges (10ga); either unit can be typed with its suffix (6.35mm, 1/4in). DXF and DWG drawings that don't say their unit are read in it too. Machine programs keep the unit set for the machine (Machines & output).", |ui| {
            ui.horizontal(|ui| {
                for u in [LenUnit::Mm, LenUnit::Inch] {
                    ui.radio_value(&mut self.cfg.nesting.units, u, u.label());
                }
            });
        });
        ui.add_space(10.0);
        let u = self.cfg.nesting.units;
        card(ui, "New jobs start with", "The smallest distance between two parts, and the distance every part keeps from the sheet edge. A job keeps its own values (Nesting screen → Spacing).", |ui| {
            row(ui, "Part gap", |ui| {
                drag_len(ui, &mut self.cfg.nesting.part_gap, u, 50.0);
                ui.label(RichText::new(u.other(self.cfg.nesting.part_gap)).size(11.5).color(th().text_dim));
            });
            row(ui, "Edge margin", |ui| {
                drag_len(ui, &mut self.cfg.nesting.edge_margin, u, 100.0);
                ui.label(RichText::new(u.other(self.cfg.nesting.edge_margin)).size(11.5).color(th().text_dim));
            });
            ui.horizontal(|ui| {
                let same = self.spec.gap == self.cfg.nesting.part_gap && self.spec.margin == self.cfg.nesting.edge_margin;
                if ui.add_enabled(!same, egui::Button::new("Use for the open job too")).on_hover_text("Set the open job's part gap and edge margin to these (nest again afterwards)").clicked() {
                    self.checkpoint();
                    self.spec.gap = self.cfg.nesting.part_gap;
                    self.spec.margin = self.cfg.nesting.edge_margin;
                    self.stale = true;
                    self.refresh();
                    self.setui.note = "The open job now uses these — nest again to apply them to its sheets.".into();
                }
                ui.label(RichText::new(format!("Open job: gap {}, margin {}", u.show(self.spec.gap), u.show(self.spec.margin))).size(11.5).color(th().text_dim));
            });
        });
        ui.add_space(10.0);
        card(ui, "Moving parts by hand", "Select parts on the sheet (click, Shift/Ctrl + click, or drag a window on empty sheet), then: Ctrl + arrow keys slide them until they meet the next part at the part gap (or the margin); Alt + arrow keys move them exactly by the step — like the original, that move doesn't stop at other parts (the status line warns when they end up closer than the part gap). Lantek's own values are taken from its install the first time the app starts.", |ui| {
            row(ui, "Step (Alt + arrows)", |ui| {
                if drag_len(ui, &mut self.cfg.nesting.step, u, 1000.0).changed() {
                    self.cfg.nesting.step = self.cfg.nesting.step.max(0.001);
                }
                ui.label(RichText::new(u.other(self.cfg.nesting.step)).size(11.5).color(th().text_dim));
            });
            ui.checkbox(&mut self.cfg.nesting.snap, "Snap dragged parts to one part gap from a neighbour or to the margin (hold Alt while dragging to place freely)");
            ui.add_enabled_ui(self.cfg.nesting.snap, |ui| {
                row(ui, "Snap reach", |ui| {
                    ui.add(egui::DragValue::new(&mut self.cfg.nesting.snap_px).range(2.0..=60.0).speed(0.5).suffix(" px on screen"));
                });
            });
            ui.checkbox(&mut self.cfg.nesting.halo, "Show the part gap as a dashed outline around parts being dragged");
            ui.checkbox(&mut self.cfg.nesting.keys_clockwise, "Ctrl + 1/5/0/3/4/9 turn clockwise (off: counter-clockwise)");
            row(ui, "Turn step", |ui| {
                ui.horizontal_wrapped(|ui| {
                    for v in crate::turnbar::STEPS {
                        let label = if v == 0.0 { "Free".to_string() } else { crate::nest::angle_text(v) };
                        if ui.selectable_label((self.cfg.nesting.turn_step - v).abs() < 1e-9, label).clicked() {
                            self.cfg.nesting.turn_step = v;
                        }
                    }
                    let mut v = if crate::turnbar::STEPS.iter().any(|&s| (s - self.cfg.nesting.turn_step).abs() < 1e-9) { self.turn_custom } else { self.cfg.nesting.turn_step };
                    if ui.add(egui::DragValue::new(&mut v).range(0.1..=crate::settings::TURN_STEP_MAX).speed(0.25).max_decimals(2).suffix("°")).on_hover_text("Your own step").changed() {
                        self.turn_custom = v;
                        self.cfg.nesting.turn_step = v;
                    }
                });
            });
            ui.label(RichText::new("What Ctrl + Q / Ctrl + W and the turn bar's buttons turn by, and what a turn by the round handle snaps to (Free: any angle, to 0.1°; the keys then turn 1°).").size(11.5).color(th().text_dim));
            ui.checkbox(&mut self.cfg.nesting.turn_bar, "Show the turn bar under selected parts (turn by the step, type an angle, choose the step)");
        });
    }

    fn settings_shortcuts(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Shortcuts").size(18.0).strong().color(th().text_strong));
            ui.add_space(16.0);
            // Expert v44's shortcut list has Copy, keeping the list's layout.
            if ui.button("Copy list").clicked() {
                let w = SHORTCUTS.iter().map(|s| s.0.len()).max().unwrap_or(0);
                let text: String = SHORTCUTS.iter().map(|(k, d)| format!("{k:<w$}  {d}\n")).collect();
                ui.ctx().copy_text(text);
                self.setui.note = "Shortcut list copied.".into();
            }
        });
        ui.add_space(8.0);
        card(ui, "Keyboard and mouse", "", |ui| {
            egui::Grid::new("shortcut_grid").num_columns(2).spacing([24.0, 10.0]).show(ui, |ui| {
                for (k, what) in SHORTCUTS {
                    egui::Frame::new().fill(th().hover).corner_radius(CornerRadius::same(5)).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
                        ui.label(RichText::new(*k).monospace().size(12.0).color(th().text_strong));
                    });
                    ui.label(*what);
                    ui.end_row();
                }
            });
        });
    }

    fn settings_about(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("About").size(18.0).strong().color(th().text_strong));
        ui.add_space(8.0);
        card(ui, "Sheet nesting", "A rebuild of a sheet-metal CAD/CAM nesting system.", |ui| {
            // UP3: the version is the package version a release is tagged with.
            row(ui, "Version", |ui| {
                ui.label(RichText::new(crate::updates::VERSION).strong());
            });
            if let Some(note) = crate::updates::install_note() {
                row(ui, "Installer", |ui| {
                    ui.label(RichText::new(note).color(th().text_dim));
                });
            }
            row(ui, "Settings file", |ui| {
                ui.label(RichText::new(AppSettings::file().display().to_string()).monospace().size(11.5));
            });
            row(ui, "Machine in use", |ui| {
                ui.label(self.cfg.machine().name);
            });
            row(ui, "Database in use", |ui| {
                ui.label(self.cfg.conn().map_or("none".to_string(), |c| c.name.clone()));
            });
        });
        ui.add_space(10.0);
        card(ui, "Compared with the original", "", |ui| {
            if ui.button("Drawing commands coming soon…").clicked() {
                self.dv.show_missing = true;
            }
        });
    }

    fn import_lantek_ui(&mut self) {
        let start = if self.cfg.lantek_dir.is_empty() { crate::settings::lantek_default_dir() } else { self.cfg.lantek_dir.clone() };
        if let Some(d) = rfd::FileDialog::new().set_title("Lantek Expert folder (with LANTEKIN.CNI)").set_directory(&start).pick_folder() {
            self.setui.note = self.cfg.import_lantek(&d.display().to_string());
            self.setui.sel_conn = self.cfg.active_conn;
            self.setui.sel_machine = self.cfg.active_machine;
        }
    }

    fn settings_connections(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Database connections").size(18.0).strong().color(th().text_strong));
            ui.add_space(16.0);
            if ui.button("+ New connection").clicked() {
                self.cfg.connections.push(Connection::default());
                self.setui.sel_conn = self.cfg.connections.len() - 1;
                self.setui.pw_input.clear();
            }
            if ui.button("Import from Lantek…").on_hover_text("Add the database and machines of a Lantek Expert install, and its settings for moving parts by hand (step, turn direction, gap outline)").clicked() {
                self.import_lantek_ui();
            }
        });
        ui.label(RichText::new("Any SQL Server database — Lantek Expert's or another. The one marked ● opens in Browse database.").size(11.5).color(th().text_dim));
        ui.add_space(8.0);
        if self.cfg.connections.is_empty() {
            card(ui, "No connections yet", "Add one, or import the database a Lantek Expert install uses.", |_| {});
            return;
        }
        ui.horizontal_wrapped(|ui| {
            for k in 0..self.cfg.connections.len() {
                let c = &self.cfg.connections[k];
                let sub = if c.server.is_empty() { "not set up".to_string() } else { format!("{} · {}", c.database, c.server) };
                if tile(ui, self.setui.sel_conn == k, self.cfg.active_conn == k, &c.name, &sub) {
                    self.setui.sel_conn = k;
                    self.setui.pw_input.clear();
                    self.setui.test.clear();
                }
            }
        });
        ui.add_space(10.0);
        let k = self.setui.sel_conn.min(self.cfg.connections.len() - 1);
        self.setui.sel_conn = k;
        let mut delete = false;
        let mut use_it = false;
        let mut browse = false;
        let mut test = false;
        card(ui, "Connection", "Where the database is and how to sign in.", |ui| {
            let c = &mut self.cfg.connections[k];
            row(ui, "Name", |ui| {
                ui.add(egui::TextEdit::singleline(&mut c.name).desired_width(280.0));
            });
            row(ui, "Server", |ui| {
                ui.add(egui::TextEdit::singleline(&mut c.server).desired_width(280.0).hint_text("host\\instance, host or host,port"));
            });
            row(ui, "Database", |ui| {
                ui.add(egui::TextEdit::singleline(&mut c.database).desired_width(280.0).hint_text("database name"));
            });
            row(ui, "Sign in with", |ui| {
                ui.radio_value(&mut c.windows, true, "My Windows account");
                ui.radio_value(&mut c.windows, false, "SQL Server login");
            });
            if !c.windows {
                row(ui, "User name", |ui| {
                    ui.add(egui::TextEdit::singleline(&mut c.user).desired_width(200.0).hint_text("e.g. sa or lantek"));
                });
                row(ui, "Password", |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.setui.pw_input).password(true).desired_width(200.0).hint_text(if c.password_enc.is_empty() { "password" } else { "remembered — type to change" }));
                    if ui.add_enabled(!self.setui.pw_input.is_empty(), egui::Button::new("Remember")).on_hover_text("Saved encrypted for your Windows account on this PC").clicked() {
                        match crate::settings::protect(&self.setui.pw_input) {
                            Ok(enc) => {
                                c.password_enc = enc;
                                self.setui.pw_input.clear();
                                self.setui.note = "Password remembered (encrypted for your Windows account).".into();
                            }
                            Err(e) => self.setui.note = e,
                        }
                    }
                    if !c.password_enc.is_empty() {
                        chip(ui, "remembered", th().ok_bg, th().ok_fg);
                        if ui.small_button("Forget").clicked() {
                            c.password_enc.clear();
                        }
                    }
                });
            }
            ui.add_space(4.0);
            ui.label(RichText::new(conn_string(c)).monospace().size(11.0).color(th().text_dim));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let active = self.cfg.active_conn == k;
                if ui.add_enabled(!active, egui::Button::new(if active { "● In use" } else { "Use this connection" })).clicked() {
                    use_it = true;
                }
                if ui.add_enabled(self.setui.pending.is_none(), egui::Button::new("Test server")).on_hover_text("Checks the server answers (no sign-in)").clicked() {
                    test = true;
                }
                if ui.button("Open database…").on_hover_text("Sign in and browse jobs and tables").clicked() {
                    browse = true;
                }
                ui.add_space(24.0);
                if ui.button(RichText::new("Delete").color(th().warn_fg)).clicked() {
                    delete = true;
                }
            });
            if !self.setui.test.is_empty() {
                let col = if self.setui.test.starts_with('✔') { th().ok_fg } else { th().warn_fg };
                ui.label(RichText::new(&self.setui.test).color(col));
            }
        });
        if test {
            let c = self.cfg.connections[k].clone();
            let db = crate::dbconn::LantekDb { server: c.server, database: c.database, ..Default::default() };
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(crate::dbconn::test_reach(&db));
            });
            self.setui.pending = Some(rx);
            self.setui.test = "Testing…".into();
        }
        if use_it || browse {
            self.cfg.active_conn = k;
        }
        if browse {
            if !self.setui.pw_input.is_empty() {
                self.dbb.password = self.setui.pw_input.clone();
            }
            self.open_db_browser();
            self.dbb.open = true;
            self.db_open_now();
        }
        if delete {
            self.cfg.connections.remove(k);
            self.setui.sel_conn = k.saturating_sub(1);
            if self.cfg.active_conn >= self.cfg.connections.len() {
                self.cfg.active_conn = self.cfg.connections.len().saturating_sub(1);
            }
        }
    }

    fn settings_machines(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Machines & output").size(18.0).strong().color(th().text_strong));
            ui.add_space(16.0);
            if ui.button("+ New machine").clicked() {
                self.cfg.machines.push(Machine { name: format!("Machine {}", self.cfg.machines.len() + 1), ..Default::default() });
                self.setui.sel_machine = self.cfg.machines.len() - 1;
            }
            if ui.button("Import from Lantek…").clicked() {
                self.import_lantek_ui();
            }
        });
        ui.label(RichText::new("Your shop's machines: what each one does and its program format. Pick the machine a job is cut on from the Sheet tab; the one marked ● is in use.").size(11.5).color(th().text_dim));
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for k in 0..self.cfg.machines.len() {
                let m = &self.cfg.machines[k];
                let sub = format!("{} · {}", m.kind.process(), output_name(m.output));
                if tile(ui, self.setui.sel_machine == k, self.cfg.active_machine == k, &m.name, &sub) {
                    self.setui.sel_machine = k;
                }
            }
        });
        ui.add_space(10.0);
        let k = self.setui.sel_machine.min(self.cfg.machines.len() - 1);
        self.setui.sel_machine = k;
        let start = self.cfg.lantek_dir.clone();
        let mut delete = false;
        let mut use_now = false;
        let kind_before = self.cfg.machines[k].kind;
        card(ui, "Machine", "", |ui| {
            let can_delete = self.cfg.machines.len() > 1;
            let active = self.cfg.active_machine == k;
            let m = &mut self.cfg.machines[k];
            row(ui, "Name", |ui| {
                ui.add(egui::TextEdit::singleline(&mut m.name).desired_width(280.0));
            });
            // What it does: picking the machine on the Sheet tab sets the job to it (MK1).
            row(ui, "What it does", |ui| {
                egui::ComboBox::from_id_salt("machine_kind").width(180.0).selected_text(m.kind.process()).show_ui(ui, |ui| {
                    for k in crate::machine::MachineKind::ALL {
                        ui.selectable_value(&mut m.kind, k, k.process()).on_hover_text(k.describe());
                    }
                });
                ui.label(RichText::new(m.kind.describe()).size(11.5).color(th().text_dim));
            });
            row(ui, "Program format", |ui| {
                for (o, title) in [(Output::Iso, "ISO G-code (.CNC) — most controls"), (Output::BlmSiemens, "BLM / Siemens 840D (.SPF set + XML, zipped)"), (Output::Omax, "OMAX waterjet (.omx path file)")] {
                    ui.radio_value(&mut m.output, o, title);
                }
            });
            // GS17: the machine's gas, a pick from the list: the gas a new
            // job starts with (GS12). A saved number and name that aren't the
            // list's pair stay as saved until a gas is picked; nothing is
            // written by just looking.
            row(ui, "Gas", |ui| {
                let shown = m.gas().map_or_else(|| format!("{} \"{}\" (as saved)", m.gas_id, m.gas_name), |g| g.name().to_string());
                let mut picked = None;
                egui::ComboBox::from_id_salt("machine_gas")
                    .width(180.0)
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        for g in crate::gas::Gas::ALL {
                            let what = if g.program_name() == g.name() { format!("Gas {} in the LC5 program", g.number()) } else { format!("Gas {} in the LC5 program, written {}", g.number(), g.program_name()) };
                            if ui.selectable_label(m.gas() == Some(g), g.name()).on_hover_text(what).clicked() {
                                picked = Some(g);
                            }
                        }
                    })
                    .response
                    .on_hover_text("The gas a new job on this machine starts with; each job keeps its own, picked on the Sheet tab. Jobs saved before jobs carried a gas are cut with this one.");
                if let Some(g) = picked {
                    m.pick_gas(g);
                }
            });
            if let Some(note) = m.gas_mismatch() {
                ui.horizontal(|ui| {
                    ui.add_space(154.0);
                    ui.add(egui::Label::new(RichText::new(format!("⚠ {note}")).size(11.5).color(th().warn_fg)).wrap());
                });
            }
            ui.horizontal(|ui| {
                ui.add_space(154.0);
                if ui.add_enabled(!active, egui::Button::new(if active { "● In use" } else { "Use this machine" })).on_hover_text("Cut the open job on this machine (the same as picking it on the Sheet tab)").clicked() {
                    use_now = true;
                }
                if can_delete && ui.button(RichText::new("Delete").color(th().warn_fg)).clicked() {
                    delete = true;
                }
            });
        });
        // In use = the open job is cut on it too (MK1).
        if use_now || (self.cfg.active_machine == k && self.spec.machine != self.cfg.machines[k].kind && kind_before != self.cfg.machines[k].kind) {
            self.cfg.active_machine = k;
            self.spec.machine = self.cfg.machines[k].kind;
            self.refresh();
        }
        if delete {
            self.cfg.machines.remove(k);
            self.setui.sel_machine = k.saturating_sub(1);
            self.cfg.active_machine = self.cfg.active_machine.min(self.cfg.machines.len() - 1);
            return;
        }
        ui.add_space(10.0);
        let output = self.cfg.machines[k].output;
        if output == Output::Iso {
            card(ui, "Post library", "Block numbers, units, axis signs and file name. Load a .CFF / .CF1 library or edit the values.", |ui| {
                let m = &mut self.cfg.machines[k];
                file_row(ui, "Library file", &mut m.post_lib, ("Post library", &["CFF", "cff", "CF1", "cf1", "CF2", "cf2"]), &start);
                let editing_active = self.cfg.active_machine == k;
                if !editing_active {
                    ui.label(RichText::new("Use this machine to edit its values below.").size(11.5).color(th().text_dim));
                    return;
                }
                let p = &mut self.post;
                ui.add_space(4.0);
                row(ui, "Block prefix", |ui| {
                    ui.add(egui::TextEdit::singleline(&mut p.prefix).desired_width(60.0));
                });
                row(ui, "First block / step", |ui| {
                    ui.add(egui::DragValue::new(&mut p.first).range(0..=99999));
                    ui.add(egui::DragValue::new(&mut p.step).range(1..=1000));
                });
                row(ui, "Digits", |ui| {
                    ui.add(egui::DragValue::new(&mut p.block_digits).range(1..=9));
                    let mut keep = p.zero_suppress == 0;
                    if ui.checkbox(&mut keep, "keep leading zeros (N00010)").changed() {
                        p.zero_suppress = if keep { 0 } else { 1 };
                    }
                });
                row(ui, "Units", |ui| {
                    ui.radio_value(&mut p.units, 1.0, "mm");
                    ui.radio_value(&mut p.units, 25.4, "inch");
                });
                row(ui, "Axes", |ui| {
                    let mut fx = p.sign_x < 0.0;
                    let mut fy = p.sign_y < 0.0;
                    if ui.checkbox(&mut fx, "flip X").changed() {
                        p.sign_x = if fx { -1.0 } else { 1.0 };
                    }
                    if ui.checkbox(&mut fy, "flip Y").changed() {
                        p.sign_y = if fy { -1.0 } else { 1.0 };
                    }
                });
                row(ui, "Circles", |ui| {
                    ui.checkbox(&mut p.circles_one_move, "full circle as one G2/G3");
                });
                row(ui, "File extension", |ui| {
                    ui.add(egui::TextEdit::singleline(&mut p.extension).desired_width(80.0));
                });
            });
        } else {
            card(ui, "BLM / Siemens post", "The post's own settings files give the cut-off overruns, CORNER angle and arc tolerance.", |ui| {
                let m = &mut self.cfg.machines[k];
                file_row(ui, "Post settings (.PS1)", &mut m.blm_ps1, ("BLM post settings", &["PS1", "ps1"]), &start);
                file_row(ui, "Post library (.CF1)", &mut m.blm_cf1, ("BLM post library", &["CF1", "cf1"]), &start);
                let a = crate::blm::AdiParams::from_files(&m.blm_ps1, &m.blm_cf1);
                row(ui, "Values in use", |ui| {
                    ui.label(RichText::new(format!("overrun −{} / +{} mm · CORNER ≤ {}° · arcs within {} mm", a.trim_neg, a.trim_pos, a.corner_deg, a.arc_tol)).size(12.0));
                });
            });
        }
        ui.add_space(10.0);
        self.settings_cut_order(ui, k);
        ui.add_space(10.0);
        if self.cfg.active_machine == k {
            let sheet_no = self.cur_sheet + 1;
            let text = if self.blm.on {
                match (self.result.sheets.get(self.cur_sheet), self.plans.get(self.cur_sheet)) {
                    (Some(s), Some(pl)) => {
                        let set = crate::blm::sheet_set(sheet_no, &self.parts, s, &self.sheet_spec(sheet_no - 1), pl, self.cut_list(), &self.blm_now());
                        format!("{}  +  {}.XML\n\n{}", set.spf.iter().map(|f| f.0.clone()).collect::<Vec<_>>().join(", "), set.nest, set.spf[0].1.replace("\r\n", "\n"))
                    }
                    _ => "Nest a job to see its program.".into(),
                }
            } else {
                self.program.clone()
            };
            let name = self.cnc_name(sheet_no);
            card(ui, &format!("Preview — {name}"), "", |ui| {
                let preview: String = text.lines().take(18).collect::<Vec<_>>().join("\n");
                ui.add(egui::Label::new(RichText::new(preview).monospace().size(11.0)));
            });
        }
    }

    /// Machine `k`'s sheet order and hole order (CO16, CO17): each value
    /// starts as its machine file's; a change is kept for this machine and
    /// wins over the file; "Use the machine file" puts the file's back.
    fn settings_cut_order(&mut self, ui: &mut egui::Ui, k: usize) {
        use crate::ct1::{HoleMode, OrderChanges, SheetOrder};
        let m = self.cfg.machines[k].clone();
        let key = format!("{}|{}|{}|{}", m.post_lib, m.blm_ps1, m.blm_cf1, self.cfg.lantek_dir);
        if self.setui.order_file.as_ref().is_none_or(|c| c.0 != key) {
            let t = crate::ct1::MachineTech::for_machine(&m, &self.cfg.lantek_dir);
            let name = t.source.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
            self.setui.order_file = Some((key, t.file_order(), t.file_says_top(), name));
        }
        let Some((_, file, says_top, file_name)) = self.setui.order_file.clone() else { return };
        let unit = self.cfg.nesting.units;
        let mut now = m.order.apply(file);
        let (mut edited, mut reset) = (false, false);
        let dim = |ui: &mut egui::Ui, s: &str| {
            ui.horizontal(|ui| {
                ui.add_space(154.0);
                ui.label(RichText::new(s).size(11.5).color(th().text_dim));
            });
        };
        let from = match &file_name {
            Some(n) => format!("Each value starts as the machine file's ({n}); a change is kept for this machine and wins over the file."),
            None => "No machine file was found, so each value starts as the default; a change is kept for this machine.".to_string(),
        };
        if self.setui.show_order {
            ui.scroll_to_cursor(Some(egui::Align::TOP));
            // A screenshot keeps it in view; a person can scroll on.
            self.setui.show_order = self.shot_mode_now();
        }
        card(ui, "Cut order", &format!("How this machine orders the parts on a sheet, and a part's holes nobody has ordered by hand. {from}"), |ui| {
            // The sheet's order (CZ1–CZ6, CO27 (c)).
            ui.label(RichText::new("Sheet order").size(13.0).strong().color(th().text_strong));
            let (s, f) = (&mut now.sheet, &file.sheet);
            row(ui, "Start corner", |ui| {
                egui::ComboBox::from_id_salt("sheet_start_corner").width(150.0).selected_text(cap(s.corner_label())).show_ui(ui, |ui| {
                    for (l, b) in [(true, true), (false, true), (true, false), (false, false)] {
                        let o = SheetOrder { start_left: l, start_bottom: b, ..*s };
                        if ui.selectable_label(s.start_left == l && s.start_bottom == b, cap(o.corner_label())).clicked() {
                            (s.start_left, s.start_bottom) = (l, b);
                            edited = true;
                        }
                    }
                })
                .response
                .on_hover_text("The corner of the sheet the first zone is in");
                changed_tag(ui, (s.start_left, s.start_bottom) != (f.start_left, f.start_bottom), &cap(f.corner_label()));
            });
            if says_top && s.start_bottom {
                dim(ui, "The machine file's start bottom is off, but sheets are cut from the bottom up, as programs from the machine are. Pick an upper corner to start at the top.");
            }
            row(ui, "Run", |ui| {
                edited |= ui.radio_value(&mut s.horizontal, false, "Columns").on_hover_text("Up or down the sheet, then the next column").changed();
                edited |= ui.radio_value(&mut s.horizontal, true, "Rows").on_hover_text("Across the sheet, then the next row").changed();
                changed_tag(ui, s.horizontal != f.horizontal, if f.horizontal { "Rows" } else { "Columns" });
            });
            row(ui, "Zig-zag", |ui| {
                edited |= ui.checkbox(&mut s.zigzag, "every other column or row runs back").changed();
                changed_tag(ui, s.zigzag != f.zigzag, if f.zigzag { "on" } else { "off" });
            });
            row(ui, "Cutting zones", |ui| {
                let max = crate::ct1::MAX_ZONES;
                edited |= ui.add(egui::DragValue::new(&mut s.zones_x).range(0..=max).clamp_existing_to_range(false)).on_hover_text("Zones across the sheet (0 or 1 = one)").changed();
                ui.label("across  ×");
                edited |= ui.add(egui::DragValue::new(&mut s.zones_y).range(0..=max).clamp_existing_to_range(false)).on_hover_text("Zones up the sheet (0 or 1 = one)").changed();
                ui.label("up");
                changed_tag(ui, (s.zones_x, s.zones_y) != (f.zones_x, f.zones_y), &format!("{} across × {} up", f.zones_x, f.zones_y));
            });
            dim(ui, "The sheet is cut zone by zone in that order, the nearest part next inside a zone; parts sitting in a hole still go before it.");
            ui.add_space(8.0);
            // A part's holes nobody has ordered by hand (CO14, CO15, CO20).
            ui.label(RichText::new("Hole order").size(13.0).strong().color(th().text_strong));
            let (h, f) = (&mut now.holes, &file.holes);
            let mode_text = |m: HoleMode| match m {
                HoleMode::None => "None",
                HoleMode::Proximity => "By proximity",
                HoleMode::Direction => "By direction",
                HoleMode::MinimumHeat => "Minimum heat",
            };
            row(ui, "Order mode", |ui| {
                egui::ComboBox::from_id_salt("hole_order_mode").width(150.0).selected_text(mode_text(h.mode)).show_ui(ui, |ui| {
                    let mut modes = vec![
                        (HoleMode::None, "The part's own contour order, as drawn"),
                        (HoleMode::Proximity, "The nearest hole next, from where the head is"),
                        (HoleMode::Direction, "In rows or columns from a start corner, every other one back"),
                    ];
                    // Offered while in use or set by the machine file (MAC-62), so
                    // choosing another mode can be taken back here.
                    if h.mode == HoleMode::MinimumHeat || f.mode == HoleMode::MinimumHeat {
                        modes.push((HoleMode::MinimumHeat, "Not built yet: the part's own contour order"));
                    }
                    for (md, tip) in modes {
                        edited |= ui.selectable_value(&mut h.mode, md, mode_text(md)).on_hover_text(tip).changed();
                    }
                });
                changed_tag(ui, h.mode != f.mode, mode_text(f.mode));
            });
            let corners = ["Nearest the head", "Lower left", "Lower right", "Upper right", "Upper left"];
            let corner = |c: u8| corners.get(c as usize).copied().unwrap_or(corners[0]);
            ui.add_enabled_ui(h.mode == HoleMode::Direction, |ui| {
                row(ui, "Start corner", |ui| {
                    egui::ComboBox::from_id_salt("hole_start_corner").width(150.0).selected_text(corner(h.corner)).show_ui(ui, |ui| {
                        for c in 0..=4u8 {
                            edited |= ui.selectable_value(&mut h.corner, c, corner(c)).changed();
                        }
                    })
                    .response
                    .on_hover_text("The corner of each part's box By direction starts from");
                    changed_tag(ui, h.corner != f.corner, corner(f.corner));
                });
                row(ui, "Direction", |ui| {
                    edited |= ui.radio_value(&mut h.columns, false, "Rows").changed();
                    edited |= ui.radio_value(&mut h.columns, true, "Columns").changed();
                    changed_tag(ui, h.columns != f.columns, if f.columns { "Columns" } else { "Rows" });
                });
                row(ui, "Same row within", |ui| {
                    edited |= crate::units::drag_len(ui, &mut h.ortho_tol, unit, 100.0).on_hover_text("Holes whose edges are this close are in one row or column (orthogonality tolerance)").changed();
                    changed_tag(ui, (h.ortho_tol - f.ortho_tol).abs() > 1e-6, &unit.show(f.ortho_tol));
                });
            });
            row(ui, "Small holes first", |ui| {
                edited |= ui.checkbox(&mut h.small_first, "cut the small holes next to a big hole before it").changed();
                changed_tag(ui, h.small_first != f.small_first, if f.small_first { "on" } else { "off" });
            });
            ui.add_enabled_ui(h.small_first, |ui| {
                row(ui, "Big from", |ui| {
                    edited |= crate::units::drag_len(ui, &mut h.big_length, unit, 10_000.0).on_hover_text("A hole this long along X is big (0 = every hole)").changed();
                    ui.label("long, or");
                    edited |= crate::units::drag_len(ui, &mut h.big_width, unit, 10_000.0).on_hover_text("A hole this wide along Y is big (0 = every hole)").changed();
                    ui.label("wide");
                    let ch = (h.big_length - f.big_length).abs() > 1e-6 || (h.big_width - f.big_width).abs() > 1e-6;
                    changed_tag(ui, ch, &format!("{} long, or {} wide", unit.show(f.big_length), unit.show(f.big_width)));
                });
                row(ui, "Next to it within", |ui| {
                    edited |= crate::units::drag_len(ui, &mut h.big_offset, unit, 1_000.0).on_hover_text("A small hole this close to a big hole's box is cut before it").changed();
                    changed_tag(ui, (h.big_offset - f.big_offset).abs() > 1e-6, &unit.show(f.big_offset));
                });
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(154.0);
                if ui.add_enabled(!m.order.is_empty(), egui::Button::new("Use the machine file")).on_hover_text("Put back every sheet and hole order value of this machine from its machine file").clicked() {
                    reset = true;
                }
                let note = if m.order.is_empty() { "Every value is the machine file's." } else { "Values marked changed are this machine's own." };
                ui.label(RichText::new(note).size(11.5).color(th().text_dim));
            });
        });
        // A value set back to the file's follows the file again (CO16, CO17).
        if reset {
            self.cfg.machines[k].order = OrderChanges::default();
        } else if edited {
            self.cfg.machines[k].order = OrderChanges::between(&file, &now);
        }
    }

    /// Where this PC's material list lives (MA14), and its settings folder.
    pub(crate) fn material_home(&self) -> (crate::materials::Home, std::path::PathBuf) {
        let pc_dir = crate::settings::AppSettings::file().parent().map(std::path::Path::to_path_buf).unwrap_or_default();
        (crate::materials::Home::of(&self.library_dir(), &pc_dir), pc_dir)
    }

    /// The jobs open in this window, as what uses a material counts them
    /// (MA20, JOB-88).
    fn open_material_jobs(&mut self) -> Vec<crate::materials::Open> {
        self.store_job();
        // MA35: with the name each was read with and whether the list has
        // it, so an unknown one is found again against the list as stored.
        self.jobs.iter().map(|j| crate::materials::Open::new(&j.name, j.spec.material, &crate::engine::material::read_name(j.spec.material, &j.material_names), j.lib_id)).collect()
    }

    /// Settings → Materials: the material list (MA21). The page edits a copy
    /// of the list (`Draft`); Save materials stores every change together
    /// under the Library's write lock, Undo changes throws them away.
    /// The Materials page before it is drawn: its draft (the list as
    /// stored, read again when nothing is typed and another PC or an agent
    /// changed it), and whether the list can be changed now (MA14, MA21),
    /// looked at again when the page is opened, when the Library was looked
    /// at again or changed, and after a refused save. Unsaved changes stay.
    pub fn materials_page_ready(&mut self) {
        let list = crate::engine::material::current();
        match self.setui.mat_draft.as_mut() {
            Some(d) => d.follow(&list),
            None => {
                self.setui.mat_draft = Some(crate::engine::materialedit::Draft::of(&list));
                self.setui.mat_recheck = true;
            }
        }
        if std::mem::take(&mut self.setui.mat_recheck) {
            self.setui.mat_locked = crate::materials::can_change(&self.material_home().0).err();
        }
    }

    /// The window's automatic look at the Library found it gone (`Some`,
    /// why) or back (`None`): the Materials page follows at once, without
    /// looking at the Library again itself; what was typed stays (MA14,
    /// MA21). A PC with no Library keeps its own list and can always edit.
    pub(crate) fn materials_reach(&mut self, unreachable: Option<&str>) {
        if let crate::materials::Home::Library(_) = self.material_home().0 {
            self.setui.mat_locked = unreachable.map(crate::materials::cant_change);
        }
    }

    fn settings_materials(&mut self, ui: &mut egui::Ui) {
        use crate::engine::catalogue::{self, CATALOGUE};
        use crate::engine::materialedit::Field;
        let t = th();
        let (home, _) = self.material_home();
        self.materials_page_ready();
        if self.setui.mat_add == 0 {
            self.setui.mat_add = 100;
        }
        ui.label(RichText::new("Materials").size(18.0).strong().color(t.text_strong));
        ui.label(RichText::new("The shop's materials, each grade its own. Every PC that shares the Library uses this list.").size(11.5).color(t.text_dim));
        ui.add_space(8.0);

        enum Act {
            TryAgain,
            Delete(usize),
            Keep(usize),
            Save,
            Undo,
            Reset,
        }
        let mut act = None;
        // MA14: while the Library can't be reached, the page says why and
        // nothing on it can be changed.
        let can = self.setui.mat_locked.is_none();
        if let Some(why) = &self.setui.mat_locked {
            egui::Frame::new().fill(t.warn_bg).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(why).color(t.warn_fg));
                    if page_button(ui, "Materials", "Try again", true).clicked() {
                        act = Some(Act::TryAgain);
                    }
                });
            });
            ui.add_space(8.0);
        }

        let su = &mut self.setui;
        let d = su.mat_draft.as_mut().unwrap();
        let where_ = match home {
            crate::materials::Home::Library(_) => "The Library's list",
            crate::materials::Home::Pc(_) => "This PC's own list (it has no Library)",
        };
        card(ui, "Material list", &format!("{where_}: {} materials. Pick one to edit it.", d.rows.iter().filter(|r| !r.delete).count()), |ui| {
            ui.add_enabled_ui(can, |ui| {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("mat_add_standard").width(230.0).selected_text(catalogue::label(su.mat_add)).show_ui(ui, |ui| {
                        for e in CATALOGUE.iter() {
                            ui.selectable_value(&mut su.mat_add, e.number, format!("{}  ·  {}", e.label(), e.details()));
                        }
                    });
                    if page_button(ui, "Materials", "Add from standard", true).on_hover_text("A new material based on the standard picked: named as it, its weight, machine table key and codes in programs the standard's until you type its own.").clicked() {
                        if let Ok(k) = d.add_from_standard(su.mat_add) {
                            su.mat_sel = Some(k);
                        }
                    }
                    ui.add_space(8.0);
                    if page_button(ui, "Materials", "Add blank", true).on_hover_text("A new material with no name yet, on standard 0010 until you pick another.").clicked() {
                        su.mat_sel = Some(d.add_blank());
                    }
                });
            });
            ui.add_space(6.0);
            egui::Grid::new("mat_list").num_columns(5).striped(true).spacing([18.0, 6.0]).show(ui, |ui| {
                for h in ["Material", "Kind", "Standard", "Also called", ""] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for (k, r) in d.rows.iter().enumerate() {
                    let mut name = RichText::new(if r.m.name.trim().is_empty() { "(no name yet)".to_string() } else { r.m.name.clone() });
                    if r.delete {
                        name = name.strikethrough().color(t.text_dim);
                    }
                    if ui.selectable_label(su.mat_sel == Some(k), name).clicked() {
                        su.mat_sel = Some(k);
                    }
                    ui.label(catalogue::kind_name(r.m.standard));
                    ui.label(catalogue::label(r.m.standard));
                    let other = r.other_names.trim();
                    let short: String = if other.chars().count() > 34 { format!("{}…", other.chars().take(33).collect::<String>()) } else { other.to_string() };
                    let l = ui.label(RichText::new(&short).color(t.text_dim));
                    if short.ends_with('…') {
                        l.on_hover_text(other);
                    }
                    let state = if r.delete { "to be deleted" } else if r.is_new() { "new" } else if r.changed() { "changed" } else { "" };
                    ui.label(RichText::new(state).size(11.0).color(t.accent_fg));
                    ui.end_row();
                }
            });
        });

        // The material picked, every value editable; a blank one shows its
        // standard's, greyed (MA16).
        if let Some(k) = su.mat_sel.filter(|&k| k < d.rows.len()) {
            ui.add_space(10.0);
            let r = &mut d.rows[k];
            let title = if r.is_new() { format!("{} (new)", r.label()) } else { format!("{} (material {})", r.label(), r.m.id) };
            card(ui, &title, "A blank field takes the standard's value, shown grey. Type a value to give the material its own; clear it to go back to the standard's.", |ui| {
                ui.add_enabled_ui(can && !r.delete, |ui| {
                    row(ui, "Name", |ui| {
                        ui.add(egui::TextEdit::singleline(&mut r.m.name).desired_width(260.0).hint_text("needed"));
                    });
                    row(ui, "Standard", |ui| {
                        egui::ComboBox::from_id_salt(("mat_standard", r.key)).width(260.0).selected_text(catalogue::label(r.m.standard)).show_ui(ui, |ui| {
                            for e in CATALOGUE.iter() {
                                ui.selectable_value(&mut r.m.standard, e.number, format!("{}  ·  {}", e.label(), e.details()));
                            }
                        });
                        ui.label(RichText::new(catalogue::kind_name(r.m.standard)).color(t.text_dim));
                    });
                    let field = |ui: &mut egui::Ui, r: &mut crate::engine::materialedit::Row, label: &str, f: Field, w: f32, unit: &str| {
                        // Blank: the standard's value, grey; an empty value of
                        // its own: what is really used (MA15, MA21).
                        let hint = r.hint(f);
                        let mut typed = r.text(f).to_string();
                        row(ui, label, |ui| {
                            if ui.add(egui::TextEdit::singleline(&mut typed).desired_width(w).hint_text(if hint.is_empty() { "empty".to_string() } else { hint })).changed() {
                                r.set(f, &typed);
                            }
                            if !unit.is_empty() {
                                ui.label(RichText::new(unit).color(t.text_dim));
                            }
                            if r.is_own(f) && page_button(ui, "Materials", "Use standard", true).on_hover_text(format!("Use the standard's value: {}", r.standard_value(f))).clicked() {
                                r.use_standard(f);
                            }
                        });
                    };
                    field(ui, r, "Weight per volume", Field::Density, 80.0, "g/cm³");
                    field(ui, r, "Name in programs", Field::ProgramName, 180.0, "");
                    field(ui, r, "Code in programs", Field::ProgramCode, 80.0, "");
                    field(ui, r, "Machine table key", Field::MachineRef, 120.0, "catalogue number, or the name the machine file uses");
                    row(ui, "Also called", |ui| {
                        ui.add(egui::TextEdit::singleline(&mut r.other_names).desired_width(400.0).hint_text("other names, comma-separated: grades, norms, shop names"));
                    });
                });
                ui.add_space(4.0);
                ui.add_enabled_ui(can, |ui| {
                    if r.delete {
                        if page_button(ui, "Materials", "Keep", true).on_hover_text("Keep this material: it isn't deleted when you save.").clicked() {
                            act = Some(Act::Keep(k));
                        }
                    } else if page_button(ui, "Materials", "Delete", true).on_hover_text("Delete this material when you save. Refused for the first three materials and while a job, library part or stock sheet uses it.").clicked() {
                        act = Some(Act::Delete(k));
                    }
                });
            });
        }

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Try a name:").size(11.5).color(t.text_dim));
            ui.add(egui::TextEdit::singleline(&mut su.mat_probe).desired_width(160.0).hint_text("e.g. S355"));
            if !su.mat_probe.trim().is_empty() {
                let txt = match crate::engine::material::find(&su.mat_probe) {
                    Some(k) => format!("→ {}", crate::engine::material::name_or(k, "?")),
                    None => "→ not a known material".into(),
                };
                ui.label(RichText::new(txt).size(11.5));
            }
        });
        ui.add_space(6.0);
        let changed = d.changed();
        ui.horizontal(|ui| {
            if page_button(ui, "Materials", "Save materials", can && changed).on_hover_text("Keeps every change on this page, in the list every PC that shares the Library uses. Nothing is kept if a name is taken.").clicked() {
                act = Some(Act::Save);
            }
            if page_button(ui, "Materials", "Undo changes", changed).on_hover_text("Throws away the changes not saved yet.").clicked() {
                act = Some(Act::Undo);
            }
            if page_button(ui, "Materials", "Reset to defaults", can).on_hover_text("Puts the first three materials' names and codes in programs, machine table keys and other names back as a new install has them. Save materials keeps it.").clicked() {
                act = Some(Act::Reset);
            }
        });
        if let Some((ok, note)) = &su.mat_note {
            ui.add_space(4.0);
            ui.label(RichText::new(note).size(11.5).color(if *ok { t.ok_fg } else { t.warn_fg }));
        }

        match act {
            Some(Act::TryAgain) => {
                self.reload_materials(false);
                self.setui.mat_locked = crate::materials::can_change(&home).err();
            }
            Some(Act::Delete(k)) => {
                let open = self.open_material_jobs();
                let lib = crate::library::Library::at(&self.library_dir(), &self.who, &self.clock);
                let list = crate::engine::material::current();
                if let Some(d) = self.setui.mat_draft.as_mut() {
                    let r = d.delete(k, |id| crate::engine::materialedit::check_delete(&list, id, |id| crate::materials::uses(&lib, id, &open)));
                    if d.rows.len() <= k {
                        self.setui.mat_sel = None;
                    }
                    self.setui.mat_note = r.err().map(|e| (false, e));
                }
            }
            Some(Act::Keep(k)) => {
                if let Some(d) = self.setui.mat_draft.as_mut() {
                    d.keep(k);
                }
            }
            Some(Act::Save) => self.save_materials(),
            Some(Act::Undo) => {
                self.setui.mat_draft = None;
                self.setui.mat_sel = None;
                self.setui.mat_note = None;
            }
            Some(Act::Reset) => {
                let defaults = crate::engine::material::List::seed(&crate::settings::AppSettings::default());
                if let Some(d) = self.setui.mat_draft.as_mut() {
                    d.reset_first_three(&defaults);
                }
            }
            None => {}
        }
    }

    /// Save materials: every change of the page together, under the write
    /// lock of the list (MA14, MA21); refused (and kept for another try)
    /// when a name is taken, a material is used, or the Library can't be
    /// reached, with nothing stored.
    pub(crate) fn save_materials(&mut self) {
        let Some(d) = self.setui.mat_draft.clone() else { return };
        let (home, pc_dir) = self.material_home();
        let open = self.open_material_jobs();
        let lib = crate::library::Library::at(&self.library_dir(), &self.who, &self.clock);
        let r = d.changes().and_then(|c| crate::materials::apply(&home, &pc_dir, &self.cfg, || lib.saved(), &lib, &c, &open));
        match r {
            Ok((_, l)) => {
                // MA34: the open jobs follow the list as stored.
                self.install_materials(|| {
                    crate::engine::material::install(l);
                    None
                });
                self.setui.mat_draft = None;
                self.setui.mat_sel = None;
                self.setui.mat_note = Some((true, "Materials saved.".into()));
                self.refresh();
            }
            Err(e) => {
                self.setui.mat_note = Some((false, e));
                self.setui.mat_locked = crate::materials::can_change(&home).err();
            }
        }
    }

    pub(crate) fn settings_folders(&mut self, ui: &mut egui::Ui) {
        // Looked for once, as the lists do; Try again looks again (LB11).
        if !self.libui.loaded {
            self.lib_refresh();
        }
        ui.label(RichText::new("Folders").size(18.0).strong().color(th().text_strong));
        ui.add_space(8.0);
        card(ui, "Export folder", "Where Export CNC writes programs (a sub-folder per job). Empty = Documents\\Sheet Nesting.", |ui| {
            row(ui, "Folder", |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.cfg.export_dir).desired_width(380.0).hint_text("Documents\\Sheet Nesting"));
                if ui.button("Browse…").clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        self.cfg.export_dir = d.display().to_string();
                    }
                }
                if !self.cfg.export_dir.is_empty() && ui.small_button("✖").clicked() {
                    self.cfg.export_dir.clear();
                }
            });
        });
        ui.add_space(10.0);
        card(ui, "Library folder", "This app's own product and job database. Put it on a shared drive and everyone in the shop sees the same products and jobs. Empty = Documents\\Sheet Nesting\\Library.", |ui| {
            row(ui, "Folder", |ui| {
                // Typed: it's another folder once the typing is done, never
                // while half a name is in the box (LB12).
                let mut text = self.lib_dir_edit.clone().unwrap_or_else(|| self.cfg.library_dir.clone());
                let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(380.0).hint_text("Documents\\Sheet Nesting\\Library"));
                if r.has_focus() || r.changed() {
                    self.lib_dir_edit = Some(text);
                }
                if r.lost_focus() {
                    if let Some(t) = self.lib_dir_edit.take() {
                        self.pick_library_folder(t);
                    }
                }
                if ui.button("Browse…").clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        self.pick_library_folder(d.display().to_string());
                    }
                }
                if !self.cfg.library_dir.is_empty() && ui.small_button("✖").on_hover_text("Back to Documents\\Sheet Nesting\\Library on this PC").clicked() {
                    self.pick_library_folder(String::new());
                }
            });
            // The Library can't be reached or isn't there (LB11, LB12).
            if let Some(e) = self.libui.error.clone() {
                ui.add_space(4.0);
                ui.label(RichText::new(format!("⚠ {e}")).size(11.5).color(th().warn_fg));
                ui.horizontal(|ui| {
                    if ui.small_button("Try again").clicked() {
                        self.lib_retry();
                    }
                    // Asked first, as for any folder that holds no Library.
                    if ui.small_button("Make a new Library here…").on_hover_text("Only for a new Library: the shop's own may just be out of reach").clicked() {
                        self.lib_new_ask = Some(self.cfg.library_dir.clone());
                    }
                });
            }
        });
        ui.add_space(10.0);
        // JX8, JX9: one setting, off on a new install; ticking it starts the
        // period at 1 minute; whole minutes, 1 to 1440. LB14: while it's on,
        // a quick look every few seconds shows other PCs' saves.
        card(ui, "Auto-refresh", "Show what other PCs save to the Library without pressing Refresh: the Job Explorer, the Library window, Add product, the sheet stock, and open jobs someone else saved. Off on a new install. Each PC sets its own.", |ui| {
            let mut on = self.cfg.refresh_minutes > 0;
            row(ui, "Auto-refresh", |ui| {
                if ui.checkbox(&mut on, "Update the lists and open jobs by themselves").changed() {
                    self.cfg.refresh_minutes = if on { 1 } else { 0 };
                }
            });
            row(ui, "Full reload every", |ui| {
                let mut m = self.cfg.refresh_minutes.max(1);
                let r = ui.add_enabled(on, egui::DragValue::new(&mut m).range(1..=crate::engine::refresh::PERIOD_MAX).speed(1.0).suffix(" min"));
                if on && r.changed() {
                    self.cfg.refresh_minutes = crate::engine::refresh::period(m as i64);
                }
                ui.label(RichText::new("whole minutes, 1 to 1440").size(11.5).color(th().text_dim));
            });
            ui.label(
                RichText::new(format!(
                    "While it's on, the program also looks every {} seconds whether another PC saved something, and at once when this window comes back to the front; the period reads everything again.",
                    crate::engine::refresh::QUICK_S
                ))
                .size(11.5)
                .color(th().text_dim),
            );
        });
        ui.add_space(10.0);
        card(ui, "Lantek Expert install (optional)", "Only used to import its database connection, machines and post files.", |ui| {
            row(ui, "Folder", |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.cfg.lantek_dir).desired_width(380.0).hint_text("not used"));
                if ui.button("Import…").clicked() {
                    self.import_lantek_ui();
                }
            });
        });
    }
}

#[cfg(test)]
mod settings_ui_tests {
    use super::*;

    /// SH16: Join a shop… is offered on a PC whose Library is no shop, and
    /// on one whose Library can't be reached or used, not only on a working
    /// one.
    #[test]
    fn join_a_shop_is_offered_when_this_pcs_library_cant_be_used() {
        assert!(join_offered(&Err("The Library on \\\\srv\\Nesting can't be reached.".into())).contains("asked first whether to join instead"));
        assert!(join_offered(&Ok(None)).contains("left as it is"));
    }

    /// SH14, SH16: a PC in a shop is offered Join a shop… too, which asks
    /// first whether to leave its shop.
    #[test]
    fn join_a_shop_is_offered_on_a_pc_in_a_shop() {
        use crate::shared::{Clock, Who};
        let d = std::env::temp_dir().join(format!("nestpro_join_offered_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        crate::library::create(&d).unwrap();
        crate::engine::shop::start(&d, "Other", &Who::new("kim", "PC-B"), &Clock::at(1_794_216_000), "0.1.0").unwrap();
        let st = crate::engine::shop::status(&d);
        assert!(matches!(st, Ok(Some(_))), "no shop");
        assert!(join_offered(&st).contains("asked first whether to leave Other"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Wait for the made-up download to answer.
    fn wait_fetch(u: &mut UpdateState) {
        for _ in 0..500 {
            if !u.poll_fetch() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the download never answered");
    }

    fn rel(v: &str) -> crate::engine::updates::Release {
        crate::engine::updates::Release { version: v.into(), notes: String::new() }
    }

    /// A PC of its own on a test Library, with 0.2.0 chosen and downloaded
    /// (a made-up list: nothing is fetched).
    fn chosen_and_ready(d: &std::path::Path) -> (UpdateState, &'static crate::updates::Handoff, std::path::PathBuf) {
        let lib = d.join("Library");
        crate::library::create(&lib).unwrap();
        let h: &'static crate::updates::Handoff = Box::leak(Box::new(crate::updates::Handoff::new()));
        let mut u = UpdateState { source: Some(crate::updates::Source::Fake(d.join("releases.json"))), ..Default::default() };
        u.follow_library(&lib, h);
        u.receive(crate::updates::Found::Releases(vec![rel("9.2.0")]), h);
        u.choose(d.join("own"), h);
        wait_fetch(&mut u);
        assert!(matches!(&u.fetch, Fetch::Ready(v) if v == "9.2.0"));
        (u, h, lib)
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("nestpro_upd_state_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// S2: pointing the Library elsewhere withdraws the release chosen for
    /// the old one: nothing installs on close, and it is checked anew.
    #[test]
    fn another_library_withdraws_the_chosen_update() {
        let d = tmp("library");
        let (mut u, h, lib) = chosen_and_ready(&d);
        u.follow_library(&lib, h);
        assert!(matches!(u.fetch, Fetch::Ready(_)), "the same Library changed nothing");
        u.follow_library(&d.join("Other"), h);
        assert!(matches!(u.fetch, Fetch::No) && u.offer.is_none() && u.chosen.is_none() && u.checked_at.is_none());
        assert_eq!(h.take_ready(std::time::Duration::ZERO), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S2: a check that finds the Library is a shop's now withdraws it too.
    #[test]
    fn a_library_that_became_a_shops_withdraws_the_chosen_update() {
        let d = tmp("shop");
        let (mut u, h, _) = chosen_and_ready(&d);
        u.receive(crate::updates::Found::InShop("Acme".into()), h);
        assert!(matches!(u.fetch, Fetch::No) && u.offer.is_none());
        assert_eq!(h.take_ready(std::time::Duration::ZERO), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S6: a newer release found while one is ready: "Newest release" is
    /// the newer one, and the chosen one keeps its own line.
    #[test]
    fn the_newest_release_follows_the_latest_check() {
        let d = tmp("newest");
        let (mut u, h, _) = chosen_and_ready(&d);
        u.receive(crate::updates::Found::Releases(vec![rel("9.3.0"), rel("9.2.0")]), h);
        assert_eq!(u.newest(), Some("9.3.0"));
        assert_eq!(u.offer.as_ref().map(|o| o.version.as_str()), Some("9.3.0"));
        assert!(matches!(&u.fetch, Fetch::Ready(v) if v == "9.2.0"));
        assert_eq!(u.chosen.as_ref().map(|o| o.version.as_str()), Some("9.2.0"));
        // A check that couldn't answer withdraws nothing chosen.
        u.receive(crate::updates::Found::NotChecked("Couldn't check for updates".into()), h);
        assert!(matches!(&u.fetch, Fetch::Ready(v) if v == "9.2.0"));
        // N1: the banner still shows the chosen release, so its What's new
        // has that release's notes even with nothing offered now.
        assert!(u.offer.is_none());
        assert_eq!(u.banner_release().map(|o| o.version), Some("9.2.0".to_string()));
        // The dialog opened from that banner shows those notes.
        let mut app = crate::App::new(None);
        app.setui.upd = u;
        app.setui.upd.whats_new = app.setui.upd.banner_release();
        let ctx = egui::Context::default();
        let mut text = String::new();
        for _ in 0..3 {
            let mut out = ctx.run_pass(egui::RawInput::default(), |c| app.whats_new_window(c));
            out.textures_delta.clear();
            text = crate::ui_parts::shown_text(&out);
        }
        assert!(text.contains("What's new in 9.2.0"), "{text}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn connection_string_masks_the_password() {
        let mut c = Connection { server: "srv".into(), database: "DB".into(), windows: false, user: "u".into(), password_enc: "0100abc".into(), ..Default::default() };
        assert_eq!(conn_string(&c), "DRIVER=SQL SERVER;SERVER=srv;DATABASE=DB;User ID=u;Password=••••••");
        c.windows = true;
        assert_eq!(conn_string(&c), "Provider=sqloledb;Data Source=srv;Initial Catalog=DB;Persist Security Info=False;Integrated Security=SSPI");
    }

    #[test]
    fn the_materials_page_looks_again_whether_it_can_edit_and_keeps_what_was_typed() {
        // MA14, MA21: the page was opened with the Library in reach, then the
        // Library went away. Opening the page again (or the window looking
        // at the Library again) says why it can't edit; the edit typed
        // before stays, and is saved once the Library is back.
        use crate::libui::libui_tests::{new_library, window};
        use crate::shared::Clock;
        let dir = new_library("materials_page_reach");
        let mut a = window(&dir, "demo", "DEMO-PC", &Clock::pc());
        a.reload_materials(true);
        a.open_settings(Section::Materials);
        a.materials_page_ready();
        assert_eq!(a.setui.mat_locked, None);
        a.setui.mat_draft.as_mut().unwrap().rows[1].m.name = "DEMO Inox".into();
        let away = dir.with_extension("away");
        let _ = std::fs::remove_dir_all(&away);
        std::fs::rename(&dir, &away).unwrap();
        a.materials_page_ready();
        assert_eq!(a.setui.mat_locked, None, "not looked at again every frame");
        // Settings opened again: it looks again.
        a.setui.open = false;
        a.open_settings(Section::Materials);
        a.materials_page_ready();
        let why = a.setui.mat_locked.clone().expect("the page says why it can't edit");
        assert!(why.contains("can't be reached"), "{why}");
        assert_eq!(a.setui.mat_draft.as_ref().unwrap().rows[1].m.name, "DEMO Inox", "what was typed stays");
        // Back again, found when the window looks at the Library again.
        std::fs::rename(&away, &dir).unwrap();
        a.lib_refresh();
        a.materials_page_ready();
        assert_eq!(a.setui.mat_locked, None);
        a.save_materials();
        assert_eq!(crate::engine::material::name(1).as_deref(), Some("DEMO Inox"));
        let stored = crate::materials::read(&crate::materials::Home::Library(dir.clone()).file()).unwrap().unwrap();
        assert_eq!(stored.get(1).unwrap().name, "DEMO Inox");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BK7, review S9: Save backup settings waits for the Library's write
    /// lock in the background (a backup can hold it a while): the window
    /// carries on, says it's saving, and takes the result when it's done.
    #[test]
    fn saving_backup_settings_never_holds_the_window_up() {
        use crate::libui::libui_tests::window;
        use crate::shared::{Clock, Who, WriteLock};
        let root = std::env::temp_dir().join(format!("nestpro_backup_save_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let lib = root.join("Library");
        crate::library::create(&lib).unwrap();
        crate::engine::shop::start(&lib, "Acme", &Who::new("sam", "PC-A"), &Clock::pc(), "0.1.0").unwrap();
        let mut a = window(&lib, "sam", "PC-A", &Clock::pc());
        // Another writer (a backup) holds the lock.
        let lock = WriteLock::take(&lib).unwrap();
        let t = std::time::Instant::now();
        a.save_backup_settings(&lib, &crate::backup::Settings { keep_days: 14, second_place: String::new() });
        assert!(t.elapsed() < std::time::Duration::from_secs(1), "the window waited for the lock");
        assert!(a.setui.backup_run.lock().unwrap().saving, "the card doesn't say it's saving");
        drop(lock);
        let t = std::time::Instant::now();
        while a.setui.backup_run.lock().unwrap().saved.is_none() {
            assert!(t.elapsed() < std::time::Duration::from_secs(20), "never saved");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        a.backup_settings_saved();
        assert_eq!(crate::shop::read(&lib).unwrap().unwrap().backups.keep_days, 14);
        assert!(matches!(&a.setui.shop_note, Some((true, _))), "{:?}", a.setui.shop_note);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// BK7, review round 2 (N2): on a PC of its own, backup settings saved
    /// and then Settings closed are still stored: the window takes the
    /// answer every frame, whatever page is open.
    #[test]
    fn backup_settings_saved_with_settings_closed_are_kept() {
        use crate::libui::libui_tests::window;
        use crate::shared::Clock;
        let root = std::env::temp_dir().join(format!("nestpro_backup_save_closed_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let lib = root.join("Library");
        crate::library::create(&lib).unwrap();
        let mut a = window(&lib, "sam", "PC-A", &Clock::pc());
        a.save_backup_settings(&lib, &crate::backup::Settings { keep_days: 9, second_place: String::new() });
        let t = std::time::Instant::now();
        while a.setui.backup_run.lock().unwrap().saved.is_none() {
            assert!(t.elapsed() < std::time::Duration::from_secs(20), "never saved");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        a.setui.open = false;
        let ctx = egui::Context::default();
        let mut out = ctx.run_pass(egui::RawInput::default(), |c| a.settings_window(c));
        out.textures_delta.clear();
        assert_eq!(a.cfg.backups.keep_days, 9, "the save was lost with Settings closed");
        assert_eq!(a.cfg_saved.backups.keep_days, 9, "not stored in this PC's settings");
        let _ = std::fs::remove_dir_all(&root);
    }
}

// ---------------------------------------------------------------------------
// Settings → Shop (spec #178, SH5–SH8): this shop and its PCs. Short lines,
// details on hover. The rules are in engine::shop; agents have `shop` and
// `setup`.
// ---------------------------------------------------------------------------

impl App {
    fn settings_shop(&mut self, ui: &mut egui::Ui) {
        let dir = self.library_dir();
        // A backup that ended: read the shop and its backups again (BK7).
        let ended = self.setui.backup_run.lock().map_or(0, |r| r.ended);
        if ended != self.setui.backup_seen {
            self.setui.backup_seen = ended;
            self.setui.shop = None;
        }
        if self.setui.shop.is_none() {
            self.setui.backups = Some(crate::engine::shop::backups(&dir, &self.cfg.backups));
            // A Library that is no shop must be one to start a shop in
            // (LB12): read once with the shop, not at every frame.
            self.setui.shop = Some(crate::engine::shop::status(&dir).and_then(|s| match s {
                None => crate::library::usable(&dir).map(|()| None),
                s => Ok(s),
            }));
        }
        ui.label(RichText::new("Shop").size(18.0).strong().color(th().text_strong));
        ui.add_space(8.0);
        let t = th();
        match self.setui.shop.clone().unwrap_or(Ok(None)) {
            // SH8 (LB11): said, naming the folder; nothing is offered that
            // would make a shop or a Library in its place.
            Err(e) => {
                let join_hover = join_offered(&Err(e.clone()));
                card(ui, "This shop", "This PC's Library can't be used just now.", |ui| {
                    ui.label(RichText::new(format!("⚠ {e}")).size(12.0).color(t.warn_fg));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if page_button(ui, "Shop", "Try again", true).clicked() {
                            self.setui.shop = None;
                        }
                        // SH16: a PC whose Library is gone (or never was)
                        // can still join a shop, after the checks; nothing is
                        // made in place of the folder it can't reach.
                        if page_button(ui, "Shop", "Join a shop…", true).on_hover_text(join_hover).clicked() {
                            self.firstrun = Some(crate::startup::FirstRun::join_from_settings());
                            self.setui.shop_note = None;
                        }
                    });
                });
            }
            Ok(None) => {
                self.shop_not_started(ui, &dir);
                ui.add_space(10.0);
                self.backups_card(ui, &dir, None);
            }
            Ok(Some(st)) => {
                self.shop_cards(ui, &dir, &st);
                ui.add_space(10.0);
                self.backups_card(ui, &dir, Some(&st.shop));
            }
        }
        ui.add_space(10.0);
        // UP15: a PC in a shop updates from the shop folder: its own card.
        match self.setui.shop.clone() {
            Some(Ok(Some(st))) => self.shop_updates_card(ui, &st),
            _ => self.updates_card(ui),
        }
        if let Some((ok, note)) = &self.setui.shop_note {
            ui.add_space(6.0);
            ui.label(RichText::new(note).size(11.5).color(if *ok { t.ok_fg } else { t.warn_fg }));
        }
    }

    /// SH1: a Library that is no shop yet, with Start a shop here.
    fn shop_not_started(&mut self, ui: &mut egui::Ui, dir: &std::path::Path) {
        card(ui, "This shop", "This PC's Library isn't in a shop.", |ui| {
            row(ui, "Library folder", |ui| {
                ui.label(RichText::new(dir.display().to_string()).monospace().size(11.5));
            });
            let start_hover = "The Library's folder becomes the shop folder where it is: the shop file and the Shop, Updates and Backups folders are added in it. Nothing moves, and every PC keeps its path. This PC becomes the admin PC.";
            match self.setui.shop_start.clone() {
                None => {
                    ui.horizontal(|ui| {
                        if page_button(ui, "Shop", "Start a shop here…", true).on_hover_text(start_hover).clicked() {
                            self.setui.shop_start = Some(String::new());
                            self.setui.shop_note = None;
                        }
                        // SH16: join a shop started on another PC; its
                        // folder becomes this PC's Library, after the checks.
                        if page_button(ui, "Shop", "Join a shop…", true).on_hover_text(join_offered(&Ok(None))).clicked() {
                            self.firstrun = Some(crate::startup::FirstRun::join_from_settings());
                            self.setui.shop_note = None;
                        }
                    });
                }
                Some(mut name) => {
                    row(ui, "Shop name", |ui| {
                        ui.add(egui::TextEdit::singleline(&mut name).desired_width(220.0).hint_text("Acme"));
                        let go = page_button(ui, "Shop", "Start the shop", !name.trim().is_empty()).on_hover_text(start_hover).clicked();
                        let cancel = ui.button("Cancel").clicked();
                        if go {
                            self.start_shop_here(name.trim());
                        } else if cancel {
                            self.setui.shop_start = None;
                        } else {
                            self.setui.shop_start = Some(name.clone());
                        }
                    });
                }
            }
        });
    }

    /// SH1: start a shop in this PC's Library folder, this PC its admin PC.
    /// When this PC can't be added to the PCs list, the shop is started and
    /// the PCs card says why, with Add this PC (SH4).
    pub(crate) fn start_shop_here(&mut self, name: &str) {
        let dir = self.library_dir();
        self.setui.shop_note = Some(match crate::engine::shop::start_shop(&dir, name, &self.who, &self.clock, crate::updates::VERSION) {
            Ok(st) => {
                self.setui.shop_start = None;
                self.setui.shop_unrecorded = st.unrecorded;
                // S2: a PC in a shop never takes a public release: whatever
                // was offered or chosen from them is withdrawn.
                self.setui.upd.withdraw(self.handoff);
                (true, format!("Started shop {} in {}. This PC is its admin PC.", st.shop.name, dir.display()))
            }
            Err(e) => (false, e),
        });
        self.setui.shop = None;
    }

    /// SH5: This shop, and PCs in this shop.
    fn shop_cards(&mut self, ui: &mut egui::Ui, dir: &std::path::Path, st: &crate::engine::shop::Status) {
        let t = th();
        let s = &st.shop;
        let this_admin = s.admin.pc == self.who.pc;
        card(ui, "This shop", "The PCs that share this Library.", |ui| {
            row(ui, "Name", |ui| {
                ui.label(RichText::new(&s.name).strong());
            });
            row(ui, "Folder", |ui| {
                ui.label(RichText::new(dir.display().to_string()).monospace().size(11.5))
                    .on_hover_text("The shop folder is the Library's folder. It also holds the shop file and the Shop, Updates and Backups folders.");
            });
            row(ui, "Admin PC", |ui| {
                let me = if this_admin { " (this PC)" } else { "" };
                ui.label(format!("{}{me}", s.admin.pc)).on_hover_text(format!("{}. The admin PC rolls out updates and restores backups.", st.admin_note(&self.clock)));
                if !this_admin && page_button(ui, "Shop", "Make this PC the admin PC", true).on_hover_text("For when the admin PC is gone or replaced. Asks first, and records who did it and when.").clicked() {
                    self.setui.shop_admin_ask = Some((dir.to_path_buf(), s.admin.pc.clone()));
                }
            });
            row(ui, "Shop settings", |ui| {
                ui.label(cap(s.settings_by.words())).on_hover_text("Who may change the settings every PC in the shop shares: only on the admin PC, or from any PC. A new shop: only the admin PC.");
            });
            // SH14, SH16: joining another shop from here asks first whether
            // to leave this one.
            if page_button(ui, "Shop", "Join a shop…", true).on_hover_text(join_offered(&Ok(Some(st.clone())))).clicked() {
                self.firstrun = Some(crate::startup::FirstRun::join_from_settings());
                self.setui.shop_note = None;
            }
        });
        ui.add_space(10.0);
        card(ui, "PCs in this shop", "Each PC adds itself when the program opens.", |ui| {
            egui::Grid::new("shop_pcs").num_columns(4).spacing([28.0, 6.0]).show(ui, |ui| {
                for h in ["PC", "Who", "Version", "Last seen"] {
                    ui.label(RichText::new(h).size(11.5).strong().color(t.text_dim));
                }
                ui.end_row();
                for p in &st.pcs {
                    let mut tags = vec![];
                    if p.pc == self.who.pc {
                        tags.push("this PC");
                    }
                    if p.pc == s.admin.pc {
                        tags.push("admin");
                    }
                    let tag = if tags.is_empty() { String::new() } else { format!("  ({})", tags.join(", ")) };
                    let hover = format!("{} opened the program on {} {} with version {}.", p.user, p.pc, self.clock.when(p.last_seen), p.version);
                    ui.label(RichText::new(format!("{}{tag}", p.pc)).color(t.text_strong)).on_hover_text(&hover);
                    ui.label(&p.user).on_hover_text(&hover);
                    // UP22: the PCs behind the shop version are marked.
                    match crate::engine::shop::against(&p.version, s.shop_version()) {
                        (true, _) => ui.label(RichText::new(format!("{}  ⬇ behind", p.version)).color(t.warn_fg)).on_hover_text(format!("{hover} The shop is on {}: it installs that from the shop folder when its program closes.", s.shop_version().unwrap_or_default())),
                        (_, true) => ui.label(RichText::new(format!("{}  ahead", p.version)).color(t.warn_fg)).on_hover_text(format!("{hover} Newer than the shop version {}: the shop is read-only on it until it goes back, when its program closes.", s.shop_version().unwrap_or_default())),
                        _ => ui.label(&p.version).on_hover_text(&hover),
                    };
                    ui.label(self.clock.shown(p.last_seen)).on_hover_text(&hover);
                    ui.end_row();
                }
            });
            if st.pcs.is_empty() {
                ui.label(RichText::new("No PC has opened the program in this shop yet.").size(11.5).color(t.text_dim));
            }
            // SH4: this PC couldn't add itself (the share was busy or
            // read-only when the program opened, or its row couldn't be
            // brought up to date): one line saying so, and Add this PC.
            let adding = self.setui.shop_adding.is_some();
            let listed = st.pcs.iter().any(|p| p.pc == self.who.pc);
            if adding || !listed || self.setui.shop_unrecorded.is_some() {
                let _ = dir;
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let (line, hover) = match &self.setui.shop_unrecorded {
                        _ if adding => (format!("Adding {}…", self.who.pc), String::new()),
                        Some(e) => (format!("{} couldn't be added to the PCs list just now.", self.who.pc), e.clone()),
                        None => (format!("{} isn't in the list yet.", self.who.pc), String::new()),
                    };
                    let l = ui.label(RichText::new(line).size(11.5).color(if adding { t.text_dim } else { t.warn_fg }));
                    if !hover.is_empty() {
                        l.on_hover_text(hover);
                    }
                    if page_button(ui, "Shop", "Add this PC", !adding).on_hover_text("Adds this PC, its user and version to the shop's PCs list. Each PC adds itself when the program opens; this tries again.").clicked() {
                        self.add_this_pc(true);
                    }
                });
            }
        });
    }

    /// SH7: the warning before this PC becomes the admin PC. Yes records who
    /// did it and when; agents can't do this.
    pub fn shop_admin_window(&mut self, ctx: &egui::Context) {
        let Some((dir, replacing)) = self.setui.shop_admin_ask.clone() else { return };
        let Some(Ok(Some(st))) = self.setui.shop.clone() else {
            // Not read yet: asked once the page has read the shop. No shop,
            // or it can't be reached: nothing to ask.
            if self.setui.shop.is_some() {
                self.setui.shop_admin_ask = None;
            }
            return;
        };
        let me = self.who.pc.clone();
        let (mut yes, mut no) = (false, false);
        let mut open = true;
        egui::Window::new(RichText::new("Make this PC the admin PC?").size(16.0).strong())
            .id(egui::Id::unique("shop_admin_question"))
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_max_width(480.0);
                ui.label(RichText::new(format!("{me} becomes the admin PC of {} in place of {replacing}.", st.shop.name)).color(th().text_strong));
                ui.add_space(6.0);
                ui.label(RichText::new("The admin PC rolls out updates and restores backups. Do this only when the admin PC is gone or replaced.").color(th().text));
                ui.add_space(4.0);
                ui.label(RichText::new(format!("The shop records that {} did it, and when.", self.who.label())).size(11.5).color(th().text_dim));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    yes = ui.add(egui::Button::new(RichText::new("Make this PC the admin PC").color(egui::Color32::WHITE)).fill(th().accent).min_size(egui::Vec2::new(0.0, 30.0))).clicked();
                    no = ui.add(egui::Button::new(format!("Cancel — keep {replacing}")).min_size(egui::Vec2::new(0.0, 30.0))).clicked();
                });
            });
        if yes {
            self.setui.shop_admin_ask = None;
            // SH7: about the shop folder and admin PC the question named,
            // checked again under the lock; if either changed, nothing is
            // changed and the page shows the shop as it is now.
            let r = if self.library_dir() != dir {
                Err(format!("This PC's Library folder changed while you were asking (now {}). Nothing was changed. Look again and ask again if this PC should still be the admin PC.", self.library_dir().display()))
            } else {
                crate::engine::shop::replace_admin(&dir, &self.who, &self.clock, &replacing)
            };
            self.setui.shop_note = Some(match r {
                Ok(s) => (true, format!("{} is now the admin PC of {}.", s.admin.pc, s.name)),
                Err(e) => (false, e),
            });
            self.setui.shop = None;
        } else if no || !open {
            self.setui.shop_admin_ask = None;
        }
    }

    /// SH4: this PC records itself in the shop when the program opens, in
    /// the background (a share can be slow to answer); a failure is kept and
    /// shown on Settings → Shop.
    pub fn record_in_shop(&mut self) {
        self.add_this_pc(false);
    }

    /// SH4: add this PC to its shop's PCs list in the background: the write
    /// lock can be busy for up to 2 minutes, and the window never waits for
    /// it. `asked`: a person pressed Add this PC (its result is said).
    pub fn add_this_pc(&mut self, asked: bool) {
        if self.setui.shop_adding.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let (dir, who, clock) = (self.library_dir(), self.who.clone(), self.clock.clone());
        std::thread::spawn(move || {
            let _ = tx.send(crate::engine::shop::record_pc(&dir, &who, &clock, crate::updates::VERSION));
        });
        self.setui.shop_adding = Some((rx, asked));
    }

    /// Takes in the answer of adding this PC once it has come. True while
    /// it is still on its way.
    pub fn poll_add_this_pc(&mut self) -> bool {
        let Some((rx, asked)) = &self.setui.shop_adding else { return false };
        let asked = *asked;
        let r = match rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err(format!("{} couldn't be added to the shop's PCs list: the try stopped. Try again.", self.who.pc)),
        };
        self.setui.shop_adding = None;
        let (unrecorded, note) = added_note(&self.who.pc, r, asked);
        self.setui.shop_unrecorded = unrecorded;
        if note.is_some() {
            self.setui.shop_note = note;
        }
        // The page shows the list as it is now.
        self.setui.shop = None;
        false
    }

    /// `--shop-demo` (screenshot runs only): a made-up shop Acme with three
    /// PCs in a temporary Library of its own (`shop_demo_dir`), this window
    /// on PC-B (not the admin PC). `--shop-demo=none`: a Library that isn't a
    /// shop yet, with Start a shop here pressed; `--shop-demo=unrecorded`:
    /// PC-B isn't in the PCs list. Never the configured Library or one named
    /// with `--library-dir`, and nothing at all outside a screenshot run, so
    /// no real Library or shop ever gets made-up PCs.
    pub fn demo_shop(&mut self, what: &str) {
        use crate::shared::{Clock, Who};
        // Only the folder allocated for this run, fresh and empty; nothing
        // is ever deleted to make room for it.
        let Some(dir) = shop_demo_dir() else { return };
        if !self.shot_mode_now() || self.library_dir() != dir {
            return;
        }
        if crate::library::create(&dir).is_err() {
            return;
        }
        self.who = Who::new("kim", "PC-B");
        if what == "none" {
            self.setui.shop_start = Some("Acme".into());
            return;
        }
        let now = self.clock.now();
        let at = |ago: u64| Clock::at(now - ago);
        let _ = crate::engine::shop::start(&dir, "Acme", &Who::new("sam", "PC-A"), &at(40 * 86400), "0.1.0");
        let _ = crate::engine::shop::record_pc(&dir, &Who::new("sam", "PC-A"), &at(25 * 60), "0.1.0");
        if what != "unrecorded" {
            let _ = crate::engine::shop::record_pc(&dir, &Who::new("kim", "PC-B"), &at(0), crate::updates::VERSION);
        } else {
            self.setui.shop_unrecorded = Some("PC-B couldn't be added to the shop's PCs list (Permission denied). It adds itself again when the program next opens, or with Add this PC on Settings → Shop.".into());
        }
        let _ = crate::engine::shop::record_pc(&dir, &Who::new("lee", "PC-C"), &at(2 * 86400 + 3 * 3600), "0.0.9");
        // Backups (BK7): the day's backups of the last days, the newest
        // taken on this PC, and a second place that can't be reached.
        let own = crate::backup::Settings::default();
        for (who, ago) in [("PC-A", 3 * 86400), ("PC-C", 2 * 86400 + 3 * 3600), ("PC-A", 86400 + 1800)] {
            let _ = crate::engine::shop::daily_backup(&dir, &own, &Who::new("sam", who), &at(ago));
        }
        if let Ok(Some(mut s)) = crate::shop::read(&dir)
            && let Ok(lock) = crate::shared::WriteLock::take(&dir)
        {
            s.backups.second_place = r"\\nas\Backups".into();
            let _ = crate::shop::write(&dir, &s, &lock, crate::updates::VERSION);
        }
        if let Ok(d) = crate::engine::shop::backup_now(&dir, &own, &self.who, &at(25 * 60))
            && let Ok(mut r) = self.setui.backup_run.lock()
        {
            r.note = Some(backup_note(&d));
        }
        self.setui.shop = None;
        if std::env::args().any(|a| a == "--settings=shop-admin") {
            self.setui.shop_admin_ask = Some((dir.clone(), "PC-A".into()));
        }
        // `--shop-demo=restore` (BK8): this window on the admin PC, PC-A,
        // with the Restore list open on the backup of two days ago;
        // `--shop-demo=restore-ask`: asking once more.
        if what == "restore" || what == "restore-ask" {
            self.who = Who::new("sam", "PC-A");
            let all = crate::backup::overview(&dir, &own).map(|o| o.all).unwrap_or_default();
            let pick = all.iter().find(|e| e.pc == "PC-C").or(all.first()).map(|e| e.name.clone());
            self.setui.restore = Some(RestoreAsk { pick, sure: what == "restore-ask" });
        }
        // `--shop-demo=restore-stopped` (BK13): the admin PC's restore of the
        // backup of two days ago stopped halfway 20 minutes ago; the Backups
        // card says so from the shop file.
        if what == "restore-stopped" {
            self.who = Who::new("sam", "PC-A");
            let all = crate::backup::overview(&dir, &own).map(|o| o.all).unwrap_or_default();
            if let (Some(e), Ok(Some(mut s)), Ok(lock)) = (all.iter().find(|e| e.pc == "PC-C"), crate::shop::read(&dir), crate::shared::WriteLock::take(&dir)) {
                let id8 = e.name.split(' ').nth(2).unwrap_or_default();
                let before = format!("Backup Acme {id8} {} PC-A before restore.zip", self.clock.stamp(now - 20 * 60).replace(':', ""));
                s.restored = Some(crate::shop::Restore { backup: e.name.clone(), backup_when: e.stamp(), before, by: Who::new("sam", "PC-A"), at: now - 20 * 60, finished: false, revision_floor: 0, generation: 1, lock: String::new() });
                let _ = crate::shop::write(&dir, &s, &lock, crate::updates::VERSION);
            }
            self.setui.shop = None;
        }
    }

    // -----------------------------------------------------------------------
    // Backups (BK1–BK7): the rules are in crate::backup and engine::shop.
    // -----------------------------------------------------------------------

    /// BK7: the Backups card: the last backup (when, which PC), where they
    /// go, Keep, the second place and Back up now. `shop`: the shop file
    /// when the Library is a shop.
    fn backups_card(&mut self, ui: &mut egui::Ui, dir: &std::path::Path, shop: Option<&crate::shop::ShopFile>) {
        let t = th();
        let (since, note, saving, restoring) = self.setui.backup_run.lock().map(|r| (r.since, r.note.clone(), r.saving, r.restoring)).unwrap_or_default();
        // Who may change Keep and the second place (SH3).
        let may = match shop {
            Some(s) if s.settings_by == crate::shop::SettingsBy::AdminPc && s.admin.pc != self.who.pc => Err(format!("Shop settings are changed only on the admin PC ({}).", s.admin.pc)),
            _ => Ok(()),
        };
        let sub = if shop.is_some() { "One zip of the shop a day, by the first PC to open the program." } else { "One zip of this PC's Library a day, kept beside it." };
        card(ui, "Backups", sub, |ui| {
            let ov = match self.setui.backups.clone() {
                Some(Ok(o)) => o,
                Some(Err(e)) => {
                    ui.label(RichText::new(format!("⚠ {e}")).size(12.0).color(t.warn_fg));
                    return;
                }
                None => return,
            };
            row(ui, "Last backup", |ui| {
                if let Some(at) = since {
                    ui.spinner();
                    ui.label(RichText::new(format!("Backing up… (since {})", self.clock.shown(at)))).on_hover_text("The program takes it in the background: carry on working. Saves from other PCs wait until it's done.");
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(400));
                } else if let Some(e) = &ov.last {
                    let hover = format!("{} — {:.1} MB. {} backup{} kept.", e.name, e.bytes as f64 / 1e6, ov.count, if ov.count == 1 { "" } else { "s" });
                    ui.label(RichText::new(format!("{} on {}", e.shown(), e.pc)).color(t.text_strong)).on_hover_text(hover);
                } else {
                    ui.label(RichText::new("None yet").color(t.text_dim));
                }
            });
            // BK12, BK13: the shop's last restore, as the shop file says,
            // whatever this window saw: running, stopped halfway (with the
            // backup that holds the Library as it was), or done.
            if let Some(r) = shop.and_then(|s| s.restored.as_ref()) {
                let state = crate::engine::shop::restore_state(dir, r);
                let (warn, line) = restore_line(r, state, &self.clock);
                row(ui, "Last restore", |ui| {
                    ui.label(RichText::new(line).size(11.5).color(if warn { t.warn_fg } else { t.text })).on_hover_text(format!("{} — restored from {} by {}. The Library as it was before is in {}.", backup_day(r), r.backup, r.by.label(), r.before));
                });
            }
            row(ui, "Folder", |ui| match &ov.folder {
                Ok(f) => {
                    ui.label(RichText::new(f.display().to_string()).monospace().size(11.5)).on_hover_text("Each backup is one zip, named by its day and the PC that took it. Other files kept here are never touched.");
                }
                Err(e) => {
                    ui.label(RichText::new(format!("⚠ {e}")).size(11.5).color(t.warn_fg));
                }
            });
            let mut edit = self.setui.backup_edit.clone().unwrap_or_else(|| ov.settings.clone());
            let lock_hover = may.as_ref().err().cloned();
            row(ui, "Keep", |ui| {
                ui.add_enabled_ui(may.is_ok(), |ui| {
                    let r = ui.add(egui::DragValue::new(&mut edit.keep_days).range(1..=crate::backup::MOST_KEEP).suffix(" days"));
                    r.on_hover_text(lock_hover.clone().unwrap_or_else(|| "Older backups are removed after each new one. The newest is always kept.".into()));
                });
            });
            row(ui, "Second place", |ui| {
                ui.add_enabled_ui(may.is_ok(), |ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut edit.second_place).desired_width(260.0).hint_text("None (e.g. \\\\nas\\Backups)"));
                    r.on_hover_text(lock_hover.clone().unwrap_or_else(|| "Another drive or a NAS: each backup is copied there too, so a dead server doesn't take the backups with it. If it can't be reached, it's said here.".into()));
                });
            });
            let changed = edit != ov.settings;
            self.setui.backup_edit = changed.then_some(edit.clone());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if page_button(ui, "Shop", "Back up now", since.is_none() && restoring.is_none()).on_hover_text("Take a backup now, whatever was taken today: before a big change, say.").clicked() {
                    self.start_backup(crate::backup::Why::Now);
                }
                // BK8: offered only on the admin PC, whatever the shop says
                // about who changes shop settings.
                if shop.is_some_and(|s| s.admin.pc == self.who.pc)
                    && page_button(ui, "Shop", "Restore…", since.is_none() && restoring.is_none() && ov.count > 0)
                        .on_hover_text("Put the shop back as it was in an earlier backup. The Library as it is now is kept first, as a backup named \"before restore\".")
                        .clicked()
                {
                    self.setui.restore = Some(RestoreAsk { pick: ov.all.first().map(|e| e.name.clone()), sure: false });
                }
                if let Some(at) = restoring {
                    ui.spinner();
                    ui.label(RichText::new(format!("Restoring… (since {})", self.clock.shown(at)))).on_hover_text("The Library is being put back from the backup. Other PCs wait until it's done.");
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(400));
                }
                if changed && page_button(ui, "Shop", "Save backup settings", may.is_ok() && !saving).clicked() {
                    self.save_backup_settings(dir, &edit);
                }
                if saving {
                    ui.spinner();
                    ui.label(RichText::new("Saving…").color(t.text_dim)).on_hover_text("Waiting for the Library: another PC, or a backup, is writing to it.");
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(400));
                }
            });
            if let Some((ok, n)) = note {
                ui.add_space(4.0);
                ui.label(RichText::new(n).size(11.5).color(if ok { t.ok_fg } else { t.warn_fg }));
            }
        });
    }

    /// BK4, BK5: save Keep and the second place: in the shop file, or this
    /// PC's settings for a PC of its own.
    /// In the background: a shop's settings wait for the write lock, which
    /// a backup can hold for a while; the card says it's saving meanwhile.
    pub(crate) fn save_backup_settings(&mut self, dir: &std::path::Path, edit: &crate::backup::Settings) {
        let run = self.setui.backup_run.clone();
        {
            let Ok(mut r) = run.lock() else { return };
            if r.saving {
                return;
            }
            r.saving = true;
        }
        let (dir, who, edit, mut own) = (dir.to_path_buf(), self.who.clone(), edit.clone(), self.cfg.backups.clone());
        std::thread::spawn(move || {
            let got = crate::engine::shop::set_backup_settings(&dir, &mut own, &who, &edit, crate::updates::VERSION);
            if let Ok(mut r) = run.lock() {
                r.saving = false;
                r.saved = Some((got, own));
            }
        });
    }

    /// Take what saving backup settings gave, once it's done (BK7): this
    /// PC's own settings go in its settings file.
    pub(crate) fn backup_settings_saved(&mut self) {
        let Some((got, own)) = self.setui.backup_run.lock().ok().and_then(|mut r| r.saved.take()) else { return };
        match got {
            Ok(in_shop) => {
                if !in_shop {
                    self.cfg.backups = own;
                    self.apply_settings(false);
                }
                self.setui.backup_edit = None;
                self.setui.shop_note = Some((true, "Saved the backup settings.".into()));
            }
            Err(e) => self.setui.shop_note = Some((false, e)),
        }
        self.setui.shop = None;
    }

    /// BK2, BK3: take a backup in the background: the day's when the program
    /// opens, or Back up now. One at a time.
    pub fn start_backup(&self, why: crate::backup::Why) {
        let run = self.setui.backup_run.clone();
        // S9: a PC newer than the shop version writes nothing in the shop:
        // no daily backup (said only when asked for one).
        if let Some(ahead) = crate::engine::shop::ahead_of_shop(&self.library_dir(), crate::updates::VERSION) {
            if why == crate::backup::Why::Now
                && let Ok(mut r) = run.lock()
            {
                r.note = Some((false, format!("No backup was taken: the shop is read-only on this PC. {ahead}")));
                r.ended += 1;
            }
            return;
        }
        {
            let Ok(mut r) = run.lock() else { return };
            if r.since.is_some() {
                return;
            }
            r.since = Some(self.clock.now());
        }
        let (dir, who, clock, own) = (self.library_dir(), self.who.clone(), self.clock.clone(), self.cfg.backups.clone());
        // S2: closing the program waits for it.
        let working = self.handoff.begin(crate::updates::Work::Backup);
        std::thread::spawn(move || {
            let _working = working;
            use crate::backup::Daily;
            let got = match why {
                crate::backup::Why::Daily => crate::engine::shop::daily_backup(&dir, &own, &who, &clock),
                crate::backup::Why::Now => crate::engine::shop::backup_now(&dir, &own, &who, &clock).map(Daily::Taken),
            };
            let note = match got {
                Ok(Daily::Taken(d)) => Some(backup_note(&d)),
                // Today's was there: said only when its copy to the second
                // place can't be made (BK5).
                Ok(Daily::AlreadyToday { second: Some(Err(e)), .. }) => Some((false, e)),
                Ok(Daily::AlreadyToday { .. }) => None,
                // No Library on this PC yet: nothing to back up, nothing said.
                Err(_) if why == crate::backup::Why::Daily && !dir.is_dir() => None,
                Err(e) => Some((false, e)),
            };
            if let Ok(mut r) = run.lock() {
                r.since = None;
                r.ended += 1;
                r.note = note;
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Restore (BK8–BK13): the rules are in crate::backup and engine::shop.
// ---------------------------------------------------------------------------

impl App {
    /// BK8: the Restore window: the backups by date, newest first, one
    /// picked; Restore asks once more, then restores in the background.
    pub fn restore_window(&mut self, ctx: &egui::Context) {
        let Some(mut ask) = self.setui.restore.clone() else { return };
        let all = match &self.setui.backups {
            Some(Ok(o)) => o.all.clone(),
            _ => vec![],
        };
        let shop = match &self.setui.shop {
            Some(Ok(Some(st))) => st.shop.name.clone(),
            _ => String::new(),
        };
        let t = th();
        let mut open = true;
        let (mut go, mut cancel) = (false, false);
        egui::Window::new(RichText::new("Restore a backup").size(16.0).strong())
            .id(egui::Id::unique("restore_window"))
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_max_width(520.0);
                let picked = all.iter().find(|e| Some(&e.name) == ask.pick.as_ref()).cloned();
                if ask.sure {
                    let Some(e) = picked else {
                        ask.sure = false;
                        return;
                    };
                    ui.label(RichText::new(format!("Restore {shop} to {}?", e.shown())).size(14.0).strong().color(t.text_strong));
                    ui.add_space(6.0);
                    ui.label(RichText::new("Every job, part and sheet saved since then is taken out of the Library, on every PC. Programmers should save and close their jobs first.").color(t.text));
                    ui.add_space(4.0);
                    ui.label(RichText::new("The Library as it is now is kept first, as a backup named \"before restore\", so this can be undone.").size(11.5).color(t.text_dim));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        go = ui.add(egui::Button::new(RichText::new(format!("Restore to {}", e.shown())).color(egui::Color32::WHITE)).fill(t.warn_fg).min_size(egui::Vec2::new(0.0, 30.0))).clicked();
                        cancel = ui.add(egui::Button::new("Cancel — change nothing").min_size(egui::Vec2::new(0.0, 30.0))).clicked();
                    });
                    return;
                }
                ui.label(RichText::new("Pick the day to put the shop back to. Nothing changes until you confirm.").color(t.text));
                ui.add_space(6.0);
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for e in &all {
                        let mut text = format!("{}   on {}", e.shown(), e.pc);
                        if e.before_restore {
                            text.push_str("   (before restore)");
                        }
                        let on = ask.pick.as_ref() == Some(&e.name);
                        let r = ui.add(egui::Button::selectable(on, RichText::new(text).monospace().size(12.5)).min_size(egui::Vec2::new(480.0, 24.0)));
                        if r.on_hover_text(format!("{} — {:.1} MB", e.name, e.bytes as f64 / 1e6)).clicked() {
                            ask.pick = Some(e.name.clone());
                        }
                    }
                    if all.is_empty() {
                        ui.label(RichText::new("No backups yet.").color(t.text_dim));
                    }
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let label = picked.as_ref().map_or_else(|| "Restore…".to_string(), |e| format!("Restore {}…", e.shown()));
                    if ui.add_enabled(picked.is_some(), egui::Button::new(label).min_size(egui::Vec2::new(0.0, 28.0))).clicked() {
                        ask.sure = true;
                    }
                    cancel = ui.add(egui::Button::new("Cancel").min_size(egui::Vec2::new(0.0, 28.0))).clicked();
                });
            });
        if go {
            self.setui.restore = None;
            if let Some(name) = ask.pick {
                self.start_restore(&name);
            }
        } else if cancel || !open {
            self.setui.restore = None;
        } else {
            self.setui.restore = Some(ask);
        }
    }

    /// BK8: restore the backup named `name`, in the background (it zips and
    /// unpacks the whole Library). The card says how it went, and the page
    /// reads the shop and its backups again when it's done.
    pub(crate) fn start_restore(&self, name: &str) {
        let run = self.setui.backup_run.clone();
        {
            let Ok(mut r) = run.lock() else { return };
            if r.since.is_some() || r.restoring.is_some() {
                return;
            }
            r.restoring = Some(self.clock.now());
        }
        let (dir, who, clock, name) = (self.library_dir(), self.who.clone(), self.clock.clone(), name.to_string());
        // S2: closing the program waits for it.
        let working = self.handoff.begin(crate::updates::Work::Restore);
        std::thread::spawn(move || {
            let _working = working;
            let got = crate::engine::shop::restore(&dir, &name, &who, &clock, crate::updates::VERSION);
            if let Ok(mut r) = run.lock() {
                r.restoring = None;
                r.ended += 1;
                r.note = Some(restore_note(got));
            }
        });
    }
}

/// "9 Nov 09:20": the day of the backup a restore put back.
fn backup_day(r: &crate::shop::Restore) -> String {
    crate::backup::shown_stamp(&r.backup_when)
}

/// BK12, BK13: the Backups card's line for the shop's last restore, from
/// the shop file (true: a warning).
pub(crate) fn restore_line(r: &crate::shop::Restore, state: crate::engine::shop::RestoreState, clock: &crate::shared::Clock) -> (bool, String) {
    use crate::engine::shop::RestoreState::*;
    match state {
        Finished => (false, format!("Back to {}, by {}, {}", backup_day(r), r.by.pc, clock.shown(r.at))),
        Running => (false, format!("Restoring to {} on {} (since {})…", backup_day(r), r.by.pc, clock.shown(r.at))),
        Stopped => (true, format!("⚠ Restore to {} stopped halfway ({}, {}): restore its \"before restore\" backup", backup_day(r), r.by.pc, clock.shown(r.at))),
    }
}

/// BK8, BK9: what the Backups card says of a restore this PC made.
pub(crate) fn restore_note(got: Result<crate::engine::shop::RestoreDone, String>) -> (bool, String) {
    match got {
        Ok(r) => {
            let d = &r.done;
            let mut n = format!("Restored the shop to {}: {} file{} put back, {} taken out. The Library as it was is kept in {}.", d.from.shown(), d.files, if d.files == 1 { "" } else { "s" }, d.removed, d.before.name);
            let mut ok = true;
            if let Some(Err(e)) = &r.second {
                n.push_str(&format!(" {e}"));
                ok = false;
            }
            (ok, n)
        }
        Err(e) => (false, e),
    }
}

/// BK5, BK7: what the Backups card says of a backup this PC took: what it
/// is, and a second place that let it down (true: nothing did).
fn backup_note(d: &crate::backup::Done) -> (bool, String) {
    let mut n = format!("Backup taken: {}.", d.entry.name);
    let mut ok = true;
    if let Some(Err(e)) = &d.second {
        n.push_str(&format!(" {e}"));
        ok = false;
    }
    (ok, n)
}

// ---------------------------------------------------------------------------
// Updates on a PC of its own (spec #178, UP11–UP14): the check against the
// public releases, the banner, What's new and the Updates card. The rules
// are in engine::updates; the installer's calls in crate::updates. Agents
// have `shop mode=updates`; installing stays a person's choice, here.
// ---------------------------------------------------------------------------

impl App {
    /// UP11: start a check when the program opens and once a day after, and
    /// pick up its answer and a download's, never holding up the window.
    pub fn update_tick(&mut self, ctx: &egui::Context) {
        if self.setui.upd.source.is_none() {
            return;
        }
        let dir = self.library_dir();
        let h = self.handoff;
        let u = &mut self.setui.upd;
        let Some(src) = u.source.clone() else { return };
        u.follow_library(&dir, h);
        if let Some(rx) = &u.checking {
            match rx.try_recv() {
                Ok(found) => {
                    u.receive(found, h);
                    let (chosen, whats) = std::mem::take(&mut u.demo);
                    if whats && u.offer.is_some() {
                        u.whats_new = u.offer.clone();
                    }
                    if chosen {
                        self.update_when_closed();
                    }
                    return;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(250)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    u.found = Some(crate::updates::Found::NotChecked(crate::updates::COULDNT_CHECK.into()));
                    u.checking = None;
                }
            }
        } else if u.checked_at.is_none_or(|t| t.elapsed() >= crate::updates::CHECK_EVERY) {
            u.checked_at = Some(std::time::Instant::now());
            u.checking = Some(crate::updates::check_in_background(dir, crate::library::default_dir(), src));
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        } else if let Some(t) = u.checked_at {
            // Wake for the next day's check even if nobody touches the window.
            ctx.request_repaint_after(crate::updates::CHECK_EVERY.saturating_sub(t.elapsed()));
        }
        if u.poll_fetch() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    /// UP14: Update when I close: download the offered release in the
    /// background; it installs when the program closes, never before.
    fn update_when_closed(&mut self) {
        self.setui.upd.choose(crate::library::default_dir(), self.handoff);
    }

    /// UP13: the banner under the ribbon while a newer release is offered.
    pub fn update_banner(&mut self, ui: &mut egui::Ui) {
        let u = &self.setui.upd;
        let fetching = !matches!(u.fetch, Fetch::No);
        // While a chosen release downloads or waits, the banner is about it.
        let Some(o) = u.banner_release() else { return };
        if !fetching && u.later.as_deref() == Some(o.version.as_str()) {
            return;
        }
        let t = th();
        egui::Panel::top("update_banner")
            .frame(egui::Frame::new().fill(t.accent_soft).stroke(Stroke::new(1.0, t.accent_border)).inner_margin(egui::Margin::symmetric(10, 5)))
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let v = &o.version;
                    match &self.setui.upd.fetch {
                        Fetch::No => {
                            ui.label(RichText::new(format!("⬆ Version {v} is out.")).strong().color(t.text_strong));
                        }
                        Fetch::Running(..) => {
                            ui.spinner();
                            ui.label(RichText::new(format!("Downloading version {v}…")).strong().color(t.text_strong)).on_hover_text("It installs when you close the program, never while it's open.");
                        }
                        Fetch::Ready(_) => {
                            ui.label(RichText::new(format!("✔ Version {v} is ready: it installs when you close the program.")).strong().color(t.ok_fg))
                                .on_hover_text("Finish your work and close the program when you're done. The update installs then; the program doesn't restart by itself.");
                        }
                        Fetch::Failed(_, e) => {
                            ui.label(RichText::new(format!("⚠ {e}")).color(t.warn_fg));
                        }
                    }
                    if !o.machine_changes.is_empty() {
                        let n = o.machine_changes.len();
                        ui.label(RichText::new(format!("It changes machine programs ({n}): test-cut before using it.")).color(t.warn_fg))
                            .on_hover_text(o.machine_changes.join("\n"));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if matches!(self.setui.upd.fetch, Fetch::No) && ui.button("Later").on_hover_text("Hide this until the next version, or until the program opens again.").clicked() {
                            self.setui.upd.later = Some(v.clone());
                        }
                        let again = matches!(self.setui.upd.fetch, Fetch::Failed(..));
                        if matches!(self.setui.upd.fetch, Fetch::No | Fetch::Failed(..)) {
                            let label = if again { "Try again" } else { "Update when I close" };
                            let b = egui::Button::new(RichText::new(label).color(egui::Color32::WHITE)).fill(t.accent);
                            if ui.add(b).on_hover_text("Downloads it now, in the background. It installs when you close the program, never while a job is open.").clicked() {
                                self.setui.upd.fetch = Fetch::No;
                                self.update_when_closed();
                            }
                        }
                        // N1: the notes of the release this banner shows.
                        if ui.button("What's new").clicked() {
                            self.setui.upd.whats_new = Some(o.clone());
                        }
                    });
                });
            });
    }

    /// UP13: What's new: the release's notes, its changes to machine programs
    /// first.
    pub fn whats_new_window(&mut self, ctx: &egui::Context) {
        let Some(o) = self.setui.upd.whats_new.clone() else { return };
        let t = th();
        let mut open = true;
        let mut update = false;
        egui::Window::new(RichText::new(format!("What's new in {}", o.version)).size(16.0).strong())
            .id(egui::Id::unique("whats_new"))
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_max_width(560.0);
                ui.label(RichText::new(format!("This PC runs {}.", crate::updates::VERSION)).size(12.0).color(t.text_dim));
                ui.add_space(6.0);
                if !o.machine_changes.is_empty() {
                    egui::Frame::new().fill(t.warn_bg).corner_radius(CornerRadius::same(6)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(format!("⚠ {}", crate::engine::updates::MACHINE_SECTION)).strong().color(t.warn_fg));
                        ui.label(RichText::new("Test-cut before using this release on the machine.").size(12.0).color(t.text));
                        for e in &o.machine_changes {
                            ui.label(RichText::new(format!("• {e}")).size(12.0).color(t.text_strong));
                        }
                    });
                    ui.add_space(6.0);
                }
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    notes_text(ui, &o.notes);
                });
                ui.add_space(8.0);
                // Only for the public release offered to a PC of its own: in a
                // shop the admin PC tries and rolls out (UP15).
                let offered = self.setui.upd.offer.as_ref().is_some_and(|x| x.version == o.version);
                if offered && matches!(self.setui.upd.fetch, Fetch::No | Fetch::Failed(..)) {
                    let b = egui::Button::new(RichText::new("Update when I close").color(egui::Color32::WHITE)).fill(t.accent);
                    update = ui.add(b).clicked();
                }
            });
        if update {
            self.setui.upd.fetch = Fetch::No;
            self.update_when_closed();
        }
        if !open || update {
            self.setui.upd.whats_new = None;
        }
    }

    /// UP11, UP14: Settings → Shop → Updates: this PC's version and the
    /// newest release.
    fn updates_card(&mut self, ui: &mut egui::Ui) {
        use crate::updates::{Found, Source};
        let t = th();
        let (mut check, mut update, mut whats) = (false, false, false);
        card(ui, "Updates", "This PC's version and the newest release.", |ui| {
            row(ui, "This PC", |ui| {
                ui.label(RichText::new(crate::updates::VERSION).strong());
            });
            let u = &self.setui.upd;
            row(ui, "Newest release", |ui| match (&u.source, &u.found) {
                (Some(Source::Off(why)), _) => {
                    ui.label(RichText::new(*why).color(t.text_dim));
                }
                _ if u.checking.is_some() => {
                    ui.spinner();
                    ui.label(RichText::new("Checking…").color(t.text_dim));
                }
                (_, Some(Found::Releases(_))) => match (&u.offer, u.newest()) {
                    (Some(_), Some(n)) => {
                        ui.label(RichText::new(n).strong().color(t.accent_fg));
                        ui.label(RichText::new("is out").color(t.text));
                    }
                    (_, n) => {
                        ui.label(cap(&crate::engine::updates::up_to_date(crate::updates::VERSION, n)));
                    }
                },
                (_, Some(Found::InShop(name))) => {
                    ui.label(RichText::new(format!("From the shop folder: {name}'s admin PC rolls out releases.")).color(t.text_dim))
                        .on_hover_text("A PC in a shop never checks GitHub: it installs the release the admin PC rolled out, from the shop folder.");
                }
                (_, Some(Found::NotChecked(e))) => {
                    // UP11: no connection is said here only: no banner, no dialog.
                    let short = if e.starts_with(crate::updates::COULDNT_CHECK) { crate::updates::COULDNT_CHECK } else { e.as_str() };
                    ui.label(RichText::new(short).color(t.warn_fg)).on_hover_text(e);
                }
                (None, None) => {
                    ui.label(RichText::new("Not checked").color(t.text_dim));
                }
                (Some(_), None) => {
                    ui.label(RichText::new("Checking…").color(t.text_dim));
                }
            });
            if let Some(o) = &u.offer {
                if !o.machine_changes.is_empty() {
                    ui.label(RichText::new(format!("⚠ It changes machine programs ({}): test-cut before using it.", o.machine_changes.len())).size(11.5).color(t.warn_fg))
                        .on_hover_text(o.machine_changes.join("\n"));
                }
                if matches!(u.fetch, Fetch::No | Fetch::Failed(..)) {
                    row(ui, "", |ui| {
                        update = page_button(ui, "Shop", "Update when I close", true).on_hover_text("Downloads it now, in the background. It installs when you close the program, never while a job is open.").clicked();
                        whats = page_button(ui, "Shop", "What's new", true).clicked();
                    });
                }
            }
            // S6: the chosen release keeps its own line; "Newest release"
            // above is the latest check's.
            match &u.fetch {
                Fetch::No => {}
                Fetch::Failed(_, e) => {
                    ui.label(RichText::new(format!("⚠ {e}")).size(11.5).color(t.warn_fg));
                }
                Fetch::Running(v, _) => {
                    ui.label(RichText::new(format!("Downloading version {v}…")).size(11.5).color(t.text_dim));
                }
                Fetch::Ready(v) => {
                    ui.label(RichText::new(format!("Version {v} installs when you close the program.")).size(11.5).color(t.ok_fg));
                }
            }
            let can_check = matches!(u.source, Some(Source::Github | Source::Fake(_))) && u.checking.is_none();
            if can_check {
                ui.add_space(4.0);
                check = page_button(ui, "Shop", "Check now", true).on_hover_text("This PC checks by itself when the program opens and once a day.").clicked();
            }
        });
        if check {
            self.setui.upd.checked_at = None;
        }
        if whats {
            self.setui.upd.whats_new = self.setui.upd.offer.clone();
        }
        if update {
            self.setui.upd.fetch = Fetch::No;
            self.update_when_closed();
        }
    }

    /// `--fake-releases=<file>` (screenshots) or `NESTPRO_FAKE_RELEASES`: a
    /// made-up release list in place of GitHub, on any system. Without one, a
    /// screenshot never checks.
    pub fn start_update_checks(&mut self, fake: Option<std::path::PathBuf>) {
        let src = crate::updates::source(fake.clone());
        if fake.is_none() && self.shot_mode_now() && src == crate::updates::Source::Github {
            return;
        }
        self.setui.upd.source = Some(src);
    }
}

/// UP13: release notes as written (UP1): headings, list lines and text.
fn notes_text(ui: &mut egui::Ui, notes: &str) {
    let t = th();
    let mut skip = false;
    for line in notes.lines() {
        let l = line.trim();
        if let Some(h) = l.strip_prefix('#') {
            let h = h.trim_start_matches('#').trim();
            // The machine-program changes are shown above, on their own.
            skip = h.eq_ignore_ascii_case(crate::engine::updates::MACHINE_SECTION);
            if !skip {
                ui.add_space(4.0);
                ui.label(RichText::new(h).strong().color(t.text_strong));
            }
            continue;
        }
        if skip || l.is_empty() {
            continue;
        }
        let text = l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")).map_or(l.to_string(), |e| format!("• {e}"));
        ui.label(RichText::new(text).size(12.0).color(t.text));
    }
}

/// SH16: Settings → Shop offers Join a shop… whatever this PC's Library is:
/// no shop, a shop (joining another asks first whether to leave it, SH14),
/// or a folder that can't be reached or used (gone, or never made). Its
/// hover text for each.
pub fn join_offered(shop: &Result<Option<crate::engine::shop::Status>, String>) -> String {
    let base = "Use the shop's folder on the share, e.g. \\\\srv\\Nesting. The folder is checked first";
    match shop {
        Ok(Some(st)) => format!("Join another shop instead of {}: {base}, and this PC is asked first whether to leave {}.", st.shop.name, st.shop.name),
        Ok(None) => format!("{base}; this PC's own Library is left as it is."),
        Err(_) => format!("{base}. This PC's Library can't be reached just now, so you are asked first whether to join instead."),
    }
}

/// `--shop-demo[=…]` on the command line.
pub fn shop_demo_arg() -> Option<String> {
    std::env::args().find(|a| a == "--shop-demo" || a.starts_with("--shop-demo=")).map(|a| a.strip_prefix("--shop-demo=").unwrap_or("").to_string())
}

/// SH4: what to keep and say after this PC tried to add itself to its shop:
/// why it failed (kept for the PCs card until a try succeeds), and the note
/// for a person who pressed Add this PC. Only a record written counts:
/// `Ok(false)` (the folder was no shop when it looked) is no success.
pub fn added_note(pc: &str, r: Result<bool, String>, asked: bool) -> (Option<String>, Option<(bool, String)>) {
    match r {
        Ok(true) => (None, asked.then(|| (true, format!("{pc} is in the shop's PCs list.")))),
        Ok(false) => (None, asked.then(|| (false, format!("{pc} wasn't added: this PC's Library isn't a shop any more.")))),
        Err(e) => (Some(e), None),
    }
}

/// The temporary Library `--shop-demo` works in: `Nesting` inside a folder
/// made fresh for this run in the system's temporary folder (never one that
/// was there before, and never a real Library). Made once, the first time
/// it is asked for; None when no folder could be made.
pub fn shop_demo_dir() -> Option<std::path::PathBuf> {
    static DIR: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| new_temp_folder("nestpro-shop-demo").map(|d| d.join("Nesting"))).clone()
}

/// A folder made just now in the system's temporary folder, under a name no
/// folder had: made with create-dir, so a folder already there (another
/// run's, whatever it holds) is never used, and nothing is deleted.
pub fn new_temp_folder(stem: &str) -> Option<std::path::PathBuf> {
    let base = std::env::temp_dir();
    for n in 0..1000u32 {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
        let d = base.join(format!("{stem}-{}-{nanos:09}-{n}", std::process::id()));
        match std::fs::create_dir(&d) {
            Ok(()) => return Some(d),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}
