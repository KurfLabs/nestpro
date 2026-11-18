//! Each sheet has its machine (ADR 0004, MS1–MS8): a job's sheets are cut on
//! the job's machine unless one was moved to another machine in Settings →
//! Machines. A sheet's automatic cut plan (cut qualities from its bands,
//! leads, cut order, kerf, cut time) is worked out with its machine's tables,
//! and its program is written by that machine's post into that machine's
//! program folder.

use crate::nest::SheetFormat;
use crate::settings::{AppSettings, Machine};

/// MS1: the machine sheet `f` is cut on. A sheet whose machine is no longer
/// in Settings → Machines is cut on the job's machine (`missing` says so).
pub fn machine_of(f: &SheetFormat, cfg: &AppSettings) -> Machine {
    find(cfg, &f.machine).cloned().unwrap_or_else(|| cfg.machine())
}

/// The machine named `name` in Settings → Machines (names are compared
/// without case, as the shop types them).
pub fn find<'a>(cfg: &'a AppSettings, name: &str) -> Option<&'a Machine> {
    if name.is_empty() {
        return None;
    }
    cfg.machines.iter().find(|m| m.name == name).or_else(|| cfg.machines.iter().find(|m| m.name.eq_ignore_ascii_case(name)))
}

/// MS1: a sheet set to a machine that isn't in Settings → Machines any more:
/// what to tell the programmer (it is planned and written on the job's).
pub fn missing(f: &SheetFormat, cfg: &AppSettings) -> Option<String> {
    (!f.machine.is_empty() && find(cfg, &f.machine).is_none()).then(|| format!("its machine \"{}\" isn't in Settings → Machines any more, so it's planned and written for the job's machine ({})", f.machine, cfg.machine().name))
}

/// The machines in Settings → Machines, as a refusal lists them.
pub fn list(cfg: &AppSettings) -> String {
    cfg.machines.iter().map(|m| format!("\"{}\"", m.name)).collect::<Vec<_>>().join(", ")
}

/// What a sheet's machine is called wherever sheets are listed (MS5).
pub fn label(f: &SheetFormat, cfg: &AppSettings) -> String {
    machine_of(f, cfg).name
}

// ---------------------------------------------------------------------------
// The machine's tables while a sheet is worked out.
// ---------------------------------------------------------------------------

/// A machine's technology and post values, read once per machine set-up.
#[derive(Clone)]
struct Tables {
    id: u64,
    tech: crate::ct1::MachineTech,
    adi: crate::blm::AdiParams,
}

/// Machines read so far: (the machine as set up + the install folder, its
/// tables). Settings → Apply empties it (`forget`).
static READ: std::sync::Mutex<Vec<(String, Tables)>> = std::sync::Mutex::new(Vec::new());
static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn tables(m: &Machine, lantek_dir: &str) -> Tables {
    let key = format!("{m:?}|{lantek_dir}");
    if let Some(t) = READ.lock().ok().and_then(|g| g.iter().find(|(k, _)| *k == key).map(|(_, t)| t.clone())) {
        return t;
    }
    let t = Tables { id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst), tech: crate::ct1::MachineTech::for_machine(m, lantek_dir), adi: crate::blm::AdiParams::from_files(&m.blm_ps1, &m.blm_cf1) };
    if let Ok(mut g) = READ.lock() {
        g.retain(|(k, _)| *k != key);
        g.push((key, t.clone()));
    }
    t
}

/// Settings → Apply: machine files may have changed, read them again.
pub fn forget() {
    if let Ok(mut g) = READ.lock() {
        g.clear();
    }
}

/// MS3: work `f` out on sheet `sheet`'s machine: its technology (bands,
/// leads, cut order, kerf, speeds) and its post values. A sheet on the job's
/// machine (or on one no longer set up) uses the active machine as it is.
pub fn on_sheet<R>(sheet: &SheetFormat, cfg: &AppSettings, f: impl FnOnce() -> R) -> R {
    match find(cfg, &sheet.machine) {
        Some(m) if m.name != cfg.machine().name => {
            let t = tables(m, &cfg.lantek_dir);
            crate::ct1::with_sheet_machine(Some((t.id, t.tech)), || crate::blm::with_sheet_adi(Some(t.adi), f))
        }
        _ => f(),
    }
}

// ---------------------------------------------------------------------------
// Moving a sheet, and Nest all.
// ---------------------------------------------------------------------------

/// MS2: move sheet `k` (from 0) to the machine named `name`. Its cut
/// qualities, leads, cut order and kerf are worked out again from that
/// machine's tables the next time it is planned; what was set by hand (a
/// part's cut quality, a hand cut order, hand-placed leads) stays.
pub fn move_sheet(formats: &mut Vec<SheetFormat>, k: usize, name: &str, cfg: &AppSettings) -> Result<String, String> {
    let Some(m) = find(cfg, name) else {
        return Err(format!("There's no machine \"{name}\" in Settings → Machines. The machines are: {}.", list(cfg)));
    };
    let Some(f) = formats.get_mut(k) else {
        return Err(format!("There's no sheet {}.", k + 1));
    };
    let before = label(f, cfg);
    let job = cfg.machine().name;
    // The job's machine is stored as none, so the sheet follows the job.
    f.machine = if m.name == job { String::new() } else { m.name.clone() };
    if before == m.name {
        return Ok(format!("Sheet {} is already on {}.", k + 1, m.name));
    }
    Ok(format!("Sheet {} moved from {before} to {}: its automatic cut qualities, leads, cut order and kerf are now that machine's; cut qualities, a cut order and leads set by hand stay.", k + 1, m.name))
}

/// MS4: how many of `formats` are on another machine than the job's; Nest
/// all makes fresh sheets on the job's machine, so it moves them back.
pub fn moved(formats: &[SheetFormat], cfg: &AppSettings) -> usize {
    let job = cfg.machine().name;
    formats.iter().filter(|f| !f.machine.is_empty() && label(f, cfg) != job).count()
}

/// MS4: what Nest all adds to its message when it moved `n` sheets back.
pub fn moved_back_note(n: usize, cfg: &AppSettings) -> String {
    match n {
        0 => String::new(),
        1 => format!(" 1 sheet that was moved to another machine is back on the job's machine ({}); move it again if it should go there.", cfg.machine().name),
        n => format!(" {n} sheets that were moved to other machines are back on the job's machine ({}); move them again if they should go there.", cfg.machine().name),
    }
}

/// MS7: every machine's program folder (`shared` for those without one of
/// their own), each once: an export may write or clear any of them.
pub fn program_folders(cfg: &AppSettings, shared: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut v: Vec<std::path::PathBuf> = vec![];
    for m in &cfg.machines {
        let d = program_base(m, shared);
        if !v.contains(&d) {
            v.push(d);
        }
    }
    v
}

/// MS7: the folder a machine's programs go to: its own program folder, else
/// the shared export folder.
pub fn program_base(m: &Machine, shared: &std::path::Path) -> std::path::PathBuf {
    if m.program_dir.trim().is_empty() {
        shared.to_path_buf()
    } else {
        std::path::PathBuf::from(m.program_dir.trim())
    }
}
