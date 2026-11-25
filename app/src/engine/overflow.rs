//! Export for more than one machine (MECHANICS EXP3–EXP6): when one machine
//! is busy, the same nested sheets are written for another machine of the
//! job's process, each machine into its own programs folder, each with its
//! own technology (lead-ins, micro-joints, qualities, post). A machine that
//! can't take the nest as it is says why and gets nothing; the job itself
//! never changes. The window's Export review and the agents' `export
//! machines=` both go through here.

use super::*;

/// EXP3: the folder a machine's programs go under: its own programs folder
/// (Settings → Machines), else the export folder (Settings → Folders), else
/// Documents\Sheet Nesting. `k` past the list: the export folder.
pub fn programs_base(cfg: &AppSettings, k: usize) -> PathBuf {
    match cfg.machines.get(k).map(|m| m.programs_dir.trim()).filter(|d| !d.is_empty()) {
        Some(d) => PathBuf::from(d),
        None if cfg.export_dir.is_empty() => crate::library::documents_dir(),
        None => PathBuf::from(&cfg.export_dir),
    }
}

/// EXP3: the folder machine `k` writes this job's programs into.
pub fn job_dir(cfg: &AppSettings, k: usize, job_name: &str) -> PathBuf {
    programs_base(cfg, k).join(safe_name(job_name))
}

/// EXP4: two folders are one place: compared as the system finds them (the
/// deepest part that exists), with either slash and any case, so `\\srv\CNC`
/// and `\\SRV\cnc\` are the same folder. Our choice: any case on every system,
/// which only ever refuses more.
pub fn same_folder(a: &Path, b: &Path) -> bool {
    fn norm(p: &Path) -> String {
        // A folder given from the working folder is the same as written out in full.
        let p = &std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
        let mut rest: Vec<std::ffi::OsString> = vec![];
        let mut at = p.to_path_buf();
        let real = loop {
            if let Ok(r) = std::fs::canonicalize(&at) {
                break rest.iter().rev().fold(r, |acc, n| acc.join(n));
            }
            match (at.file_name().map(|n| n.to_os_string()), at.parent().map(Path::to_path_buf)) {
                (Some(n), Some(up)) if !up.as_os_str().is_empty() => {
                    rest.push(n);
                    at = up;
                }
                _ => break p.to_path_buf(),
            }
        };
        real.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase()
    }
    norm(a) == norm(b)
}

/// The job an export writes, from the window or a job file.
#[derive(Clone, Copy)]
pub struct JobRef<'a> {
    pub name: &'a str,
    pub parts: &'a [Part],
    pub sheets: &'a [Vec<Placed>],
    pub formats: &'a [SheetFormat],
    pub spec: &'a SheetSpec,
    pub manual: &'a [Vec<ManualCut>],
    pub cuts: &'a [Vec<CutLine>],
}

