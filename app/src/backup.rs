//! The Library's backups (spec #178, BK1–BK7): one zip of the Library a day,
//! taken by the first PC that opens the program that day, kept for the
//! shop's Keep days, and copied to a second place when one is set.
//!
//! In a shop the zips go in the shop folder's `Backups` (made when the shop
//! was started, SH2); a PC of its own puts them in a `Backups` folder beside
//! its Library (BK6). A backup only reads the Library's files; the one file
//! it adds is the Library's backup id (`backup-id.txt`), once (BK1).
//!
//! Each Library's backups carry its id, in their file name and in a mark in
//! the zip (BK1), so a folder several Libraries back up into (one NAS for
//! all) never mixes them up: a backup counts, and is ever removed, only when
//! it is a whole zip marked with this Library's id (BK4). The only files a
//! backup ever deletes are this Library's own backups past Keep and its own
//! abandoned `.part` files.
//!
//! The rules on who may change Keep and the second place, and the `shop`
//! operation, are in `engine::shop`.

use crate::shared::{self, Clock, Who, WriteLock};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

/// The backup folder's name (SH2).
pub const FOLDER: &str = crate::shop::BACKUPS;
/// The Library's backup id, in the Library folder (BK1).
pub const ID_FILE: &str = "backup-id.txt";
/// Days of backups kept by default (BK4).
pub const DEFAULT_KEEP: u32 = 30;
/// The most days Keep may say.
pub const MOST_KEEP: u32 = 3650;
/// Every backup's file name starts so (BK1).
const PREFIX: &str = "Backup ";
/// The mark in every backup's zip comment, followed by `id=<the Library's id>`.
const MARK: &str = "nestpro-backup";
/// A `.part` file in a second place left this long is taken for one whose
/// PC stopped (its copy never takes this long).
const PART_LEFT: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

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

/// One backup of this Library, from its file name (BK1).
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
    /// Taken by a restore just before it changed the Library (BK9).
    pub before_restore: bool,
}

impl Entry {
    /// "9 Nov 09:20".
    pub fn shown(&self) -> String {
        shown_at(self.day, self.time)
    }

    /// "2026-11-09 09:20" (agents).
    pub fn stamp(&self) -> String {
        let (y, m, d) = self.day;
        format!("{y}-{m:02}-{d:02} {:02}:{:02}", self.time.0, self.time.1)
    }
}

/// "9 Nov 09:20".
fn shown_at((_, m, d): (i64, u32, u32), (hh, mm): (u32, u32)) -> String {
    let month = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][(m as usize).clamp(1, 12) - 1];
    format!("{d} {month} {hh:02}:{mm:02}")
}

/// A backup's "2026-11-09 09:20" (`Entry::stamp`) as "9 Nov 09:20"; as it
/// is when it isn't one.
pub fn shown_stamp(stamp: &str) -> String {
    let n = |a: usize, b: usize| stamp.get(a..b).filter(|x| x.bytes().all(|c| c.is_ascii_digit())).and_then(|x| x.parse::<u32>().ok());
    match (n(5, 7), n(8, 10), n(11, 13), n(14, 16)) {
        (Some(m), Some(d), Some(hh), Some(mm)) if stamp.len() == 16 && (1..=12).contains(&m) => shown_at((0, m, d), (hh, mm)),
        _ => stamp.to_string(),
    }
}

/// Whose backups: the Library's id, and the name shown in file names (the
/// shop's, or the Library folder's).
#[derive(Clone, Debug)]
struct Ns {
    id: String,
    label: String,
}

impl Ns {
    /// The id's first 8 signs, as file names carry it.
    fn short(&self) -> &str {
        &self.id[..8]
    }
}

/// A backup's file name: `Backup Acme 1a2b3c4d 2026-11-09 0920 PC-A.zip`,
/// with ` 2`, ` 3`… after the PC when that name is taken (BK1), and
/// ` before restore` at the end for the one a restore takes first (BK9).
fn name_for(ns: &Ns, day: (i64, u32, u32), time: (u32, u32), pc: &str, n: u32, before_restore: bool) -> String {
    let (y, m, d) = day;
    let more = if n > 1 { format!(" {n}") } else { String::new() };
    let label: String = shared::file_safe(&ns.label).chars().take(24).collect();
    let why = if before_restore { BEFORE_RESTORE } else { "" };
    format!("{PREFIX}{label} {} {y:04}-{m:02}-{d:02} {:02}{:02} {}{more}{why}.zip", ns.short(), time.0, time.1, shared::file_safe(pc))
}

/// The end of the name of a backup a restore takes first (BK9).
const BEFORE_RESTORE: &str = " before restore";

/// What a backup's file name says.
#[derive(Debug, PartialEq)]
struct Named {
    short: String,
    day: (i64, u32, u32),
    time: (u32, u32),
    pc: String,
    seq: u32,
    before_restore: bool,
}

/// What a backup's file name says; None for any other name. Only ASCII
/// digits are read as numbers (a name with other signs is no backup's).
fn parse(name: &str) -> Option<Named> {
    let rest = name.strip_prefix(PREFIX)?.strip_suffix(".zip")?;
    let (rest, before_restore) = match rest.strip_suffix(BEFORE_RESTORE) {
        Some(r) => (r, true),
        None => (rest, false),
    };
    let t: Vec<&str> = rest.split(' ').collect();
    if !(5..=6).contains(&t.len()) {
        return None;
    }
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let safe = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    let (label, short, date, hhmm, pc) = (t[0], t[1], t[2], t[3], t[4]);
    if !safe(label) || short.len() != 8 || !short.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) || !safe(pc) {
        return None;
    }
    let db = date.as_bytes();
    if db.len() != 10 || db[4] != b'-' || db[7] != b'-' || !digits(&date[..4]) || !digits(&date[5..7]) || !digits(&date[8..]) {
        return None;
    }
    if hhmm.len() != 4 || !digits(hhmm) {
        return None;
    }
    let (y, m, d): (i64, u32, u32) = (date[..4].parse().ok()?, date[5..7].parse().ok()?, date[8..].parse().ok()?);
    let (hh, mm): (u32, u32) = (hhmm[..2].parse().ok()?, hhmm[2..].parse().ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 {
        return None;
    }
    let seq = match t.get(5) {
        Some(n) if digits(n) && n.len() < 6 => n.parse().ok().filter(|n| *n >= 2)?,
        Some(_) => return None,
        None => 1,
    };
    Some(Named { short: short.into(), day: (y, m, d), time: (hh, mm), pc: pc.into(), seq, before_restore })
}

/// A half-written backup of this Library's own
/// (`<backup name>.<writer>.part`).
fn is_own_part(name: &str, ns: &Ns) -> bool {
    let Some((b, tag)) = name.strip_suffix(".part").and_then(|n| n.rsplit_once(".zip.")) else { return false };
    !tag.is_empty() && !tag.contains(' ') && parse(&format!("{b}.zip")).is_some_and(|n| n.short == ns.short())
}

/// Whether the file at `path` is a whole zip marked as a backup of the
/// Library with this `id` (BK1): its end record is there, and its comment
/// is the mark with the id. Reads only the file's last 2 KB.
fn marked(path: &Path, id: &str) -> bool {
    let read = || -> std::io::Result<Vec<u8>> {
        let mut f = std::fs::File::open(path)?;
        let len = f.metadata()?.len();
        let n = len.min(2048);
        f.seek(std::io::SeekFrom::End(-(n as i64)))?;
        let mut b = vec![0; n as usize];
        f.read_exact(&mut b)?;
        Ok(b)
    };
    let Ok(b) = read() else { return false };
    if b.len() < 22 {
        return false;
    }
    // The end record: its signature, then its comment's length at 20, and
    // the comment ends the file.
    for p in (0..=b.len() - 22).rev() {
        if b[p..p + 4] == [0x50, 0x4b, 0x05, 0x06] {
            let clen = usize::from(u16::from_le_bytes([b[p + 20], b[p + 21]]));
            if p + 22 + clen == b.len() {
                let c = String::from_utf8_lossy(&b[p + 22..]);
                let mut w = c.split_whitespace();
                return w.next() == Some(MARK) && w.any(|x| x == format!("id={id}"));
            }
        }
    }
    false
}

