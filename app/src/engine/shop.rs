//! The shop's rules (spec #178, SH1–SH8): starting a shop in a Library's
//! folder, each PC recording itself, the admin PC, and what the shop looks
//! like. Plain functions over a shop folder: Settings → Shop and the agents'
//! `shop` and `setup` operations call them.

use super::ops::{Args, Outcome};
use super::Ctx;
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
/// Made under the Library's write lock (LB1). `start_shop` also says
/// whether the admin PC got into the PCs list (SH4).
pub fn start(dir: &Path, name: &str, who: &Who, clock: &Clock, version: &str) -> Result<ShopFile, String> {
    start_shop(dir, name, who, clock, version).map(|st| st.shop)
}

/// A shop just started, and whether its admin PC is in its PCs list yet.
pub struct Started {
    pub shop: ShopFile,
    /// Why the admin PC couldn't be added to the PCs list, when it couldn't:
    /// the shop is started all the same, and the PC adds itself again when
    /// the program next opens, at the next `shop` call, or with Add this PC
    /// on Settings → Shop (SH4).
    pub unrecorded: Option<String>,
}

/// `start`, saying whether the admin PC could be added to the PCs list.
pub fn start_shop(dir: &Path, name: &str, who: &Who, clock: &Clock, version: &str) -> Result<Started, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the shop a name (name=Acme). Nothing was made.".into());
    }
    crate::library::usable(dir)?;
    if let Some(s) = shop::read(dir)? {
        return Err(already(dir, &s));
    }
    let lock = WriteLock::take(dir).map_err(|e| cant_write(dir, &e))?;
    crate::shared::pause("shop locked");
    // Looked at again under the lock: the share may have dropped (nothing is
    // made in its place, SH8), or another PC started a shop meanwhile.
    crate::library::usable(dir)?;
    if let Some(s) = shop::read(dir)? {
        return Err(already(dir, &s));
    }
    // A folder made here is taken away again if the shop can't be started,
    // but only while the lock is still this PC's: once it isn't, another PC
    // may have started the shop with these folders, so they stay (a later
    // start uses them as they are).
    let mut made = vec![];
    let undo = |made: &[std::path::PathBuf]| {
        for d in made.iter().rev() {
            if lock.still_held().is_err() {
                return;
            }
            let _ = std::fs::remove_dir(d);
        }
    };
    for f in shop::FOLDERS {
        let d = dir.join(f);
        if !d.is_dir() {
            // Made only inside a shop folder that is there, never in its place.
            if let Err(e) = lock.still_held().and_then(|_| std::fs::create_dir(&d).map_err(|e| e.to_string())) {
                undo(&made);
                return Err(cant_write(dir, &e));
            }
            made.push(d);
        }
    }
    crate::shared::pause("shop folders made");
    let at = clock.now();
    let s = ShopFile {
        name: name.to_string(),
        admin: Admin { pc: who.pc.clone(), by: who.clone(), at, was: None },
        settings_by: SettingsBy::AdminPc,
        started: Saved { by: who.clone(), at },
        other: Map::new(),
    };
    if let Err(e) = shop::write(dir, &s, &lock) {
        undo(&made);
        return Err(cant_write(dir, &e));
    }
    // The PC that started it is in it (SH4). The shop is started either way;
    // when its record can't be written, that is said, not hidden.
    let unrecorded = write_record(dir, who, at, version, &lock).err().map(|e| unrecorded(who, &e));
    Ok(Started { shop: s, unrecorded })
}

/// "PC-A couldn't be added to the shop's PCs list (…). It adds itself again…"
fn unrecorded(who: &Who, e: &str) -> String {
    format!("{} couldn't be added to the shop's PCs list ({e}). It adds itself again when the program next opens, or with Add this PC on Settings → Shop.", who.pc)
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
    write_record(dir, who, clock.now(), version, &lock).map_err(|e| unrecorded(who, &e))?;
    Ok(true)
}

/// SH7: make `who`'s PC the admin PC, recording who did it and when. Only a
/// person, in the window, after a warning: an agent is refused with the
/// reason and nothing changes. Under the Library's write lock (LB1).
pub fn make_admin(dir: &Path, who: &Who, clock: &Clock) -> Result<ShopFile, String> {
    change_admin(dir, who, clock, None)
}

/// SH7: `make_admin` as the window asks it: "`who`'s PC in place of
/// `replacing`", about the shop in `dir`. Checked under the lock: when the
/// admin PC is no longer `replacing` (another PC was made the admin PC while
/// the question was open), nothing changes and the refusal names the admin
/// PC now, so the person can ask again.
pub fn replace_admin(dir: &Path, who: &Who, clock: &Clock, replacing: &str) -> Result<ShopFile, String> {
    change_admin(dir, who, clock, Some(replacing))
}

