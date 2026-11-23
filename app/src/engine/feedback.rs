//! Feedback from the people who use the program (FB1–FB10): a note saved in
//! the Library's Feedback folder, a link that posts the typed text publicly,
//! and the note a crash leaves for the next start. Plain functions: the
//! window's Feedback button and the agents' `feedback` operation call them.

use crate::shared::{Clock, Who};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::cell::Cell;
use std::path::{Path, PathBuf};

/// The Library's folder of notes (FB2).
pub const FOLDER: &str = "Feedback";
/// A note's files inside its own folder (FB2).
pub const NOTE_TXT: &str = "note.txt";
pub const NOTE_JSON: &str = "note.json";
pub const PICTURE: &str = "screen.png";
pub const JOB: &str = "job.snjob";
/// FB7: the last crash, in this PC's settings folder.
pub const CRASH_FILE: &str = "last-crash.json";
/// FB5: the most of the typed text a public link carries.
pub const LINK_TEXT_MAX: usize = 4000;

/// What a note is about (FB1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Problem,
    Idea,
    Question,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Problem, Kind::Idea, Kind::Question];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Problem => "Problem",
            Kind::Idea => "Idea",
            Kind::Question => "Question",
        }
    }

    /// What the window's pick says under each kind.
    pub fn hint(self) -> &'static str {
        match self {
            Kind::Problem => "Something went wrong, or gave the wrong result",
            Kind::Idea => "Something the program should do, or do differently",
            Kind::Question => "How to do something",
        }
    }

    pub fn parse(s: &str) -> Result<Kind, String> {
        match s.trim().to_lowercase().as_str() {
            "problem" | "bug" | "issue" => Ok(Kind::Problem),
            "idea" | "recommendation" | "suggestion" => Ok(Kind::Idea),
            "question" => Ok(Kind::Question),
            other => Err(format!("kind '{other}': problem, idea or question. Nothing was saved.")),
        }
    }
}

/// The last crash on this PC (FB7).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Crash {
    /// Seconds since 1970.
    pub when: u64,
    pub version: String,
    /// The error, in the program's words.
    pub message: String,
    /// Where in the program (source file and line).
    #[serde(default)]
    pub place: String,
}

impl Crash {
    pub fn text(&self, clock: &Clock) -> String {
        let place = if self.place.is_empty() { String::new() } else { format!(" ({})", self.place) };
        format!("The program closed unexpectedly {} (version {}): {}{place}", clock.when(self.when), self.version, self.message)
    }
}

/// A note as it is written (FB2, FB3): what the person typed and the details
/// the program adds by itself.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub kind: Kind,
    pub text: String,
    /// Seconds since 1970.
    pub when: u64,
    pub who: Who,
    pub version: String,
    /// The operating system and processor ("windows x86_64").
    pub system: String,
    /// The screen and ribbon tab in front ("Nesting › Home"); "agent" for
    /// an agent's note.
    #[serde(default)]
    pub screen: String,
    #[serde(default)]
    pub machine: String,
    /// The open job's name (only its name: the job itself is attached only
    /// when asked, FB1).
    #[serde(default)]
    pub job: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crash: Option<Crash>,
    /// The files saved with it (`screen.png`, `job.snjob`).
    #[serde(default)]
    pub attached: Vec<String>,
}

/// This PC's operating system and processor.
pub fn system() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

/// FB3: where a note goes when there is no Library to put it in: the
/// Feedback folder in this PC's Documents\Sheet Nesting.
pub fn own_dir() -> PathBuf {
    if cfg!(test) {
        return crate::testdir::run_dir().join("documents").join(FOLDER);
    }
    crate::library::documents_dir().join(FOLDER)
}

/// Where a note is saved: the Library's Feedback folder (FB2), or this PC's
/// own when the Library can't be used (FB3). Says which.
pub fn target(library_dir: &Path) -> (PathBuf, bool) {
    match crate::library::usable(library_dir) {
        Ok(()) => (library_dir.join(FOLDER), true),
        Err(_) => (own_dir(), false),
    }
}

/// A note saved: its folder, and whether it went into the Library.
#[derive(Clone, Debug, PartialEq)]
pub struct Saved {
    pub folder: PathBuf,
    pub in_library: bool,
}

