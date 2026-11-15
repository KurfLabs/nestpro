//! Each sheet has its machine (ADR 0004; MS1–MS8): which machine set-up in
//! Settings → Machines a nested sheet is cut on, so one job can go to two
//! lasers. A sheet stores its machine by name (`SheetFormat::machine`); none
//! means the job's machine, the active one, so a job saved before sheets had
//! a machine opens and writes exactly as before (MS1).
//!
//! The rules read the machine's technology from the active machine
//! (`ct1::active`, `blm::adi`). A sheet on another machine is worked out
//! inside `on_sheet`, which puts that machine's tables in for the call on
//! this thread only (MS4, MS6); the job's machine needs nothing put in.

use super::*;
use std::sync::Arc;

/// One machine of Settings → Machines as the rules use it for a sheet: its
/// technology tables, post library and LC5 post values.
pub struct MachineUse {
    /// Tells the cached section picks of two machines apart.
    pub id: u64,
    pub machine: settings::Machine,
    pub tech: crate::ct1::MachineTech,
    pub post: crate::post::PostConfig,
    pub adi: crate::blm::AdiParams,
}

impl MachineUse {
    /// Machine `m`, its technology record picked with `gas` (number, name)
    /// for a job without a gas of its own (MS5).
    fn load(m: &settings::Machine, gas: (u32, &str), lantek_dir: &str) -> MachineUse {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        MachineUse {
            id: NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            machine: m.clone(),
            tech: crate::ct1::MachineTech::for_machine(&settings::Machine { gas_id: gas.0, gas_name: gas.1.to_string(), ..m.clone() }, lantek_dir),
            post: load_post(m),
            adi: crate::blm::AdiParams::from_files(&m.blm_ps1, &m.blm_cf1),
        }
    }
}

/// Every machine of the settings, loaded, and which one is the job's.
struct Machines {
    /// What they were loaded from: reloaded when it changes.
    key: String,
    job: usize,
    list: Vec<Arc<MachineUse>>,
}

static INSTALLED: std::sync::RwLock<Option<Arc<Machines>>> = std::sync::RwLock::new(None);

#[cfg(test)]
thread_local! {
    /// A test's own machines (tests run side by side).
    static LOCAL: std::cell::RefCell<Option<Arc<Machines>>> = const { std::cell::RefCell::new(None) };
}

fn installed() -> Option<Arc<Machines>> {
    #[cfg(test)]
    return LOCAL.with(|l| l.borrow().clone());
    #[cfg(not(test))]
    INSTALLED.read().ok().and_then(|g| g.clone())
}

fn put(m: Arc<Machines>) {
    #[cfg(test)]
    {
        let _ = &INSTALLED;
        LOCAL.with(|l| *l.borrow_mut() = Some(m));
    }
    #[cfg(not(test))]
    if let Ok(mut g) = INSTALLED.write() {
        *g = Some(m);
    }
}

/// Load the machines of `cfg` for the sheets on them (Settings → Apply, a
/// session's every call). Nothing is read again while they stay the same;
/// true when they were (re)loaded.
pub fn install(cfg: &AppSettings) -> bool {
    let key = format!("{}\n{}\n{:?}", cfg.lantek_dir, cfg.active_machine, cfg.machines);
    if installed().is_some_and(|m| m.key == key) {
        return false;
    }
    // MS5: a job without a gas of its own is cut with the job's machine's
    // gas on every machine, so each machine picks its records for that gas.
    let job = cfg.machine();
    let list = cfg.machines.iter().map(|m| Arc::new(MachineUse::load(m, (job.gas_id, &job.gas_name), &cfg.lantek_dir))).collect();
    put(Arc::new(Machines { key, job: cfg.active_machine.min(cfg.machines.len().saturating_sub(1)), list }));
    true
}

/// MS3, MAC-79: two machine names are the same machine when they differ only
/// in upper and lower case or in spaces (around and between words).
pub fn same_name(a: &str, b: &str) -> bool {
    let n = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    n(a) == n(b)
}

/// The machine of Settings → Machines a name means (MS3), if any.
pub fn find(cfg: &AppSettings, name: &str) -> Option<usize> {
    cfg.machines.iter().position(|m| same_name(&m.name, name))
}

/// Where a sheet is cut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum On {
    /// The job's machine: no machine named, or the job's own name (MS1).
    Job,
    /// Another machine of Settings → Machines, by its index there.
    Other(usize),
    /// A machine that isn't in Settings → Machines (any more): deleted or
    /// renamed since the sheet was moved to it (MS8).
    Unknown(String),
}