/// This Library's backups in `dir`, newest first (BK1, BK4): only whole
/// zips named and marked as its own. Anything else kept there (other
/// Libraries' backups, half-written ones, other files, links) is never
/// counted, listed or removed. A folder that isn't there holds none; one
/// that can't be read is an error naming it.
fn list(dir: &Path, ns: &Ns) -> Result<Vec<Entry>, String> {
    let rd = match std::fs::read_dir(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        r => r.map_err(|e| format!("{} can't be read ({e}).", dir.display()))?,
    };
    let mut out = vec![];
    for e in rd {
        let e = e.map_err(|e| format!("{} can't be read ({e}).", dir.display()))?;
        let Some(name) = e.file_name().to_str().map(str::to_string) else { continue };
        let Some(n) = parse(&name).filter(|n| n.short == ns.short()) else { continue };
        let path = e.path();
        // Never through a link: only a file that is itself in the folder.
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        if !meta.is_file() || meta.len() == 0 || !marked(&path, &ns.id) {
            continue;
        }
        out.push(Entry { path, bytes: meta.len(), name, day: n.day, time: n.time, pc: n.pc, seq: n.seq, before_restore: n.before_restore });
    }
    out.sort_by(|a, b| (b.day, b.time, b.seq, &b.name).cmp(&(a.day, a.time, a.seq, &a.name)));
    Ok(out)
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

/// A folder, with links followed, as it really is; for one not made yet,
/// the real folder it would be made in, with its name.
fn real(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| match (p.parent(), p.file_name()) {
        (Some(d), Some(n)) => std::fs::canonicalize(d).unwrap_or_else(|_| d.to_path_buf()).join(n),
        _ => p.to_path_buf(),
    })
}

/// Where the backups of the Library in `library` go (BK1, BK6): the shop
/// folder's `Backups` in a shop; a `Backups` folder beside the Library (in
/// the folder it is in) for a PC of its own. For a PC of its own, never the
/// Library itself or inside it (it would be zipped in the next backup), and
/// none at the top of a drive's root or a share.
pub fn folder(library: &Path, shop: bool) -> Result<PathBuf, String> {
    let lib = std::path::absolute(library).map_err(|e| format!("{} can't be read ({e}).", library.display()))?;
    if shop {
        return Ok(lib.join(FOLDER));
    }
    let Some(parent) = lib.parent().filter(|p| !p.as_os_str().is_empty()) else {
        return Err(format!("The Library {} has no folder beside it to keep backups in (it's at the top of a drive or share). Move the Library into a folder, or start a shop there.", library.display()));
    };
    let dir = parent.join(FOLDER);
    if dir == lib || real(&dir).starts_with(real(&lib)) {
        return Err(format!("The backups of the Library {} would go in {}, which is the Library itself. Rename the Library's folder, or start a shop there.", library.display(), dir.display()));
    }
    Ok(dir)
}

/// The backup folder as it is (BK1, BK6): none yet, or a real folder of its
/// own in the folder it should be in. A link or junction that takes it
/// elsewhere is refused, naming it: what it points to belongs to something
/// else, and backing up or removing there would be under the wrong lock.
fn check_folder(dir: &Path) -> Result<(), String> {
    let redirected = || format!("No backup was taken: {} is a link to another folder. Make it a plain folder (or remove it, and the program makes one).", dir.display());
    match std::fs::symlink_metadata(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("No backup was taken: {} can't be reached ({e}).", dir.display())),
        Ok(m) if m.file_type().is_symlink() => Err(redirected()),
        Ok(m) if !m.is_dir() => Err(format!("No backup was taken: {} isn't a folder.", dir.display())),
        Ok(_) => {
            // Where it really is: in the folder it's in, under its own name
            // (any case on Windows).
            let parent = dir.parent().map(real);
            let here = std::fs::canonicalize(dir).ok();
            let name_ok = here.as_ref().and_then(|h| h.file_name()).and_then(|n| n.to_str()).is_some_and(|n| if cfg!(windows) { n.eq_ignore_ascii_case(FOLDER) } else { n == FOLDER });
            if !name_ok || here.as_ref().and_then(|h| h.parent()) != parent.as_deref() {
                return Err(redirected());
            }
            Ok(())
        }
    }
}

/// The Library's backup id (BK1): None when it has none yet. One that can't
/// be read or is damaged is an error naming it, never replaced.
pub fn library_id(library: &Path) -> Result<Option<String>, String> {
    let f = library.join(ID_FILE);
    match std::fs::read_to_string(&f) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{} can't be read ({e}).", f.display())),
        Ok(t) => {
            let t = t.trim();
            if t.len() == 32 && t.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
                Ok(Some(t.to_string()))
            } else {
                Err(format!("{} is damaged: it should hold the Library's backup id. Put it back from a backup.", f.display()))
            }
        }
    }
}

/// Give the Library its backup id, under its write lock (BK1). Written only
/// into a Library folder that is there.
fn make_id(library: &Path, lock: &WriteLock) -> Result<String, String> {
    use std::hash::{BuildHasher, Hasher};
    let half = || {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write(shared::writer_tag().as_bytes());
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        h.finish()
    };
    let id = format!("{:016x}{:016x}", half(), half());
    shared::write_existing(&library.join(ID_FILE), id.as_bytes(), lock)?;
    Ok(id)
}

/// The Library's backup settings and place, as its files say now (BK7).
struct Plan {
    shop: bool,
    settings: Settings,
    label: String,
    dir: Result<PathBuf, String>,
    id: Option<String>,
}

/// Read whose settings a backup follows: the shop file's in a shop, `own`
/// (this PC's) otherwise; where the backups go, checked (BK6); and the
/// Library's backup id.
fn plan(library: &Path, own: &Settings) -> Result<Plan, String> {
    let (shop, settings, label) = match crate::shop::read(library)? {
        Some(s) => (true, s.backups, s.name),
        None => (false, own.clone(), std::path::absolute(library).ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_else(|| "Library".into())),
    };
    let dir = folder(library, shop).and_then(|d| check_folder(&d).map(|()| d));
    Ok(Plan { shop, settings, label, dir, id: library_id(library)? })
}

impl Plan {
    fn ns(&self) -> Option<Ns> {
        self.id.clone().map(|id| Ns { id, label: self.label.clone() })
    }

    /// This Library's backup of `today`, if there is one.
    fn of_today(&self, dir: &Path, today: (i64, u32, u32)) -> Result<Option<Entry>, String> {
        let Some(ns) = self.ns() else { return Ok(None) };
        Ok(list(dir, &ns)?.into_iter().find(|e| e.day == today))
    }

    /// Today's backup is there: copy it to the second place if it isn't
    /// there yet (the PC that took it stopped before copying, BK5), then
    /// tidy that place by the settings as they are under the lock.
    fn already(&self, library: &Path, own: &Settings, dir: &Path, entry: Entry, today: (i64, u32, u32)) -> Daily {
        let second = match (self.second_place(), self.ns()) {
            (Some(place), Some(ns)) => Some(copy_to_second(&entry, &place, library, dir, &ns).map(|(to, copied)| {
                if copied {
                    prune_second(library, own, &place, &ns, today);
                }
                to
            })),
            _ => None,
        };
        Daily::AlreadyToday { entry, second }
    }

    fn second_place(&self) -> Option<PathBuf> {
        let p = self.settings.second_place.trim();
        (!p.is_empty()).then(|| PathBuf::from(p))
    }
}