impl<'a> JobRef<'a> {
    pub fn of(j: &'a JobFile) -> JobRef<'a> {
        JobRef { name: &j.name, parts: &j.parts, sheets: &j.sheets, formats: &j.formats, spec: &j.spec, manual: &j.manual_cuts, cuts: &j.cut_lines }
    }
}

/// "BLM 2" as Settings → Machines names machine `k` (from 0).
pub fn machine_name(cfg: &AppSettings, k: usize) -> String {
    cfg.machines.get(k).map_or_else(|| format!("machine {}", k + 1), |m| m.name.clone())
}

/// EXP4, EXP5: with machine `k` in use (`on`), the job's parts as that
/// machine cuts them and its plan of every sheet; or why it can't take the
/// nest as it is. Its automatic micro-joint gap and spacing follow it (as a
/// gas change does, GS14); hand-set values, leads and cut order stay.
fn fit(cfg: &AppSettings, k: usize, job: JobRef, on: &CallMachine) -> Result<(Vec<Part>, Vec<SheetPlan>), String> {
    let m = &cfg.machines[k];
    if m.kind != job.spec.machine {
        return Err(format!(
            "it is a {} machine and this job is set for {}: the same nest can't go to another kind of machine yet (FEATURES MAC-109)",
            m.kind.process().to_lowercase(),
            job.spec.machine.process().to_lowercase()
        ));
    }
    if let Some(why) = on.post.refusal() {
        return Err(why);
    }
    if job.sheets.is_empty() {
        return Err("nothing is nested yet".into());
    }
    let t = crate::ct1::for_spec(job.spec);
    // Its own file: the BLM post's in the Lantek folder belongs to an LC5,
    // never to a machine on another post.
    let lantek = if cfg.lantek_dir.is_empty() { settings::lantek_default_dir() } else { cfg.lantek_dir.clone() };
    let blm_file = Path::new(&lantek).join("MchFiles").join("PstAdi06.CT1");
    let own = t.source.as_ref().is_some_and(|f| m.output == settings::Output::BlmSiemens || !same_folder(f, &blm_file));
    if !own {
        return Err("no technology file of its own was found (a .CT1 beside its post files; the BLM post's in the Lantek folder is the LC5's), so its lead-ins, micro-joints and part gap aren't known".into());
    }
    if !t.file.sections.is_empty() && t.applied().is_none() {
        let gas = job.spec.gas.map_or(String::new(), |g| format!(" with {}", g.name()));
        return Err(format!("its technology file has no table for {} {} mm{gas}", material::name_or(job.spec.material, "?"), job.spec.thickness));
    }
    let mut parts = job.parts.to_vec();
    for pt in parts.iter_mut().filter_map(|p| p.tech.as_mut()) {
        joints_follow_machine(pt, &t);
    }
    let plans = plans(&parts, job.sheets, job.formats, job.spec, job.manual);
    // Its part gap and margin for the material, else the job's.
    let gap = t.part_gap.unwrap_or(job.spec.gap);
    let margin = t.sheet_margin().unwrap_or(job.spec.margin);
    let edge = SheetSpec { margin, ..job.spec.clone() };
    let mut bad: Vec<(usize, usize, String)> = vec![];
    for (s, (sheet, plan)) in job.sheets.iter().zip(&plans).enumerate() {
        let f = format_at(job.formats, job.spec, s);
        let mut issues: Vec<String> = verify_sheet(&parts, sheet, &job.spec.for_format(&f), plan, Some(gap)).into_iter().map(|i| i.text).collect();
        let near = sheet.iter().filter(|c| !part_fits_format(&parts, &edge, &f, c)).count();
        if near > 0 {
            issues.push(format!("{near} part(s) inside its {margin} mm margin"));
        }
        if let Some(first) = issues.first() {
            bad.push((s + 1, issues.len(), first.clone()));
        }
    }
    if let Some((_, _, first)) = bad.first() {
        let n: usize = bad.iter().map(|b| b.1).sum();
        let which = bad.iter().map(|b| b.0.to_string()).collect::<Vec<_>>().join(", ");
        return Err(format!(
            "Verify machining with its technology (part gap {gap} mm, margin {margin} mm) finds {n} problem(s) on sheet {which}, first: {first}. To cut it there, pick it as the machine and nest the job for it"
        ));
    }
    Ok((parts, plans))
}

/// EXP4: can machine `k` take the job's nest as it is? Err says why not.
/// Reads its post and technology files; the machine in use is back after.
pub fn check(cfg: &AppSettings, k: usize, job: JobRef) -> Result<(), String> {
    let m = cfg.machines.get(k).ok_or_else(|| format!("there's no machine {} in Settings → Machines", k + 1))?;
    let on = CallMachine::load_in(&cfg.lantek_dir, m, String::new());
    let (parts, plans) = fit(cfg, k, job, &on)?;
    // BL39: what an LC5's writer would refuse shows here, not only at export.
    if on.blm.on {
        let blm = blm_for(job.spec, &on.blm, job.name);
        for (s, (sheet, plan)) in job.sheets.iter().zip(&plans).enumerate() {
            let spec = job.spec.for_format(&format_at(job.formats, job.spec, s));
            let cuts = job.cuts.get(s).map_or(&[][..], |v| v.as_slice());
            let set = crate::blm::sheet_set(s + 1, &parts, sheet, &spec, plan, cuts, &blm);
            let issues = readback::check_set(&set, &parts, sheet, plan, cuts);
            if !issues.is_empty() {
                return Err(readback::refusal(s + 1, &issues));
            }
        }
    }
    Ok(())
}

/// EXP4: write the job's programs for machine `k` into `base\<job>` (its
/// programs folder, EXP3), checked first (`check`); `marks` gives each
/// sheet's remnant marks, asked while the machine is in use (its kerf). The
/// export's own stops (JOB-41, BL39, OM1) apply as for the machine in use.
/// Returns what was written and the machine's LC5 switches, for the note.
pub fn write(cfg: &AppSettings, k: usize, job: JobRef, base: &Path, marks: &dyn Fn() -> Vec<Vec<Vec<crate::nest::P>>>, bridge: f64) -> Result<(Exported, crate::blm::BlmSettings), String> {
    let m = cfg.machines.get(k).ok_or_else(|| format!("there's no machine {} in Settings → Machines", k + 1))?;
    let on = CallMachine::load_in(&cfg.lantek_dir, m, String::new());
    let (parts, plans) = fit(cfg, k, job, &on)?;
    let blm = blm_for(job.spec, &on.blm, job.name);
    let marks = marks();
    let ex = export(base, job.name, &parts, job.sheets, job.formats, &plans, job.spec, job.cuts, &marks, &on.post, Some(&blm), bridge)?;
    Ok((ex, on.blm.clone()))
}

/// EXP6: this job's programs (any format a machine in Settings writes) in
/// one machine's job folder that another export left there.
#[derive(Clone, Debug, PartialEq)]
pub struct Earlier {
    /// The machines (from 0) whose job folder it is.
    pub machines: Vec<usize>,
    pub dir: PathBuf,
    pub files: Vec<PathBuf>,
    /// The folder couldn't be looked in (a share offline): why.
    pub unread: Option<String>,
}

/// This job's program files in `dir` as `post` and the LC5 job name
/// `blm_job` name them (sheets 1–999): plain G-code, OMAX `.omx`, LC5 `.zip`
/// and `.XML`. One look at the folder, not one per name. With what was
/// found, why the folder couldn't be read to the end, if it couldn't.
fn job_programs(dir: &Path, job_name: &str, post: &crate::post::PostConfig, blm_job: &str) -> (Vec<PathBuf>, Option<String>) {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        // No job folder there: nothing of this job's.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (vec![], None),
        Err(e) => return (vec![], Some(e.to_string())),
    };
    let mut names = std::collections::HashSet::new();
    for k in 1..=999 {
        let nest = crate::blm::nest_name(blm_job, k);
        for n in [post.iso_file_name(job_name, k), crate::post::PostConfig::name_with(job_name, k, crate::omax::EXTENSION), format!("{nest}.zip"), format!("{nest}.XML")] {
            names.insert(n.to_lowercase());
        }
    }
    let mut out: Vec<PathBuf> = vec![];
    let mut unread = None;
    for e in rd {
        // A folder only partly read could keep programs back unseen: said,
        // and what was found still goes.
        let e = match e {
            Ok(e) => e,
            Err(err) => {
                unread.get_or_insert(err.to_string());
                continue;
            }
        };
        if names.contains(&e.file_name().to_string_lossy().to_lowercase()) {
            match e.file_type() {
                Ok(t) if t.is_file() => out.push(e.path()),
                Ok(_) => {}
                Err(err) => {
                    unread.get_or_insert(err.to_string());
                }
            }
        }
    }
    out.sort();
    (out, unread)
}

