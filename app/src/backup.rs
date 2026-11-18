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
    /// Each file and folder (`name/`, size 0) put in it, by its name in the
    /// zip, with its size: what a restore checks its "before restore"
    /// backup against (BK9).
    pub listing: Vec<(String, u64)>,
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
    let listing = match zipped {
        Ok(f) => f,
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            return Err(format!("No backup was taken: {e}. Nothing in the Library was changed."));
        }
    };
    // The one a restore takes first removes none past Keep (BK9): the
    // backup it's about to restore may be one of them.
    let removed = if before_restore { vec![] } else { prune(&dir, &ns, p.settings.keep(), today, Some(&mut lock)) };
    drop(lock);
    let files = listing.iter().filter(|(n, _)| !n.ends_with('/')).count();
    let bytes = std::fs::metadata(&path).map_or(0, |m| m.len());
    let entry = Entry { name, path, day: today, time: now, pc: shared::file_safe(&who.pc), seq: n, bytes, before_restore };
    // The one a restore takes first goes to the second place once the
    // restore lets go of the lock (`to_second_place`, BK9): a slow copy never
    // holds the restore up.
    if before_restore {
        return Ok(Daily::Taken(Done { entry, files, listing, removed, second: None }));
    }
    let second = p.second_place().map(|place| {
        copy_to_second(&entry, &place, library, &dir, &ns).map(|(to, _)| {
            prune_second(library, own, &place, &ns, today);
            to
        })
    });
    Ok(Daily::Taken(Done { entry, files, listing, removed, second }))
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
fn zip_library(library: &Path, to: &Path, skip_top: &[&str], mark: &str, lock: &mut WriteLock, clock: &Clock) -> Result<Vec<(String, u64)>, String> {
    use zip::write::SimpleFileOptions;
    let file = std::fs::OpenOptions::new().write(true).create_new(true).open(to).map_err(|e| format!("{} can't be written ({e})", to.display()))?;
    let mut z = zip::ZipWriter::new(std::io::BufWriter::new(file));
    let mut files: Vec<(String, u64)> = vec![];
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
                files.push((format!("{inner}/"), 0));
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
            files.push((inner.clone(), stamp_of(&path).map_or(0, |s| s.0)));
            #[cfg(test)]
            if STOP_AFTER.with(|s| s.get()).is_some_and(|n| files.len() >= n) {
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

/// A name Windows keeps for a device (`CON`, `NUL`, `CONIN$`, `COM1`,
/// `LPT²`…), with or without an extension: never a file there.
fn is_device(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or("").trim_end_matches(' ').to_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$") {
        return true;
    }
    let mut c = stem.chars();
    let head: String = c.by_ref().take(3).collect();
    let rest: Vec<char> = c.collect();
    (head == "COM" || head == "LPT") && rest.len() == 1 && matches!(rest[0], '0'..='9' | '¹' | '²' | '³')
}

/// A name part Windows could read as another name: a trailing dot or space
/// (dropped), or a short name like `SHOP~1` (another name for a folder).
fn ambiguous(part: &str) -> bool {
    let short = part.char_indices().any(|(i, ch)| ch == '~' && part[i + 1..].starts_with(|d: char| d.is_ascii_digit()));
    part.ends_with('.') || part.ends_with(' ') || short
}

/// A zip entry's name as the folders and file it goes to inside the Library
/// (BK11), split at `/` or `\` (a zip made on Windows by another tool). A
/// name that could reach outside the Library (`..`, starting with `/` or
/// `\`, a drive or a stream with `:`) is refused, and so is one Windows
/// could take for another name or can't keep as a file (BK11), naming it.
fn entry_parts(raw: &str) -> Result<Vec<String>, String> {
    let outside = || format!("it holds a file named \"{raw}\", which would go outside the shop folder");
    let not_kept = || format!("it holds a file named \"{raw}\", which Windows can't keep under that name");
    if raw.starts_with(['/', '\\']) {
        return Err(outside());
    }
    let mut out = vec![];
    for part in raw.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return Err(outside()),
            p if p.contains(':') || p.chars().any(char::is_control) => return Err(outside()),
            p if ambiguous(p) || is_device(p) || p.contains(['<', '>', '"', '|', '?', '*']) => return Err(not_kept()),
            p => out.push(p.to_string()),
        }
    }
    if out.is_empty() {
        return Err(format!("it holds an entry with no name (\"{raw}\")"));
    }
    Ok(out)
}

