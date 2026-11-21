// Start-up rules from the original (MECHANICS.md "Start-up", SU1–SU5):
// - it changes to the program's own folder first (SU1),
//   so loose files such as the window icon are found next to the program;
// - it reads the first command-line word: "/hide" starts without showing
//   the main window, a name ending in ".NXP" or ".nxp" is opened once the
//   window is up (SU3);
// - the main window is shown maximized (show state 3, SW_SHOWMAXIMIZED),
//   or hidden (0) in /hide mode; the icons are set last (SU4–SU5).

use std::path::PathBuf;

/// ShowWindow states the original uses for its main window.
pub const SW_HIDE: i32 = 0;
pub const SW_SHOWMAXIMIZED: i32 = 3;
/// The first-word switch for starting hidden.
pub const HIDE_SWITCH: &str = "/hide";
/// The extension compared at the end of the first word, in the two spellings
/// the original accepts (it checks 'N','X','P' or 'n','x','p' letter by letter).
pub const OPEN_EXT_UPPER: &str = ".NXP";
pub const OPEN_EXT_LOWER: &str = ".nxp";

/// What a plain first argument asks for.
#[derive(Debug, PartialEq)]
pub enum StartFile {
    /// A Lantek nesting export (the original's rule).
    Nesting(PathBuf),
    /// One of this app's own job files (so double-clicking one opens it).
    Job(PathBuf),
}

/// The first command-line word when it isn't one of the app's `--` options.
pub fn first_word(args: &[String]) -> Option<&str> {
    args.get(1).map(String::as_str).filter(|a| !a.starts_with("--"))
}

/// The main window's show state for a first word (SW_HIDE for "/hide").
pub fn show_state(first: Option<&str>) -> i32 {
    if first == Some(HIDE_SWITCH) { SW_HIDE } else { SW_SHOWMAXIMIZED }
}

/// The file to open after start-up, if the first word names one.
pub fn start_file(first: Option<&str>, job_ext: &str) -> Option<StartFile> {
    let w = first?;
    if w.ends_with(OPEN_EXT_UPPER) || w.ends_with(OPEN_EXT_LOWER) {
        return Some(StartFile::Nesting(PathBuf::from(w)));
    }
    let p = PathBuf::from(w);
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case(job_ext)) {
        return Some(StartFile::Job(p));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn nxp_in_either_spelling() {
        let f = |w: &str| start_file(Some(w), "snjob");
        assert_eq!(f("C:\\x\\job.NXP"), Some(StartFile::Nesting("C:\\x\\job.NXP".into())));
        assert_eq!(f("job.nxp"), Some(StartFile::Nesting("job.nxp".into())));
        // Mixed case isn't one of the original's two spellings.
        assert_eq!(f("job.Nxp"), None);
        assert_eq!(f("job.dxf"), None);
        assert_eq!(f("plate.SNJOB"), Some(StartFile::Job("plate.SNJOB".into())));
    }

    #[test]
    fn options_are_not_files() {
        assert_eq!(first_word(&a(&["app", "--screenshot", "x.png"])), None);
        assert_eq!(first_word(&a(&["app", "a.nxp"])), Some("a.nxp"));
        assert_eq!(first_word(&a(&["app"])), None);
    }

    #[test]
    fn maximized_unless_hidden() {
        assert_eq!(show_state(None), SW_SHOWMAXIMIZED);
        assert_eq!(show_state(Some("a.nxp")), SW_SHOWMAXIMIZED);
        assert_eq!(show_state(Some("/hide")), SW_HIDE);
    }
}

// ---------------------------------------------------------------------------
// The first-run screen (spec #178, SH9, SH16): Start a shop, Join a shop or
// Just this PC, the first time the program opens on a PC with no settings.
// The rules are in engine::shop; agents have `setup join=` and `setup start=`.
// ---------------------------------------------------------------------------

