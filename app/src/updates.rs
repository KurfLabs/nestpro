//! The installer and updates, Windows only (spec #178, ADR 0006).
//!
//! This is the one place the program talks to the installer (Velopack, see
//! docs/releases.md). Velopack is in Windows builds made with the MSVC
//! toolchain (CI and releases); other builds run without it. The decisions
//! about which release to offer, roll out or roll back are engine functions;
//! they never call the installer, so they are tested without it.

/// The program's version: the package version, which a release tag must match
/// (UP4). Settings → About shows it (UP3).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// UP2: the installer's start-up hook. It runs first in `main`, before any
/// window or command line, because the installer starts the program with its
/// own arguments while installing, updating and uninstalling, and the hook
/// handles those and exits. A program that wasn't installed by Setup.exe (a
/// developer build, a copied .exe) carries on as before.
///
/// An update downloaded earlier is not applied here by itself: updates are
/// applied only when this program decides to (UP2), so one never lands
/// in the middle of work.
pub fn startup_hook() {
    #[cfg(all(windows, target_env = "msvc"))]
    velopack::VelopackApp::build().set_auto_apply_on_startup(false).run();
}

/// What Settings → About says about installing (UP3): only that installers
/// and updates are for Windows, on other systems. On Windows it says nothing:
/// a copy unpacked from the portable zip looks just like an installed one.
pub fn install_note() -> Option<&'static str> {
    if cfg!(windows) { None } else { Some("Installers and updates are for Windows.") }
}

// ---------------------------------------------------------------------------
// A PC of its own: checking GitHub Releases and updating when the program
// closes (UP11–UP14). Which release is offered is `engine::updates::offer`.
// ---------------------------------------------------------------------------

use crate::engine::updates::Release;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// The public releases a PC of its own updates from (the installer reads
/// it, in Windows builds made with MSVC).
#[cfg_attr(not(all(windows, target_env = "msvc")), allow(dead_code))]
pub const REPO: &str = "https://github.com/KurfLabs/nestpro";

/// UP11: a whole check (the shop lookup and every request) that hasn't
/// answered by then counts as no connection.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(30);

/// UP11: a PC of its own checks when the program opens and then once a day.
pub const CHECK_EVERY: Duration = Duration::from_secs(24 * 3600);

/// UP14: after the window has closed, how long the program waits for a
/// release still downloading before it ends without installing it.
pub const CLOSE_WAIT: Duration = Duration::from_secs(90);

/// UP11: what the Updates card says when the releases couldn't be read.
pub const COULDNT_CHECK: &str = "Couldn't check for updates";

/// UP14: why this copy has no updates.
const LINUX: &str = "Releases are for Windows.";
const NO_INSTALLER: &str = "This copy can't update itself: updates come with the program Setup.exe installs.";

/// Where the release list comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// A made-up list (tests and screenshots, `--fake-releases=<file>` or
    /// `NESTPRO_FAKE_RELEASES`): a JSON list of `{"version", "notes"}`.
    /// `offline`, or a file that isn't there, plays no connection.
    Fake(PathBuf),
    /// GitHub Releases (Windows builds made with MSVC, which have the
    /// installer).
    Github,
    /// No releases on this copy, and why (UP14: on Linux, that releases are
    /// for Windows).
    Off(&'static str),
    /// A release folder (UP19, UP21): the shop folder's `Updates\<version>`
    /// on a PC in a shop, or the release fetched for a trial on the admin PC.
    Folder(PathBuf),
}

/// This copy's release source: the made-up list when one is given (it plays
/// a Windows PC on any system), else GitHub on Windows builds that have the
/// installer, else none.
pub fn source(fake: Option<PathBuf>) -> Source {
    match fake {
        Some(p) => Source::Fake(p),
        None if cfg!(all(windows, target_env = "msvc")) => Source::Github,
        None if cfg!(windows) => Source::Off(NO_INSTALLER),
        None => Source::Off(LINUX),
    }
}

/// The release list, read now, giving up at `deadline` (UP11). Run it off
/// the window's thread (`check_in_background`).
pub fn release_list(src: &Source, deadline: Instant) -> Result<Vec<Release>, String> {
    match src {
        Source::Fake(p) => {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{COULDNT_CHECK} ({e})."))?;
            serde_json::from_str(&text).map_err(|e| format!("{COULDNT_CHECK} ({e})."))
        }
        Source::Github => github::release_list(deadline, &github::get).map_err(|e| format!("{COULDNT_CHECK} ({e}).")),
        Source::Off(why) => Err(why.to_string()),
        Source::Folder(p) => Err(format!("{COULDNT_CHECK}: {} is a release folder, not a release list.", p.display())),
    }
}

/// UP15: on the admin PC in a shop, a person's click: the public releases,
/// read now (off the window's thread), to find a release to try. A PC in a
/// shop never reads them by itself (UP11).
pub fn releases_in_background(src: Source) -> Receiver<Result<Vec<Release>, String>> {
    let (tx, rx) = channel();
    let tx2 = tx.clone();
    if let Err(e) = spawn("release list", move || {
        let _ = tx.send(release_list(&src, Instant::now() + CHECK_TIMEOUT));
    }) {
        let _ = tx2.send(Err(format!("{COULDNT_CHECK} ({e}).")));
    }
    rx
}

/// UP16, UP19: how long fetching a release's packages may take.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// UP16, UP19: fetch release `version`'s packages (its index and full
/// package) into the release folder `into` on this PC, for Try on this PC
/// and Roll out. A folder that already holds them is used as it is. They
/// are fetched into a folder of this PC's own beside it, then renamed into
/// place, so a fetch that stops halfway never counts. A made-up list makes
/// a made-up release (screenshots and tests).
pub fn fetch_release(src: &Source, version: &str, into: &Path) -> Result<(), String> {
    if crate::engine::rollout::pack_in(into, version).and_then(|p| crate::engine::rollout::verify(into, &p)).is_ok() {
        return Ok(());
    }
    let part = into.with_file_name(format!(".{}.{}.part", into.file_name().map_or_else(|| "release".into(), |n| n.to_string_lossy().into_owned()), crate::shared::writer_tag()));
    std::fs::create_dir_all(&part).map_err(|e| format!("{} can't be made ({e})", part.display()))?;
    let got = match src {
        Source::Fake(_) => fake_release(&part, version),
        Source::Github => github::download_release(version, &part, Instant::now() + FETCH_TIMEOUT, &github::get, &github::get_bytes),
        Source::Off(why) => Err(why.to_string()),
        Source::Folder(p) => Err(format!("{} is a release folder already", p.display())),
    }
    .and_then(|()| crate::engine::rollout::pack_in(&part, version).and_then(|p| crate::engine::rollout::verify(&part, &p)))
    .and_then(|()| {
        if into.exists() {
            std::fs::remove_dir_all(into).map_err(|e| format!("{} can't be replaced ({e})", into.display()))?;
        }
        std::fs::rename(&part, into).map_err(|e| format!("{} can't be made ({e})", into.display()))
    });
    if got.is_err() {
        let _ = std::fs::remove_dir_all(&part);
    }
    got.map_err(|e| format!("Version {version} couldn't be fetched ({e})."))
}

