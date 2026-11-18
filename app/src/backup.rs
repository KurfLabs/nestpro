//! The Library's backups (spec #178, BK1–BK7): one zip of the Library a day,
//! taken by the first PC that opens the program that day, kept for the
//! shop's Keep days, and copied to a second place when one is set.
//!
//! In a shop the zips go in the shop folder's `Backups` (made when the shop
//! was started, SH2); a PC of its own puts them in a `Backups` folder beside
//! its Library (BK6). A backup only reads the Library: it never moves,
//! changes or deletes a Library file. The only files it ever deletes are its
//! own: zips named like `Backup 2026-11-09 0920 PC-A.zip` past Keep, and its
//! own half-written `.part` files (BK4).
//!
//! The rules on who may change Keep and the second place, and the `shop`
//! operation, are in `engine::shop`.

use crate::shared::{self, Clock, Who, WriteLock};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// The backup folder's name (SH2).
pub const FOLDER: &str = crate::shop::BACKUPS;
/// Days of backups kept by default (BK4).
pub const DEFAULT_KEEP: u32 = 30;
/// The most days Keep may say.
pub const MOST_KEEP: u32 = 3650;
/// Every backup's file name starts so (BK1).
const PREFIX: &str = "Backup ";

fn default_keep() -> u32 {
    DEFAULT_KEEP
}

/// Keep and the second place (BK4, BK5): in the shop file for a shop, in
/// this PC's settings for a PC of its own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Days of backups kept; 0 (never offered) counts as 1 (BK4).
    #[serde(default = "default_keep")]
    pub keep_days: u32,
    /// Another drive or NAS each backup is copied to; empty for none (BK5).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub second_place: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { keep_days: DEFAULT_KEEP, second_place: String::new() }
    }
}

impl Settings {
    /// Keep, at least one day: the newest backup is never removed (BK4).
    pub fn keep(&self) -> u32 {
        self.keep_days.clamp(1, MOST_KEEP)
    }
}

/// One backup in a backup folder, from its file name (BK1).
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    /// Its day and time in the local time of the PC that took it.
    pub day: (i64, u32, u32),
    pub time: (u32, u32),
    /// The PC that took it, as its file name gives it.
    pub pc: String,
    /// 1, or 2, 3… for a second backup in the same minute.
    pub seq: u32,
    pub bytes: u64,
}

impl Entry {
    /// "9 Nov 09:20".
    pub fn shown(&self) -> String {
        let (_, m, d) = self.day;
        let month = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][(m as usize).clamp(1, 12) - 1];
        format!("{d} {month} {:02}:{:02}", self.time.0, self.time.1)
    }

    /// "2026-11-09 09:20" (agents).
    pub fn stamp(&self) -> String {
        let (y, m, d) = self.day;
        format!("{y}-{m:02}-{d:02} {:02}:{:02}", self.time.0, self.time.1)
    }
}

/// A backup's file name: `Backup 2026-11-09 0920 PC-A.zip`, with ` 2`, ` 3`…
/// after the PC when that name is taken (BK1).
fn name_for(day: (i64, u32, u32), time: (u32, u32), pc: &str, n: u32) -> String {
    let (y, m, d) = day;
    let more = if n > 1 { format!(" {n}") } else { String::new() };
    format!("{PREFIX}{y:04}-{m:02}-{d:02} {:02}{:02} {}{more}.zip", time.0, time.1, shared::file_safe(pc))
}

/// Day, time and PC of a backup's file name; None for any other file.
fn parse(name: &str) -> Option<((i64, u32, u32), (u32, u32), String, u32)> {
    let rest = name.strip_prefix(PREFIX)?.strip_suffix(".zip")?;
    let (date, rest) = rest.split_once(' ')?;
    let (hhmm, pc) = rest.split_once(' ')?;
    let num = |s: &str, n: usize| (s.len() == n && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse::<u32>().ok()).flatten();
    let mut p = date.split('-');
    let (y, m, d) = (num(p.next()?, 4)?, num(p.next()?, 2)?, num(p.next()?, 2)?);
    if p.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) || hhmm.len() != 4 {
        return None;
    }
    let (hh, mm) = (num(&hhmm[..2], 2)?, num(&hhmm[2..], 2)?);
    if hh > 23 || mm > 59 {
        return None;
    }
    // The PC, then a number when the name was taken.
    let (pc, seq) = match pc.rsplit_once(' ') {
        Some((pc, n)) if n.bytes().all(|b| b.is_ascii_digit()) && !n.is_empty() && n.len() < 6 => (pc, n.parse().ok()?),
        Some(_) => return None,
        None => (pc, 1),
    };
    if pc.is_empty() || !pc.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return None;
    }
    Some(((i64::from(y), m, d), (hh, mm), pc.to_string(), seq))
}

