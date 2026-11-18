//! Writing to a Library that several PCs share (ADR 0003: the Library is a
//! folder on the shop's share). Every change to a shared file in the Library
//! (job and library part records, the list of records, the sheet stock) is
//! made under the Library's write lock (LB1); a lock left by a PC that
//! crashed is taken over after a while, by exactly one PC (LB2); and every
//! file is written through a temporary file of this writer's own and renamed
//! into place (LB3). Reading never waits for the lock: a file is always whole.
//!
//! How the lock holds with nothing but files on a share (LB2):
//! - `library.lock` is made with create-new, so only one writer gets it, and
//!   only its holder ever removes it.
//! - A PC taking over an abandoned lock never deletes it. It first makes a
//!   mark named after that lock's token (`library.lock.<token>.taken`) with
//!   create-new, so only one PC can take over each abandoned lock; then it
//!   checks the lock is still the abandoned one and renames its own lock
//!   over it. A PC that stops halfway leaves its mark, which is taken over in
//!   turn the same way.
//! - A writer replaces a file only while it has held the lock for less than
//!   a third of the abandon time (30 s of 90 s), by its own clocks, and only
//!   after checking the lock is still its own and nobody has begun to take it
//!   over. A takeover needs the lock seen unchanged for the whole abandon
//!   time, so what a writer replaces lands first unless the share sits on
//!   the request for more than the 60 s between.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

/// Test seam: tests stop a writer at a named step (`take over`, `record
/// written`, `lock checked`…) to play out how two PCs' steps interleave.
/// Does nothing outside tests.
pub(crate) fn pause(step: &str) {
    #[cfg(test)]
    hooks::run(step);
    let _ = step;
}

#[cfg(test)]
pub(crate) mod hooks {
    use std::cell::RefCell;
    thread_local! {
        static HOOK: RefCell<Option<Box<dyn FnMut(&str)>>> = const { RefCell::new(None) };
    }
    /// Call `f` at every step this thread reaches, until `clear`.
    pub fn set(f: impl FnMut(&str) + 'static) {
        HOOK.with(|h| *h.borrow_mut() = Some(Box::new(f)));
    }
    pub fn clear() {
        HOOK.with(|h| *h.borrow_mut() = None);
    }
    pub(super) fn run(step: &str) {
        // Taken out while it runs, so a step reached inside it doesn't re-enter it.
        let f = HOOK.with(|h| h.borrow_mut().take());
        if let Some(mut f) = f {
            f(step);
            HOOK.with(|h| {
                let mut h = h.borrow_mut();
                if h.is_none() {
                    *h = Some(f);
                }
            });
        }
    }
}

/// The write lock's file, in the Library folder (LB1).
pub const LOCK_FILE: &str = "library.lock";
/// A write lock (or a takeover's mark) that a waiting PC has seen unchanged
/// this long was left by a PC that crashed or lost the share, and is taken
/// over (LB2).
pub const ABANDONED_AFTER: Duration = Duration::from_secs(90);
/// A writer replaces files only this long after it took the lock, by its own
/// clocks: a third of ABANDONED_AFTER, leaving 60 s for a request already
/// sent to reach the share before anyone may take the lock over (LB2).
pub const HOLD_FOR: Duration = Duration::from_secs(ABANDONED_AFTER.as_secs() / 3);
/// The longest a change waits for the write lock before it gives up,
/// storing nothing (LB1). Longer than ABANDONED_AFTER, so a lock left by a
/// crash is taken over rather than given up on.
pub const LONGEST_WAIT: Duration = Duration::from_secs(120);
/// How many takeovers in a row (PCs that stopped halfway through taking a
/// lock over) are followed before the rest are ignored.
const MAX_MARKS: usize = 16;

/// Who is working: the Windows user and PC name (user and host name on
/// Linux), and whether it's an agent (the command line or the MCP server)
/// working for that user (LB5). The window and the agents give the engine
/// their own; it never reads them itself, so a test can be two PCs at once.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Who {
    pub user: String,
    pub pc: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub agent: bool,
}

impl Who {
    /// This PC and the user logged on to it.
    pub fn this_pc() -> Who {
        Who::new(&user_name(), &pc_name())
    }

    pub fn new(user: &str, pc: &str) -> Who {
        Who { user: user.into(), pc: pc.into(), agent: false }
    }

    /// The same user and PC, working through an agent.
    pub fn as_agent(mut self) -> Who {
        self.agent = true;
        self
    }

    /// "sam on PROG-1", or "sam on PROG-1 (agent)".
    pub fn label(&self) -> String {
        format!("{} on {}{}", self.user, self.pc, if self.agent { " (agent)" } else { "" })
    }
}

/// Who saved a Library record, and when (seconds since 1970) (LB5).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub by: Who,
    pub at: u64,
}

/// The clock the engine goes by: seconds since 1970, shown in local time.
/// The window and the agents give the engine this PC's clock; a test gives
/// it one that stands still until the test moves it (shown as UTC), shared
/// by every PC the test plays.
#[derive(Clone, Debug, Default)]
pub struct Clock(Option<std::sync::Arc<AtomicU64>>);

impl Clock {
    /// This PC's clock and time zone.
    pub fn pc() -> Clock {
        Clock(None)
    }

    /// A clock set to `secs` since 1970 (tests).
    pub fn at(secs: u64) -> Clock {
        Clock(Some(std::sync::Arc::new(AtomicU64::new(secs))))
    }

    /// Move a set clock forward (this PC's clock moves by itself).
    pub fn advance(&self, d: Duration) {
        if let Some(t) = &self.0 {
            t.fetch_add(d.as_secs(), Ordering::SeqCst);
        }
    }

    pub fn now(&self) -> u64 {
        self.0.as_ref().map_or_else(now_secs, |t| t.load(Ordering::SeqCst))
    }

    /// `t` in local time: year, month, day, hour, minute.
    fn local(&self, t: u64) -> (i64, u32, u32, u32, u32) {
        let zone = if self.0.is_some() { 0 } else { local_offset(t) };
        civil(t as i64 + zone)
    }

    /// "9 Nov 09:20" in local time, with the year when it isn't this year
    /// ("9 Nov 2025 09:20") (LB6).
    pub fn shown(&self, t: u64) -> String {
        let (y, m, d, hh, mm) = self.local(t);
        let month = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][(m as usize - 1).min(11)];
        if y == self.local(self.now()).0 { format!("{d} {month} {hh:02}:{mm:02}") } else { format!("{d} {month} {y} {hh:02}:{mm:02}") }
    }

    /// "at 09:20" for a time today, else "on 9 Nov 09:20" (with the year
    /// when it isn't this year), in local time (SV10).
    pub fn when(&self, t: u64) -> String {
        let (y, m, d, hh, mm) = self.local(t);
        let now = self.local(self.now());
        if (y, m, d) == (now.0, now.1, now.2) { format!("at {hh:02}:{mm:02}") } else { format!("on {}", self.shown(t)) }
    }

    /// "2026-11-09 09:20" in local time (agents) (LB6).
    pub fn stamp(&self, t: u64) -> String {
        let (y, m, d, hh, mm) = self.local(t);
        format!("{y}-{m:02}-{d:02} {hh:02}:{mm:02}")
    }
}

/// Year, month, day, hour and minute of `t` seconds since 1970 (Howard
/// Hinnant's civil-from-days).
fn civil(t: i64) -> (i64, u32, u32, u32, u32) {
    let (day, s) = (t.div_euclid(86400), t.rem_euclid(86400));
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d, (s / 3600) as u32, (s % 3600 / 60) as u32)
}