/// EXP6: the job folders of the machines in Settings that aren't among
/// `written` and still hold programs of this job from an earlier export.
pub fn earlier_elsewhere(cfg: &AppSettings, job_name: &str, spec: &SheetSpec, written: &[PathBuf]) -> Vec<Earlier> {
    let mut out: Vec<Earlier> = vec![];
    for (i, m) in cfg.machines.iter().enumerate() {
        let dir = job_dir(cfg, i, job_name);
        if written.iter().any(|w| same_folder(w, &dir)) {
            continue;
        }
        let blm = blm_for(spec, &blm_base(m), job_name);
        let post = load_post(m);
        let (files, mut unread) = job_programs(&dir, job_name, &post, &blm.job);
        // Its program names come from its post: one that can't be read
        // leaves them unknown (EXP2).
        if !post.missing.is_empty() && unread.is_none() {
            unread = Some(format!("its post file {} can't be read, so the names of its programs aren't known", post.missing.join(", ")));
        }
        // Machines with no folder of their own share the export folder.
        match out.iter_mut().find(|e| e.dir == dir) {
            Some(e) => {
                e.machines.push(i);
                for f in files {
                    if !e.files.contains(&f) {
                        e.files.push(f);
                    }
                }
                if e.unread.is_none() {
                    e.unread = unread;
                }
            }
            None => out.push(Earlier { machines: vec![i], dir, files, unread }),
        }
    }
    out.retain(|e| !e.files.is_empty() || e.unread.is_some());
    out
}