/// A made-up release folder (`--fake-releases`): an index and a package
/// that are no program.
fn fake_release(into: &Path, version: &str) -> Result<(), String> {
    let pkg = format!("KurfLabs.nestpro-{version}-full.nupkg");
    let body = format!("made-up package {version}");
    std::fs::write(into.join(&pkg), &body).map_err(|e| e.to_string())?;
    let sha: String = <sha2::Sha256 as sha2::Digest>::digest(body.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    let idx = serde_json::json!({"Assets": [{"PackageId": "KurfLabs.nestpro", "Version": version, "Type": "Full", "FileName": pkg, "SHA256": sha, "Size": body.len(), "NotesMarkdown": ""}]});
    std::fs::write(into.join(crate::engine::rollout::INDEX), idx.to_string()).map_err(|e| e.to_string())
}

/// UP11: what a check found.
#[derive(Clone, Debug)]
pub enum Found {
    /// The releases (this PC isn't in a shop).
    Releases(Vec<Release>),
    /// This PC's Library is a shop's: it never checks GitHub, it updates from
    /// the shop folder (ADR 0006).
    InShop(String),
    /// Not checked: the reason (the releases couldn't be read, the Library
    /// can't be reached so it can't be told whether this PC is in a shop, or
    /// this copy has no releases).
    NotChecked(String),
}

/// Why this PC may not use the public releases now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotStandalone {
    /// Its Library is this shop's.
    InShop(String),
    /// Its Library can't be reached, so it can't be told.
    Unknown(String),
    /// A shop release that isn't the shop version any more, or a trial
    /// that ended: why (UP21).
    Moved(String),
}

impl From<NotStandalone> for Found {
    fn from(n: NotStandalone) -> Found {
        match n {
            NotStandalone::InShop(name) => Found::InShop(name),
            NotStandalone::Unknown(e) | NotStandalone::Moved(e) => Found::NotChecked(e),
        }
    }
}

/// UP11: whether this PC may use the public releases: its Library (`dir`)
/// isn't a shop's. `own` is this PC's own Library folder: while it isn't
/// made yet, this PC is in no shop.
pub fn standalone(dir: &Path, own: &Path) -> Result<(), NotStandalone> {
    match crate::engine::updates::shop_of(dir, own) {
        Ok(None) => Ok(()),
        Ok(Some(s)) => Err(NotStandalone::InShop(s.name)),
        Err(e) => Err(NotStandalone::Unknown(format!("Not checked: this PC's Library can't be reached, so it isn't known whether this PC is in a shop. {e}"))),
    }
}

/// Run `f` on a thread of its own; Err when the system can't start one.
fn spawn(name: &str, f: impl FnOnce() + Send + 'static) -> Result<(), String> {
    std::thread::Builder::new().name(name.into()).spawn(f).map(|_| ()).map_err(|e| format!("couldn't start a thread: {e}"))
}

/// UP11: check for releases: whether this PC's Library (`dir`) is a shop's,
/// then the release list. The whole check, the shop lookup included, has
/// `timeout`: one that takes longer counts as no connection, and no request
/// to GitHub starts after it.
pub fn check(dir: &Path, own: &Path, src: &Source, timeout: Duration) -> Found {
    if let Source::Off(why) = src {
        return Found::NotChecked(why.to_string());
    }
    let (dir, own, src) = (dir.to_path_buf(), own.to_path_buf(), src.clone());
    check_with(move || standalone(&dir, &own), move |deadline| release_list(&src, deadline), timeout)
}

/// `check`, with the shop lookup and the release list given (tests).
pub fn check_with(lookup: impl FnOnce() -> Result<(), NotStandalone> + Send + 'static, list: impl FnOnce(Instant) -> Result<Vec<Release>, String> + Send + 'static, timeout: Duration) -> Found {
    let deadline = Instant::now() + timeout;
    let (tx, rx) = channel();
    let started = spawn("update check", move || {
        let found = match lookup() {
            Err(n) => n.into(),
            // The lookup took the whole time: nothing more is asked.
            Ok(()) if Instant::now() >= deadline => return,
            Ok(()) => match list(deadline) {
                Ok(l) => Found::Releases(l),
                Err(e) => Found::NotChecked(e),
            },
        };
        let _ = tx.send(found);
    });
    if let Err(e) = started {
        return Found::NotChecked(format!("{COULDNT_CHECK} ({e})."));
    }
    rx.recv_timeout(timeout).unwrap_or_else(|_| Found::NotChecked(format!("{COULDNT_CHECK} (no answer in {} s).", timeout.as_secs())))
}

/// UP11: `check`, off the window's thread: the answer comes on the receiver.
pub fn check_in_background(dir: PathBuf, own: PathBuf, src: Source) -> Receiver<Found> {
    let (tx, rx) = channel();
    let tx2: Sender<Found> = tx.clone();
    if let Err(e) = spawn("update check", move || {
        let _ = tx.send(check(&dir, &own, &src, CHECK_TIMEOUT));
    }) {
        let _ = tx2.send(Found::NotChecked(format!("{COULDNT_CHECK} ({e}).")));
    }
    rx
}

// --- Update when I close: from the click to the install (UP14) ----------

/// Where an accepted update is, from Update when I close to the install.
#[derive(Clone, Debug, PartialEq)]
enum Stage {
    Idle,
    Fetching(Chosen),
    Ready(Chosen),
}

/// Why a release installs when the program closes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Why {
    /// A PC of its own chose a public release (UP14).
    #[default]
    Public,
    /// The shop version, from the shop folder (UP21).
    Shop,
    /// The release the admin PC tries (UP16).
    Trial,
}

/// The release chosen, and for which Library and source.
#[derive(Clone, Debug, PartialEq)]
pub struct Chosen {
    pub version: String,
    pub dir: PathBuf,
    pub own: PathBuf,
    pub src: Source,
    pub why: Why,
}

impl Chosen {
    /// UP14, UP21, UP16: whether the choice still stands, looked at before
    /// a download and once the window has closed: a public release only
    /// while this PC is outside a shop; the shop's only while it is still
    /// the shop version; a trial's while this PC's Library is a trial of it.
    pub fn stands(&self) -> Result<(), NotStandalone> {
        match self.why {
            Why::Public => standalone(&self.dir, &self.own),
            Why::Shop => match crate::shop::read(&self.dir) {
                Ok(Some(s)) if s.trial().is_none() && s.shop_version() == Some(self.version.as_str()) => Ok(()),
                Ok(Some(s)) => Err(NotStandalone::Moved(format!("Version {} wasn't installed: the shop version is {} now.", self.version, s.shop_version().unwrap_or("none")))),
                Ok(None) => Err(NotStandalone::Moved(format!("Version {} wasn't installed: this PC's Library isn't a shop's any more.", self.version))),
                Err(e) => Err(NotStandalone::Unknown(format!("Not installed: {e}"))),
            },
            // S3: a trial stands only while the real shop is still on the
            // version it began on: a roll out or a roll back ends it.
            Why::Trial => match crate::shop::read(&self.dir) {
                Ok(Some(s)) if s.trial().is_some_and(|m| m.version == self.version) => match crate::engine::rollout::trial_now(&self.dir, &self.version) {
                    crate::engine::rollout::TrialNow::Trying { .. } => Ok(()),
                    crate::engine::rollout::TrialNow::ShopAway { said, .. } => Err(NotStandalone::Unknown(format!("Version {} wasn't installed: {said}", self.version))),
                    _ => Err(NotStandalone::Moved(format!("Version {} wasn't installed: the shop moved on while this PC was trying it.", self.version))),
                },
                Ok(_) => Err(NotStandalone::Moved(format!("Version {} wasn't installed: this PC stopped trying it.", self.version))),
                Err(e) => Err(NotStandalone::Unknown(format!("Not installed: {e}"))),
            },
        }
    }
}

