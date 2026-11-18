//! The shop's rules (spec #178, SH1–SH8): starting a shop in a Library's
//! folder, each PC recording itself, the admin PC, and what the shop looks
//! like. Plain functions over a shop folder: Settings → Shop and the agents'
//! `shop` and `setup` operations call them.

use super::ops::{Args, Outcome};
use super::Ctx;
use crate::backup;
use crate::shared::{Clock, Saved, Who, WriteLock};
use crate::shop::{self, Admin, PcRecord, SettingsBy, ShopFile};
use serde_json::{json, Map, Value};
use std::path::Path;

/// SH7: why an agent can't change the admin PC.
pub const AGENT_ADMIN: &str = "Changing the admin PC is refused for agents: the admin PC rolls out updates and restores backups, so a person changes it, in the window (Settings → Shop → Make this PC the admin PC, on the PC that should be the admin PC). Nothing was changed.";

/// A shop folder this PC can't write to.
fn cant_write(dir: &Path, e: &str) -> String {
    format!("No shop was started: this PC can't write in {} ({e}). Check that this PC's user may change files there.", dir.display())
}

/// SH1, SH2: start a shop in the Library folder `dir`, named `name`, with
/// `who`'s PC as its admin PC. The Library folder becomes the shop folder in
/// place: the shop file and the Shop, Updates and Backups folders are added,
/// and nothing already there is changed or moved. A folder that can't be
/// reached, or holds no Library, is refused, naming it, and nothing is made
/// (SH8, LB11, LB12). A folder that is already a shop is left as it is.
/// Made under the Library's write lock (LB1).
pub fn start(dir: &Path, name: &str, who: &Who, clock: &Clock, version: &str) -> Result<ShopFile, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the shop a name (name=Acme). Nothing was made.".into());
    }
    crate::library::usable(dir)?;
    if let Some(s) = shop::read(dir)? {
        return Err(already(dir, &s));
    }
    let lock = WriteLock::take(dir).map_err(|e| cant_write(dir, &e))?;
    // Another PC may have started it while this one waited for the lock.
    if let Some(s) = shop::read(dir)? {
        return Err(already(dir, &s));
    }
    let mut made = vec![];
    let undo = |made: &[std::path::PathBuf]| {
        for d in made.iter().rev() {
            let _ = std::fs::remove_dir(d);
        }
    };
    for f in shop::FOLDERS {
        let d = dir.join(f);
        if !d.is_dir() {
            if let Err(e) = std::fs::create_dir(&d) {
                undo(&made);
                return Err(cant_write(dir, &e.to_string()));
            }
            made.push(d);
        }
    }
    let at = clock.now();
    let s = ShopFile {
        name: name.to_string(),
        admin: Admin { pc: who.pc.clone(), by: who.clone(), at, was: None },
        settings_by: SettingsBy::AdminPc,
        started: Saved { by: who.clone(), at },
        backups: Default::default(),
        other: Map::new(),
    };
    if let Err(e) = shop::write(dir, &s, &lock) {
        undo(&made);
        return Err(cant_write(dir, &e));
    }
    // The PC that started it is in it (SH4); the shop is made either way.
    let _ = write_record(dir, who, at, version, &lock);
    Ok(s)
}

fn already(dir: &Path, s: &ShopFile) -> String {
    format!("{} is already the shop folder of {} (admin PC {}). Nothing was changed.", dir.display(), s.name, s.admin.pc)
}

fn write_record(dir: &Path, who: &Who, at: u64, version: &str, lock: &WriteLock) -> Result<(), String> {
    shop::write_record(dir, &PcRecord { pc: who.pc.clone(), user: who.user.clone(), version: version.to_string(), last_seen: at, other: Map::new() }, lock)
}

/// SH4: this PC records itself in the shop when it opens the program: its
/// name, its user, its version and when. Only its own record, under the
/// Library's write lock (LB1). A Library that is no shop is left alone; one
/// that can't be reached is refused, naming it (nothing is made).
pub fn record_pc(dir: &Path, who: &Who, clock: &Clock, version: &str) -> Result<bool, String> {
    if shop::read(dir)?.is_none() {
        return Ok(false);
    }
    let lock = WriteLock::take(dir)?;
    write_record(dir, who, clock.now(), version, &lock)?;
    Ok(true)
}

