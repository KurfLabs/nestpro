//! Where the material list is kept, and changing it safely (MA14).
//!
//! The list is one file in the Library, next to the sheet stock
//! (`Sheets\materials.json`), so every programming PC that shares the
//! Library sees the same materials. It is read and written under the
//! Library's write lock (LB1), like the stock; a change is refused while the
//! Library can't be reached. A PC with no Library keeps the same file in its
//! own settings folder. The rules over the list are in `engine::material`.

use crate::engine::material::List;
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

fn store(home: &Home, l: &List, lock: &WriteLock) -> Result<(), String> {
    let text = serde_json::to_string_pretty(l).map_err(|e| e.to_string())?;
    crate::shared::write_file(&home.file(), text.as_bytes(), Some(lock))
}
