//! Trying a release on the admin PC and rolling it out to the shop (spec
//! #178, ADR 0006, UP15–UP26). Plain functions over a shop folder and a
//! release folder: Settings → Shop → Updates calls them, and the agents'
//! `shop` operation reads what they read (rolling out is refused for
//! agents). They never call the installer: a release folder is a folder of
//! files (the installer's index and package), so they are tested on any
//! system against a made-up one.
//!
//! - A **release folder** holds a release's packages as the installer packs
//!   them: its index (`releases.win.json`) and its full package. The admin
//!   PC fetches one from GitHub into a folder on this PC; the shop keeps the
//!   releases it rolled out in `Updates\<version>\`.
//! - **Roll out** (UP19) copies the release this PC runs into `Updates\`,
//!   makes the release's one-time changes to the shop's files (UP18) and
//!   makes it the shop version, keeping the previous one beside it.
//! - **A trial** (UP16) works on a copy of the shop made from a fresh
//!   backup, in a folder on this PC: nothing it does reaches the shop.
//! - **Roll back** (UP23–UP26) makes the release kept beside the shop
//!   version the shop version again, when it can read every file layout the
//!   shop records; files the release left wrote count as the shop
//!   version's from then on.

use crate::backup;
use crate::engine::updates::plain_version;
use crate::shared::{self, Clock, Versions, Who, WriteLock};
use crate::shop::{self, PcRecord, ShopFile, TrialMark};
use serde_json::{json, Value};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The installer's index in a release folder (the Windows channel).
pub const INDEX: &str = "releases.win.json";

/// UP19: why an agent can't roll out.
pub const AGENT_ROLLOUT: &str = "Rolling out is refused for agents: it changes the release every PC in the shop installs, so a person does it on the admin PC, in the window (Settings → Shop → Updates → Roll out to the shop), after trying the release there and reading the warning. The shop version and the shop folder are as they were. Agents can read the shop version and each PC's version with `shop`.";

// ---------------------------------------------------------------------------
// File layouts (UP20): the layout number each kind of Library file is
// written in by this program. A release that raises one writes files older
// programs can't read at all.
// ---------------------------------------------------------------------------

/// UP20: the file layouts this program writes, by kind of file.
pub fn layouts() -> BTreeMap<String, u32> {
    BTreeMap::from([
        ("jobs".to_string(), crate::jobfile::VERSION),
        ("library".to_string(), crate::library::VERSION),
        ("materials".to_string(), crate::engine::material::VERSION),
        ("stock".to_string(), crate::stock::LAYOUT),
    ])
}

// ---------------------------------------------------------------------------
// One-time changes (UP18): a release's changes to the shop's files (such as
// moving the machines into the shop). Each is tied to the release that
// introduces it, runs on the trial copy during a trial and on the shop only
// at roll out, under the Library's write lock, and is recorded in the shop
// file so it never runs twice.
// ---------------------------------------------------------------------------

/// UP18: one one-time change to the shop's files.
pub struct OneTimeChange {
    /// Its name, recorded in the shop file once made.
    pub name: &'static str,
    /// The release that introduces it.
    pub release: &'static str,
    /// What it does, in shop words ("Moves the machines into the shop").
    pub what: &'static str,
    /// Make it in the shop folder (or a trial copy), under the write lock.
    /// It must be safe to run again: a change is recorded only after it has
    /// run, so one stopped between its effects and its record (a lost lock,
    /// a share that dropped, the program ended) runs again on the next try,
    /// and must then find its own effects and finish, not repeat them.
    pub run: fn(&Path, &WriteLock) -> Result<(), String>,
}

/// UP18: the program's one-time changes, oldest release first. None yet:
/// spec #179 adds the first (the machines into the shop).
pub const CHANGES: &[OneTimeChange] = &[];

/// UP18: the changes of `list` a shop on `from` (None: no release rolled
/// out yet) going to `to` needs: those of releases newer than `from` up to
/// `to`, not made yet (`done`), oldest release first.
pub fn pending<'a>(list: &'a [OneTimeChange], from: Option<&str>, to: &str, done: &[String]) -> Vec<&'a OneTimeChange> {
    let to = plain_version(to);
    let from = from.and_then(plain_version);
    let mut out: Vec<&OneTimeChange> = list
        .iter()
        .filter(|c| !done.iter().any(|d| d == c.name))
        .filter(|c| plain_version(c.release).is_some_and(|r| Some(r) <= to && from.is_none_or(|f| r > f)))
        .collect();
    out.sort_by_key(|c| plain_version(c.release));
    out
}

/// UP18: make `todo` in `dir` (a shop folder or a trial copy), under `lock`,
/// recording each in its shop file as soon as it is made. Stops at the first
/// that fails, saying which. Gives what was made.
fn make_changes(dir: &Path, todo: &[&OneTimeChange], lock: &WriteLock, version: &str, judged: &Versions) -> Result<Vec<String>, String> {
    let mut made = vec![];
    for c in todo {
        lock.still_held()?;
        (c.run)(dir, lock).map_err(|e| format!("the one-time change \"{}\" ({}) failed: {e}", c.what, c.release))?;
        shared::pause("rollout: change made");
        let mut s = shop::read(dir)?.ok_or_else(|| format!("{} holds no shop file any more", dir.display()))?;
        let mut done = s.changes_made();
        done.push(c.name.to_string());
        s.other.insert(shop::CHANGES_MADE.into(), json!(done));
        shop::write_judged(dir, &s, lock, version, judged)?;
        made.push(c.what.to_string());
    }
    Ok(made)
}

// ---------------------------------------------------------------------------
// Release folders (UP19): the installer's index and a release's full
// package, as the release workflow packs them.
// ---------------------------------------------------------------------------

/// A release's packages in a release folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Pack {
    pub version: String,
    /// The full package's file name, in the same folder.
    pub package: String,
    /// The index with only this release's full package in it.
    pub index: String,
    pub notes: String,
    /// The package's size and checksums, as the index gives them.
    pub size: Option<u64>,
    pub sha256: Option<String>,
    pub sha1: Option<String>,
}

/// A file's checksum, in lower-case hex.
fn digest<D: sha2::Digest>(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|e| format!("{} can't be read ({e})", path.display()))?;
    let mut h = D::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => h.update(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(format!("{} can't be read ({e})", path.display())),
        }
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// UP19: the package in `folder` is whole: its size and checksum are those
/// the release's index gives (SHA-256, or SHA-1 when that is all it gives).
/// An index that gives no checksum can't be checked, and is refused.
pub fn verify(folder: &Path, pack: &Pack) -> Result<(), String> {
    let p = folder.join(&pack.package);
    let len = std::fs::metadata(&p).map_err(|e| format!("{} can't be read ({e})", p.display()))?.len();
    if let Some(size) = pack.size.filter(|s| *s != len) {
        return Err(format!("{} is {len} bytes, but its index says {size}: it is incomplete or damaged", p.display()));
    }
    let (want, got) = match (&pack.sha256, &pack.sha1) {
        (Some(w), _) => (w, digest::<sha2::Sha256>(&p)?),
        (None, Some(w)) => (w, digest::<sha1::Sha1>(&p)?),
        (None, None) => return Err(format!("the index for {} gives no checksum, so the package can't be checked", pack.version)),
    };
    if !want.eq_ignore_ascii_case(&got) {
        return Err(format!("{} doesn't match its index's checksum: it is incomplete or damaged", p.display()));
    }
    Ok(())
}

/// A field of an index entry, any letter case (the installer's own files
/// use `Version`, others `version`).
fn field<'a>(o: &'a Value, k: &str) -> Option<&'a Value> {
    o.as_object()?.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v)
}

/// UP19: the packages of release `version` in `folder`: its index lists the
/// release's full package, and that file is there. A name that isn't a
/// plain file name in the folder is refused.
pub fn pack_in(folder: &Path, version: &str) -> Result<Pack, String> {
    let v = plain_version(version).ok_or_else(|| format!("{version} isn't a release (MAJOR.MINOR.PATCH)"))?;
    let f = folder.join(INDEX);
    let text = std::fs::read_to_string(&f).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound if folder.is_dir() => format!("{} holds no release index", folder.display()),
        std::io::ErrorKind::NotFound => format!("{} isn't there", folder.display()),
        _ => format!("{} can't be read ({e})", f.display()),
    })?;
    let idx: Value = serde_json::from_str(&text).map_err(|e| format!("{} isn't a release index ({e})", f.display()))?;
    let assets = field(&idx, "Assets").and_then(Value::as_array).ok_or_else(|| format!("{} lists no releases", f.display()))?;
    let a = assets
        .iter()
        .find(|a| field(a, "Type").and_then(Value::as_str).is_some_and(|t| t.eq_ignore_ascii_case("Full")) && field(a, "Version").and_then(Value::as_str).and_then(plain_version) == Some(v))
        .ok_or_else(|| format!("{} doesn't list version {version}", f.display()))?;
    let package = field(a, "FileName").and_then(Value::as_str).unwrap_or_default().to_string();
    let plain = !package.is_empty() && !package.contains(['/', '\\', ':']) && package != "." && package != ".." && package != INDEX;
    if !plain {
        return Err(format!("{} names no package file for version {version}", f.display()));
    }
    let p = folder.join(&package);
    let len = match std::fs::metadata(&p) {
        Ok(m) if m.is_file() => m.len(),
        _ => return Err(format!("{} isn't there", p.display())),
    };
    // The installer needs the size: an index without it isn't a release's.
    let Some(size) = field(a, "Size").and_then(Value::as_u64) else {
        return Err(format!("{} gives no size for version {version}'s package, so it can't be checked", f.display()));
    };
    // The size is cheap to look at every time; the checksum is `verify`'s.
    if size != len {
        return Err(format!("{} is {len} bytes, but its index says {size}: it is incomplete or damaged", p.display()));
    }
    let size = Some(size);
    let hex = |k: &str| field(a, k).and_then(Value::as_str).map(str::trim).filter(|h| !h.is_empty()).map(String::from);
    let notes = field(a, "NotesMarkdown").and_then(Value::as_str).unwrap_or_default().to_string();
    let index = serde_json::to_string_pretty(&json!({"Assets": [a]})).map_err(|e| e.to_string())?;
    Ok(Pack { version: version.trim().trim_start_matches('v').to_string(), package, index, notes, size, sha256: hex("SHA256"), sha1: hex("SHA1") })
}

