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
    shared::may_write_over(&file(dir), &versions(dir, version), "The shop file")?;
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

/// UP7, UP8: the versions a Library file in `dir` is judged by: this
/// program's `this`, and the shop's version when `dir` is a shop folder that
/// records one. A shop file that can't be read just now names no shop
/// version (the refusal then names only the file's). The releases the shop
/// rolled back from (UP10) are filled in by roll back; none yet.
pub fn versions(dir: &Path, this: &str) -> shared::Versions {
    let shop = read(dir).ok().flatten().and_then(|s| s.other.get(SHOP_VERSION).and_then(Value::as_str).map(String::from));
    shared::Versions { this: this.to_string(), shop, rolled_back_from: vec![] }
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
/// left out; a shop folder or `Shop` folder that can't be read (the share
/// dropped while reading it) is an error naming it, never an empty list.
pub fn records(dir: &Path) -> Result<Vec<PcRecord>, String> {
    let pcs = dir.join(PCS);
    let failed = |e: std::io::Error| {
        if dir.is_dir() { format!("The PCs list in {} can't be read ({e}). Check the network or the server.", pcs.display()) } else { crate::library::unreachable(dir) }
    };
    let list = match std::fs::read_dir(&pcs) {
        // No PC has recorded itself yet (or someone took the folder away).
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && dir.is_dir() => return Ok(vec![]),
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
