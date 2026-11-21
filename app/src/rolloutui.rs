//! Updates in a shop, the window's side (spec #178, ADR 0006, UP15–UP22):
//! what this PC installs from the shop folder and its banner ("Update ready,
//! restart when you're done"), the trial banner with Stop trying, and on the
//! admin PC the Updates card's Check GitHub, Try on this PC, Roll out to
//! the shop and Roll back, each with the warning before it. The rules are engine::rollout and
//! engine::updates; the installer's calls are crate::updates. Agents read
//! the same with `shop` (rolling out and rolling back are refused for them).

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
    /// UP24: the warning before rolling back, open, about this shop folder.
    pub back: Option<(PathBuf, rollout::RollBackAsk)>,
    /// What the last step said (Settings → Shop → Updates and the banner).
    pub note: Option<(bool, String)>,
    /// Scroll Settings → Shop to the Updates card (screenshots).
    pub show_card: bool,
    /// Where this PC keeps fetched releases and trial copies, and its
    /// settings file: None for the usual places (a test gives its own).
    pub local: Option<PathBuf>,
    pub settings_file: Option<PathBuf>,
    /// N1: a roll out runs, from Yes on the warning until it is done: no
    /// job is opened meanwhile.
    pub rolling: bool,
    /// S8: on the admin PC on the shop version, why that version's package
    /// in Updates is missing or damaged (Roll out repairs it), and the
    /// package as it was when last checked (size, last change), so a quick
    /// check doesn't read it whole again.
    pub repair: Option<String>,
    pub repair_seen: Option<(String, u64, Option<std::time::SystemTime>)>,
    /// S8: when the shop version is looked at next on its own timer (this
    /// window's clock; 0: not set yet), and that look, running in the
    /// background: the Library it is for, and its answer.
    pub next_look: u64,
    pub shop_look: Option<(PathBuf, Receiver<Result<FromShop, String>>)>,
}