/// A half-written backup of this module's own (`<backup name>.<writer>.part`).
fn is_own_part(name: &str) -> bool {
    name.strip_suffix(".part").and_then(|n| n.rsplit_once(".zip.")).is_some_and(|(b, tag)| !tag.is_empty() && parse(&format!("{b}.zip")).is_some())
}

/// The backups in `dir`, newest first; only files named like a backup, so
/// anything else kept there is never counted, listed or removed (BK1, BK4).
pub fn list(dir: &Path) -> Vec<Entry> {
    let mut out: Vec<Entry> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let (day, time, pc, seq) = parse(&name)?;
            Some(Entry { path: e.path(), bytes: e.metadata().map_or(0, |m| m.len()), name, day, time, pc, seq })
        })
        .collect();
    out.sort_by(|a, b| (b.day, b.time, b.seq, &b.name).cmp(&(a.day, a.time, a.seq, &a.name)));
    out
}

/// Days since 1970 of a day (Howard Hinnant's days-from-civil).
fn day_number((y, m, d): (i64, u32, u32)) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

/// Day and time of `t` in the clock's local time.
fn local(clock: &Clock, t: u64) -> ((i64, u32, u32), (u32, u32)) {
    // "2026-11-09 09:20"
    let s = clock.stamp(t);
    let n = |a: usize, b: usize| s.get(a..b).and_then(|x| x.parse::<u32>().ok()).unwrap_or(0);
    ((i64::from(n(0, 4)), n(5, 7), n(8, 10)), (n(11, 13), n(14, 16)))
}

/// Where the backups of the Library in `library` go (BK1, BK6): the shop
/// folder's `Backups` in a shop; a `Backups` folder beside the Library for
/// a PC of its own. Never inside a Library that is no shop (it would be
/// zipped in the next backup), and never made where the folder it goes in
/// can't be reached.
pub fn folder(library: &Path, shop: bool) -> Result<PathBuf, String> {
    if shop {
        return Ok(library.join(FOLDER));
    }
    beside(library).ok_or_else(|| format!("The Library {} has no folder beside it to keep backups in (it's at the top of a drive or share). Move the Library into a folder, or start a shop there.", library.display()))
}

/// The `Backups` folder beside a Library, on either kind of path: `\` and
/// `/` both separate (a Windows path read on Linux too). None at the top of
/// a drive or share (`C:\`, `\\srv\Nesting`).
pub fn beside(library: &Path) -> Option<PathBuf> {
    let s = library.to_str()?.trim_end_matches(['\\', '/']);
    let cut = s.rfind(['\\', '/'])?;
    let parent = &s[..cut];
    let sep = &s[cut..=cut];
    // A share's top (`\\srv`), a drive (`C:`) or the root have nothing beside.
    let unc_server = (parent.starts_with("\\\\") || parent.starts_with("//")) && !parent[2..].contains(['\\', '/']);
    if parent.is_empty() || parent.ends_with(':') || unc_server {
        return None;
    }
    Some(PathBuf::from(format!("{parent}{sep}{FOLDER}")))
}

/// What a backup did (BK1–BK5).
#[derive(Clone, Debug)]
pub struct Done {
    pub entry: Entry,
    /// Files in it.
    pub files: usize,
    /// Library files that couldn't be read and aren't in it.
    pub skipped: Vec<String>,
    /// Old backups removed past Keep (BK4).
    pub removed: Vec<String>,
    /// The copy in the second place, or why there's none (BK5). None: no
    /// second place is set.
    pub second: Option<Result<PathBuf, String>>,
}

/// What `daily` found (BK2).
#[derive(Clone, Debug)]
pub enum Daily {
    /// This PC took the day's backup.
    Taken(Done),
    /// Another PC (or this one) already took it today.
    Done(Entry),
}

/// Why a backup is taken: the day's (skipped when one was taken today), or
/// Back up now (always).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    Daily,
    Now,
}