/// What a backup did (BK1–BK5).
#[derive(Clone, Debug)]
pub struct Done {
    pub entry: Entry,
    /// Files in it.
    pub files: usize,
    /// Old backups removed past Keep (BK4).
    pub removed: Vec<String>,
    /// The copy in the second place, or why there's none (BK5). None: no
    /// second place is set.
    pub second: Option<Result<PathBuf, String>>,
}

/// What `take` did (BK2).
#[derive(Clone, Debug)]
pub enum Daily {
    /// This PC took a backup.
    Taken(Done),
    /// Another PC (or this one) already took today's; its copy in the second
    /// place, made now if it was missing (BK5), or why there's none.
    AlreadyToday { entry: Entry, second: Option<Result<PathBuf, String>> },
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

/// Take a backup of the Library in `library` (BK1–BK6), following the shop
/// file's settings in a shop and `own` (this PC's) otherwise, as they are
/// once this PC holds the write lock. `Why::Daily` takes none when a backup
/// of today is there already (BK2), looked at again under the write lock,
/// so of two PCs opening at once only one takes it.
///
/// Under the Library's write lock (LB1), renewed while it works (BK3), so no
/// PC changes the Library while it is zipped. The zip is written to a
/// `.part` file and published under its name only when whole, never over
/// another file, so a half-written one never counts (BK3). Any file or
/// folder that can't be read, or that changes while it is zipped, stops the
/// backup: nothing is published and nothing removed. A Library that can't
/// be reached is refused, naming it, and nothing is made (LB11).
pub fn take(library: &Path, own: &Settings, who: &Who, clock: &Clock, why: Why) -> Result<Daily, String> {
    take_as(library, own, who, clock, why, false)
}

/// `take`; `before_restore`: the backup a restore takes first, named so
/// (BK9).
fn take_as(library: &Path, own: &Settings, who: &Who, clock: &Clock, why: Why, before_restore: bool) -> Result<Daily, String> {
    crate::library::usable(library)?;
    let (today, now) = local(clock, clock.now());
    if why == Why::Daily {
        // A first look without waiting for the lock: most openings stop here.
        let p = plan(library, own)?;
        let dir = p.dir.clone()?;
        if let Some(e) = p.of_today(&dir, today)? {
            return Ok(p.already(library, own, &dir, e, today));
        }
    }
    let mut lock = WriteLock::take(library)?;
    // Whose settings and where, as they are now that no PC can change them.
    let p = plan(library, own)?;
    let dir = p.dir.clone()?;
    if why == Why::Daily
        && let Some(e) = p.of_today(&dir, today)?
    {
        drop(lock);
        return Ok(p.already(library, own, &dir, e, today));
    }
    if !dir.is_dir() {
        // Only the folder itself, in a folder that is there: never the
        // Library or the folders it's in (LB11).
        match dir.parent() {
            Some(d) if d.is_dir() => {
                lock.still_held()?;
                std::fs::create_dir(&dir).map_err(|e| format!("No backup was taken: {} can't be made ({e}).", dir.display()))?;
                check_folder(&dir)?;
            }
            _ => return Err(format!("No backup was taken: {} can't be reached.", dir.display())),
        }
    }
    let ns = match p.ns() {
        Some(ns) => ns,
        None => Ns { id: make_id(library, &lock)?, label: p.label.clone() },
    };
    // Half-written backups of this Library's own left by a PC that stopped
    // (BK3): only now that this PC holds the lock, which every PC writing
    // one holds while it does.
    let parts: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| format!("No backup was taken: {} can't be read ({e}).", dir.display()))?
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(|n| is_own_part(n, &ns)))
        .map(|e| e.path())
        .collect();
    for part in parts {
        lock.renew_if_due()?;
        lock.still_held()?;
        let _ = std::fs::remove_file(&part);
        shared::pause("backup: part cleared");
    }
    let mut n = 1;
    let name = loop {
        let name = name_for(&ns, today, now, &who.pc, n, before_restore);
        if std::fs::symlink_metadata(dir.join(&name)).is_err() {
            break name;
        }
        n += 1;
    };
    let path = dir.join(&name);
    let part = dir.join(format!("{name}.{}.part", shared::writer_tag()));
    let skip_top: &[&str] = if p.shop { &[crate::shop::UPDATES, crate::shop::BACKUPS] } else { &[] };
    let mark = format!("{MARK} id={} pc={} at={}", ns.id, shared::file_safe(&who.pc), clock.now());
    let zipped = zip_library(library, &part, skip_top, &mark, &mut lock, clock).and_then(|files| {
        lock.renew_if_due()?;
        lock.still_held()?;
        publish(&part, &path)?;
        Ok(files)
    });
    let files = match zipped {
        Ok(f) => f,
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            return Err(format!("No backup was taken: {e}. Nothing in the Library was changed."));
        }
    };
    let removed = prune(&dir, &ns, p.settings.keep(), today, Some(&mut lock));
    drop(lock);
    let bytes = std::fs::metadata(&path).map_or(0, |m| m.len());
    let entry = Entry { name, path, day: today, time: now, pc: shared::file_safe(&who.pc), seq: n, bytes, before_restore };
    // The one a restore takes first goes to the second place once the
    // restore lets go of the lock (`to_second_place`, BK9): a slow copy never
    // holds the restore up.
    if before_restore {
        return Ok(Daily::Taken(Done { entry, files, removed, second: None }));
    }
    let second = p.second_place().map(|place| {
        copy_to_second(&entry, &place, library, &dir, &ns).map(|(to, _)| {
            prune_second(library, own, &place, &ns, today);
            to
        })
    });
    Ok(Daily::Taken(Done { entry, files, removed, second }))
}