/// Where sheet format `f` is cut, with the machines of `cfg` (MS1, MS8).
pub fn sheet_on(cfg: &AppSettings, f: &SheetFormat) -> On {
    if f.machine.trim().is_empty() {
        return On::Job;
    }
    match find(cfg, &f.machine) {
        Some(i) if i == cfg.active_machine => On::Job,
        Some(i) => On::Other(i),
        None => On::Unknown(f.machine.clone()),
    }
}

/// Run `g` with the tables of the machine sheet format `f` is cut on (MS4,
/// MS6): the job's machine (and a machine that isn't in Settings any more,
/// MS8) as it is; another machine's technology and LC5 post values put in
/// for the call, on this thread only.
pub fn on_sheet<R>(f: &SheetFormat, g: impl FnOnce() -> R) -> R {
    match other_use(f) {
        Some(mu) => crate::ct1::with_machine(mu.id, &mu.tech, || crate::blm::with_adi(mu.adi, g)),
        None => g(),
    }
}

/// The machine a sheet format names when it is another one than the job's,
/// loaded (`install`).
fn other_use(f: &SheetFormat) -> Option<Arc<MachineUse>> {
    if f.machine.trim().is_empty() {
        return None;
    }
    let ms = installed()?;
    let i = ms.list.iter().position(|m| same_name(&m.machine.name, &f.machine))?;
    (i != ms.job).then(|| ms.list[i].clone())
}

/// The kerf a sheet of format `f` (`spec` with its size) is cut with: its
/// machine's (MS4; remnants, remnant lines and stock follow it).
pub fn kerf(f: &SheetFormat, spec: &SheetSpec) -> f64 {
    on_sheet(f, || crate::kerf::kerf_mm(spec))
}

/// Machine `m`'s post library: as loaded (`install`), else read now.
fn post_of(m: &settings::Machine) -> crate::post::PostConfig {
    installed().and_then(|ms| ms.list.iter().find(|u| &u.machine == m).map(|u| u.post.clone())).unwrap_or_else(|| load_post(m))
}

/// A sheet's machine as screens and agents show it (MS1, MS8): its name, the
/// job's machine's for a sheet that names none, and a machine that isn't in
/// Settings → Machines any more marked so.
pub fn shown(cfg: &AppSettings, f: &SheetFormat) -> String {
    match sheet_on(cfg, f) {
        On::Job => cfg.machine().name,
        On::Other(i) => cfg.machines[i].name.clone(),
        On::Unknown(n) => format!("{n} (not in Settings → Machines)"),
    }
}

/// The machines of the settings as a refusal lists them, the job's marked.
pub fn machine_list(cfg: &AppSettings) -> String {
    cfg.machines
        .iter()
        .enumerate()
        .map(|(i, m)| if i == cfg.active_machine { format!("{} (the job's)", m.name) } else { m.name.clone() })
        .collect::<Vec<_>>()
        .join(", ")
}

/// How many sheets are on another machine than the job's (MS2: what Nest
/// all moves back), machines no longer in Settings included.
pub fn moved_count(cfg: &AppSettings, formats: &[SheetFormat]) -> usize {
    formats.iter().filter(|f| sheet_on(cfg, f) != On::Job).count()
}

/// MS2: what Nest all adds about the sheets it put back on the job's machine.
pub fn moved_back_note(cfg: &AppSettings, n: usize) -> String {
    match n {
        0 => String::new(),
        1 => format!(" 1 sheet that was on another machine is back on the job's machine, {}: move it again if it should go there.", cfg.machine().name),
        n => format!(" {n} sheets that were on other machines are back on the job's machine, {}: move them again if they should go there.", cfg.machine().name),
    }
}

/// What moving a sheet did (MS3, MS4).
#[derive(Clone, Debug, PartialEq)]
pub struct Moved {
    /// The sheet's machine changed: nothing to save or undo when it didn't.
    pub changed: bool,
    pub message: String,
}

