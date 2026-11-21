//! Folders of the tests' own, in the system's temporary folder (on Linux
//! often a RAM disk with a limit per user, so test runs must not fill it).
//!
//! Every folder a test run makes is in one folder of the run's own,
//! `nestpro-tests/run-<process id>`:
//! - a test's folder ([`TestDir`]) is deleted when the test passes; a failed
//!   test's folder is kept to look at, and its path printed;
//! - the run's folder (a failed test's, and what the app saves in a test build:
//!   settings, the default Library) is deleted by the next test run. The run
//!   holds a lock on it while it lasts, so a run still going is never touched,
//!   and one that was stopped half-way is cleaned up all the same.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The lock file in each run's folder: held for as long as the run lasts.
const LOCK: &str = ".lock";

/// This test run's own folder, made (and the ones of finished runs deleted)
/// the first time it is asked for.
pub fn run_dir() -> &'static Path {
    static RUN: OnceLock<(PathBuf, File)> = OnceLock::new();
    &RUN.get_or_init(|| {
        let base = std::env::temp_dir().join("nestpro-tests");
        sweep(&base);
        let run = base.join(format!("run-{}", std::process::id()));
        // Left by a finished run that had this process number.
        remove(&run);
        std::fs::create_dir_all(&run).unwrap_or_else(|e| panic!("can't make the test run's folder {}: {e}", run.display()));
        let lock = File::create(run.join(LOCK)).unwrap();
        lock.lock().unwrap();
        (run, lock)
    })
    .0
}

/// Deletes the folders of runs that are over: their lock is free. A folder
/// with no lock file yet may be a run starting now; it is left unless it is
/// over an hour old.
fn sweep(base: &Path) {
    let Ok(runs) = std::fs::read_dir(base) else { return };
    for run in runs.flatten().map(|e| e.path()) {
        let over = match File::open(run.join(LOCK)) {
            Ok(f) => f.try_lock().is_ok(),
            Err(_) => std::fs::metadata(&run).and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().is_ok_and(|age| age.as_secs() > 3600)),
        };
        // The lock is let go of before deleting: Windows won't delete an open file.
        if over {
            remove(&run);
        }
    }
}

/// Deletes a file or folder and everything in it, read-only ones included
/// (some tests make folders read-only to check what a failed save does).
fn remove(p: &Path) {
    if std::fs::remove_dir_all(p).is_ok() || std::fs::remove_file(p).is_ok() || std::fs::symlink_metadata(p).is_err() {
        return;
    }
    writable(p);
    if std::fs::remove_dir_all(p).is_err() {
        let _ = std::fs::remove_file(p);
    }
}

fn writable(p: &Path) {
    let Ok(meta) = std::fs::symlink_metadata(p) else { return };
    if meta.file_type().is_symlink() {
        return;
    }
    let mut perms = meta.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(perms.mode() | 0o700);
    }
    #[cfg(not(unix))]
    perms.set_readonly(false);
    let _ = std::fs::set_permissions(p, perms);
    if meta.is_dir()
        && let Ok(entries) = std::fs::read_dir(p)
    {
        for e in entries.flatten() {
            writable(&e.path());
        }
    }
}

/// A folder of one test's own: `name` in the run's folder, with nothing there
/// yet (not made: the test makes it, or checks what happens when it's
/// missing). Deleted when the test passes, kept when it fails.
#[cfg(test)]
pub struct TestDir(PathBuf);

#[cfg(test)]
impl TestDir {
    pub fn new(name: &str) -> TestDir {
        let p = run_dir().join(name);
        remove(&p);
        TestDir(p)
    }

    /// The same, made (empty).
    pub fn made(name: &str) -> TestDir {
        let d = TestDir::new(name);
        std::fs::create_dir_all(&d.0).unwrap();
        d
    }
}

#[cfg(test)]
impl std::ops::Deref for TestDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

#[cfg(test)]
impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

#[cfg(test)]
impl Drop for TestDir {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("kept the failed test's folder until the next test run: {}", self.0.display());
        } else {
            remove(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_folder_is_deleted_when_the_test_ends_read_only_parts_too() {
        let p = {
            let d = TestDir::made("testdir_read_only");
            let ro = d.join("Shop");
            std::fs::create_dir_all(&ro).unwrap();
            std::fs::write(ro.join("PC-A.json"), "{}").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(ro.join("PC-A.json"), std::fs::Permissions::from_mode(0o000)).unwrap();
                std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).unwrap();
            }
            d.to_path_buf()
        };
        assert!(!p.exists(), "{} is still there", p.display());
        assert!(p.starts_with(run_dir()) && run_dir().starts_with(std::env::temp_dir()));
    }

    #[test]
    fn a_new_test_folder_starts_empty_and_unmade() {
        std::fs::create_dir_all(run_dir().join("testdir_left").join("old")).unwrap();
        let d = TestDir::new("testdir_left");
        assert!(!d.exists());
    }

    #[test]
    fn a_finished_runs_folder_is_deleted_and_a_running_ones_kept() {
        let base = TestDir::made("testdir_sweep");
        // Over: its lock file is there, held by no one.
        let over = base.join("run-1");
        std::fs::create_dir_all(over.join("settings")).unwrap();
        File::create(over.join(LOCK)).unwrap();
        // Going: held (by this test, standing in for another run).
        let going = base.join("run-2");
        std::fs::create_dir_all(&going).unwrap();
        let held = File::create(going.join(LOCK)).unwrap();
        held.lock().unwrap();
        // Starting: no lock file yet, made just now.
        let starting = base.join("run-3");
        std::fs::create_dir_all(&starting).unwrap();
        sweep(&base);
        assert!(!over.exists() && going.exists() && starting.exists());
        // This run's own folder is held the same way.
        assert!(File::open(run_dir().join(LOCK)).unwrap().try_lock().is_err());
    }
}
