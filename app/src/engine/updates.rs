//! What a PC is offered to update to (spec #178, UP11–UP14). Plain functions
//! over this PC's version, a list of releases and the shop file: the window's
//! banner and Updates card, and the agents' `shop mode=updates`, call them.
//! They never fetch or install anything: the release list comes from the
//! installer module (`crate::updates`), or a fake list in tests and
//! screenshots.

use crate::shop::ShopFile;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// One release in the list: its version and its release notes (the notes
/// `tools/release-notes.sh` writes, UP1).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    #[serde(default)]
    pub notes: String,
}

/// UP12: the release a PC is offered, with what it changes in machine
/// programs (UP13).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub version: String,
    /// The entries of the notes' "Changes machine programs" section: empty
    /// when it has none (it says "None", or the notes have no such section).
    pub machine_changes: Vec<String>,
    pub notes: String,
}

/// The heading of the notes' section on machine programs (UP1).
pub const MACHINE_SECTION: &str = "Changes machine programs";

/// UP12: a release version, `MAJOR.MINOR.PATCH` and nothing else (UP4): a
/// pre-release (`0.5.0-rc1`), a build (`0.5.0+x`) or a short one (`0.5`) is
/// none. A leading `v` (the tag's) is allowed.
pub fn plain_version(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim();
    let v = v.strip_prefix('v').unwrap_or(v);
    let mut it = v.split('.');
    let mut n = || -> Option<u64> {
        let p = it.next()?;
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        p.parse().ok()
    };
    let out = (n()?, n()?, n()?);
    it.next().is_none().then_some(out)
}

/// UP12: the newest release in the list, compared number by number (0.10.0
/// is newer than 0.9.0). Pre-releases and anything else that isn't a plain
/// release are skipped.
pub fn newest(releases: &[Release]) -> Option<&Release> {
    releases.iter().filter_map(|r| plain_version(&r.version).map(|v| (v, r))).max_by_key(|(v, _)| *v).map(|(_, r)| r)
}

/// UP12: what a PC on `this` version is offered from `releases`: the newest
/// plain release newer than `this`, or nothing when it is up to date, the
/// list is empty or holds only older releases. A PC in a shop (`shop` given)
/// is offered nothing from this list: it updates from the shop folder, to the
/// release the admin PC rolled out (ADR 0006).
pub fn offer(this: &str, releases: &[Release], shop: Option<&ShopFile>) -> Option<Offer> {
    if shop.is_some() {
        return None;
    }
    let mine = plain_version(this)?;
    let r = newest(releases)?;
    (plain_version(&r.version)? > mine).then(|| Offer { version: r.version.trim().trim_start_matches('v').to_string(), machine_changes: machine_changes(&r.notes), notes: r.notes.clone() })
}

/// UP11: the shop this PC's Library (`dir`) belongs to, to know whether it
/// checks GitHub: None for a Library that is no shop, and for this PC's own
/// Library folder (`own`) when it is certainly not there yet (a new
/// install). Any other folder that can't be reached, and this PC's own
/// folder when it can't be told whether it is there, is refused, naming it
/// (LB11): it may be a shop's, so it is never taken for none.
pub fn shop_of(dir: &Path, own: &Path) -> Result<Option<ShopFile>, String> {
    if dir == own {
        if let Err(e) = std::fs::metadata(dir) {
            if e.kind() == std::io::ErrorKind::NotFound {
                return Ok(None);
            }
            return Err(format!("{} ({e})", crate::library::unreachable(dir)));
        }
    }
    crate::shop::read(dir)
}

/// UP21: what a PC in a shop installs, from the shop folder (never GitHub).
#[derive(Clone, Debug, PartialEq)]
pub enum FromShop {
    /// Its Library isn't a shop's.
    NotInShop,
    /// Its Library is a trial copy (UP16): the release tried installs, not
    /// the shop's.
    Trial(crate::shop::TrialMark),
    /// The shop has no shop version yet: nothing to install.
    NoVersion,
    /// This PC runs the shop version.
    UpToDate(String),
    /// This PC runs another version than the shop's: `version` installs from
    /// `folder` (the shop folder's `Updates\<version>`) when the program
    /// closes. `back`: this PC is newer, and goes back to it (UP21).
    Update { version: String, folder: std::path::PathBuf, back: bool },
    /// The shop version's packages aren't in `Updates`: why.
    Missing { version: String, why: String },
}