/// MS3, MS4: move sheet `k` (from 0) of a job to the machine named `name` in
/// Settings → Machines. The job's machine is stored as no name, so the sheet
/// follows the job; another machine by its name as Settings writes it. Its
/// automatic cut plan then comes from that machine's tables; cut qualities,
/// the cut order and leads set by hand stay, and so do the job's material,
/// thickness and gas (MS5). Refused, listing the machines, for a name that
/// isn't one of them.
#[allow(clippy::too_many_arguments)]
pub fn move_sheet(cfg: &AppSettings, parts: &[Part], sheets: &[Vec<Placed>], formats: &mut Vec<SheetFormat>, spec: &SheetSpec, manual: &[Vec<ManualCut>], k: usize, name: &str) -> Result<Moved, String> {
    if k >= sheets.len() {
        return Err(format!("The job has {} sheet(s); there is no sheet {}.", sheets.len(), k + 1));
    }
    let Some(to) = find(cfg, name) else {
        return Err(format!("No machine named \"{}\" in Settings → Machines. The machines are: {}.", name.trim(), machine_list(cfg)));
    };
    install(cfg);
    while formats.len() < sheets.len() {
        formats.push(SheetFormat::rect(spec.w, spec.h));
    }
    let to_name = cfg.machines[to].name.clone();
    let from = sheet_on(cfg, &formats[k]);
    let now = if to == cfg.active_machine { On::Job } else { On::Other(to) };
    if from == now {
        return Ok(Moved { changed: false, message: format!("Sheet {} is already on {to_name}: nothing changed.", k + 1) });
    }
    let from_name = shown(cfg, &formats[k]);
    let one = |formats: &[SheetFormat]| plans(parts, &sheets[k..=k], &formats[k..=k], spec, manual.get(k).map(std::slice::from_ref).unwrap_or(&[]));
    let before = one(formats);
    let kerf0 = kerf(&formats[k], &spec.for_format(&formats[k]));
    formats[k].machine = if to == cfg.active_machine { String::new() } else { to_name.clone() };
    let after = one(formats);
    // MS4, ST12: remnants stored from the sheet with the old kerf are made again.
    if kerf(&formats[k], &spec.for_format(&formats[k])) != kerf0 {
        crate::engine::stockflow::remnants_outdated(&mut formats[k]);
    }
    let (t0, t1) = (before.first().map_or(0.0, |p| p.seconds), after.first().map_or(0.0, |p| p.seconds));
    let mut said = vec![format!("Sheet {} moved from {from_name} to {to_name}: its automatic cut qualities, leads, cut order and kerf come from {to_name}'s tables now", k + 1)];
    let hand = hand_qualities(parts, &sheets[k]);
    if !hand.is_empty() {
        let list = hand.iter().map(|q| q.to_string()).collect::<Vec<_>>().join(", ");
        said.push(format!("cut qualities picked by hand keep their numbers ({list})"));
        let missing = on_sheet(&formats[k], || missing_qualities(&crate::ct1::for_spec(&spec.for_format(&formats[k])), spec, &hand));
        if !missing.is_empty() {
            let list = missing.iter().map(|q| q.to_string()).collect::<Vec<_>>().join(", ");
            said.push(format!("check them: {to_name}'s tables have no quality {list} (hidden, or no cutting speed in its technology table)"));
        }
    }
    if manual.get(k).is_some_and(|m| !m.is_empty()) {
        said.push("the cut order picked by hand stays".to_string());
    }
    if parts.iter().enumerate().any(|(i, p)| sheets[k].iter().any(|pl| pl.part == i) && p.tech.is_some_and(|t| t.lead_edits.iter().any(Option::is_some))) {
        said.push("leads moved by hand stay".to_string());
    }
    said.push(format!("the job's material, thickness and gas stay; cut time {t0:.1} s → {t1:.1} s"));
    Ok(Moved { changed: true, message: format!("{}.", said.join("; ")) })
}

/// The cut qualities set by hand on a contour of a part on `sheet` (MS4),
/// smallest first.
fn hand_qualities(parts: &[Part], sheet: &[Placed]) -> Vec<usize> {
    let mut qs: Vec<usize> = vec![];
    for (i, p) in parts.iter().enumerate() {
        if !sheet.iter().any(|pl| pl.part == i) {
            continue;
        }
        for c in contours::list(p) {
            if let Some(q) = c.quality {
                if !qs.contains(&q.number()) {
                    qs.push(q.number());
                }
            }
        }
    }
    qs.sort();
    qs
}

