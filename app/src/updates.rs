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
}