impl Saved {
    /// One line for the person or agent who sent it.
    pub fn message(&self) -> String {
        if self.in_library {
            format!("Thank you: the note is saved in the Library's Feedback folder ({}). Whoever looks after the program in the shop reads it there.", self.folder.display())
        } else {
            format!("Thank you: the Library can't be reached, so the note is saved on this PC instead ({}). Send it on, or post it on GitHub.", self.folder.display())
        }
    }
}

/// FB2, FB3: save `note` with its attachments in a folder of its own,
/// named by its time, PC and kind ("2026-11-23 14-05 PROG-2 Problem",
/// " (2)" and on when that is taken). Nothing else in the Library is read or
/// changed, so it needs no write lock (FB4). Refused, saving nothing, when
/// the text is empty.
pub fn save(library_dir: &Path, note: &Note, clock: &Clock, picture: Option<&[u8]>, job: Option<&[u8]>) -> Result<Saved, String> {
    if note.text.trim().is_empty() && note.crash.is_none() {
        return Err("Write what happened, or what you'd like, first. Nothing was saved.".into());
    }
    let (dir, in_library) = target(library_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("The note couldn't be saved: this PC can't write in {} ({e}). Nothing was saved.", dir.display()))?;
    let base = format!("{} {} {}", clock.stamp(note.when).replace(':', "-"), crate::shared::file_safe(&note.who.pc), note.kind.label());
    let folder = (1..1000)
        .map(|n| if n == 1 { dir.join(&base) } else { dir.join(format!("{base} ({n})")) })
        // Made, not just looked for: two PCs sending in the same minute
        // each get a folder of their own.
        .find(|p| std::fs::create_dir(p).is_ok())
        .ok_or_else(|| format!("The note couldn't be saved: no new folder could be made in {}. Nothing was saved.", dir.display()))?;
    let mut note = note.clone();
    note.text = note.text.trim().to_string();
    note.attached.clear();
    let write = |name: &str, bytes: &[u8]| std::fs::write(folder.join(name), bytes).map_err(|e| format!("The note couldn't be saved in {} ({e}).", folder.display()));
    let done = (|| {
        if let Some(p) = picture {
            write(PICTURE, p)?;
            note.attached.push(PICTURE.into());
        }
        if let Some(j) = job {
            write(JOB, j)?;
            note.attached.push(JOB.into());
        }
        write(NOTE_TXT, note_text(&note, clock).as_bytes())?;
        // The JSON last: a folder without it was never finished (FB9).
        write(NOTE_JSON, serde_json::to_string_pretty(&note).map_err(|e| e.to_string())?.as_bytes())
    })();
    if let Err(e) = done {
        let _ = std::fs::remove_dir_all(&folder);
        return Err(format!("{e} Nothing was saved."));
    }
    Ok(Saved { folder, in_library })
}

/// The note as plain text (`note.txt`), for whoever opens the folder.
pub fn note_text(n: &Note, clock: &Clock) -> String {
    let mut s = format!("{}\n\n{}\n\n", n.kind.label(), n.text.trim());
    s += &format!("When: {}\n", clock.stamp(n.when));
    s += &format!("Who: {}\n", n.who.label());
    s += &format!("Version: {} ({})\n", n.version, n.system);
    for (k, v) in [("Screen", &n.screen), ("Machine", &n.machine), ("Job", &n.job)] {
        if !v.is_empty() {
            s += &format!("{k}: {v}\n");
        }
    }
    if !n.attached.is_empty() {
        s += &format!("Attached: {}\n", n.attached.join(", "));
    }
    if let Some(c) = &n.crash {
        s += &format!("\n{}\n", c.text(clock));
    }
    s
}

/// FB5: the link that opens a new public issue on GitHub with the typed
/// text, the kind and the version only: never the PC, the user, the
/// machine, the job, a picture or a crash's details. The text is cut at
/// `LINK_TEXT_MAX` characters, and says so.
pub fn github_link(kind: Kind, text: &str, version: &str) -> String {
    let text = text.trim();
    let first = text.lines().next().unwrap_or_default();
    let mut title: String = first.chars().take(70).collect();
    if title.chars().count() < first.chars().count() {
        title.push('…');
    }
    let mut body: String = text.chars().take(LINK_TEXT_MAX).collect();
    if body.chars().count() < text.chars().count() {
        body += "\n\n(cut short here)";
    }
    body += &format!("\n\n---\n{} · version {} · {}\nPosted from the program's Feedback window: nothing else was attached.", kind.label(), version, system());
    format!("{}/issues/new?title={}&body={}", crate::updates::REPO, encode(&format!("{}: {title}", kind.label())), encode(&body))
}