/// `Updates\<version>` in a shop folder: where the shop keeps a release it
/// rolled out (UP19).
pub fn release_dir(dir: &Path, version: &str) -> PathBuf {
    dir.join(shop::UPDATES).join(version)
}

/// The mark in every folder a roll out makes in `Updates` (UP19): only
/// those are ever taken out again (S11).
pub const MADE_HERE: &str = "nestpro-release.txt";

/// A release folder's name: a plain version, no `v`.
fn is_release_name(n: &str) -> bool {
    plain_version(n).is_some() && !n.starts_with('v')
}

/// A folder a roll out stages a release in (`.0.5.0.<writer>.part`), or
/// moves one aside in (`.old`).
fn is_staging_name(n: &str) -> bool {
    let Some(rest) = n.strip_prefix('.') else { return false };
    let Some(body) = rest.strip_suffix(".part").or_else(|| rest.strip_suffix(".old")) else { return false };
    let parts: Vec<&str> = body.splitn(4, '.').collect();
    parts.len() == 4 && is_release_name(&parts[..3].join(".")) && !parts[3].is_empty()
}

/// UP19: the releases kept in the shop folder's `Updates`, oldest first:
/// each a folder named by its version that holds its packages. Anything
/// else there (a copy that stopped halfway) isn't one.
pub fn in_updates(dir: &Path) -> Result<Vec<String>, String> {
    let u = dir.join(shop::UPDATES);
    let rd = match std::fs::read_dir(&u) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && dir.is_dir() => return Ok(vec![]),
        r => r.map_err(|e| format!("{} can't be read ({e})", u.display()))?,
    };
    let mut out: Vec<String> = rd.flatten().filter_map(|e| e.file_name().to_str().map(String::from)).filter(|n| is_release_name(n) && pack_in(&u.join(n), n).is_ok()).collect();
    out.sort_by_key(|n| plain_version(n));
    Ok(out)
}

// ---------------------------------------------------------------------------
// The warning before rolling out (UP20).
// ---------------------------------------------------------------------------

/// UP20: what rolling out `version` does to the shop's PCs.
#[derive(Clone, Debug, PartialEq)]
pub struct Warning {
    pub version: String,
    /// PCs on an older version (or one that can't be read as a version).
    pub behind: Vec<PcRecord>,
    /// PCs on a newer version than the release.
    pub ahead: Vec<PcRecord>,
    /// The kinds of file the release writes in a newer layout than the
    /// shop's: (kind, the shop's layout, the release's).
    pub newer_layouts: Vec<(String, u32, u32)>,
}

/// UP20: the warning before rolling out `version` (whose layouts are
/// `layouts`) to the shop `st`.
pub fn warning(st: &super::shop::Status, version: &str, layouts: &BTreeMap<String, u32>) -> Warning {
    let mut behind = vec![];
    let mut ahead = vec![];
    for p in &st.pcs {
        match shared::version_order(&p.version, version) {
            Some(Ordering::Equal) => {}
            Some(Ordering::Greater) => ahead.push(p.clone()),
            _ => behind.push(p.clone()),
        }
    }
    let shop = st.shop.layouts().unwrap_or_default();
    let newer_layouts = layouts.iter().filter_map(|(k, n)| shop.get(k).filter(|s| n > s).map(|s| (k.clone(), *s, *n))).collect();
    Warning { version: version.to_string(), behind, ahead, newer_layouts }
}