/// SH9: the first-run screen shows only on a PC with no settings yet: no
/// settings file, nothing else in its settings folder, and no Library in the
/// default place on this PC. A PC that has used the program (settings saved,
/// or a Library of its own) never sees it, after an update included.
pub fn first_run(settings_file: &std::path::Path, default_library: &std::path::Path) -> bool {
    if settings_file.exists() {
        return false;
    }
    let folder_empty = settings_file.parent().map_or(true, |d| match std::fs::read_dir(d) {
        Ok(mut e) => e.next().is_none(),
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    });
    folder_empty && !crate::library::holds_library(default_library)
}

/// The three choices (SH9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Start,
    Join,
    ThisPc,
}

impl Choice {
    /// Its title and its one short line.
    pub fn words(self) -> (&'static str, &'static str) {
        match self {
            Choice::Start => ("Start a shop", "Make a folder on the share the shop's folder. This PC becomes its admin PC."),
            Choice::Join => ("Join a shop", "Use the shop's folder on the share, e.g. \\\\srv\\Nesting."),
            Choice::ThisPc => ("Just this PC", "A Library on this PC only. You can join a shop later."),
        }
    }
}

/// What a start or join running in the background answers. Each carries
/// the folder as typed when it was asked, so a question it raises is about
/// that folder only.
pub enum Answer {
    Joined(crate::engine::shop::Joined),
    Refused(crate::engine::shop::Refused, String),
    Started { folder: std::path::PathBuf, name: String, unrecorded: Option<String> },
    /// SH16: the folder picked to start a shop in is there but holds no
    /// Library: asked before one is made.
    NoLibrary(std::path::PathBuf, String),
    Failed(String),
}

/// A question the screen asks about one folder (SH14, SH16): only while the
/// folder box still says what it said when the folder was checked.
#[derive(Clone, Debug, PartialEq)]
pub struct Ask {
    /// The folder box when it was checked.
    pub typed: String,
    pub what: Asked,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Asked {
    /// SH16: make a new, empty Library in this folder (checked: it is there
    /// and holds none) and start the shop there.
    MakeLibrary(std::path::PathBuf),
    /// SH14: this PC is in shop `name` (folder `from`): leave it and join.
    /// No name: its Library folder `from` can't be reached, so which shop it
    /// is in can't be told: join this one instead.
    Leave { name: Option<String>, from: std::path::PathBuf },
}

/// The first-run screen's state, or the Join a shop window's from Settings
/// → Shop (`from_settings`: only the join part, with Cancel).
#[derive(Default)]
pub struct FirstRun {
    pub pick: Option<Choice>,
    pub from_settings: bool,
    pub folder: String,
    pub name: String,
    /// A start or join on its way (a share can take seconds to answer, and
    /// the write lock up to 2 minutes).
    pub pending: Option<std::sync::mpsc::Receiver<Answer>>,
    /// The checks of the last join, and why it was refused (None: joined).
    pub checks: Vec<crate::engine::shop::Check>,
    pub refused: Option<String>,
    /// The question open about the folder checked last.
    pub ask: Option<Ask>,
    /// Done: what was set up, shown with Start working.
    pub done: Option<String>,
}

impl FirstRun {
    pub fn join_from_settings() -> FirstRun {
        FirstRun { pick: Some(Choice::Join), from_settings: true, ..Default::default() }
    }