/// MAC-79: the qualities of `qs` a machine (`mt`, for the job's material,
/// thickness and gas) hides or has no cutting speed for in its technology
/// table's record. A machine without a technology table has every one.
fn missing_qualities(mt: &crate::ct1::MachineTech, spec: &SheetSpec, qs: &[usize]) -> Vec<usize> {
    let files = &mt.speed_files;
    let rec = files.table.as_ref().and_then(|t| t.pick(&crate::techtable::Keys::for_spec(spec, files.gas)).map(|(i, _)| t.records[i].clone()));
    qs.iter()
        .copied()
        .filter(|&q| {
            let hidden = nest::CutQuality::new(q).is_some_and(|c| !mt.qualities.offered(c));
            let no_speed = files.table.is_some() && rec.as_ref().is_none_or(|r| r.cut_speed(q).is_none());
            hidden || no_speed
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Writing programs per machine (MS7, MS8).
// ---------------------------------------------------------------------------

/// How and where one machine's programs are written.
#[derive(Clone, Debug)]
pub struct Output {
    /// The machine's name, as Settings → Machines shows it.
    pub machine: String,
    /// The folder its programs go in, `<folder>\<job>` below it (MS7).
    pub base: PathBuf,
    pub post: crate::post::PostConfig,
    /// Its BLM set settings with the job's name, material, thickness and gas
    /// filled in; `on` = it writes LC5 sets.
    pub blm: Option<crate::blm::BlmSettings>,
}

/// Each sheet's output, every machine's (to clear a job's old programs from
/// every folder, EXP1), and the sheets whose machine isn't in Settings.
#[derive(Clone, Debug)]
pub struct Targets {
    pub sheets: Vec<Output>,
    pub all: Vec<Output>,
    /// (sheet from 0, the machine name it stores).
    pub unknown: Vec<(usize, String)>,
    /// MS8: why nothing may be written, when a sheet's machine is unknown.
    pub refusal: Option<String>,
    /// MS7: a machine has a program folder of its own, so the job remembers
    /// where its programs went (`JobFile::written`) to clear them later.
    pub remember: bool,
    /// The program files the job's last write remembered (`JobFile::written`):
    /// cleared with the rest when this write doesn't make them again (MS7).
    pub earlier: Vec<PathBuf>,
}

/// MS7: where and how each sheet of a job is written. The job's machine
/// writes with `post` and `base` (its BLM switches, the session's or the
/// window's, which may differ from Settings); another machine with its own.
/// Each machine's programs go in its program folder, empty = `shared` (the
/// export folder); `dir`, a folder an agent names, takes them all (MAC-79),
/// while the old programs are still looked for in the machines' own folders
/// (`all`).
/// The job's gas stays the job's on every machine (MS5): a job without one
/// is cut with the job's machine's.
#[allow(clippy::too_many_arguments)]
pub fn targets(cfg: &AppSettings, job_name: &str, spec: &SheetSpec, formats: &[SheetFormat], n: usize, post: &crate::post::PostConfig, base: &crate::blm::BlmSettings, shared: &Path, dir: Option<&Path>) -> Targets {
    let configured = |m: Option<&settings::Machine>| match m {
        Some(m) if !m.program_dir.trim().is_empty() => PathBuf::from(m.program_dir.trim()),
        _ => shared.to_path_buf(),
    };
    let job_m = cfg.machines.get(cfg.active_machine);
    let job_all = Output { machine: cfg.machine().name, base: configured(job_m), post: post.clone(), blm: Some(blm_for(spec, base, job_name, cfg)) };
    let other_all = |i: usize| {
        let m = &cfg.machines[i];
        let b = crate::blm::BlmSettings { gas_id: base.gas_id, gas_name: base.gas_name.clone(), ..blm_base(m) };
        Output { machine: m.name.clone(), base: configured(Some(m)), post: post_of(m), blm: Some(blm_for(spec, &b, job_name, cfg)) }
    };
    // MS7, MAC-79: a folder an agent names takes what is written.
    let to = |o: Output| match dir {
        Some(d) => Output { base: d.to_path_buf(), ..o },
        None => o,
    };
    let job_out = to(job_all.clone());
    let other = |i: usize| to(other_all(i));
    let mut unknown = vec![];
    let sheets = (0..n)
        .map(|k| match sheet_on(cfg, &format_at(formats, spec, k)) {
            On::Job => job_out.clone(),
            On::Other(i) => other(i),
            On::Unknown(name) => {
                unknown.push((k, name));
                job_out.clone()
            }
        })
        .collect();
    // EXP1, MS7: every machine's own folder (and the shared one), whatever
    // `dir` says, and the folder `dir` names too.
    let mut all: Vec<Output> = (0..cfg.machines.len()).map(|i| if i == cfg.active_machine { job_all.clone() } else { other_all(i) }).chain(std::iter::once(Output { base: shared.to_path_buf(), ..job_all.clone() })).collect();
    if dir.is_some() {
        all.extend((0..cfg.machines.len()).map(|i| if i == cfg.active_machine { job_out.clone() } else { other(i) }));
    }
    let refusal = (!unknown.is_empty()).then(|| unknown_refusal(cfg, &unknown));
    let remember = cfg.machines.iter().any(|m| !m.program_dir.trim().is_empty());
    Targets { sheets, all, unknown, refusal, remember, earlier: vec![] }
}

/// MS8: the refusal for writing a job with a sheet on a machine that isn't in
/// Settings → Machines.
pub fn unknown_refusal(cfg: &AppSettings, unknown: &[(usize, String)]) -> String {
    let rows: Vec<String> = unknown.iter().map(|(k, n)| format!("sheet {} is on \"{n}\"", k + 1)).collect();
    format!("Export stopped: {}, which isn't in Settings → Machines (deleted or renamed). Move the sheet to one of the machines ({}), or nest again to put every sheet on the job's machine.", rows.join(", "), machine_list(cfg))
}