/// Seconds since 1970 of a civil date (Howard Hinnant's days-from-civil).
#[cfg(windows)]
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// What to add to `t` (UTC) for this PC's local time at that moment.
#[cfg(windows)]
fn local_offset(t: u64) -> i64 {
    #[repr(C)]
    #[derive(Default)]
    #[allow(dead_code)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        ms: u16,
    }
    unsafe extern "system" {
        fn SystemTimeToTzSpecificLocalTime(zone: *const std::ffi::c_void, utc: *const SystemTime, local: *mut SystemTime) -> i32;
    }
    let t = t - t % 60;
    let (y, m, d, hh, mm) = civil(t as i64);
    let utc = SystemTime { year: y as u16, month: m as u16, day: d as u16, hour: hh as u16, minute: mm as u16, ..Default::default() };
    let mut local = SystemTime::default();
    // SAFETY: both point to SYSTEMTIMEs that live through the call; no zone
    // (null) means this PC's own time zone.
    if unsafe { SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) } == 0 {
        return 0;
    }
    let back = days_from_civil(i64::from(local.year), u32::from(local.month), u32::from(local.day)) * 86400 + i64::from(local.hour) * 3600 + i64::from(local.minute) * 60;
    back - t as i64
}

/// What to add to `t` (UTC) for this PC's local time at that moment.
#[cfg(all(unix, target_pointer_width = "64"))]
fn local_offset(t: u64) -> i64 {
    use std::os::raw::{c_char, c_int, c_long};
    #[repr(C)]
    #[allow(dead_code)]
    struct Tm {
        sec: c_int,
        min: c_int,
        hour: c_int,
        mday: c_int,
        mon: c_int,
        year: c_int,
        wday: c_int,
        yday: c_int,
        isdst: c_int,
        gmtoff: c_long,
        zone: *const c_char,
    }
    unsafe extern "C" {
        fn localtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
    }
    let secs = t as i64;
    let mut tm = std::mem::MaybeUninit::<Tm>::zeroed();
    // SAFETY: `secs` and `tm` live through the call; localtime_r fills `tm`
    // and returns null when it can't.
    let ok = unsafe { localtime_r(&secs, tm.as_mut_ptr()) };
    if ok.is_null() {
        return 0;
    }
    // SAFETY: filled by localtime_r above.
    i64::from(unsafe { tm.assume_init() }.gmtoff)
}

/// UTC, where the time zone can't be asked.
#[cfg(not(any(windows, all(unix, target_pointer_width = "64"))))]
fn local_offset(_t: u64) -> i64 {
    0
}

/// This PC's name: `COMPUTERNAME` on Windows, the host name elsewhere.
pub fn pc_name() -> String {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        let env = |k: &str| std::env::var(k).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let name = if cfg!(windows) {
            env("COMPUTERNAME")
        } else {
            env("HOSTNAME").or_else(|| ["/proc/sys/kernel/hostname", "/etc/hostname"].iter().find_map(|f| std::fs::read_to_string(f).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())))
        };
        name.unwrap_or_else(|| "this PC".into())
    })
    .clone()
}

/// The user logged on: `USERNAME` on Windows, `USER` elsewhere.
pub fn user_name() -> String {
    ["USERNAME", "USER", "LOGNAME"].iter().find_map(|k| std::env::var(k).ok().filter(|s| !s.trim().is_empty())).unwrap_or_else(|| "someone".into())
}

/// A name made safe for a file name (letters, digits, `-` and `_`).
pub fn file_safe(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).take(40).collect();
    if t.is_empty() { "PC".into() } else { t }
}

/// A tag no other writer has: this PC's name, the process and a counter
/// (`PROG-1-4312-7`). Names temporary files and write locks (LB3).
pub fn writer_tag() -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!("{}-{}-{}", file_safe(&pc_name()), std::process::id(), N.fetch_add(1, Ordering::Relaxed))
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// "30 s", or "200 ms" for the short times tests use.
fn shown(d: Duration) -> String {
    if d >= Duration::from_secs(1) { format!("{} s", d.as_secs()) } else { format!("{} ms", d.as_millis()) }
}

/// What the lock file (or a takeover's mark) says: who holds the lock, since
/// when, and the token that tells this holder's lock from every other.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Holder {
    user: String,
    pc: String,
    process: u32,
    /// Seconds since 1970 on the holder's clock (shown only; never compared
    /// with this PC's clock).
    since: u64,
    token: String,
    /// How many times the holder renewed it (BK3): each renewal changes the
    /// file, so a waiting PC sees it in use and never takes it over.
    #[serde(default, skip_serializing_if = "is_zero")]
    renewed: u64,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// The name a lock file (or a mark) goes by in the name of the mark that
/// takes it over: its token, or a fingerprint of what's in it when it can't
/// be read (a PC stopped while making it).
fn key(text: &[u8]) -> String {
    match serde_json::from_slice::<Holder>(text) {
        Ok(h) if !h.token.is_empty() && h.token.len() <= 100 && h.token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') => h.token,
        _ => {
            // FNV-1a: the same everywhere, unlike the standard hasher.
            let h = text.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3));
            format!("x{h:016x}")
        }
    }
}

/// The mark a PC makes to take over the lock (or mark) holding `text` (LB2).
fn mark_path(lock: &Path, text: &[u8]) -> PathBuf {
    lock.with_file_name(format!("{LOCK_FILE}.{}.taken", key(text)))
}

/// The lock file, then the mark of each PC taking it over in turn: the last
/// one is whoever is writing (or taking over) now. Empty when the Library
/// isn't locked.
fn read_chain(lock: &Path) -> Vec<Vec<u8>> {
    let mut chain = vec![];
    let Ok(t) = std::fs::read(lock) else { return chain };
    chain.push(t);
    while chain.len() < MAX_MARKS {
        match chain.last().map(|t| std::fs::read(mark_path(lock, t))) {
            Some(Ok(t)) => chain.push(t),
            _ => break,
        }
    }
    chain
}

/// "sam on PROG-2", from a lock file's text.
fn holder_label(text: Option<&Vec<u8>>) -> String {
    text.and_then(|t| serde_json::from_slice::<Holder>(t).ok()).map_or_else(|| "another PC".to_string(), |h| format!("{} on {}", h.user, h.pc))
}

/// Test seam: time a test plays as gone by on this thread on top of the real
/// time (a share that answers slowly), so tests of a writer's time neither
/// sleep nor hang on how loaded the machine is. Always none outside tests.
fn played() -> Duration {
    #[cfg(test)]
    return PLAYED.with(|p| p.get());
    #[cfg(not(test))]
    Duration::ZERO
}

#[cfg(test)]
thread_local! {
    static PLAYED: std::cell::Cell<Duration> = const { std::cell::Cell::new(Duration::ZERO) };
}

/// Play `d` as gone by for the writer's time of this thread's locks (tests).
#[cfg(test)]
pub(crate) fn play_time(d: Duration) {
    PLAYED.with(|p| p.set(p.get() + d));
}

/// How long since `since` (and `wall`), by this PC's steady clock and its
/// wall clock, the longer, with the time a test played since `played0`.
fn gone(since: Instant, wall: SystemTime, played0: Duration) -> Duration {
    let real = since.elapsed().max(wall.elapsed().unwrap_or_default());
    real + played().saturating_sub(played0)
}

/// Still inside this writer's time: held for less than `hold_for` since
/// `since`, by this PC's steady clock and its wall clock both (the steady
/// one doesn't count time asleep on every system) (LB2).
fn within(since: Instant, wall: SystemTime, played0: Duration, hold_for: Duration) -> Result<(), String> {
    if gone(since, wall, played0) < hold_for {
        Ok(())
    } else {
        Err(format!("this PC took longer than {} to write to the Library, so another PC may take its write lock over. Nothing was saved — save again", shown(hold_for)))
    }
}

/// A claim on the lock: a fresh token, and the text of a lock file (or
/// mark) naming this PC with it. Every claim has its own token, so no two
/// lock files or marks ever say the same.
fn claim() -> (String, String) {
    let who = Who::this_pc();
    let token = format!("{}-{}", writer_tag(), SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos()));
    let me = Holder { user: who.user, pc: who.pc, process: std::process::id(), since: now_secs(), token: token.clone(), renewed: 0 };
    let text = serde_json::to_string(&me).unwrap_or_default();
    (token, text)
}

