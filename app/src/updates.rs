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
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

/// The public releases a PC of its own updates from (read by the installer,
/// in Windows builds made with MSVC).
#[cfg_attr(not(all(windows, target_env = "msvc")), allow(dead_code))]
pub const REPO: &str = "https://github.com/KurfLabs/nestpro";

/// UP11: a check that hasn't answered by then counts as no connection.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(30);

/// UP11: a PC of its own checks when the program opens and then once a day.
pub const CHECK_EVERY: Duration = Duration::from_secs(24 * 3600);

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
    /// GitHub Releases, through the installer (Windows builds made with MSVC).
    Github,
    /// No releases on this copy, and why (UP14: on Linux, that releases are
    /// for Windows).
    Off(&'static str),
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

/// The release list, read now (it can take seconds: run it off the window's
/// thread, `check_in_background`).
pub fn release_list(src: &Source) -> Result<Vec<Release>, String> {
    match src {
        Source::Fake(p) => {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{COULDNT_CHECK} ({e})."))?;
            serde_json::from_str(&text).map_err(|e| format!("{COULDNT_CHECK} ({e})."))
        }
        Source::Github => installer::release_list().map_err(|e| format!("{COULDNT_CHECK} ({e}).")),
        Source::Off(why) => Err(why.to_string()),
    }
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

/// UP11: check for releases: first whether this PC's Library (`dir`) is a
/// shop's, then the release list, waiting at most `timeout`. A check that
/// takes longer counts as no connection.
/// `own` is this PC's own Library folder (`Documents\Sheet Nesting\Library`):
/// while it isn't made yet, this PC is in no shop.
pub fn check(dir: &Path, own: &Path, src: &Source, timeout: Duration) -> Found {
    if let Source::Off(why) = src {
        return Found::NotChecked(why.to_string());
    }
    match crate::engine::updates::shop_of(dir, own) {
        Ok(Some(s)) => return Found::InShop(s.name),
        Err(e) => return Found::NotChecked(format!("Not checked: this PC's Library can't be reached, so it isn't known whether this PC is in a shop. {e}")),
        Ok(None) => {}
    }
    let (tx, rx) = channel();
    let src = src.clone();
    std::thread::spawn(move || {
        let _ = tx.send(release_list(&src));
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(list)) => Found::Releases(list),
        Ok(Err(e)) => Found::NotChecked(e),
        Err(_) => Found::NotChecked(format!("{COULDNT_CHECK} (no answer in {} s).", timeout.as_secs())),
    }
}

/// UP11: `check`, off the window's thread: the answer comes on the receiver.
pub fn check_in_background(dir: PathBuf, own: PathBuf, src: Source) -> Receiver<Found> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(check(&dir, &own, &src, CHECK_TIMEOUT));
    });
    rx
}

/// UP14: Update when I close: fetch `version` in the background. When it's
/// fetched, it installs when the program closes (`install_on_close`), never
/// before. A made-up list fetches nothing.
pub fn download_in_background(src: Source, version: String) -> Receiver<Result<(), String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let r = match &src {
            Source::Fake(_) => Ok(()),
            Source::Github => installer::download(&version).map_err(|e| format!("Couldn't download version {version} ({e}).")),
            Source::Off(why) => Err(why.to_string()),
        };
        let _ = tx.send(r);
    });
    rx
}

/// UP14: the program has closed (its window is gone, and every job with it):
/// install the update fetched by Update when I close, if any. The installer
/// waits for this program to end, then installs; the program isn't started
/// again by itself.
pub fn install_on_close() {
    installer::install_on_close();
}

/// The installer's own calls (Velopack, docs/releases.md), in Windows builds
/// made with MSVC.
#[cfg(all(windows, target_env = "msvc"))]
mod installer {
    use super::Release;
    use std::sync::Mutex;
    use velopack::{sources::GithubSource, UpdateInfo, UpdateManager, VelopackAsset};

    /// The release fetched by Update when I close, to install on close.
    static FETCHED: Mutex<Option<VelopackAsset>> = Mutex::new(None);

    fn manager() -> Result<UpdateManager, String> {
        // Pre-releases are never offered (UP4).
        UpdateManager::new(GithubSource::new(super::REPO, None, false), None, None).map_err(|e| format!("this copy wasn't installed by Setup.exe: {e}"))
    }

    fn full_releases(um: &UpdateManager) -> Result<Vec<VelopackAsset>, String> {
        let feed = um.get_release_feed().map_err(|e| e.to_string())?;
        Ok(feed.Assets.into_iter().filter(|a| a.Type.eq_ignore_ascii_case("Full")).collect())
    }

    pub fn release_list() -> Result<Vec<Release>, String> {
        Ok(full_releases(&manager()?)?.into_iter().map(|a| Release { version: a.Version, notes: a.NotesMarkdown }).collect())
    }

    pub fn download(version: &str) -> Result<(), String> {
        let um = manager()?;
        let asset = full_releases(&um)?.into_iter().find(|a| a.Version == version).ok_or_else(|| format!("version {version} isn't in the releases any more"))?;
        let info = UpdateInfo { TargetFullRelease: asset.clone(), BaseRelease: None, DeltasToTarget: vec![], IsDowngrade: false };
        um.download_updates(&info, None).map_err(|e| e.to_string())?;
        *FETCHED.lock().unwrap_or_else(|p| p.into_inner()) = Some(asset);
        Ok(())
    }

    pub fn install_on_close() {
        let Some(asset) = FETCHED.lock().unwrap_or_else(|p| p.into_inner()).take() else { return };
        if let Ok(um) = manager() {
            if let Err(e) = um.wait_exit_then_apply_updates(&asset, false, false, Vec::<String>::new()) {
                eprintln!("update {}: {e}", asset.Version);
            }
        }
    }
}

/// Builds without the installer: nothing to fetch or install.
#[cfg(not(all(windows, target_env = "msvc")))]
mod installer {
    use super::Release;

    pub fn release_list() -> Result<Vec<Release>, String> {
        Err(super::NO_INSTALLER.into())
    }

    pub fn download(_version: &str) -> Result<(), String> {
        Err(super::NO_INSTALLER.into())
    }

    pub fn install_on_close() {}
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
}