/// SH7: make `who`'s PC the admin PC, recording who did it and when. Only a
/// person, in the window, after a warning: an agent is refused with the
/// reason and nothing changes. Under the Library's write lock (LB1).
pub fn make_admin(dir: &Path, who: &Who, clock: &Clock) -> Result<ShopFile, String> {
    if who.agent {
        return Err(AGENT_ADMIN.into());
    }
    let not_shop = || format!("{} isn't a shop. Nothing was changed.", dir.display());
    shop::read(dir)?.ok_or_else(not_shop)?;
    let lock = WriteLock::take(dir)?;
    let mut s = shop::read(dir)?.ok_or_else(not_shop)?;
    if s.admin.pc != who.pc {
        let was = std::mem::take(&mut s.admin.pc);
        s.admin = Admin { pc: who.pc.clone(), by: who.clone(), at: clock.now(), was: Some(was) };
        shop::write(dir, &s, &lock)?;
    }
    Ok(s)
}

/// SH5: what the shop is, as Settings → Shop and agents show it.
#[derive(Clone)]
pub struct Status {
    pub shop: ShopFile,
    pub pcs: Vec<PcRecord>,
}

/// The shop in `dir` and its PCs; None when the folder is no shop. A folder
/// that can't be reached is refused, naming it (SH8).
pub fn status(dir: &Path) -> Result<Option<Status>, String> {
    Ok(shop::read(dir)?.map(|shop| Status { shop, pcs: shop::records(dir) }))
}

impl Status {
    /// "PC-B made the admin PC by kim on PC-B, 9 Nov 09:20 (was PC-A)".
    pub fn admin_note(&self, clock: &Clock) -> String {
        let a = &self.shop.admin;
        let was = a.was.as_ref().map_or(String::new(), |w| format!(" (was {w})"));
        format!("{} made the admin PC by {}, {}{was}", a.pc, a.by.label(), clock.shown(a.at))
    }
}

// ---------------------------------------------------------------------------
// Backups (BK1–BK7): the rules on where they go and what they hold are in
// `crate::backup`; these say whose settings they follow and who changes them.
// ---------------------------------------------------------------------------

/// BK7: whether the Library in `dir` is a shop, and the backup settings it
/// follows: the shop file's in a shop, this PC's own (`own`) otherwise. A
/// Library that can't be reached is refused, naming it (SH8).
pub fn backup_settings(dir: &Path, own: &backup::Settings) -> Result<(bool, backup::Settings), String> {
    Ok(match shop::read(dir)? {
        Some(s) => (true, s.backups),
        None => (false, own.clone()),
    })
}

/// BK2: the day's backup, as the program takes it when it opens: none when
/// one of today is there already (another PC, or this one, took it).
pub fn daily_backup(dir: &Path, own: &backup::Settings, who: &Who, clock: &Clock) -> Result<backup::Daily, String> {
    let (in_shop, set) = backup_settings(dir, own)?;
    backup::take(dir, in_shop, &set, who, clock, backup::Why::Daily)
}

/// BK7: Back up now (and agents' `shop mode=backup`): a backup whatever
/// was taken today.
pub fn backup_now(dir: &Path, own: &backup::Settings, who: &Who, clock: &Clock) -> Result<backup::Done, String> {
    let (in_shop, set) = backup_settings(dir, own)?;
    match backup::take(dir, in_shop, &set, who, clock, backup::Why::Now)? {
        backup::Daily::Taken(d) => Ok(d),
        backup::Daily::Done(e) => Err(format!("No backup was taken: {} is today's already.", e.name)),
    }
}

/// BK7: the backups of the Library in `dir`, for Settings → Shop and agents.
pub fn backups(dir: &Path, own: &backup::Settings) -> Result<backup::Overview, String> {
    let (in_shop, set) = backup_settings(dir, own)?;
    Ok(backup::overview(dir, in_shop, &set))
}