/// A lock this writer holds: which, its token, and its time.
#[derive(Clone)]
struct Lease {
    file: PathBuf,
    token: String,
    /// When it was taken (just before asking for it), by both clocks.
    since: Instant,
    wall: SystemTime,
    /// The time a test had played on this thread at `since` (tests).
    played0: Duration,
    /// How long after `since` it may still replace files (LB2).
    hold_for: Duration,
}

thread_local! {
    /// Locks this thread holds, and how many times each is taken.
    static HELD: std::cell::RefCell<Vec<(Lease, usize)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The Library's write lock, held until dropped (LB1). Taking it again on
/// the same thread while it's held just counts, so a change made inside
/// another never waits for itself.
pub struct WriteLock(Lease);

impl WriteLock {
    /// Take the write lock of the Library in `dir`, waiting while another
    /// writer has it (LB1) and taking over one that was abandoned (LB2).
    pub fn take(dir: &Path) -> Result<WriteLock, String> {
        Self::take_for(dir, ABANDONED_AFTER, HOLD_FOR, LONGEST_WAIT)
    }

    /// `take` with other times (tests). The lock replaces files for a third
    /// of `abandoned_after`, as HOLD_FOR is of ABANDONED_AFTER.
    pub fn take_with(dir: &Path, abandoned_after: Duration, longest_wait: Duration) -> Result<WriteLock, String> {
        Self::take_for(dir, abandoned_after, abandoned_after / 3, longest_wait)
    }

    /// `take` with its own times to be abandoned after and to write in
    /// (tests).
    #[cfg(test)]
    pub(crate) fn take_holding(dir: &Path, abandoned_after: Duration, hold_for: Duration, longest_wait: Duration) -> Result<WriteLock, String> {
        Self::take_for(dir, abandoned_after, hold_for, longest_wait)
    }

    fn take_for(dir: &Path, abandoned_after: Duration, hold_for: Duration, longest_wait: Duration) -> Result<WriteLock, String> {
        let file = dir.join(LOCK_FILE);
        let again = HELD.with(|h| {
            h.borrow_mut().iter_mut().find(|(l, _)| l.file == file).map(|(l, n)| {
                *n += 1;
                l.clone()
            })
        });
        if let Some(lease) = again {
            return Ok(WriteLock(lease));
        }
        // A folder that isn't there is never made here: the share may be
        // down (LB11).
        if !dir.is_dir() {
            return Err(crate::library::unreachable(dir));
        }
        let (token, text) = claim();
        let started = Instant::now();
        // The lock (and marks) seen last, and since when this PC has seen
        // them unchanged: judged by this PC's own watch, never by another
        // PC's clock (LB2).
        let mut seen: Option<(Vec<Vec<u8>>, Instant)> = None;
        let mut wait = Duration::from_millis(2);
        loop {
            let (since, wall) = (Instant::now(), SystemTime::now());
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&file) {
                Ok(mut f) => {
                    use std::io::Write as _;
                    if let Err(e) = f.write_all(text.as_bytes()).and_then(|_| f.sync_all()) {
                        drop(f);
                        let _ = std::fs::remove_file(&file);
                        return Err(format!("can't lock the Library at {}: {e}", dir.display()));
                    }
                    return Ok(Lease { file, token, since, wall, played0: played(), hold_for }.held());
                }
                // Windows reports a lock file being deleted by its holder as
                // access denied: busy, like one that's there.
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists || (cfg!(windows) && e.kind() == std::io::ErrorKind::PermissionDenied) => {
                    let chain = read_chain(&file);
                    match &seen {
                        Some((c, t)) if !chain.is_empty() && *c == chain => {
                            if t.elapsed() >= abandoned_after {
                                pause("take over");
                                if let Some(lease) = take_over(&file, &chain, hold_for) {
                                    return Ok(lease.held());
                                }
                                seen = None;
                                continue;
                            }
                        }
                        _ => seen = (!chain.is_empty()).then(|| (chain.clone(), Instant::now())),
                    }
                    if started.elapsed() >= longest_wait {
                        return Err(format!("the Library at {} is busy: {} has been writing to it for more than {}. Nothing was saved — try again.", dir.display(), holder_label(chain.first()), shown(longest_wait)));
                    }
                    pause("lock busy");
                    // A little jitter so two waiting PCs don't keep colliding.
                    let jitter = Duration::from_micros(u64::from(SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos())) % 3000);
                    std::thread::sleep(wait + jitter);
                    wait = (wait * 2).min(Duration::from_millis(40));
                }
                Err(e) => return Err(format!("can't lock the Library at {}: {e}", dir.display())),
            }
        }
    }

    /// Still this writer's to write with (LB2): inside its time, still its
    /// own lock, and nobody has begun to take it over. Checked just before
    /// every file is replaced.
    pub fn still_held(&self) -> Result<(), String> {
        let l = &self.0;
        within(l.since, l.wall, l.played0, l.hold_for)?;
        let now = std::fs::read(&l.file).ok();
        let ours = now.as_deref().and_then(|t| serde_json::from_slice::<Holder>(t).ok()).is_some_and(|h| h.token == l.token);
        let taking_over = now.as_deref().is_some_and(|t| mark_path(&l.file, t).exists());
        pause("lock read");
        if !ours || taking_over {
            return Err("another PC took over the Library's write lock while this PC was writing. Nothing was saved — save again".into());
        }
        // Reading the lock can be slow on a share: still inside the time
        // after it, so only the change itself is left to send (LB2).
        within(l.since, l.wall, l.played0, l.hold_for)
    }

    /// Renew the lock for a long job (a backup, BK3): rewrite the lock file,
    /// same token, so a PC waiting for it sees it change and starts its
    /// abandon time again, and this writer's own time to write starts again.
    /// Only while it's still this writer's (`still_held`); otherwise nothing
    /// is changed and the job must stop.
    pub fn renew(&mut self) -> Result<(), String> {
        self.still_held()?;
        let l = &self.0;
        let mut h: Holder = std::fs::read(&l.file).ok().and_then(|t| serde_json::from_slice(&t).ok()).ok_or("the Library's write lock can't be read")?;
        h.since = now_secs();
        h.renewed += 1;
        let text = serde_json::to_vec(&h).map_err(|e| e.to_string())?;
        let (since, wall, played0) = (Instant::now(), SystemTime::now(), played());
        replace_in(&l.file, &text, || self.still_held(), false)?;
        let (file, token) = (l.file.clone(), l.token.clone());
        HELD.with(|held| {
            for (x, _) in held.borrow_mut().iter_mut().filter(|(x, _)| x.file == file && x.token == token) {
                (x.since, x.wall, x.played0) = (since, wall, played0);
            }
        });
        (self.0.since, self.0.wall, self.0.played0) = (since, wall, played0);
        Ok(())
    }

    /// `renew` once a third of this writer's time has gone, so a long job
    /// that checks often keeps the lock without ever running out of time.
    pub fn renew_if_due(&mut self) -> Result<(), String> {
        if gone(self.0.since, self.0.wall, self.0.played0) >= self.0.hold_for / 3 {
            self.renew()
        } else {
            Ok(())
        }
    }
}

impl Lease {
    /// Note this lock as held by this thread.
    fn held(self) -> WriteLock {
        HELD.with(|h| h.borrow_mut().push((self.clone(), 1)));
        WriteLock(self)
    }
}