/// A step done in the background.
pub enum Step {
    /// UP16: a trial begun, on this copy, of this release.
    Trial(rollout::Trial),
    /// UP20: the release this PC runs is ready to roll out: the warning.
    Ready(WarnAsk),
    /// UP19: rolled out.
    RolledOut(rollout::RolledOut),
    /// UP24: the shop in this folder may roll back: the warning.
    BackReady(PathBuf, rollout::RollBackAsk),
    /// UP23: rolled back.
    RolledBack(rollout::RolledBack),
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

/// S8: how often an open program looks at the shop version by itself,
/// whatever the auto-refresh setting: one small read of the shop file, off
/// the window's thread, so a roll out made after it opened says "Update
/// ready" without waiting for the close.
pub const SHOP_LOOK_EVERY: u64 = 5 * 60;

/// N1: why no job opens while a roll out runs.
pub const ROLLING: &str = "Rolling out — wait until it's done.";

/// Decision on #185: why Roll out waits while jobs from the trial are open.
pub const CLOSE_TRIAL_JOBS: &str = "Close your jobs from the trial first: saved after the roll out, they would go into the shop.";

/// S3: why Stop trying waits while jobs from the trial are open.
pub const CLOSE_TRIAL_JOBS_STOP: &str = "Close your jobs from the trial first: saved after Stop trying, they would go into the shop with the trial's sheets.";

/// UP16, S4: Try on this PC's steps, off the window's thread. A shop with no
/// shop version yet first gets the release this PC runs as its shop version
/// (fetched and rolled out, under the lock), so Stop trying has a release to
/// go back to; then the tried release is fetched, and the copy made.
#[allow(clippy::too_many_arguments)]
pub fn trial_steps(src: &Source, shop: &Path, releases: &Path, root: &Path, version: &str, who: &crate::shared::Who, clock: &crate::shared::Clock, own: &crate::backup::Settings) -> Result<rollout::Trial, String> {
    // S4: where the copy goes is checked before anything is fetched or made.
    rollout::trial_place_ok(shop, root)?;
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

/// UP16 (S4): this PC's trial copies: `%LOCALAPPDATA%\nestpro\Trials` on
/// Windows (app data that stays on this PC), beside the settings elsewhere
/// (`~/.config/nestpro/Trials`). Never in `Documents\Sheet Nesting`, which
/// can be the shop folder itself. Trials begun in the old place
/// (`Documents\Sheet Nesting\Trials`) are still read and ended as usual:
/// this PC's settings name the copy it works on.
pub fn trials_root() -> PathBuf {
    let local = if cfg!(windows) && !cfg!(test) { std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()) } else { None };
    match local {
        Some(l) => PathBuf::from(l).join("nestpro").join("Trials"),
        None => crate::settings::AppSettings::file().parent().map(|p| p.join("Trials")).unwrap_or_default(),
    }
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

    /// Where trial copies go on this PC (UP16, S4): its own app data, never
    /// a Library folder (`trials_root`).
    fn trials_dir(&self) -> PathBuf {
        if let Some(l) = &self.rollout.local {
            return l.join("Trials");
        }
        if self.shot_mode_now() {
            return crate::settings_ui::shop_demo_dir().and_then(|d| d.parent().map(|p| p.join("Trials"))).unwrap_or_default();
        }
        trials_root()
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
        // S3: the open jobs' stock links and undo steps are the old
        // Library's (as when joining a shop).
        self.forget_old_library_stock();
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
        let dir = self.library_dir();
        // S3: the close looks at this Library's shop once more.
        self.handoff.set_place(&dir, &crate::library::default_dir());
        if self.setui.upd.source.is_none() {
            return;
        }
        if self.rollout.library.as_deref() != Some(dir.as_path()) {
            self.drop_fetch();
            let r = &mut self.rollout;
            r.library = Some(dir.clone());
            r.from = None;
            r.look = true;
            r.trial_looked = false;
            r.releases = None;
            r.shop_look = None;
        }
        if !self.rollout.trial_looked {
            self.rollout.trial_looked = true;
            self.look_at_trial(&dir, true);
        }
        if std::mem::take(&mut self.rollout.look) {
            self.look_at_shop(&dir);
            self.rollout.next_look = self.clock.now() + SHOP_LOOK_EVERY;
        }
        self.shop_look_timer(&dir);
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
        if self.rollout.checking.is_some() || self.rollout.busy.is_some() || self.rollout.shop_look.is_some() || matches!(self.rollout.fetch, Fetch::Running(..)) {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    /// S8: the shop version on its own timer (`SHOP_LOOK_EVERY`), read in
    /// the background; what it finds is taken in as the look at the quick
    /// check's is. Not in screenshots.
    fn shop_look_timer(&mut self, dir: &Path) {
        if let Some((d, rx)) = &self.rollout.shop_look {
            match rx.try_recv() {
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Ok(from) if d == dir => {
                    let dir = d.clone();
                    self.rollout.shop_look = None;
                    self.took_shop_look(&dir, from, false);
                }
                // For a Library this window left, or the look stopped.
                _ => self.rollout.shop_look = None,
            }
        }
        let now = self.clock.now();
        if self.shot.is_some() {
            return;
        }
        if self.rollout.next_look == 0 {
            self.rollout.next_look = now + SHOP_LOOK_EVERY;
            return;
        }
        if now < self.rollout.next_look || self.rollout.shop_look.is_some() {
            return;
        }
        self.rollout.next_look = now + SHOP_LOOK_EVERY;
        let (tx, rx) = std::sync::mpsc::channel();
        let d = dir.to_path_buf();
        if std::thread::Builder::new()
            .name("shop version look".into())
            .spawn(move || {
                let _ = tx.send(crate::engine::updates::from_shop(&d, updates::VERSION));
            })
            .is_ok()
        {
            self.rollout.shop_look = Some((dir.to_path_buf(), rx));
        }
    }

    /// UP21: what this PC installs from its shop, and fetching it.
    fn look_at_shop(&mut self, dir: &Path) {
        let from = crate::engine::updates::from_shop(dir, updates::VERSION);
        self.took_shop_look(dir, from, true);
    }

    /// UP21: take in what a look at the shop found. `package`: on the admin
    /// PC, check the shop version's package too (S8: not on the timer's
    /// looks, which read only the shop file).
    fn took_shop_look(&mut self, dir: &Path, from: Result<FromShop, String>, package: bool) {
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
            // S8: the admin PC on the shop version checks that version's
            // package in Updates, to offer the repair.
            Ok(FromShop::UpToDate(v)) => {
                let v = v.clone();
                if self.rollout.fetch_why == Why::Shop {
                    self.drop_fetch();
                }
                if package {
                    self.look_at_package(dir, &v);
                }
            }
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

    /// S8: on the admin PC, whether the shop version `v`'s package in
    /// Updates is whole: its size at every look, its checksum again only
    /// when the file changed since the last check.
    fn look_at_package(&mut self, dir: &Path, v: &str) {
        let admin = crate::shop::read(dir).ok().flatten().is_some_and(|s| s.admin.pc == self.who.pc);
        if !admin {
            self.rollout.repair = None;
            return;
        }
        let folder = rollout::release_dir(dir, v);
        let pack = match rollout::pack_in(&folder, v) {
            Ok(p) => p,
            Err(e) => {
                self.rollout.repair = Some(e);
                self.rollout.repair_seen = None;
                return;
            }
        };
        let m = std::fs::metadata(folder.join(&pack.package)).ok();
        let seen = (v.to_string(), m.as_ref().map_or(0, |m| m.len()), m.and_then(|m| m.modified().ok()));
        if self.rollout.repair_seen.as_ref() != Some(&seen) {
            self.rollout.repair = rollout::verify(&folder, &pack).err();
            self.rollout.repair_seen = Some(seen);
        }
    }

    /// S8: the release this PC runs is the shop version, and its package in
    /// Updates is missing or damaged: Roll out copies it again.
    pub fn repair_needed(&self) -> bool {
        self.rollout.repair.is_some() || matches!(self.rollout.from, Some(Ok(FromShop::Missing { ref version, .. })) if version == updates::VERSION)
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
        self.rollout.rolling = false;
        match got {
            Err(e) => self.rollout.note = Some((false, e)),
            Ok(Step::Ready(w)) => self.rollout.warn = Some(w),
            Ok(Step::BackReady(dir, ask)) => self.rollout.back = Some((dir, ask)),
            Ok(Step::RolledBack(r)) => {
                // UP21: this PC goes back too, when its program closes.
                self.rollout.look = true;
                self.rollout.note = Some((true, format!("Rolled back to {}: it is the shop version again, and every PC installs it from the shop folder when its program closes. {} stays in Updates, so it can be rolled out again.", r.to, r.from)));
                self.setui.shop = None;
            }
            Ok(Step::Trial(t)) => self.trial_begun(t),
            Ok(Step::RolledOut(r)) => {
                let shop = self.real_shop();
                // N1: never back to the shop with a job from the trial open.
                if self.trial_mark().is_some() && self.trial_jobs_open() {
                    self.rollout.note = Some((false, format!("Rolled out {}. This PC stays on its trial copy while jobs from it are open: close them and reopen the program, and it works on the shop again.", r.version)));
                    self.setui.shop = None;
                    return;
                }
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
        // S2: it backs the shop up: closing the program waits for it.
        let working = self.handoff.begin(updates::Work::Backup);
        std::thread::spawn(move || {
            let _working = working;
            let _ = tx.send(trial_steps(&src, &shop, &releases, &root, &v, &who, &clock, &own).map(Step::Trial));
        });
        self.rollout.note = None;
        self.rollout.busy = Some((format!("Getting {version} ready to try: fetching it, backing up the shop and copying it to this PC…"), rx));
    }

    /// UP17: Stop trying: back to the shop folder at once (read-only until
    /// the shop version is back, UP21), the shop version installing when the
    /// program closes. S3: not while a job from the trial is open (saved
    /// afterwards it would book the trial's sheets in the shop's stock); the
    /// other open jobs forget the trial's stock links and undo steps.
    pub fn stop_trying(&mut self) {
        let Some(m) = self.trial_mark() else { return };
        if self.trial_jobs_open() {
            let said = format!("{CLOSE_TRIAL_JOBS_STOP} Still trying {}.", m.version);
            // Said on the status line too: Stop trying is on the banner.
            self.status = said.clone();
            self.rollout.note = Some((false, said));
            return;
        }
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
        self.rollout.rolling = true;
        // S2: closing the program waits for it.
        let working = self.handoff.begin(updates::Work::RollOut);
        std::thread::spawn(move || {
            let _working = working;
            let _ = tx.send(rollout::rollout(&w.shop, &w.release, &who, &w.version).map(Step::RolledOut));
        });
        self.rollout.busy = Some((format!("Rolling out {v}…"), rx));
    }

    /// UP24: Roll back: look whether the shop may go back (in the
    /// background: the shop folder is on the share), then show the warning
    /// or say why not.
    fn prepare_rollback(&mut self) {
        let (shop, who) = (self.real_shop(), self.who.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(rollout::rollback_ask(&shop, &who, updates::VERSION).map(|a| Step::BackReady(shop.clone(), a)));
        });
        self.rollout.note = None;
        self.rollout.busy = Some(("Looking whether the shop can go back…".into(), rx));
    }

    /// UP23: Yes on the warning: roll back, in the background.
    fn roll_back(&mut self, shop: PathBuf, ask: rollout::RollBackAsk) {
        let who = self.who.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let to = ask.to.clone();
        // S2: closing the program waits for it.
        let working = self.handoff.begin(updates::Work::RollBack);
        // S4: only the roll back the person confirmed (from and to).
        std::thread::spawn(move || {
            let _working = working;
            let _ = tx.send(rollout::rollback_as_asked(&shop, &who, updates::VERSION, &ask).map(Step::RolledBack));
        });
        self.rollout.busy = Some((format!("Rolling back to {to}…"), rx));
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
        let (mut check, mut try_v, mut whats, mut roll, mut stop, mut back) = (false, None, None, false, false, false);
        let busy = self.rollout.busy.as_ref().map(|(w, _)| w.clone());
        if self.rollout.show_card {
            ui.scroll_to_cursor(Some(egui::Align::TOP));
            // A screenshot keeps it in view; a person can scroll on.
            self.rollout.show_card = self.shot_mode_now();
        }
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
                let repair = sv.as_deref() == Some(this) && self.repair_needed();
                let newer = newer || repair;
                let trial_ready = trial.as_ref().is_none_or(|m| m.version == this);
                if newer && trial_ready && can_check {
                    roll = page_button(ui, "Shop", "Roll out to the shop…", busy.is_none()).on_hover_text(format!("Makes {this}, the release this PC runs, the shop version: every PC installs it from the shop folder when its program closes. Warns first.")).clicked();
                }
                // UP23: back to the release kept beside the shop version.
                if let (Some(p), None, true) = (s.previous_version(), &trial, can_check) {
                    back = page_button(ui, "Shop", &format!("Roll back to {p}…"), busy.is_none()).on_hover_text(format!("Makes {p}, the release kept in the shop folder, the shop version again: every PC goes back to it when its program closes. Refused when the shop's files were saved in a layout {p} can't read. Warns first.")).clicked();
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
            // S8: the shop version's package needs copying again.
            if sv.as_deref() == Some(this) && self.repair_needed() {
                let why = self.rollout.repair.clone().unwrap_or_default();
                ui.label(RichText::new(format!("⚠ The shop version's package in the shop folder is missing or damaged: Roll out to the shop copies it again. {why}")).size(11.5).color(t.warn_fg));
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
        if back {
            self.prepare_rollback();
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

    /// UP24: the warning before Roll back. It still lets the admin go ahead.
    pub fn rollback_window(&mut self, ctx: &egui::Context) {
        let Some((shop, a)) = self.rollout.back.clone() else { return };
        let t = th();
        let (mut yes, mut no) = (false, false);
        let mut open = true;
        egui::Window::new(RichText::new(format!("Roll the shop back to {}?", a.to)).size(16.0).strong())
            .id(egui::Id::unique("rollback_question"))
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_max_width(540.0);
                ui.label(RichText::new(format!("{} becomes the shop version again, in place of {}. Every PC goes back to it from the shop folder when its program closes, never while a job is open.", a.to, a.from)).color(t.text_strong));
                ui.add_space(6.0);
                for (i, l) in a.lines.iter().enumerate() {
                    // The first line is what may be lost: it stands out.
                    let (size, colour) = if i == 0 { (13.0, t.warn_fg) } else { (12.5, t.text) };
                    ui.label(RichText::new(format!("• {l}")).size(size).color(colour));
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    yes = ui.add(egui::Button::new(RichText::new(format!("Roll back to {}", a.to)).color(egui::Color32::WHITE)).fill(t.accent).min_size(egui::Vec2::new(0.0, 30.0))).clicked();
                    no = ui.add(egui::Button::new(format!("Cancel — keep {}", a.from)).min_size(egui::Vec2::new(0.0, 30.0))).clicked();
                });
            });
        if yes {
            self.rollout.back = None;
            self.roll_back(shop, a);
        } else if no || !open {
            self.rollout.back = None;
        }
    }

    /// `--rollout-demo=warning|ready|trial|rollback|rollback-refused` (screenshot runs, with
    /// `--shop-demo` and `--fake-releases`): made-up shops on made-up PCs in
    /// the run's own folder. `warning`: this PC is the admin PC PC-A, on a
    /// newer release than the shop's, with the warning before Roll out open.
    /// `ready`: this PC (PC-B) has the shop version ready to install.
    /// `trial`: PC-A trying a release on a copy of the shop. `rollback`: the
    /// admin PC PC-A, on the shop version, with an older release kept for
    /// roll back and the warning before Roll back open; `rollback-refused`:
    /// the same, but the shop's jobs are in a layout the release kept can't
    /// read, and Roll back says so.
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
            if what.starts_with("rollback") {
                // UP23: the shop is on this program's release, `older` kept
                // for roll back (it wrote older jobs when refused).
                let mut kept = rollout::layouts();
                if what == "rollback-refused" {
                    kept.insert("jobs".into(), crate::jobfile::VERSION - 1);
                }
                s.other.insert(crate::shop::SHOP_VERSION.into(), serde_json::json!(v));
                s.other.insert(crate::shop::PREVIOUS_VERSION.into(), serde_json::json!(older));
                s.other.insert(crate::shop::RELEASE_LAYOUTS.into(), serde_json::json!(std::collections::BTreeMap::from([(older.to_string(), kept), (v.to_string(), rollout::layouts())])));
            }
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
            "rollback" | "rollback-refused" => {
                self.who = sam.clone();
                self.rollout.show_card = true;
                // The shop version's own packages, as its roll out left them.
                let _ = updates::fetch_release(&src, v, &rollout::release_dir(&dir, v));
                let _ = crate::engine::shop::record_pc(&dir, &sam, &Clock::at(now), v);
                let _ = crate::engine::shop::record_pc(&dir, &Who::new("kim", "PC-B"), &Clock::at(now - 1200), v);
                self.prepare_rollback();
                if let Some((_, rx)) = self.rollout.busy.take() {
                    match rx.recv() {
                        Ok(Ok(Step::BackReady(d, a))) => self.rollout.back = Some((d, a)),
                        Ok(Err(e)) => self.rollout.note = Some((false, e)),
                        _ => {}
                    }
                }
            }
            "trial" | "trial-stop" => {
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
                        // S3: Stop trying with a job from the trial open.
                        if what == "trial-stop" {
                            self.rollout.show_card = true;
                            self.job_lib_id = Some(1);
                            self.stop_trying();
                        }
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

    fn tmp(name: &str) -> crate::testdir::TestDir {
        let d = crate::testdir::TestDir::made(&format!("rolloutui_{name}"));
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

    /// S3: Stop trying waits while a job from the trial is open, as Roll out
    /// does; once they are closed, the jobs still open forget the trial's
    /// stock (sheets taken, remnants stored) and their undo steps, so a
    /// later save never gives the shop's stock sheets booked only on the
    /// trial copy.
    #[test]
    fn stop_trying_waits_for_trial_jobs_and_forgets_the_trials_stock() {
        let d = tmp("stop_trying");
        let lib = acme(&d, Some(updates::VERSION));
        let mut a = admin_app(&d, &lib);
        let t = try_now(&a, &lib, "9.0.0");
        a.trial_begun(t);
        let copy = a.library_dir();
        assert_ne!(copy, lib);
        a.job_lib_id = Some(1);
        a.stop_trying();
        assert_eq!(a.library_dir(), copy, "Stop trying left with a job from the trial open");
        assert!(a.rollout.note.as_ref().is_some_and(|n| !n.0 && n.1.starts_with("Close your jobs from the trial first")), "{:?}", a.rollout.note);
        a.job_lib_id = None;
        // A job (its file outside the copy) that took a sheet from the
        // trial's stock and stored a remnant there.
        if a.result.formats.is_empty() {
            a.result.formats.push(crate::nest::SheetFormat::rect(2000.0, 1000.0));
        }
        for f in &mut a.result.formats {
            f.stock_ref = "S1".into();
            f.made = vec!["R1".into()];
        }
        a.held.insert("S1".into(), 1);
        let snap = a.snapshot();
        a.undo.push(snap);
        a.stop_trying();
        assert_eq!(a.library_dir(), lib, "{:?}", a.rollout.note);
        assert!(a.held.is_empty() && a.result.formats.iter().all(|f| f.stock_ref.is_empty() && f.made.is_empty()), "a job kept the trial's stock");
        assert!(a.undo.is_empty() && a.redo.is_empty(), "undo could bring the trial's stock back");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S2: Roll out and Roll back hold the close while they write the shop
    /// folder: closing then waits ("Rolling out — the program closes when
    /// it's done"), and the update hand-off too.
    #[test]
    fn closing_waits_while_a_roll_out_or_roll_back_writes_the_shop() {
        let d = tmp("close_rolling");
        let lib = acme(&d, Some("0.0.9"));
        let mut a = admin_app(&d, &lib);
        let answer = |a: &mut App| a.rollout.busy.take().map(|(_, rx)| rx.recv_timeout(Duration::from_secs(120)).expect("no answer"));
        a.prepare_rollout();
        let Some(Ok(Step::Ready(w))) = answer(&mut a) else { panic!("{:?}", a.rollout.note) };
        // Another writer holds the shop's lock: the roll out waits for it.
        let lock = crate::shared::WriteLock::take(&lib).unwrap();
        a.roll_out(w);
        assert_eq!(a.handoff.working(), Some(updates::Work::RollOut), "closing wouldn't wait for the roll out");
        drop(lock);
        assert!(matches!(answer(&mut a), Some(Ok(Step::RolledOut(_)))));
        a.handoff.wait_for_work();
        assert_eq!(a.handoff.working(), None);
        a.prepare_rollback();
        let Some(Ok(Step::BackReady(shop, ask))) = answer(&mut a) else { panic!("{:?}", a.rollout.note) };
        let lock = crate::shared::WriteLock::take(&lib).unwrap();
        a.roll_back(shop, ask);
        assert_eq!(a.handoff.working(), Some(updates::Work::RollBack), "closing wouldn't wait for the roll back");
        drop(lock);
        let r = answer(&mut a);
        assert!(matches!(r, Some(Ok(Step::RolledBack(_)))), "{:?}", r.map(|r| r.err()));
        a.handoff.wait_for_work();
        assert_eq!(a.handoff.working(), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S8: a PC left open with auto-refresh off (the default) finds a roll
    /// out made after it opened on the shop version's own timer, and gets
    /// its "Update ready" before it closes.
    #[test]
    fn a_roll_out_after_the_program_opened_is_found_without_auto_refresh() {
        let d = tmp("s8_timer");
        let lib = acme(&d, Some(updates::VERSION));
        let mut a = admin_app(&d, &lib);
        a.who = Who::new("kim", "PC-B");
        a.clock = Clock::at(1_794_220_000);
        a.cfg.refresh_minutes = 0;
        let ctx = egui::Context::default();
        a.shop_update_tick(&ctx);
        assert!(matches!(a.rollout.from, Some(Ok(FromShop::UpToDate(_)))), "{:?}", a.rollout.from);
        rollout::rollout(&lib, &rollout::fake_release(&d, "9.0.0"), &sam(), "9.0.0").unwrap();
        a.clock.advance(Duration::from_secs(60));
        a.refresh_tick(false);
        a.shop_update_tick(&ctx);
        assert!(matches!(a.rollout.from, Some(Ok(FromShop::UpToDate(_)))), "looked before its time");
        a.clock.advance(Duration::from_secs(SHOP_LOOK_EVERY));
        let until = Instant::now() + Duration::from_secs(60);
        while !matches!(a.rollout.fetch, Fetch::Ready(_)) {
            a.shop_update_tick(&ctx);
            assert!(Instant::now() < until, "never found the roll out: {:?}", a.rollout.from);
            std::thread::yield_now();
        }
        assert!(matches!(&a.rollout.from, Some(Ok(FromShop::Update { version, .. })) if version == "9.0.0"), "{:?}", a.rollout.from);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S4: Try on this PC refuses a trial folder inside the shop before it
    /// fetches, rolls out or backs up anything.
    #[test]
    fn a_trial_folder_inside_the_shop_is_refused_before_anything_is_made() {
        let d = tmp("trial_place");
        let lib = acme(&d, None);
        let a = admin_app(&d, &lib);
        let e = trial_steps(&src(&a), &lib, &a.releases_dir(), &lib.join("Trials"), "9.0.0", &a.who, &a.clock, &a.cfg.backups).unwrap_err();
        assert!(e.contains("inside the shop folder"), "{e}");
        assert_eq!(crate::shop::read(&lib).unwrap().unwrap().shop_version(), None, "the shop version was rolled out for a refused trial");
        assert!(!a.releases_dir().exists() && !lib.join("Trials").exists());
        assert!(!crate::shared::inside(&trials_root(), &crate::library::documents_dir()), "{}", trials_root().display());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S8: the admin PC on the shop version finds its package damaged in
    /// Updates at the next look, and the repair is offered; once copied
    /// again, it isn't.
    #[test]
    fn the_admin_pc_is_offered_the_repair_of_a_damaged_package() {
        let d = tmp("s8");
        let lib = acme(&d, Some(updates::VERSION));
        let mut a = admin_app(&d, &lib);
        let ctx = egui::Context::default();
        a.shop_update_tick(&ctx);
        assert!(matches!(a.rollout.from, Some(Ok(FromShop::UpToDate(_)))));
        assert!(!a.repair_needed());
        let pkg = rollout::release_dir(&lib, updates::VERSION).join(format!("KurfLabs.nestpro-{}-full.nupkg", updates::VERSION));
        // Damaged, its size kept: only the checksum tells.
        let len = std::fs::metadata(&pkg).unwrap().len() as usize;
        std::fs::write(&pkg, "X".repeat(len)).unwrap();
        // A file system that keeps times to the second still tells it changed.
        std::fs::File::options().write(true).open(&pkg).unwrap().set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1_000_000)).unwrap();
        a.rollout.look = true;
        a.shop_update_tick(&ctx);
        assert!(a.repair_needed(), "no repair offered");
        rollout::rollout(&lib, &rollout::fake_release(&d, updates::VERSION), &sam(), updates::VERSION).unwrap();
        a.rollout.look = true;
        a.shop_update_tick(&ctx);
        assert!(!a.repair_needed());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// N1: from Yes on the warning until the roll out is done, no job is
    /// opened; completing it with a trial job open keeps the PC on the copy.
    #[test]
    fn no_job_opens_while_a_roll_out_runs() {
        let d = tmp("n1");
        let lib = acme(&d, Some(updates::VERSION));
        let l = crate::library::Library::open(&lib, &sam(), &Clock::at(1_794_216_000)).unwrap();
        let id = l.save_job(&crate::jobfile::JobFile::new_job("DEMO-001", crate::engine::default_spec()), crate::library::SaveTo::New).unwrap().id;
        let mut a = admin_app(&d, &lib);
        let t = try_now(&a, &lib, "9.0.0");
        let copy = t.copy.clone();
        a.trial_begun(t);
        a.roll_out(WarnAsk { version: "9.0.0".into(), shop: lib.clone(), name: "Acme".into(), release: d.join("nowhere"), lines: vec![] });
        assert!(a.rollout.rolling);
        a.open_library_job(id);
        assert_eq!(a.job_lib_id, None, "a job opened while rolling out");
        assert_eq!(a.status, ROLLING);
        a.open_job_file(&copy.join("x.snjob"));
        assert_eq!(a.status, ROLLING);
        // Done (here it failed: no release there): jobs open again.
        let until = Instant::now() + Duration::from_secs(60);
        while a.rollout.busy.is_some() {
            a.poll_busy();
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!a.rollout.rolling);
        a.open_library_job(id);
        assert_eq!(a.job_lib_id, Some(id));
        // A roll out that completes with a job from the trial open leaves
        // the PC on its copy.
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Ok(Step::RolledOut(rollout::RolledOut { version: "9.0.0".into(), previous: None, removed: vec![], changes: vec![], repaired: false }))).unwrap();
        a.rollout.busy = Some((String::new(), rx));
        a.poll_busy();
        assert_eq!(a.library_dir(), copy);
        assert!(a.rollout.note.as_ref().is_some_and(|n| n.1.contains("stays on its trial copy")), "{:?}", a.rollout.note);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S9: opening a Library job on a PC ahead of the shop version takes no
    /// hold in the shop (the job opens read-only).
    #[test]
    fn a_pc_ahead_of_the_shop_holds_no_job() {
        let d = tmp("s9_hold");
        let lib = acme(&d, None);
        let l = crate::library::Library::open(&lib, &sam(), &Clock::at(1_794_216_000)).unwrap();
        let id = l.save_job(&crate::jobfile::JobFile::new_job("DEMO-001", crate::engine::default_spec()), crate::library::SaveTo::New).unwrap().id;
        // Then the shop is on an older release than this PC's.
        let lock = crate::shared::WriteLock::take(&lib).unwrap();
        let mut s = crate::shop::read(&lib).unwrap().unwrap();
        s.other.insert(crate::shop::SHOP_VERSION.into(), serde_json::json!("0.0.1"));
        crate::shop::write_judged(&lib, &s, &lock, updates::VERSION, &crate::shared::Versions::this(updates::VERSION)).unwrap();
        drop(lock);
        let mut a = admin_app(&d, &lib);
        a.cfg.in_use_warn = true;
        a.open_library_job(id);
        assert_eq!(a.job_lib_id, Some(id));
        assert!(a.inuse.held.is_empty() && !crate::engine::inuse::path(&lib, id).exists(), "a hold was taken in the shop");
        assert!(a.read_only().is_some());
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

    /// UP24 (review S4): the warning asked "0.1.0 back to 0.0.9"; the shop
    /// moved on (0.2.0 rolled out) before Yes. Yes refuses, changing
    /// nothing, and asks again: it never rolls back what wasn't confirmed.
    #[test]
    fn yes_on_a_roll_back_warning_the_shop_moved_on_from_changes_nothing() {
        let d = tmp("rollback_moved");
        let older = "0.0.9";
        let lib = acme(&d, Some(older));
        rollout::rollout(&lib, &rollout::fake_release(&d, updates::VERSION), &sam(), updates::VERSION).unwrap();
        let mut a = admin_app(&d, &lib);
        let ask = rollout::rollback_ask(&lib, &sam(), updates::VERSION).unwrap();
        assert_eq!((ask.from.as_str(), ask.to.as_str()), (updates::VERSION, older));
        // Meanwhile 0.2.0 is rolled out (from another window of the admin
        // PC running it).
        let next = "9.9.9";
        rollout::rollout(&lib, &rollout::fake_release(&d, next), &sam(), next).unwrap();
        let before = std::fs::read(crate::shop::file(&lib)).unwrap();
        a.roll_back(lib.clone(), ask);
        let until = Instant::now() + Duration::from_secs(60);
        while a.rollout.busy.is_some() {
            a.poll_busy();
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(std::fs::read(crate::shop::file(&lib)).unwrap(), before, "the shop file changed");
        assert_eq!(crate::shop::read(&lib).unwrap().unwrap().shop_version(), Some(next));
        let note = a.rollout.note.clone().unwrap_or_default().1;
        assert!(note.contains(&format!("the shop version is {next} now")) && note.contains("Nothing was rolled back") && note.contains("ask again"), "{note}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