fn change_admin(dir: &Path, who: &Who, clock: &Clock, replacing: Option<&str>) -> Result<ShopFile, String> {
    if who.agent {
        return Err(AGENT_ADMIN.into());
    }
    let not_shop = || format!("{} isn't a shop. Nothing was changed.", dir.display());
    shop::read(dir)?.ok_or_else(not_shop)?;
    let lock = WriteLock::take(dir)?;
    let mut s = shop::read(dir)?.ok_or_else(not_shop)?;
    if let Some(r) = replacing.filter(|r| *r != s.admin.pc) {
        let a = &s.admin;
        return Err(format!("{} was made the admin PC of {} by {}, {}, while you were asking to replace {r}. Nothing was changed. Look again and ask again if this PC should still be the admin PC.", a.pc, s.name, a.by.label(), clock.shown(a.at)));
    }
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
    match shop::read(dir)? {
        None => Ok(None),
        Some(shop) => Ok(Some(Status { pcs: shop::records(dir)?, shop })),
    }
}

impl Status {
    /// "PC-B made the admin PC by kim on PC-B, 9 Nov 09:20 (was PC-A)".
    pub fn admin_note(&self, clock: &Clock) -> String {
        let a = &self.shop.admin;
        let was = a.was.as_ref().map_or(String::new(), |w| format!(" (was {w})"));
        format!("{} made the admin PC by {}, {}{was}", a.pc, a.by.label(), clock.shown(a.at))
    }
}

/// The `shop` operation (SH6, SH7).
pub fn run(ctx: &Ctx, a: &Args) -> Result<Outcome, String> {
    let dir = &ctx.library_dir;
    match a.str("mode").as_deref().unwrap_or("status") {
        // Agents never change the admin PC (SH7): the engine's own rule
        // refuses it, whoever the session says it works for.
        "admin" => make_admin(dir, &ctx.who.clone().as_agent(), &ctx.clock).map(|_| unreachable!("an agent changed the admin PC")),
        "updates" => Ok(super::updates::run(ctx)),
        _ => {
            // Using the program on this PC records it (SH4). When it can't,
            // the shop is still shown, with why this PC isn't in it (SH6).
            let unrecorded = match record_pc(dir, &ctx.who, &ctx.clock, &ctx.version) {
                Ok(_) => None,
                Err(e) => Some(e),
            };
            let Some(st) = status(dir)? else {
                return Ok(Outcome::read(
                    format!("This PC's Library ({}) isn't a shop. Start one there with `nestpro setup --start <folder> --name <name>` or Settings → Shop → Start a shop here.", dir.display()),
                    json!({"shop": null, "folder": dir.display().to_string()}),
                ));
            };
            let pcs: Vec<Value> = st
                .pcs
                .iter()
                .map(|p| json!({"pc": p.pc, "user": p.user, "version": p.version, "last_seen": ctx.clock.stamp(p.last_seen), "admin": p.pc == st.shop.admin.pc, "this_pc": p.pc == ctx.who.pc}))
                .collect();
            let s = &st.shop;
            let note = unrecorded.as_ref().map_or(String::new(), |e| format!(" {e}"));
            Ok(Outcome::read(
                format!("Shop {} in {}: admin PC {}, {} PC(s); shop settings are changed {}.{note}", s.name, dir.display(), s.admin.pc, pcs.len(), s.settings_by.words()),
                json!({
                    "name": s.name,
                    "folder": dir.display().to_string(),
                    "admin_pc": s.admin.pc,
                    "admin_set_by": s.admin.by.label(),
                    "admin_set_at": ctx.clock.stamp(s.admin.at),
                    "admin_was": s.admin.was,
                    "shop_settings": s.settings_by.words(),
                    "pcs": pcs,
                    "this_pc_recorded": unrecorded.is_none(),
                    "this_pc_not_recorded": unrecorded,
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
    let Started { shop: s, unrecorded } = start_shop(dir, &name, &ctx.who, &ctx.clock, &ctx.version)?;
    let mut msg = format!("Started shop {} in {}, with {} as its admin PC. Added the shop file and the Shop, Updates and Backups folders; nothing else in the Library changed.", s.name, dir.display(), s.admin.pc);
    if let Some(e) = &unrecorded {
        msg.push_str(&format!(" {e}"));
    }
    // The same folder written another way (relative, or with `..`) is the same.
    let same = |a: &Path, b: &Path| a == b || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y);
    if !same(dir, &ctx.library_dir) {
        msg.push_str(&format!(" This PC's Library is still {}; point it at {} in Settings → Folders to work in the shop.", ctx.library_dir.display(), dir.display()));
    }
    Ok(Outcome { message: msg, data: json!({"name": s.name, "folder": dir.display().to_string(), "admin_pc": s.admin.pc, "this_pc_recorded": unrecorded.is_none()}), changed: true, files: vec![shop::file(dir)] })
}