/// Take over the abandoned lock (or mark) at the end of `chain` (LB2). Only
/// one PC can: it makes the mark named after it with create-new, checks that
/// nothing has changed, and renames its own lock over `library.lock`; then
/// the old marks are cleared. None when another PC is taking it over, or
/// it changed or went.
fn take_over(file: &Path, chain: &[Vec<u8>], hold_for: Duration) -> Option<Lease> {
    use std::io::Write as _;
    let (token, me) = claim();
    // This PC's time runs from making the mark: from then on others see it
    // and could judge it abandoned in turn.
    let (since, wall, played0) = (Instant::now(), SystemTime::now(), played());
    let mark = mark_path(file, chain.last()?);
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&mark).ok()?;
    let written = f.write_all(me.as_bytes()).and_then(|_| f.sync_all());
    drop(f);
    // Half a mark is taken over in turn once it has stayed that way.
    written.ok()?;
    pause("take over: marked");
    // Gone or replaced since: there's nothing to take over, and this mark is
    // of no use to anyone (no lock holding what it names comes back).
    let root_moved_on = || read_chain(file).first() != chain.first();
    let mut expect = chain.to_vec();
    expect.push(me.as_bytes().to_vec());
    if read_chain(file) != expect {
        if root_moved_on() {
            let _ = std::fs::remove_file(&mark);
        }
        return None;
    }
    pause("take over: checked");
    if replace(file, me.as_bytes(), || within(since, wall, played0, hold_for)).is_err() {
        if root_moved_on() {
            let _ = std::fs::remove_file(&mark);
        }
        return None;
    }
    // The old lock is gone: its marks, this PC's too, are history.
    for t in chain {
        let _ = std::fs::remove_file(mark_path(file, t));
    }
    Some(Lease { file: file.to_path_buf(), token, since, wall, played0, hold_for })
}

impl WriteLock {
    /// Let go of the lock and look that its file is gone (SH11): Err names
    /// the lock file this writer made but couldn't remove (a share that lets
    /// a PC make files but not remove them), so a refusal can say so.
    pub fn release(self) -> Result<(), PathBuf> {
        let (file, token) = (self.0.file.clone(), self.0.token.clone());
        drop(self);
        let still = std::fs::read(&file).ok().and_then(|t| serde_json::from_slice::<Holder>(&t).ok()).is_some_and(|h| h.token == token);
        if still { Err(file) } else { Ok(()) }
    }
}

impl Drop for WriteLock {
    fn drop(&mut self) {
        let last = HELD.with(|h| {
            let mut h = h.borrow_mut();
            match h.iter().position(|(l, _)| l.file == self.0.file && l.token == self.0.token) {
                Some(i) => {
                    h[i].1 -= 1;
                    if h[i].1 == 0 {
                        h.remove(i);
                        true
                    } else {
                        false
                    }
                }
                None => false,
            }
        });
        // Removed only by its holder, and only inside its time (LB2): later,
        // it is left for another PC to take over.
        if last && self.still_held().is_ok() {
            let _ = std::fs::remove_file(&self.0.file);
        }
    }
}

/// Write a whole file: into a temporary file of this writer's own next to it
/// (`index.json.PROG-1-4312-7.tmp`), then renamed over it, so a reader never
/// sees half a file and two writers never share a temporary file (LB3). With
/// `lock`, the write lock must still be this writer's just before the rename
/// (LB2); if it isn't, nothing is replaced.
pub fn write_file(path: &Path, bytes: &[u8], lock: Option<&WriteLock>) -> Result<(), String> {
    replace(path, bytes, || lock.map_or(Ok(()), WriteLock::still_held))
}

/// `write_file` into a folder that must already be there: the folder and
/// those above it are never made, so a write into a Library that went away
/// (the share dropped) fails instead of making an empty folder in its place
/// (LB11). The shop's files are written this way (SH2). Nothing is put in
/// the folder or taken out of it unless the write lock is still this
/// writer's: it is checked before the temporary file is made and before a
/// failed one is cleared away (a temporary file left once the lock was lost
/// is harmless: no one reads it).
pub fn write_existing(path: &Path, bytes: &[u8], lock: &WriteLock) -> Result<(), String> {
    replace_in(path, bytes, || lock.still_held(), false)
}

/// `write_file`, checking `fence` just before every try of the rename.
fn replace(path: &Path, bytes: &[u8], fence: impl Fn() -> Result<(), String>) -> Result<(), String> {
    replace_in(path, bytes, fence, true)
}

/// `replace`, making the file's folder first only when `make_parent`.
/// Without `make_parent` (`write_existing`), `fence` is also checked before
/// the temporary file is made and before a failed one is removed.
fn replace_in(path: &Path, bytes: &[u8], fence: impl Fn() -> Result<(), String>, make_parent: bool) -> Result<(), String> {
    if let Some(d) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        if make_parent {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        } else if !d.is_dir() {
            return Err(format!("{} can't be reached", d.display()));
        }
    }
    let strict = !make_parent;
    if strict {
        fence()?;
    }
    let name = path.file_name().map_or_else(|| "file".into(), |n| n.to_string_lossy().into_owned());
    let tmp = path.with_file_name(format!("{name}.{}.tmp", writer_tag()));
    let written = (|| {
        use std::io::Write as _;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()
    })();
    let done = written.map_err(|e| e.to_string()).and_then(|_| rename(&tmp, path, &fence));
    if done.is_err() && (!strict || fence().is_ok()) {
        let _ = std::fs::remove_file(&tmp);
    }
    done
}

/// Rename after `fence` says it may, trying again for a moment when Windows
/// says the file is busy (a virus scanner or the search indexer reading it),
/// with the fence checked again before each try.
fn rename(from: &Path, to: &Path, fence: &impl Fn() -> Result<(), String>) -> Result<(), String> {
    let mut tries = 0;
    loop {
        fence()?;
        pause("lock checked");
        match std::fs::rename(from, to) {
            Err(e) if cfg!(windows) && tries < 40 && !matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::AlreadyExists) => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(25));
            }
            r => return r.map_err(|e| e.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// Older never overwrites newer (spec #178, UP5–UP10): every Library file
// records the program version that last wrote it, and a program never writes
// over a file a newer one wrote.
// ---------------------------------------------------------------------------

/// UP5: the field a Library file records the program version that last
/// wrote it in, first in the file. A file written before versions were
/// recorded has none and counts as old.
pub const VERSION_FIELD: &str = "program_version";

/// UP6: a version as numbers ("0.10.0" → 0, 10, 0), anything after a `-` or
/// `+` left out. None when it isn't numbers.
fn version_numbers(v: &str) -> Option<Vec<u64>> {
    let core = v.trim().split(['-', '+']).next()?;
    core.split('.').map(|p| p.parse::<u64>().ok()).collect()
}

/// UP6: `a` is newer than `b`, part by part as numbers, a missing part
/// counting as 0 (0.10.0 is newer than 0.9.0, and 0.5 is 0.5.0).
fn is_newer(a: &[u64], b: &[u64]) -> bool {
    let n = a.len().max(b.len());
    let at = |v: &[u64], i: usize| v.get(i).copied().unwrap_or(0);
    (0..n).map(|i| at(a, i).cmp(&at(b, i))).find(|o| o.is_ne()) == Some(std::cmp::Ordering::Greater)
}

/// UP6: how version `a` stands to `b` (0.10.0 is newer than 0.9.0), as
/// joining a shop compares this PC's with the shop's (SH12); None when
/// either isn't numbers.
pub fn version_order(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    let (a, b) = (version_numbers(a)?, version_numbers(b)?);
    Some(if is_newer(&a, &b) {
        std::cmp::Ordering::Greater
    } else if is_newer(&b, &a) {
        std::cmp::Ordering::Less
    } else {
        std::cmp::Ordering::Equal
    })
}

/// The program versions a Library file is judged by (UP7): this program's,
/// and, in a shop, the shop's version and the releases it has rolled back
/// from. The window and the agents give the engine their own (a test plays
/// a PC on another version).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Versions {
    /// This program's version.
    pub this: String,
    /// The shop's version, when the Library is a shop that records one.
    pub shop: Option<String>,
    /// UP10: the releases the shop has rolled back from. What they wrote
    /// counts as the shop version's, so it can be saved again. Roll back
    /// fills this list; until then it is empty.
    pub rolled_back_from: Vec<String>,
    /// UP21: the shop version, when this PC runs a release newer than it
    /// and its Library isn't a trial copy: every file in the shop folder
    /// (`shop_dir`) is read-only on this PC until it goes back to that
    /// version (when its program closes), so nothing it saves there is
    /// unreadable to the other PCs. Files elsewhere stay its own.
    pub back_to: Option<String>,
    /// The shop folder `back_to` covers.
    pub shop_dir: Option<PathBuf>,
    /// UP16: the real shop folder, when this PC's Library is a trial copy of
    /// it: nothing the trial saves may land in it.
    pub trial_of: Option<PathBuf>,
}

