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
use crate::updates::{self, Chosen, Source, Why, HANDOFF};
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

impl App {
    /// The release folders fetched on this PC (UP16, UP19): beside its
    /// settings, or in the screenshot run's own folder.
    fn releases_dir(&self) -> PathBuf {
        if self.shot_mode_now() {
            return crate::settings_ui::shop_demo_dir().and_then(|d| d.parent().map(|p| p.join("Releases"))).unwrap_or_default();
        }
        crate::settings::AppSettings::file().parent().map(|p| p.join("Releases")).unwrap_or_default()
    }

    /// Where trial copies go on this PC (UP16): `Documents\Sheet Nesting\Trials`.
    fn trials_dir(&self) -> PathBuf {
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
            _ => None,
        }
    }

    /// The shop folder: the Library's, or in a trial the shop it was copied
    /// from.
    pub fn real_shop(&self) -> PathBuf {
        self.trial_mark().map_or_else(|| self.library_dir(), |m| PathBuf::from(m.shop_folder))
    }

    /// UP16, UP17: this PC's Library becomes `dir` (a trial copy, or the
    /// shop folder again), saved in its settings at once.
    fn switch_library(&mut self, dir: &Path) {
        self.cfg.library_dir = dir.display().to_string();
        if !self.shot_mode_now() {
            if let Err(e) = self.cfg.save() {
                self.setui.note = format!("Couldn't save settings: {e}");
            }
            self.cfg_saved = self.cfg.clone();
        }
        self.library_folder_changed();
        self.setui.shop = None;
        self.drop_fetch();
        let r = &mut self.rollout;
        r.library = Some(dir.to_path_buf());
        r.from = None;
        r.look = true;
        r.trial_looked = true;
    }

    /// Withdraw the shop's or a trial's release chosen to install (a PC of
    /// its own's public one is UpdateState's).
    fn drop_fetch(&mut self) {
        if !matches!(self.rollout.fetch, Fetch::No) {
            HANDOFF.withdraw();
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
        self.rollout.fetch = Fetch::Running(version.to_string(), updates::download_in_background(&HANDOFF, c));
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
            self.look_at_trial(&dir);
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

    /// UP16, UP17, UP18: the trial, when this Library opens.
    fn look_at_trial(&mut self, dir: &Path) {
        match rollout::trial_now(dir, updates::VERSION) {
            TrialNow::NotTrial => {}
            TrialNow::Over { mark, said } => {
                self.switch_library(Path::new(&mark.shop_folder));
                self.rollout.note = Some((true, said));
            }
            TrialNow::ShopAway { said, .. } => self.rollout.note = Some((false, said)),
            TrialNow::Trying { installed: true, .. } => match rollout::trial_changes(dir, updates::VERSION) {
                Ok(made) if !made.is_empty() => self.rollout.note = Some((true, format!("Made this release's one-time changes on the trial copy: {}.", made.join("; ")))),
                Ok(_) => {}
                Err(e) => self.rollout.note = Some((false, e)),
            },
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
            Ok(Step::Trial(t)) => {
                let v = t.mark.version.clone();
                self.switch_library(&t.copy);
                // The copy is this PC's Library now: the trial's release
                // installs when the program closes.
                self.rollout.from = Some(Ok(FromShop::Trial(t.mark.clone())));
                if updates::VERSION != v {
                    self.choose_from(&v, self.releases_dir().join(&v), Why::Trial);
                }
                self.rollout.note = Some((true, format!("This PC works on a copy of the shop now ({}), made from a fresh backup. Nothing you save here reaches the shop. Version {v} installs when the program closes.", t.copy.display())));
            }
            Ok(Step::RolledOut(r)) => {
                let shop = self.real_shop();
                if self.trial_mark().is_some() {
                    self.switch_library(&shop);
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

    /// UP16: Try on this PC: fetch the release, back up the shop and make
    /// the copy, all in the background.
    fn try_release(&mut self, version: String) {
        let Some(src) = self.setui.upd.source.clone() else { return };
        let (shop, into, root, who, clock, own) = (self.real_shop(), self.releases_dir().join(&version), self.trials_dir(), self.who.clone(), self.clock.clone(), self.cfg.backups.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        let v = version.clone();
        std::thread::spawn(move || {
            let r = updates::fetch_release(&src, &v, &into).and_then(|()| rollout::start_trial(&shop, &root, &v, &who, &clock, updates::VERSION, &own)).map(Step::Trial);
            let _ = tx.send(r);
        });
        self.rollout.note = None;
        self.rollout.busy = Some((format!("Getting {version} ready to try: fetching it, backing up the shop and copying it to this PC…"), rx));
    }

    /// UP17: Stop trying: back to the shop folder at once (read-only until
    /// the shop version is back, UP21), the shop version installing when the
    /// program closes.
    fn stop_trying(&mut self) {
        let Some(m) = self.trial_mark() else { return };
        self.switch_library(Path::new(&m.shop_folder));
        self.rollout.note = Some((true, format!("Stopped trying {}. This PC works on the shop again; the shop version installs when the program closes. The trial copy stays on this PC, untouched.", m.version)));
    }

    /// UP19, UP20: Roll out: make sure this PC's release is fetched, then
    /// show the warning.
    fn prepare_rollout(&mut self) {
        let Some(src) = self.setui.upd.source.clone() else { return };
        let v = updates::VERSION.to_string();
        let (shop, into) = (self.real_shop(), self.releases_dir().join(&v));
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let r = updates::fetch_release(&src, &v, &into).and_then(|()| {
                let st = crate::engine::shop::status(&shop)?.ok_or_else(|| format!("{} isn't a shop.", shop.display()))?;
                let w = rollout::warning(&st, &v, &rollout::layouts());
                Ok(Step::Ready(WarnAsk { version: v.clone(), shop: shop.clone(), name: st.shop.name.clone(), release: into.clone(), lines: w.lines() }))
            });
            let _ = tx.send(r);
        });
        self.rollout.note = None;
        self.rollout.busy = Some((format!("Getting {} ready to roll out…", updates::VERSION), rx));
    }

    /// UP19: Yes on the warning: roll out, in the background.
    fn roll_out(&mut self, w: WarnAsk) {
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
                        self.switch_library(&t.copy);
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