/// UP14: the update a person chose, kept until the program closes. Each
/// choice gets a number; a download that finishes for a choice withdrawn
/// since (the Library changed or became a shop's) changes nothing.
pub struct Handoff {
    slot: Mutex<(u64, Stage)>,
    changed: Condvar,
    /// S3: the Library the window works on, and this PC's own Library
    /// folder, so the close can look at its shop once more.
    place: Mutex<Option<(PathBuf, PathBuf)>>,
    /// S2: backups, restores, roll outs and roll backs under way (how many
    /// of each, by `Work::slot`): the window doesn't close, and nothing
    /// installs, until they are done.
    work: Mutex<[u32; 4]>,
    work_done: Condvar,
}

/// S2: work on the Library's files that must never stop halfway when the
/// program closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Work {
    /// A backup being taken (the day's, Back up now, or Try on this PC's).
    Backup,
    /// A restore putting a backup's files back (BK8).
    Restore,
    /// Roll out writing the release and the shop version into the shop
    /// folder (UP19).
    RollOut,
    /// Roll back writing the shop version back (UP23).
    RollBack,
}

impl Work {
    /// Where it is counted; the window names the first that runs.
    const ALL: [Work; 4] = [Work::Restore, Work::RollBack, Work::RollOut, Work::Backup];

    fn slot(self) -> usize {
        match self {
            Work::Restore => 0,
            Work::RollBack => 1,
            Work::RollOut => 2,
            Work::Backup => 3,
        }
    }

    /// What the window says while closing waits for it.
    pub fn closing_text(self) -> &'static str {
        match self {
            Work::Restore => "Restoring — the program closes when it's done.",
            Work::Backup => "Backing up the shop — the program closes when it's done.",
            Work::RollOut => "Rolling out — the program closes when it's done.",
            Work::RollBack => "Rolling back — the program closes when it's done.",
        }
    }
}

/// S2: held by a backup's or a restore's thread while it runs.
pub struct WorkGuard {
    h: &'static Handoff,
    w: Work,
}

impl Drop for WorkGuard {
    fn drop(&mut self) {
        let mut n = self.h.work.lock().unwrap_or_else(|p| p.into_inner());
        n[self.w.slot()] = n[self.w.slot()].saturating_sub(1);
        self.h.work_done.notify_all();
    }
}

/// The program's own handoff: the window fills it, `install_on_close`
/// empties it once the window has closed.
pub static HANDOFF: Handoff = Handoff::new();

impl Handoff {
    pub const fn new() -> Handoff {
        Handoff { slot: Mutex::new((0, Stage::Idle)), changed: Condvar::new(), place: Mutex::new(None), work: Mutex::new([0; 4]), work_done: Condvar::new() }
    }

    /// S2: a backup, restore, roll out or roll back starts: taken on the
    /// window's thread before its own thread starts, and held by that
    /// thread until it is done.
    pub fn begin(&'static self, w: Work) -> WorkGuard {
        let mut n = self.work.lock().unwrap_or_else(|p| p.into_inner());
        n[w.slot()] += 1;
        WorkGuard { h: self, w }
    }

    /// S2: what runs now that closing must wait for (a restore first).
    pub fn working(&self) -> Option<Work> {
        let n = *self.work.lock().unwrap_or_else(|p| p.into_inner());
        Work::ALL.into_iter().find(|w| n[w.slot()] > 0)
    }

    /// S2: wait until none of that work runs (however long it takes:
    /// ending the program halfway would leave the shop folder half
    /// written).
    pub fn wait_for_work(&self) {
        while self.working().is_some() {
            crate::shared::pause("close: waiting for a backup or restore");
            let n = self.work.lock().unwrap_or_else(|p| p.into_inner());
            if n.iter().all(|&k| k == 0) {
                break;
            }
            let _ = self.work_done.wait_timeout(n, Duration::from_millis(500));
        }
    }

    /// S3: the window works on the Library in `dir` (`own`: this PC's own
    /// Library folder).
    pub fn set_place(&self, dir: &Path, own: &Path) {
        let mut p = self.place.lock().unwrap_or_else(|p| p.into_inner());
        if p.as_ref().is_none_or(|(d, o)| d != dir || o != own) {
            *p = Some((dir.to_path_buf(), own.to_path_buf()));
        }
    }