/// UP21: what this PC (on `this`) installs from the shop in `dir`. A shop
/// folder that can't be reached is refused, naming it (LB11).
pub fn from_shop(dir: &Path, this: &str) -> Result<FromShop, String> {
    let Some(s) = crate::shop::read(dir)? else { return Ok(FromShop::NotInShop) };
    if let Some(m) = s.trial() {
        return Ok(FromShop::Trial(m));
    }
    let Some(v) = s.shop_version().map(String::from) else { return Ok(FromShop::NoVersion) };
    let order = crate::shared::version_order(this, &v);
    if order == Some(std::cmp::Ordering::Equal) {
        return Ok(FromShop::UpToDate(v));
    }
    let folder = super::rollout::release_dir(dir, &v);
    Ok(match super::rollout::pack_in(&folder, &v) {
        Ok(_) => FromShop::Update { back: order == Some(std::cmp::Ordering::Greater), version: v, folder },
        Err(why) => FromShop::Missing { version: v, why },
    })
}

impl FromShop {
    /// UP21: what it says, for agents and the Updates card.
    pub fn said(&self, this: &str, admin: &str) -> String {
        match self {
            FromShop::NotInShop => format!("This PC runs {this}; its Library isn't a shop's."),
            FromShop::Trial(m) => format!("This PC is trying {} on a copy of the shop (nothing it saves reaches the shop); it runs {this}.", m.version),
            FromShop::NoVersion => format!("This PC runs {this}. The shop has no shop version yet: the admin PC ({admin}) rolls one out."),
            FromShop::UpToDate(v) => format!("This PC runs the shop version, {v}."),
            FromShop::Update { version, back: false, .. } => format!("This PC runs {this}; the shop is on {version}. {UPDATE_READY}: {version} installs from the shop folder when the program closes, never while it is open."),
            FromShop::Update { version, back: true, .. } => format!("This PC runs {this}, newer than the shop version {version}: the shop is read-only on it. Restart to go back to {version}: it installs from the shop folder when the program closes."),
            FromShop::Missing { version, why } => format!("This PC runs {this}; the shop is on {version}, but its packages aren't in the shop folder's Updates ({why}). The admin PC ({admin}) rolls it out again."),
        }
    }
}

/// UP21: the banner on a PC whose shop version is ready to install.
pub const UPDATE_READY: &str = "Update ready, restart when you're done";

/// UP13: the entries of the notes' "Changes machine programs" section, one
/// per change: the list lines under that heading, up to the next heading.
/// Empty when the section says "None" or the notes have no such section.
pub fn machine_changes(notes: &str) -> Vec<String> {
    let mut out = vec![];
    let mut inside = false;
    for line in notes.lines() {
        let t = line.trim();
        if let Some(h) = t.strip_prefix('#') {
            inside = h.trim_start_matches('#').trim().eq_ignore_ascii_case(MACHINE_SECTION);
            continue;
        }
        if !inside {
            continue;
        }
        if let Some(e) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            let e = e.trim();
            if !e.is_empty() {
                out.push(e.to_string());
            }
        }
    }
    out
}

/// UP12: what the Updates card says when nothing is offered.
pub fn up_to_date(this: &str, newest: Option<&str>) -> String {
    match newest {
        None => "no release is published yet.".into(),
        Some(n) if plain_version(n) == plain_version(this) => "up to date.".into(),
        Some(n) => format!("up to date (the newest release is {n})."),
    }
}