/// A file a newer program wrote (UP7), and what to say about it (UP8).
#[derive(Clone, Debug, PartialEq)]
pub struct Newer {
    /// The version the file records.
    pub written: String,
    /// The shop's version, when it is the one to update to.
    pub shop: Option<String>,
    /// UP21: this PC runs a release newer than the shop version (this PC's
    /// version, the shop's): read-only until it goes back.
    pub back_to: Option<(String, String)>,
}

impl Versions {
    /// This program's version alone, outside a shop.
    pub fn this(v: &str) -> Versions {
        Versions { this: v.to_string(), ..Default::default() }
    }

    /// UP6, UP7: the file recorded as written by `written` is newer than this
    /// program, so it is read-only here. A file with no version is old
    /// (UP5); one whose version isn't numbers counts as newer, as it can't be
    /// told older. A file a release the shop rolled back from wrote counts
    /// as the shop version's (UP10).
    pub fn newer(&self, written: Option<&str>) -> Option<Newer> {
        if let Some(n) = self.ahead() {
            return Some(Newer { written: written.unwrap_or_default().trim().to_string(), ..n });
        }
        let w = written.map(str::trim).filter(|w| !w.is_empty())?;
        let this = version_numbers(&self.this).unwrap_or_default();
        let shop = self.shop.as_deref().and_then(version_numbers);
        let Some(file) = version_numbers(w) else {
            return Some(Newer { written: w.to_string(), shop: None, back_to: None });
        };
        let counts_as = match &shop {
            Some(s) if self.rolled_back_from.iter().filter_map(|r| version_numbers(r)).any(|r| !is_newer(&r, &file) && !is_newer(&file, &r)) => s.clone(),
            _ => file.clone(),
        };
        if !is_newer(&counts_as, &this) {
            return None;
        }
        // UP8: the shop's version is named when updating to it is enough.
        let shop = shop.filter(|s| !is_newer(&file, s)).and(self.shop.clone());
        Some(Newer { written: w.to_string(), shop, back_to: None })
    }

    /// UP21: these versions as they judge the file at `path`: a PC newer
    /// than the shop version finds only the files in the shop folder
    /// read-only, not those elsewhere.
    pub fn at(&self, path: &Path) -> Versions {
        let mut v = self.clone();
        if !v.shop_dir.as_deref().is_some_and(|d| inside(path, d)) {
            v.back_to = None;
        }
        v
    }

    /// UP21: this PC runs a release newer than the shop version, outside a
    /// trial: every file of the shop is read-only here, whoever wrote it.
    pub fn ahead(&self) -> Option<Newer> {
        let s = self.back_to.as_ref()?;
        Some(Newer { written: String::new(), shop: Some(s.clone()), back_to: Some((self.this.clone(), s.clone())) })
    }
}

impl Newer {
    /// UP8: "Made by version 0.5.0. Update this PC: the shop is on 0.5.0."
    /// Outside a shop (or when the shop's version is older than the file's)
    /// only the file's version is named.
    pub fn message(&self) -> String {
        if let Some((this, shop)) = &self.back_to {
            return format!("This PC runs {this}, newer than the shop version {shop}. Restart to go back to {shop}.");
        }
        let w = &self.written;
        if version_numbers(w).is_none() {
            return format!("Made by version {w}, which this PC can't read. Update this PC.");
        }
        match &self.shop {
            Some(s) => format!("Made by version {w}. Update this PC: the shop is on {s}."),
            None => format!("Made by version {w}. Update this PC to {w} or later."),
        }
    }

    /// UP7: a save over the file `what` names, refused.
    pub fn refusal(&self, what: &str) -> String {
        format!("{what} is read-only on this PC. {} Nothing was saved.", self.message())
    }
}

/// UP16: may a PC whose Library is a trial copy of the shop in `shop`
/// write `what` at `path`? Never inside the real shop folder.
pub fn trial_may_write(path: &Path, shop: &Path, what: &str) -> Result<(), String> {
    if inside(path, shop) {
        return Err(format!("{what} would go in the shop folder {}, and this PC is trying a release on a copy of the shop: nothing it saves reaches the shop. Save it elsewhere, or Stop trying first. Nothing was saved.", shop.display()));
    }
    Ok(())
}

/// `path` is `dir` or inside it, as the file system names them (links and
/// `..` resolved for the part of `path` that is there; without case on
/// Windows).
pub fn inside(path: &Path, dir: &Path) -> bool {
    fn real(p: &Path) -> PathBuf {
        let p = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
        let mut rest = vec![];
        let mut at = p.as_path();
        loop {
            if let Ok(c) = std::fs::canonicalize(at) {
                return rest.iter().rev().fold(c, |acc: PathBuf, n: &&std::ffi::OsStr| acc.join(n));
            }
            match (at.parent(), at.file_name()) {
                (Some(up), Some(n)) => {
                    rest.push(n);
                    at = up;
                }
                _ => return p.clone(),
            }
        }
    }
    let (p, d) = (real(path), real(dir));
    if cfg!(windows) {
        let low = |x: &Path| PathBuf::from(x.to_string_lossy().to_lowercase());
        low(&p).starts_with(low(&d))
    } else {
        p.starts_with(&d)
    }
}

/// UP5: the version a Library file's text records (None: written before
/// versions were recorded, or not a file of this program's).
pub fn recorded_version(bytes: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct Recorded {
        #[serde(default)]
        program_version: Option<serde_json::Value>,
    }
    let r: Recorded = serde_json::from_slice(bytes).ok()?;
    match r.program_version? {
        serde_json::Value::String(s) => Some(s),
        serde_json::Value::Null => None,
        v => Some(v.to_string()),
    }
}

/// UP5: a file's `program_version` as read into a struct: a version that
/// isn't text (a number) is kept as written, so it is never taken for none.
pub fn version_field<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Option::<serde_json::Value>::deserialize(d)? {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(s)) => Some(s),
        Some(v) => Some(v.to_string()),
    })
}

/// UP5: `value` as a Library file's text, recording `version` as the
/// program that wrote it, first, before the file's own fields.
pub fn stamped<T: Serialize>(value: &T, version: &str) -> Result<String, String> {
    #[derive(Serialize)]
    struct Stamped<'a, T> {
        program_version: &'a str,
        #[serde(flatten)]
        value: &'a T,
    }
    serde_json::to_string_pretty(&Stamped { program_version: version, value }).map_err(|e| e.to_string())
}

