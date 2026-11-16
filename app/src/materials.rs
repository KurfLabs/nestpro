//! Where the material list is kept, and changing it safely (MA14).
//!
//! The list is one file in the Library, next to the sheet stock
//! (`Sheets\materials.json`), so every programming PC that shares the
//! Library sees the same materials. It is read and written under the
//! Library's write lock (LB1), like the stock; a change is refused while the
//! Library can't be reached. A PC with no Library keeps the same file in its
//! own settings folder. The rules over the list are in `engine::material`.

use crate::engine::material::{Id, List, Uses};
use crate::engine::materialedit::{Change, Done};
use crate::shared::{Saved, WriteLock};
use std::path::{Path, PathBuf};

pub const FILE: &str = "materials.json";

/// Where the list lives.
#[derive(Clone, Debug, PartialEq)]
pub enum Home {
    /// In the Library folder (which may be out of reach just now).
    Library(PathBuf),
    /// In this PC's settings folder: it has no Library.
    Pc(PathBuf),
}

impl Home {
    /// MA14: the Library's, unless this PC has no Library: its Library
    /// folder is its own default one and holds no Library (none was made or
    /// picked). A Library folder that was picked but can't be reached stays
    /// the home, so this PC never starts a list of its own beside the
    /// shop's.
    pub fn of(library_dir: &Path, pc_dir: &Path) -> Home {
        if library_dir == crate::library::default_dir() && !crate::library::holds_library(library_dir) {
            Home::Pc(pc_dir.to_path_buf())
        } else {
            Home::Library(library_dir.to_path_buf())
        }
    }

    pub fn file(&self) -> PathBuf {
        match self {
            Home::Library(d) => d.join(crate::stock::FOLDER).join(FILE),
            Home::Pc(d) => d.join(FILE),
        }
    }

    /// The folder whose write lock guards the file.
    fn lock_dir(&self) -> &Path {
        match self {
            Home::Library(d) | Home::Pc(d) => d,
        }
    }

    /// "the Library's" / "this PC's own" (in replies).
    pub fn label(&self) -> &'static str {
        match self {
            Home::Library(_) => "the Library's",
            Home::Pc(_) => "this PC's own (it has no Library)",
        }
    }
}