    /// The question open, while the folder box still names the folder it is
    /// about (SH14, SH16); a folder typed since takes it away.
    pub fn asked(&self) -> Option<&Asked> {
        self.ask.as_ref().filter(|a| a.typed == self.folder).map(|a| &a.what)
    }
}

impl crate::App {
    /// SH9, SH16: the first-run screen (or Settings → Shop's Join a shop).
    pub fn first_run_window(&mut self, ctx: &eframe::egui::Context) {
        use crate::theme::th;
        use eframe::egui::{self, RichText};
        let Some(mut fr) = self.firstrun.take() else { return };
        if let Some(rx) = &fr.pending {
            match rx.try_recv() {
                Ok(a) => {
                    fr.pending = None;
                    self.first_run_answer(&mut fr, a);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(150)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    fr.pending = None;
                    fr.refused = Some("The try stopped before it answered. Nothing was changed. Try again.".into());
                }
            }
        }
        // A folder typed since the question was asked takes it away.
        if fr.ask.is_some() && fr.asked().is_none() {
            fr.ask = None;
        }
        let busy = fr.pending.is_some();
        let mut close = false;
        let title = if fr.from_settings { "Join a shop" } else { "Set up this PC" };
        egui::Modal::new(egui::Id::unique("first_run")).show(ctx, |ui| {
            let t = th();
            ui.set_width(560.0);
            ui.label(RichText::new(title).size(20.0).strong().color(t.text_strong));
            if !fr.from_settings {
                ui.label(RichText::new("Where does this PC keep its jobs, parts and sheet stock? You can change it later in Settings.").color(t.text_dim));
            }
            ui.add_space(10.0);
            if let Some(done) = fr.done.clone() {
                ui.label(RichText::new(format!("✔ {done}")).color(t.ok_fg));
                check_list(ui, &fr.checks);
                ui.add_space(10.0);
                if ui.button(RichText::new("Start working").strong()).clicked() {
                    close = true;
                }
                return;
            }
            if !fr.from_settings {
                let (pick, mut clicked) = (fr.pick, None);
                ui.add_enabled_ui(!busy, |ui| {
                    let line = |ui: &mut egui::Ui, c: Choice| {
                        ui.label(RichText::new(c.words().1).color(t.text));
                    };
                    egui::Grid::new("first_run_choices").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
                        if ui.selectable_label(pick == Some(Choice::Start), RichText::new("Start a shop").size(15.0).strong()).clicked() {
                            clicked = Some(Choice::Start);
                        }
                        line(ui, Choice::Start);
                        ui.end_row();
                        if ui.selectable_label(pick == Some(Choice::Join), RichText::new("Join a shop").size(15.0).strong()).clicked() {
                            clicked = Some(Choice::Join);
                        }
                        line(ui, Choice::Join);
                        ui.end_row();
                        if ui.selectable_label(pick == Some(Choice::ThisPc), RichText::new("Just this PC").size(15.0).strong()).clicked() {
                            clicked = Some(Choice::ThisPc);
                        }
                        line(ui, Choice::ThisPc);
                        ui.end_row();
                    });
                });
                if let Some(c) = clicked {
                    fr.pick = Some(c);
                    fr.checks.clear();
                    fr.refused = None;
                    fr.ask = None;
                }
                ui.add_space(4.0);
                ui.separator();
            }
            let folder_row = |ui: &mut egui::Ui, folder: &mut String| {
                ui.horizontal(|ui| {
                    ui.label("Shop folder");
                    ui.add_enabled(!busy, egui::TextEdit::singleline(folder).desired_width(330.0).hint_text("\\\\srv\\Nesting"));
                    if ui.add_enabled_ui(!busy, |ui| ui.button("Browse…")).inner.clicked() {
                        if let Some(d) = rfd::FileDialog::new().set_title("The shop folder on the share").pick_folder() {
                            *folder = d.display().to_string();
                        }
                    }
                });
            };
            match fr.pick {
                None => {}
                Some(Choice::Join) => {
                    folder_row(ui, &mut fr.folder);
                    ui.label(RichText::new("Pick the folder by its full \\\\server\\share path, or a mapped drive: the program uses the path behind the drive.").size(11.5).color(t.text_dim));
                    ui.add_space(6.0);
                    let leave = match fr.asked() {
                        Some(Asked::Leave { name, from }) => Some((name.clone(), from.clone())),
                        _ => None,
                    };
                    ui.horizontal_wrapped(|ui| {
                        // SH14: leaving the shop this PC is in is asked first,
                        // and so is joining while this PC's own shop folder
                        // can't be reached (which shop it is in can't be told).
                        match &leave {
                            Some((Some(name), from)) => {
                                ui.label(RichText::new(format!("This PC is in shop {name} ({}). Leave it and join this shop?", from.display())).color(t.text));
                            }
                            Some((None, from)) => {
                                ui.label(RichText::new(format!("This PC's shop folder {} can't be reached. Join this shop instead?", from.display())).color(t.text));
                            }
                            None => {}
                        }
                    });
                    ui.horizontal(|ui| {
                        if let Some((name, _)) = &leave {
                            let yes = if name.is_some() { ui.add_enabled_ui(!busy, |ui| ui.button("Leave that shop and join")).inner } else { ui.add_enabled_ui(!busy, |ui| ui.button("Join this shop instead")).inner };
                            if yes.clicked() {
                                self.first_run_join(&mut fr, true);
                            }
                        } else if ui.add_enabled_ui(!busy && !fr.folder.trim().is_empty(), |ui| ui.button("Join the shop")).inner.clicked() {
                            self.first_run_join(&mut fr, false);
                        }
                        if busy {
                            ui.spinner();
                            ui.label("Checking the folder…");
                        }
                    });
                }
                Some(Choice::Start) => {
                    folder_row(ui, &mut fr.folder);
                    ui.horizontal(|ui| {
                        ui.label("Shop name");
                        ui.add_enabled(!busy, egui::TextEdit::singleline(&mut fr.name).desired_width(220.0).hint_text("Acme"));
                    });
                    ui.add_space(6.0);
                    let ready = !busy && !fr.folder.trim().is_empty() && !fr.name.trim().is_empty();
                    if let Some(Asked::MakeLibrary(d)) = fr.asked().cloned() {
                        ui.label(RichText::new(format!("{} holds no Library yet. Make a new, empty Library there and start the shop in it? Every PC that joins then uses that folder.", d.display())).color(t.text));
                        if ui.add_enabled_ui(ready, |ui| ui.button("Make a Library there and start the shop")).inner.clicked() {
                            self.first_run_start(&mut fr, Some(d));
                        }
                    } else if ui.add_enabled_ui(ready, |ui| ui.button("Start the shop")).inner.clicked() {
                        self.first_run_start(&mut fr, None);
                    }
                }
                Some(Choice::ThisPc) => {
                    let lib = crate::library::default_dir();
                    ui.label(RichText::new(format!("The Library goes in {} on this PC.", lib.display())).color(t.text));
                    ui.add_space(6.0);
                    if ui.button("Use this PC only").clicked() {
                        let mut save = |c: &crate::settings::AppSettings| c.save().map_err(|e| e.to_string());
                        match crate::library::create(&lib).and_then(|()| self.use_library(std::path::Path::new(""), &mut save)) {
                            Ok(_) => {
                                close = true;
                                self.status = format!("This PC keeps its own Library, in {}. Join a shop any time on Settings → Shop.", lib.display());
                            }
                            Err(e) => fr.refused = Some(e),
                        }
                    }
                }
            }
            if let Some(why) = &fr.refused {
                ui.add_space(6.0);
                ui.label(RichText::new(format!("⚠ {why}")).color(t.warn_fg));
            }
            if !fr.checks.is_empty() {
                check_list(ui, &fr.checks);
            }
            if fr.from_settings {
                ui.add_space(8.0);
                if ui.add_enabled_ui(!busy, |ui| ui.button("Cancel")).inner.clicked() {
                    close = true;
                }
            }
        });
        if !close {
            self.firstrun = Some(fr);
        }
    }

    /// Join a shop, in the background (SH10, SH11); `leave`: the person said
    /// to leave the shop this PC is in (SH14).
    fn first_run_join(&mut self, fr: &mut FirstRun, leave: bool) {
        let (tx, rx) = std::sync::mpsc::channel();
        let (typed, current, who, clock) = (fr.folder.clone(), self.library_dir(), self.who.clone(), self.clock.clone());
        std::thread::spawn(move || {
            let a = match crate::engine::shop::join(&typed, &current, &who, &clock, crate::updates::VERSION, leave) {
                Ok(j) => Answer::Joined(j),
                Err(r) => Answer::Refused(r, typed),
            };
            let _ = tx.send(a);
        });
        fr.checks.clear();
        fr.refused = None;
        fr.ask = None;
        fr.pending = Some(rx);
    }

    /// Start a shop, in the background (SH1, SH16): in the folder picked,
    /// named on its share (SH13). A folder there with no Library is asked
    /// about first; `make`: the folder the person said yes for, as it was
    /// checked (never the box's text since), where a Library is made
    /// without making it or any folder above it.
    fn first_run_start(&mut self, fr: &mut FirstRun, make: Option<std::path::PathBuf>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let (typed, name, who, clock) = (fr.folder.clone(), fr.name.trim().to_string(), self.who.clone(), self.clock.clone());
        std::thread::spawn(move || {
            let dir = make.clone().unwrap_or_else(|| std::path::PathBuf::from(crate::shop::full_folder(&crate::shop::absolute_folder(&crate::shop::tidy_folder(&typed, cfg!(windows)))).0));
            let a = if !dir.is_dir() {
                // Never made in place of a share that's down (SH8, LB11).
                Answer::Failed(format!("{} can't be reached. Check the folder's name, and that this PC is on the network. Nothing was made.", dir.display()))
            } else if !crate::library::holds_library(&dir) && make.is_none() {
                Answer::NoLibrary(dir, typed)
            } else {
                match crate::library::create_in_existing(&dir).and_then(|()| crate::engine::shop::start_shop(&dir, &name, &who, &clock, crate::updates::VERSION)) {
                    Ok(st) => Answer::Started { folder: dir, name: st.shop.name, unrecorded: st.unrecorded },
                    Err(e) => Answer::Failed(e),
                }
            };
            let _ = tx.send(a);
        });
        fr.refused = None;
        fr.ask = None;
        fr.pending = Some(rx);
    }

    /// SH10 (S4, S6): this PC's Library becomes `folder` (empty: this PC's
    /// own, in the default place). The settings with it, the active job
    /// cleared and the first-run screen answered, are saved first with
    /// `save`; only once they are does the window switch: it forgets the open
    /// jobs' stock bookings and the remnants they stored (they are the old
    /// Library's, never booked into the new one), withdraws any update
    /// offered from the public releases, and reads the new Library's
    /// materials. Err: nothing switched. Ok: why this PC's materials couldn't
    /// join the new Library's list, when they couldn't.
    pub fn use_library(&mut self, folder: &std::path::Path, save: &mut dyn FnMut(&crate::settings::AppSettings) -> Result<(), String>) -> Result<Option<String>, String> {
        let staged = crate::engine::shop::joined_settings(&self.cfg, folder);
        if !self.shot_mode_now() {
            save(&staged)?;
        }
        self.cfg = staged;
        self.forget_old_library_stock();
        // S2: a PC in a shop never takes a public release: whatever was
        // offered or chosen from them is withdrawn.
        self.setui.upd.withdraw(self.handoff);
        self.setui.shop = None;
        Ok(self.library_folder_changed())
    }

    /// SH10 (S6): the Library changed: the open jobs' sheets taken from
    /// stock, what they hold and the remnants they stored are the old
    /// Library's stock: forgotten, so a save never books them against the
    /// new Library's, and the undo steps that could bring them back go too.
    /// A join and a trial's switches (Try on this PC, Stop trying, back to
    /// the shop after a roll out) all do this.
    pub fn forget_old_library_stock(&mut self) {
        let forget = |formats: &mut [crate::nest::SheetFormat], held: &mut std::collections::BTreeMap<String, i32>| {
            crate::engine::stockflow::forget_booking(formats, held);
            for f in formats.iter_mut() {
                f.stock_ref.clear();
            }
        };
        forget(&mut self.result.formats, &mut self.held);
        for j in &mut self.jobs {
            forget(&mut j.result.formats, &mut j.held);
        }
        self.undo.clear();
        self.redo.clear();
    }

    /// Take in what a start or join answered: on success this PC's Library
    /// is the shop folder from now on (SH10); when its settings can't be
    /// saved, nothing switches and a join's record is taken back.
    fn first_run_answer(&mut self, fr: &mut FirstRun, a: Answer) {
        let mut save = |c: &crate::settings::AppSettings| c.save().map_err(|e| e.to_string());
        match a {
            Answer::Joined(j) => {
                let switch = !j.already || self.cfg.setup_pending;
                let materials = if switch { self.use_library(&j.folder, &mut save) } else { Ok(None) };
                match materials {
                    Err(e) => {
                        let back = match crate::engine::shop::undo(&j, &self.who) {
                            Ok(()) => format!("{} was taken off its PCs list again", self.who.pc),
                            Err(u) => format!("{} couldn't be taken off its PCs list again ({u})", self.who.pc),
                        };
                        fr.refused = Some(format!("Not joined shop {}: this PC's settings couldn't be saved ({e}). This PC's Library is still {}, and {back}.", j.shop.name, self.library_dir().display()));
                        fr.checks = j.checks;
                    }
                    Ok(m) => {
                        self.setui.shop_unrecorded = j.unrecorded.clone();
                        let mut done = j.summary();
                        if let Some(e) = m {
                            done.push_str(&format!(" This PC's materials couldn't join the shop's material list just now ({e}); they join the next time the list is read."));
                        }
                        fr.done = Some(done);
                        fr.checks = j.checks;
                    }
                }
            }
            // SH14: leaving the shop this PC is in is a question for the
            // person (the agents' wording, leave=true, isn't theirs).
            Answer::Refused(r, typed) if r.leaving.is_some() => {
                let (name, from) = r.leaving.clone().unwrap_or_default();
                fr.ask = Some(Ask { typed, what: Asked::Leave { name, from } });
                fr.checks = r.checks.into_iter().filter(|c| c.what != "leave").collect();
            }
            Answer::Refused(r, _) => {
                fr.refused = Some(r.why);
                fr.checks = r.checks;
            }
            Answer::Started { folder, name, unrecorded } => match self.use_library(&folder, &mut save) {
                Ok(m) => {
                    self.setui.shop_unrecorded = unrecorded;
                    let mut done = format!("Started shop {name} in {}. This PC is its admin PC; other PCs join it with Join a shop, or `nestpro setup --join {}`.", folder.display(), folder.display());
                    if let Some(e) = m {
                        done.push_str(&format!(" This PC's materials couldn't join the shop's material list just now ({e}); they join the next time the list is read."));
                    }
                    fr.done = Some(done);
                }
                Err(e) => fr.refused = Some(format!("Started shop {name} in {}, but this PC's settings couldn't be saved ({e}): its Library is still {}. Point Settings → Folders at {} once they can be.", folder.display(), self.library_dir().display(), folder.display())),
            },
            Answer::NoLibrary(d, typed) => fr.ask = Some(Ask { typed, what: Asked::MakeLibrary(d) }),
            Answer::Failed(e) => fr.refused = Some(e),
        }
    }

    /// `--first-run[=join|leave]` (screenshot runs only): the first-run
    /// screen; `join`: joined to a made-up shop Acme in a temporary folder of
    /// its own, with each check shown; `leave`: Settings → Shop's Join a
    /// shop on a PC in a made-up shop Other, asked whether to leave it.
    /// Never a real Library or shop.
    pub fn demo_first_run(&mut self, what: &str) {
        if !self.shot_mode_now() {
            return;
        }
        let mut fr = FirstRun::default();
        if what == "join" || what == "leave" {
            use crate::shared::{Clock, Who};
            let Some(base) = crate::settings_ui::new_temp_folder("nestpro-first-run-demo") else { return };
            let dir = base.join("Nesting");
            if crate::library::create(&dir).is_err() {
                return;
            }
            let now = self.clock.now();
            let _ = crate::engine::shop::start(&dir, "Acme", &Who::new("sam", "PC-A"), &Clock::at(now - 40 * 86400), crate::updates::VERSION);
            self.who = Who::new("kim", "PC-B");
            if what == "leave" {
                let other = base.join("Other");
                if crate::library::create(&other).is_err() {
                    return;
                }
                let _ = crate::engine::shop::start(&other, "Other", &self.who, &Clock::at(now - 9 * 86400), crate::updates::VERSION);
                self.cfg.library_dir = other.display().to_string();
                fr = FirstRun::join_from_settings();
            }
            fr.pick = Some(Choice::Join);
            fr.folder = dir.display().to_string();
            let a = match crate::engine::shop::join(&fr.folder, &self.library_dir(), &self.who, &self.clock, crate::updates::VERSION, false) {
                Ok(j) => Answer::Joined(j),
                Err(r) => Answer::Refused(r, fr.folder.clone()),
            };
            self.first_run_answer(&mut fr, a);
        }
        self.firstrun = Some(fr);
    }
}

