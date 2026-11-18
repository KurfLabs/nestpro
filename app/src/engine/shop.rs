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
        backups: Default::default(),
        other: Map::new(),
    };
    if let Err(e) = shop::write(dir, &s, &lock, version) {
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
    change_admin(dir, who, clock, None, crate::updates::VERSION)
}

/// `make_admin` by a program on `version`: refused when a newer program
/// wrote the shop file last (UP7).
pub fn make_admin_on(dir: &Path, who: &Who, clock: &Clock, version: &str) -> Result<ShopFile, String> {
    change_admin(dir, who, clock, None, version)
}

/// SH7: `make_admin` as the window asks it: "`who`'s PC in place of
/// `replacing`", about the shop in `dir`. Checked under the lock: when the
/// admin PC is no longer `replacing` (another PC was made the admin PC while
/// the question was open), nothing changes and the refusal names the admin
/// PC now, so the person can ask again.
pub fn replace_admin(dir: &Path, who: &Who, clock: &Clock, replacing: &str) -> Result<ShopFile, String> {
    change_admin(dir, who, clock, Some(replacing), crate::updates::VERSION)
}

fn change_admin(dir: &Path, who: &Who, clock: &Clock, replacing: Option<&str>, version: &str) -> Result<ShopFile, String> {
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
        shop::write(dir, &s, &lock, version)?;
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

// ---------------------------------------------------------------------------
// Backups (BK1–BK7): the rules on where they go and what they hold are in
// `crate::backup`; these say whose settings they follow and who changes them.
// ---------------------------------------------------------------------------

/// BK2: the day's backup, as the program takes it when it opens: none when
/// one of today is there already (another PC, or this one, took it). It
/// follows the shop file's settings in a shop and `own` (this PC's)
/// otherwise, read once this PC holds the write lock.
pub fn daily_backup(dir: &Path, own: &backup::Settings, who: &Who, clock: &Clock) -> Result<backup::Daily, String> {
    backup::take(dir, own, who, clock, backup::Why::Daily)
}

/// BK7: Back up now (and agents' `shop mode=backup`): a backup whatever
/// was taken today.
pub fn backup_now(dir: &Path, own: &backup::Settings, who: &Who, clock: &Clock) -> Result<backup::Done, String> {
    match backup::take(dir, own, who, clock, backup::Why::Now)? {
        backup::Daily::Taken(d) => Ok(d),
        backup::Daily::AlreadyToday { entry, .. } => Err(format!("No backup was taken: {} is today's already.", entry.name)),
    }
}

/// BK7: the backups of the Library in `dir`, for Settings → Shop and agents.
pub fn backups(dir: &Path, own: &backup::Settings) -> Result<backup::Overview, String> {
    backup::overview(dir, own)
}

/// BK4, BK5, BK7: change Keep and the second place. In a shop they are shop
/// settings (SH3): changed only on the admin PC unless the shop changes
/// them from any PC, in the shop file, under the Library's write lock
/// (LB1). A PC of its own changes `own` (its settings, which the caller
/// saves). Keep is 1 to 3650 days; the second place can't be the backup
/// folder or in the Library (it would be zipped in every backup). Gives
/// whether it was the shop's.
pub fn set_backup_settings(dir: &Path, own: &mut backup::Settings, who: &Who, new: &backup::Settings, version: &str) -> Result<bool, String> {
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
    shop::write(dir, &s, &lock, version)?;
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
fn backups_json(o: &Result<backup::Overview, String>) -> Value {
    let o = match o {
        Ok(o) => o,
        Err(e) => return json!({"problem": e}),
    };
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
    let data = json!({
        "file": d.entry.path.display().to_string(),
        "folder": d.entry.path.parent().map(|p| p.display().to_string()),
        "when": d.entry.stamp(),
        "pc": d.entry.pc,
        "files": d.files,
        "bytes": d.entry.bytes,
        "second_place": second,
        "removed": d.removed,
    });
    (msg, data)
}

/// The `shop` operation (SH6, SH7, BK7).
pub fn run(ctx: &Ctx, a: &Args) -> Result<Outcome, String> {
    let dir = &ctx.library_dir;
    match a.str("mode").as_deref().unwrap_or("status") {
        // Agents never change the admin PC (SH7): the engine's own rule
        // refuses it, whoever the session says it works for.
        "admin" => make_admin(dir, &ctx.who.clone().as_agent(), &ctx.clock).map(|_| unreachable!("an agent changed the admin PC")),
        "backup" => {
            if let Some(why) = ahead_of_shop(dir, &ctx.version) {
                return Err(format!("No backup was taken: the shop is read-only on this PC. {why}"));
            }
            let d = backup_now(dir, &ctx.cfg.backups, &ctx.who, &ctx.clock)?;
            let (message, data) = backup_said(&d);
            Ok(Outcome { message, data, changed: false, files: vec![d.entry.path.clone()] })
        }
        "updates" => Ok(super::updates::run(ctx)),
        // UP19: rolling out stays with a person on the admin PC: the
        // engine's own rule refuses it, whoever the session says it works for.
        "rollout" => super::rollout::rollout(dir, Path::new(""), &ctx.who.clone().as_agent(), &ctx.version).map(|_| unreachable!("an agent rolled out a release")),
        // UP23: so is rolling back.
        "rollback" => super::rollout::rollback(dir, &ctx.who.clone().as_agent(), &ctx.version).map(|_| unreachable!("an agent rolled the shop back")),
        _ => {
            // Using the program on this PC records it (SH4). When it can't,
            // the shop is still shown, with why this PC isn't in it (SH6).
            // Only a record written counts: Ok(false) means the folder was
            // no shop when this PC looked, so it wasn't added (a shop
            // started a moment later is shown, without this PC).
            let unrecorded = match record_pc(dir, &ctx.who, &ctx.clock, &ctx.version) {
                Ok(true) => None,
                Ok(false) => Some(format!("{} wasn't added to the PCs list: there was no shop here when it looked. Call shop again.", ctx.who.pc)),
                Err(e) => Some(e),
            };
            crate::shared::pause("pc recorded");
            let Some(st) = status(dir)? else {
                let b = backup::overview(dir, &ctx.cfg.backups);
                return Ok(Outcome::read(
                    format!("This PC's Library ({}) isn't a shop. Start one there with `nestpro setup --start <folder> --name <name>` or Settings → Shop → Start a shop here.", dir.display()),
                    json!({"shop": null, "folder": dir.display().to_string(), "backups": backups_json(&b)}),
                ));
            };
            let s = &st.shop;
            let sv = s.shop_version();
            // UP22: each PC's version against the shop version.
            let pcs: Vec<Value> = st
                .pcs
                .iter()
                .map(|p| {
                    let (behind, ahead) = against(&p.version, sv);
                    json!({"pc": p.pc, "user": p.user, "version": p.version, "last_seen": ctx.clock.stamp(p.last_seen), "admin": p.pc == st.shop.admin.pc, "this_pc": p.pc == ctx.who.pc, "behind": behind, "ahead": ahead})
                })
                .collect();
            let note = unrecorded.as_ref().map_or(String::new(), |e| format!(" {e}"));
            let behind: Vec<&str> = st.pcs.iter().filter(|p| against(&p.version, sv).0).map(|p| p.pc.as_str()).collect();
            let versions = match sv {
                None => " No shop version is rolled out yet.".to_string(),
                Some(v) if behind.is_empty() => format!(" Shop version {v}."),
                Some(v) => format!(" Shop version {v}; behind it: {}.", behind.join(", ")),
            };
            Ok(Outcome::read(
                format!("Shop {} in {}: admin PC {}, {} PC(s); shop settings are changed {}.{versions}{note}", s.name, dir.display(), s.admin.pc, pcs.len(), s.settings_by.words()),
                json!({
                    "name": s.name,
                    "folder": dir.display().to_string(),
                    "admin_pc": s.admin.pc,
                    "admin_set_by": s.admin.by.label(),
                    "admin_set_at": ctx.clock.stamp(s.admin.at),
                    "admin_was": s.admin.was,
                    "shop_settings": s.settings_by.words(),
                    "shop_version": sv,
                    "previous_shop_version": s.previous_version(),
                    "rolled_back_from": s.rolled_back_from(),
                    "releases_in_updates": super::rollout::in_updates(dir).unwrap_or_default(),
                    "pcs": pcs,
                    "this_pc_recorded": unrecorded.is_none(),
                    "this_pc_not_recorded": unrecorded,
                    "backups": backups_json(&backup::overview(dir, &ctx.cfg.backups)),
                }),
            ))
        }
    }
}

/// UP21 (S9): why a PC on `version` writes nothing in the shop in `dir`:
/// it runs a release newer than the shop version, outside a trial. Such a
/// PC starts no backup and holds no job; only its own record in the PCs
/// list is written (SH4), so the list shows it ahead.
pub fn ahead_of_shop(dir: &Path, version: &str) -> Option<String> {
    shop::versions(dir, version).ahead().map(|n| n.message())
}

/// UP22: a PC on `version` against the shop version `shop`: (behind,
/// ahead). A version that can't be compared counts as behind; nothing is
/// behind or ahead while the shop has no shop version.
pub fn against(version: &str, shop: Option<&str>) -> (bool, bool) {
    let Some(sv) = shop else { return (false, false) };
    match crate::shared::version_order(version, sv) {
        Some(std::cmp::Ordering::Equal) => (false, false),
        Some(std::cmp::Ordering::Greater) => (false, true),
        _ => (true, false),
    }
}

/// What `setup` did, and the Library folder the session works in from now
/// on when it joined a shop (SH10).
pub struct Setup {
    pub outcome: Outcome,
    pub library: Option<std::path::PathBuf>,
}

/// The `setup` operation (SH6, SH15): `start` starts a shop in a Library
/// folder, `join` joins one.
pub fn setup(ctx: &Ctx, a: &Args) -> Result<Outcome, String> {
    setup_in(ctx, a).map(|s| s.outcome)
}

/// `setup`, saying which Library the session works in afterwards.
pub fn setup_in(ctx: &Ctx, a: &Args) -> Result<Setup, String> {
    match (a.str("start"), a.str("join")) {
        (Some(_), Some(_)) => Err("Give start= or join=, not both. Nothing was changed.".into()),
        (None, Some(folder)) => join_op(ctx, &folder),
        (Some(folder), None) => start_op(ctx, &folder, &a.str("name").unwrap_or_default()).map(|outcome| Setup { outcome, library: None }),
        (None, None) => Err("Say what to set up: join=<the shop folder> joins a shop (`nestpro setup --join \\\\srv\\Nesting`); start=<the Library folder> name=<the shop's name> starts one there (`nestpro setup --start \\\\srv\\Nesting --name Acme`).".into()),
    }
}

/// `setup join=` (SH10–SH15): the checks, then this PC's Library setting.
fn join_op(ctx: &Ctx, folder: &str) -> Result<Setup, String> {
    let j = join(folder, &ctx.library_dir, &ctx.who, &ctx.clock, &ctx.version).map_err(|r| r.text())?;
    if !j.already {
        use_library_folder(&ctx.pc_dir, &ctx.cfg, &j.folder).map_err(|e| {
            format!("Not joined: the checks passed and {} is in shop {}'s PCs list, but {e}. This PC's Library is still {}. Try again.\n{}", ctx.who.pc, j.shop.name, ctx.library_dir.display(), check_lines(&j.checks))
        })?;
    }
    let s = &j.shop;
    let data = json!({
        "joined": true,
        "name": s.name,
        "folder": j.folder.display().to_string(),
        "admin_pc": s.admin.pc,
        "already_in_shop": j.already,
        "left_shop": j.left.as_ref().map(|(n, d)| json!({"name": n, "folder": d.display().to_string()})),
        "shop_version": shop_version(s),
        "this_pc_version": ctx.version,
        "update": j.update,
        "this_pc_recorded": j.unrecorded.is_none(),
        "checks": j.checks.iter().map(Check::to_json).collect::<Vec<_>>(),
    });
    let outcome = Outcome { message: format!("{}\n{}", j.summary(), check_lines(&j.checks)), data, changed: true, files: vec![ctx.pc_dir.join(crate::settings::FILE_NAME)] };
    Ok(Setup { outcome, library: Some(j.folder) })
}

/// `setup start= name=` (SH1).
fn start_op(ctx: &Ctx, folder: &str, name: &str) -> Result<Outcome, String> {
    let dir = Path::new(folder);
    let Started { shop: s, unrecorded } = start_shop(dir, name, &ctx.who, &ctx.clock, &ctx.version)?;
    let mut msg = format!("Started shop {} in {}, with {} as its admin PC. Added the shop file and the Shop, Updates and Backups folders; nothing else in the Library changed.", s.name, dir.display(), s.admin.pc);
    if let Some(e) = &unrecorded {
        msg.push_str(&format!(" {e}"));
    }
    if !shop::same_folder(dir, &ctx.library_dir) {
        msg.push_str(&format!(" This PC's Library is still {}; point it at {} in Settings → Folders to work in the shop.", ctx.library_dir.display(), dir.display()));
    }
    Ok(Outcome { message: msg, data: json!({"name": s.name, "folder": dir.display().to_string(), "admin_pc": s.admin.pc, "this_pc_recorded": unrecorded.is_none()}), changed: true, files: vec![shop::file(dir)] })
}

// ---------------------------------------------------------------------------
// Joining a shop (SH10–SH15): this PC points its Library at the shop folder,
// after checking it can work there.
// ---------------------------------------------------------------------------

/// One check before joining (SH11), as the window and agents show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    /// Short key for agents: `folder`, `reachable`, `library`, `shop`,
    /// `lock`, `write`, `version`.
    pub what: &'static str,
    pub mark: Mark,
    pub said: String,
}

/// How a check came out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Ok,
    /// Passed, with something to know (the folder was renamed to its full
    /// path, this PC runs another version than the shop).
    Note,
    /// Failed: nothing is joined.
    No,
}