/// Percent-encoding for a link's query (everything but letters, digits
/// and `-._~`).
fn encode(s: &str) -> String {
    let mut o = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            o.push(b as char);
        } else {
            o += &format!("%{b:02X}");
        }
    }
    o
}

/// A note found in a Feedback folder (FB9).
#[derive(Clone, Debug)]
pub struct Found {
    pub folder: PathBuf,
    pub note: Note,
}

/// FB9: the notes in a Feedback folder, newest first. A folder whose
/// `note.json` is missing (still being written) or can't be read is left out.
pub fn list(dir: &Path) -> Vec<Found> {
    let mut out: Vec<Found> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let folder = e.path();
            let note: Note = serde_json::from_slice(&std::fs::read(folder.join(NOTE_JSON)).ok()?).ok()?;
            Some(Found { folder, note })
        })
        .collect();
    out.sort_by(|a, b| b.note.when.cmp(&a.note.when).then_with(|| b.folder.cmp(&a.folder)));
    out
}

/// A found note for agents.
pub fn found_json(f: &Found, clock: &Clock) -> Value {
    let n = &f.note;
    json!({
        "folder": f.folder.display().to_string(),
        "kind": n.kind.label().to_lowercase(),
        "when": clock.stamp(n.when),
        "who": n.who.label(),
        "version": n.version,
        "system": n.system,
        "screen": n.screen,
        "machine": n.machine,
        "job": n.job,
        "text": n.text,
        "crash": n.crash.as_ref().map(|c| c.text(clock)),
        "attached": n.attached,
    })
}

// --- FB7, FB8: the crash note ---

thread_local! {
    /// How many `caught` calls this thread is inside.
    static CAUGHT: Cell<u32> = const { Cell::new(0) };
}

/// Run `f`, catching a panic as the readers of damaged drawing files do
/// (FB8): an error caught here never counts as a crash.
pub fn caught<R>(f: impl FnOnce() -> R) -> std::thread::Result<R> {
    CAUGHT.with(|c| c.set(c.get() + 1));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    CAUGHT.with(|c| c.set(c.get() - 1));
    r
}

/// FB8: whether a panic here closes the window: on the window's own thread,
/// outside `caught`.
pub fn closes_window() -> bool {
    std::thread::current().name() == Some("main") && CAUGHT.with(Cell::get) == 0
}

/// The error's own words from a panic.
pub fn panic_message(info: &std::panic::PanicHookInfo) -> (String, String) {
    let p = info.payload();
    let msg = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "unknown error".into());
    let place = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
    (msg, place)
}

/// FB7: remember a crash in this PC's settings folder `pc_dir` (the last
/// one only).
pub fn record_crash(pc_dir: &Path, crash: &Crash) {
    let _ = std::fs::create_dir_all(pc_dir);
    if let Ok(t) = serde_json::to_string_pretty(crash) {
        let _ = std::fs::write(pc_dir.join(CRASH_FILE), t);
    }
}

/// FB7: the crash this PC remembers, if any.
pub fn last_crash(pc_dir: &Path) -> Option<Crash> {
    serde_json::from_slice(&std::fs::read(pc_dir.join(CRASH_FILE)).ok()?).ok()
}

/// FB7: forget it (sent, or Not now).
pub fn forget_crash(pc_dir: &Path) {
    let _ = std::fs::remove_file(pc_dir.join(CRASH_FILE));
}

