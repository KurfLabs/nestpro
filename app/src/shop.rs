//! The shop (spec #178, ADR 0003, ADR 0006): the PCs that share one Library.
//! The shop folder is the Library's own folder. Starting a shop adds the shop
//! file (`shop.json`) and its `Shop`, `Updates` and `Backups` folders in it,
//! and nothing else moves (SH1, SH2). Each PC keeps its own small record in
//! `Shop`, so no two PCs write the same file for the PCs list (SH4).
//!
//! This module reads and writes those files. The rules (who may start a
//! shop, change the admin PC, and what is shown) are in `engine::shop`.

use crate::shared::{self, Saved, Who, WriteLock};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// The shop file, in the shop folder beside the Library's list (SH2).
pub const FILE: &str = "shop.json";
/// Each PC's record (SH4).
pub const PCS: &str = "Shop";
/// Releases rolled out to the shop (ADR 0006).
pub const UPDATES: &str = "Updates";
/// The shop's backups.
pub const BACKUPS: &str = "Backups";
/// The folders starting a shop adds (SH2).
pub const FOLDERS: [&str; 3] = [PCS, UPDATES, BACKUPS];

/// Who may change shop settings (SH3): a new shop, only the admin PC.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SettingsBy {
    #[default]
    AdminPc,
    AnyPc,
}

impl SettingsBy {
    /// "only on the admin PC", as Settings → Shop and agents say it.
    pub fn words(self) -> &'static str {
        match self {
            SettingsBy::AdminPc => "only on the admin PC",
            SettingsBy::AnyPc => "from any PC",
        }
    }
}

/// The shop's admin PC, and who made it the admin PC and when (SH3, SH7).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Admin {
    pub pc: String,
    pub by: Who,
    pub at: u64,
    /// The admin PC before this one, when it was changed (SH7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub was: Option<String>,
}

/// The shop file (SH3).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShopFile {
    pub name: String,
    pub admin: Admin,
    #[serde(default)]
    pub settings_by: SettingsBy,
    /// Who started the shop, and when.
    pub started: Saved,
    /// Keep and the second place of the shop's backups (BK4, BK5).
    #[serde(default)]
    pub backups: crate::backup::Settings,
    /// What later versions keep in the shop file, kept as it was when this
    /// version writes it again.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// One PC's record in `Shop` (SH4): written by that PC when it opens the
/// program.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PcRecord {
    pub pc: String,
    /// The Windows user who opened the program last on it.
    pub user: String,
    /// The program's version on it.
    pub version: String,
    /// When it opened the program last, by its own clock (seconds since 1970).
    pub last_seen: u64,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// The shop file of a shop folder.
pub fn file(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// A PC's record in the shop folder: a short readable part of its name and
/// a key made from the whole name (`PC-A-3f1c…json`), so two PCs whose names
/// start alike, or differ only in signs a file name can't hold, never share a
/// record (SH4).
pub fn record_file(dir: &Path, pc: &str) -> PathBuf {
    let short: String = shared::file_safe(pc).chars().take(24).collect();
    dir.join(PCS).join(format!("{short}-{:016x}.json", name_key(pc)))
}

/// FNV-1a of the PC's whole name: the same on every PC and every version.
fn name_key(pc: &str) -> u64 {
    pc.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// The shop in `dir`: None when the folder holds no shop file. A folder that
/// can't be reached is refused, naming it (SH8, LB11): never taken for a
/// folder without a shop.
pub fn read(dir: &Path) -> Result<Option<ShopFile>, String> {
    if !dir.is_dir() {
        return Err(crate::library::unreachable(dir));
    }
    let f = file(dir);
    match std::fs::read(&f) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("The shop file {} can't be read ({e}). Check the network or the server.", f.display())),
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|e| format!("The shop file {} can't be read ({e}).", f.display())),
    }
}

/// Write the shop file, under the Library's write lock (LB1, LB3),
/// recording `version` as the program that wrote it (UP5). Refused when a
/// newer program wrote it last (UP7). Never makes the shop folder: a
/// Library that went away fails the write (SH2).
pub fn write(dir: &Path, shop: &ShopFile, lock: &WriteLock, version: &str) -> Result<(), String> {
    write_judged(dir, shop, lock, version, &versions(dir, version))
}