impl Check {
    fn new(what: &'static str, mark: Mark, said: impl Into<String>) -> Check {
        Check { what, mark, said: said.into() }
    }

    /// `[ok] It holds a Library.`, as `nestpro setup --join` prints it.
    pub fn line(&self) -> String {
        let m = match self.mark {
            Mark::Ok => "[ok]",
            Mark::Note => "[!!]",
            Mark::No => "[no]",
        };
        format!("{m} {}", self.said)
    }

    pub fn to_json(&self) -> Value {
        json!({"check": self.what, "ok": self.mark != Mark::No, "note": self.mark == Mark::Note, "said": self.said})
    }
}

/// Every check's line, one under the other.
pub fn check_lines(checks: &[Check]) -> String {
    checks.iter().map(|c| format!("  {}", c.line())).collect::<Vec<_>>().join("\n")
}

/// A join that was refused: why, and the checks up to the one that failed.
#[derive(Clone, Debug)]
pub struct Refused {
    pub why: String,
    pub checks: Vec<Check>,
}

impl Refused {
    /// The refusal, then every check made (SH15).
    pub fn text(&self) -> String {
        if self.checks.is_empty() {
            return self.why.clone();
        }
        format!("{}\n{}", self.why, check_lines(&self.checks))
    }
}

/// A shop joined (SH10).
#[derive(Clone, Debug)]
pub struct Joined {
    pub shop: ShopFile,
    /// The folder as this PC now names it: its Library folder setting.
    pub folder: std::path::PathBuf,
    pub checks: Vec<Check>,
    /// "Update this PC: the shop is on 0.5.0", when this PC runs an older
    /// version than the shop's (SH12).
    pub update: Option<String>,
    /// This PC's Library was already that folder (SH14).
    pub already: bool,
    /// The shop this PC was in before, and its folder, when it was another
    /// one (SH14).
    pub left: Option<(String, std::path::PathBuf)>,
    /// Why this PC couldn't be added to the PCs list, when it couldn't.
    pub unrecorded: Option<String>,
}