impl Warning {
    /// UP20: the warning's lines, as the window shows them before Roll out.
    pub fn lines(&self) -> Vec<String> {
        let v = &self.version;
        let mut out = vec![];
        if self.behind.is_empty() {
            out.push("Every PC in the shop already runs it, or a newer one.".to_string());
        } else {
            let names: Vec<String> = self.behind.iter().map(|p| format!("{} ({})", p.pc, p.version)).collect();
            out.push(format!("On an older version: {}. Each installs {v} from the shop folder when its program closes.", names.join(", ")));
        }
        if !self.newer_layouts.is_empty() {
            let kinds: Vec<String> = self.newer_layouts.iter().map(|(k, s, n)| format!("{k} {s} → {n}")).collect();
            out.push(format!("{v} saves in a newer file layout ({}): older versions can't open those files at all.", kinds.join(", ")));
        }
        // UP7: whatever the layout, a file a newer version saved is
        // read-only on an older one.
        for p in &self.behind {
            out.push(format!("{} is on {} and will open new jobs read-only until it updates.", p.pc, p.version));
        }
        for p in &self.ahead {
            out.push(format!("{} is on {}, newer than {v}: it opens the shop read-only until it goes back to {v} (when its program closes).", p.pc, p.version));
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Roll out (UP19).
// ---------------------------------------------------------------------------

/// What a roll out did.
#[derive(Clone, Debug, PartialEq)]
pub struct RolledOut {
    pub version: String,
    /// The shop version before, kept beside it for roll back.
    pub previous: Option<String>,
    /// Older releases taken out of `Updates`.
    pub removed: Vec<String>,
    /// The one-time changes made to the shop's files (UP18).
    pub changes: Vec<String>,
    /// S8: only the shop version's own package was copied again.
    pub repaired: bool,
}

/// UP19: what may be rolled out, as the shop is: by a person on the admin
/// PC, the release this PC runs (`this`), newer than the shop version.
fn may_roll_out(dir: &Path, s: &ShopFile, who: &Who, this: &str) -> Result<(), String> {
    if s.trial().is_some() {
        return Err(format!("{} is a trial copy, not the shop. Nothing was rolled out.", dir.display()));
    }
    if s.admin.pc != who.pc {
        return Err(format!("Rolling out is done on the admin PC ({}). Nothing was rolled out.", s.admin.pc));
    }
    match s.shop_version().map(|sv| (sv, shared::version_order(this, sv))) {
        // S8: the shop version whose package in Updates is missing or
        // damaged is copied again (a repair); one that is whole is left.
        Some((sv, Some(Ordering::Equal))) if current_release_ok(dir, sv).is_ok() => Err(format!("{sv} is already the shop version, and its package in the shop folder is whole. Nothing was changed.")),
        Some((_, Some(Ordering::Equal))) => Ok(()),
        Some((sv, Some(Ordering::Less))) => Err(format!("This PC runs {this}, older than the shop version {sv}: rolling out never goes back. Roll back (or restore a backup) does. Nothing was rolled out.")),
        _ => Ok(()),
    }
}

/// UP19: roll out the release this PC runs (`this`), whose packages are in
/// `release`, to the shop in `dir`, with the program's one-time changes.
pub fn rollout(dir: &Path, release: &Path, who: &Who, this: &str) -> Result<RolledOut, String> {
    rollout_with(dir, release, who, this, CHANGES)
}

/// `rollout`, with the one-time changes given (tests).
///
/// Only a person on the admin PC; agents are refused with the reason. Only
/// the release this PC runs (its one-time changes are this program's own),
/// newer than the shop version. Under the Library's write lock (LB1): the
/// packages are copied into `Updates\<version>` whole or not at all, the
/// one-time changes are made, then the shop file names it the shop version
/// and keeps the one before; releases older than those two are taken out of
/// `Updates`. Anything that fails leaves the shop version as it was.
pub fn rollout_with(dir: &Path, release: &Path, who: &Who, this: &str, changes: &[OneTimeChange]) -> Result<RolledOut, String> {
    if who.agent {
        return Err(AGENT_ROLLOUT.into());
    }
    if plain_version(this).is_none() {
        return Err(format!("This PC runs {this}, which isn't a release (MAJOR.MINOR.PATCH). Nothing was rolled out."));
    }
    let not_shop = || format!("{} isn't a shop. Nothing was rolled out.", dir.display());
    let s = shop::read(dir)?.ok_or_else(not_shop)?;
    may_roll_out(dir, &s, who, this)?;
    let pack = pack_in(release, this).and_then(|p| verify(release, &p).map(|()| p)).map_err(|e| format!("The packages of {this} aren't ready to roll out: {e}. Nothing was rolled out."))?;
    let still0 = || s.shop_version().map_or("The shop has no shop version yet.".to_string(), |v| format!("The shop version is still {v}."));
    // S10: the package is copied before the lock is taken.
    let staging = stage_release(dir, release, &pack).map_err(|e| format!("Nothing was rolled out: {e}. {}", still0()))?;
    let lock = match WriteLock::take(dir) {
        Ok(l) => l,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(format!("Nothing was rolled out: {e}"));
        }
    };
    shared::pause("rollout: locked");
    // Looked at again under the lock: another PC may have become the admin
    // PC, or the shop version changed.
    let checked = shop::read(dir).and_then(|s| s.ok_or_else(not_shop)).and_then(|s| may_roll_out(dir, &s, who, this).map(|()| s));
    let mut s = match checked {
        Ok(s) => s,
        Err(e) => {
            if lock.still_held().is_ok() {
                let _ = std::fs::remove_dir_all(&staging);
            }
            return Err(e);
        }
    };
    let before = s.shop_version().map(String::from);
    let still = || before.as_ref().map_or("The shop has no shop version yet.".to_string(), |v| format!("The shop version is still {v}."));
    publish_release(dir, &staging, &pack, &lock).map_err(|e| format!("Nothing was rolled out: {e}. {}", still()))?;
    shared::pause("rollout: copied");
    // S8: a repair of the shop version's own package changes nothing else.
    if before.as_deref() == Some(this) {
        return Ok(RolledOut { version: this.to_string(), previous: s.previous_version().map(String::from), removed: vec![], changes: vec![], repaired: true });
    }
    // The shop file is judged by its own version only: this PC runs a
    // newer release than the shop version it replaces (UP21 doesn't apply).
    let judged = Versions::this(this);
    let todo = pending(changes, before.as_deref(), this, &s.changes_made());
    let made = make_changes(dir, &todo, &lock, this, &judged).map_err(|e| format!("Not rolled out: {e}. {} Its packages are in {}.", still(), release_dir(dir, this).display()))?;
    if !made.is_empty() {
        s = shop::read(dir)?.ok_or_else(not_shop)?;
    }
    s.other.insert(shop::SHOP_VERSION.into(), json!(this));
    match &before {
        Some(b) => s.other.insert(shop::PREVIOUS_VERSION.into(), json!(b)),
        None => s.other.remove(shop::PREVIOUS_VERSION),
    };
    s.other.insert(shop::LAYOUTS.into(), json!(layouts()));
    // UP24: this release records its own layouts, so the shop can roll
    // back to it later; the record of the one before stays (it is kept for
    // roll back), older ones go with their packages.
    let mut mine = s.other.get(shop::RELEASE_LAYOUTS).and_then(Value::as_object).cloned().unwrap_or_default();
    mine.retain(|v, _| before.as_deref() == Some(v.as_str()));
    mine.insert(this.to_string(), json!(layouts()));
    s.other.insert(shop::RELEASE_LAYOUTS.into(), Value::Object(mine));
    shop::write_judged(dir, &s, &lock, this, &judged).map_err(|e| format!("Not rolled out: the shop file couldn't be written ({e}). {}", still()))?;
    let removed = prune_updates(dir, &[this, before.as_deref().unwrap_or(this)], &lock);
    Ok(RolledOut { version: this.to_string(), previous: before, removed, changes: made, repaired: false })
}

/// UP19: stage a release's package in `Updates`, before the Library's write
/// lock is taken, so copying a big package over a slow share never holds up
/// the other PCs' saves (S10): into a folder of this writer's own
/// (`.0.5.0.<writer>.part`), marked as made here, then checked against the
/// index. Nothing reads such a folder; a failed one is taken away again.
fn stage_release(dir: &Path, release: &Path, pack: &Pack) -> Result<PathBuf, String> {
    if !dir.is_dir() {
        return Err(crate::library::unreachable(dir));
    }
    let updates = dir.join(shop::UPDATES);
    if !updates.is_dir() {
        // Made under the lock, only inside a shop folder that is there.
        let lock = WriteLock::take(dir)?;
        if !updates.is_dir() {
            lock.still_held()?;
            std::fs::create_dir(&updates).map_err(|e| format!("{} can't be made ({e})", updates.display()))?;
        }
    }
    let staging = updates.join(format!(".{}.{}.part", pack.version, shared::writer_tag()));
    std::fs::create_dir(&staging).map_err(|e| format!("{} can't be made ({e})", staging.display()))?;
    let staged = (|| -> Result<(), String> {
        std::fs::write(staging.join(MADE_HERE), format!("A release this program rolled out: {}.\n", pack.version)).map_err(|e| format!("{} can't be written ({e})", staging.display()))?;
        let from = release.join(&pack.package);
        std::fs::copy(&from, staging.join(&pack.package)).map_err(|e| format!("{} couldn't be copied into {} ({e})", from.display(), updates.display()))?;
        shared::pause("rollout: package copied");
        verify(&staging, pack)
    })();
    if let Err(e) = staged {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    Ok(staging)
}

/// UP19: publish a staged release as `Updates\<version>` under the lock:
/// its index is written, a folder of the same version already there is
/// moved aside and taken away. The staging folder is taken away when it
/// fails while the lock is still this PC's.
fn publish_release(dir: &Path, staging: &Path, pack: &Pack, lock: &WriteLock) -> Result<(), String> {
    let target = release_dir(dir, &pack.version);
    let published = (|| -> Result<(), String> {
        if !dir.is_dir() {
            return Err(crate::library::unreachable(dir));
        }
        // N2: a folder of that name this program didn't make (no mark) is
        // never moved aside or removed: it may hold someone else's files.
        if target.exists() && !target.join(MADE_HERE).is_file() {
            return Err(format!("{} holds files this program didn't put there, so it isn't replaced. Move them elsewhere (or rename the folder), then roll out again", target.display()));
        }
        shared::write_existing(&staging.join(INDEX), pack.index.as_bytes(), lock)?;
        let old = dir.join(shop::UPDATES).join(format!(".{}.{}.old", pack.version, shared::writer_tag()));
        lock.still_held()?;
        if target.exists() {
            std::fs::rename(&target, &old).map_err(|e| format!("{} can't be replaced ({e})", target.display()))?;
        }
        lock.still_held()?;
        std::fs::rename(staging, &target).map_err(|e| format!("{} can't be made ({e})", target.display()))?;
        if old.exists() && lock.still_held().is_ok() {
            let _ = std::fs::remove_dir_all(&old);
        }
        Ok(())
    })();
    if published.is_err() && staging.exists() && lock.still_held().is_ok() {
        let _ = std::fs::remove_dir_all(staging);
    }
    published
}

/// A staging folder left this long is taken for one whose roll out
/// stopped (none takes this long).
const STAGING_LEFT: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

/// UP19 (S11): take out of `Updates` the release folders a roll out made
/// (marked so) but `keep` (the shop version and the one before), and
/// staging folders a roll out made and left a day or more. Nothing else
/// there is touched. Under the lock, checked before each.
fn prune_updates(dir: &Path, keep: &[&str], lock: &WriteLock) -> Vec<String> {
    let u = dir.join(shop::UPDATES);
    let mut removed = vec![];
    let Ok(rd) = std::fs::read_dir(&u) else { return removed };
    for e in rd.flatten() {
        let Some(n) = e.file_name().to_str().map(String::from) else { continue };
        let p = e.path();
        let ours = p.is_dir() && !p.is_symlink() && p.join(MADE_HERE).is_file();
        let release = is_release_name(&n) && !keep.contains(&n.as_str());
        let left = is_staging_name(&n) && e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age >= STAGING_LEFT);
        if !ours || !(release || left) {
            continue;
        }
        if lock.still_held().is_err() {
            break;
        }
        if std::fs::remove_dir_all(&p).is_ok() && release {
            removed.push(n);
        }
    }
    removed.sort_by_key(|n| plain_version(n));
    removed
}

/// UP19 (S8): the shop version's own release in `Updates` is whole: there,
/// and matching its index's size and checksum.
pub fn current_release_ok(dir: &Path, version: &str) -> Result<(), String> {
    let f = release_dir(dir, version);
    pack_in(&f, version).and_then(|p| verify(&f, &p))
}

// ---------------------------------------------------------------------------
// Roll back (UP23–UP26).
// ---------------------------------------------------------------------------

/// UP23: why an agent can't roll back.
pub const AGENT_ROLLBACK: &str = "Rolling back is refused for agents: it changes the release every PC in the shop installs, going back to an older one, so a person does it on the admin PC, in the window (Settings → Shop → Updates → Roll back), after reading the warning. The shop version and the shop folder are as they were. Agents can read the shop version, the release kept for roll back and the releases rolled back from with `shop`.";

/// UP24: what a roll back would do, as the window asks it.
#[derive(Clone, Debug, PartialEq)]
pub struct RollBackAsk {
    /// The shop version now: the release the shop leaves.
    pub from: String,
    /// The release kept for roll back, which becomes the shop version.
    pub to: String,
    /// The warning's lines.
    pub lines: Vec<String>,
}

/// What a roll back did.
#[derive(Clone, Debug, PartialEq)]
pub struct RolledBack {
    pub from: String,
    pub to: String,
}

/// UP24: a kind of file as roll back's refusal names it, and whether it
/// takes "were".
fn kind_words(kind: &str) -> (String, bool) {
    match kind {
        "jobs" => ("jobs".into(), true),
        "library" => ("library parts".into(), true),
        "materials" => ("materials".into(), true),
        "stock" => ("the sheet stock".into(), false),
        k => (format!("{k} files"), true),
    }
}

/// UP24: "Jobs were saved in a layout 0.4.0 can't read. Restore a backup
/// instead." for the kinds of file in `kinds`.
fn layout_refusal(kinds: &[String], to: &str) -> String {
    let words: Vec<(String, bool)> = kinds.iter().map(|k| kind_words(k)).collect();
    let mut names = match words.len() {
        1 => words[0].0.clone(),
        n => format!("{} and {}", words[..n - 1].iter().map(|w| w.0.as_str()).collect::<Vec<_>>().join(", "), words[n - 1].0),
    };
    if let Some(c) = names.get(0..1) {
        names.replace_range(0..1, &c.to_uppercase());
    }
    let verb = if words.len() == 1 && !words[0].1 { "was" } else { "were" };
    format!("{names} {verb} saved in a layout {to} can't read. Restore a backup instead. Nothing was changed.")
}

/// UP23, UP24: may `who`, on `this`, roll the shop `s` (in `dir`) back?
/// Gives (the release left, the release gone back to). Only a person on the
/// admin PC, on the shop itself (not a trial copy), running the shop
/// version or the one kept; only to a release whose packages are whole in
/// `Updates`, which reads every file layout the shop records, and which
/// knows every one-time change (of `changes`) made to the shop's files.
fn may_roll_back(dir: &Path, s: &ShopFile, who: &Who, this: &str, changes: &[OneTimeChange]) -> Result<(String, String), String> {
    if who.agent {
        return Err(AGENT_ROLLBACK.into());
    }
    if s.trial().is_some() {
        return Err(format!("{} is a trial copy, not the shop. Stop trying first. Nothing was rolled back.", dir.display()));
    }
    if s.admin.pc != who.pc {
        return Err(format!("Rolling back is done on the admin PC ({}). Nothing was rolled back.", s.admin.pc));
    }
    let Some(from) = s.shop_version().map(String::from) else {
        return Err("The shop has no shop version yet: there is nothing to roll back. Nothing was changed.".into());
    };
    let Some(to) = s.previous_version().map(String::from) else {
        return Err(format!("The shop keeps no release before {from} to go back to. Restore a backup instead. Nothing was changed."));
    };
    // The shop file is written by this program: on the release left (its
    // files count as the shop version's afterwards) or the one gone back
    // to, it stays saveable on the shop version (invariant 3).
    let on = |v: &str| shared::version_order(this, v) == Some(Ordering::Equal);
    if !on(&from) && !on(&to) {
        return Err(format!("This PC runs {this}. Roll back on the admin PC while it runs the shop version {from} (or {to}), so the shop file stays saveable on {to}. Nothing was rolled back."));
    }
    // Its package checked as a roll out checks one (size and checksum).
    let folder = release_dir(dir, &to);
    if let Err(e) = current_release_ok(dir, &to) {
        return Err(format!("{to}'s packages aren't whole in the shop folder's Updates ({e}), so the PCs couldn't install it. Put {to}'s release index and full package (from its GitHub release) back in {}, then Roll back again, or restore a backup instead. Nothing was changed.", folder.display()));
    }
    // UP24: both records must be there, readable and whole: every kind of
    // file this program guards. Nothing is ever guessed.
    let guarded = layouts();
    let whole = |l: &BTreeMap<String, u32>| guarded.keys().all(|k| l.contains_key(k));
    let reads = match s.release_layouts(&to) {
        Ok(None) => return Err(format!("{to} didn't record the file layouts it saves in when it was rolled out: it is from before roll back, and can't save what {from} wrote. Restore a backup instead. Nothing was changed.")),
        Ok(Some(l)) if whole(&l) => l,
        _ => return Err(format!("The shop file's record of the file layouts {to} saves in is damaged or incomplete, so it can't be told whether {to} reads the shop's files. Restore a backup instead. Nothing was changed.")),
    };
    let Some(written) = s.layouts().filter(|l| whole(l)) else {
        return Err(format!("The shop file doesn't record every file layout the shop's files are saved in, so it can't be told whether {to} reads them. Restore a backup instead. Nothing was changed."));
    };
    let unread: Vec<String> = written.into_iter().filter(|(k, n)| reads.get(k).is_none_or(|r| n > r)).map(|(k, _)| k).collect();
    if !unread.is_empty() {
        return Err(layout_refusal(&unread, &to));
    }
    // UP24: a one-time change of a release after `to` (or one this program
    // doesn't know) changed the shop's files in a way `to` doesn't know.
    let newer: Vec<String> = s
        .changes_made()
        .into_iter()
        .filter(|n| changes.iter().find(|c| c.name == n).is_none_or(|c| shared::version_order(c.release, &to) == Some(Ordering::Greater)))
        .map(|n| changes.iter().find(|c| c.name == n).map_or(n, |c| c.what.to_string()))
        .collect();
    if !newer.is_empty() {
        return Err(format!("{from} changed the shop's files in a way {to} doesn't know about ({}). Restore a backup instead. Nothing was changed.", newer.join("; ")));
    }
    Ok((from, to))
}

/// UP24: the kinds of shop record a roll back makes saveable on the release
/// gone back to again (UP25), as its warning names them.
pub const RECORDS: &str = "Jobs, library parts, the Library's list, the sheet stock, the material list and the shop file";

/// UP24: what rolling the shop in `dir` back would do, as the window warns
/// before it: refused (with the reason, as Roll back itself would refuse
/// it), or the warning's lines. Changes nothing.
pub fn rollback_ask(dir: &Path, who: &Who, this: &str) -> Result<RollBackAsk, String> {
    rollback_ask_with(dir, who, this, CHANGES)
}

/// `rollback_ask`, with the one-time changes given (tests).
pub fn rollback_ask_with(dir: &Path, who: &Who, this: &str, changes: &[OneTimeChange]) -> Result<RollBackAsk, String> {
    if who.agent {
        return Err(AGENT_ROLLBACK.into());
    }
    let st = super::shop::status(dir)?.ok_or_else(|| format!("{} isn't a shop. Nothing was rolled back.", dir.display()))?;
    let (from, to) = may_roll_back(dir, &st.shop, who, this, changes)?;
    // S7: every kind of shop record the roll back makes saveable on `to`
    // again (UP25), each saved back through `to`'s own layout.
    let mut lines = vec![format!("{RECORDS} saved since {from} was rolled out may lose what only {from} knew about them when saved again.")];
    for p in &st.pcs {
        let me = if p.pc == who.pc { " (this PC)" } else { "" };
        match shared::version_order(&p.version, &to) {
            Some(Ordering::Equal) => {}
            Some(Ordering::Greater) => lines.push(format!("{}{me} is on {}: it opens the shop read-only until it goes back to {to} (when its program closes).", p.pc, p.version)),
            _ => lines.push(format!("{}{me} is on {}: it installs {to} from the shop folder when its program closes.", p.pc, p.version)),
        }
    }
    lines.push(format!("{from} stays in the shop folder's Updates, so it can be rolled out again."));
    Ok(RollBackAsk { from, to, lines })
}

/// UP23, UP25: roll the shop in `dir` back to the release kept beside the
/// shop version, by a person on the admin PC running `this`.
pub fn rollback(dir: &Path, who: &Who, this: &str) -> Result<RolledBack, String> {
    rollback_with(dir, who, this, CHANGES, None)
}

/// UP24 (S4): Yes on the warning: the roll back `ask` showed, from its
/// release to its release, and only that. Looked at under the lock: when
/// the shop version or the release kept changed since the warning, nothing
/// changes and the person is asked again.
pub fn rollback_as_asked(dir: &Path, who: &Who, this: &str, ask: &RollBackAsk) -> Result<RolledBack, String> {
    rollback_with(dir, who, this, CHANGES, Some((&ask.from, &ask.to)))
}

/// `rollback`, with the one-time changes given (tests).
///
/// Refused (changing nothing) as `rollback_ask` says. Under the Library's
/// write lock (LB1), looked at again there: the shop file names the release
/// kept the shop version, keeps no release before it, records the layouts
/// that release writes, and adds the release left to the releases rolled
/// back from (UP25). `Updates` keeps both releases. Every PC then installs
/// the shop version when its program closes, going back (UP21).
pub fn rollback_with(dir: &Path, who: &Who, this: &str, changes: &[OneTimeChange], asked: Option<(&str, &str)>) -> Result<RolledBack, String> {
    if who.agent {
        return Err(AGENT_ROLLBACK.into());
    }
    let not_shop = || format!("{} isn't a shop. Nothing was rolled back.", dir.display());
    let s = shop::read(dir)?.ok_or_else(not_shop)?;
    may_roll_back(dir, &s, who, this, changes)?;
    let lock = WriteLock::take(dir).map_err(|e| format!("Nothing was rolled back: {e}"))?;
    shared::pause("rollback: locked");
    let mut s = shop::read(dir)?.ok_or_else(not_shop)?;
    let (from, to) = may_roll_back(dir, &s, who, this, changes)?;
    if let Some((f, t)) = asked.filter(|(f, t)| *f != from || *t != to) {
        return Err(format!("The shop changed since you were asked to roll back from {f} to {t}: the shop version is {from} now, and the release kept for roll back {to}. Nothing was rolled back: ask again to see what rolling back would do now."));
    }
    let mut list = s.rolled_back_from();
    if !list.iter().any(|v| shared::version_order(v, &from) == Some(Ordering::Equal)) {
        list.push(from.clone());
    }
    let reads = s.release_layouts(&to).ok().flatten().unwrap_or_default();
    s.other.insert(shop::SHOP_VERSION.into(), json!(to));
    s.other.remove(shop::PREVIOUS_VERSION);
    s.other.insert(shop::LAYOUTS.into(), json!(reads));
    s.other.insert(shop::ROLLED_BACK_FROM.into(), json!(list));
    // Judged as the shop will be: what the release left wrote counts as
    // the shop version's.
    let judged = Versions { this: this.to_string(), shop: Some(to.clone()), rolled_back_from: list, shop_dir: Some(dir.to_path_buf()), ..Default::default() };
    shop::write_judged(dir, &s, &lock, this, &judged).map_err(|e| format!("Not rolled back: the shop file couldn't be written ({e}). The shop version is still {from}."))?;
    Ok(RolledBack { from, to })
}

// ---------------------------------------------------------------------------
// Trying a release on the admin PC (UP16, UP17).
// ---------------------------------------------------------------------------

/// A trial begun: the copy this PC works on now, and its mark.
#[derive(Clone, Debug)]
pub struct Trial {
    pub copy: PathBuf,
    pub mark: TrialMark,
    /// The fresh backup the copy was made from.
    pub backup: PathBuf,
}

/// UP16 (S4): trial copies go in `root`, a folder of this PC's own: never
/// in the shop folder `dir`, nor in a folder that is the same place under
/// another name (both are resolved before anything is made).
pub fn trial_place_ok(dir: &Path, root: &Path) -> Result<(), String> {
    if shared::inside(root, dir) {
        return Err(format!("Nothing was tried: trial copies go in {}, which is inside the shop folder {}, and a trial never writes to the shop folder.", root.display(), dir.display()));
    }
    Ok(())
}

/// UP16: begin trying `version` on this PC (the admin PC of the shop in
/// `dir`): take a fresh backup of the shop (BK7: Back up now) and unpack it
/// into a new folder in `root`, on this PC, marked as a trial copy. From
/// then on this PC's Library is the copy (the caller points it there), so
/// nothing the trial saves reaches the shop. The copy gets a backup id of
/// its own and no second place, so its own backups never mix with the
/// shop's. `this` is the program writing the copy's shop file.
pub fn start_trial(dir: &Path, root: &Path, version: &str, who: &Who, clock: &Clock, this: &str, own: &backup::Settings) -> Result<Trial, String> {
    trial_place_ok(dir, root)?;
    let s = shop::read(dir)?.ok_or_else(|| format!("{} isn't a shop. Nothing was tried.", dir.display()))?;
    if let Some(m) = s.trial() {
        return Err(format!("This PC is already trying {} on a copy of the shop. Stop trying first.", m.version));
    }
    if s.admin.pc != who.pc {
        return Err(format!("A release is tried on the admin PC ({}). Nothing was tried.", s.admin.pc));
    }
    if plain_version(version).is_none() {
        return Err(format!("{version} isn't a release (MAJOR.MINOR.PATCH). Nothing was tried."));
    }
    // S4: Stop trying goes back to the shop version: there must be one, and
    // its package must be whole in the shop folder.
    let Some(sv) = s.shop_version() else {
        return Err(format!("Shop {} has no shop version yet, so a trial would have no release to go back to. Roll out the release this PC runs first. Nothing was tried.", s.name));
    };
    if shared::version_order(version, sv) != Some(Ordering::Greater) {
        return Err(format!("{version} isn't newer than the shop version {sv}: there's nothing to try. Nothing was tried."));
    }
    current_release_ok(dir, sv).map_err(|e| format!("Nothing was tried: the shop version {sv} couldn't be gone back to ({e}). Roll it out again on this PC first (Roll out to the shop repairs it)."))?;
    let taken = backup::take(dir, own, who, clock, backup::Why::Now).map_err(|e| format!("Nothing was tried: a backup of the shop couldn't be taken first. {e}"))?;
    let zip = match taken {
        backup::Daily::Taken(d) => d.entry.path,
        backup::Daily::AlreadyToday { entry, .. } => entry.path,
    };
    std::fs::create_dir_all(root).map_err(|e| format!("Nothing was tried: {} can't be made ({e}).", root.display()))?;
    let label: String = shared::file_safe(&s.name).chars().take(24).collect();
    let mut n = 1;
    let copy = loop {
        let name = if n == 1 { format!("{label} trying {version}") } else { format!("{label} trying {version} {n}") };
        let c = root.join(name);
        match std::fs::create_dir(&c) {
            Ok(()) => break c,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && n < 1000 => n += 1,
            Err(e) => return Err(format!("Nothing was tried: {} can't be made ({e}).", c.display())),
        }
    };
    unzip(&zip, &copy).map_err(|e| format!("Nothing was tried: the backup {} couldn't be unpacked into {} ({e}).", zip.display(), copy.display()))?;
    // The copy's own backups: a new id, kept beside it, never in the shop's
    // second place.
    let _ = std::fs::remove_file(copy.join(backup::ID_FILE));
    let mut cs = shop::read(&copy)?.ok_or_else(|| format!("Nothing was tried: the backup {} holds no shop file.", zip.display()))?;
    let mark = TrialMark { shop_folder: dir.display().to_string(), version: version.trim().trim_start_matches('v').to_string(), shop_version: s.shop_version().map(String::from), by: who.clone(), at: clock.now() };
    cs.other.insert(shop::TRIAL.into(), json!(mark));
    cs.backups.second_place.clear();
    let lock = WriteLock::take(&copy)?;
    shop::write_judged(&copy, &cs, &lock, this, &Versions::this(this))?;
    Ok(Trial { copy, mark, backup: zip })
}

/// Unpack a backup zip into the empty folder `to`. Only names inside it.
fn unzip(zip: &Path, to: &Path) -> Result<usize, String> {
    let f = std::fs::File::open(zip).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(std::io::BufReader::new(f)).map_err(|e| e.to_string())?;
    let mut files = 0;
    for i in 0..z.len() {
        let mut e = z.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = e.enclosed_name() else { return Err(format!("{} names a file outside the folder", e.name())) };
        let out = to.join(rel);
        if e.is_dir() {
            std::fs::create_dir_all(&out).map_err(|x| format!("{}: {x}", out.display()))?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p).map_err(|x| format!("{}: {x}", p.display()))?;
        }
        let mut w = std::fs::OpenOptions::new().write(true).create_new(true).open(&out).map_err(|x| format!("{}: {x}", out.display()))?;
        std::io::copy(&mut e, &mut w).map_err(|x| format!("{}: {x}", out.display()))?;
        files += 1;
    }
    Ok(files)
}

/// UP16, UP17: where a trial stands, looked at when the program opens.
#[derive(Clone, Debug, PartialEq)]
pub enum TrialNow {
    /// This PC's Library isn't a trial copy.
    NotTrial,
    /// Trying `mark.version`; `installed` once this PC runs it.
    Trying { mark: TrialMark, installed: bool },
    /// The shop moved on while this PC was trying (another release was
    /// rolled out, or this one): the trial is over, and this PC works on the
    /// shop again (UP17).
    Over { mark: TrialMark, said: String },
    /// The shop can't be reached, or holds no shop any more: this PC stays
    /// on the copy, and says so (LB11).
    ShopAway { mark: TrialMark, said: String },
}

/// UP16, UP17: where the trial in `lib` stands for a PC running `this`.
pub fn trial_now(lib: &Path, this: &str) -> TrialNow {
    let Some(mark) = shop::read(lib).ok().flatten().and_then(|s| s.trial()) else { return TrialNow::NotTrial };
    let shop_dir = PathBuf::from(&mark.shop_folder);
    match shop::read(&shop_dir) {
        Err(e) => TrialNow::ShopAway { said: format!("This PC stays on its trial copy: {e}"), mark },
        Ok(None) => TrialNow::ShopAway { said: format!("This PC stays on its trial copy: {} holds no shop any more.", shop_dir.display()), mark },
        Ok(Some(s)) if s.shop_version() != mark.shop_version.as_deref() => {
            let now = s.shop_version().unwrap_or("none");
            // UP26: a roll back ends a trial too.
            let back = mark.shop_version.as_deref().is_some_and(|was| shared::version_order(now, was) == Some(Ordering::Less));
            let did = if back { "rolled back to" } else { "rolled out" };
            TrialNow::Over { said: format!("Shop {} {did} {now} while this PC was trying {}: the trial is over, and this PC works on the shop again.", s.name, mark.version), mark }
        }
        Ok(Some(_)) => TrialNow::Trying { installed: shared::version_order(this, &mark.version) == Some(Ordering::Equal), mark },
    }
}

/// UP18: make the one-time changes of the release tried on the trial copy
/// `copy`, once this PC runs it (`this`): those of releases newer than the
/// shop version the copy was made at, up to `this`, not made yet. Under the
/// copy's own write lock; nothing reaches the shop folder.
pub fn trial_changes(copy: &Path, this: &str) -> Result<Vec<String>, String> {
    trial_changes_with(copy, this, CHANGES)
}

/// `trial_changes`, with the one-time changes given (tests).
pub fn trial_changes_with(copy: &Path, this: &str, changes: &[OneTimeChange]) -> Result<Vec<String>, String> {
    let s = shop::read(copy)?.ok_or_else(|| format!("{} holds no shop file.", copy.display()))?;
    let Some(mark) = s.trial() else { return Err(format!("{} isn't a trial copy: one-time changes reach the shop only when it is rolled out.", copy.display())) };
    if shared::version_order(this, &mark.version) != Some(Ordering::Equal) {
        return Ok(vec![]);
    }
    let todo = pending(changes, mark.shop_version.as_deref(), this, &s.changes_made());
    if todo.is_empty() {
        return Ok(vec![]);
    }
    let lock = WriteLock::take(copy)?;
    let s = shop::read(copy)?.ok_or_else(|| format!("{} holds no shop file.", copy.display()))?;
    let todo = pending(changes, mark.shop_version.as_deref(), this, &s.changes_made());
    make_changes(copy, &todo, &lock, this, &Versions::this(this))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::shop as eshop;

    fn tmp(name: &str) -> crate::testdir::TestDir {
        let d = crate::testdir::TestDir::made(&format!("rollout_{name}"));
        d
    }

    /// A made-up release folder for `version`, as the release workflow
    /// packs it (`vpk`): the index and the full package.
    pub fn fake_release(at: &Path, version: &str) -> PathBuf {
        let d = at.join(format!("release-{version}"));
        std::fs::create_dir_all(&d).unwrap();
        let pkg = format!("KurfLabs.nestpro-{version}-full.nupkg");
        let body = format!("made-up package {version}");
        std::fs::write(d.join(&pkg), &body).unwrap();
        let sha: String = <sha2::Sha256 as sha2::Digest>::digest(body.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
        let idx = json!({"Assets": [
            {"PackageId": "KurfLabs.nestpro", "Version": version, "Type": "Full", "FileName": pkg, "SHA256": sha, "Size": body.len(), "NotesMarkdown": format!("## Changes\n\n- Release {version}\n")},
            {"PackageId": "KurfLabs.nestpro", "Version": version, "Type": "Delta", "FileName": format!("KurfLabs.nestpro-{version}-delta.nupkg")},
        ]});
        std::fs::write(d.join(INDEX), idx.to_string()).unwrap();
        d
    }

    /// A shop Acme in a test Library, PC-A its admin PC, on `version`.
    fn acme(d: &Path, version: &str) -> PathBuf {
        let lib = d.join("Nesting");
        crate::library::create(&lib).unwrap();
        eshop::start(&lib, "Acme", &Who::new("sam", "PC-A"), &Clock::at(1_794_216_000), version).unwrap();
        lib
    }

    fn sam() -> Who {
        Who::new("sam", "PC-A")
    }

    #[test]
    fn a_release_folder_names_its_package_and_only_that_release() {
        let d = tmp("pack");
        let r = fake_release(&d, "0.5.0");
        let p = pack_in(&r, "0.5.0").unwrap();
        assert_eq!(p.package, "KurfLabs.nestpro-0.5.0-full.nupkg");
        let idx: Value = serde_json::from_str(&p.index).unwrap();
        assert_eq!(idx["Assets"].as_array().unwrap().len(), 1, "{idx}");
        assert!(pack_in(&r, "0.6.0").unwrap_err().contains("doesn't list version 0.6.0"));
        std::fs::remove_file(r.join(&p.package)).unwrap();
        assert!(pack_in(&r, "0.5.0").unwrap_err().contains("isn't there"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// UP19: a roll out copies the release into Updates, keeps the previous
    /// one beside it, takes older ones out and sets the shop version.
    #[test]
    fn rolling_out_copies_the_release_and_keeps_the_previous_one() {
        let d = tmp("rollout");
        let lib = acme(&d, "0.3.0");
        let r1 = rollout(&lib, &fake_release(&d, "0.3.0"), &sam(), "0.3.0").unwrap();
        assert_eq!((r1.version.as_str(), r1.previous.as_deref()), ("0.3.0", None));
        let r2 = rollout(&lib, &fake_release(&d, "0.4.0"), &sam(), "0.4.0").unwrap();
        assert_eq!(r2.previous.as_deref(), Some("0.3.0"));
        assert_eq!(in_updates(&lib).unwrap(), ["0.3.0", "0.4.0"]);
        let r3 = rollout(&lib, &fake_release(&d, "0.5.0"), &sam(), "0.5.0").unwrap();
        assert_eq!((r3.previous.as_deref(), r3.removed.clone()), (Some("0.4.0"), vec!["0.3.0".to_string()]));
        assert_eq!(in_updates(&lib).unwrap(), ["0.4.0", "0.5.0"]);
        let s = shop::read(&lib).unwrap().unwrap();
        assert_eq!((s.shop_version(), s.previous_version()), (Some("0.5.0"), Some("0.4.0")));
        assert_eq!(s.layouts(), Some(layouts()));
        // The package in Updates is the release's own bytes.
        assert_eq!(std::fs::read(release_dir(&lib, "0.5.0").join("KurfLabs.nestpro-0.5.0-full.nupkg")).unwrap(), b"made-up package 0.5.0");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// UP19: what a roll out refuses, each leaving the shop as it was.
    #[test]
    fn a_roll_out_refuses_an_older_or_the_same_release_another_pc_and_an_agent() {
        let d = tmp("rollout_refused");
        let lib = acme(&d, "0.4.0");
        rollout(&lib, &fake_release(&d, "0.4.0"), &sam(), "0.4.0").unwrap();
        let before = std::fs::read(shop::file(&lib)).unwrap();
        let older = rollout(&lib, &fake_release(&d, "0.3.0"), &sam(), "0.3.0").unwrap_err();
        assert!(older.contains("older than the shop version 0.4.0") && older.contains("Roll back"), "{older}");
        let same = rollout(&lib, &fake_release(&d, "0.4.0"), &sam(), "0.4.0").unwrap_err();
        assert!(same.contains("already the shop version"), "{same}");
        let other = rollout(&lib, &fake_release(&d, "0.5.0"), &Who::new("kim", "PC-B"), "0.5.0").unwrap_err();
        assert!(other.contains("admin PC (PC-A)"), "{other}");
        assert_eq!(rollout(&lib, &fake_release(&d, "0.5.0"), &sam().as_agent(), "0.5.0").unwrap_err(), AGENT_ROLLOUT);
        // A release folder without this PC's release.
        let missing = rollout(&lib, &fake_release(&d, "0.5.1"), &sam(), "0.5.0").unwrap_err();
        assert!(missing.contains("aren't ready to roll out"), "{missing}");
        assert_eq!(std::fs::read(shop::file(&lib)).unwrap(), before, "a refused roll out changed the shop file");
        assert_eq!(in_updates(&lib).unwrap(), ["0.4.0"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// UP19: a copy that fails halfway leaves no release in Updates and
    /// the shop version as it was; the next roll out goes through.
    #[test]
    fn a_copy_that_fails_halfway_changes_nothing() {
        let d = tmp("rollout_half");
        let lib = acme(&d, "0.4.0");
        rollout(&lib, &fake_release(&d, "0.4.0"), &sam(), "0.4.0").unwrap();
        let rel = fake_release(&d, "0.5.0");
        // The index can't be written: a folder stands where it goes.
        let lib2 = lib.clone();
        shared::hooks::set(move |step| {
            if step == "rollout: package copied" {
                let u = lib2.join(shop::UPDATES);
                for e in std::fs::read_dir(&u).unwrap().flatten() {
                    if e.file_name().to_string_lossy().ends_with(".part") {
                        std::fs::create_dir(e.path().join(INDEX)).unwrap();
                    }
                }
            }
        });
        let e = rollout(&lib, &rel, &sam(), "0.5.0").unwrap_err();
        shared::hooks::clear();
        assert!(e.starts_with("Nothing was rolled out") && e.contains("The shop version is still 0.4.0."), "{e}");
        assert_eq!(shop::read(&lib).unwrap().unwrap().shop_version(), Some("0.4.0"));
        assert_eq!(in_updates(&lib).unwrap(), ["0.4.0"]);
        let left: Vec<String> = std::fs::read_dir(lib.join(shop::UPDATES)).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(left, ["0.4.0"], "the half copy was left");
        rollout(&lib, &rel, &sam(), "0.5.0").unwrap();
        assert_eq!(in_updates(&lib).unwrap(), ["0.4.0", "0.5.0"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// UP19 (LB11): the share drops in the middle of a roll out: it says
    /// so, nothing is made in the shop folder's place, and once it is back
    /// the shop version is as it was.
    #[test]
    fn a_share_lost_during_a_roll_out_leaves_the_shop_version() {
        let d = tmp("rollout_share");
        let lib = acme(&d, "0.4.0");
        rollout(&lib, &fake_release(&d, "0.4.0"), &sam(), "0.4.0").unwrap();
        let away = d.join("away");
        let (l2, a2) = (lib.clone(), away.clone());
        shared::hooks::set(move |step| {
            if step == "rollout: locked" {
                std::fs::rename(&l2, &a2).unwrap();
            }
        });
        let e = rollout(&lib, &fake_release(&d, "0.5.0"), &sam(), "0.5.0").unwrap_err();
        shared::hooks::clear();
        assert!(e.contains(&lib.display().to_string()), "{e}");
        assert!(!lib.exists(), "something was made in the shop folder's place");
        std::fs::rename(&away, &lib).unwrap();
        assert_eq!(shop::read(&lib).unwrap().unwrap().shop_version(), Some("0.4.0"));
        assert_eq!(in_updates(&lib).unwrap(), ["0.4.0"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    fn mark_machines(dir: &Path, lock: &WriteLock) -> Result<(), String> {
        shared::write_existing(&dir.join("machines-moved.txt"), b"moved", lock)
    }

    fn fail(_: &Path, _: &WriteLock) -> Result<(), String> {
        Err("the disk is full".into())
    }

    const FAKE: &[OneTimeChange] = &[
        OneTimeChange { name: "machines-into-shop", release: "0.5.0", what: "Moves the machines into the shop", run: mark_machines },
        OneTimeChange { name: "older", release: "0.3.0", what: "An older release's change", run: fail },
        OneTimeChange { name: "later", release: "0.6.0", what: "A later release's change", run: fail },
    ];

    /// UP18: a release's one-time change runs at roll out, on the shop,
    /// once; changes of older or later releases don't.
    #[test]
    fn a_one_time_change_runs_once_at_roll_out() {
        let d = tmp("rollout_change");
        let lib = acme(&d, "0.4.0");
        rollout_with(&lib, &fake_release(&d, "0.4.0"), &sam(), "0.4.0", &[]).unwrap();
        let r = rollout_with(&lib, &fake_release(&d, "0.5.0"), &sam(), "0.5.0", FAKE).unwrap();
        assert_eq!(r.changes, ["Moves the machines into the shop"]);
        assert!(lib.join("machines-moved.txt").is_file());
        assert_eq!(shop::read(&lib).unwrap().unwrap().changes_made(), ["machines-into-shop"]);
        assert!(pending(FAKE, Some("0.4.0"), "0.5.0", &["machines-into-shop".into()]).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// UP18: a one-time change that fails stops the roll out: the shop
    /// version stays.
    #[test]
    fn a_failed_one_time_change_leaves_the_shop_version() {
        let d = tmp("rollout_change_fail");
        let lib = acme(&d, "0.5.0");
        rollout_with(&lib, &fake_release(&d, "0.5.0"), &sam(), "0.5.0", &[]).unwrap();
        let e = rollout_with(&lib, &fake_release(&d, "0.6.0"), &sam(), "0.6.0", FAKE).unwrap_err();
        assert!(e.contains("A later release's change") && e.contains("the disk is full") && e.contains("The shop version is still 0.5.0."), "{e}");
        assert_eq!(shop::read(&lib).unwrap().unwrap().shop_version(), Some("0.5.0"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// UP20: the warning names the PCs behind, each opening new jobs
    /// read-only until it updates, the layout change, and PCs ahead.
    #[test]
    fn the_warning_lists_the_pcs_behind_and_the_layout_change() {
        let d = tmp("warning");
        let lib = acme(&d, "0.4.0");
        rollout(&lib, &fake_release(&d, "0.4.0"), &sam(), "0.4.0").unwrap();
        let c = Clock::at(1_794_216_600);
        for (who, v) in [(Who::new("sam", "PC-A"), "0.5.0"), (Who::new("kim", "PC-B"), "0.4.0"), (Who::new("lee", "PC-C"), "0.3.2"), (Who::new("max", "PC-D"), "0.6.0")] {
            eshop::record_pc(&lib, &who, &c, v).unwrap();
        }
        let st = eshop::status(&lib).unwrap().unwrap();
        let mut newer = layouts();
        *newer.get_mut("jobs").unwrap() += 1;
        let w = warning(&st, "0.5.0", &newer);
        assert_eq!(
            w.lines(),
            [
                "On an older version: PC-B (0.4.0), PC-C (0.3.2). Each installs 0.5.0 from the shop folder when its program closes.".to_string(),
                format!("0.5.0 saves in a newer file layout (jobs {} → {}): older versions can't open those files at all.", crate::jobfile::VERSION, crate::jobfile::VERSION + 1),
                "PC-B is on 0.4.0 and will open new jobs read-only until it updates.".to_string(),
                "PC-C is on 0.3.2 and will open new jobs read-only until it updates.".to_string(),
                "PC-D is on 0.6.0, newer than 0.5.0: it opens the shop read-only until it goes back to 0.5.0 (when its program closes).".to_string(),
            ]
        );
        // The same layouts: no layout line.
        assert!(warning(&st, "0.5.0", &layouts()).newer_layouts.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S10: the package is copied into Updates before the Library's write
    /// lock is taken, so a slow copy never holds up the other PCs' saves.
    #[test]
    fn the_package_is_copied_before_the_lock_is_taken() {
        let d = tmp("rollout_stage");
        let lib = acme(&d, "0.4.0");
        let locked = std::rc::Rc::new(std::cell::Cell::new(None));
        let (l2, seen) = (lib.clone(), locked.clone());
        shared::hooks::set(move |step| {
            if step == "rollout: package copied" {
                seen.set(Some(l2.join(shared::LOCK_FILE).exists()));
            }
        });
        let r = rollout(&lib, &fake_release(&d, "0.4.0"), &sam(), "0.4.0");
        shared::hooks::clear();
        r.unwrap();
        assert_eq!(locked.get(), Some(false), "the package was copied under the write lock");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S8: a package that isn't whole never becomes the shop version; the
    /// shop version's own package, damaged in Updates, is copied again.
    #[test]
    fn a_damaged_package_is_refused_and_the_current_one_can_be_repaired() {
        let d = tmp("rollout_damaged");
        let lib = acme(&d, "0.4.0");
        rollout(&lib, &fake_release(&d, "0.4.0"), &sam(), "0.4.0").unwrap();
        // The admin PC's copy of 0.5.0 stopped halfway.
        let rel = fake_release(&d, "0.5.0");
        let pkg = rel.join("KurfLabs.nestpro-0.5.0-full.nupkg");
        std::fs::write(&pkg, "made-up").unwrap();
        let e = rollout(&lib, &rel, &sam(), "0.5.0").unwrap_err();
        assert!(e.contains("incomplete or damaged") && e.contains("Nothing was rolled out"), "{e}");
        // The same length, other bytes: the checksum catches it.
        std::fs::write(&pkg, "made-up package 0.5.X").unwrap();
        assert!(rollout(&lib, &rel, &sam(), "0.5.0").unwrap_err().contains("checksum"));
        assert_eq!(shop::read(&lib).unwrap().unwrap().shop_version(), Some("0.4.0"));
        // The shop version's package in Updates got damaged: the other PCs
        // are told, and rolling the same version out again repairs it.
        std::fs::write(release_dir(&lib, "0.4.0").join("KurfLabs.nestpro-0.4.0-full.nupkg"), "x").unwrap();
        assert!(matches!(crate::engine::updates::from_shop(&lib, "0.3.0"), Ok(crate::engine::updates::FromShop::Missing { .. })));
        let r = rollout(&lib, &fake_release(&d, "0.4.0"), &sam(), "0.4.0").unwrap();
        assert!(r.repaired && r.changes.is_empty(), "{r:?}");
        current_release_ok(&lib, "0.4.0").unwrap();
        assert_eq!(shop::read(&lib).unwrap().unwrap().shop_version(), Some("0.4.0"));
        // Whole again: the same version is refused.
        assert!(rollout(&lib, &fake_release(&d, "0.4.0"), &sam(), "0.4.0").unwrap_err().contains("already the shop version"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// N2: a folder named for the release in Updates that this program
    /// didn't make is never replaced or removed: the roll out (a repair
    /// here) is refused, naming it, and its files stay.
    #[test]
    fn an_unmarked_release_folder_is_never_replaced() {
        let d = tmp("rollout_unmarked");
        let lib = acme(&d, "0.4.0");
        rollout(&lib, &fake_release(&d, "0.4.0"), &sam(), "0.4.0").unwrap();
        let f = release_dir(&lib, "0.4.0");
        std::fs::remove_file(f.join(MADE_HERE)).unwrap();
        std::fs::write(f.join("drivers.txt"), "someone's").unwrap();
        std::fs::write(f.join("KurfLabs.nestpro-0.4.0-full.nupkg"), "x").unwrap();
        let e = rollout(&lib, &fake_release(&d, "0.4.0"), &sam(), "0.4.0").unwrap_err();
        assert!(e.contains(&f.display().to_string()) && e.contains("didn't put there"), "{e}");
        assert_eq!(std::fs::read_to_string(f.join("drivers.txt")).unwrap(), "someone's");
        let left: Vec<String> = std::fs::read_dir(lib.join(shop::UPDATES)).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(left, ["0.4.0"], "a staging folder was left");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S8: an index without the package's size isn't a release's: refused.
    #[test]
    fn an_index_without_the_size_is_refused() {
        let d = tmp("rollout_nosize");
        let r = fake_release(&d, "0.5.0");
        let mut idx: Value = serde_json::from_str(&std::fs::read_to_string(r.join(INDEX)).unwrap()).unwrap();
        idx["Assets"][0].as_object_mut().unwrap().remove("Size");
        std::fs::write(r.join(INDEX), idx.to_string()).unwrap();
        assert!(pack_in(&r, "0.5.0").unwrap_err().contains("gives no size"));
        let lib = acme(&d, "0.4.0");
        assert!(rollout(&lib, &r, &sam(), "0.5.0").unwrap_err().contains("gives no size"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S11: a roll out takes out of Updates only the release folders and
    /// staging folders it made; anything else there stays.
    #[test]
    fn a_roll_out_takes_out_only_what_it_made() {
        let d = tmp("rollout_prune");
        let lib = acme(&d, "0.3.0");
        let u = lib.join(shop::UPDATES);
        std::fs::create_dir_all(u.join("0.1.0")).unwrap();
        std::fs::write(u.join("0.1.0").join("readme.txt"), "someone's").unwrap();
        std::fs::create_dir_all(u.join(".0.2.0.other.part")).unwrap();
        std::fs::create_dir_all(u.join("drivers")).unwrap();
        for v in ["0.3.0", "0.4.0", "0.5.0"] {
            rollout(&lib, &fake_release(&d, v), &sam(), v).unwrap();
        }
        let mut left: Vec<String> = std::fs::read_dir(&u).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left, [".0.2.0.other.part", "0.1.0", "0.4.0", "0.5.0", "drivers"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A made-up one-time change that can run again: it moves the machines'
    /// file into the shop's folder, and finds it moved when it runs again.
    fn move_machines(dir: &Path, lock: &WriteLock) -> Result<(), String> {
        let (from, to) = (dir.join("machines-old.json"), dir.join(shop::PCS).join("machines.json"));
        if from.is_file() {
            let text = std::fs::read(&from).map_err(|e| e.to_string())?;
            shared::write_existing(&to, &text, lock)?;
            lock.still_held()?;
            std::fs::remove_file(&from).map_err(|e| e.to_string())?;
        }
        if !to.is_file() {
            return Err("the machines' file is in neither place".into());
        }
        Ok(())
    }

    /// S7, UP18: a one-time change stopped between its effects and its
    /// record (the lock was lost) runs again on the next roll out and
    /// finishes, without doing its work twice; it is recorded once.
    #[test]
    fn a_change_stopped_before_its_record_finishes_on_the_next_roll_out() {
        let d = tmp("rollout_change_resume");
        let lib = acme(&d, "0.4.0");
        rollout_with(&lib, &fake_release(&d, "0.4.0"), &sam(), "0.4.0", &[]).unwrap();
        std::fs::write(lib.join("machines-old.json"), r#"{"machines": ["LC5"]}"#).unwrap();
        const MOVE: &[OneTimeChange] = &[OneTimeChange { name: "machines-into-shop", release: "0.5.0", what: "Moves the machines into the shop", run: move_machines }];
        let l2 = lib.clone();
        shared::hooks::set(move |step| {
            if step == "rollout: change made" {
                // Another PC took the lock over: the record can't be written.
                let _ = std::fs::remove_file(l2.join(shared::LOCK_FILE));
            }
        });
        let e = rollout_with(&lib, &fake_release(&d, "0.5.0"), &sam(), "0.5.0", MOVE).unwrap_err();
        shared::hooks::clear();
        assert!(e.contains("The shop version is still 0.4.0"), "{e}");
        let s = shop::read(&lib).unwrap().unwrap();
        assert!(s.changes_made().is_empty() && s.shop_version() == Some("0.4.0"));
        let r = rollout_with(&lib, &fake_release(&d, "0.5.0"), &sam(), "0.5.0", MOVE).unwrap();
        assert_eq!(r.changes, ["Moves the machines into the shop"]);
        assert_eq!(std::fs::read_to_string(lib.join(shop::PCS).join("machines.json")).unwrap(), r#"{"machines": ["LC5"]}"#);
        assert!(!lib.join("machines-old.json").exists());
        assert_eq!(shop::read(&lib).unwrap().unwrap().changes_made(), ["machines-into-shop"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S4: a trial needs a shop version to go back to, whole in Updates: a
    /// new shop rolls the release this PC runs out first.
    #[test]
    fn a_first_trial_on_a_new_shop_needs_a_version_to_go_back_to() {
        let d = tmp("rollout_first_trial");
        let lib = acme(&d, "0.4.0");
        let c = Clock::at(1_794_216_600);
        let e = start_trial(&lib, &d.join("Trials"), "0.5.0", &sam(), &c, "0.4.0", &Default::default()).unwrap_err();
        assert!(e.contains("no shop version yet") && e.contains("Roll out the release this PC runs first"), "{e}");
        rollout(&lib, &fake_release(&d, "0.4.0"), &sam(), "0.4.0").unwrap();
        // Its package damaged: nothing to go back to.
        std::fs::write(release_dir(&lib, "0.4.0").join("KurfLabs.nestpro-0.4.0-full.nupkg"), "x").unwrap();
        let e = start_trial(&lib, &d.join("Trials"), "0.5.0", &sam(), &c, "0.4.0", &Default::default()).unwrap_err();
        assert!(e.contains("couldn't be gone back to"), "{e}");
        rollout(&lib, &fake_release(&d, "0.4.0"), &sam(), "0.4.0").unwrap();
        let t = start_trial(&lib, &d.join("Trials"), "0.5.0", &sam(), &c, "0.4.0", &Default::default()).unwrap();
        // Stop trying after 0.5.0 installed: the shop has a version to go
        // back to, and is read-only on this PC until then.
        assert_eq!(t.mark.shop_version.as_deref(), Some("0.4.0"));
        assert!(matches!(crate::engine::updates::from_shop(&lib, "0.5.0"), Ok(crate::engine::updates::FromShop::Update { back: true, ref version, .. }) if version == "0.4.0"));
        assert!(shop::versions(&lib, "0.5.0").at(&lib.join("index.json")).ahead().is_some());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// S4: a shop folder that is `Documents\Sheet Nesting` itself never gets
    /// the trial copy: a place inside the shop (also by another name, a
    /// link to it) is refused before the backup is taken or a folder made.
    #[test]
    fn a_trial_copy_never_goes_inside_the_shop_folder() {
        let d = tmp("rollout_trial_place");
        let lib = acme(&d, "0.4.0");
        rollout(&lib, &fake_release(&d, "0.4.0"), &sam(), "0.4.0").unwrap();
        let c = Clock::at(1_794_216_600);
        let mut places = vec![lib.join("Trials"), lib.join("Library").join("..").join("Trials")];
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&lib, d.join("link")).unwrap();
            places.push(d.join("link").join("Trials"));
        }
        for root in places {
            let e = start_trial(&lib, &root, "0.5.0", &sam(), &c, "0.4.0", &Default::default()).unwrap_err();
            assert!(e.contains("inside the shop folder") && e.starts_with("Nothing was tried"), "{e}");
        }
        assert!(!lib.join("Trials").exists(), "a trial folder was made in the shop");
        assert_eq!(crate::backup::backups_in(&lib, &lib.join(crate::backup::FOLDER)).map(|l| l.len()).unwrap_or(0), 0, "a backup was taken for a trial that was refused");
        assert!(shop::read(&lib).unwrap().unwrap().trial().is_none());
        let t = start_trial(&lib, &d.join("pc").join("Trials"), "0.5.0", &sam(), &c, "0.4.0", &Default::default()).unwrap();
        assert!(!crate::shared::inside(&t.copy, &lib), "{}", t.copy.display());
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
pub use tests::fake_release;