/// EXP6: what the review says about one: its programs will go, or its
/// folder couldn't be looked in.
pub fn earlier_line(cfg: &AppSettings, e: &Earlier) -> String {
    let who = e.machines.iter().map(|&k| machine_name(cfg, k)).collect::<Vec<_>>().join(", ");
    match &e.unread {
        Some(why) => format!("{who}'s folder {} couldn't be looked in ({why}): check it has no programs of this job from an earlier export before anyone cuts there.", e.dir.display()),
        None => format!("{who}'s folder has {} program file(s) of this job from an earlier export — they will be removed, so no sheet is cut on two machines.", e.files.len()),
    }
}

/// EXP6: remove those programs. Says what went (and what couldn't, which
/// must be deleted by hand before anyone runs it).
pub fn remove_earlier(cfg: &AppSettings, list: &[Earlier]) -> String {
    let mut said = vec![];
    for e in list {
        let who = e.machines.iter().map(|&k| machine_name(cfg, k)).collect::<Vec<_>>().join(", ");
        if e.unread.is_some() {
            said.push(format!(" {}", earlier_line(cfg, e)));
        }
        // One already gone counts as removed.
        let failed: Vec<String> = e
            .files
            .iter()
            .filter(|f| std::fs::remove_file(f).is_err_and(|err| err.kind() != std::io::ErrorKind::NotFound))
            .map(|f| f.file_name().map_or_else(|| f.display().to_string(), |n| n.to_string_lossy().into_owned()))
            .collect();
        if e.files.is_empty() {
            continue;
        }
        if failed.is_empty() {
            said.push(format!(" Removed this job's {} earlier program file(s) from {who}'s folder {}, so they can't be cut there too.", e.files.len(), e.dir.display()));
        } else {
            said.push(format!(" This job's earlier program(s) {} in {who}'s folder {} couldn't be removed: delete them before anyone runs them.", failed.join(", "), e.dir.display()));
        }
    }
    said.concat()
}

/// EXP4: the reminder after programs for the same sheets went to more than one machine.
pub fn cut_once_note(names: &[String]) -> String {
    if names.len() < 2 {
        return String::new();
    }
    format!(" The same sheets are now in the folders of {}: cut each sheet on one machine only.", names.join(" and "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_the_same_whatever_its_slashes_and_case() {
        let d = std::env::temp_dir().join(format!("nestpro_same_folder_{}", std::process::id()));
        std::fs::create_dir_all(d.join("A")).unwrap();
        assert!(same_folder(&d.join("A"), &d.join("A").join(".")));
        assert!(same_folder(&d.join("A").join("job"), &PathBuf::from(format!("{}/A/JOB/", d.display()))));
        assert!(!same_folder(&d.join("A").join("job"), &d.join("B").join("job")));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_machine_without_its_own_folder_writes_into_the_export_folder() {
        let mut cfg = AppSettings::default();
        cfg.export_dir = r"C:\cnc\out".into();
        cfg.machines = vec![settings::Machine { name: "Demo 1".into(), ..Default::default() }, settings::Machine { name: "Demo 2".into(), programs_dir: r"\\srv\cnc\demo2".into(), ..Default::default() }];
        assert_eq!(programs_base(&cfg, 0), PathBuf::from(r"C:\cnc\out"));
        assert_eq!(programs_base(&cfg, 1), PathBuf::from(r"\\srv\cnc\demo2"));
        assert_eq!(programs_base(&cfg, 7), PathBuf::from(r"C:\cnc\out"));
        assert_eq!(cut_once_note(&["Demo 1".into()]), "");
        assert!(cut_once_note(&["Demo 1".into(), "Demo 2".into()]).contains("one machine only"));
    }
}