/// UP11, UP12: the agents' `shop mode=updates`: this PC's version, and for
/// a PC of its own the newest release and what the window offers, with what
/// it changes in machine programs. Installing stays a person's choice, in
/// the window (the banner's Update when I close).
pub fn run(ctx: &super::Ctx) -> super::ops::Outcome {
    use crate::updates::{check, Found, CHECK_TIMEOUT};
    use serde_json::{json, Value};
    let this = &ctx.version;
    // UP21: a PC in a shop says what it installs from the shop folder,
    // whatever its copy's release source (on Linux, none).
    let found = match shop_of(&ctx.library_dir, &crate::library::default_dir()) {
        Ok(Some(s)) => Found::InShop(s.name),
        _ => check(&ctx.library_dir, &crate::library::default_dir(), &ctx.releases, CHECK_TIMEOUT),
    };
    let (msg, data) = match found {
        Found::InShop(name) => {
            // UP21: what it installs from the shop folder.
            let admin = crate::shop::read(&ctx.library_dir).ok().flatten().map(|s| s.admin.pc).unwrap_or_default();
            let (said, shop_version, from) = match from_shop(&ctx.library_dir, this) {
                Ok(f) => {
                    let sv = match &f {
                        FromShop::UpToDate(v) | FromShop::Update { version: v, .. } | FromShop::Missing { version: v, .. } => Some(v.clone()),
                        FromShop::Trial(m) => m.shop_version.clone(),
                        _ => None,
                    };
                    let from = match &f {
                        FromShop::Update { version, folder, back } => json!({"version": version, "folder": folder.display().to_string(), "back": back, "installs": "when the program closes"}),
                        _ => Value::Null,
                    };
                    (f.said(this, &admin), sv, from)
                }
                Err(e) => (e, None, Value::Null),
            };
            (
                format!("This PC runs {this}. It's in shop {name}, so it updates from the shop folder to the release the admin PC rolls out, never from GitHub. {said}"),
                json!({"this_version": this, "shop": name, "checked": false, "newest": null, "offer": null, "shop_version": shop_version, "from_shop": from}),
            )
        }
        Found::NotChecked(why) => (format!("This PC runs {this}. {why}"), json!({"this_version": this, "checked": false, "reason": why, "newest": null, "offer": null})),
        Found::Releases(list) => {
            let newest = newest(&list).map(|r| r.version.clone());
            match offer(this, &list, None) {
                Some(o) => {
                    let mp = if o.machine_changes.is_empty() { String::new() } else { format!(" It changes machine programs ({}): test-cut before using it.", o.machine_changes.len()) };
                    (
                        format!("This PC runs {this}; version {} is out.{mp} A person installs it in the window (Update when I close); agents don't.", o.version),
                        json!({"this_version": this, "checked": true, "newest": newest, "offer": {"version": o.version, "machine_changes": o.machine_changes, "notes": o.notes}}),
                    )
                }
                None => (
                    format!("This PC runs {this}: {}", up_to_date(this, newest.as_deref())),
                    json!({"this_version": this, "checked": true, "newest": newest, "offer": null}),
                ),
            }
        }
    };
    super::ops::Outcome::read(msg, data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::{Saved, Who};
    use crate::shop::{Admin, SettingsBy};

    fn rel(v: &str) -> Release {
        Release { version: v.into(), notes: String::new() }
    }

    fn list(vs: &[&str]) -> Vec<Release> {
        vs.iter().map(|v| rel(v)).collect()
    }

    /// Notes as `tools/release-notes.sh` writes them (UP1).
    const NOTES: &str = "Since v0.4.0: 3 changes.\n\n## Changes machine programs\n\nTest-cut before rolling this release out: these change the code that writes machine programs.\n\n- Arc leads on round holes aim at the centre (#177): app/src/leads.rs\n- Stop cut-offs on the BLM (#169): app/src/blm.rs, app/src/cnc.rs\n\n## Changes\n\n- Arc leads on round holes aim at the centre (#177)\n- Stop cut-offs on the BLM (#169)\n- Settings → Shop (#200)\n";
    const NOTES_NONE: &str = "Since v0.4.0: 1 change.\n\n## Changes machine programs\n\nNone: no change in this release touches the code that writes machine programs.\n\n## Changes\n\n- Settings → Shop (#200)\n";

    fn acme() -> ShopFile {
        let who = Who::new("sam", "PC-A");
        ShopFile { name: "Acme".into(), admin: Admin { pc: "PC-A".into(), by: who.clone(), at: 0, was: None }, settings_by: SettingsBy::AdminPc, started: Saved { by: who, at: 0 }, backups: Default::default(), restored: None, other: Default::default() }
    }

    #[test]
    fn offers_the_newest_newer_release() {
        let o = offer("0.4.0", &list(&["0.4.1", "0.5.0", "0.3.0"]), None).expect("an offer");
        assert_eq!(o.version, "0.5.0");
    }

    #[test]
    fn nothing_when_the_list_is_empty() {
        assert_eq!(offer("0.4.0", &[], None), None);
    }

    #[test]
    fn nothing_when_only_older_releases() {
        assert_eq!(offer("0.4.0", &list(&["0.3.9", "0.1.0"]), None), None);
    }

    #[test]
    fn nothing_when_up_to_date() {
        assert_eq!(offer("0.4.0", &list(&["0.4.0", "0.3.0"]), None), None);
    }

    #[test]
    fn versions_compare_number_by_number() {
        // 0.10.0 is newer than 0.9.0, though "0.10.0" < "0.9.0" as text.
        assert_eq!(offer("0.9.0", &list(&["0.9.0", "0.10.0"]), None).map(|o| o.version), Some("0.10.0".into()));
        assert_eq!(offer("0.10.0", &list(&["0.9.0", "0.10.0"]), None), None);
        assert_eq!(newest(&list(&["0.9.0", "0.10.0", "0.2.0"])).map(|r| r.version.as_str()), Some("0.10.0"));
    }

    #[test]
    fn a_pre_release_is_never_offered() {
        // UP4: only MAJOR.MINOR.PATCH is a release.
        let l = list(&["0.5.0-rc1", "0.6.0-beta.2", "0.4.1", "0.7", "v0.4.2+build"]);
        assert_eq!(offer("0.4.0", &l, None).map(|o| o.version), Some("0.4.1".into()));
        assert_eq!(offer("0.4.1", &list(&["0.5.0-rc1"]), None), None);
        assert_eq!(newest(&list(&["0.5.0-rc1"])), None);
    }

    #[test]
    fn a_pc_in_a_shop_is_offered_nothing_from_the_public_releases() {
        // ADR 0006: it updates to what the admin PC rolled out.
        assert_eq!(offer("0.4.0", &list(&["0.5.0"]), Some(&acme())), None);
    }

    /// S5: this PC's own Library folder counts as "not made yet" only when
    /// it is certainly not there; one that can't be looked at may be a
    /// shop's.
    #[test]
    fn an_own_library_that_cant_be_looked_at_isnt_taken_for_none() {
        let d = crate::testdir::TestDir::made("shop_of");
        let missing = d.join("Library");
        assert_eq!(shop_of(&missing, &missing), Ok(None));
        // Looking at it fails, but not with "not found": under a file on
        // Linux, a name Windows can't have on Windows.
        std::fs::write(d.join("file"), "x").unwrap();
        let odd = if cfg!(windows) { d.join("Lib<rary>") } else { d.join("file").join("Library") };
        let e = std::fs::metadata(&odd).unwrap_err();
        assert_ne!(e.kind(), std::io::ErrorKind::NotFound, "{e}");
        assert!(shop_of(&odd, &odd).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn plain_versions() {
        assert_eq!(plain_version("0.10.2"), Some((0, 10, 2)));
        assert_eq!(plain_version("v1.2.3"), Some((1, 2, 3)));
        for bad in ["", "1.2", "1.2.3.4", "1.2.3-rc1", "1.2.3+b", "a.b.c", "1..3", "-1.2.3"] {
            assert_eq!(plain_version(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_offer_carries_the_machine_program_changes() {
        let mut l = list(&["0.4.1"]);
        l.push(Release { version: "0.5.0".into(), notes: NOTES.into() });
        let o = offer("0.4.0", &l, None).unwrap();
        assert_eq!(o.machine_changes, vec!["Arc leads on round holes aim at the centre (#177): app/src/leads.rs".to_string(), "Stop cut-offs on the BLM (#169): app/src/blm.rs, app/src/cnc.rs".to_string()]);
        assert_eq!(o.notes, NOTES);
    }

    #[test]
    fn a_section_that_says_none_has_no_entries() {
        assert!(machine_changes(NOTES_NONE).is_empty());
        let o = offer("0.4.0", &[Release { version: "0.5.0".into(), notes: NOTES_NONE.into() }], None).unwrap();
        assert!(o.machine_changes.is_empty());
    }

    #[test]
    fn notes_without_the_section_have_no_entries() {
        assert!(machine_changes("## Changes\n\n- Settings → Shop (#200)\n").is_empty());
        assert!(machine_changes("").is_empty());
        let o = offer("0.4.0", &[rel("0.5.0")], None).unwrap();
        assert!(o.machine_changes.is_empty());
    }
}