/// UP7: may this program write over `path`? Not when the file there was
/// last written by a newer program: refused, naming it (`what`) and saying
/// to update this PC (UP8). A file that isn't there yet may be written; one
/// that can't be read just now is refused, as it can't be told older.
pub fn may_write_over(path: &Path, versions: &Versions, what: &str) -> Result<(), String> {
    // UP16: a trial never writes in the real shop folder.
    if let Some(shop) = versions.trial_of.as_deref() {
        trial_may_write(path, shop, what)?;
    }
    // UP21: a PC newer than the shop version writes nothing in the shop,
    // not even a new file; elsewhere it saves as usual.
    let versions = &versions.at(path);
    if let Some(n) = versions.ahead() {
        return Err(n.refusal(what));
    }
    match std::fs::read(path) {
        Ok(b) => versions.newer(recorded_version(&b).as_deref()).map_or(Ok(()), |n| Err(n.refusal(what))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{what} can't be read before saving over it ({e}). Nothing was saved.")),
    }
}

#[cfg(test)]
mod shared_tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nestpro_lock_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut n: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|f| f.file_name().to_string_lossy().into_owned()).collect();
        n.sort();
        n
    }

    #[test]
    fn a_lock_left_by_a_pc_that_crashed_is_taken_over_but_one_in_use_is_waited_for() {
        use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
        use std::sync::Arc;
        let dir = scratch("abandoned");
        // Abandoned after a short time, but with the full 30 s to write once
        // taken over, so a busy PC never runs out of time while it takes the
        // lock over or lets go (the time to write has tests of its own).
        let short = Duration::from_millis(100);
        let take_dead_lock = || WriteLock::take_for(&dir, short, HOLD_FOR, Duration::from_secs(10));
        // PROG-9 crashed while saving: its lock stays behind.
        std::fs::write(dir.join(LOCK_FILE), r#"{"user":"sam","pc":"PROG-9","process":7,"since":0,"token":"PROG-9-7-0"}"#).unwrap();
        let t = Instant::now();
        let lock = take_dead_lock().unwrap();
        assert!(t.elapsed() >= short, "taken over before it counted as abandoned");
        assert!(std::fs::read_to_string(dir.join(LOCK_FILE)).unwrap().contains(&pc_name()));
        drop(lock);
        // A PC that crashed just as it made the lock, before writing in it.
        std::fs::write(dir.join(LOCK_FILE), "").unwrap();
        let t = Instant::now();
        drop(take_dead_lock().unwrap());
        assert!(t.elapsed() >= short);
        // A PC writing now holds it: another waits for it and never takes it
        // over while it's in use. The holder lets go once the other has found
        // it busy a few times; the other gets it then, and only then.
        let lock = WriteLock::take(&dir).unwrap();
        let let_go = Arc::new(AtomicBool::new(false));
        let (busy, busy_rx) = std::sync::mpsc::channel::<()>();
        let other = {
            let (dir, let_go) = (dir.clone(), let_go.clone());
            std::thread::spawn(move || {
                let (mut times, mut busy) = (0, Some(busy));
                let taken_over = Arc::new(AtomicBool::new(false));
                let saw = taken_over.clone();
                hooks::set(move |s| match s {
                    "lock busy" => {
                        times += 1;
                        if times >= 3 && let Some(b) = busy.take() {
                            let _ = b.send(());
                        }
                    }
                    "take over" => saw.store(true, SeqCst),
                    _ => {}
                });
                let got = WriteLock::take(&dir).map(|l| {
                    let after = let_go.load(SeqCst);
                    drop(l);
                    after
                });
                hooks::clear();
                (got, taken_over.load(SeqCst))
            })
        };
        busy_rx.recv_timeout(Duration::from_secs(60)).expect("the other PC never found the lock busy");
        lock.still_held().unwrap();
        let_go.store(true, SeqCst);
        drop(lock);
        let (got, taken_over) = other.join().unwrap();
        assert!(!taken_over, "a lock in use was taken over");
        assert!(got.unwrap(), "the other PC got the lock before it was let go");
        // Released: no lock file, mark or temporary file is left.
        assert!(names(&dir).is_empty(), "{:?}", names(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_writer_whose_lock_was_taken_over_replaces_nothing() {
        let dir = scratch("taken_over");
        let file = dir.join("index.json");
        std::fs::write(&file, "before").unwrap();
        let lock = WriteLock::take(&dir).unwrap();
        // Another PC has begun to take it over (its mark is there): nothing
        // is replaced either.
        let mine = std::fs::read(dir.join(LOCK_FILE)).unwrap();
        std::fs::write(mark_path(&dir.join(LOCK_FILE), &mine), r#"{"user":"kim","pc":"PROG-2","process":9,"since":0,"token":"PROG-2-9-1"}"#).unwrap();
        let e = write_file(&file, b"after", Some(&lock)).unwrap_err();
        assert!(e.contains("Nothing was saved"), "{e}");
        // This PC stalled; another took the lock over as abandoned.
        std::fs::write(dir.join(LOCK_FILE), r#"{"user":"kim","pc":"PROG-2","process":9,"since":0,"token":"PROG-2-9-1"}"#).unwrap();
        let e = write_file(&file, b"after", Some(&lock)).unwrap_err();
        assert!(e.contains("Nothing was saved"), "{e}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "before");
        drop(lock);
        // The other PC's lock is left alone, and no temporary file stays.
        assert!(std::fs::read_to_string(dir.join(LOCK_FILE)).unwrap().contains("PROG-2"));
        assert_eq!(names(&dir).len(), 3, "{:?}", names(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_change_made_inside_another_on_the_same_thread_does_not_wait_for_itself() {
        let dir = scratch("reentrant");
        let outer = WriteLock::take(&dir).unwrap();
        let t = Instant::now();
        let inner = WriteLock::take_with(&dir, Duration::from_secs(5), Duration::from_secs(1)).unwrap();
        assert!(t.elapsed() < Duration::from_millis(100));
        drop(inner);
        outer.still_held().unwrap();
        drop(outer);
        assert!(!dir.join(LOCK_FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB2: a PC that stopped halfway through taking over a crashed PC's
    /// lock leaves its mark; once that has stayed unchanged for the abandon
    /// time, the next PC takes over in its turn, and every mark is cleared.
    #[test]
    fn a_pc_that_stopped_halfway_through_a_takeover_is_taken_over_in_turn() {
        let dir = scratch("halfway");
        let lock = dir.join(LOCK_FILE);
        let dead = r#"{"user":"sam","pc":"PROG-9","process":7,"since":0,"token":"PROG-9-7-0"}"#;
        std::fs::write(&lock, dead).unwrap();
        // PROG-8 made its mark on PROG-9's lock, then crashed.
        std::fs::write(mark_path(&lock, dead.as_bytes()), r#"{"user":"kim","pc":"PROG-8","process":8,"since":0,"token":"PROG-8-8-0"}"#).unwrap();
        // Abandoned after a short time, with the full 30 s to write once
        // taken over, so a busy PC never runs out of it here.
        let short = Duration::from_millis(100);
        let t = Instant::now();
        let held = WriteLock::take_for(&dir, short, HOLD_FOR, Duration::from_secs(5)).unwrap();
        assert!(t.elapsed() >= short);
        let text = std::fs::read_to_string(&lock).unwrap();
        assert!(text.contains(&pc_name()) && !text.contains("PROG-9"), "{text}");
        held.still_held().unwrap();
        assert_eq!(names(&dir), vec![LOCK_FILE.to_string()], "marks left behind");
        drop(held);
        assert!(names(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB2's bound. A writer replaces files only for a third of the abandon
    /// time after it took the lock, and another PC takes a lock over only
    /// after seeing it unchanged for the whole abandon time. So a rename this
    /// writer sends after its last check lands before any takeover unless it
    /// is held up for longer than the two thirds between.
    #[test]
    fn a_writer_replaces_files_only_inside_its_time_and_a_takeover_waits_for_the_rest() {
        // Abandoned after 1.8 s: a writer replaces files for 0.6 s.
        let abandoned = Duration::from_millis(1800);
        let dir = scratch("bound");
        let file = dir.join("index.json");
        std::fs::write(&file, "before").unwrap();
        // PROG-1 took the lock, then stalled before it wrote: past its time,
        // it replaces nothing and leaves its lock to be taken over.
        let lock = WriteLock::take_with(&dir, abandoned, Duration::from_secs(10)).unwrap();
        std::thread::sleep(Duration::from_millis(700));
        let e = write_file(&file, b"PROG-1, late", Some(&lock)).unwrap_err();
        assert!(e.contains("took longer than 600 ms") && e.contains("Nothing was saved"), "{e}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "before");
        drop(lock);
        assert!(dir.join(LOCK_FILE).exists(), "a lock past its time was removed");
        std::fs::remove_file(dir.join(LOCK_FILE)).unwrap();

        // PROG-1 takes the lock and writes in time, but its rename is held up
        // after its last check for 1.0 s, under the 1.2 s between. PROG-2
        // waits meanwhile and takes the lock over (PROG-1 is past its time
        // when it lets go), then saves.
        let lock = WriteLock::take_with(&dir, abandoned, Duration::from_secs(10)).unwrap();
        let t0 = Instant::now();
        let prog2 = {
            let (dir, file) = (dir.clone(), file.clone());
            std::thread::spawn(move || {
                let l = WriteLock::take_with(&dir, abandoned, Duration::from_secs(10)).unwrap();
                let got = t0.elapsed();
                write_file(&file, b"PROG-2", Some(&l)).unwrap();
                got
            })
        };
        hooks::set(|s| {
            if s == "lock checked" {
                std::thread::sleep(Duration::from_millis(1000));
            }
        });
        let r = write_file(&file, b"PROG-1", Some(&lock));
        hooks::clear();
        r.unwrap();
        drop(lock);
        let got = prog2.join().unwrap();
        assert!(got >= abandoned, "PROG-2 took the lock over after {got:?}, before it had stayed unchanged for {abandoned:?}");
        // PROG-1's held-up rename landed first; PROG-2's save is the one kept.
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "PROG-2");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB2: the time is checked again after reading the lock, not only
    /// before. A writer inside its time whose read of the lock is held up
    /// past it replaces nothing, so a slow read can't push its rename past
    /// another PC's takeover.
    #[test]
    fn a_writer_held_up_while_checking_its_lock_replaces_nothing() {
        let abandoned = Duration::from_millis(1800);
        let dir = scratch("slow_check");
        let file = dir.join("index.json");
        std::fs::write(&file, "before").unwrap();
        let lock = WriteLock::take_with(&dir, abandoned, Duration::from_secs(10)).unwrap();
        hooks::set(|s| {
            if s == "lock read" {
                std::thread::sleep(Duration::from_millis(700));
            }
        });
        let r = write_file(&file, b"PROG-1, late", Some(&lock));
        hooks::clear();
        let e = r.unwrap_err();
        assert!(e.contains("took longer than 600 ms") && e.contains("Nothing was saved"), "{e}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "before");
        drop(lock);
        let _ = std::fs::remove_file(dir.join(LOCK_FILE));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB2: two waiting PCs judge one crashed PC's lock abandoned at the
    /// same moment and both take it over, the second one stopped at each step
    /// of its takeover in turn while the first one goes ahead. However their
    /// steps interleave, only one of them holds the lock at any moment, and
    /// both get it in the end.
    #[test]
    fn two_pcs_taking_over_one_abandoned_lock_never_both_hold_it() {
        use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
        use std::sync::mpsc::channel;
        use std::sync::Arc;
        for (n, step) in ["take over", "take over: marked", "take over: checked"].into_iter().enumerate() {
            let dir = scratch(&format!("double_takeover_{n}"));
            // PROG-9 crashed while saving: its lock stays behind.
            std::fs::write(dir.join(LOCK_FILE), r#"{"user":"sam","pc":"PROG-9","process":7,"since":0,"token":"PROG-9-7-0"}"#).unwrap();
            let abandoned = Duration::from_millis(600);
            let (holding, both) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
            let (pc2_there, pc2_there_rx) = channel::<()>();
            let (pc1_holds, pc1_holds_rx) = channel::<()>();
            // Hold the lock a moment, counting how often two PCs held it at once.
            let hold = |lock: WriteLock, holding: &AtomicUsize, both: &AtomicUsize, told: Option<std::sync::mpsc::Sender<()>>| {
                if holding.fetch_add(1, SeqCst) > 0 {
                    both.fetch_add(1, SeqCst);
                }
                if let Some(t) = told {
                    let _ = t.send(());
                }
                std::thread::sleep(Duration::from_millis(150));
                holding.fetch_sub(1, SeqCst);
                drop(lock);
            };
            // PROG-1 takes it over once PROG-2 has reached its step.
            let pc1 = {
                let (dir, holding, both) = (dir.clone(), holding.clone(), both.clone());
                std::thread::spawn(move || {
                    let mut first = true;
                    hooks::set(move |s| {
                        if s == "take over" && std::mem::take(&mut first) {
                            let _ = pc2_there_rx.recv_timeout(Duration::from_secs(3));
                        }
                    });
                    let lock = WriteLock::take_with(&dir, abandoned, Duration::from_secs(20));
                    hooks::clear();
                    hold(lock.unwrap(), &holding, &both, Some(pc1_holds));
                })
            };
            // PROG-2 stops at `step` until PROG-1 holds the lock (or a while).
            let pc2 = {
                let (dir, holding, both) = (dir.clone(), holding.clone(), both.clone());
                std::thread::spawn(move || {
                    let mut first = true;
                    hooks::set(move |s| {
                        if s == step && std::mem::take(&mut first) {
                            let _ = pc2_there.send(());
                            let _ = pc1_holds_rx.recv_timeout(Duration::from_secs(3));
                        }
                    });
                    let lock = WriteLock::take_with(&dir, abandoned, Duration::from_secs(20));
                    hooks::clear();
                    hold(lock.unwrap(), &holding, &both, None);
                })
            };
            pc1.join().unwrap();
            pc2.join().unwrap();
            assert_eq!(both.load(SeqCst), 0, "with PROG-2 stopped at '{step}', PROG-1 and PROG-2 both held the write lock at once");
            let left: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|f| f.file_name().to_string_lossy().into_owned()).filter(|f| f.ends_with(".taken")).collect();
            assert!(left.is_empty(), "takeover marks left behind: {left:?}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// BK3: a long job that renews the lock keeps it far past the abandon
    /// time: a PC waiting for it never takes it over, and gets it as soon as
    /// the job lets go. The job can still write all the while.
    #[test]
    fn a_renewed_lock_is_never_taken_over_while_its_job_runs() {
        let dir = scratch("renew");
        // A waiting PC takes a lock over once it has stayed unchanged 6 s.
        // The job renews every 2 s (a third of its time to write, here as
        // long as the abandon time), so a slow, loaded machine whose share
        // stalls for up to 4 s still keeps it; it runs one and a half times
        // the abandon time.
        let short = Duration::from_secs(6);
        let mut lock = WriteLock::take_holding(&dir, short, short, Duration::from_secs(30)).unwrap();
        let let_go = std::sync::Arc::new(std::sync::Mutex::new(None::<Instant>));
        let waiter = {
            let (dir, let_go) = (dir.clone(), let_go.clone());
            std::thread::spawn(move || {
                let got = WriteLock::take_with(&dir, short, Duration::from_secs(30)).unwrap();
                let at = Instant::now();
                drop(got);
                (at, *let_go.lock().unwrap())
            })
        };
        let start = Instant::now();
        while start.elapsed() < short * 3 / 2 {
            std::thread::sleep(Duration::from_millis(100));
            lock.renew_if_due().unwrap();
        }
        // Still this job's to write with, long after the abandon time.
        lock.still_held().unwrap();
        write_file(&dir.join("index.json"), b"{}", Some(&lock)).unwrap();
        *let_go.lock().unwrap() = Some(Instant::now());
        drop(lock);
        let (got_at, let_go_at) = waiter.join().unwrap();
        assert!(got_at >= let_go_at.unwrap(), "the waiting PC took the lock over while it was being renewed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod version_tests {
    use super::*;

    fn on(this: &str, shop: Option<&str>) -> Versions {
        Versions { this: this.into(), shop: shop.map(String::from), ..Default::default() }
    }

    #[test]
    fn versions_compare_as_numbers_not_text() {
        // UP6: 0.10.0 is newer than 0.9.0, though "0.10.0" < "0.9.0" as text.
        assert!(on("0.9.0", None).newer(Some("0.10.0")).is_some());
        assert!(on("0.10.0", None).newer(Some("0.9.0")).is_none());
        // The same version is never newer; nor an older one.
        assert!(on("0.5.0", None).newer(Some("0.5.0")).is_none());
        assert!(on("0.5.0", None).newer(Some("0.4.9")).is_none());
        assert!(on("1.0.0", None).newer(Some("0.99.99")).is_none());
        // A missing part counts as 0; a suffix after the numbers is left out.
        assert!(on("0.5.0", None).newer(Some("0.5")).is_none());
        assert!(on("0.5.0", None).newer(Some("0.5.0-rc1")).is_none());
        assert!(on("0.5.0", None).newer(Some("0.5.1+build7")).is_some());
    }

    #[test]
    fn a_file_with_no_version_is_old_and_one_that_cant_be_read_is_never_saved_over() {
        // UP5: written before versions were recorded.
        assert!(on("0.1.0", None).newer(None).is_none());
        assert!(on("0.1.0", None).newer(Some("")).is_none());
        // UP6: a version that isn't numbers can't be told older, so it counts
        // as newer and is never written over.
        let n = on("0.4.0", None).newer(Some("next")).expect("an unreadable version was taken as older");
        assert_eq!(n.message(), "Made by version next, which this PC can't read. Update this PC.");
        assert!(on("0.4.0", None).newer(Some("0..1")).is_some());
    }

    #[test]
    fn the_message_names_the_shops_version_in_a_shop_and_only_the_files_outside() {
        // UP8.
        assert_eq!(on("0.4.0", Some("0.5.0")).newer(Some("0.5.0")).unwrap().message(), "Made by version 0.5.0. Update this PC: the shop is on 0.5.0.");
        assert_eq!(on("0.4.0", None).newer(Some("0.5.0")).unwrap().message(), "Made by version 0.5.0. Update this PC to 0.5.0 or later.");
        // A shop on a version older than the file's: updating to the shop's
        // wouldn't be enough, so the file's is named.
        assert_eq!(on("0.4.0", Some("0.4.0")).newer(Some("0.5.0")).unwrap().message(), "Made by version 0.5.0. Update this PC to 0.5.0 or later.");
        assert_eq!(on("0.4.0", Some("0.6.0")).newer(Some("0.5.0")).unwrap().message(), "Made by version 0.5.0. Update this PC: the shop is on 0.6.0.");
        let n = on("0.4.0", Some("0.5.0")).newer(Some("0.5.0")).unwrap();
        assert_eq!(n.refusal("The sheet stock"), "The sheet stock is read-only on this PC. Made by version 0.5.0. Update this PC: the shop is on 0.5.0. Nothing was saved.");
    }

    #[test]
    fn files_written_by_a_release_the_shop_rolled_back_from_count_as_the_shop_versions() {
        // UP10: the seam roll back fills. The shop went back from 0.5.0 to
        // 0.4.0: what 0.5.0 wrote is saved again on 0.4.0, but not on 0.3.0.
        let v = Versions { this: "0.4.0".into(), shop: Some("0.4.0".into()), rolled_back_from: vec!["0.5.0".into()], back_to: None, ..Default::default() };
        assert!(v.newer(Some("0.5.0")).is_none());
        assert!(v.newer(Some("0.6.0")).is_some(), "only the releases rolled back from count");
        let old = Versions { this: "0.3.0".into(), ..v.clone() };
        assert!(old.newer(Some("0.5.0")).is_some());
        // Not in a shop that records a version, nothing counts as another's.
        let none = Versions { shop: None, ..v };
        assert!(none.newer(Some("0.5.0")).is_some());
    }

    /// UP21: a PC on a release newer than the shop version (outside a
    /// trial) saves nothing in the shop, a new file neither, and says to
    /// restart to go back to the shop version.
    #[test]
    fn a_pc_newer_than_the_shop_version_writes_nothing_and_says_to_go_back() {
        let d = std::env::temp_dir().join(format!("nestpro_version_ahead_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("shop")).unwrap();
        let v = Versions { this: "0.5.0".into(), shop: Some("0.4.0".into()), back_to: Some("0.4.0".into()), shop_dir: Some(d.join("shop")), ..Default::default() };
        let n = v.newer(Some("0.4.0")).expect("a file of the shop version was writable on a newer PC");
        assert_eq!(n.message(), "This PC runs 0.5.0, newer than the shop version 0.4.0. Restart to go back to 0.4.0.");
        assert!(v.newer(None).is_some(), "an old file was writable on a newer PC");
        let e = may_write_over(&d.join("shop").join("new.snjob"), &v, "The job file new.snjob").unwrap_err();
        assert_eq!(e, "The job file new.snjob is read-only on this PC. This PC runs 0.5.0, newer than the shop version 0.4.0. Restart to go back to 0.4.0. Nothing was saved.");
        // Outside the shop folder its files are its own (a job on the desktop).
        may_write_over(&d.join("desktop").join("own.snjob"), &v, "The job file own.snjob").unwrap();
        assert!(v.at(&d.join("own.snjob")).newer(Some("0.4.0")).is_none());
        assert!(v.at(&d.join("shop").join("Jobs").join("x.json")).newer(Some("0.4.0")).is_some());
        // Without the mark (a trial copy, or the same version) nothing changes.
        assert!(Versions { back_to: None, ..v }.newer(Some("0.4.0")).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// UP16: a PC on a trial copy writes nothing inside the real shop
    /// folder, a new file neither, whatever the versions; elsewhere it does.
    #[test]
    fn a_trial_writes_nothing_in_the_real_shop_folder() {
        let d = std::env::temp_dir().join(format!("nestpro_version_trial_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("shop").join("Jobs")).unwrap();
        let v = Versions { this: "0.5.0".into(), trial_of: Some(d.join("shop")), ..Default::default() };
        let e = may_write_over(&d.join("shop").join("Jobs").join("..").join("a.snjob"), &v, "The job file a.snjob").unwrap_err();
        assert!(e.contains("this PC is trying a release on a copy of the shop") && e.contains("Nothing was saved."), "{e}");
        may_write_over(&d.join("copy").join("a.snjob"), &v, "The job file a.snjob").unwrap();
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_file_records_the_version_that_wrote_it_first_and_keeps_its_own_fields() {
        // UP5.
        #[derive(Serialize)]
        struct F {
            format: &'static str,
            n: f64,
        }
        let t = stamped(&F { format: "nestpro-demo", n: 0.1 }, "0.5.0").unwrap();
        assert!(t.starts_with("{\n  \"program_version\": \"0.5.0\",\n  \"format\": \"nestpro-demo\""), "{t}");
        assert_eq!(recorded_version(t.as_bytes()).as_deref(), Some("0.5.0"));
        assert_eq!(recorded_version(br#"{"format":"nestpro-demo"}"#), None);
        assert_eq!(recorded_version(br#"{"program_version":5}"#).as_deref(), Some("5"));
    }

    #[test]
    fn a_newer_file_is_never_written_over_and_a_missing_one_is_written() {
        // UP7.
        let d = std::env::temp_dir().join(format!("nestpro_version_guard_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("stock.json");
        let v = on("0.4.0", None);
        may_write_over(&f, &v, "The sheet stock").unwrap();
        std::fs::write(&f, r#"{"program_version":"0.4.0"}"#).unwrap();
        may_write_over(&f, &v, "The sheet stock").unwrap();
        std::fs::write(&f, r#"{"program_version":"0.10.0"}"#).unwrap();
        let e = may_write_over(&f, &v, "The sheet stock").unwrap_err();
        assert_eq!(e, "The sheet stock is read-only on this PC. Made by version 0.10.0. Update this PC to 0.10.0 or later. Nothing was saved.");
        let _ = std::fs::remove_dir_all(&d);
    }
}
