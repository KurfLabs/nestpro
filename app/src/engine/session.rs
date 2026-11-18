//! A working session on job files, shared by the command line and the MCP
//! server. The job file is the truth: a job is read from disk when it is named
//! (or changed on disk since), and every change is saved straight back, the
//! previous version kept beside it as `<job>.snjob~` for one-step undo across
//! runs. Inside one session every step can be undone.

use super::ops::{self, Kind, Outcome};
use super::*;
use serde_json::{json, Value};
use std::time::SystemTime;

pub struct Session {
    pub ctx: Ctx,
    pub job: Option<JobFile>,
    pub path: Option<PathBuf>,
    /// When the open job's file was last read or written by us, and its
    /// size then: another PC's save within the same clock tick keeps the
    /// time but almost never the size.
    stamp: Option<(SystemTime, u64)>,
    undo: Vec<JobFile>,
    /// The material list this session read last (MA34): whether the open
    /// job's material was in it, whatever list another session in the same
    /// process installed since.
    seen: Option<std::sync::Arc<super::material::List>>,
    /// Why the open job is read-only (UP9): a newer version saved its file
    /// last. Operations that change it are refused; reading it works.
    read_only: Option<crate::shared::Newer>,
}

/// What a call gave back: the JSON answer and, for render, the PNG.
pub struct Reply {
    pub json: Value,
    pub image: Option<Vec<u8>>,
}

/// Undo copy of a job file.
pub fn backup_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push("~");
    PathBuf::from(s)
}

fn mtime(p: &Path) -> Option<(SystemTime, u64)> {
    std::fs::metadata(p).and_then(|m| Ok((m.modified()?, m.len()))).ok()
}

/// A job path as given, with the job extension added when it has none.
pub fn job_path(s: &str) -> PathBuf {
    let p = PathBuf::from(s);
    if p.extension().is_none() { p.with_extension(jobfile::EXT) } else { p }
}

impl Session {
    pub fn new(ctx: Ctx) -> Session {
        Session { ctx, job: None, path: None, stamp: None, undo: vec![], seen: None, read_only: None }
    }

    /// Open a job file (again, when it changed on disk).
    pub fn open(&mut self, path: &Path) -> Result<(), String> {
        let same = self.path.as_deref() == Some(path);
        if same && self.job.is_some() && mtime(path) == self.stamp {
            return Ok(());
        }
        let (j, written) = jobfile::load_versioned(path).map_err(|e| format!("Couldn't open {}: {e}", path.display()))?;
        self.read_only = self.ctx.versions().newer(written.as_deref());
        // Changed on disk by someone else (the window, another agent):
        // undoing to a copy from before that would drop their change. A job
        // a newer version saved is only read here (UP9): its undo copy is
        // that version's, and stays.
        if same && self.job.is_some() && self.read_only.is_none() {
            let _ = std::fs::remove_file(backup_path(path));
        }
        self.undo.clear();
        self.job = Some(j);
        self.path = Some(path.to_path_buf());
        self.stamp = mtime(path);
        Ok(())
    }

    /// The job file's save lock (`jobfile::save_lock`), taken once the file
    /// is checked under it: not one a newer version saved (UP7). Held from
    /// before anything the change books or stores until the file is written.
    fn lock_job_file(&self) -> Result<Option<crate::shared::WriteLock>, String> {
        let path = self.path.as_ref().ok_or("no job open")?;
        let lock = jobfile::save_lock(&self.ctx.library_dir)?;
        jobfile::may_save(path, &self.ctx.versions())?;
        Ok(lock)
    }

    /// Save the open job (under `lock`, from `lock_job_file`), keeping the
    /// file as it was beside it for undo. Checked first, so the undo copy
    /// is never changed for a save that is refused (UP7).
    fn save(&mut self, before: Option<JobFile>, lock: Option<&crate::shared::WriteLock>) -> Result<(), String> {
        let (Some(job), Some(path)) = (&self.job, &self.path) else { return Err("no job open".into()) };
        let versions = self.ctx.versions();
        jobfile::may_save(path, &versions)?;
        if path.exists() && before.is_some() {
            let _ = std::fs::copy(path, backup_path(path));
        }
        jobfile::save_under(path, job, &versions, lock)?;
        self.stamp = mtime(path);
        if let Some(b) = before {
            self.undo.push(b);
            if self.undo.len() > 100 {
                self.undo.remove(0);
            }
        }
        Ok(())
    }