/// BK4, BK5, BK7: change Keep and the second place. In a shop they are shop
/// settings (SH3): changed only on the admin PC unless the shop changes
/// them from any PC, in the shop file, under the Library's write lock
/// (LB1). A PC of its own changes `own` (its settings, which the caller
/// saves). Keep is 1 to 3650 days; the second place can't be the backup
/// folder or in the Library (it would be zipped in every backup). Gives
/// whether it was the shop's.
pub fn set_backup_settings(dir: &Path, own: &mut backup::Settings, who: &Who, new: &backup::Settings) -> Result<bool, String> {
    if !(1..=backup::MOST_KEEP).contains(&new.keep_days) {
        return Err(format!("Keep backups 1 to {} days. Nothing was changed.", backup::MOST_KEEP));
    }
    let place = new.second_place.trim();
    if !place.is_empty() {
        let p = Path::new(place);
        let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let lib = canon(dir);
        let in_shop = shop::read(dir)?.is_some();
        let folder = backup::folder(dir, in_shop).map(|f| canon(&f)).ok();
        let cp = canon(p);
        if cp.starts_with(&lib) || folder.is_some_and(|f| cp == f) {
            return Err(format!("The second place {place} is in the Library or is its backup folder, so a dead server would take it too. Pick another drive or a NAS. Nothing was changed."));
        }
    }
    let new = backup::Settings { keep_days: new.keep_days, second_place: place.to_string() };
    let Some(s) = shop::read(dir)? else {
        *own = new;
        return Ok(false);
    };
    may_change_settings(&s, who)?;
    let lock = WriteLock::take(dir)?;
    let mut s = shop::read(dir)?.ok_or_else(|| format!("{} isn't a shop any more. Nothing was changed.", dir.display()))?;
    may_change_settings(&s, who)?;
    s.backups = new;
    shop::write(dir, &s, &lock)?;
    Ok(true)
}

/// SH3: shop settings change only on the admin PC, unless the shop changes
/// them from any PC.
fn may_change_settings(s: &ShopFile, who: &Who) -> Result<(), String> {
    if s.settings_by == SettingsBy::AdminPc && s.admin.pc != who.pc {
        return Err(format!("Shop settings are changed only on the admin PC ({}). Nothing was changed.", s.admin.pc));
    }
    Ok(())
}

/// What agents see of the backups (BK7).
fn backups_json(o: &backup::Overview) -> Value {
    json!({
        "folder": o.folder.as_ref().map(|f| f.display().to_string()).ok(),
        "folder_problem": o.folder.as_ref().err(),
        "last": o.last.as_ref().map(|e| json!({"file": e.name, "when": e.stamp(), "pc": e.pc})),
        "count": o.count,
        "keep_days": o.settings.keep(),
        "second_place": (!o.settings.second_place.is_empty()).then_some(&o.settings.second_place),
    })
}

/// BK7: what a backup did, in one line and for agents.
pub fn backup_said(d: &backup::Done) -> (String, Value) {
    let mut msg = format!("Backed up the Library to {} ({} file{}).", d.entry.path.display(), d.files, if d.files == 1 { "" } else { "s" });
    let second = match &d.second {
        None => Value::Null,
        Some(Ok(p)) => {
            msg.push_str(&format!(" Copied to {}.", p.display()));
            json!({"copied_to": p.display().to_string()})
        }
        Some(Err(e)) => {
            msg.push_str(&format!(" {e}"));
            json!({"problem": e})
        }
    };
    if !d.removed.is_empty() {
        msg.push_str(&format!(" Removed {} backup{} past Keep.", d.removed.len(), if d.removed.len() == 1 { "" } else { "s" }));
    }
    if !d.skipped.is_empty() {
        msg.push_str(&format!(" {} file{} couldn't be read and {} not in it: {}.", d.skipped.len(), if d.skipped.len() == 1 { "" } else { "s" }, if d.skipped.len() == 1 { "is" } else { "are" }, d.skipped.join(", ")));
    }
    let data = json!({
        "file": d.entry.path.display().to_string(),
        "folder": d.entry.path.parent().map(|p| p.display().to_string()),
        "when": d.entry.stamp(),
        "pc": d.entry.pc,
        "files": d.files,
        "bytes": d.entry.bytes,
        "second_place": second,
        "removed": d.removed,
        "not_read": d.skipped,
    });
    (msg, data)
}