/// The agents' `feedback` operation (FB10): send a note, or list them.
pub fn run(ctx: &super::Ctx, a: &super::ops::Args) -> Result<super::ops::Outcome, String> {
    use super::ops::Outcome;
    match a.str("mode").unwrap_or_default().as_str() {
        "send" => {
            let kind = a.str("kind").map_or(Ok(Kind::Problem), |k| Kind::parse(&k))?;
            let text = a.str("text").unwrap_or_default();
            let job = match a.str("attach_job") {
                Some(p) => {
                    let p = PathBuf::from(p);
                    // Only a job file goes in: anything else is refused, naming it.
                    crate::jobfile::load(&p).map_err(|e| format!("attach_job {}: not a job file this program can open ({e}). Nothing was saved.", p.display()))?;
                    Some(std::fs::read(&p).map_err(|e| format!("attach_job {}: {e}. Nothing was saved.", p.display()))?)
                }
                None => None,
            };
            let note = Note {
                kind,
                text: text.clone(),
                when: ctx.clock.now(),
                who: ctx.who.clone(),
                version: ctx.version.clone(),
                system: system(),
                screen: "agent".into(),
                machine: ctx.cfg.machine().name.clone(),
                job: String::new(),
                crash: None,
                attached: vec![],
            };
            let saved = save(&ctx.library_dir, &note, &ctx.clock, None, job.as_deref())?;
            let link = github_link(kind, &text, &ctx.version);
            let mut o = Outcome::read(
                format!("{} To post the text publicly as well, a person opens github_link (only the kind, the text and the version go; anyone can read it).", saved.message()),
                json!({ "folder": saved.folder.display().to_string(), "in_library": saved.in_library, "github_link": link }),
            );
            o.files.push(saved.folder);
            Ok(o)
        }
        "list" => {
            let (dir, in_library) = target(&ctx.library_dir);
            let mut found = list(&dir);
            if let Some(n) = a.int("limit")? {
                if n < 1 {
                    return Err(format!("limit {n}: at least 1"));
                }
                found.truncate(n as usize);
            }
            let place = if in_library { "the Library's Feedback folder" } else { "this PC's Feedback folder (the Library can't be used)" };
            let msg = if found.is_empty() { format!("No notes in {place} ({}).", dir.display()) } else { format!("{} note(s) in {place} ({}), newest first.", found.len(), dir.display()) };
            Ok(Outcome::read(msg, json!({ "folder": dir.display().to_string(), "in_library": in_library, "notes": found.iter().map(|f| found_json(f, &ctx.clock)).collect::<Vec<_>>() })))
        }
        m => Err(format!("mode '{m}': send or list")),
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn a_public_link_carries_only_the_text_kind_and_version() {
        let l = github_link(Kind::Idea, "Nest  all & keep\nthe sheet", "0.2.1");
        assert!(l.starts_with("https://github.com/KurfLabs/nestpro/issues/new?title=Idea%3A%20Nest%20%20all%20%26%20keep&body=Nest%20%20all%20%26%20keep%0Athe%20sheet"), "{l}");
        assert!(l.contains("version%200.2.1"), "{l}");
        let long = "x".repeat(LINK_TEXT_MAX + 50);
        let l = github_link(Kind::Problem, &long, "0.2.1");
        assert!(l.contains("cut%20short"), "a cut text says so");
        assert!(!l.contains(&"x".repeat(LINK_TEXT_MAX + 1)));
    }

    #[test]
    fn kinds_take_their_everyday_names() {
        assert_eq!(Kind::parse("Bug"), Ok(Kind::Problem));
        assert_eq!(Kind::parse("recommendation"), Ok(Kind::Idea));
        assert!(Kind::parse("rant").is_err());
    }

    #[test]
    fn a_caught_panic_never_counts_as_a_crash() {
        std::thread::Builder::new()
            .name("main".into())
            .spawn(|| {
                assert!(closes_window());
                let inside = caught(|| closes_window()).unwrap();
                assert!(!inside);
                assert!(closes_window());
            })
            .unwrap()
            .join()
            .unwrap();
        assert!(std::thread::spawn(closes_window).join().map(|b| !b).unwrap(), "a background thread's panic doesn't close the window");
    }

    #[test]
    fn the_last_crash_is_kept_until_it_is_forgotten() {
        let d = crate::testdir::run_dir().join("crash_note");
        let c = Crash { when: 1_700_000_000, version: "0.2.1".into(), message: "index out of bounds".into(), place: "src/nest.rs:10".into() };
        assert_eq!(last_crash(&d), None);
        record_crash(&d, &c);
        assert_eq!(last_crash(&d), Some(c));
        forget_crash(&d);
        assert_eq!(last_crash(&d), None);
    }
}