#[cfg(test)]
thread_local! {
    /// Test seam: a backup stops with an error after this many files, as one
    /// whose disk filled halfway does.
    pub static STOP_AFTER: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// Take a backup of the Library in `library` (BK1–BK6). `shop`: the Library
/// is a shop folder. `Why::Daily` takes none when a backup of today is
/// there already (BK2), looked at again under the write lock, so of two PCs
/// opening at once only one takes it.
///
/// Under the Library's write lock (LB1), renewed while the zip is written
/// (BK3), so no PC changes the Library while it is zipped. The zip is
/// written to a `.part` file and renamed when it's whole, so a half-written
/// one never counts (BK3). A Library that can't be reached is refused,
/// naming it, and nothing is made (LB11).
pub fn take(library: &Path, shop: bool, settings: &Settings, who: &Who, clock: &Clock, why: Why) -> Result<Daily, String> {
    crate::library::usable(library)?;
    let dir = folder(library, shop)?;
    let (today, now) = local(clock, clock.now());
    let of_today = |dir: &Path| list(dir).into_iter().find(|e| e.day == today);
    if why == Why::Daily
        && let Some(e) = of_today(&dir)
    {
        return Ok(Daily::Done(e));
    }
    let mut lock = WriteLock::take(library)?;
    if why == Why::Daily
        && let Some(e) = of_today(&dir)
    {
        return Ok(Daily::Done(e));
    }
    if !dir.is_dir() {
        // Only the folder itself, in a folder that is there: never the
        // Library or the folders it's in (LB11).
        match dir.parent() {
            Some(p) if p.is_dir() => std::fs::create_dir(&dir).map_err(|e| format!("No backup was taken: {} can't be made ({e}).", dir.display()))?,
            _ => return Err(format!("No backup was taken: {} can't be reached.", dir.display())),
        }
    }
    // Half-written backups left by a PC that stopped (BK3): only this
    // module's own, and only now that this PC holds the lock.
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        if e.file_name().to_str().is_some_and(is_own_part) {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let mut n = 1;
    let name = loop {
        let name = name_for(today, now, &who.pc, n);
        if !dir.join(&name).exists() {
            break name;
        }
        n += 1;
    };
    let path = dir.join(&name);
    let part = dir.join(format!("{name}.{}.part", shared::writer_tag()));
    let skip_top: &[&str] = if shop { &[crate::shop::UPDATES, crate::shop::BACKUPS] } else { &[] };
    let zipped = zip_library(library, &part, skip_top, &mut lock, clock);
    let (files, skipped) = match zipped.and_then(|r| lock.still_held().map(|()| r)) {
        Ok(r) => r,
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            return Err(format!("No backup was taken: {e}. Nothing in the Library was changed."));
        }
    };
    if let Err(e) = std::fs::rename(&part, &path) {
        let _ = std::fs::remove_file(&part);
        return Err(format!("No backup was taken: {} can't be written ({e}).", path.display()));
    }
    let removed = prune(&dir, settings.keep(), today);
    drop(lock);
    let entry = list(&dir).into_iter().find(|e| e.name == name).unwrap_or(Entry { name: name.clone(), path: path.clone(), day: today, time: now, pc: shared::file_safe(&who.pc), seq: n, bytes: 0 });
    let second = (!settings.second_place.trim().is_empty()).then(|| copy_to_second(&path, Path::new(settings.second_place.trim()), library, settings.keep(), today));
    Ok(Daily::Taken(Done { entry, files, skipped, removed, second }))
}

/// Remove the backups in `dir` past Keep (BK4): those `keep` days old or
/// more, by this PC's day, that aren't among the newest `keep` backups. So
/// the newest is never removed, and a clock that went wrong (backwards, or
/// years ahead) never takes the backups that are there with it. Only files
/// named like a backup.
fn prune(dir: &Path, keep: u32, today: (i64, u32, u32)) -> Vec<String> {
    let today = day_number(today);
    let mut removed = vec![];
    for (i, e) in list(dir).into_iter().enumerate() {
        if i >= keep as usize && today - day_number(e.day) >= i64::from(keep) && std::fs::remove_file(&e.path).is_ok() {
            removed.push(e.name);
        }
    }
    removed
}

/// Copy a backup to the second place (BK5), then remove that place's own
/// backups past Keep. A place that can't be reached or written is reported,
/// never made, and stops nothing.
fn copy_to_second(zip: &Path, place: &Path, library: &Path, keep: u32, today: (i64, u32, u32)) -> Result<PathBuf, String> {
    if !place.is_dir() {
        return Err(format!("The second place {} can't be reached, so this backup is only in {}.", place.display(), zip.parent().map_or(String::new(), |p| p.display().to_string())));
    }
    let same = |a: &Path, b: &Path| matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y || x.starts_with(&y));
    if zip.parent().is_some_and(|p| same(place, p)) || same(place, library) {
        return Err(format!("The second place {} is the backup folder or in the Library, so it isn't a second place. Pick another drive or a NAS.", place.display()));
    }
    let name = zip.file_name().ok_or("no name")?;
    let to = place.join(name);
    let part = place.join(format!("{}.{}.part", name.to_string_lossy(), shared::writer_tag()));
    let copied = std::fs::copy(zip, &part).and_then(|_| std::fs::rename(&part, &to));
    if let Err(e) = copied {
        let _ = std::fs::remove_file(&part);
        return Err(format!("This backup couldn't be copied to the second place {} ({e}).", place.display()));
    }
    prune(place, keep, today);
    Ok(to)
}