/// The `shop` operation (SH6, SH7, BK7).
pub fn run(ctx: &Ctx, a: &Args) -> Result<Outcome, String> {
    let dir = &ctx.library_dir;
    match a.str("mode").as_deref().unwrap_or("status") {
        "admin" => Err(AGENT_ADMIN.into()),
        "backup" => {
            record_pc(dir, &ctx.who, &ctx.clock, &ctx.version)?;
            let d = backup_now(dir, &ctx.cfg.backups, &ctx.who, &ctx.clock)?;
            let (message, data) = backup_said(&d);
            Ok(Outcome { message, data, changed: false, files: vec![d.entry.path.clone()] })
        }
        _ => {
            // Using the program on this PC records it (SH4).
            record_pc(dir, &ctx.who, &ctx.clock, &ctx.version)?;
            let Some(st) = status(dir)? else {
                let b = backup::overview(dir, false, &ctx.cfg.backups);
                return Ok(Outcome::read(
                    format!("This PC's Library ({}) isn't a shop. Start one there with `nestpro setup --start <folder> --name <name>` or Settings → Shop → Start a shop here.", dir.display()),
                    json!({"shop": null, "folder": dir.display().to_string(), "backups": backups_json(&b)}),
                ));
            };
            let pcs: Vec<Value> = st
                .pcs
                .iter()
                .map(|p| json!({"pc": p.pc, "user": p.user, "version": p.version, "last_seen": ctx.clock.stamp(p.last_seen), "admin": p.pc == st.shop.admin.pc, "this_pc": p.pc == ctx.who.pc}))
                .collect();
            let s = &st.shop;
            Ok(Outcome::read(
                format!("Shop {} in {}: admin PC {}, {} PC(s); shop settings are changed {}.", s.name, dir.display(), s.admin.pc, pcs.len(), s.settings_by.words()),
                json!({
                    "name": s.name,
                    "folder": dir.display().to_string(),
                    "admin_pc": s.admin.pc,
                    "admin_set_by": s.admin.by.label(),
                    "admin_set_at": ctx.clock.stamp(s.admin.at),
                    "admin_was": s.admin.was,
                    "shop_settings": s.settings_by.words(),
                    "pcs": pcs,
                    "backups": backups_json(&backup::overview(dir, true, &s.backups)),
                }),
            ))
        }
    }
}

/// The `setup` operation (SH6): `start` starts a shop in a Library folder.
pub fn setup(ctx: &Ctx, a: &Args) -> Result<Outcome, String> {
    let Some(folder) = a.str("start") else {
        return Err("Say what to set up: start=<the Library folder> name=<the shop's name> starts a shop there (`nestpro setup --start \\\\srv\\Nesting --name Acme`).".into());
    };
    let dir = Path::new(&folder);
    let name = a.str("name").unwrap_or_default();
    let s = start(dir, &name, &ctx.who, &ctx.clock, &ctx.version)?;
    let mut msg = format!("Started shop {} in {}, with {} as its admin PC. Added the shop file and the Shop, Updates and Backups folders; nothing else in the Library changed.", s.name, dir.display(), s.admin.pc);
    // The same folder written another way (relative, or with `..`) is the same.
    let same = |a: &Path, b: &Path| a == b || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y);
    if !same(dir, &ctx.library_dir) {
        msg.push_str(&format!(" This PC's Library is still {}; point it at {} in Settings → Folders to work in the shop.", ctx.library_dir.display(), dir.display()));
    }
    Ok(Outcome { message: msg, data: json!({"name": s.name, "folder": dir.display().to_string(), "admin_pc": s.admin.pc}), changed: true, files: vec![shop::file(dir)] })
}
