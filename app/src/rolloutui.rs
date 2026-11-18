//! Updates in a shop, the window's side (spec #178, ADR 0006, UP15–UP22):
//! what this PC installs from the shop folder and its banner ("Update ready,
//! restart when you're done"), the trial banner with Stop trying, and on the
//! admin PC the Updates card's Check GitHub, Try on this PC and Roll out to
//! the shop, with the warning before it. The rules are engine::rollout and
//! engine::updates; the installer's calls are crate::updates. Agents read
//! the same with `shop` (rolling out is refused for them).

use crate::engine::rollout::{self, TrialNow};
use crate::engine::updates::{FromShop, Offer, Release};
use crate::settings_ui::{card, page_button, row, Fetch};
use crate::theme::th;
use crate::updates::{self, Chosen, Source, Why};
use crate::App;
use eframe::egui;
use egui::{RichText, Stroke};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

/// What the window knows about this PC's updates in a shop.
#[derive(Default)]
pub struct RolloutUi {
    /// The Library this was looked at for: another one starts again.
    pub library: Option<PathBuf>,
    /// UP21: what this PC installs from its shop, at the last look.
    pub from: Option<Result<FromShop, String>>,
    /// Look at the shop version again at the next frame: when the program
    /// opens and at the Library's quick check (UP21).
    pub look: bool,
    /// The trial was looked at for this Library (once, when it opens).
    pub trial_looked: bool,
    /// The release chosen to install when the program closes (the shop's
    /// or a trial's), and why.
    pub fetch: Fetch,
    pub fetch_why: Why,
    /// UP15: the public releases a person on the admin PC asked for.
    pub checking: Option<Receiver<Result<Vec<Release>, String>>>,
    pub releases: Option<Result<Vec<Release>, String>>,
    /// A trial or a roll out being made ready in the background: what it
    /// says while it runs, and its answer.
    pub busy: Option<(String, Receiver<Result<Step, String>>)>,
    /// UP20: the warning before rolling out, open.
    pub warn: Option<WarnAsk>,
    /// What the last step said (Settings → Shop → Updates and the banner).
    pub note: Option<(bool, String)>,
    /// Where this PC keeps fetched releases and trial copies, and its
    /// settings file: None for the usual places (a test gives its own).
    pub local: Option<PathBuf>,
    pub settings_file: Option<PathBuf>,
}

/// A step done in the background.
pub enum Step {
    /// UP16: a trial begun, on this copy, of this release.
    Trial(rollout::Trial),
    /// UP20: the release this PC runs is ready to roll out: the warning.
    Ready(WarnAsk),
    /// UP19: rolled out.
    RolledOut(rollout::RolledOut),
}

/// UP20: the warning before Roll out, and what it is about.
#[derive(Clone)]
pub struct WarnAsk {
    pub version: String,
    pub shop: PathBuf,
    pub name: String,
    pub release: PathBuf,
    pub lines: Vec<String>,
}

/// Decision on #185: why Roll out waits while jobs from the trial are open.
pub const CLOSE_TRIAL_JOBS: &str = "Close your jobs from the trial first: saved after the roll out, they would go into the shop.";

/// UP16, S4: Try on this PC's steps, off the window's thread. A shop with no
/// shop version yet first gets the release this PC runs as its shop version
/// (fetched and rolled out, under the lock), so Stop trying has a release to
/// go back to; then the tried release is fetched, and the copy made.
#[allow(clippy::too_many_arguments)]
pub fn trial_steps(src: &Source, shop: &Path, releases: &Path, root: &Path, version: &str, who: &crate::shared::Who, clock: &crate::shared::Clock, own: &crate::backup::Settings) -> Result<rollout::Trial, String> {
    let s = crate::shop::read(shop)?.ok_or_else(|| format!("{} isn't a shop. Nothing was tried.", shop.display()))?;
    if s.shop_version().is_none() {
        let mine = releases.join(updates::VERSION);
        updates::fetch_release(src, updates::VERSION, &mine)
            .and_then(|()| rollout::rollout(shop, &mine, who, updates::VERSION).map(|_| ()))
            .map_err(|e| format!("Nothing was tried: shop {} has no shop version yet, so this PC first rolls out the release it runs ({}), for the trial to go back to, and that didn't work. {e}", s.name, updates::VERSION))?;
    }
    updates::fetch_release(src, version, &releases.join(version))?;
    rollout::start_trial(shop, root, version, who, clock, updates::VERSION, own)
}

impl App {
    /// The release folders fetched on this PC (UP16, UP19): beside its
    /// settings, or in the screenshot run's own folder.
    fn releases_dir(&self) -> PathBuf {
        if let Some(l) = &self.rollout.local {
            return l.join("Releases");
        }
        if self.shot_mode_now() {
            return crate::settings_ui::shop_demo_dir().and_then(|d| d.parent().map(|p| p.join("Releases"))).unwrap_or_default();
        }
        crate::settings::AppSettings::file().parent().map(|p| p.join("Releases")).unwrap_or_default()
    }

    /// Where trial copies go on this PC (UP16): `Documents\Sheet Nesting\Trials`.
    fn trials_dir(&self) -> PathBuf {
        if let Some(l) = &self.rollout.local {
            return l.join("Trials");
        }
        if self.shot_mode_now() {
            return crate::settings_ui::shop_demo_dir().and_then(|d| d.parent().map(|p| p.join("Trials"))).unwrap_or_default();
        }
        crate::library::documents_dir().join("Trials")
    }

    /// Where a release this PC fetches is downloaded from: a made-up one in
    /// screenshots and tests, else the folder.
    fn install_source(&self, folder: PathBuf) -> Source {
        match &self.setui.upd.source {
            Some(Source::Fake(f)) => Source::Fake(f.clone()),
            _ => Source::Folder(folder),
        }
    }