/// `write`, judged by `versions` (UP19: rolling out, the admin PC runs the
/// release it rolls out, newer than the shop version it replaces, so only
/// the file's own version counts).
pub fn write_judged(dir: &Path, shop: &ShopFile, lock: &WriteLock, version: &str, versions: &shared::Versions) -> Result<(), String> {
    shared::may_write_over(&file(dir), versions, "The shop file")?;
    // The version this program records replaces the one it was read with.
    let mut shop = shop.clone();
    shop.other.remove(shared::VERSION_FIELD);
    let text = shared::stamped(&shop, version)?;
    shared::write_existing(&file(dir), text.as_bytes(), lock)
}

/// The shop file's field for the shop version: the release every PC in the
/// shop runs. Rolling out sets it (spec #178); a shop that hasn't rolled one
/// out has none.
pub const SHOP_VERSION: &str = "shop_version";

/// The shop version this shop rolled out before the one it is on now, kept
/// for roll back (UP19).
pub const PREVIOUS_VERSION: &str = "previous_shop_version";
/// The file layouts of the release the shop rolled out last (UP19, UP20):
/// `{"jobs": 2, "library": 1, …}`.
pub const LAYOUTS: &str = "layouts";
/// The one-time changes made to the shop's files, by their names (UP18).
pub const CHANGES_MADE: &str = "one_time_changes";
/// The mark of a trial copy (UP16): only ever in a copy, never in a shop.
pub const TRIAL: &str = "trial";

/// UP16: a trial copy's mark, in the copy's shop file: the shop it was
/// copied from, the release tried on it and the shop version then.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrialMark {
    /// The shop folder, as this PC names it.
    pub shop_folder: String,
    /// The release tried.
    pub version: String,
    /// The shop version when the trial began (None: none was rolled out).
    #[serde(default)]
    pub shop_version: Option<String>,
    pub by: Who,
    pub at: u64,
}