    fn place(&self) -> Option<(PathBuf, PathBuf)> {
        self.place.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn slot(&self) -> std::sync::MutexGuard<'_, (u64, Stage)> {
        self.slot.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// A download starts for `c`: its number.
    pub fn start(&self, c: Chosen) -> u64 {
        let mut s = self.slot();
        s.0 += 1;
        s.1 = Stage::Fetching(c);
        self.changed.notify_all();
        s.0
    }

    /// The download numbered `n` has finished (`ok`: the package is on this
    /// PC). Ignored when that choice was withdrawn or replaced since.
    pub fn done(&self, n: u64, ok: bool) {
        let mut s = self.slot();
        if s.0 == n {
            if let Stage::Fetching(c) = std::mem::replace(&mut s.1, Stage::Idle) {
                if ok {
                    s.1 = Stage::Ready(c);
                }
            }
            self.changed.notify_all();
        }
    }

    /// Withdraw the choice (S2: the Library changed, or is a shop's now):
    /// nothing is installed, and its download's end is ignored.
    pub fn withdraw(&self) {
        let mut s = self.slot();
        s.0 += 1;
        s.1 = Stage::Idle;
        self.changed.notify_all();
    }

    /// S1: withdraw the choice only when it is `why`'s (or there is none):
    /// a PC of its own's public release is withdrawn without touching the
    /// shop's or a trial's, and the other way round.
    pub fn withdraw_for(&self, why: Why) {
        let mut s = self.slot();
        let owned = match &s.1 {
            Stage::Fetching(c) | Stage::Ready(c) => c.why == why,
            Stage::Idle => true,
        };
        if owned {
            s.0 += 1;
            s.1 = Stage::Idle;
            self.changed.notify_all();
        }
    }

    /// The program has closed: wait up to `limit` for a download still
    /// running, then take the release that is ready, if any.
    pub fn take_ready(&self, limit: Duration) -> Option<Chosen> {
        let s = self.slot();
        let (mut s, _) = self.changed.wait_timeout_while(s, limit, |s| matches!(s.1, Stage::Fetching(_))).unwrap_or_else(|p| p.into_inner());
        match std::mem::replace(&mut s.1, Stage::Idle) {
            Stage::Ready(c) => Some(c),
            other => {
                s.1 = other;
                None
            }
        }
    }
}

/// UP14: Update when I close: fetch `c.version` in the background. This PC
/// must still be outside a shop when the download starts. Once fetched, it
/// installs when the program closes (`install_on_close`), never before. A
/// made-up list fetches nothing.
pub fn download_in_background(h: &'static Handoff, c: Chosen) -> Receiver<Result<(), String>> {
    let (tx, rx) = channel();
    let tx2 = tx.clone();
    let n = h.start(c.clone());
    let started = spawn("update download", move || {
        let r = match c.stands() {
            Err(NotStandalone::InShop(name)) => Err(format!("Not downloaded: this PC is in shop {name} now, and updates from the shop folder.")),
            Err(NotStandalone::Unknown(e) | NotStandalone::Moved(e)) => Err(e),
            Ok(()) => match &c.src {
                Source::Fake(_) => Ok(()),
                Source::Github => installer::download(&c.version).map_err(|e| format!("Couldn't download version {} ({e}).", c.version)),
                Source::Folder(p) => installer::download_from(p, &c.version).map_err(|e| format!("Couldn't get version {} from {} ({e}).", c.version, p.display())),
                Source::Off(why) => Err(why.to_string()),
            },
        };
        h.done(n, r.is_ok());
        let _ = tx.send(r);
    });
    if let Err(e) = started {
        h.done(n, false);
        let _ = tx2.send(Err(format!("Couldn't download the update ({e}).")));
    }
    rx
}

/// UP14, UP21 (S3): what happens once the window has closed: wait up to
/// `limit` for the chosen release, look once more at where this PC stands
/// (`at_close`), fetch the shop's release then if it wasn't fetched yet, and
/// hand it to `apply`. Ok(None): nothing to install (none chosen, not ready
/// in time, or this PC runs the shop version: the next start decides).
pub fn finish_on_close(h: &Handoff, limit: Duration, apply: impl FnOnce(&Chosen) -> Result<(), String>) -> Result<Option<String>, String> {
    finish_on_close_as(h, VERSION, limit, |c| fetch_now(c, CLOSE_WAIT), apply)
}

/// `finish_on_close` for a program on `this`, fetching with `fetch` (tests).
pub fn finish_on_close_as(h: &Handoff, this: &str, limit: Duration, fetch: impl FnOnce(&Chosen) -> Result<(), String>, apply: impl FnOnce(&Chosen) -> Result<(), String>) -> Result<Option<String>, String> {
    let place = h.place();
    let this = this.to_string();
    finish_on_close_with(h, limit, move |q| at_close(q, place, &this), CHECK_TIMEOUT, fetch, apply)
}

/// `finish_on_close`, with the look at the shop given (tests). The look has
/// `lookup_limit`: a share that doesn't answer in time installs nothing (the
/// next start decides), and the program still ends.
pub fn finish_on_close_with(
    h: &Handoff,
    limit: Duration,
    resolve: impl FnOnce(Option<Chosen>) -> Result<Option<(Chosen, bool)>, NotStandalone> + Send + 'static,
    lookup_limit: Duration,
    fetch: impl FnOnce(&Chosen) -> Result<(), String>,
    apply: impl FnOnce(&Chosen) -> Result<(), String>,
) -> Result<Option<String>, String> {
    // S2: a backup or a restore still running finishes first: the program
    // never ends, nor an update installs, halfway through one.
    h.wait_for_work();
    let queued = h.take_ready(limit);
    let what = queued.as_ref().map_or("The shop's release".to_string(), |c| format!("Version {}", c.version));
    let (tx, rx) = channel();
    spawn("update shop lookup", move || {
        let _ = tx.send(resolve(queued));
    })
    .map_err(|e| format!("{what} wasn't installed: {e}"))?;
    let (c, fetched) = match rx.recv_timeout(lookup_limit) {
        Ok(Ok(None)) => return Ok(None),
        Ok(Ok(Some(c))) => c,
        Ok(Err(NotStandalone::InShop(name))) => return Err(format!("{what} wasn't installed: this PC is in shop {name} now.")),
        Ok(Err(NotStandalone::Unknown(e))) => return Err(format!("{what} wasn't installed: {e}")),
        Ok(Err(NotStandalone::Moved(e))) => return Err(e),
        Err(_) => return Err(format!("{what} wasn't installed: this PC's Library didn't answer in {} s, so it isn't known what the shop runs. The next start decides.", lookup_limit.as_secs())),
    };
    if !fetched {
        fetch(&c).map_err(|e| format!("Version {} wasn't installed: {e}", c.version))?;
    }
    apply(&c)?;
    Ok(Some(c.version))
}

/// UP21 (S3): what a PC on `this` installs when it closes, looked at then,
/// with `queued` the release it chose (and fetched) before, and `place` the
/// Library it works on and its own Library folder. Gives the release, and
/// whether it is fetched already.
///
/// - Not known where it works: the release queued, while it still stands.
/// - A Library of its own: a public release queued, while this PC is still
///   outside a shop; a shop's or a trial's queued release is dropped.
/// - A trial copy: the tried release queued, while the trial lasts. A
///   trial the shop moved on from (a roll out or a roll back) is over: what
///   the real shop runs, as below.
/// - A shop: its shop version, from its `Updates`, when this PC runs
///   another (fetched now when it wasn't queued); nothing when this PC runs
///   it, or the shop has none, or its package isn't there.
pub fn at_close(queued: Option<Chosen>, place: Option<(PathBuf, PathBuf)>, this: &str) -> Result<Option<(Chosen, bool)>, NotStandalone> {
    use crate::engine::rollout::{trial_now, TrialNow};
    use crate::engine::updates::{from_shop, FromShop};
    let Some((lib, own)) = place else {
        return match queued {
            None => Ok(None),
            Some(c) => c.stands().map(|()| Some((c, true))),
        };
    };
    // S6: the same look as the offer and the download (`standalone`): this
    // PC's own Library folder not made yet is a PC of its own, not an error.
    let shop_dir = match crate::engine::updates::shop_of(&lib, &own) {
        Err(e) => return Err(NotStandalone::Unknown(e)),
        Ok(None) => {
            return match queued {
                Some(c) if c.why == Why::Public && c.dir == lib => c.stands().map(|()| Some((c, true))),
                _ => Ok(None),
            }
        }
        Ok(Some(s)) => match s.trial() {
            None => lib.clone(),
            Some(m) => match trial_now(&lib, this) {
                TrialNow::Trying { .. } | TrialNow::NotTrial => {
                    return Ok(queued.filter(|c| c.why == Why::Trial && c.version == m.version && c.dir == lib).map(|c| (c, true)));
                }
                TrialNow::ShopAway { said, .. } => return Err(NotStandalone::Unknown(said)),
                TrialNow::Over { mark, .. } => PathBuf::from(mark.shop_folder),
            },
        },
    };
    match from_shop(&shop_dir, this) {
        Err(e) => Err(NotStandalone::Unknown(e)),
        Ok(FromShop::Update { version, folder, .. }) => {
            if let Some(c) = queued.filter(|c| c.why == Why::Shop && c.version == version && c.dir == shop_dir) {
                return Ok(Some((c, true)));
            }
            Ok(Some((Chosen { version, dir: shop_dir, own, src: Source::Folder(folder), why: Why::Shop }, false)))
        }
        Ok(_) => Ok(None),
    }
}

/// S3: fetch `c` at close, from its folder, waiting at most `limit`.
fn fetch_now(c: &Chosen, limit: Duration) -> Result<(), String> {
    let p = match &c.src {
        Source::Folder(p) => p.clone(),
        Source::Fake(_) => return Ok(()),
        Source::Off(why) => return Err(why.to_string()),
        Source::Github => return Err("a shop's release comes from the shop folder".into()),
    };
    let (tx, rx) = channel();
    let v = c.version.clone();
    spawn("update fetch on close", move || {
        let _ = tx.send(installer::download_from(&p, &v));
    })?;
    rx.recv_timeout(limit).unwrap_or_else(|_| Err(format!("it wasn't fetched within {} s", limit.as_secs())))
}

/// UP14: the program has closed (its window is gone, and every job with it):
/// install the update chosen with Update when I close. The installer waits
/// for this program to end, then installs; the program isn't started again
/// by itself.
pub fn install_on_close() {
    let apply = |c: &Chosen| match c.src {
        Source::Github => installer::apply(&c.version),
        Source::Folder(_) => installer::apply_fetched(&c.version),
        _ => Ok(()),
    };
    match finish_on_close(&HANDOFF, CLOSE_WAIT, apply) {
        Ok(_) => {}
        Err(e) => eprintln!("{e}"),
    }
}

/// UP11: reading GitHub Releases ourselves, with a time limit on every
/// request, and an error when a published release's index can't be read
/// (S3: the installer's own reader skips those, which would offer an older
/// release as the newest).
mod github {
    use super::Release;
    use serde_json::Value;
    use std::time::Instant;