/// Each check of a join, one per line, marked.
fn check_list(ui: &mut eframe::egui::Ui, checks: &[crate::engine::shop::Check]) {
    use crate::engine::shop::Mark;
    use crate::theme::th;
    use eframe::egui::RichText;
    let t = th();
    ui.add_space(6.0);
    for c in checks {
        let (m, col) = match c.mark {
            Mark::Ok => ("✔", t.ok_fg),
            Mark::Note => ("!", t.warn_fg),
            Mark::No => ("✖", t.warn_fg),
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(m).color(col).strong());
            ui.label(RichText::new(&c.said).size(12.0).color(t.text));
        });
    }
}

#[cfg(test)]
mod first_run_tests {
    use super::*;

    /// SH9: only a PC with no settings and no Library of its own gets the
    /// first-run screen.
    #[test]
    fn only_a_new_pc_gets_the_first_run_screen() {
        let d = crate::testdir::TestDir::new("first_run");
        let (settings, lib) = (d.join("config").join("nestpro").join("settings.ini"), d.join("Documents").join("Library"));
        // A new PC: nothing at all.
        assert!(first_run(&settings, &lib));
        // The settings folder made, but empty: still new.
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        assert!(first_run(&settings, &lib));
        // Anything in the settings folder (a material list, the materials
        // a Library was joined with): it has been used.
        std::fs::write(settings.parent().unwrap().join("materials-joined.json"), "[]").unwrap();
        assert!(!first_run(&settings, &lib));
        std::fs::remove_file(settings.parent().unwrap().join("materials-joined.json")).unwrap();
        // A Library of its own in the default place (settings lost): used.
        crate::library::create(&lib).unwrap();
        assert!(!first_run(&settings, &lib));
        std::fs::remove_dir_all(&lib).unwrap();
        // Settings saved: an existing user, after an update too.
        std::fs::write(&settings, "library_dir = \n").unwrap();
        assert!(!first_run(&settings, &lib));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// SH14, SH16: a question is about the folder checked: once the folder
    /// box says something else, it is gone, and Yes can't act on the new text.
    #[test]
    fn a_question_is_only_about_the_folder_that_was_checked() {
        let checked = std::path::PathBuf::from("/srv/Nesting");
        let mut fr = FirstRun { folder: "/srv/Nesting".into(), ..Default::default() };
        fr.ask = Some(Ask { typed: fr.folder.clone(), what: Asked::MakeLibrary(checked.clone()) });
        assert_eq!(fr.asked(), Some(&Asked::MakeLibrary(checked)));
        fr.folder = "/srv/Other".into();
        assert_eq!(fr.asked(), None, "the question stayed for a folder typed since");
    }

    /// SH16: a Library is made only in a folder that is there: never the
    /// folder or those above it (a share gone since it was checked).
    #[test]
    fn a_library_is_never_made_in_place_of_a_folder_that_went() {
        let d = crate::testdir::TestDir::new("create_existing");
        let gone = d.join("srv").join("Nesting");
        let e = crate::library::create_in_existing(&gone).unwrap_err();
        assert!(e.contains("can't be reached") && !d.exists(), "{e}");
        std::fs::create_dir_all(&gone).unwrap();
        crate::library::create_in_existing(&gone).unwrap();
        assert!(crate::library::holds_library(&gone));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// SH10 (S4, S6): the window switches to the shop only once its settings
    /// are saved; the settings it saves have the shop folder, no active job
    /// (the old Library's) and the first-run screen answered; the open job's
    /// stock booking and stored remnants are forgotten, never booked into
    /// the shop. A save that fails changes nothing.
    #[test]
    fn the_window_switches_to_a_shop_only_once_its_settings_are_saved() {
        let shop = std::path::PathBuf::from("/srv/Nesting");
        let mut a = crate::App::new(None);
        a.cfg.setup_pending = true;
        a.cfg.active_job = "7".into();
        a.held.insert("S1".into(), 2);
        let (cfg, held) = (a.cfg.clone(), a.held.clone());
        let e = a.use_library(&shop, &mut |_| Err("the disk is full".into())).unwrap_err();
        assert_eq!(e, "the disk is full");
        assert_eq!((&a.cfg, &a.held), (&cfg, &held), "a save that failed changed something");
        let mut saved = None;
        a.use_library(&shop, &mut |c| {
            saved = Some(c.clone());
            Ok(())
        })
        .unwrap();
        let saved = saved.unwrap();
        assert_eq!((saved.library_dir.as_str(), saved.active_job.as_str(), saved.setup_pending), ("/srv/Nesting", "", false));
        assert_eq!(a.cfg, saved);
        assert!(a.held.is_empty() && a.jobs.iter().all(|j| j.held.is_empty()), "the old Library's booking was kept");
        assert!(a.result.formats.iter().all(|f| f.made.is_empty()));
    }

    /// SH10 (S6): on switching Library the open jobs' sheets taken from
    /// stock and the remnants they stored are forgotten, and the undo steps
    /// that could bring the old Library's stock back go too.
    #[test]
    fn switching_library_forgets_the_old_librarys_stock_and_undo() {
        let mut a = crate::App::new(None);
        if a.result.formats.is_empty() {
            a.result.formats.push(crate::nest::SheetFormat::rect(2000.0, 1000.0));
        }
        for f in &mut a.result.formats {
            f.stock_ref = "S1".into();
            f.made = vec!["R1".into()];
        }
        for j in &mut a.jobs {
            for f in &mut j.result.formats {
                f.stock_ref = "S2".into();
            }
        }
        let snap = a.snapshot();
        a.undo.push(snap.clone());
        a.redo.push(snap);
        a.use_library(std::path::Path::new("/srv/Nesting"), &mut |_| Ok(())).unwrap();
        assert!(a.result.formats.iter().chain(a.jobs.iter().flat_map(|j| &j.result.formats)).all(|f| f.stock_ref.is_empty() && f.made.is_empty()), "a sheet kept the old Library's stock");
        assert!(a.undo.is_empty() && a.redo.is_empty(), "undo could bring the old Library's stock back");
    }

    /// SH14: a join refused because this PC is in another shop, or because
    /// its own shop folder can't be reached, opens the question about that
    /// folder (Settings → Shop's Join a shop… leads here too).
    #[test]
    fn a_join_that_would_leave_a_shop_asks_first() {
        let mut a = crate::App::new(None);
        let mut fr = FirstRun::join_from_settings();
        fr.folder = "/srv/Nesting".into();
        for name in [Some("Other".to_string()), None] {
            let r = crate::engine::shop::Refused { why: "Not joined".into(), checks: vec![], leaving: Some((name.clone(), "/srv/Other".into())) };
            let typed = fr.folder.clone();
            a.first_run_answer(&mut fr, Answer::Refused(r, typed));
            assert_eq!(fr.asked(), Some(&Asked::Leave { name, from: "/srv/Other".into() }));
            assert!(fr.refused.is_none(), "the agents' wording was shown to a person");
        }
    }

    /// SH9: each choice has a title and one short line.
    #[test]
    fn each_choice_has_one_short_line() {
        for c in [Choice::Start, Choice::Join, Choice::ThisPc] {
            let (head, line) = c.words();
            assert!(!head.is_empty() && line.len() < 100 && line.matches(". ").count() <= 1, "{head}: {line}");
        }
    }
}