impl ShopFile {
    /// The shop version: the release every PC in the shop runs (ADR 0006).
    /// None while none is rolled out.
    pub fn shop_version(&self) -> Option<&str> {
        self.other.get(SHOP_VERSION).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty())
    }

    /// The shop version before the last roll out (UP19).
    pub fn previous_version(&self) -> Option<&str> {
        self.other.get(PREVIOUS_VERSION).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty())
    }

    /// UP16: the mark of a trial copy; None for a shop.
    pub fn trial(&self) -> Option<TrialMark> {
        self.other.get(TRIAL).and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// UP18: the one-time changes made to these files.
    pub fn changes_made(&self) -> Vec<String> {
        self.other.get(CHANGES_MADE).and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default()
    }

    /// UP20: the file layouts recorded at the last roll out; None before.
    pub fn layouts(&self) -> Option<std::collections::BTreeMap<String, u32>> {
        self.other.get(LAYOUTS).and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}

/// UP7, UP8, UP21: the versions a Library file in `dir` is judged by: this
/// program's `this`, and the shop's version when `dir` is a shop folder that
/// records one. A shop file that can't be read just now names no shop
/// version (the refusal then names only the file's). The releases the shop
/// rolled back from (UP10) are filled in by roll back; none yet. A PC on a
/// release newer than the shop version, in the shop itself (not a trial
/// copy), gets the whole shop read-only (UP21).
pub fn versions(dir: &Path, this: &str) -> shared::Versions {
    let s = read(dir).ok().flatten();
    let shop = s.as_ref().and_then(|s| s.shop_version().map(String::from));
    let trial = s.as_ref().is_some_and(|s| s.trial().is_some());
    let back_to = shop.clone().filter(|v| !trial && shared::version_order(this, v) == Some(std::cmp::Ordering::Greater));
    shared::Versions { this: this.to_string(), shop, rolled_back_from: vec![], back_to }
}

/// Write this PC's record, under the Library's write lock (LB1, LB3). The
/// `Shop` folder is made again if someone took it away, but only inside a
/// shop folder that is there: never the shop folder or those above it (SH2).
pub fn write_record(dir: &Path, rec: &PcRecord, lock: &WriteLock) -> Result<(), String> {
    let text = serde_json::to_vec_pretty(rec).map_err(|e| e.to_string())?;
    let pcs = dir.join(PCS);
    if !pcs.is_dir() {
        lock.still_held()?;
        match std::fs::create_dir(&pcs) {
            Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => return Err(format!("{} can't be made ({e})", pcs.display())),
            _ => {}
        }
    }
    shared::write_existing(&record_file(dir, &rec.pc), &text, lock)
}

/// Every PC's record in the shop, by PC name. A record that is damaged is
/// left out; a shop folder or `Shop` folder that is missing or can't be read
/// (the share dropped while reading it) is an error naming it, never an
/// empty list.
pub fn records(dir: &Path) -> Result<Vec<PcRecord>, String> {
    let pcs = dir.join(PCS);
    let failed = |e: std::io::Error| {
        if dir.is_dir() { format!("The PCs list in {} can't be read ({e}). Check the network or the server.", pcs.display()) } else { crate::library::unreachable(dir) }
    };
    let list = match std::fs::read_dir(&pcs) {
        // Starting a shop makes it: one that's missing was taken away or
        // can't be seen, never "no PCs".
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && dir.is_dir() => {
            return Err(format!("The PCs list folder {} is missing. A PC adds it again when it adds itself to the shop.", pcs.display()))
        }
        r => r.map_err(failed)?,
    };
    let mut out = vec![];
    for e in list {
        let p = e.map_err(failed)?.path();
        if !p.extension().is_some_and(|x| x == "json") {
            continue;
        }
        match std::fs::read(&p) {
            Ok(bytes) => out.extend(serde_json::from_slice::<PcRecord>(&bytes).ok()),
            // Gone since the folder was listed (a PC's record replaced).
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(failed(e)),
        }
    }
    out.sort_by_key(|r| r.pc.to_lowercase());
    Ok(out)
}

// ---------------------------------------------------------------------------
// The shop folder as a PC names it (SH13): every PC should name the same
// folder the same way, so a folder typed or picked is tidied, and one picked
// through a mapped drive letter is named by the `\\server\share` path behind
// it, where Windows gives one.
// ---------------------------------------------------------------------------

/// A folder as typed or pasted, tidied (SH13): no spaces or quotes around it
/// (Explorer's Copy as path adds quotes), no doubled separators, and no
/// separator at its end (but a drive's or the system's root keeps its own).
/// With `windows`, `/` and `\` are both separators and it is written with
/// `\`; a `\\server\share` path keeps its two leading backslashes.
pub fn tidy_folder(typed: &str, windows: bool) -> String {
    let t = typed.trim();
    let t = t.strip_prefix('"').and_then(|t| t.strip_suffix('"')).unwrap_or(t).trim();
    if t.is_empty() {
        return String::new();
    }
    let sep = if windows { '\\' } else { '/' };
    let is_sep = |c: char| c == '/' || (windows && c == '\\');
    let unc = windows && t.chars().take(2).all(is_sep) && t.chars().count() > 2;
    let mut out = String::new();
    let mut last_sep = false;
    for c in t.chars() {
        if is_sep(c) {
            if !last_sep {
                out.push(sep);
            }
            last_sep = true;
        } else {
            out.push(c);
            last_sep = false;
        }
    }
    if unc {
        out.insert(0, '\\');
    }
    // No separator at the end, but `C:\` and `/` stay roots.
    while out.ends_with(sep) && out.len() > 1 && !(windows && out.len() == 3 && out.as_bytes()[1] == b':') {
        out.pop();
    }
    out
}

/// The drive letter a Windows path starts with, and the rest after it
/// (`Z:\Nesting` → `('Z', "\Nesting")`); None for any other path.
pub fn drive_of(path: &str) -> Option<(char, &str)> {
    let mut cs = path.chars();
    let (l, colon) = (cs.next()?, cs.next()?);
    let rest = &path[2..];
    (l.is_ascii_alphabetic() && colon == ':' && (rest.is_empty() || rest.starts_with('\\'))).then(|| (l.to_ascii_uppercase(), rest))
}

/// The folder on the share: the share behind a drive letter and the rest of
/// the path after the letter (`\\srv\Nesting` + `\Jobs`).
pub fn on_share(share: &str, rest: &str) -> String {
    format!("{}{rest}", share.trim_end_matches('\\'))
}

/// SH13: a tidied folder as every PC should name it, and what was changed:
/// on Windows, a folder on a mapped drive letter becomes the `\\server\share`
/// path behind it ("Z: is \\srv\Nesting on this PC…"). Any other folder, a
/// local drive, or any folder on other systems, stays as it is.
pub fn full_folder(tidy: &str) -> (String, Option<String>) {
    let Some((letter, rest)) = drive_of(tidy) else { return (tidy.to_string(), None) };
    match mapped_share(letter) {
        Some(share) => {
            let full = on_share(&share, rest);
            let note = format!("{letter}: is {share} on this PC: using {full}, so every PC names the same Library.");
            (full, Some(note))
        }
        None => (tidy.to_string(), None),
    }
}

/// The `\\server\share` a drive letter is mapped to on this PC (Windows);
/// None for a local drive, or on other systems.
#[cfg(windows)]
fn mapped_share(letter: char) -> Option<String> {
    #[link(name = "mpr")]
    unsafe extern "system" {
        fn WNetGetConnectionW(local: *const u16, remote: *mut u16, len: *mut u32) -> u32;
    }
    let local: Vec<u16> = format!("{letter}:").encode_utf16().chain([0]).collect();
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: `local` ends with a 0; `buf` holds `len` u16s and lives
    // through the call, which writes at most that many.
    let r = unsafe { WNetGetConnectionW(local.as_ptr(), buf.as_mut_ptr(), &mut len) };
    if r != 0 {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let share = String::from_utf16_lossy(&buf[..end]);
    share.starts_with("\\\\").then_some(share)
}

#[cfg(not(windows))]
fn mapped_share(_letter: char) -> Option<String> {
    None
}

/// Two names of the same folder (SH14): the same once tidied (without case
/// on Windows), or the same folder on disk.
pub fn same_folder(a: &Path, b: &Path) -> bool {
    let w = cfg!(windows);
    let (x, y) = (tidy_folder(&a.display().to_string(), w), tidy_folder(&b.display().to_string(), w));
    (if w { x.eq_ignore_ascii_case(&y) } else { x == y }) || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

#[cfg(test)]
mod folder_tests {
    use super::*;

    /// SH13: what a person types or pastes is tidied the same way each time.
    #[test]
    fn a_folder_is_tidied_the_same_however_it_is_typed() {
        let w = |s| tidy_folder(s, true);
        assert_eq!(w(r"\\srv\Nesting"), r"\\srv\Nesting");
        assert_eq!(w(r"\\srv\Nesting\"), r"\\srv\Nesting");
        assert_eq!(w("//srv/Nesting/"), r"\\srv\Nesting");
        assert_eq!(w(r"\\srv/Nesting\\Jobs//"), r"\\srv\Nesting\Jobs");
        assert_eq!(w(r#"  "\\srv\Nesting"  "#), r"\\srv\Nesting");
        assert_eq!(w(r"Z:\Nesting\"), r"Z:\Nesting");
        assert_eq!(w("Z:/Nesting"), r"Z:\Nesting");
        assert_eq!(w(r"C:\"), r"C:\");
        assert_eq!(w("C:/"), r"C:\");
        assert_eq!(w("   "), "");
        let u = |s| tidy_folder(s, false);
        assert_eq!(u("/srv/nesting/"), "/srv/nesting");
        assert_eq!(u("/srv//nesting///"), "/srv/nesting");
        assert_eq!(u("/"), "/");
        assert_eq!(u("'quoted'"), "'quoted'");
    }

    /// SH13: a mapped drive's folder is named on the share behind it.
    #[test]
    fn a_folder_on_a_mapped_drive_is_named_on_its_share() {
        assert_eq!(drive_of(r"Z:\Nesting"), Some(('Z', r"\Nesting")));
        assert_eq!(drive_of("z:"), Some(('Z', "")));
        assert_eq!(drive_of(r"Z:Nesting"), None);
        assert_eq!(drive_of(r"\\srv\Nesting"), None);
        assert_eq!(drive_of("/srv/nesting"), None);
        assert_eq!(on_share(r"\\srv\Nesting", r"\Library"), r"\\srv\Nesting\Library");
        assert_eq!(on_share(r"\\srv\Nesting\", ""), r"\\srv\Nesting");
        // Nothing to convert on other systems, nor for a share path.
        if !cfg!(windows) {
            assert_eq!(full_folder(r"Z:\Nesting"), (r"Z:\Nesting".to_string(), None));
        }
        assert_eq!(full_folder(r"\\srv\Nesting"), (r"\\srv\Nesting".to_string(), None));
    }
}