/// Publish a whole `.part` file under its name, never over a file already
/// there (BK3, BK5): by a hard link where the folder allows one, else a
/// rename once nothing is there.
fn publish(part: &Path, to: &Path) -> Result<(), String> {
    let taken = || format!("{} is there already", to.display());
    match std::fs::hard_link(part, to) {
        Ok(()) => {
            let _ = std::fs::remove_file(part);
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(taken()),
        Err(_) => {
            if std::fs::symlink_metadata(to).is_ok() {
                return Err(taken());
            }
            std::fs::rename(part, to).map_err(|e| format!("{} can't be written ({e})", to.display()))
        }
    }
}

/// Remove this Library's backups in `dir` past Keep (BK4): those `keep`
/// days old or more, by this PC's day, that aren't among the newest `keep`
/// backups. So the newest is never removed, and a clock that went wrong
/// (backwards, or years ahead) never takes the backups that are there with
/// it. Only whole zips marked as this Library's. With `lock`, it is renewed
/// and checked before each removal; once it isn't this PC's, no more is
/// removed.
fn prune(dir: &Path, ns: &Ns, keep: u32, today: (i64, u32, u32), mut lock: Option<&mut WriteLock>) -> Vec<String> {
    let today = day_number(today);
    let mut removed = vec![];
    for (i, e) in list(dir, ns).unwrap_or_default().into_iter().enumerate() {
        if i < keep as usize || today - day_number(e.day) < i64::from(keep) {
            continue;
        }
        if let Some(l) = lock.as_deref_mut()
            && (l.renew_if_due().is_err() || l.still_held().is_err())
        {
            break;
        }
        if std::fs::remove_file(&e.path).is_ok() {
            removed.push(e.name);
        }
        shared::pause("backup: old one removed");
    }
    removed
}

/// Remove this Library's backups in the second place past Keep (BK4, BK5),
/// by Keep and the second place as they are once this PC holds the
/// Library's write lock (another PC may have just changed them), checking
/// the lock before each. Nothing when the lock can't be had or the second
/// place is another now.
fn prune_second(library: &Path, own: &Settings, place: &Path, ns: &Ns, today: (i64, u32, u32)) -> Vec<String> {
    shared::pause("backup: copied to second place");
    let Ok(mut lock) = WriteLock::take(library) else { return vec![] };
    let Ok(p) = plan(library, own) else { return vec![] };
    if p.second_place().as_deref() != Some(place) {
        return vec![];
    }
    prune(place, ns, p.settings.keep(), today, Some(&mut lock))
}

/// Copy a backup to the second place (BK5), and remove this Library's own
/// `.part` files there left a day or more. A place that can't be reached or
/// written is reported, never made, and stops nothing. A file of the same
/// name already there is never replaced: when it is this very backup, it is
/// the copy; otherwise it is reported. Gives the copy, and whether it was
/// made now.
fn copy_to_second(entry: &Entry, place: &Path, library: &Path, dir: &Path, ns: &Ns) -> Result<(PathBuf, bool), String> {
    let only_in = || format!("so this backup is only in {}", dir.display());
    if !place.is_dir() {
        return Err(format!("The second place {} can't be reached, {}.", place.display(), only_in()));
    }
    let rp = real(place);
    if rp == real(dir) || rp.starts_with(real(library)) {
        return Err(format!("The second place {} is the backup folder or in the Library, so it isn't a second place. Pick another drive or a NAS.", place.display()));
    }
    let to = place.join(&entry.name);
    if std::fs::symlink_metadata(&to).is_ok() {
        let same = std::fs::metadata(&to).is_ok_and(|m| m.is_file() && m.len() == entry.bytes) && marked(&to, &ns.id);
        return if same { Ok((to, false)) } else { Err(format!("The second place {} already holds another file named {}, {}.", place.display(), entry.name, only_in())) };
    }
    if let Ok(rd) = std::fs::read_dir(place) {
        for e in rd.flatten() {
            let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age >= PART_LEFT);
            if old && e.file_name().to_str().is_some_and(|n| is_own_part(n, ns)) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let part = place.join(format!("{}.{}.part", entry.name, shared::writer_tag()));
    let copied = (|| -> Result<(), String> {
        let werr = |e: std::io::Error| format!("{e}");
        let mut from = std::fs::File::open(&entry.path).map_err(werr)?;
        let mut out = std::fs::OpenOptions::new().write(true).create_new(true).open(&part).map_err(werr)?;
        std::io::copy(&mut from, &mut out).map_err(werr)?;
        out.sync_all().map_err(werr)?;
        drop(out);
        publish(&part, &to)
    })();
    if let Err(e) = copied {
        let _ = std::fs::remove_file(&part);
        return Err(format!("This backup couldn't be copied to the second place {} ({e}), {}.", place.display(), only_in()));
    }
    Ok((to, true))
}

/// A file's size and last change, to tell it changed while it was read.
fn stamp_of(path: &Path) -> std::io::Result<(u64, Option<std::time::SystemTime>)> {
    let m = std::fs::metadata(path)?;
    Ok((m.len(), m.modified().ok()))
}

/// Zip every file of the Library into a new file `to`, but the top-level
/// folders in `skip_top`, the write lock's own files and links, marking it
/// with `mark` (BK1). Renews the lock as it goes (BK3). A folder or file
/// that can't be read, or a file that changes while it's read (twice), stops
/// it with an error naming it.
fn zip_library(library: &Path, to: &Path, skip_top: &[&str], mark: &str, lock: &mut WriteLock, clock: &Clock) -> Result<usize, String> {
    use zip::write::SimpleFileOptions;
    let file = std::fs::OpenOptions::new().write(true).create_new(true).open(to).map_err(|e| format!("{} can't be written ({e})", to.display()))?;
    let mut z = zip::ZipWriter::new(std::io::BufWriter::new(file));
    let mut files = 0usize;
    let mut todo = vec![(library.to_path_buf(), String::new())];
    let mut buf = vec![0u8; 256 * 1024];
    let werr = |e: &dyn std::fmt::Display| format!("{} can't be written ({e})", to.display());
    let rerr = |p: &Path, e: &dyn std::fmt::Display| format!("{} can't be read ({e})", p.display());
    while let Some((dir, rel)) = todo.pop() {
        let mut entries = vec![];
        for e in std::fs::read_dir(&dir).map_err(|e| rerr(&dir, &e))? {
            entries.push(e.map_err(|e| rerr(&dir, &e))?);
        }
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            lock.renew_if_due()?;
            let path = e.path();
            let t = e.file_type().map_err(|x| rerr(&path, &x))?;
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
                todo.push((path, inner));
                continue;
            }
            if !t.is_file() {
                continue;
            }
            let mut whole = false;
            for _ in 0..2 {
                let before = stamp_of(&path).map_err(|x| rerr(&path, &x))?;
                let mut src = std::fs::File::open(&path).map_err(|x| rerr(&path, &x))?;
                let mut opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(before.0 >= u64::from(u32::MAX) / 2);
                if let Some(t) = before.1.and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()) {
                    let ((y, mo, d), (h, mi)) = local(clock, t.as_secs());
                    if let Ok(dt) = zip::DateTime::from_date_and_time(y.clamp(1980, 2107) as u16, mo as u8, d as u8, h as u8, mi as u8, 0) {
                        opts = opts.last_modified_time(dt);
                    }
                }
                z.start_file(inner.as_str(), opts).map_err(|e| werr(&e))?;
                let mut read = 0u64;
                loop {
                    lock.renew_if_due()?;
                    match src.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            z.write_all(&buf[..n]).map_err(|e| werr(&e))?;
                            read += n as u64;
                        }
                        Err(x) if x.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(x) => return Err(rerr(&path, &x)),
                    }
                }
                drop(src);
                shared::pause("backup: file read");
                let after = stamp_of(&path).map_err(|x| rerr(&path, &x))?;
                if after == before && read == before.0 {
                    whole = true;
                    break;
                }
                z.abort_file().map_err(|e| werr(&e))?;
            }
            if !whole {
                return Err(format!("{} changed while it was backed up", path.display()));
            }
            files += 1;
            #[cfg(test)]
            if STOP_AFTER.with(|s| s.get()).is_some_and(|n| files >= n) {
                return Err("the test stopped it halfway".into());
            }
        }
    }
    z.set_comment(mark).map_err(|e| werr(&e))?;
    let w = z.finish().map_err(|e| werr(&e))?;
    let f = w.into_inner().map_err(|e| werr(&e.error()))?;
    f.sync_all().map_err(|e| werr(&e))?;
    Ok(files)
}

/// What the backups of a Library are: where they go, the newest, how many,
/// Keep and the second place, and whether it's a shop's (BK7, Settings →
/// Shop and agents).
#[derive(Clone, Debug)]
pub struct Overview {
    /// Where they go; Err when the Library has no place for them.
    pub folder: Result<PathBuf, String>,
    pub last: Option<Entry>,
    pub count: usize,
    pub settings: Settings,
    pub shop: bool,
    /// Every backup of this Library there, newest first (Restore, BK8).
    pub all: Vec<Entry>,
}

/// The backups of the Library in `library`, `own` being this PC's settings
/// for a Library that is no shop. A Library or backup folder that can't be
/// read is an error naming it.
pub fn overview(library: &Path, own: &Settings) -> Result<Overview, String> {
    let p = plan(library, own)?;
    let all = match (&p.dir, p.ns()) {
        (Ok(d), Some(ns)) => list(d, &ns)?,
        _ => vec![],
    };
    Ok(Overview { last: all.first().cloned(), count: all.len(), folder: p.dir, settings: p.settings, shop: p.shop, all })
}

// ---------------------------------------------------------------------------
// Restore (BK8–BK13): a backup put back over the shop folder. Who may (the
// admin PC, a person) is in `engine::shop`; this is how it is done.
// ---------------------------------------------------------------------------