/// The shop version in a shop file (rolled out by the admin PC, ADR 0006;
/// what the version guard judges files by, UP8):
/// None while the shop has none yet.
pub fn shop_version(s: &ShopFile) -> Option<&str> {
    s.shop_version()
}

/// SH12: this PC's version against the shop's, as a check; None when there
/// is nothing to compare (the shop has no version yet, or either can't be
/// read as a version). The second value is the update line for an older PC.
fn version_check(shop: Option<&str>, mine: &str) -> Option<(Check, Option<String>)> {
    let shop = shop?;
    // UP6: the version guard's order (a missing part is 0).
    Some(match crate::shared::version_order(mine, shop)? {
        std::cmp::Ordering::Equal => (Check::new("version", Mark::Ok, format!("This PC runs the shop version, {mine}.")), None),
        std::cmp::Ordering::Less => {
            let update = format!("Update this PC: the shop is on {shop}.");
            (Check::new("version", Mark::Note, format!("This PC runs {mine}, older than the shop version {shop}. It may join. {update}")), Some(update))
        }
        std::cmp::Ordering::Greater => (Check::new("version", Mark::Note, format!("This PC runs {mine}, newer than the shop version {shop}. The admin PC decides which version the shop runs.")), None),
    })
}

/// SH10, SH11: join the shop in the folder `typed` (as typed or picked),
/// as `who`, whose Library is `current` now. The folder is tidied and named
/// on its share (SH13), then checked, in order, stopping at the first that
/// fails: it can be reached; it holds a Library; it holds a shop file; this
/// PC can take the Library's write lock there; and, under that lock, it can
/// write and remove a file there (a probe, written only into the folder that
/// is there). Nothing is made in a folder that fails a check (the lock file
/// and the probe are gone again). Then this PC's version is compared with the
/// shop's (SH12) and, still under the lock, this PC adds itself to the PCs
/// list (SH4). Changing this PC's Library setting is the caller's: the window
/// and `setup` do it.
pub fn join(typed: &str, current: &Path, who: &Who, clock: &Clock, version: &str) -> Result<Joined, Refused> {
    let mut checks = vec![];
    let tidy = shop::tidy_folder(typed, cfg!(windows));
    if tidy.is_empty() {
        return Err(Refused { why: "Give the shop folder to join (join=\\\\srv\\Nesting). Nothing was changed.".into(), checks });
    }
    let (full, renamed) = shop::full_folder(&tidy);
    if let Some(note) = renamed {
        checks.push(Check::new("folder", Mark::Note, note));
    }
    let dir = std::path::PathBuf::from(&full);
    let refuse = |checks: &mut Vec<Check>, what: &'static str, why: String| -> Result<Joined, Refused> {
        checks.push(Check::new(what, Mark::No, why.clone()));
        Err(Refused { why: format!("Not joined: {why} Nothing was changed, and nothing was made in {}.", dir.display()), checks: std::mem::take(checks) })
    };
    // SH11, SH8: a folder that isn't there is said so, never made.
    if !dir.is_dir() {
        return refuse(&mut checks, "reachable", format!("{} can't be reached. Check the folder's name, and that this PC is on the network (or the VPN).", dir.display()));
    }
    checks.push(Check::new("reachable", Mark::Ok, format!("{} can be reached.", dir.display())));
    if !crate::library::holds_library(&dir) {
        return refuse(&mut checks, "library", format!("{} holds no Library, so it isn't a shop folder. Pick the folder the shop was started in.", dir.display()));
    }
    checks.push(Check::new("library", Mark::Ok, "It holds a Library."));
    let s = match shop::read(&dir) {
        Err(e) => return refuse(&mut checks, "shop", e),
        Ok(None) => return refuse(&mut checks, "shop", format!("{} holds a Library but no shop. Start the shop there first, on the PC that should be its admin PC (Settings → Shop → Start a shop here, or `nestpro setup --start`).", dir.display())),
        Ok(Some(s)) => s,
    };
    checks.push(Check::new("shop", Mark::Ok, format!("It is the folder of shop {} (admin PC {}).", s.name, s.admin.pc)));
    let lock = match WriteLock::take(&dir) {
        Ok(l) => l,
        Err(e) => return refuse(&mut checks, "lock", format!("This PC can't take the Library's write lock there ({e}). Check that this PC's user may change files there: the shop's group needs Modify rights.")),
    };
    crate::shared::pause("join locked");
    // A probe file, written as the shop's files are (only into the folder
    // that is there) and removed again: this PC can make, replace and remove
    // files there.
    let probe = dir.join(format!("join-check-{}.tmp", crate::shared::writer_tag()));
    let wrote = crate::shared::write_existing(&probe, b"nestpro: this PC checked it can write here, and removed this file again.\n", &lock);
    let removed = if wrote.is_ok() { std::fs::remove_file(&probe).map_err(|e| e.to_string()) } else { Ok(()) };
    if let Err(e) = wrote {
        return refuse(&mut checks, "write", format!("This PC can't write in {} ({e}). Check that this PC's user may change files there: the shop's group needs Modify rights.", dir.display()));
    }
    if let Err(e) = removed {
        // The one file a refused join can leave: said, naming it.
        let why = format!("This PC can make files in {} but can't remove them ({e}). The shop's group needs Modify rights, not only Write.", dir.display());
        checks.push(Check::new("write", Mark::No, why.clone()));
        return Err(Refused { why: format!("Not joined: {why} Nothing was changed; remove the check file {} by hand.", probe.display()), checks });
    }
    checks.push(Check::new("write", Mark::Ok, "This PC can make, change and remove files there."));
    if let Err(e) = lock.still_held() {
        return refuse(&mut checks, "lock", format!("The Library's write lock didn't hold there ({e}). Try again; if it keeps failing, the share may not keep files as this program needs."));
    }
    checks.push(Check::new("lock", Mark::Ok, "The Library's write lock works there."));
    let (vc, update) = version_check(shop_version(&s), version).map_or((None, None), |(c, u)| (Some(c), u));
    checks.extend(vc);
    let unrecorded = write_record(&dir, who, clock.now(), version, &lock).err().map(|e| unrecorded(who, &e));
    drop(lock);
    let already = shop::same_folder(current, &dir);
    let left = if already { None } else { shop::read(current).ok().flatten().map(|o| (o.name, current.to_path_buf())) };
    Ok(Joined { shop: s, folder: dir, checks, update, already, left, unrecorded })
}