/// Zip every file of the Library into `to`, but the top-level folders in
/// `skip_top`, the write lock's own files and links. Renews the lock as it
/// goes (BK3). A file that can't be read is left out and named; anything
/// else that goes wrong stops the backup.
fn zip_library(library: &Path, to: &Path, skip_top: &[&str], lock: &mut WriteLock, clock: &Clock) -> Result<(usize, Vec<String>), String> {
    use zip::write::SimpleFileOptions;
    let file = std::fs::File::create(to).map_err(|e| format!("{} can't be written ({e})", to.display()))?;
    let mut z = zip::ZipWriter::new(std::io::BufWriter::new(file));
    let (mut files, mut skipped) = (0usize, vec![]);
    let mut todo = vec![(library.to_path_buf(), String::new())];
    let mut buf = vec![0u8; 256 * 1024];
    let werr = |e: &dyn std::fmt::Display| format!("{} can't be written ({e})", to.display());
    while let Some((dir, rel)) = todo.pop() {
        let mut entries: Vec<_> = match std::fs::read_dir(&dir) {
            Ok(r) => r.flatten().collect(),
            Err(e) => return Err(format!("{} can't be read ({e})", dir.display())),
        };
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            lock.renew_if_due()?;
            let Ok(t) = e.file_type() else { continue };
            let fname = e.file_name().to_string_lossy().into_owned();
            let top = rel.is_empty();
            if top && (fname == shared::LOCK_FILE || fname.starts_with(&format!("{}.", shared::LOCK_FILE))) {
                continue;
            }
            let inner = if top { fname.clone() } else { format!("{rel}/{fname}") };
            if t.is_dir() {
                if top && skip_top.iter().any(|s| s.eq_ignore_ascii_case(&fname)) {
                    continue;
                }
                z.add_directory(format!("{inner}/"), SimpleFileOptions::default()).map_err(|e| werr(&e))?;
                todo.push((e.path(), inner));
                continue;
            }
            if !t.is_file() {
                continue;
            }
            let mut src = match std::fs::File::open(e.path()) {
                Ok(f) => f,
                Err(_) => {
                    skipped.push(inner);
                    continue;
                }
            };
            let meta = src.metadata().ok();
            let len = meta.as_ref().map_or(0, |m| m.len());
            let mut opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(len >= u64::from(u32::MAX) / 2);
            if let Some(t) = meta.and_then(|m| m.modified().ok()).and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()) {
                let ((y, mo, d), (h, mi)) = local(clock, t.as_secs());
                if let Ok(dt) = zip::DateTime::from_date_and_time(y.clamp(1980, 2107) as u16, mo as u8, d as u8, h as u8, mi as u8, 0) {
                    opts = opts.last_modified_time(dt);
                }
            }
            z.start_file(inner.as_str(), opts).map_err(|e| werr(&e))?;
            let read_all = loop {
                lock.renew_if_due()?;
                match src.read(&mut buf) {
                    Ok(0) => break true,
                    Ok(n) => z.write_all(&buf[..n]).map_err(|e| werr(&e))?,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break false,
                }
            };
            if !read_all {
                z.abort_file().map_err(|e| werr(&e))?;
                skipped.push(inner);
                continue;
            }
            files += 1;
            #[cfg(test)]
            if STOP_AFTER.with(|s| s.get()).is_some_and(|n| files >= n) {
                return Err("the test stopped it halfway".into());
            }
        }
    }
    let w = z.finish().map_err(|e| werr(&e))?;
    let f = w.into_inner().map_err(|e| werr(&e.error()))?;
    f.sync_all().map_err(|e| werr(&e))?;
    Ok((files, skipped))
}