/// The stored list, or None when none is stored there yet.
pub fn read(file: &Path) -> Result<Option<List>, String> {
    match std::fs::read_to_string(file) {
        Ok(t) => {
            let l: List = serde_json::from_str(&t).map_err(|e| format!("{} isn't a material list ({e})", file.display()))?;
            if l.format != crate::engine::material::FORMAT {
                return Err(format!("{} isn't a material list", file.display()));
            }
            if l.version > crate::engine::material::VERSION {
                return Err(format!("{} was made by a newer version (layout {})", file.display(), l.version));
            }
            Ok(Some(l))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}

/// The list as it is in `home` now: the stored one; before one is stored,
/// this PC's own list when the home is the Library and this PC has one,
/// else the one this PC's settings seed (MA15). Refused when the Library
/// can't be reached or its file can't be read.
pub fn load(home: &Home, pc_dir: &Path, cfg: &crate::settings::AppSettings) -> Result<List, String> {
    if let Home::Library(d) = home {
        crate::library::usable(d)?;
    }
    if let Some(l) = read(&home.file())? {
        return Ok(l);
    }
    if matches!(home, Home::Library(_)) {
        if let Ok(Some(l)) = read(&Home::Pc(pc_dir.to_path_buf()).file()) {
            return Ok(l);
        }
    }
    Ok(List::seed(cfg))
}

/// This PC's own list (MA28): the one stored in its settings folder; else,
/// on a new install, its starter list (MA29). None for settings from before
/// the list, whose three are every list's first three.
pub fn own_list(pc_dir: &Path, cfg: &crate::settings::AppSettings) -> Result<Option<List>, String> {
    if let Some(l) = read(&Home::Pc(pc_dir.to_path_buf()).file())? {
        return Ok(Some(l));
    }
    Ok(cfg.starter_materials.then(|| List::seed(cfg)))
}

/// Where this PC notes the Libraries its own list has joined (MA28).
pub const JOINED_FILE: &str = "materials-joined.json";

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct JoinedLibraries {
    libraries: Vec<String>,
}

fn joined_file(pc_dir: &Path) -> PathBuf {
    pc_dir.join(JOINED_FILE)
}

fn joined_libraries(pc_dir: &Path) -> JoinedLibraries {
    std::fs::read_to_string(joined_file(pc_dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Whether this PC's own list has joined the Library in `dir` already.
pub fn has_joined(pc_dir: &Path, dir: &Path) -> bool {
    joined_libraries(pc_dir).libraries.iter().any(|l| Path::new(l) == dir)
}

/// What joining a Library did now (MA28).
#[derive(Clone, Debug, PartialEq)]
pub struct JoinedNow {
    /// This PC's material numbers and the Library's for the same material:
    /// a job open here in this PC's numbering takes the Library's.
    pub map: Vec<(Id, Id)>,
    /// The names of this PC's materials added to the Library's list.
    pub added: Vec<String>,
}

impl JoinedNow {
    /// The Library's number for this PC's material `id`.
    pub fn id(&self, id: Id) -> Id {
        self.map.iter().find(|m| m.0 == id).map_or(id, |m| m.1)
    }

    /// What the window and agents say about it.
    pub fn note(&self) -> String {
        if self.added.is_empty() {
            "This PC's materials joined the Library's material list: each was there already.".into()
        } else {
            format!("This PC's materials joined the Library's material list: added {}.", self.added.join(", "))
        }
    }
}

/// MA28: the first time this PC points at the Library of `home`, its own
/// list joins the Library's, under the Library's write lock (invariant 5):
/// the Library's list wins and this PC's materials whose names it doesn't
/// have are added (`List::merge`); a Library with no list yet takes this
/// PC's own as it is. Nothing to do (None) when the home is this PC's own,
/// or it joined this Library before. A PC whose settings are from before
/// the list has no list of its own: its settings' three are the Library's
/// first three, so it has nothing to add to a Library's list; a Library
/// with no list yet takes them, as its settings seed them (MA15), so every
/// PC sharing it answers from the same list. Refused, to be tried again the
/// next time the list is read, while the Library can't be reached; nothing
/// is stored then.
pub fn join(home: &Home, pc_dir: &Path, cfg: &crate::settings::AppSettings, by: impl FnOnce() -> Saved) -> Result<Option<JoinedNow>, String> {
    let Home::Library(d) = home else { return Ok(None) };
    if has_joined(pc_dir, d) {
        return Ok(None);
    }
    let own = own_list(pc_dir, cfg)?;
    // Settings from before the list: only a Library with no list yet needs
    // them (looked at again under the lock).
    if own.is_none() && read(&home.file())?.is_some() {
        return Ok(None);
    }
    let own = own.unwrap_or_else(|| List::seed(cfg));
    crate::library::usable(d).map_err(|e| format!("{e} This PC's materials will join the Library's material list once it can be reached."))?;
    let lock = WriteLock::take(d)?;
    let joined = match read(&home.file())? {
        None => crate::engine::material::Joined { same: own.materials.iter().map(|m| (m.id, m.id)).collect(), added: vec![], list: own.clone() },
        Some(lib) => lib.merge(&own),
    };
    let stored = read(&home.file())?;
    if stored.as_ref() != Some(&joined.list) {
        let mut l = joined.list.clone();
        l.changed = Some(by());
        store(home, &l, &lock)?;
    }
    drop(lock);
    let mut marks = joined_libraries(pc_dir);
    marks.libraries.push(d.to_string_lossy().into_owned());
    let text = serde_json::to_string_pretty(&marks).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(pc_dir).map_err(|e| format!("{}: {e}", pc_dir.display()))?;
    crate::shared::write_file(&joined_file(pc_dir), text.as_bytes(), None)?;
    let added = joined.added.iter().filter_map(|(_, id)| joined.list.get(*id).map(|m| m.name.clone())).collect();
    Ok(Some(JoinedNow { map: joined.same.iter().chain(&joined.added).copied().collect(), added }))
}

/// MA28, MA34: the material of a job open in a window or session once the
/// list was read again, from its material `id` and the name it was read
/// with: `known` says whether the list read before had `id`. One it had
/// keeps its number, or takes the Library's when this PC's list `joined`
/// it just now. One it didn't (unknown, MA30) is found again by its name
/// (`material::find_again`), never by its number alone, which may be
/// another material now.
pub fn after_reload(joined: Option<&JoinedNow>, id: Id, name: &str, known: bool) -> Id {
    if known {
        joined.map_or(id, |n| n.id(id))
    } else {
        crate::engine::material::find_again(id, name)
    }
}

/// The list to answer from when a window or session starts while the list
/// can't be read (the Library is out of reach): this PC's own while it
/// hasn't joined the Library yet (its jobs are still in its numbering).
/// Otherwise only the first three, as this PC's settings seed them (MA15):
/// every other number is the Library's, which can't be read now, so it is
/// unknown (`?`, MA30) and keeps its number and name, never taken as one of
/// this PC's own (MA28).
pub fn fallback(home: &Home, pc_dir: &Path, cfg: &crate::settings::AppSettings) -> List {
    let library = matches!(home, Home::Library(_));
    let joined = matches!(home, Home::Library(d) if has_joined(pc_dir, d));
    if !joined {
        if let Ok(Some(l)) = own_list(pc_dir, cfg) {
            return l;
        }
    }
    let mut l = List::seed(cfg);
    if library {
        l.materials.retain(|m| m.id < crate::engine::material::KEPT);
        l.offline = true;
    }
    l
}

/// Re-read the list, change it and store it, all under the write lock of
/// the folder it lives in (LB1, invariant 5), so two PCs (or the window and
/// an agent) changing it at once both count. Nothing is stored when `f`
/// refuses. Refused, saying what to do, while the Library can't be reached
/// or holds no Library (LB11, LB12). Gives `f`'s answer and the list as
/// stored.
pub fn update<T>(home: &Home, pc_dir: &Path, cfg: &crate::settings::AppSettings, by: impl FnOnce() -> Saved, f: impl FnOnce(&mut List) -> Result<T, String>) -> Result<(T, List), String> {
    let refused = |e: String| format!("{e} The material list wasn't changed: try again once it can be reached.");
    let lock = match home {
        Home::Library(d) => {
            crate::library::usable(d).map_err(refused)?;
            WriteLock::take(d).map_err(refused)?
        }
        Home::Pc(d) => {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
            WriteLock::take(home.lock_dir())?
        }
    };
    let mut l = load(home, pc_dir, cfg).map_err(refused)?;
    let out = f(&mut l)?;
    l.changed = Some(by());
    store(home, &l, &lock)?;
    Ok((out, l))
}

/// A job open in this window or agent session, as what uses a material
/// counts it (JOB-88).
#[derive(Clone, Debug)]
pub struct Open {
    pub name: String,
    pub material: Id,
    /// Its record in the Library, when it is a saved job.
    pub record: Option<u64>,
}

impl From<&crate::jobfile::JobFile> for Open {
    fn from(j: &crate::jobfile::JobFile) -> Open {
        Open { name: j.name.clone(), material: j.spec.material, record: j.library.map(|r| r.id) }
    }
}

/// MA20: what uses material `id`: the saved jobs and library parts of the
/// Library `lib` (read from `scan`), the sheets and remnants of its stock,
/// and the jobs `open` here. A job open here that is also a saved job
/// counts once, as saved.
fn uses_in(lib: &crate::library::Library, scan: &crate::library::RecordScan, id: Id, open: &[Open]) -> Result<Uses, String> {
    let (jobs, parts) = scan.using_material(id);
    let stock = crate::stock::Stock::load(&lib.dir).map_err(|e| format!("The sheet stock can't be read ({e}), so it can't be told whether a stock sheet uses the material: it wasn't deleted."))?;
    Ok(Uses {
        stock: stock.items.iter().filter(|i| i.material == id).map(|i| i.reference.clone()).collect(),
        open: open.iter().filter(|j| j.material == id && !j.record.is_some_and(|r| jobs.iter().any(|x| x.0 == r))).map(|j| j.name.clone()).collect(),
        jobs: jobs.into_iter().map(|x| x.1).collect(),
        parts: parts.into_iter().map(|x| x.1).collect(),
    })
}

/// MA20, looked at now (no lock): what uses material `id`, for the page to
/// refuse a deletion before it is marked. Saving looks again under the lock.
pub fn uses(lib: &crate::library::Library, id: Id, open: &[Open]) -> Result<Uses, String> {
    uses_in(lib, &lib.scan_records(None), id, open)
}

/// MA21: make `changes` to the list in `home` together, under its write
/// lock, all or none (`List::apply`). A deletion is refused while anything
/// uses the material (MA20): a saved job or library part of the Library
/// `lib`, a sheet or remnant of its stock, or one of the jobs `open` in this
/// window or agent session (JOB-88). What uses it is read under the same
/// write lock as the change (invariant 4, 5), after a first read without it
/// so the lock stays short (LB2): a job saved in it at the same moment on
/// another PC either counts or comes after. On a refusal nothing is
/// changed. Gives what each change did and the list as stored.
pub fn apply(home: &Home, pc_dir: &Path, cfg: &crate::settings::AppSettings, by: impl FnOnce() -> Saved, lib: &crate::library::Library, changes: &[Change], open: &[Open]) -> Result<(Vec<Done>, List), String> {
    use crate::engine::material::reading_with;
    // The first three are kept whatever uses them: no need to look.
    let deletes = changes.iter().any(|c| matches!(c, Change::Delete(id) if *id >= crate::engine::material::KEPT));
    // MA35: the files are read against the list as it is stored now, never
    // the one this window or session read last.
    let first = if deletes { load(home, pc_dir, cfg).ok().map(|seen| (reading_with(&seen, || lib.scan_records(None)), seen)) } else { None };
    crate::shared::pause("materials scanned");
    update(home, pc_dir, cfg, by, |l| {
        // What uses a material is read against the list read under the
        // lock; files read first against another list are read again.
        let base = l.clone();
        let known = first.as_ref().filter(|(_, seen)| *seen == base).map(|f| &f.0);
        let mut scan = None;
        l.apply(changes, |id| {
            reading_with(&base, || {
                let s = scan.get_or_insert_with(|| lib.scan_records(known));
                uses_in(lib, s, id, open)
            })
        })
    })
}

/// MA14: whether the list in `home` can be changed now, or why not (what
/// Settings → Materials says above the list): the Library can't be reached
/// or holds no Library. A PC with no Library can always change its own.
pub fn can_change(home: &Home) -> Result<(), String> {
    match home {
        Home::Library(d) => crate::library::usable(d).map_err(|e| cant_change(&e)),
        Home::Pc(_) => Ok(()),
    }
}

/// Why the list can't be changed, from why the Library can't be used.
pub fn cant_change(why: &str) -> String {
    format!("{why} The material list can't be changed until it can be reached: the list shown is the one read last.")
}

/// MA20, invariant 4: a write that gives a job, library part or stock item
/// material `id`, checked under the write lock of the Library in
/// `library_dir` (which the caller holds): refused when the Library's list
/// has deleted the material since this PC picked it, so nothing is saved
/// in a material that is gone. A number the list never gave (a file from
/// another Library) isn't refused: it keeps its number and name (MA17).
pub fn not_deleted(library_dir: &Path, id: Id, what: &str) -> Result<(), String> {
    let deleted = read(&Home::Library(library_dir.to_path_buf()).file())?.is_some_and(|l| l.deleted.contains(&id) && l.get(id).is_none());
    if !deleted {
        return Ok(());
    }
    let which = crate::engine::material::name(id).map_or_else(|| format!("Material {id}"), |n| format!("{n} (material {id})"));
    Err(format!("{which} was deleted from the material list after it was picked, so {what} can't be saved in it: pick another material (see material mode=list), or add it again with material mode=add. Nothing was saved."))
}

fn store(home: &Home, l: &List, lock: &WriteLock) -> Result<(), String> {
    let text = serde_json::to_string_pretty(l).map_err(|e| e.to_string())?;
    crate::shared::write_file(&home.file(), text.as_bytes(), Some(lock))
}