    /// The releases, newest published first, as GitHub's API lists them.
    pub const RELEASES: &str = "https://api.github.com/repos/KurfLabs/nestpro/releases?per_page=10";
    /// The installer's index in each release (the Windows channel).
    pub const INDEX: &str = "releases.win.json";

    /// GET `url`, giving up at `deadline`; never starts after it.
    #[cfg_attr(not(all(windows, target_env = "msvc")), allow(dead_code))]
    pub fn get(url: &str, deadline: Instant) -> Result<String, String> {
        let left = deadline.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()).ok_or("no answer in time")?;
        #[cfg(windows)]
        let provider = ureq::tls::TlsProvider::NativeTls;
        #[cfg(not(windows))]
        let provider = ureq::tls::TlsProvider::Rustls;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .tls_config(ureq::tls::TlsConfig::builder().provider(provider).build())
            .timeout_global(Some(left))
            .user_agent(concat!("nestpro/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        let mut r = agent.get(url).header("Accept", "application/vnd.github+json").call().map_err(|e| match e {
            ureq::Error::Timeout(_) => "no answer in time".to_string(),
            ureq::Error::StatusCode(c) => format!("GitHub answered {c}"),
            e => e.to_string(),
        })?;
        r.body_mut().with_config().limit(10 * 1024 * 1024).read_to_string().map_err(|e| e.to_string())
    }

    /// GET `url` as bytes (a package), giving up at `deadline`.
    #[cfg_attr(not(all(windows, target_env = "msvc")), allow(dead_code))]
    pub fn get_bytes(url: &str, deadline: Instant) -> Result<Vec<u8>, String> {
        let left = deadline.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()).ok_or("no answer in time")?;
        #[cfg(windows)]
        let provider = ureq::tls::TlsProvider::NativeTls;
        #[cfg(not(windows))]
        let provider = ureq::tls::TlsProvider::Rustls;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .tls_config(ureq::tls::TlsConfig::builder().provider(provider).build())
            .timeout_global(Some(left))
            .user_agent(concat!("nestpro/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        let mut r = agent.get(url).header("Accept", "application/octet-stream").call().map_err(|e| match e {
            ureq::Error::Timeout(_) => "no answer in time".to_string(),
            ureq::Error::StatusCode(c) => format!("GitHub answered {c}"),
            e => e.to_string(),
        })?;
        r.body_mut().with_config().limit(2 * 1024 * 1024 * 1024).read_to_vec().map_err(|e| e.to_string())
    }

    /// UP16, UP19: the published release `version`'s index and full
    /// package, written into the folder `into`.
    #[allow(clippy::type_complexity)]
    pub fn download_release(version: &str, into: &std::path::Path, deadline: Instant, get: &dyn Fn(&str, Instant) -> Result<String, String>, get_bytes: &dyn Fn(&str, Instant) -> Result<Vec<u8>, String>) -> Result<(), String> {
        let list: Value = serde_json::from_str(&get(RELEASES, deadline)?).map_err(|e| format!("GitHub's release list can't be read: {e}"))?;
        let tag = format!("v{version}");
        let r = list
            .as_array()
            .into_iter()
            .flatten()
            .find(|r| r["tag_name"].as_str() == Some(tag.as_str()) && !r["draft"].as_bool().unwrap_or(false) && !r["prerelease"].as_bool().unwrap_or(false))
            .ok_or_else(|| format!("release {tag} isn't among the published releases"))?;
        let asset = |name: &str| r["assets"].as_array().into_iter().flatten().find(|a| a["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(name))).and_then(|a| a["browser_download_url"].as_str()).map(String::from);
        let idx_url = asset(INDEX).ok_or_else(|| format!("release {tag} has no {INDEX}"))?;
        let idx = get(&idx_url, deadline)?;
        std::fs::write(into.join(crate::engine::rollout::INDEX), &idx).map_err(|e| e.to_string())?;
        let v: Value = serde_json::from_str(&idx).map_err(|e| format!("its index can't be read ({e})"))?;
        let key = |o: &Value, k: &str| o.as_object().and_then(|m| m.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v.clone())).unwrap_or(Value::Null);
        let assets = key(&v, "Assets");
        let full = assets
            .as_array()
            .into_iter()
            .flatten()
            .find(|a| key(a, "Type").as_str().is_some_and(|t| t.eq_ignore_ascii_case("Full")) && key(a, "Version").as_str() == Some(version))
            .and_then(|a| key(a, "FileName").as_str().map(String::from))
            .ok_or_else(|| format!("release {tag}'s index lists no full package"))?;
        if full.contains(['/', '\\', ':']) {
            return Err(format!("release {tag}'s index names an odd package {full}"));
        }
        let url = asset(&full).ok_or_else(|| format!("release {tag} has no {full}"))?;
        let bytes = get_bytes(&url, deadline)?;
        std::fs::write(into.join(&full), bytes).map_err(|e| e.to_string())
    }

    /// Every plain release's versions and notes, from the index in each
    /// published release (drafts and pre-releases left out). A release with
    /// no index (not made by the release workflow) is skipped; one whose
    /// index can't be read or understood fails the whole check.
    pub fn release_list(deadline: Instant, get: &dyn Fn(&str, Instant) -> Result<String, String>) -> Result<Vec<Release>, String> {
        let list: Value = serde_json::from_str(&get(RELEASES, deadline)?).map_err(|e| format!("GitHub's release list can't be read: {e}"))?;
        let list = list.as_array().ok_or("GitHub's release list can't be read")?;
        let mut out = vec![];
        for r in list {
            if r["draft"].as_bool().unwrap_or(false) || r["prerelease"].as_bool().unwrap_or(false) {
                continue;
            }
            let tag = r["tag_name"].as_str().unwrap_or("?");
            let Some(url) = r["assets"].as_array().into_iter().flatten().find(|a| a["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(INDEX))).and_then(|a| a["browser_download_url"].as_str()) else {
                continue;
            };
            let text = get(url, deadline).map_err(|e| format!("release {tag}: {e}"))?;
            out.extend(index(&text).map_err(|e| format!("release {tag}: {e}"))?);
        }
        Ok(out)
    }

    /// The full releases in an installer index (`{"Assets": [{"Version",
    /// "Type", "NotesMarkdown"}, …]}`, any letter case).
    pub fn index(text: &str) -> Result<Vec<Release>, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("its index can't be read ({e})"))?;
        let key = |o: &Value, k: &str| o.as_object().and_then(|m| m.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v.clone())).unwrap_or(Value::Null);
        let assets = key(&v, "Assets");
        let assets = assets.as_array().ok_or("its index lists no releases")?;
        Ok(assets
            .iter()
            .filter(|a| key(a, "Type").as_str().is_some_and(|t| t.eq_ignore_ascii_case("Full")))
            .filter_map(|a| Some(Release { version: key(a, "Version").as_str()?.to_string(), notes: key(a, "NotesMarkdown").as_str().unwrap_or("").to_string() }))
            .collect())
    }
}