/// The Library's top-level folders and files a restore never takes from a
/// backup, and never changes or removes (BK10): the shop's releases and
/// backups, each PC's record, the holds on jobs in use, the shop file (it
/// records the restore), the backup id and the write lock's own files. They
/// say what the shop and its PCs are now, not what the Library held.
fn kept_live(top: &str) -> bool {
    let lock = shared::LOCK_FILE;
    [crate::shop::UPDATES, crate::shop::BACKUPS, crate::shop::PCS, crate::engine::inuse::FOLDER, crate::shop::FILE, ID_FILE, lock].iter().any(|k| top.eq_ignore_ascii_case(k))
        || top.get(..lock.len()).is_some_and(|h| h.eq_ignore_ascii_case(lock)) && top[lock.len()..].starts_with('.')
}

/// The name Windows keeps for a device (`CON`, `NUL`, `COM1`…), with or
/// without an extension: never a file there.
fn is_device(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    let numbered = stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && matches!(stem.as_bytes()[3], b'1'..=b'9');
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") || numbered
}

/// A zip entry's name as the folders and file it goes to inside the Library
/// (BK11), split at `/` or `\` (a zip another tool made on Windows). A name
/// that could reach outside the Library (`..`, starting with `/` or `\`, a
/// drive or anything with `:`) is refused, naming it.
fn entry_parts(raw: &str) -> Result<Vec<String>, String> {
    let outside = || format!("it holds a file named \"{raw}\", which would go outside the shop folder");
    if raw.starts_with(['/', '\\']) {
        return Err(outside());
    }
    let mut out = vec![];
    for part in raw.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return Err(outside()),
            p if p.contains(':') || p.chars().any(char::is_control) => return Err(outside()),
            p if cfg!(windows) && is_device(p) => return Err(format!("it holds a file named \"{raw}\", which Windows can't keep as a file")),
            p => out.push(p.to_string()),
        }
    }
    if out.is_empty() {
        return Err(format!("it holds an entry with no name (\"{raw}\")"));
    }
    Ok(out)
}

/// A path's parts as one key: the same file on this system (names differing
/// only in capitals are one file on Windows).
fn key(parts: &[String]) -> String {
    let j = parts.join("/");
    if cfg!(windows) { j.to_lowercase() } else { j }
}

fn join(base: &Path, parts: &[String]) -> PathBuf {
    parts.iter().fold(base.to_path_buf(), |p, x| p.join(x))
}

/// A Library record (LB7): `Jobs\<n>\<id>.snjob` or `Products\<n>\<id>.json`,
/// or with `.deleted` after it, the mark a deleted record leaves (LB16).
/// Gives the record's own parts and whether these are the mark's.
fn record_of(parts: &[String]) -> Option<(Vec<String>, bool)> {
    let [top, sub, file] = parts else { return None };
    let ext = match top.as_str() {
        "Jobs" => crate::jobfile::EXT,
        "Products" => "json",
        _ => return None,
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let (stem, mark) = match file.strip_suffix(".deleted") {
        Some(f) => (f.strip_suffix(&format!(".{ext}"))?, true),
        None => (file.strip_suffix(&format!(".{ext}"))?, false),
    };
    (digits(sub) && digits(stem)).then(|| (vec![top.clone(), sub.clone(), format!("{stem}.{ext}")], mark))
}

/// A Library record's revision (LB7): jobs keep it with their record,
/// library parts at the top. 0 for anything else.
fn record_revision(bytes: &[u8]) -> u64 {
    #[derive(Deserialize)]
    struct Rec {
        #[serde(default)]
        revision: u64,
    }
    #[derive(Deserialize)]
    struct OnDisk {
        #[serde(default)]
        library: Option<Rec>,
        #[serde(default)]
        revision: u64,
    }
    serde_json::from_slice::<OnDisk>(bytes).map_or(0, |d| d.library.map_or(d.revision, |r| r.revision))
}

/// What a backup puts back (BK10, BK11): its folders and files by their
/// parts inside the Library (each file with its place in the zip), and the
/// highest revision of a record in it (BK12).
struct Held {
    dirs: Vec<Vec<String>>,
    files: Vec<(usize, Vec<String>)>,
    top_revision: u64,
}

/// Read a backup through before anything changes (BK11): every name inside
/// the Library, no links, no name twice, every file whole (its check sum),
/// and none written by a program newer than this one (`versions`, UP7): it
/// couldn't read them or save them again. What a restore never takes from a
/// backup (BK10) is left out.
fn read_backup(path: &Path, versions: &shared::Versions) -> Result<Held, String> {
    let f = std::fs::File::open(path).map_err(|e| format!("it can't be read ({e})"))?;
    let mut z = zip::ZipArchive::new(std::io::BufReader::new(f)).map_err(|e| format!("it isn't a whole zip ({e})"))?;
    let mut held = Held { dirs: vec![], files: vec![], top_revision: 0 };
    let mut seen = std::collections::HashSet::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).map_err(|e| format!("it can't be read ({e})"))?;
        let raw = e.name().to_string();
        if e.is_symlink() {
            return Err(format!("it holds a link (\"{raw}\"); a restore puts back only files and folders"));
        }
        let parts = entry_parts(&raw)?;
        if kept_live(&parts[0]) {
            continue;
        }
        if !seen.insert(key(&parts)) {
            return Err(format!("it holds \"{raw}\" twice"));
        }
        if raw.ends_with(['/', '\\']) {
            held.dirs.push(parts);
            continue;
        }
        let mut b = Vec::new();
        e.read_to_end(&mut b).map_err(|x| format!("\"{raw}\" in it is damaged ({x})"))?;
        if let Some(n) = versions.newer(shared::recorded_version(&b).as_deref()) {
            return Err(format!("\"{raw}\" in it was written by version {}, newer than this program ({}), which couldn't read it. Update this PC first", n.written, versions.this));
        }
        if record_of(&parts).is_some_and(|(_, mark)| !mark) {
            held.top_revision = held.top_revision.max(record_revision(&b));
        }
        held.files.push((i, parts));
    }
    Ok(held)
}