/// A path's parts as one key for the same file on this system (names
/// differing only in capitals are one file on Windows).
fn key(parts: &[String]) -> String {
    let j = parts.join("/");
    if cfg!(windows) { j.to_lowercase() } else { j }
}

/// A path's parts as one key whatever the capitals: two entries of a backup
/// that differ only so would be one file on Windows (BK11).
fn folded(parts: &[String]) -> String {
    parts.join("/").to_lowercase()
}

fn join(base: &Path, parts: &[String]) -> PathBuf {
    parts.iter().fold(base.to_path_buf(), |p, x| p.join(x))
}

/// A Library record (LB7): `Jobs\<n>\<id>.snjob` or `Products\<n>\<id>.json`
/// (any capitals), or with `.deleted` after it, the mark a deleted record
/// leaves (LB16). Gives the record's own parts as they are spelled here and
/// whether these are the mark's.
fn record_of(parts: &[String]) -> Option<(Vec<String>, bool)> {
    let [top, sub, file] = parts else { return None };
    let ext = if top.eq_ignore_ascii_case("Jobs") {
        crate::jobfile::EXT
    } else if top.eq_ignore_ascii_case("Products") {
        "json"
    } else {
        return None;
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let lower = file.to_ascii_lowercase();
    let (record, mark) = match lower.strip_suffix(".deleted") {
        Some(_) => (&file[..file.len() - ".deleted".len()], true),
        None => (file.as_str(), false),
    };
    let stem = record.get(..record.len().checked_sub(ext.len() + 1)?)?;
    let dot_ext = &record[stem.len()..];
    (dot_ext.eq_ignore_ascii_case(&format!(".{ext}")) && digits(sub) && digits(stem)).then(|| (vec![top.clone(), sub.clone(), record.to_string()], mark))
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

/// A file whose text records the program version that wrote it (UP5): the
/// Library's own `.json` and `.snjob` files.
fn stamped(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".json") || n.ends_with(&format!(".{}", crate::jobfile::EXT))
}

/// How many entries a zip's own end record says it holds (its central
/// directory may name one file twice; the reader keeps only one of them).
/// None when it can't be read.
fn raw_entries(path: &Path) -> Option<u64> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let n = len.min(66 * 1024);
    f.seek(std::io::SeekFrom::End(-(n as i64))).ok()?;
    let mut b = vec![0; n as usize];
    f.read_exact(&mut b).ok()?;
    let p = (0..=b.len().checked_sub(22)?).rev().find(|&p| b[p..p + 4] == [0x50, 0x4b, 0x05, 0x06])?;
    let total = u64::from(u16::from_le_bytes([b[p + 10], b[p + 11]]));
    if total != 0xFFFF {
        return Some(total);
    }
    // Zip64: its locator just before, giving where its own end record is.
    let l = p.checked_sub(20)?;
    if b[l..l + 4] != [0x50, 0x4b, 0x06, 0x07] {
        return None;
    }
    let at = u64::from_le_bytes(b[l + 8..l + 16].try_into().ok()?);
    let mut e = [0u8; 56];
    f.seek(std::io::SeekFrom::Start(at)).ok()?;
    f.read_exact(&mut e).ok()?;
    (e[..4] == [0x50, 0x4b, 0x06, 0x06]).then(|| u64::from_le_bytes(e[32..40].try_into().unwrap_or_default()))
}

/// A backup opened for a restore, read through from one handle.
type Archive = zip::ZipArchive<std::io::BufReader<std::fs::File>>;

/// Open a zip and check its reader sees every entry its end record counts
/// (a name twice is refused: one of them would be lost, BK11).
fn open_zip(path: &Path) -> Result<Archive, String> {
    let f = std::fs::File::open(path).map_err(|e| format!("it can't be read ({e})"))?;
    let z = zip::ZipArchive::new(std::io::BufReader::new(f)).map_err(|e| format!("it isn't a whole zip ({e})"))?;
    match raw_entries(path) {
        Some(n) if n == z.len() as u64 => Ok(z),
        Some(n) => Err(format!("it names {} file(s) more than once ({n} entries, {} names)", n - (z.len() as u64).min(n), z.len())),
        None => Err("its end record can't be read".into()),
    }
}