/// What the backups of a Library are: where they go, the newest, how many,
/// Keep and the second place (BK7, Settings → Shop and agents).
#[derive(Clone, Debug)]
pub struct Overview {
    /// Where they go; Err when the Library has no place for them.
    pub folder: Result<PathBuf, String>,
    pub last: Option<Entry>,
    pub count: usize,
    pub settings: Settings,
}

pub fn overview(library: &Path, shop: bool, settings: &Settings) -> Overview {
    let folder = folder(library, shop);
    let all = folder.as_ref().map(|d| list(d)).unwrap_or_default();
    Overview { last: all.first().cloned(), count: all.len(), folder, settings: settings.clone() }
}

#[cfg(test)]
mod backup_tests {
    use super::*;

    #[test]
    fn only_names_of_its_own_count_as_backups() {
        assert_eq!(parse("Backup 2026-11-09 0920 PC-A.zip"), Some(((2026, 11, 9), (9, 20), "PC-A".into(), 1)));
        assert_eq!(parse("Backup 2026-11-09 0920 PC-A 2.zip"), Some(((2026, 11, 9), (9, 20), "PC-A".into(), 2)));
        for other in ["Backup 2026-11-09 0920 PC-A.zip.PC-A-1-2.part", "backup 2026-11-09 0920 PC-A.zip", "Backup 2026-11-09 PC-A.zip", "Backup 2026-13-09 0920 PC-A.zip", "Backup 2026-11-09 0920 PC A copy.zip", "Backup 2026-11-09 0920 PC-A.zip.bak", "Notes.zip", "Backup 2026-11-09 2460 PC-A.zip"] {
            assert_eq!(parse(other), None, "{other}");
        }
        assert!(is_own_part("Backup 2026-11-09 0920 PC-A.zip.PC-A-1-2.part"));
        assert!(!is_own_part("Backup 2026-11-09 0920 PC-A.zip"));
        assert!(!is_own_part("Report.zip.x.part"));
        assert_eq!(name_for((2026, 11, 9), (9, 5), "PC-Ä 1", 1), "Backup 2026-11-09 0905 PC-__1.zip");
        assert_eq!(parse(&name_for((2026, 11, 9), (9, 5), "PC-Ä 1", 3)).map(|p| p.2), Some("PC-__1".into()));
    }

    /// BK6: beside a Library on a Windows path, read on Linux too.
    #[test]
    fn a_pc_of_its_own_keeps_backups_beside_its_library() {
        let p = |s: &str| beside(Path::new(s)).map(|p| p.display().to_string());
        assert_eq!(p(r"D:\Sheet Nesting\Library"), Some(r"D:\Sheet Nesting\Backups".into()));
        assert_eq!(p(r"\\srv\Nesting\Library\"), Some(r"\\srv\Nesting\Backups".into()));
        assert_eq!(p("/srv/Sheet Nesting/Library"), Some("/srv/Sheet Nesting/Backups".into()));
        // At the top of a share or a drive there is nothing beside it.
        assert_eq!(p(r"\\srv\Nesting"), None);
        assert_eq!(p(r"C:\Library"), None);
        assert_eq!(p("/Library"), None);
        // In a shop: the shop folder's own Backups.
        assert_eq!(folder(Path::new(r"\\srv\Nesting"), true).unwrap(), Path::new(r"\\srv\Nesting").join("Backups"));
    }

    #[test]
    fn day_numbers_count_days() {
        assert_eq!(day_number((1970, 1, 1)), 0);
        assert_eq!(day_number((2026, 11, 9)) - day_number((2026, 10, 10)), 30);
        assert_eq!(day_number((2025, 3, 1)) - day_number((2025, 2, 28)), 1);
    }
}