impl Joined {
    /// What joining says (SH10, SH12, SH14), without the checks.
    pub fn summary(&self) -> String {
        let s = &self.shop;
        let mut msg = if self.already {
            format!("This PC is already in shop {} ({}): its Library folder stays as it is.", s.name, self.folder.display())
        } else {
            format!("Joined shop {} in {} (admin PC {}). This PC's Library is now that folder; its own Library is left as it was, and its materials join the shop's list.", s.name, self.folder.display(), s.admin.pc)
        };
        if let Some((name, dir)) = &self.left {
            msg.push_str(&format!(" It left shop {name} ({}); join it again with `nestpro setup --join {}`.", dir.display(), dir.display()));
        }
        if let Some(u) = &self.update {
            msg.push_str(&format!(" {u}"));
        }
        if let Some(e) = &self.unrecorded {
            msg.push_str(&format!(" {e}"));
        }
        msg
    }
}

/// SH10: this PC's Library folder setting becomes `folder`, in the settings
/// file of its settings folder `pc_dir` (read again first, so nothing else
/// in it changes). A PC with no settings file yet starts from `cfg`.
pub fn use_library_folder(pc_dir: &Path, cfg: &crate::settings::AppSettings, folder: &Path) -> Result<(), String> {
    let f = pc_dir.join(crate::settings::FILE_NAME);
    let mut s = crate::settings::AppSettings::read_from(&f).unwrap_or_else(|| cfg.clone());
    s.library_dir = folder.display().to_string();
    s.save_to(&f).map_err(|e| format!("this PC's settings ({}) can't be saved ({e})", f.display()))
}
