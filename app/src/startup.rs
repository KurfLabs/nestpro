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

/// What a start or join running in the background answers.
pub enum Answer {
    Joined(crate::engine::shop::Joined),
    Refused(crate::engine::shop::Refused),
    Started { folder: std::path::PathBuf, name: String, unrecorded: Option<String> },
    /// SH16: the folder picked to start a shop in is there but holds no
    /// Library: asked before one is made.
    NoLibrary(std::path::PathBuf),
    Failed(String),
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
    /// SH16: the folder holding no Library that Start a shop asks about.
    pub ask_library: Option<std::path::PathBuf>,
    /// Done: what was set up, shown with Start working.
    pub done: Option<String>,
}

impl FirstRun {
    pub fn join_from_settings() -> FirstRun {
        FirstRun { pick: Some(Choice::Join), from_settings: true, ..Default::default() }
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
                    fr.ask_library = None;
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
                    ui.horizontal(|ui| {
                        if ui.add_enabled_ui(!busy && !fr.folder.trim().is_empty(), |ui| ui.button("Join the shop")).inner.clicked() {
                            self.first_run_join(&mut fr);
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
                    if let Some(d) = fr.ask_library.clone() {
                        ui.label(RichText::new(format!("{} holds no Library yet. Make a new, empty Library there and start the shop in it? Every PC that joins then uses that folder.", d.display())).color(t.text));
                        if ui.add_enabled_ui(ready, |ui| ui.button("Make a Library there and start the shop")).inner.clicked() {
                            self.first_run_start(&mut fr, true);
                        }
                    } else if ui.add_enabled_ui(ready, |ui| ui.button("Start the shop")).inner.clicked() {
                        self.first_run_start(&mut fr, false);
                    }
                }
                Some(Choice::ThisPc) => {
                    let lib = crate::library::default_dir();
                    ui.label(RichText::new(format!("The Library goes in {} on this PC.", lib.display())).color(t.text));
                    ui.add_space(6.0);
                    if ui.button("Use this PC only").clicked() {
                        match crate::library::create(&lib) {
                            Ok(()) => {
                                self.cfg.library_dir.clear();
                                self.first_run_save();
                                self.library_folder_changed();
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

    /// Join a shop, in the background (SH10, SH11).
    fn first_run_join(&mut self, fr: &mut FirstRun) {
        let (tx, rx) = std::sync::mpsc::channel();
        let (typed, current, who, clock) = (fr.folder.clone(), self.library_dir(), self.who.clone(), self.clock.clone());
        std::thread::spawn(move || {
            let a = match crate::engine::shop::join(&typed, &current, &who, &clock, crate::updates::VERSION) {
                Ok(j) => Answer::Joined(j),
                Err(r) => Answer::Refused(r),
            };
            let _ = tx.send(a);
        });
        fr.checks.clear();
        fr.refused = None;
        fr.pending = Some(rx);
    }

    /// Start a shop, in the background (SH1, SH16): in the folder picked,
    /// named on its share (SH13). A folder there with no Library is asked
    /// about first; `make`: the person said yes.
    fn first_run_start(&mut self, fr: &mut FirstRun, make: bool) {
        let (tx, rx) = std::sync::mpsc::channel();
        let (typed, name, who, clock) = (fr.folder.clone(), fr.name.trim().to_string(), self.who.clone(), self.clock.clone());
        std::thread::spawn(move || {
            let (full, _) = crate::shop::full_folder(&crate::shop::tidy_folder(&typed, cfg!(windows)));
            let dir = std::path::PathBuf::from(full);
            let a = if !dir.is_dir() {
                // Never made in place of a share that's down (SH8, LB11).
                Answer::Failed(format!("{} can't be reached. Check the folder's name, and that this PC is on the network. Nothing was made.", dir.display()))
            } else if !crate::library::holds_library(&dir) && !make {
                Answer::NoLibrary(dir)
            } else {
                match crate::library::create(&dir).and_then(|()| crate::engine::shop::start_shop(&dir, &name, &who, &clock, crate::updates::VERSION)) {
                    Ok(st) => Answer::Started { folder: dir, name: st.shop.name, unrecorded: st.unrecorded },
                    Err(e) => Answer::Failed(e),
                }
            };
            let _ = tx.send(a);
        });
        fr.refused = None;
        fr.pending = Some(rx);
    }

    /// Take in what a start or join answered: on success this PC's Library
    /// is the shop folder from now on (SH10).
    fn first_run_answer(&mut self, fr: &mut FirstRun, a: Answer) {
        if matches!(a, Answer::Joined(_) | Answer::Started { .. }) {
            // S2: a PC in a shop never takes a public release: whatever was
            // offered or chosen from them is withdrawn.
            self.setui.upd.withdraw(&crate::updates::HANDOFF);
        }
        match a {
            Answer::Joined(j) => {
                if !j.already {
                    self.cfg.library_dir = j.folder.display().to_string();
                    self.first_run_save();
                    self.library_folder_changed();
                }
                self.setui.shop = None;
                self.setui.shop_unrecorded = j.unrecorded.clone();
                fr.done = Some(j.summary());
                fr.checks = j.checks;
            }
            Answer::Refused(r) => {
                fr.refused = Some(r.why);
                fr.checks = r.checks;
            }
            Answer::Started { folder, name, unrecorded } => {
                self.cfg.library_dir = folder.display().to_string();
                self.first_run_save();
                self.library_folder_changed();
                self.setui.shop = None;
                self.setui.shop_unrecorded = unrecorded;
                fr.done = Some(format!("Started shop {name} in {}. This PC is its admin PC; other PCs join it with Join a shop, or `nestpro setup --join {}`.", folder.display(), folder.display()));
            }
            Answer::NoLibrary(d) => fr.ask_library = Some(d),
            Answer::Failed(e) => fr.refused = Some(e),
        }
    }

    /// The choice is kept, so the first-run screen never shows again; a
    /// screenshot run never saves the settings.
    fn first_run_save(&mut self) {
        if !self.shot_mode_now() {
            if let Err(e) = self.cfg.save() {
                self.status = format!("This PC's settings couldn't be saved ({e}).");
            }
        }
    }

    /// `--first-run[=join]` (screenshot runs only): the first-run screen;
    /// `join`: joined to a made-up shop Acme in a temporary folder of its
    /// own, with each check shown. Never a real Library or shop.
    pub fn demo_first_run(&mut self, what: &str) {
        if !self.shot_mode_now() {
            return;
        }
        let mut fr = FirstRun::default();
        if what == "join" {
            use crate::shared::{Clock, Who};
            let Some(dir) = crate::settings_ui::new_temp_folder("nestpro-first-run-demo").map(|d| d.join("Nesting")) else { return };
            if crate::library::create(&dir).is_err() {
                return;
            }
            let now = self.clock.now();
            let _ = crate::engine::shop::start(&dir, "Acme", &Who::new("sam", "PC-A"), &Clock::at(now - 40 * 86400), crate::updates::VERSION);
            self.who = Who::new("kim", "PC-B");
            fr.pick = Some(Choice::Join);
            fr.folder = dir.display().to_string();
            let a = match crate::engine::shop::join(&fr.folder, &self.library_dir(), &self.who, &self.clock, crate::updates::VERSION) {
                Ok(j) => Answer::Joined(j),
                Err(r) => Answer::Refused(r),
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
        let d = std::env::temp_dir().join(format!("nestpro_first_run_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
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

    /// SH9: each choice has a title and one short line.
    #[test]
    fn each_choice_has_one_short_line() {
        for c in [Choice::Start, Choice::Join, Choice::ThisPc] {
            let (head, line) = c.words();
            assert!(!head.is_empty() && line.len() < 100 && line.matches(". ").count() <= 1, "{head}: {line}");
        }
    }
}