/// A backup just taken is whole and this Library's (BK9): marked with its
/// id, and a zip that opens with as many files as were put in it.
fn verify(path: &Path, id: &str, files: usize) -> Result<(), String> {
    if !marked(path, id) {
        return Err(format!("{} isn't marked as this Library's backup", path.display()));
    }
    let f = std::fs::File::open(path).map_err(|e| format!("{} can't be read ({e})", path.display()))?;
    let z = zip::ZipArchive::new(std::io::BufReader::new(f)).map_err(|e| format!("{} isn't a whole zip ({e})", path.display()))?;
    let n = z.file_names().filter(|n| !n.ends_with('/')).count();
    if n != files {
        return Err(format!("{} holds {n} files, not the {files} put in it", path.display()));
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    /// Test seam: a restore stops with an error after putting back this many
    /// files, as one whose share dropped halfway does.
    pub static RESTORE_STOP_AFTER: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// Unpack the backup's files into `stage`, a new folder of this restore's
/// own in the shop's `Backups` (BK11), renewing the lock as it goes. Every
/// file is read whole: one whose check sum is wrong stops it.
fn unpack(zip_path: &Path, held: &Held, stage: &Path, lock: &mut WriteLock) -> Result<(), String> {
    let f = std::fs::File::open(zip_path).map_err(|e| format!("it can't be read ({e})"))?;
    let mut z = zip::ZipArchive::new(std::io::BufReader::new(f)).map_err(|e| format!("it isn't a whole zip ({e})"))?;
    let werr = |p: &Path, e: &dyn std::fmt::Display| format!("{} can't be written ({e})", p.display());
    for d in &held.dirs {
        let p = join(stage, d);
        std::fs::create_dir_all(&p).map_err(|e| werr(&p, &e))?;
    }
    let mut buf = vec![0u8; 256 * 1024];
    for (i, parts) in &held.files {
        lock.renew_if_due()?;
        let to = join(stage, parts);
        if let Some(d) = to.parent() {
            std::fs::create_dir_all(d).map_err(|e| werr(d, &e))?;
        }
        let mut e = z.by_index(*i).map_err(|e| format!("it can't be read ({e})"))?;
        let mut out = std::fs::OpenOptions::new().write(true).create_new(true).open(&to).map_err(|e| werr(&to, &e))?;
        loop {
            lock.renew_if_due()?;
            match e.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => out.write_all(&buf[..n]).map_err(|x| werr(&to, &x))?,
                Err(x) if x.kind() == std::io::ErrorKind::Interrupted => {}
                Err(x) => return Err(format!("\"{}\" in it is damaged ({x})", parts.join("/"))),
            }
        }
        out.sync_all().map_err(|x| werr(&to, &x))?;
    }
    Ok(())
}

/// The Library's folders and files now, by their parts, but those a
/// restore keeps as they are (BK10), links (never in a backup, left as they
/// are) and names that aren't text. A folder that can't be read stops it.
fn live(library: &Path) -> Result<(Vec<Vec<String>>, Vec<Vec<String>>), String> {
    let (mut dirs, mut files) = (vec![], vec![]);
    let mut todo = vec![Vec::<String>::new()];
    while let Some(rel) = todo.pop() {
        let at = join(library, &rel);
        for e in std::fs::read_dir(&at).map_err(|e| format!("{} can't be read ({e})", at.display()))? {
            let e = e.map_err(|e| format!("{} can't be read ({e})", at.display()))?;
            let Some(name) = e.file_name().to_str().map(str::to_string) else { continue };
            if rel.is_empty() && kept_live(&name) {
                continue;
            }
            let t = e.file_type().map_err(|x| format!("{} can't be read ({x})", e.path().display()))?;
            let mut parts = rel.clone();
            parts.push(name);
            if t.is_dir() {
                dirs.push(parts.clone());
                todo.push(parts);
            } else if t.is_file() {
                files.push(parts);
            }
        }
    }
    Ok((dirs, files))
}

/// What a restore did (BK8–BK12).
#[derive(Clone, Debug)]
pub struct Restored {
    /// The backup put back.
    pub from: Entry,
    /// The backup of the Library as it was, taken first (BK9).
    pub before: Entry,
    /// Files put back from the backup, and files removed (made since it).
    pub files: usize,
    pub removed: usize,
}

/// Put the backup named `name` back over the shop folder `library` (BK8–
/// BK12), as `who` on a program of `version`, under `lock`, the Library's
/// write lock the caller holds (renewed while it works). Who may restore is
/// the caller's to check (`engine::shop::restore`).
///
/// 1. The backup is read through (BK11): nothing is changed for one with a
///    name that would go outside the Library, a link, a damaged file, or a
///    file a newer version wrote.
/// 2. The Library as it is now is backed up ("… before restore", BK9) and
///    that backup checked; if it can't be, nothing is restored.
/// 3. The backup is unpacked into a folder of its own in `Backups`, the
///    restore is recorded in the shop file, unfinished, and the files are
///    moved into place one by one; then what isn't in the backup is removed
///    (BK10). The shop file then says it finished.
///
/// An error says what state the Library is in: as it was, or partly
/// restored with the "before restore" backup holding what was there.
pub fn restore(library: &Path, name: &str, who: &Who, clock: &Clock, version: &str, lock: &mut WriteLock) -> Result<Restored, String> {
    let nothing = |e: String| format!("Nothing was restored: {e}. The Library is as it was.");
    let p = plan(library, &Settings::default()).map_err(nothing)?;
    if !p.shop {
        return Err(nothing(format!("{} isn't a shop", library.display())));
    }
    let dir = p.dir.clone().map_err(nothing)?;
    let ns = p.ns().ok_or_else(|| nothing("this Library has no backups yet".into()))?;
    let from = list(&dir, &ns).map_err(nothing)?.into_iter().find(|e| e.name == name).ok_or_else(|| nothing(format!("there's no backup of this Library named {name} in {}", dir.display())))?;
    let versions = crate::shop::versions(library, version);
    let held = read_backup(&from.path, &versions).map_err(|e| nothing(format!("the backup of {} can't be restored: {e}", from.shown())))?;
    // The shop file stays and records the restore: never over one a newer
    // version wrote (UP7).
    shared::may_write_over(&crate::shop::file(library), &versions, "The shop file").map_err(nothing)?;
    lock.still_held().map_err(nothing)?;
    // BK9: the Library as it is now, first, checked whole.
    let before = match take_as(library, &Settings::default(), who, clock, Why::Now, true) {
        Ok(Daily::Taken(d)) => d,
        Ok(Daily::AlreadyToday { .. }) => return Err(nothing("the backup of the Library as it is now wasn't taken".into())),
        Err(e) => return Err(nothing(format!("the Library as it is now couldn't be backed up first ({e})"))),
    };
    shared::pause("restore: before restore taken");
    verify(&before.entry.path, &ns.id, before.files).map_err(|e| nothing(format!("the backup of the Library as it is now ({}) can't be trusted: {e}", before.entry.name)))?;
    let kept_before = |e: String| nothing(format!("{e}. The backup {} of the Library as it was is kept", before.entry.name));
    // Unpacked into a folder of its own first (BK11). One a restore left
    // when its PC stopped goes first.
    lock.renew_if_due().map_err(kept_before)?;
    let stem = format!("Restoring {} ", ns.short());
    for e in std::fs::read_dir(&dir).map_err(|e| kept_before(format!("{} can't be read ({e})", dir.display())))?.flatten() {
        if e.file_name().to_str().is_some_and(|n| n.starts_with(&stem)) && e.file_type().is_ok_and(|t| t.is_dir()) {
            lock.still_held().map_err(kept_before)?;
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
    let stage = dir.join(format!("{stem}{}", shared::writer_tag()));
    std::fs::create_dir(&stage).map_err(|e| kept_before(format!("{} can't be made ({e})", stage.display())))?;
    let clear = |lock: &WriteLock| {
        if lock.still_held().is_ok() {
            let _ = std::fs::remove_dir_all(&stage);
        }
    };
    if let Err(e) = unpack(&from.path, &held, &stage, lock) {
        clear(lock);
        return Err(kept_before(format!("the backup of {} couldn't be unpacked ({e})", from.shown())));
    }
    shared::pause("restore: unpacked");
    // BK12: every revision a record has now, or had in the backup, stays
    // below the floor, so no save after the restore repeats one (LB7).
    let (live_dirs, live_files) = match live(library) {
        Ok(l) => l,
        Err(e) => {
            clear(lock);
            return Err(kept_before(e));
        }
    };
    let mut shop = match crate::shop::read(library) {
        Ok(Some(s)) => s,
        Ok(None) => {
            clear(lock);
            return Err(kept_before(format!("{} isn't a shop any more", library.display())));
        }
        Err(e) => {
            clear(lock);
            return Err(kept_before(e));
        }
    };
    let mut floor = shop.restored.as_ref().map_or(0, |r| r.revision_floor).max(held.top_revision);
    for f in &live_files {
        if record_of(f).is_some_and(|(_, mark)| !mark) {
            if let Ok(b) = std::fs::read(join(library, f)) {
                floor = floor.max(record_revision(&b));
            }
        }
    }
    shop.restored = Some(crate::shop::Restore { backup: from.name.clone(), backup_when: from.stamp(), before: before.entry.name.clone(), by: who.clone(), at: clock.now(), finished: false, revision_floor: floor });
    if let Err(e) = lock.renew_if_due().and_then(|()| crate::shop::write(library, &shop, lock, version)) {
        clear(lock);
        return Err(kept_before(format!("the shop file couldn't record the restore ({e})")));
    }
    // From here a failure leaves the Library partly restored.
    let halfway = |e: String| {
        format!(
            "The restore stopped halfway: {e}. Some of the Library's files are from the backup of {}, the rest are as they were. The backup {} holds the whole Library as it was before: restore it to put the Library back as it was.",
            from.shown(),
            before.entry.name
        )
    };
    let swapped = swap(library, &stage, &held, &live_dirs, &live_files, lock);
    clear(lock);
    let (files, removed) = swapped.map_err(halfway)?;
    if let Some(r) = shop.restored.as_mut() {
        r.finished = true;
    }
    lock.renew_if_due().and_then(|()| crate::shop::write(library, &shop, lock, version)).map_err(|e| halfway(format!("every file was put back, but the shop file couldn't record that the restore finished ({e})")))?;
    Ok(Restored { from, before: before.entry, files, removed })
}

/// Make the folder at `parts` in the Library, where it isn't one: a file
/// standing in its place goes (it isn't in the backup; the "before
/// restore" backup holds it). A link there stops the restore.
fn make_dir(library: &Path, parts: &[String], lock: &WriteLock) -> Result<(), String> {
    let p = join(library, parts);
    match std::fs::symlink_metadata(&p) {
        Ok(m) if m.is_dir() => return Ok(()),
        Ok(m) if m.file_type().is_symlink() => return Err(format!("{} is a link", p.display())),
        Ok(_) => {
            lock.still_held()?;
            std::fs::remove_file(&p).map_err(|e| format!("{} can't be removed ({e})", p.display()))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("{} can't be read ({e})", p.display())),
    }
    lock.still_held()?;
    std::fs::create_dir(&p).map_err(|e| format!("{} can't be made ({e})", p.display()))
}

/// Move the unpacked backup into the Library, file by file under the lock
/// (renewed, and checked before each), then remove the files and empty
/// folders that aren't in the backup (BK10, BK11). A record removed leaves
/// its mark, so its number is never given to another record (LB16). Gives
/// the files put back and removed.
fn swap(library: &Path, stage: &Path, held: &Held, live_dirs: &[Vec<String>], live_files: &[Vec<String>], lock: &mut WriteLock) -> Result<(usize, usize), String> {
    // Everything in the backup, and every folder its files are in.
    let mut keep = std::collections::HashSet::new();
    let mut dirs: Vec<Vec<String>> = vec![];
    let mut add_dir = |d: &[String], keep: &mut std::collections::HashSet<String>| {
        for n in 1..=d.len() {
            if keep.insert(key(&d[..n])) {
                dirs.push(d[..n].to_vec());
            }
        }
    };
    for d in &held.dirs {
        add_dir(d, &mut keep);
    }
    for (_, f) in &held.files {
        add_dir(&f[..f.len() - 1], &mut keep);
        keep.insert(key(f));
    }
    dirs.sort_by_key(Vec::len);
    for d in &dirs {
        lock.renew_if_due()?;
        make_dir(library, d, lock)?;
    }
    let mut put = 0;
    for (_, f) in &held.files {
        lock.renew_if_due()?;
        let to = join(library, f);
        if std::fs::symlink_metadata(&to).is_ok_and(|m| m.is_dir()) {
            // A folder where the backup has a file: it isn't in the backup.
            lock.still_held()?;
            std::fs::remove_dir_all(&to).map_err(|e| format!("{} can't be removed ({e})", to.display()))?;
        }
        shared::move_over(&join(stage, f), &to, lock).map_err(|e| format!("{} can't be put back ({e})", to.display()))?;
        put += 1;
        shared::pause("restore: file put back");
        #[cfg(test)]
        if RESTORE_STOP_AFTER.with(|s| s.get()).is_some_and(|n| put >= n) {
            return Err("the test stopped it halfway".into());
        }
    }
    let mut removed = 0;
    let mut gone_records = vec![];
    for f in live_files.iter().filter(|f| !keep.contains(&key(f))) {
        lock.renew_if_due()?;
        lock.still_held()?;
        let p = join(library, f);
        match std::fs::remove_file(&p) {
            Ok(()) => removed += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{} can't be removed ({e})", p.display())),
        }
        if let Some((rec, _)) = record_of(f) {
            gone_records.push(rec);
        }
    }
    // A record (or a deleted record's mark) that was here and isn't in the
    // backup keeps its number taken (LB16).
    for rec in gone_records {
        if keep.contains(&key(&rec)) {
            continue;
        }
        let mut mark = rec.clone();
        if let Some(last) = mark.last_mut() {
            last.push_str(".deleted");
        }
        let p = join(library, &mark);
        lock.still_held()?;
        if p.parent().is_some_and(Path::is_dir) {
            let _ = std::fs::OpenOptions::new().write(true).create_new(true).open(&p);
        }
    }
    let mut old: Vec<&Vec<String>> = live_dirs.iter().filter(|d| !keep.contains(&key(d))).collect();
    old.sort_by_key(|d| std::cmp::Reverse(d.len()));
    for d in old {
        lock.still_held()?;
        // Only when empty: a folder holding a record's mark stays.
        let _ = std::fs::remove_dir(join(library, d));
    }
    Ok((put, removed))
}

/// BK9: copy the "before restore" backup to the second place, once the
/// restore has let go of the lock. None: no second place is set.
pub fn to_second_place(library: &Path, entry: &Entry) -> Option<Result<PathBuf, String>> {
    let p = plan(library, &Settings::default()).ok()?;
    let place = p.second_place()?;
    let ns = p.ns()?;
    let dir = p.dir.clone().ok()?;
    Some(copy_to_second(entry, &place, library, &dir, &ns).map(|(to, _)| {
        prune_second(library, &Settings::default(), &place, &ns, entry.day);
        to
    }))
}

/// This Library's backups in `dir`, newest first (tests): none before its
/// first backup.
#[cfg(test)]
pub fn backups_in(library: &Path, dir: &Path) -> Result<Vec<Entry>, String> {
    match library_id(library)? {
        Some(id) => list(dir, &Ns { id, label: String::new() }),
        None => Ok(vec![]),
    }
}

#[cfg(test)]
mod backup_tests {
    use super::*;

    fn ns() -> Ns {
        Ns { id: "1a2b3c4d5e6f70811a2b3c4d5e6f7081".into(), label: "Acme".into() }
    }

    #[test]
    fn only_names_of_its_own_count_as_backups() {
        let n = parse("Backup Acme 1a2b3c4d 2026-11-09 0920 PC-A.zip").unwrap();
        assert_eq!((n.short.as_str(), n.day, n.time, n.pc.as_str(), n.seq), ("1a2b3c4d", (2026, 11, 9), (9, 20), "PC-A", 1));
        assert_eq!(parse("Backup Acme 1a2b3c4d 2026-11-09 0920 PC-A 2.zip").unwrap().seq, 2);
        for other in [
            "Backup Acme 1a2b3c4d 2026-11-09 0920 PC-A.zip.PC-A-1-2.part",
            "backup Acme 1a2b3c4d 2026-11-09 0920 PC-A.zip",
            "Backup Acme 1a2b3c4d 2026-11-09 PC-A.zip",
            "Backup Acme 1a2b3c4d 2026-13-09 0920 PC-A.zip",
            "Backup Acme 1A2B3C4D 2026-11-09 0920 PC-A.zip",
            "Backup Acme 1a2b3c4d 2026-11-09 0920 PC A copy.zip",
            "Backup Acme 1a2b3c4d 2026-11-09 2460 PC-A.zip",
            "Backup Acme 1a2b3c4d 2026-11-09 0920 PC-A 1.zip",
            "Notes.zip",
        ] {
            assert_eq!(parse(other), None, "{other}");
        }
        assert!(is_own_part("Backup Acme 1a2b3c4d 2026-11-09 0920 PC-A.zip.PC-A-1-2.part", &ns()));
        assert!(!is_own_part("Backup Acme 9a2b3c4d 2026-11-09 0920 PC-A.zip.PC-A-1-2.part", &ns()), "another Library's");
        assert!(!is_own_part("Backup Acme 1a2b3c4d 2026-11-09 0920 PC-A.zip", &ns()));
        assert!(!is_own_part("Report.zip.x.part", &ns()));
        assert_eq!(name_for(&ns(), (2026, 11, 9), (9, 5), "PC-Ä 1", 1, false), "Backup Acme 1a2b3c4d 2026-11-09 0905 PC-__1.zip");
        assert_eq!(parse(&name_for(&ns(), (2026, 11, 9), (9, 5), "PC-Ä 1", 3, false)).map(|p| (p.pc, p.seq)), Some(("PC-__1".into(), 3)));
        // BK9: the one a restore takes first says so, and is a backup too.
        let b = name_for(&ns(), (2026, 11, 9), (9, 5), "PC-A", 2, true);
        assert_eq!(b, "Backup Acme 1a2b3c4d 2026-11-09 0905 PC-A 2 before restore.zip");
        assert_eq!(parse(&b).map(|p| (p.seq, p.before_restore)), Some((2, true)));
        assert_eq!(parse("Backup Acme 1a2b3c4d 2026-11-09 0905 PC-A.zip").map(|p| p.before_restore), Some(false));
        assert_eq!(parse("Backup Acme 1a2b3c4d 2026-11-09 0905 PC-A after restore.zip"), None);
    }

    /// Review S10: names with other signs where the numbers go are no
    /// backup's, and never stop the listing.
    #[test]
    fn names_with_other_signs_where_the_numbers_go_are_no_backups() {
        for odd in [
            "Backup Acme 1a2b3c4d 2026-11-09 €1 PC-A.zip",
            "Backup Acme 1a2b3c4d 2026-11-0€ 0920 PC-A.zip",
            "Backup Acme 1a2b3c4d 2026-1€-09 0920 PC-A.zip",
            "Backup Acme 1a2b3c4d 20€6-11-09 0920 PC-A.zip",
            "Backup Acme 1a2b3c4€ 2026-11-09 0920 PC-A.zip",
            "Backup Acme 1a2b3c4d 2026-11-09 09²0 PC-A.zip",
            "Backup Acme 1a2b3c4d 2026-11-09 0920 PC-A ².zip",
            "Backup Acme 1a2b3c4d ２０２６-11-09 0920 PC-A.zip",
        ] {
            assert_eq!(parse(odd), None, "{odd}");
        }
        let dir = std::env::temp_dir().join(format!("nestpro_backup_odd_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Backup Acme 1a2b3c4d 2026-11-09 €1 PC-A.zip"), b"x").unwrap();
        assert_eq!(list(&dir, &ns()).unwrap(), vec![]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BK6, review S5: beside the Library, in the folder it's really in,
    /// with the system's own paths; never the Library itself.
    #[test]
    fn a_pc_of_its_own_keeps_backups_beside_its_library() {
        let f = |s: &str| folder(Path::new(s), false);
        #[cfg(unix)]
        {
            assert_eq!(f("/srv/Sheet Nesting/Library").unwrap(), Path::new("/srv/Sheet Nesting/Backups"));
            assert_eq!(f("/srv/Sheet Nesting/Library/").unwrap(), Path::new("/srv/Sheet Nesting/Backups"));
        }
        #[cfg(windows)]
        {
            assert_eq!(f(r"D:\srv\Sheet Nesting\Library").unwrap(), Path::new(r"D:\srv\Sheet Nesting\Backups"));
            assert_eq!(f(r"D:\srv\Sheet Nesting\Library\").unwrap(), Path::new(r"D:\srv\Sheet Nesting\Backups"));
        }
        // At the top of a drive: beside it, in the drive's root.
        #[cfg(unix)]
        assert_eq!(f("/Library").unwrap(), Path::new("/Backups"));
        #[cfg(windows)]
        assert_eq!(f(r"C:\Library").unwrap(), Path::new(r"C:\Backups"));
        // A relative Library: beside it, where it really is.
        let rel = f("Library").unwrap();
        assert!(rel.is_absolute() && rel.ends_with("Backups") && rel.parent() == std::env::current_dir().ok().as_deref(), "{}", rel.display());
        // The drive's root itself has nothing beside it.
        #[cfg(unix)]
        assert!(f("/").is_err());
        // A Library named Backups would back up into itself.
        let e = f("/srv/Sheet Nesting/Backups").unwrap_err();
        assert!(e.contains("the Library itself"), "{e}");
        // In a shop: the shop folder's own Backups.
        assert_eq!(folder(Path::new("/srv/Nesting"), true).unwrap(), std::path::absolute("/srv/Nesting").unwrap().join("Backups"));
    }

    /// BK11: a zip entry's name, `/` or `\` between folders, never outside
    /// the Library; BK10: what a restore keeps as it is.
    #[test]
    fn a_restore_puts_files_only_inside_the_library() {
        let p = |s: &str| entry_parts(s).map(|v| v.join("|"));
        assert_eq!(p("Jobs/0/7.snjob").unwrap(), "Jobs|0|7.snjob");
        assert_eq!(p("Jobs\\0\\7.snjob").unwrap(), "Jobs|0|7.snjob");
        assert_eq!(p("Drawings/Größe 2 mm/Flansch ø40.dxf").unwrap(), "Drawings|Größe 2 mm|Flansch ø40.dxf");
        assert_eq!(p("Empty folder/").unwrap(), "Empty folder");
        assert_eq!(p("./Jobs//0/7.snjob").unwrap(), "Jobs|0|7.snjob");
        for bad in ["../x", "a/../../x", "/etc/x", "\\x", "C:\\x", "C:x", "a/b:c", "\\\\srv\\Nesting\\x", "..", "a/\u{7}b"] {
            assert!(p(bad).is_err(), "{bad}");
        }
        assert!(p("/").is_err() && p("").is_err());
        assert!(is_device("CON") && is_device("nul.txt") && is_device("COM1.dxf") && is_device("lpt9") && !is_device("COM0") && !is_device("Console.txt") && !is_device("CONTOUR.dxf"));
        for k in ["Updates", "backups", "Shop", "Holds", "shop.json", "backup-id.txt", "library.lock", "library.lock.abc.taken"] {
            assert!(kept_live(k), "{k}");
        }
        for k in ["Jobs", "Products", "Sheets", "index.json", "library.locked", "Shopping", "materials.json", "ä"] {
            assert!(!kept_live(k), "{k}");
        }
        let r = |s: &str| record_of(&s.split('/').map(String::from).collect::<Vec<_>>()).map(|(p, m)| (p.join("/"), m));
        assert_eq!(r("Jobs/0/7.snjob"), Some(("Jobs/0/7.snjob".into(), false)));
        assert_eq!(r("Jobs/0/7.snjob.deleted"), Some(("Jobs/0/7.snjob".into(), true)));
        assert_eq!(r("Products/1/1004.json"), Some(("Products/1/1004.json".into(), false)));
        assert_eq!(r("Jobs/0/notes.txt"), None);
        assert_eq!(r("Jobs/7.snjob"), None);
        assert_eq!(record_revision(br#"{"library": {"id": 7, "revision": 5}}"#), 5);
        assert_eq!(record_revision(br#"{"revision": 3, "part": {}}"#), 3);
        assert_eq!(record_revision(b"0\nEOF"), 0);
        assert_eq!(shown_stamp("2026-11-09 09:20"), "9 Nov 09:20");
        assert_eq!(shown_stamp("soon"), "soon");
    }

    #[test]
    fn day_numbers_count_days() {
        assert_eq!(day_number((1970, 1, 1)), 0);
        assert_eq!(day_number((2026, 11, 9)) - day_number((2026, 10, 10)), 30);
        assert_eq!(day_number((2025, 3, 1)) - day_number((2025, 2, 28)), 1);
    }
}