    /// Run an operation. `job` names the job file (needed for `new`, and when
    /// no job is open yet).
    pub fn call(&mut self, name: &str, job: Option<&str>, args: &Value) -> Result<Reply, String> {
        // The material list may have changed on another PC since (MA14);
        // the first time this PC points at a Library its own list joins it
        // (MA28), and a job open in this PC's numbering takes the Library's.
        // MA34: whether the open job's material was in the list read before.
        let known = self.job.as_ref().map(|j| self.seen.as_ref().map_or_else(|| super::material::exists(j.spec.material), |l| l.get(j.spec.material).is_some()));
        let joined = self.ctx.reload_materials();
        self.seen = Some(super::material::current());
        if let (Some(j), Some(known)) = (self.job.as_mut(), known) {
            // MA30: a material added since the job was read is found by the
            // name the job came with; never by its number alone (MA34).
            j.spec.material = crate::materials::after_reload(joined.as_ref(), j.spec.material, &j.material_name, known);
        }
        let mut r = self.run_call(name, job, args)?;
        if let (Some(n), Some(o)) = (joined, r.json.as_object_mut()) {
            o.insert("materials_joined".into(), json!(n.note()));
        }
        Ok(r)
    }

    fn run_call(&mut self, name: &str, job: Option<&str>, args: &Value) -> Result<Reply, String> {
        let op = ops::find(name).ok_or_else(|| format!("unknown operation '{name}' — see `nestpro ops`"))?;
        let args = if args.is_null() { json!({}) } else { args.clone() };
        ops::check_args(op, &args)?;
        let done = |o: Outcome| Reply { json: o.to_json(), image: None };
        match op.kind {
            Kind::Global => {
                // The open job as its file is now (the window or another
                // agent may have changed its material since it was read): a
                // material it uses isn't deleted (MA20, JOB-88).
                let now = if name == "material" { self.path.as_deref().and_then(|p| jobfile::load(p).ok()) } else { None };
                return ops::run_global(&self.ctx, name, &args, now.as_ref().or(self.job.as_ref())).map(done);
            }
            Kind::Create => {
                let p = job_path(job.ok_or_else(|| format!("{name} needs the job file to make (job)"))?);
                let force = ops::Args(&args).bool("force")?.unwrap_or(false);
                if p.exists() && !force {
                    return Err(format!("{} already exists — pick another name, or force=true to overwrite it.", p.display()));
                }
                // UP7: force never writes over a job a newer version saved.
                let file = p.file_name().map_or_else(String::new, |f| f.to_string_lossy().into_owned());
                crate::shared::may_write_over(&p, &self.ctx.versions(), &format!("The job file {file}"))?;
                let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "New job".into());
                let mut a = args.clone();
                if let Some(m) = a.as_object_mut() {
                    m.remove("force");
                }
                let (j, message) = ops::create(&self.ctx, op.name, &stem, &a)?;
                if let Some(d) = p.parent().filter(|d| !d.as_os_str().is_empty()) {
                    std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
                }
                let lock = jobfile::save_lock(&self.ctx.library_dir)?;
                self.job = Some(j);
                self.path = Some(p.clone());
                self.read_only = None;
                self.undo.clear();
                self.save(None, lock.as_ref())?;
                return Ok(done(Outcome {
                    message,
                    data: Value::Null,
                    changed: true,
                    files: vec![p],
                }));
            }
            _ => {}
        }
        match job {
            Some(p) => self.open(&job_path(p))?,
            None => match self.path.clone() {
                Some(p) => self.open(&p)?,
                None => return Err("No job open — give the job file (job).".into()),
            },
        }
        // UP9: a job a newer version saved is read-only here: nothing that
        // changes it runs (nor books stock for it), and its file stays as it
        // is. Listing the Library changes nothing, so it still answers.
        if let Some(n) = &self.read_only {
            let listing = op.name == "library" && ops::Args(&args).str("mode").as_deref() == Some("list");
            if op.kind == Kind::Write && !listing {
                let file = self.path.as_ref().and_then(|p| p.file_name()).map_or_else(String::new, |f| f.to_string_lossy().into_owned());
                return Err(n.refusal(&format!("The job file {file}")));
            }
        }
        match op.name {
            "undo" => return self.undo_step().map(done),
            "render" => return self.render(&args),
            _ => {}
        }
        // UP7: a change is never made to a job file a newer version saved,
        // nor anything booked or stored for it. Operations that store in the
        // Library or the stock themselves (library, stock-book) are short:
        // they run under the job file's save lock, checked first. Others
        // (a nest can take a while) are checked now, and again under the
        // lock before anything is booked or written.
        let listing = op.name == "library" && ops::Args(&args).str("mode").as_deref() == Some("list");
        let early = match op.kind {
            Kind::Write if listing => None,
            Kind::Write if matches!(op.name, "library" | "stock-book") => {
                let lock = self.lock_job_file()?;
                // A booked job's change is booked after the call (below): a
                // stock a newer version changed is refused now, under the
                // lock, before the call stores anything in the Library.
                if op.name == "library" && self.job.as_ref().is_some_and(|j| j.booked) {
                    crate::shared::may_write_over(&crate::stock::Stock::file(&self.ctx.library_dir), &self.ctx.versions(), "The sheet stock")?;
                }
                Some(lock)
            }
            Kind::Write => {
                let path = self.path.as_ref().ok_or("no job open")?;
                jobfile::may_save(path, &self.ctx.versions())?;
                None
            }
            _ => None,
        };
        let job = self.job.as_mut().unwrap();
        let before = job.clone();
        let mut out = match ops::run(job, &self.ctx, name, &args) {
            Ok(o) => o,
            Err(e) => {
                // Refused: nothing it changed before refusing stays, or the
                // next change would save it.
                *job = before;
                return Err(e);
            }
        };
        let lock = match early {
            Some(l) => l,
            None if out.changed && op.kind == Kind::Write => match self.lock_job_file() {
                Ok(l) => l,
                Err(e) => {
                    self.job = Some(before);
                    return Err(e);
                }
            },
            None => None,
        };
        let job = self.job.as_mut().unwrap();
        // A booked job keeps the stock in step with every change (as the
        // window does on every save); not with a stock a newer version
        // changed (UP7): the change is refused and the job left as it was.
        if out.changed && op.kind == Kind::Write && job.booked && op.name != "stock-book" {
            if let Err(e) = crate::shared::may_write_over(&crate::stock::Stock::file(&self.ctx.library_dir), &self.ctx.versions(), "The sheet stock") {
                *job = before;
                return Err(e);
            }
            let note = job.book_stock(&self.ctx, true);
            if !note.is_empty() {
                out.message = format!("{} Stock: {note}.", out.message);
            }
        }
        // A job saved into the Library (LB10) has booked the stock, and its
        // record holds the sheets (or, when the record couldn't be written,
        // this file does): undo can't step back past it to a file that holds
        // none of them (a save or stock-book would take them again).
        let into_library = name == "library" && out.changed && ops::Args(&args).str("mode").as_deref() == Some("save-job");
        if out.changed && op.kind == Kind::Write {
            self.save(Some(before), lock.as_ref())?;
        }
        drop(lock);
        if into_library {
            self.undo.clear();
            if let Some(p) = &self.path {
                let _ = std::fs::remove_file(backup_path(p));
            }
            // The record couldn't be written: the booking is kept in the job
            // file (LB13), but the call failed and says so.
            if out.data.get("saved") == Some(&json!(false)) {
                return Err(format!("{} Undo can't go back past it: this job file holds what was booked.", out.message));
            }
            out.message.push_str(" Undo can't go back past this save: the record keeps it.");
        }
        let mut reply = done(out);
        if let Some(p) = &self.path {
            reply.json["job"] = json!(p.display().to_string());
        }
        if let Some(n) = &self.read_only {
            reply.json["read_only"] = json!(format!("This job is read-only on this PC. {}", n.message()));
        }
        Ok(reply)
    }

    fn undo_step(&mut self) -> Result<Outcome, String> {
        let path = self.path.clone().ok_or("no job open")?;
        // UP7: checked under the save lock before the stock is touched.
        let lock = self.lock_job_file()?;
        let prev = match self.undo.pop() {
            Some(j) => j,
            // Across runs: the copy kept beside the file.
            None => jobfile::load(&backup_path(&path)).map_err(|_| "Nothing to undo.".to_string())?,
        };
        let note = self.restock_for_undo(prev)?;
        let versions = self.ctx.versions();
        jobfile::save_under(&path, self.job.as_ref().unwrap(), &versions, lock.as_ref())?;
        // The copy beside the file moves one step back too, so a later run
        // can undo once more.
        match self.undo.last() {
            Some(older) => jobfile::save_under(&backup_path(&path), older, &versions, lock.as_ref())?,
            None => {
                let _ = std::fs::remove_file(backup_path(&path));
            }
        }
        self.stamp = mtime(&path);
        Ok(Outcome { message: format!("Undone — the job is back to before the last change.{note}"), data: Value::Null, changed: true, files: vec![] })
    }

    /// Undo doesn't roll the shared stock back: the stock follows the job
    /// again instead. What the job holds stays what is really taken, then a
    /// booked job is booked against its restored sheets (undoing a booking
    /// gives everything back).
    fn restock_for_undo(&mut self, mut prev: JobFile) -> Result<String, String> {
        let (was_booked, mut held, mut formats) = match self.job.as_ref() {
            Some(cur) => (cur.booked, cur.held.clone(), cur.formats.clone()),
            None => (false, Default::default(), vec![]),
        };
        let note = if prev.booked {
            // Remnants the undone change replaced: the restored sheets take
            // over their replacements, to be rebuilt (RN6).
            if let Ok(stock) = crate::stock::Stock::load(&self.ctx.library_dir) {
                prev.normalise_formats();
                stockflow::follow_stock(&stock, &mut prev.formats);
            }
            // Remnants the undone change stored go out of stock again.
            let mut gone: Vec<SheetFormat> = formats.into_iter().filter(|f| f.made.iter().any(|r| !prev.formats.iter().any(|p| p.made.contains(r)))).collect();
            for f in gone.iter_mut() {
                f.made.retain(|r| !prev.formats.iter().any(|p| p.made.contains(r)));
            }
            if !gone.is_empty() {
                stockflow::release(&self.ctx.library_at(), &mut gone, &mut Default::default())?;
            }
            prev.held = held;
            prev.book_stock(&self.ctx, true)
        } else if was_booked {
            let (back, removed, _) = stockflow::release(&self.ctx.library_at(), &mut formats, &mut held)?;
            prev.held.clear();
            format!("{back} sheet(s) given back to stock, {removed} remnant(s) taken out")
        } else {
            String::new()
        };
        self.job = Some(prev);
        Ok(if note.is_empty() { String::new() } else { format!(" Stock: {note}.") })
    }

    fn render(&mut self, args: &Value) -> Result<Reply, String> {
        let a = ops::Args(args);
        let job = self.job.as_ref().unwrap();
        let width = a.int("size")?.unwrap_or(1400).clamp(200, 6000) as u32;
        let path = a.bool("path")?.unwrap_or(true);
        let stem = self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "job".into());
        let dir = self.path.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf).unwrap_or_default();
        let (scene, what, default_out) = if args.get("part").is_some_and(|v| !v.is_null()) {
            let v = ops::parts_sel(&job.parts, args.get("part"), false)?;
            let i = *v.first().ok_or("name one part")?;
            (render::part_scene(job, i, width, path), format!("part {} '{}'", i + 1, job.parts[i].name), dir.join(format!("{stem}-part{}.png", i + 1)))
        } else {
            let k = a.int("sheet")?.unwrap_or(1);
            if k < 1 {
                return Err("sheets are numbered from 1".into());
            }
            let k = k as usize - 1;
            (render::sheet_scene(job, k, width, path)?, format!("sheet {}", k + 1), dir.join(format!("{stem}-sheet{}.png", k + 1)))
        };
        let out = a.str("out").map(PathBuf::from).unwrap_or(default_out);
        ops::refuse_protected(&self.ctx, &out)?;
        let png = render::write(&scene, &out)?;
        let image = match png {
            Some(p) => Some(p),
            None => Some(scene.png()?),
        };
        let o = Outcome {
            message: format!("Drew {what} to {}. Colours: parts filled with #numbers; red = cut contour, green = lead-in and pierce (with cut order), yellow = micro-joint, blue dashed = rapid move, magenta = cut/remnant line, light green = remnant; on a part, blue numbers are its elements.", out.display()),
            data: Value::Null,
            changed: false,
            files: vec![out],
        };
        Ok(Reply { json: o.to_json(), image })
    }
}