/// The installer's own calls (Velopack, docs/releases.md), in Windows builds
/// made with MSVC: download a chosen release, and hand it over to install
/// when the program has ended.
#[cfg(all(windows, target_env = "msvc"))]
mod installer {
    use velopack::{sources::GithubSource, UpdateInfo, UpdateManager};

    fn manager() -> Result<UpdateManager, String> {
        // Pre-releases are never fetched (UP4).
        UpdateManager::new(GithubSource::new(super::REPO, None, false), None, None).map_err(|e| format!("this copy wasn't installed by Setup.exe: {e}"))
    }

    pub fn download(version: &str) -> Result<(), String> {
        let um = manager()?;
        let feed = um.get_release_feed().map_err(|e| e.to_string())?;
        let asset = feed.Assets.into_iter().find(|a| a.Type.eq_ignore_ascii_case("Full") && a.Version == version).ok_or_else(|| format!("version {version} isn't in the releases any more"))?;
        let info = UpdateInfo { TargetFullRelease: asset, BaseRelease: None, DeltasToTarget: vec![], IsDowngrade: false };
        um.download_updates(&info, None).map_err(|e| e.to_string())
    }

    pub fn apply(version: &str) -> Result<(), String> {
        let um = manager()?;
        let asset = um.get_update_pending_restart().filter(|a| a.Version == version).ok_or_else(|| format!("version {version} isn't downloaded"))?;
        um.wait_exit_then_apply_updates(&asset, false, false, Vec::<String>::new()).map_err(|e| format!("version {version} wasn't installed: {e}"))
    }

    /// The release fetched from a folder, kept for the install on close: the
    /// installer only finds a downloaded release newer than this one by
    /// itself, and going back to the shop version is older (UP21).
    static FETCHED: std::sync::Mutex<Option<(std::path::PathBuf, velopack::VelopackAsset)>> = std::sync::Mutex::new(None);

    fn folder_manager(folder: &std::path::Path) -> Result<UpdateManager, String> {
        let opts = velopack::UpdateOptions { AllowVersionDowngrade: true, ..Default::default() };
        UpdateManager::new(velopack::sources::FileSource::new(folder), Some(opts), None).map_err(|e| format!("this copy wasn't installed by Setup.exe: {e}"))
    }

    /// UP16, UP21: fetch release `version` from a release folder (the shop
    /// folder's `Updates\<version>`, or the admin PC's trial release), newer
    /// or older than this one.
    pub fn download_from(folder: &std::path::Path, version: &str) -> Result<(), String> {
        let um = folder_manager(folder)?;
        let feed = um.get_release_feed().map_err(|e| e.to_string())?;
        let asset = feed.Assets.into_iter().find(|a| a.Type.eq_ignore_ascii_case("Full") && a.Version == version).ok_or_else(|| format!("version {version} isn't in {}", folder.display()))?;
        let older = crate::shared::version_order(version, &um.get_current_version_as_string()) == Some(std::cmp::Ordering::Less);
        let info = UpdateInfo { TargetFullRelease: asset.clone(), BaseRelease: None, DeltasToTarget: vec![], IsDowngrade: older };
        um.download_updates(&info, None).map_err(|e| e.to_string())?;
        *FETCHED.lock().unwrap_or_else(|p| p.into_inner()) = Some((folder.to_path_buf(), asset));
        Ok(())
    }

    /// UP21: hand the release fetched from a folder over to install once
    /// the program has ended.
    pub fn apply_fetched(version: &str) -> Result<(), String> {
        let (folder, asset) = FETCHED.lock().unwrap_or_else(|p| p.into_inner()).take().filter(|(_, a)| a.Version == version).ok_or_else(|| format!("version {version} isn't downloaded"))?;
        let um = folder_manager(&folder)?;
        um.wait_exit_then_apply_updates(&asset, false, false, Vec::<String>::new()).map_err(|e| format!("version {version} wasn't installed: {e}"))
    }
}

/// Builds without the installer: nothing to fetch or install.
#[cfg(not(all(windows, target_env = "msvc")))]
mod installer {
    pub fn download(_version: &str) -> Result<(), String> {
        Err(super::NO_INSTALLER.into())
    }

    pub fn apply(_version: &str) -> Result<(), String> {
        Err(super::NO_INSTALLER.into())
    }

    pub fn download_from(_folder: &std::path::Path, _version: &str) -> Result<(), String> {
        Err(if cfg!(windows) { super::NO_INSTALLER } else { super::LINUX }.into())
    }