/// Read a zip entry to its end, in pieces, renewing the lock after each
/// (BK11): its check sum is checked at the end. The bytes are kept when
/// `keep`. Gives its size and, when kept, its bytes.
fn read_entry(e: &mut impl Read, lock: &mut WriteLock, keep: bool, what: &str) -> Result<(u64, Vec<u8>), String> {
    let mut buf = vec![0u8; 256 * 1024];
    let (mut n, mut kept) = (0u64, vec![]);
    loop {
        lock.renew_if_due()?;
        let got = match e.read(&mut buf) {
            Ok(0) => return Ok((n, kept)),
            Ok(k) => k,
            Err(x) if x.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(x) => return Err(format!("\"{what}\" in it is damaged ({x})")),
        };
        n += got as u64;
        if keep {
            kept.extend_from_slice(&buf[..got]);
        }
        shared::pause("restore: entry read");
    }
}

/// What a backup puts back (BK10, BK11): its folders and files by their
/// parts inside the Library (each file with its place in the zip), and the
/// highest revision of a record in it (BK12).
struct Held {
    dirs: Vec<Vec<String>>,
    files: Vec<(usize, Vec<String>)>,
    top_revision: u64,
    /// Every entry by its name in the zip (folders `name/`, size 0), with
    /// its size as read, kept ones included (BK9).
    listing: Vec<(String, u64)>,
}

/// Read a backup through, under the lock (renewed as it reads), before
/// anything changes (BK11): every name inside the Library and one Windows
/// keeps as it is, no links, no name twice (whatever the capitals), every
/// file whole (its check sum), and no file written by a program newer than
/// this one (`versions`, UP7), the shop file included, whether or not it
/// is put back: this program couldn't read them or save them again. What a
/// restore never takes from a backup (BK10) is then left out.
fn read_backup(z: &mut Archive, versions: &shared::Versions, lock: &mut WriteLock) -> Result<Held, String> {
    let mut held = Held { dirs: vec![], files: vec![], top_revision: 0, listing: vec![] };
    let mut seen = std::collections::HashSet::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).map_err(|e| format!("it can't be read ({e})"))?;
        let raw = e.name().to_string();
        if e.is_symlink() {
            return Err(format!("it holds a link (\"{raw}\"); a restore puts back only files and folders"));
        }
        let parts = entry_parts(&raw)?;
        if !seen.insert(folded(&parts)) {
            return Err(format!("it holds \"{raw}\" twice (names differing only in capitals are one file on Windows)"));
        }
        let dir = raw.ends_with(['/', '\\']);
        if dir {
            held.listing.push((raw.clone(), 0));
        } else {
            let (n, b) = read_entry(&mut e, lock, stamped(&raw), &raw)?;
            held.listing.push((raw.clone(), n));
            if let Some(n) = versions.newer(shared::recorded_version(&b).as_deref()) {
                return Err(format!("\"{raw}\" in it was written by version {}, newer than this program ({}), which couldn't read it. Update this PC first", n.written, versions.this));
            }
            if record_of(&parts).is_some_and(|(_, mark)| !mark) {
                held.top_revision = held.top_revision.max(record_revision(&b));
            }
        }
        if kept_live(&parts[0]) {
            continue;
        }
        if dir {
            held.dirs.push(parts);
        } else {
            held.files.push((i, parts));
        }
    }
    Ok(held)
}