    /// The trial mark of this PC's Library, when it is a trial copy.
    pub fn trial_mark(&self) -> Option<crate::shop::TrialMark> {
        match &self.rollout.from {
            Some(Ok(FromShop::Trial(m))) => Some(m.clone()),
            Some(_) => None,
            // Not looked at yet: the Library's shop file says.
            None => crate::shop::read(&self.library_dir()).ok().flatten().and_then(|s| s.trial()),
        }
    }

    /// Decision on #185: Roll out is refused while a job from the trial
    /// copy is open (a Library job, or a job file in the copy), as saving
    /// it afterwards would carry it into the shop.
    pub fn trial_jobs_open(&self) -> bool {
        if self.trial_mark().is_none() {
            return false;
        }
        let copy = self.library_dir();
        self.job_lib_id.is_some() || self.jobs.iter().any(|j| j.lib_id.is_some() || j.file.as_ref().is_some_and(|f| crate::shared::inside(&f.path, &copy)))
    }

    /// The shop folder: the Library's, or in a trial the shop it was copied
    /// from.
    pub fn real_shop(&self) -> PathBuf {
        self.trial_mark().map_or_else(|| self.library_dir(), |m| PathBuf::from(m.shop_folder))
    }

    /// UP16, UP17: this PC's Library becomes `dir` (a trial copy, or the
    /// shop folder again). S3: saved in its settings first; when they can't
    /// be saved nothing changes, so the next start never opens another
    /// Library than the one this PC works in now.
    fn switch_library(&mut self, dir: &Path) -> Result<(), String> {
        let mut next = self.cfg.clone();
        next.library_dir = dir.display().to_string();
        if !self.shot_mode_now() {
            let f = self.rollout.settings_file.clone().unwrap_or_else(crate::settings::AppSettings::file);
            next.save_to(&f).map_err(|e| format!("This PC's settings ({}) couldn't be saved ({e}), so its Library stays {}.", f.display(), self.library_dir().display()))?;
            self.cfg_saved = next.clone();
        }
        self.cfg = next;
        self.library_folder_changed();
        self.setui.shop = None;
        self.drop_fetch();
        let r = &mut self.rollout;
        r.library = Some(dir.to_path_buf());
        r.from = None;
        r.look = true;
        r.trial_looked = true;
        Ok(())
    }

    /// Withdraw the shop's or a trial's release chosen to install (a PC of
    /// its own's public one is UpdateState's).
    fn drop_fetch(&mut self) {
        if !matches!(self.rollout.fetch, Fetch::No) {
            self.handoff.withdraw_for(self.rollout.fetch_why);
        }
        self.rollout.fetch = Fetch::No;
    }

    /// UP21, UP16: fetch `version` from `folder`, to install when the
    /// program closes (never before).
    fn choose_from(&mut self, version: &str, folder: PathBuf, why: Why) {
        let busy = match &self.rollout.fetch {
            Fetch::Running(v, _) | Fetch::Ready(v) => v == version && self.rollout.fetch_why == why,
            _ => false,
        };
        if busy {
            return;
        }
        let c = Chosen { version: version.to_string(), dir: self.library_dir(), own: crate::library::default_dir(), src: self.install_source(folder), why };
        self.rollout.fetch = Fetch::Running(version.to_string(), updates::download_in_background(self.handoff, c));
        self.rollout.fetch_why = why;
    }