    pub fn apply_fetched(_version: &str) -> Result<(), String> {
        Err(super::NO_INSTALLER.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_a_plain_release_number() {
        // UP4: a release tag is v + this version, and only MAJOR.MINOR.PATCH
        // is released (tools/release-check.sh refuses anything else).
        let parts: Vec<&str> = VERSION.split('.').collect();
        assert_eq!(parts.len(), 3, "a release version is MAJOR.MINOR.PATCH, got {VERSION}");
        assert!(parts.iter().all(|p| p.parse::<u32>().is_ok()), "{VERSION}");
    }

    /// A test folder of its own, never a real Library.
    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nestpro_updates_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn fake(d: &Path, list: &str) -> Source {
        let f = d.join("releases.json");
        std::fs::write(&f, list).unwrap();
        Source::Fake(f)
    }

    const T: Duration = Duration::from_secs(5);

    #[test]
    fn a_pc_of_its_own_reads_the_release_list() {
        let d = tmp("own");
        let lib = d.join("Library");
        crate::library::create(&lib).unwrap();
        let src = fake(&d, r###"[{"version": "0.5.0", "notes": "## Changes machine programs\n\n- Leads (#1): app/src/leads.rs\n"}, {"version": "0.4.0"}]"###);
        let Found::Releases(l) = check(&lib, &d.join("elsewhere"), &src, T) else { panic!() };
        assert_eq!(l.len(), 2);
        let o = crate::engine::updates::offer("0.4.0", &l, None).unwrap();
        assert_eq!((o.version.as_str(), o.machine_changes.len()), ("0.5.0", 1));
        // A new install, whose own Library isn't made yet, isn't in a shop.
        let own = d.join("no Library yet");
        assert!(matches!(check(&own, &own, &src, T), Found::Releases(_)));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_pc_in_a_shop_never_reads_the_public_releases() {
        let d = tmp("shop");
        let lib = d.join("Library");
        crate::library::create(&lib).unwrap();
        let who = crate::shared::Who::new("sam", "PC-A");
        crate::engine::shop::start(&lib, "Acme", &who, &crate::shared::Clock::at(1_794_216_000), "0.4.0").unwrap();
        // The list would say "no connection" if it were read.
        let f = check(&lib, &d.join("elsewhere"), &Source::Fake(d.join("offline")), T);
        assert!(matches!(&f, Found::InShop(n) if n == "Acme"), "{f:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn no_connection_says_couldnt_check() {
        let d = tmp("offline");
        let lib = d.join("Library");
        crate::library::create(&lib).unwrap();
        let f = check(&lib, &d.join("elsewhere"), &Source::Fake(d.join("offline")), T);
        assert!(matches!(&f, Found::NotChecked(e) if e.starts_with(COULDNT_CHECK)), "{f:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_library_that_cant_be_reached_isnt_taken_for_no_shop() {
        // It may be a shop's (LB11): not checked, naming the folder.
        let d = tmp("unreached");
        let gone = d.join("srv").join("Nesting");
        let f = check(&gone, &d.join("own"), &fake(&d, r#"[{"version": "9.0.0"}]"#), T);
        assert!(matches!(&f, Found::NotChecked(e) if e.contains(&gone.display().to_string())), "{f:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_copy_without_releases_says_why_and_reads_nothing() {
        let f = check(Path::new("/nowhere"), Path::new("/nowhere"), &Source::Off(LINUX), T);
        assert!(matches!(&f, Found::NotChecked(e) if e == LINUX), "{f:?}");
        if !cfg!(windows) {
            assert_eq!(source(None), Source::Off(LINUX));
        }
    }

    /// A test Library of its own (no shop), under `d`.
    fn lib(d: &Path) -> PathBuf {
        let l = d.join("Library");
        crate::library::create(&l).unwrap();
        l
    }

    /// A handoff for one test (the window's own is a static).
    fn handoff() -> &'static Handoff {
        Box::leak(Box::new(Handoff::new()))
    }

    fn chosen(d: &Path, l: &Path, v: &str) -> Chosen {
        Chosen { version: v.into(), dir: l.to_path_buf(), own: d.join("own"), src: Source::Fake(d.join("releases.json")), why: Why::Public }
    }

    /// S1: closing while the chosen release still downloads waits for it
    /// (the window is already gone), then installs it.
    #[test]
    fn a_download_still_running_when_the_window_closes_is_waited_for() {
        let d = tmp("close_wait");
        let l = lib(&d);
        let h = handoff();
        let n = h.start(chosen(&d, &l, "0.2.0"));
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            h.done(n, true);
        });
        let mut applied = None;
        let r = finish_on_close(h, Duration::from_secs(5), |c| {
            applied = Some(c.version.clone());
            Ok(())
        });
        assert_eq!(r, Ok(Some("0.2.0".into())));
        assert_eq!(applied.as_deref(), Some("0.2.0"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S1: a download not done in time installs nothing (the next start
    /// offers it again); a failed one neither.
    #[test]
    fn a_download_not_ready_in_time_installs_nothing() {
        let d = tmp("close_late");
        let l = lib(&d);
        let h = handoff();
        h.start(chosen(&d, &l, "0.2.0"));
        let t = Instant::now();
        let r = finish_on_close(h, Duration::from_millis(80), |_| panic!("installed a release that wasn't downloaded"));
        assert_eq!(r, Ok(None));
        assert!(t.elapsed() >= Duration::from_millis(80));
        let n = h.start(chosen(&d, &l, "0.2.0"));
        h.done(n, false);
        assert_eq!(finish_on_close(h, Duration::from_secs(1), |_| panic!("installed a failed download")), Ok(None));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S2: a choice withdrawn (the Library changed or became a shop's)
    /// never installs, even when its download finishes afterwards.
    #[test]
    fn a_withdrawn_choice_is_never_installed() {
        let d = tmp("withdrawn");
        let l = lib(&d);
        let h = handoff();
        let n = h.start(chosen(&d, &l, "0.2.0"));
        h.withdraw();
        h.done(n, true);
        assert_eq!(h.take_ready(Duration::ZERO), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S2: once the window has closed, a restore (or a backup, a roll out or
    /// a roll back) still running finishes before the hand-off looks at the
    /// shop or installs anything.
    #[test]
    fn the_close_waits_for_a_restore_still_running() {
        for w in Work::ALL {
            the_close_waits_for(w);
        }
    }

    fn the_close_waits_for(w: Work) {
        use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
        let h = handoff();
        let mut guard = Some(h.begin(w));
        assert_eq!(h.working(), Some(w));
        let ended = std::sync::Arc::new(AtomicBool::new(false));
        let e2 = ended.clone();
        crate::shared::hooks::set(move |step| {
            if step == "close: waiting for a backup or restore" {
                // The restore's thread finishes now.
                e2.store(true, SeqCst);
                drop(guard.take());
            }
        });
        let e3 = ended.clone();
        let r = finish_on_close_with(h, Duration::ZERO, move |_| if e3.load(SeqCst) { Ok(None) } else { Err(NotStandalone::Moved(format!("looked at the shop while {w:?} ran"))) }, Duration::from_secs(5), |_| Ok(()), |_| panic!("installed"));
        crate::shared::hooks::clear();
        assert_eq!(r, Ok(None), "{w:?}");
        assert_eq!(h.working(), None);
    }

    /// S6: a PC of its own whose own Library folder isn't made yet (settings,
    /// no default Library) is offered a public release, downloads it, and
    /// installs it on close: the close looks at its shop as the offer and
    /// the download did.
    #[test]
    fn a_pc_without_its_library_folder_yet_installs_the_release_it_chose() {
        let d = tmp("no_own_folder");
        let own = d.join("own");
        let h = handoff();
        h.set_place(&own, &own);
        let c = Chosen { version: "0.2.0".into(), dir: own.clone(), own: own.clone(), src: Source::Fake(d.join("releases.json")), why: Why::Public };
        assert_eq!(c.stands(), Ok(()), "the download wouldn't start");
        let n = h.start(c);
        h.done(n, true);
        let mut applied = None;
        let r = finish_on_close(h, Duration::from_secs(5), |c| {
            applied = Some(c.version.clone());
            Ok(())
        });
        assert_eq!(r, Ok(Some("0.2.0".into())));
        assert_eq!(applied.as_deref(), Some("0.2.0"));
        assert!(!own.exists(), "the close made a Library");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S1: withdrawing a public choice leaves a trial's (or the shop's)
    /// release to install, and the other way round.
    #[test]
    fn withdrawing_one_kind_of_update_leaves_the_other() {
        let d = tmp("withdraw_for");
        let l = lib(&d);
        let h = handoff();
        let mut c = chosen(&d, &l, "0.2.0");
        c.why = Why::Trial;
        let n = h.start(c);
        h.withdraw_for(Why::Public);
        h.done(n, true);
        assert_eq!(h.take_ready(Duration::ZERO).map(|c| c.version), Some("0.2.0".into()));
        let n = h.start(chosen(&d, &l, "0.3.0"));
        h.withdraw_for(Why::Trial);
        h.done(n, true);
        assert_eq!(h.take_ready(Duration::ZERO).map(|c| c.version), Some("0.3.0".into()));
        h.start(chosen(&d, &l, "0.3.0"));
        h.withdraw_for(Why::Public);
        assert_eq!(h.take_ready(Duration::ZERO), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S2: a PC that joined a shop after choosing installs nothing on close,
    /// and a download asked for once it is in a shop never starts.
    #[test]
    fn a_pc_in_a_shop_never_downloads_or_installs_a_public_release() {
        let d = tmp("joined");
        let l = lib(&d);
        let h = handoff();
        let n = h.start(chosen(&d, &l, "0.2.0"));
        h.done(n, true);
        let who = crate::shared::Who::new("sam", "PC-A");
        crate::engine::shop::start(&l, "Acme", &who, &crate::shared::Clock::at(1_794_216_000), "0.1.0").unwrap();
        let r = finish_on_close(h, Duration::from_secs(1), |_| panic!("installed a public release in a shop"));
        assert!(matches!(&r, Err(e) if e.contains("Acme")), "{r:?}");
        let rx = download_in_background(h, chosen(&d, &l, "0.2.0"));
        let r = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(&r, Err(e) if e.contains("Acme")), "{r:?}");
        assert_eq!(h.take_ready(Duration::ZERO), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    fn gh_release(tag: &str, pre: bool, index: bool) -> serde_json::Value {
        let assets = if index { serde_json::json!([{"name": "Setup.exe", "browser_download_url": "x"}, {"name": github::INDEX, "browser_download_url": format!("idx/{tag}")}]) } else { serde_json::json!([]) };
        serde_json::json!({"tag_name": tag, "draft": false, "prerelease": pre, "assets": assets})
    }

    fn gh_index(v: &str, notes: &str) -> String {
        serde_json::json!({"Assets": [{"PackageId": "KurfLabs.nestpro", "Version": v, "Type": "Full", "FileName": format!("x-{v}-full.nupkg"), "NotesMarkdown": notes}, {"Version": v, "Type": "Delta"}]}).to_string()
    }

    /// S3: GitHub's list is read through each release's index; pre-releases
    /// and releases without an index are left out.
    #[test]
    fn github_releases_are_read_from_each_index() {
        let get = |url: &str, _: Instant| -> Result<String, String> {
            match url {
                github::RELEASES => Ok(serde_json::json!([gh_release("v0.3.0-rc1", true, true), gh_release("v0.2.0", false, true), gh_release("v0.1.5", false, false), gh_release("v0.1.0", false, true)]).to_string()),
                "idx/v0.2.0" => Ok(gh_index("0.2.0", "## Changes machine programs\n\n- Leads (#1): app/src/leads.rs\n")),
                "idx/v0.1.0" => Ok(gh_index("0.1.0", "")),
                u => Err(format!("asked for {u}")),
            }
        };
        let l = github::release_list(Instant::now() + T, &get).unwrap();
        let vs: Vec<&str> = l.iter().map(|r| r.version.as_str()).collect();
        assert_eq!(vs, ["0.2.0", "0.1.0"]);
        assert_eq!(crate::engine::updates::offer("0.1.0", &l, None).unwrap().machine_changes.len(), 1);
    }

    /// S3: an index that can't be fetched or read fails the check, so the
    /// card says "Couldn't check for updates" instead of offering an older
    /// release as the newest.
    #[test]
    fn an_index_that_cant_be_read_fails_the_check() {
        for bad in [Err("GitHub answered 502".to_string()), Ok("<html>".to_string()), Ok("{}".to_string())] {
            let get = |url: &str, _: Instant| -> Result<String, String> {
                match url {
                    github::RELEASES => Ok(serde_json::json!([gh_release("v0.2.0", false, true), gh_release("v0.1.0", false, true)]).to_string()),
                    "idx/v0.2.0" => bad.clone(),
                    _ => Ok(gh_index("0.1.0", "")),
                }
            };
            let r = github::release_list(Instant::now() + T, &get);
            assert!(matches!(&r, Err(e) if e.contains("v0.2.0")), "{r:?}");
        }
    }

    /// S4: the whole check has the time limit, the shop lookup included, and
    /// no request starts once it has passed.
    #[test]
    fn a_stalled_shop_lookup_counts_as_no_connection_and_asks_github_nothing() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let asked = Arc::new(AtomicBool::new(false));
        let a = asked.clone();
        let t = Instant::now();
        let f = check_with(
            || {
                std::thread::sleep(Duration::from_millis(400));
                Ok(())
            },
            move |_| {
                a.store(true, Ordering::SeqCst);
                Ok(vec![])
            },
            Duration::from_millis(100),
        );
        assert!(matches!(&f, Found::NotChecked(e) if e.starts_with(COULDNT_CHECK)), "{f:?}");
        assert!(t.elapsed() < Duration::from_millis(350), "waited for the lookup: {:?}", t.elapsed());
        std::thread::sleep(Duration::from_millis(500));
        assert!(!asked.load(Ordering::SeqCst), "asked GitHub after the check had expired");
        // Each request gets what is left of the check, and none starts late.
        assert_eq!(github::get(github::RELEASES, Instant::now() - Duration::from_secs(1)), Err("no answer in time".into()));
        let deadline = Instant::now() + T;
        let get = |_: &str, d: Instant| -> Result<String, String> {
            assert_eq!(d, deadline);
            Ok("[]".into())
        };
        assert_eq!(github::release_list(deadline, &get), Ok(vec![]));
    }

    /// S1: the shop lookup on close has a time limit: a share that stalls
    /// installs nothing, and the program still ends.
    #[test]
    fn a_stalled_share_on_close_installs_nothing_and_doesnt_hang() {
        let d = tmp("close_stall");
        let l = lib(&d);
        let h = handoff();
        let n = h.start(chosen(&d, &l, "0.2.0"));
        h.done(n, true);
        let t = Instant::now();
        let stall = |q: Option<Chosen>| {
            std::thread::sleep(Duration::from_millis(400));
            Ok(q.map(|c| (c, true)))
        };
        let r = finish_on_close_with(h, Duration::from_secs(1), stall, Duration::from_millis(100), |_| panic!("fetched without knowing whether this PC is in a shop"), |_| panic!("installed without knowing whether this PC is in a shop"));
        assert!(matches!(&r, Err(e) if e.contains("didn't answer")), "{r:?}");
        assert!(t.elapsed() < Duration::from_millis(350), "waited for the share: {:?}", t.elapsed());
        let _ = std::fs::remove_dir_all(&d);
    }
}