/// A backup just taken is whole, this Library's and one a restore can put
/// back (BK9): marked with its id, read through by the restore's own reader
/// (every name a restore takes, every check sum, no newer file), and
/// holding exactly the files and folders that were put in it, with their
/// sizes (`listing`). Read under the lock, renewed as it reads.
fn verify(path: &Path, id: &str, listing: &[(String, u64)], versions: &shared::Versions, lock: &mut WriteLock) -> Result<(), String> {
    if !marked(path, id) {
        return Err(format!("{} isn't marked as this Library's backup", path.display()));
    }
    let mut z = open_zip(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let held = read_backup(&mut z, versions, lock).map_err(|e| format!("{}: {e}", path.display()))?;
    let found: std::collections::BTreeMap<String, u64> = held.listing.into_iter().collect();
    let want: std::collections::BTreeMap<String, u64> = listing.iter().cloned().collect();
    if found != want {
        let odd = want.iter().find(|(k, v)| found.get(*k) != Some(v)).map(|(k, _)| k.clone()).or_else(|| found.keys().find(|k| !want.contains_key(*k)).cloned()).unwrap_or_default();
        return Err(format!("{} doesn't hold the Library's files as they were zipped (\"{odd}\")", path.display()));
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    /// Test seam: a restore stops with an error after putting back this many
    /// files, as one whose share dropped halfway does.
    pub static RESTORE_STOP_AFTER: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// Unpack the backup's files from `z` (the handle they were checked
/// through) into `stage`, a new folder of this restore's own in the shop's
/// `Backups` (BK11). Every folder made and every piece written is fenced by
/// the lock, renewed as it goes, and checked again after each read; every
/// file is read whole (its check sum).
fn unpack(z: &mut Archive, held: &Held, stage: &Path, lock: &mut WriteLock) -> Result<(), String> {
    let werr = |p: &Path, e: &dyn std::fmt::Display| format!("{} can't be written ({e})", p.display());
    let mut made = std::collections::HashSet::new();
    let mut make = |d: &Path, lock: &WriteLock| -> Result<(), String> {
        let rel: Vec<PathBuf> = d.ancestors().take_while(|a| *a != stage).map(Path::to_path_buf).collect();
        for a in rel.into_iter().rev() {
            if made.insert(a.clone()) && !a.is_dir() {
                lock.still_held()?;
                std::fs::create_dir(&a).map_err(|e| werr(&a, &e))?;
            }
        }
        Ok(())
    };
    for d in &held.dirs {
        make(&join(stage, d), lock)?;
    }
    let mut buf = vec![0u8; 256 * 1024];
    for (i, parts) in &held.files {
        lock.renew_if_due()?;
        let to = join(stage, parts);
        if let Some(d) = to.parent() {
            make(d, lock)?;
        }
        let mut e = z.by_index(*i).map_err(|e| format!("it can't be read ({e})"))?;
        lock.still_held()?;
        let mut out = std::fs::OpenOptions::new().write(true).create_new(true).open(&to).map_err(|e| werr(&to, &e))?;
        loop {
            let n = match e.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(x) if x.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(x) => return Err(format!("\"{}\" in it is damaged ({x})", parts.join("/"))),
            };
            // A read can be slow on a share: renewed (and so checked) after
            // it, before writing what it gave.
            lock.renew_if_due()?;
            lock.still_held()?;
            out.write_all(&buf[..n]).map_err(|x| werr(&to, &x))?;
        }
        out.sync_all().map_err(|x| werr(&to, &x))?;
    }
    Ok(())
}

/// The Library's folders and files now, by their parts, but those a restore
/// keeps as they are (BK10); and what a backup never holds (links, names
/// that aren't text, anything else), by its path: a restore leaves those as
/// they are and never removes or replaces them. A folder that can't be read
/// stops it.
struct Live {
    dirs: Vec<Vec<String>>,
    files: Vec<Vec<String>>,
    others: Vec<PathBuf>,
}

fn live(library: &Path) -> Result<Live, String> {
    let mut l = Live { dirs: vec![], files: vec![], others: vec![] };
    let mut todo = vec![Vec::<String>::new()];
    while let Some(rel) = todo.pop() {
        let at = join(library, &rel);
        for e in std::fs::read_dir(&at).map_err(|e| format!("{} can't be read ({e})", at.display()))? {
            let e = e.map_err(|e| format!("{} can't be read ({e})", at.display()))?;
            let Some(name) = e.file_name().to_str().map(str::to_string) else {
                l.others.push(e.path());
                continue;
            };
            if rel.is_empty() && kept_live(&name) {
                continue;
            }
            let t = e.file_type().map_err(|x| format!("{} can't be read ({x})", e.path().display()))?;
            let mut parts = rel.clone();
            parts.push(name);
            if t.is_dir() {
                l.dirs.push(parts.clone());
                todo.push(parts);
            } else if t.is_file() {
                l.files.push(parts);
            } else {
                l.others.push(e.path());
            }
        }
    }
    Ok(l)
}

/// BK9, BK11: every name in the Library now is one a restore would put back
/// from its "before restore" backup: none Windows would read as another or
/// can't keep, and no two differing only in capitals. Else it names the
/// first, to rename before restoring.
fn restorable_names(l: &Live) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for p in l.dirs.iter().chain(&l.files) {
        let name = p.join("/");
        entry_parts(&name).map_err(|e| format!("the Library holds {name}, a name a restore couldn't put back ({e}). Rename it, then restore"))?;
        if !seen.insert(folded(p)) {
            return Err(format!("the Library holds {name} and another name differing only in capitals, which a restore couldn't both put back. Rename one, then restore"));
        }
    }
    Ok(())
}

/// Before anything changes (BK11): where each of the backup's folders and
/// files goes in the Library as it is now. Refused, naming it: a link on the
/// way (a restore never goes through or replaces one), a place that is
/// really somewhere else (outside the Library, or another name for a folder
/// a restore keeps), and a folder to be replaced by a file that holds
/// anything the "before restore" backup doesn't (a link, an odd name).
fn preflight(library: &Path, held: &Held, l: &Live) -> Result<(), String> {
    let root = std::fs::canonicalize(library).map_err(|e| format!("{} can't be read ({e})", library.display()))?;
    let mut checked = std::collections::HashSet::new();
    let file_keys: std::collections::HashSet<String> = held.files.iter().map(|(_, f)| key(f)).collect();
    let dir_keys: std::collections::HashSet<String> = held.dirs.iter().map(|d| key(d)).collect();
    for parts in held.dirs.iter().chain(held.files.iter().map(|(_, f)| f)) {
        for n in 1..=parts.len() {
            let p = &parts[..n];
            if !checked.insert(key(p)) {
                continue;
            }
            let dest = join(library, p);
            let m = match std::fs::symlink_metadata(&dest) {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                Err(e) => return Err(format!("{} can't be read ({e})", dest.display())),
            };
            if m.file_type().is_symlink() {
                return Err(format!("{} is a link; a restore never replaces or goes through one. Move it out of the Library first", dest.display()));
            }
            let real = std::fs::canonicalize(&dest).map_err(|e| format!("{} can't be read ({e})", dest.display()))?;
            let rel = real.strip_prefix(&root).map_err(|_| format!("{} is really {}, outside the shop folder", dest.display(), real.display()))?;
            let names: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
            let same = names.len() == p.len() && names.iter().zip(p).all(|(a, b)| a.eq_ignore_ascii_case(b));
            if !same || names.first().is_some_and(|t| kept_live(t)) {
                return Err(format!("{} is another name for {}", dest.display(), real.display()));
            }
            // LB16 (review S10): a job or library part record, or a deleted
            // one's mark, is never taken out by a change of shape (a folder
            // of them replaced by a file, or one of them by a folder): its
            // number must stay taken.
            let under = |f: &Vec<String>| f.len() > p.len() && key(&f[..p.len()]) == key(p);
            if m.is_dir() && file_keys.contains(&key(p)) && l.files.iter().any(|f| under(f) && record_of(f).is_some()) {
                return Err(format!("{} would replace the folder {}, which holds job or library part records. Restore a backup of this Library", parts.join("/"), dest.display()));
            }
            let needs_folder = n < parts.len() || dir_keys.contains(&key(p));
            if !m.is_dir() && needs_folder && record_of(p).is_some() {
                return Err(format!("{} would replace the record {} with a folder. Restore a backup of this Library", parts.join("/"), dest.display()));
            }
            if m.is_dir() && file_keys.contains(&key(p)) {
                // A folder the backup has a file in place of: all of it must
                // be in the "before restore" backup.
                if let Some(o) = l.others.iter().find(|o| o.starts_with(&dest)) {
                    return Err(format!("{} would replace the folder {}, which holds {}, a link or file no backup holds. Move it out of the Library first", parts.join("/"), dest.display(), o.display()));
                }
            }
        }
    }
    Ok(())
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
///    name that would go outside the Library or Windows would read as
///    another, a link, a name twice, a damaged file, or a file a newer
///    version wrote; nor when the Library holds a file a newer version
///    wrote (the "before restore" backup couldn't be put back here), nor
///    when a place it goes is a link or another name for something.
/// 2. The Library as it is now is backed up ("… before restore", BK9) and
///    that backup read through and checked against what was zipped; if it
///    can't be, nothing is restored.
/// 3. The backup is unpacked into a folder of its own in `Backups`, the
///    restore is recorded in the shop file, unfinished; the numbers of the
///    records it takes out are kept taken; the files are moved into place
///    one by one; then what isn't in the backup is removed (BK10). The shop
///    file then says it finished.
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
    let cant = |e: String| nothing(format!("the backup of {} can't be restored: {e}", from.shown()));
    let mut z = open_zip(&from.path).map_err(cant)?;
    let held = read_backup(&mut z, &versions, lock).map_err(cant)?;
    // The shop file stays and records the restore: never over one a newer
    // version wrote (UP7).
    shared::may_write_over(&crate::shop::file(library), &versions, "The shop file").map_err(nothing)?;
    let now = live(library).map_err(nothing)?;
    // UP7 (BK11): the "before restore" backup must be one this program can
    // put back.
    for f in now.files.iter().filter(|f| stamped(f.last().map_or("", String::as_str))) {
        lock.renew_if_due().map_err(nothing)?;
        if let Ok(b) = std::fs::read(join(library, f))
            && let Some(n) = versions.newer(shared::recorded_version(&b).as_deref())
        {
            return Err(nothing(format!("Update this PC first: {} was saved by version {}, newer than this program ({}), so the Library as it is now couldn't be put back on this PC", f.join("/"), n.written, versions.this)));
        }
    }
    // BK9: the "before restore" backup must be one a restore can take:
    // every name in the Library one it would put back.
    restorable_names(&now).map_err(nothing)?;
    preflight(library, &held, &now).map_err(cant)?;
    lock.still_held().map_err(nothing)?;
    // BK9: the Library as it is now, first, read through and checked.
    let before = match take_as(library, &Settings::default(), who, clock, Why::Now, true) {
        Ok(Daily::Taken(d)) => d,
        Ok(Daily::AlreadyToday { .. }) => return Err(nothing("the backup of the Library as it is now wasn't taken".into())),
        Err(e) => return Err(nothing(format!("the Library as it is now couldn't be backed up first ({e})"))),
    };
    shared::pause("restore: before restore taken");
    verify(&before.entry.path, &ns.id, &before.listing, &versions, lock).map_err(|e| nothing(format!("the backup of the Library as it is now ({}) can't be trusted: {e}", before.entry.name)))?;
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
    lock.still_held().map_err(kept_before)?;
    std::fs::create_dir(&stage).map_err(|e| kept_before(format!("{} can't be made ({e})", stage.display())))?;
    // The working folder is this restore's own, in Backups: cleared while
    // the lock is still this PC's.
    let clear = |lock: &WriteLock| {
        if lock.still_held().is_ok() {
            let _ = std::fs::remove_dir_all(&stage);
        }
    };
    if let Err(e) = unpack(&mut z, &held, &stage, lock) {
        clear(lock);
        return Err(kept_before(format!("the backup of {} couldn't be unpacked ({e})", from.shown())));
    }
    drop(z);
    shared::pause("restore: unpacked");
    // BK12: every revision a record has now, or had in the backup, stays
    // below the floor, so no save after the restore repeats one (LB7).
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
    for f in &now.files {
        if record_of(f).is_some_and(|(_, mark)| !mark)
            && let Ok(b) = std::fs::read(join(library, f))
        {
            floor = floor.max(record_revision(&b));
        }
    }
    // BK12: the new generation is published only once every file is in
    // place; while unfinished, nothing read counts as read in any (see
    // `shop::restore_generation`).
    let generation = shop.restored.as_ref().map_or(0, |r| r.generation);
    shop.restored = Some(crate::shop::Restore { backup: from.name.clone(), backup_when: from.stamp(), before: before.entry.name.clone(), by: who.clone(), at: clock.now(), finished: false, revision_floor: floor, generation, lock: lock.token() });
    if let Err(e) = lock.renew_if_due().and_then(|()| crate::shop::write(library, &shop, lock, version)) {
        clear(lock);
        return Err(kept_before(format!("the shop file couldn't record the restore ({e})")));
    }
    shared::pause("restore: recorded");
    // LB16: the numbers of the records it takes out stay taken, before any
    // is taken out.
    if let Err(e) = reserve(library, &held, &now, lock) {
        clear(lock);
        return Err(kept_before(format!("the number of a record it takes out couldn't be kept taken ({e}); only such marks were added")));
    }
    // From here a failure leaves the Library partly restored.
    let halfway = |e: String| {
        format!(
            "The restore stopped halfway: {e}. Some of the Library's files are from the backup of {}, the rest are as they were. The backup {} holds the whole Library as it was before: restore it to put the Library back as it was.",
            from.shown(),
            before.entry.name
        )
    };
    let swapped = swap(library, &stage, &held, &now, lock);
    clear(lock);
    let (files, removed) = swapped.map_err(halfway)?;
    if let Some(r) = shop.restored.as_mut() {
        r.finished = true;
        r.generation = generation + 1;
    }
    lock.renew_if_due().and_then(|()| crate::shop::write(library, &shop, lock, version)).map_err(|e| halfway(format!("every file was put back, but the shop file couldn't record that the restore finished ({e})")))?;
    Ok(Restored { from, before: before.entry, files, removed })
}

/// LB16: a record (or a deleted record's mark) here now that the backup has
/// no record for keeps its number taken: its mark is written, fenced by the
/// lock, before anything is taken out. A mark that can't be written stops
/// the restore.
fn reserve(library: &Path, held: &Held, l: &Live, lock: &mut WriteLock) -> Result<(), String> {
    let records: std::collections::HashSet<String> = held.files.iter().filter_map(|(_, f)| record_of(f)).filter(|(_, m)| !m).map(|(r, _)| key(&r)).collect();
    for f in &l.files {
        let Some((rec, _)) = record_of(f) else { continue };
        if records.contains(&key(&rec)) {
            continue;
        }
        let mut mark = rec.clone();
        if let Some(last) = mark.last_mut() {
            last.push_str(".deleted");
        }
        let p = join(library, &mark);
        lock.renew_if_due()?;
        lock.still_held()?;
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&p) {
            Ok(f) => f.sync_all().map_err(|e| format!("{} can't be written ({e})", p.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("{} can't be written ({e})", p.display())),
        }
    }
    Ok(())
}

/// Remove one file of the Library, fenced by the lock (renewed); one that's
/// gone already counts.
fn remove_one(p: &Path, lock: &mut WriteLock) -> Result<bool, String> {
    lock.renew_if_due()?;
    lock.still_held()?;
    shared::pause("restore: removing");
    match std::fs::remove_file(p) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("{} can't be removed ({e})", p.display())),
    }
}