    /// Every frame: look at the shop version when the program opens and at
    /// the Library's quick check (UP21); the trial when the Library opens
    /// (UP16, UP17); and pick up what runs in the background.
    pub fn shop_update_tick(&mut self, ctx: &egui::Context) {
        if self.setui.upd.source.is_none() {
            return;
        }
        let dir = self.library_dir();
        if self.rollout.library.as_deref() != Some(dir.as_path()) {
            self.drop_fetch();
            let r = &mut self.rollout;
            r.library = Some(dir.clone());
            r.from = None;
            r.look = true;
            r.trial_looked = false;
            r.releases = None;
        }
        if !self.rollout.trial_looked {
            self.rollout.trial_looked = true;
            self.look_at_trial(&dir, true);
        }
        if std::mem::take(&mut self.rollout.look) {
            self.look_at_shop(&dir);
        }
        self.poll_shop_fetch();
        if let Some(rx) = &self.rollout.checking {
            match rx.try_recv() {
                Ok(r) => {
                    self.rollout.releases = Some(r);
                    self.rollout.checking = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => {
                    self.rollout.releases = Some(Err(updates::COULDNT_CHECK.into()));
                    self.rollout.checking = None;
                }
            }
        }
        self.poll_busy();
        if self.rollout.checking.is_some() || self.rollout.busy.is_some() || matches!(self.rollout.fetch, Fetch::Running(..)) {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    /// UP21: what this PC installs from its shop, and fetching it.
    fn look_at_shop(&mut self, dir: &Path) {
        let from = crate::engine::updates::from_shop(dir, updates::VERSION);
        match &from {
            Ok(FromShop::Update { version, folder, .. }) => {
                let (v, f) = (version.clone(), folder.clone());
                if self.rollout.fetch_why != Why::Shop {
                    self.drop_fetch();
                }
                // A copy without the installer (Linux) fetches nothing: the
                // card says what the shop is on.
                if !matches!(self.setui.upd.source, Some(Source::Off(_))) {
                    self.choose_from(&v, f, Why::Shop);
                }
            }
            Ok(FromShop::Trial(_)) => {}
            // Up to date, no shop version, missing packages or no shop:
            // a shop release chosen before is withdrawn.
            _ if self.rollout.fetch_why == Why::Shop => self.drop_fetch(),
            _ => {}
        }
        self.rollout.from = Some(from);
    }

    /// S6: when the program opens, before any job is reopened: a trial the
    /// shop moved on from ends here, and the tried release's one-time
    /// changes are made on the copy before its jobs are read.
    pub fn resolve_trial_at_start(&mut self) {
        let dir = self.library_dir();
        self.look_at_trial(&dir, false);
    }

    /// UP16, UP17, UP18: the trial, when this Library opens. `arm`: fetch the
    /// tried release to install on close, when this PC doesn't run it yet.
    fn look_at_trial(&mut self, dir: &Path, arm: bool) {
        match rollout::trial_now(dir, updates::VERSION) {
            TrialNow::NotTrial => {}
            TrialNow::Over { mark, said } => {
                self.rollout.note = Some(match self.switch_library(Path::new(&mark.shop_folder)) {
                    Ok(()) => (true, said),
                    Err(e) => (false, format!("{said} But {e}")),
                });
            }
            TrialNow::ShopAway { said, .. } => self.rollout.note = Some((false, said)),
            TrialNow::Trying { installed: true, .. } => match rollout::trial_changes(dir, updates::VERSION) {
                Ok(made) if !made.is_empty() => self.rollout.note = Some((true, format!("Made this release's one-time changes on the trial copy: {}.", made.join("; ")))),
                Ok(_) => {}
                Err(e) => self.rollout.note = Some((false, e)),
            },
            TrialNow::Trying { installed: false, .. } if !arm => {}
            TrialNow::Trying { mark, installed: false } => {
                let folder = self.releases_dir().join(&mark.version);
                if rollout::pack_in(&folder, &mark.version).is_ok() || matches!(self.setui.upd.source, Some(Source::Fake(_))) {
                    self.choose_from(&mark.version, folder, Why::Trial);
                } else {
                    self.rollout.note = Some((false, format!("Version {} isn't on this PC any more: Stop trying, then Try on this PC again.", mark.version)));
                }
            }
        }
    }

    fn poll_shop_fetch(&mut self) {
        let Fetch::Running(v, rx) = &self.rollout.fetch else { return };
        match rx.try_recv() {
            Ok(Ok(())) => self.rollout.fetch = Fetch::Ready(v.clone()),
            Ok(Err(e)) => self.rollout.fetch = Fetch::Failed(v.clone(), e),
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(_) => self.rollout.fetch = Fetch::Failed(v.clone(), format!("Couldn't get version {v}.")),
        }
    }

    fn poll_busy(&mut self) {
        let Some((_, rx)) = &self.rollout.busy else { return };
        let got = match rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(_) => Err("It stopped before it was done. Nothing was changed.".into()),
        };
        self.rollout.busy = None;
        match got {
            Err(e) => self.rollout.note = Some((false, e)),
            Ok(Step::Ready(w)) => self.rollout.warn = Some(w),
            Ok(Step::Trial(t)) => self.trial_begun(t),
            Ok(Step::RolledOut(r)) => {
                let shop = self.real_shop();
                if self.trial_mark().is_some() {
                    if let Err(e) = self.switch_library(&shop) {
                        self.rollout.note = Some((false, format!("Rolled out {}, but this PC stays on its trial copy: {e} Point it back at {} in Settings → Folders.", r.version, shop.display())));
                        self.setui.shop = None;
                        return;
                    }
                } else {
                    self.rollout.look = true;
                }
                let prev = r.previous.as_ref().map_or(String::new(), |p| format!(" {p} is kept beside it for roll back."));
                let changes = if r.changes.is_empty() { String::new() } else { format!(" One-time changes made: {}.", r.changes.join("; ")) };
                self.rollout.note = Some((true, format!("Rolled out {}: it is the shop version now, and every PC installs it from the shop folder when its program closes.{prev}{changes}", r.version)));
                self.setui.shop = None;
            }
        }
    }

    /// UP15: Check GitHub, on the admin PC: a person's click.
    fn check_github(&mut self) {
        let Some(src) = self.setui.upd.source.clone() else { return };
        self.rollout.releases = None;
        self.rollout.checking = Some(updates::releases_in_background(src));
    }

    /// UP16: the trial's copy is made: this PC works on it from now on. S5:
    /// what this window still writes in the shop (adding this PC, its holds
    /// on jobs) is finished first; S3: the settings are saved before
    /// anything else changes, and the trial's release is fetched to install
    /// only once they are.
    pub fn trial_begun(&mut self, t: rollout::Trial) {
        let v = t.mark.version.clone();
        let kept = format!("The trial copy {} stays on this PC; Try on this PC again to use a new one.", t.copy.display());
        if let Err(e) = self.finish_shop_writers().and_then(|()| self.switch_library(&t.copy)) {
            self.rollout.note = Some((false, format!("Not trying {v}: {e} {kept}")));
            return;
        }
        // The copy is this PC's Library now: the trial's release installs
        // when the program closes.
        self.rollout.from = Some(Ok(FromShop::Trial(t.mark.clone())));
        if updates::VERSION != v {
            self.choose_from(&v, self.releases_dir().join(&v), Why::Trial);
        }
        self.rollout.note = Some((true, format!("This PC works on a copy of the shop now ({}), made from a fresh backup. Nothing you save here reaches the shop. Version {v} installs when the program closes.", t.copy.display())));
    }

    /// S5: finish what this window writes in the shop before a trial starts:
    /// a backup under way stops it (try again when it's done); adding this
    /// PC to the PCs list is waited for; this window's holds on jobs are let
    /// go now, not in the background, so nothing of the shop is written once
    /// the trial has begun.
    fn finish_shop_writers(&mut self) -> Result<(), String> {
        let wait = std::time::Duration::from_secs(10);
        if self.setui.backup_run.lock().is_ok_and(|r| r.since.is_some()) {
            return Err("A backup of the shop is being taken just now: try again when it is done.".into());
        }
        if let Some((rx, asked)) = self.setui.shop_adding.take() {
            match rx.recv_timeout(wait) {
                Ok(r) => {
                    let (unrecorded, note) = crate::settings_ui::added_note(&self.who.pc, r, asked);
                    self.setui.shop_unrecorded = unrecorded;
                    if note.is_some() {
                        self.setui.shop_note = note;
                    }
                }
                Err(_) => {
                    self.setui.shop_adding = Some((rx, asked));
                    return Err("This PC is still adding itself to the shop's PCs list: try again in a moment.".into());
                }
            }
        }
        self.release_holds_now(wait).map_err(|e| format!("This PC's holds on jobs in the shop couldn't be let go ({e}): try again."))
    }

    /// UP16: Try on this PC: fetch the release, back up the shop and make
    /// the copy, all in the background.
    fn try_release(&mut self, version: String) {
        let Some(src) = self.setui.upd.source.clone() else { return };
        if self.setui.backup_run.lock().is_ok_and(|r| r.since.is_some()) {
            self.rollout.note = Some((false, "A backup of the shop is being taken just now: try again when it is done.".into()));
            return;
        }
        let (shop, releases, root, who, clock, own) = (self.real_shop(), self.releases_dir(), self.trials_dir(), self.who.clone(), self.clock.clone(), self.cfg.backups.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        let v = version.clone();
        std::thread::spawn(move || {
            let _ = tx.send(trial_steps(&src, &shop, &releases, &root, &v, &who, &clock, &own).map(Step::Trial));
        });
        self.rollout.note = None;
        self.rollout.busy = Some((format!("Getting {version} ready to try: fetching it, backing up the shop and copying it to this PC…"), rx));
    }

    /// UP17: Stop trying: back to the shop folder at once (read-only until
    /// the shop version is back, UP21), the shop version installing when the
    /// program closes.
    fn stop_trying(&mut self) {
        let Some(m) = self.trial_mark() else { return };
        if let Err(e) = self.switch_library(Path::new(&m.shop_folder)) {
            self.rollout.note = Some((false, format!("Still trying {}: {e}", m.version)));
            return;
        }
        self.rollout.note = Some((true, format!("Stopped trying {}. This PC works on the shop again; the shop version installs when the program closes. The trial copy stays on this PC, untouched.", m.version)));
    }

    /// UP19, UP20: Roll out: make sure this PC's release is fetched, then
    /// show the warning.
    pub fn prepare_rollout(&mut self) {
        let Some(src) = self.setui.upd.source.clone() else { return };
        if self.trial_jobs_open() {
            self.rollout.note = Some((false, format!("{CLOSE_TRIAL_JOBS} Nothing was rolled out.")));
            return;
        }
        let v = updates::VERSION.to_string();
        let (shop, into) = (self.real_shop(), self.releases_dir().join(&v));
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let r = updates::fetch_release(&src, &v, &into).and_then(|()| {
                let st = crate::engine::shop::status(&shop)?.ok_or_else(|| format!("{} isn't a shop.", shop.display()))?;
                // S8: the shop version's own package, copied again.
                let lines = if st.shop.shop_version() == Some(v.as_str()) {
                    vec![format!("{v} is already the shop version: only its package in the shop folder is copied again, as it is missing or damaged there. Nothing else changes.")]
                } else {
                    rollout::warning(&st, &v, &rollout::layouts()).lines()
                };
                Ok(Step::Ready(WarnAsk { version: v.clone(), shop: shop.clone(), name: st.shop.name.clone(), release: into.clone(), lines }))
            });
            let _ = tx.send(r);
        });
        self.rollout.note = None;
        self.rollout.busy = Some((format!("Getting {} ready to roll out…", updates::VERSION), rx));
    }

    /// UP19: Yes on the warning: roll out, in the background.
    fn roll_out(&mut self, w: WarnAsk) {
        if self.trial_jobs_open() {
            self.rollout.note = Some((false, format!("{CLOSE_TRIAL_JOBS} Nothing was rolled out.")));
            return;
        }
        let who = self.who.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let v = w.version.clone();
        std::thread::spawn(move || {
            let _ = tx.send(rollout::rollout(&w.shop, &w.release, &who, &w.version).map(Step::RolledOut));
        });
        self.rollout.busy = Some((format!("Rolling out {v}…"), rx));
    }

    /// UP16, UP21: the banners: a trial, and what this PC installs from its
    /// shop.
    pub fn shop_update_banner(&mut self, ui: &mut egui::Ui) {
        let t = th();
        let fetch_line = |f: &Fetch| -> Option<(bool, String)> {
            match f {
                Fetch::No => None,
                Fetch::Running(v, _) => Some((true, format!("Getting version {v}…"))),
                Fetch::Ready(v) => Some((true, format!("Version {v} installs when you close the program."))),
                Fetch::Failed(_, e) => Some((false, e.clone())),
            }
        };
        if let Some(m) = self.trial_mark() {
            let mut stop = false;
            egui::Panel::top("trial_banner")
                .frame(egui::Frame::new().fill(t.warn_bg).stroke(Stroke::new(1.0, t.border)).inner_margin(egui::Margin::symmetric(10, 5)))
                .show_separator_line(false)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("🧪 Trying {} on a copy of the shop. Nothing you save here reaches the shop.", m.version)).strong().color(t.warn_fg))
                            .on_hover_text(format!("This PC works on a copy of shop folder {} made from a fresh backup. The other PCs keep the shop version.", m.shop_folder));
                        if updates::VERSION != m.version {
                            if let Some((ok, l)) = fetch_line(&self.rollout.fetch) {
                                ui.label(RichText::new(l).color(if ok { t.text } else { t.warn_fg }));
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            stop = ui.button("Stop trying").on_hover_text("Back to the shop folder and the shop version (it installs when the program closes). The trial copy stays on this PC.").clicked();
                        });
                    });
                });
            if stop {
                self.stop_trying();
            }
            return;
        }
        let Some(Ok(from)) = self.rollout.from.clone() else { return };
        let (text, good) = match (&from, &self.rollout.fetch) {
            (FromShop::Update { version, back: false, .. }, Fetch::Ready(_)) => (format!("✔ {}: version {version} installs from the shop folder when you close the program.", crate::engine::updates::UPDATE_READY), true),
            (FromShop::Update { version, back: true, .. }, f) => {
                let more = if matches!(f, Fetch::Ready(_)) { " It installs from the shop folder when you close the program." } else { "" };
                (format!("This PC runs {}, newer than the shop version {version}: the shop is read-only here. Restart to go back to {version}.{more}", updates::VERSION), false)
            }
            (FromShop::Update { version, .. }, Fetch::Running(..)) => (format!("Getting version {version} from the shop folder…"), true),
            (FromShop::Update { .. }, Fetch::Failed(_, e)) => (e.clone(), false),
            (FromShop::Update { .. }, _) => return,
            (FromShop::Missing { .. }, _) => {
                let admin = match &self.setui.shop {
                    Some(Ok(Some(st))) => st.shop.admin.pc.clone(),
                    _ => "the admin PC".into(),
                };
                (from.said(updates::VERSION, &admin), false)
            }
            _ => return,
        };
        egui::Panel::top("shop_update_banner")
            .frame(egui::Frame::new().fill(if good { t.accent_soft } else { t.warn_bg }).stroke(Stroke::new(1.0, t.accent_border)).inner_margin(egui::Margin::symmetric(10, 5)))
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(text).strong().color(if good { t.ok_fg } else { t.warn_fg }))
                        .on_hover_text("The admin PC rolled this version out to the shop. It installs only when you close the program, never while a job is open, and the program never closes by itself.");
                });
            });
    }

    /// UP15–UP17, UP19: Settings → Shop → Updates for a PC in a shop.
    pub fn shop_updates_card(&mut self, ui: &mut egui::Ui, st: &crate::engine::shop::Status) {
        let t = th();
        let s = &st.shop;
        let trial = self.trial_mark();
        // In a trial the copy's shop file says what the shop was when it was
        // copied: the shop version is the one the trial began on.
        let sv = trial.as_ref().map_or(s.shop_version().map(String::from), |m| m.shop_version.clone());
        let admin = s.admin.pc == self.who.pc;
        let this = updates::VERSION;
        let (mut check, mut try_v, mut whats, mut roll, mut stop) = (false, None, None, false, false);
        let busy = self.rollout.busy.as_ref().map(|(w, _)| w.clone());
        card(ui, "Updates", "The shop version, rolled out from the admin PC.", |ui| {
            row(ui, "Shop version", |ui| {
                match &sv {
                    Some(v) => ui.label(RichText::new(v).strong()),
                    None => ui.label(RichText::new("None rolled out yet").color(t.text_dim)),
                }
                .on_hover_text("The release every PC in the shop runs. Each PC installs it from the shop folder when its program closes; none reads the internet.");
            });
            row(ui, "This PC", |ui| {
                ui.label(RichText::new(this).strong());
                if let Some(Ok(f)) = &self.rollout.from {
                    let line = f.said(this, &s.admin.pc);
                    let short = match f {
                        FromShop::UpToDate(_) => "the shop version".to_string(),
                        FromShop::Update { back: false, .. } => crate::engine::updates::UPDATE_READY.to_lowercase(),
                        FromShop::Update { version, back: true, .. } => format!("restart to go back to {version}"),
                        FromShop::Trial(m) => format!("trying {}", m.version),
                        FromShop::Missing { .. } => "the shop version's packages are missing".into(),
                        _ => String::new(),
                    };
                    if !short.is_empty() {
                        ui.label(RichText::new(format!("({short})")).color(t.text_dim)).on_hover_text(line);
                    }
                }
            });
            if !admin {
                ui.label(RichText::new(format!("The admin PC ({}) tries releases and rolls them out.", s.admin.pc)).size(11.5).color(t.text_dim));
                return;
            }
            // UP15: the admin PC learns of a release by a person's click.
            row(ui, "Newest release", |ui| match (&self.setui.upd.source, &self.rollout.releases) {
                (Some(Source::Off(why)), _) => {
                    ui.label(RichText::new(*why).color(t.text_dim));
                }
                _ if self.rollout.checking.is_some() => {
                    ui.spinner();
                    ui.label(RichText::new("Checking GitHub…").color(t.text_dim));
                }
                (_, Some(Ok(l))) => match crate::engine::updates::newest(l) {
                    Some(r) => {
                        ui.label(RichText::new(&r.version).strong().color(t.accent_fg));
                        let newer = crate::shared::version_order(&r.version, this) == Some(std::cmp::Ordering::Greater);
                        let past_shop = sv.as_deref().is_none_or(|v| crate::shared::version_order(&r.version, v) == Some(std::cmp::Ordering::Greater));
                        if page_button(ui, "Shop", "What's new", true).clicked() {
                            whats = Some(r.clone());
                        }
                        if newer && past_shop && trial.is_none() && page_button(ui, "Shop", "Try on this PC", busy.is_none()).on_hover_text("Installs it on this PC only, when the program closes, and works on a copy of the shop made from a fresh backup: nothing you save reaches the shop. The shop version stays as it is.").clicked() {
                            try_v = Some(r.version.clone());
                        }
                    }
                    None => {
                        ui.label(RichText::new("No release is published yet").color(t.text_dim));
                    }
                },
                (_, Some(Err(e))) => {
                    ui.label(RichText::new(updates::COULDNT_CHECK).color(t.warn_fg)).on_hover_text(e);
                }
                _ => {
                    ui.label(RichText::new("Not checked").color(t.text_dim)).on_hover_text("The admin PC checks GitHub only when you ask it to; the other PCs never do.");
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let can_check = matches!(self.setui.upd.source, Some(Source::Github | Source::Fake(_)));
                if can_check {
                    check = page_button(ui, "Shop", "Check GitHub", self.rollout.checking.is_none()).on_hover_text("Looks for the newest published release, to try it on this PC first.").clicked();
                }
                // UP19: the release this PC runs, newer than the shop version.
                let newer = sv.as_deref().is_none_or(|v| crate::shared::version_order(this, v) == Some(std::cmp::Ordering::Greater));
                // S8: the shop version's package is missing in Updates: the
                // admin PC on it copies it again.
                let repair = sv.as_deref() == Some(this) && matches!(self.rollout.from, Some(Ok(FromShop::Missing { .. })));
                let newer = newer || repair;
                let trial_ready = trial.as_ref().is_none_or(|m| m.version == this);
                if newer && trial_ready && can_check {
                    roll = page_button(ui, "Shop", "Roll out to the shop…", busy.is_none()).on_hover_text(format!("Makes {this}, the release this PC runs, the shop version: every PC installs it from the shop folder when its program closes. Warns first.")).clicked();
                }
                if trial.is_some() {
                    stop = page_button(ui, "Shop", "Stop trying", busy.is_none()).on_hover_text("Back to the shop folder and the shop version (it installs when the program closes).").clicked();
                }
            });
            if let Some(b) = &busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(b).size(11.5).color(t.text_dim));
                });
            }
            if let Some((ok, n)) = &self.rollout.note {
                ui.label(RichText::new(n).size(11.5).color(if *ok { t.ok_fg } else { t.warn_fg }));
            }
        });
        if check {
            self.check_github();
        }
        if let Some(r) = whats {
            self.setui.upd.whats_new = Some(Offer { version: r.version.clone(), machine_changes: crate::engine::updates::machine_changes(&r.notes), notes: r.notes.clone() });
        }
        if let Some(v) = try_v {
            self.try_release(v);
        }
        if roll {
            self.prepare_rollout();
        }
        if stop {
            self.stop_trying();
        }
    }

    /// UP20: the warning before Roll out: the PCs behind, the layout change
    /// and the PCs ahead. It still lets the admin go ahead.
    pub fn rollout_window(&mut self, ctx: &egui::Context) {
        let Some(w) = self.rollout.warn.clone() else { return };
        let t = th();
        let (mut yes, mut no) = (false, false);
        let mut open = true;
        egui::Window::new(RichText::new(format!("Roll out {} to the shop?", w.version)).size(16.0).strong())
            .id(egui::Id::unique("rollout_question"))
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_max_width(540.0);
                ui.label(RichText::new(format!("{} becomes the shop version of {}. Every PC installs it from the shop folder when its program closes, never while a job is open.", w.version, w.name)).color(t.text_strong));
                ui.add_space(6.0);
                for l in &w.lines {
                    ui.label(RichText::new(format!("• {l}")).size(12.5).color(t.text));
                }
                ui.add_space(4.0);
                ui.label(RichText::new("The previous release stays in the shop folder for roll back.").size(11.5).color(t.text_dim));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    yes = ui.add(egui::Button::new(RichText::new("Roll out to the shop").color(egui::Color32::WHITE)).fill(t.accent).min_size(egui::Vec2::new(0.0, 30.0))).clicked();
                    no = ui.add(egui::Button::new("Cancel — keep the shop version").min_size(egui::Vec2::new(0.0, 30.0))).clicked();
                });
            });
        if yes {
            self.rollout.warn = None;
            self.roll_out(w);
        } else if no || !open {
            self.rollout.warn = None;
        }
    }

    /// `--rollout-demo=warning|ready|trial` (screenshot runs, with
    /// `--shop-demo` and `--fake-releases`): made-up shops on made-up PCs in
    /// the run's own folder. `warning`: this PC is the admin PC PC-A, on a
    /// newer release than the shop's, with the warning before Roll out open.
    /// `ready`: this PC (PC-B) has the shop version ready to install.
    /// `trial`: PC-A trying a release on a copy of the shop.
    pub fn demo_rollout(&mut self, what: &str) {
        use crate::shared::{Clock, Who};
        let Some(dir) = crate::settings_ui::shop_demo_dir() else { return };
        if !self.shot_mode_now() || self.library_dir() != dir || !matches!(self.setui.upd.source, Some(Source::Fake(_))) {
            return;
        }
        let v = updates::VERSION;
        let older = "0.0.9";
        let src = self.setui.upd.source.clone().unwrap_or(Source::Off(""));
        let sam = Who::new("sam", "PC-A");
        // A shop that rolled out `older` long ago: its shop file says so
        // (written by this program, as the shop's PCs have opened it since).
        let set = (|| -> Result<(), String> {
            let lock = crate::shared::WriteLock::take(&dir)?;
            let mut s = crate::shop::read(&dir)?.ok_or("no shop")?;
            s.other.insert(crate::shop::SHOP_VERSION.into(), serde_json::json!(older));
            let mut l = rollout::layouts();
            if what == "warning" {
                // The release saves jobs in a newer layout than the shop's.
                l.insert("jobs".into(), crate::jobfile::VERSION - 1);
            }
            s.other.insert(crate::shop::LAYOUTS.into(), serde_json::json!(l));
            crate::shop::write_judged(&dir, &s, &lock, v, &crate::shared::Versions::this(v))
        })();
        if let Err(e) = set {
            self.rollout.note = Some((false, e));
            return;
        }
        // Its packages, as that roll out left them in Updates.
        let _ = updates::fetch_release(&src, older, &rollout::release_dir(&dir, older));
        let now = self.clock.now();
        let _ = crate::engine::shop::record_pc(&dir, &Who::new("lee", "PC-C"), &Clock::at(now - 3600), older);
        // A release newer than this program, as PC-A runs it.
        let Some((a, b, _)) = crate::engine::updates::plain_version(v) else { return };
        let next = format!("{a}.{}.0", b + 1);
        match what {
            "ready" => {
                let rel = self.releases_dir().join(&next);
                let _ = updates::fetch_release(&src, &next, &rel);
                let _ = rollout::rollout(&dir, &rel, &sam, &next);
                let _ = crate::engine::shop::record_pc(&dir, &sam, &Clock::at(now - 600), &next);
            }
            "warning" => {
                self.who = sam.clone();
                let _ = crate::engine::shop::record_pc(&dir, &sam, &Clock::at(now), v);
                let _ = crate::engine::shop::record_pc(&dir, &Who::new("kim", "PC-B"), &Clock::at(now - 1200), older);
                self.prepare_rollout();
                if let Some((_, rx)) = self.rollout.busy.take() {
                    match rx.recv() {
                        Ok(Ok(Step::Ready(w))) => self.rollout.warn = Some(w),
                        Ok(Err(e)) => self.rollout.note = Some((false, e)),
                        _ => {}
                    }
                }
            }
            "trial" => {
                self.who = sam.clone();
                let _ = crate::engine::shop::record_pc(&dir, &sam, &Clock::at(now), older);
                let _ = crate::engine::shop::record_pc(&dir, &Who::new("kim", "PC-B"), &Clock::at(now - 1200), older);
                let rel = self.releases_dir().join(&next);
                let started = updates::fetch_release(&src, &next, &rel).and_then(|()| rollout::start_trial(&dir, &self.trials_dir(), &next, &sam, &self.clock, v, &self.cfg.backups));
                match started {
                    Ok(t) => {
                        let _ = self.switch_library(&t.copy);
                        self.rollout.from = Some(Ok(FromShop::Trial(t.mark.clone())));
                        self.choose_from(&next, rel, Why::Trial);
                        self.rollout.note = Some((true, format!("This PC works on a copy of the shop now, made from a fresh backup. Nothing you save here reaches the shop. Version {next} installs when the program closes.")));
                    }
                    Err(e) => self.rollout.note = Some((false, e)),
                }
            }
            _ => {}
        }
        self.setui.shop = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::{Clock, Who};
    use std::time::{Duration, Instant};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nestpro_rolloutui_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sam() -> Who {
        Who::new("sam", "PC-A")
    }

    /// Shop Acme in `d/Nesting`, PC-A its admin PC, `version` rolled out.
    fn acme(d: &Path, version: Option<&str>) -> PathBuf {
        let lib = d.join("Nesting");
        crate::library::create(&lib).unwrap();
        crate::engine::shop::start(&lib, "Acme", &sam(), &Clock::at(1_794_216_000), version.unwrap_or("0.0.1")).unwrap();
        if let Some(v) = version {
            rollout::rollout(&lib, &rollout::fake_release(d, v), &sam(), v).unwrap();
        }
        lib
    }

    /// The window on PC-A in `lib`, with a hand-off, folders, settings file
    /// and made-up release list of its own.
    fn admin_app(d: &Path, lib: &Path) -> App {
        let mut a = App::new(None);
        a.who = sam();
        a.cfg.library_dir = lib.display().to_string();
        a.handoff = Box::leak(Box::new(updates::Handoff::new()));
        a.rollout.local = Some(d.join("pc"));
        a.rollout.settings_file = Some(d.join("pc").join("settings.ini"));
        let list = d.join("releases.json");
        std::fs::write(&list, r#"[{"version": "9.0.0", "notes": ""}]"#).unwrap();
        a.setui.upd.source = Some(Source::Fake(list));
        a
    }

    fn src(a: &App) -> Source {
        a.setui.upd.source.clone().unwrap()
    }

    /// Try on this PC's steps, as its background thread runs them.
    fn try_now(a: &App, lib: &Path, v: &str) -> rollout::Trial {
        trial_steps(&src(a), lib, &a.releases_dir(), &a.trials_dir(), v, &a.who, &a.clock, &a.cfg.backups).unwrap()
    }

    /// S1: after Try, the next frames (the public update check sees the
    /// Library change) leave the trial's release chosen, and closing the
    /// program installs it.
    #[test]
    fn a_trial_release_still_installs_after_the_next_frames() {
        let d = tmp("s1");
        let lib = acme(&d, Some(updates::VERSION));
        let mut a = admin_app(&d, &lib);
        let ctx = egui::Context::default();
        a.update_tick(&ctx);
        a.shop_update_tick(&ctx);
        let t = try_now(&a, &lib, "9.0.0");
        a.trial_begun(t);
        let until = Instant::now() + Duration::from_secs(60);
        while !matches!(a.rollout.fetch, Fetch::Ready(_)) {
            a.update_tick(&ctx);
            a.shop_update_tick(&ctx);
            assert!(Instant::now() < until && !matches!(a.rollout.fetch, Fetch::Failed(..)), "never ready: {:?}", a.rollout.note);
            std::thread::sleep(Duration::from_millis(10));
        }
        for _ in 0..5 {
            a.update_tick(&ctx);
            a.shop_update_tick(&ctx);
        }
        let mut applied = None;
        let r = updates::finish_on_close(a.handoff, Duration::from_secs(5), |c| {
            applied = Some((c.version.clone(), c.why));
            Ok(())
        });
        assert_eq!(r, Ok(Some("9.0.0".into())));
        assert_eq!(applied, Some(("9.0.0".into(), Why::Trial)));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S3: when this PC's settings can't be saved, the trial doesn't begin:
    /// the Library stays the shop and nothing is chosen to install.
    #[test]
    fn a_trial_whose_settings_cant_be_saved_changes_nothing() {
        let d = tmp("s3");
        let lib = acme(&d, Some(updates::VERSION));
        let mut a = admin_app(&d, &lib);
        std::fs::write(d.join("not a folder"), "x").unwrap();
        a.rollout.settings_file = Some(d.join("not a folder").join("settings.ini"));
        let t = try_now(&a, &lib, "9.0.0");
        a.trial_begun(t);
        assert_eq!(a.library_dir(), lib);
        assert!(matches!(a.rollout.fetch, Fetch::No));
        assert_eq!(a.handoff.take_ready(Duration::ZERO), None);
        let note = a.rollout.note.clone().unwrap();
        assert!(!note.0 && note.1.contains("couldn't be saved") && note.1.contains("Not trying 9.0.0"), "{note:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S4: the first trial on a new shop (no shop version yet) makes the
    /// release this PC runs the shop version first, so Stop trying has a
    /// release to go back to.
    #[test]
    fn the_first_trial_on_a_new_shop_rolls_this_pcs_release_out_first() {
        let d = tmp("s4");
        let lib = acme(&d, None);
        let a = admin_app(&d, &lib);
        let t = try_now(&a, &lib, "9.0.0");
        let s = crate::shop::read(&lib).unwrap().unwrap();
        assert_eq!(s.shop_version(), Some(updates::VERSION));
        assert_eq!(t.mark.shop_version.as_deref(), Some(updates::VERSION));
        rollout::current_release_ok(&lib, updates::VERSION).unwrap();
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S5: this window's holds on jobs in the shop are let go before the
    /// trial begins, on this thread, not in the background afterwards.
    #[test]
    fn holds_in_the_shop_are_let_go_before_the_trial_begins() {
        let d = tmp("s5");
        let lib = acme(&d, Some(updates::VERSION));
        let mut a = admin_app(&d, &lib);
        let l = crate::library::Library::open(&lib, &a.who, &a.clock).unwrap();
        let id = l.save_job(&crate::jobfile::JobFile::new_job("DEMO-001", crate::engine::default_spec()), crate::library::SaveTo::New).unwrap().id;
        crate::engine::inuse::take(&l, id, "DEMO-001").unwrap();
        a.inuse.held.insert(id);
        let hold = crate::engine::inuse::path(&lib, id);
        assert!(hold.is_file());
        let t = try_now(&a, &lib, "9.0.0");
        let here = std::rc::Rc::new(std::cell::Cell::new(false));
        let h2 = here.clone();
        crate::shared::hooks::set(move |step| {
            if step == "hold let go" {
                h2.set(true);
            }
        });
        a.trial_begun(t);
        crate::shared::hooks::clear();
        assert!(here.get(), "the hold was let go in the background");
        assert!(!hold.exists(), "the hold is still there when the trial has begun");
        assert_ne!(a.library_dir(), lib);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S6: when the program opens on a trial the shop moved on from, the
    /// Library is the shop folder again before the active job is reopened:
    /// the job comes from the shop, not the copy.
    #[test]
    fn a_trial_ended_by_the_shop_reopens_the_active_job_from_the_shop() {
        let d = tmp("s6");
        let lib = acme(&d, Some(updates::VERSION));
        let l = crate::library::Library::open(&lib, &sam(), &Clock::at(1_794_216_000)).unwrap();
        let id = l.save_job(&crate::jobfile::JobFile::new_job("DEMO-001", crate::engine::default_spec()), crate::library::SaveTo::New).unwrap().id;
        let a0 = admin_app(&d, &lib);
        let t = try_now(&a0, &lib, "9.0.0");
        // The shop rolled 9.1.0 out meanwhile.
        rollout::rollout(&lib, &rollout::fake_release(&d, "9.1.0"), &sam(), "9.1.0").unwrap();
        let mut a = admin_app(&d, &t.copy);
        a.cfg.active_job = id.to_string();
        a.open_at_start();
        assert_eq!(a.library_dir(), lib, "still on the copy");
        assert_eq!(a.job_lib_id, Some(id), "the active job isn't the shop's");
        assert!(a.rollout.note.as_ref().is_some_and(|n| n.1.contains("rolled out 9.1.0")), "{:?}", a.rollout.note);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Decision on #185: Roll out waits while a job from the trial copy is
    /// open. S2: nothing exported in a trial lands in the shop folder.
    #[test]
    fn roll_out_waits_for_trial_jobs_and_a_trial_exports_nothing_into_the_shop() {
        let d = tmp("q2");
        let lib = acme(&d, Some(updates::VERSION));
        let mut a = admin_app(&d, &lib);
        let t = try_now(&a, &lib, "9.0.0");
        a.trial_begun(t);
        a.job_lib_id = Some(1);
        a.prepare_rollout();
        assert!(a.rollout.busy.is_none() && a.rollout.warn.is_none());
        assert!(a.rollout.note.as_ref().is_some_and(|n| n.1.starts_with(CLOSE_TRIAL_JOBS)), "{:?}", a.rollout.note);
        a.job_lib_id = None;
        a.cfg.export_dir = lib.join("Programs").display().to_string();
        a.export();
        assert!(a.status.contains("this PC is trying a release on a copy of the shop"), "{}", a.status);
        assert!(!lib.join("Programs").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S9: a PC on a release newer than the shop version starts no backup of
    /// the shop.
    #[test]
    fn a_pc_ahead_of_the_shop_starts_no_backup() {
        let d = tmp("s9");
        let lib = acme(&d, Some("0.0.1"));
        let a = admin_app(&d, &lib);
        a.start_backup(crate::backup::Why::Daily);
        assert!(a.setui.backup_run.lock().unwrap().since.is_none(), "a backup started on a PC ahead of the shop");
        a.start_backup(crate::backup::Why::Now);
        let r = a.setui.backup_run.lock().unwrap();
        assert!(r.since.is_none() && r.note.as_ref().is_some_and(|n| n.1.contains("No backup was taken")), "{:?}", r.note);
        drop(r);
        assert!(std::fs::read_dir(lib.join("Backups")).unwrap().next().is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