/// Move the unpacked backup into the Library, file by file under the lock
/// (renewed, and checked before each change), then remove the files and
/// empty folders that aren't in the backup (BK10, BK11). A folder the backup
/// has a file in place of goes first, file by file (`preflight` checked it
/// holds only what the "before restore" backup holds). Deleted records'
/// marks are never removed (LB16). Gives the files put back and removed.
fn swap(library: &Path, stage: &Path, held: &Held, l: &Live, lock: &mut WriteLock) -> Result<(usize, usize), String> {
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
    let mut gone = std::collections::HashSet::new();
    let mut removed = 0;
    // Take out a live folder (its files, then its folders, deepest first)
    // where the backup has a file or another kind of thing.
    let clear_dir = |d: &[String], lock: &mut WriteLock, gone: &mut std::collections::HashSet<String>, removed: &mut usize| -> Result<(), String> {
        let under = |p: &[String]| p.len() > d.len() && key(&p[..d.len()]) == key(d);
        for f in l.files.iter().filter(|f| under(f)) {
            if gone.insert(key(f)) && remove_one(&join(library, f), lock)? {
                *removed += 1;
            }
        }
        let mut sub: Vec<&Vec<String>> = l.dirs.iter().filter(|x| under(x)).collect();
        sub.sort_by_key(|x| std::cmp::Reverse(x.len()));
        for x in sub.into_iter().map(|x| x.as_slice()).chain(std::iter::once(d)) {
            lock.renew_if_due()?;
            lock.still_held()?;
            let p = join(library, x);
            std::fs::remove_dir(&p).map_err(|e| format!("{} can't be removed ({e})", p.display()))?;
            gone.insert(key(x));
        }
        Ok(())
    };
    dirs.sort_by_key(Vec::len);
    for d in &dirs {
        let p = join(library, d);
        match std::fs::symlink_metadata(&p) {
            Ok(m) if m.is_dir() => continue,
            Ok(_) => {
                // A file where the backup has a folder: it's in the "before
                // restore" backup.
                if !gone.insert(key(d)) {
                    return Err(format!("{} was taken out already", p.display()));
                }
                if remove_one(&p, lock)? {
                    removed += 1;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{} can't be read ({e})", p.display())),
        }
        lock.renew_if_due()?;
        lock.still_held()?;
        std::fs::create_dir(&p).map_err(|e| format!("{} can't be made ({e})", p.display()))?;
    }
    let mut put = 0;
    for (_, f) in &held.files {
        let to = join(library, f);
        if std::fs::symlink_metadata(&to).is_ok_and(|m| m.is_dir()) {
            clear_dir(f, lock, &mut gone, &mut removed)?;
        }
        lock.renew_if_due()?;
        shared::move_over(&join(stage, f), &to, lock).map_err(|e| format!("{} can't be put back ({e})", to.display()))?;
        put += 1;
        shared::pause("restore: file put back");
        #[cfg(test)]
        if RESTORE_STOP_AFTER.with(|s| s.get()).is_some_and(|n| put >= n) {
            return Err("the test stopped it halfway".into());
        }
    }
    for f in l.files.iter().filter(|f| !keep.contains(&key(f))) {
        // Taken out already with its folder, or a deleted record's mark.
        if gone.contains(&key(f)) || record_of(f).is_some_and(|(_, mark)| mark) {
            continue;
        }
        if remove_one(&join(library, f), lock)? {
            removed += 1;
        }
    }
    let mut old: Vec<&Vec<String>> = l.dirs.iter().filter(|d| !keep.contains(&key(d)) && !gone.contains(&key(d))).collect();
    old.sort_by_key(|d| std::cmp::Reverse(d.len()));
    for d in old {
        lock.renew_if_due()?;
        lock.still_held()?;
        // Only when empty: a folder holding a mark or a link stays.
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
        assert!(is_device("CON") && is_device("nul.txt") && is_device("COM1.dxf") && is_device("lpt9") && is_device("COM0") && is_device("COM¹") && is_device("lpt³.txt") && is_device("CONIN$") && !is_device("COM10") && !is_device("Console.txt") && !is_device("CONTOUR.dxf"));
        // S2: names Windows would read as another, or can't keep.
        for bad in ["Shop./x", "Shop /x", "SHOP~1/x", "Backups/../x", "Jobs/COM¹.dxf", "Jobs/nul", "a<b", "Jobs/a?.dxf", "x/y."] {
            assert!(p(bad).is_err(), "{bad}");
        }
        assert!(p("Jobs/Job~A.dxf").is_ok() && p("Jobs/v1.2 final.dxf").is_ok());
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
        // S10: any capitals, as Windows keeps them.
        assert_eq!(r("jobs/0/7.SNJOB"), Some(("jobs/0/7.SNJOB".into(), false)));
        assert_eq!(r("JOBS/0/7.snjob.DELETED"), Some(("JOBS/0/7.snjob".into(), true)));
        assert_eq!(r("Jobs/7.snjob"), None);
        assert_eq!(record_revision(br#"{"library": {"id": 7, "revision": 5}}"#), 5);
        assert_eq!(record_revision(br#"{"revision": 3, "part": {}}"#), 3);
        assert_eq!(record_revision(b"0\nEOF"), 0);
        assert_eq!(shown_stamp("2026-11-09 09:20"), "9 Nov 09:20");
        assert_eq!(shown_stamp("soon"), "soon");
    }

    /// S1 rest (review round 2): a "before restore" backup is checked
    /// folders and all: a folder missing from what was zipped, or one too
    /// many, fails it.
    #[test]
    fn the_before_restore_check_counts_folders_too() {
        let root = std::env::temp_dir().join(format!("nestpro_backup_verify_dirs_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let lib = root.join("Library");
        crate::library::create(&lib).unwrap();
        std::fs::create_dir_all(lib.join("Empty folder")).unwrap();
        let clock = Clock::at(1_794_216_000);
        let Daily::Taken(d) = take(&lib, &Settings::default(), &Who::new("sam", "PC-A"), &clock, Why::Now).unwrap() else { panic!("no backup taken") };
        let id = library_id(&lib).unwrap().unwrap();
        let versions = shared::Versions::this(crate::updates::VERSION);
        let mut lock = WriteLock::take(&lib).unwrap();
        verify(&d.entry.path, &id, &d.listing, &versions, &mut lock).unwrap();
        assert!(d.listing.iter().any(|(n, _)| n == "Empty folder/"), "{:?}", d.listing);
        let without: Vec<(String, u64)> = d.listing.iter().filter(|(n, _)| n != "Empty folder/").cloned().collect();
        assert!(verify(&d.entry.path, &id, &without, &versions, &mut lock).is_err(), "a folder that wasn't zipped passed");
        drop(lock);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn day_numbers_count_days() {
        assert_eq!(day_number((1970, 1, 1)), 0);
        assert_eq!(day_number((2026, 11, 9)) - day_number((2026, 10, 10)), 30);
        assert_eq!(day_number((2025, 3, 1)) - day_number((2025, 2, 28)), 1);
    }
}
