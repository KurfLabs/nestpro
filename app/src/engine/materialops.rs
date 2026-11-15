//! The agents' `material` operation: the material list (MA14–MA16, MA19,
//! MA20), level with Settings → Materials — list it, add a material based on
//! a standard, edit (rename) one, delete one nothing uses. Every change goes
//! through the same rules and write lock as the window's.

use super::catalogue::{self, CATALOGUE};
use super::material::{self, Material};
use super::materialedit::{Change, Done};
use super::ops::{Args, Outcome};
use super::Ctx;
use serde_json::{json, Value};

fn done(message: String, data: Value) -> Outcome {
    Outcome { message, data, changed: false, files: vec![] }
}

/// One material as `list` shows it: each value, and whether it is the
/// material's own or its standard's.
pub fn material_json(m: &Material) -> Value {
    let s = m.standard();
    let (pn, pc) = m.program();
    let inherited: Vec<&str> = [("density", m.density.is_none()), ("program_name", m.program_name.is_none()), ("program_code", m.program_code.is_none()), ("machine_ref", m.machine_ref.is_none())].iter().filter(|x| x.1).map(|x| x.0).collect();
    json!({
        "material": m.id, "name": m.name, "other_names": m.other_names,
        "standard": catalogue::label(m.standard), "standard_kind": catalogue::kind_name(m.standard), "kind": m.kind().label(),
        "density": m.density(), "program_name": pn, "program_code": pc, "machine_key": m.machine_key(),
        "machine_ref": m.machine_ref.clone().unwrap_or_else(|| s.machine_ref()),
        "standard_values": {"density": s.density, "program_name": s.program.0, "program_code": s.program.1, "machine_ref": s.machine_ref()},
        "from_standard": inherited,
    })
}

/// A text value for one of a material's own values: "standard" (any case)
/// goes back to the standard's (None); anything else is its own, even empty.
fn own(a: &Args, k: &str) -> Option<Option<String>> {
    a.str(k).map(|v| if v.trim().eq_ignore_ascii_case("standard") { None } else { Some(v.trim().to_string()) })
}

fn density(a: &Args) -> Result<Option<Option<f64>>, String> {
    a.str("density").map(|v| super::materialedit::parse_density(&v)).transpose()
}

/// Take the values `a` gives into `m`.
fn apply(a: &Args, m: &mut Material) -> Result<(), String> {
    if let Some(n) = a.str("name") {
        m.name = n;
    }
    if let Some(o) = a.str("other_names") {
        m.other_names = material::split_names(&o);
    }
    if let Some(s) = a.str("standard") {
        m.standard = material::standard_from(&s)?.number;
    }
    if let Some(d) = density(a)? {
        m.density = d;
    }
    if let Some(v) = own(a, "program_name") {
        m.program_name = v;
    }
    if let Some(v) = own(a, "program_code") {
        m.program_code = v;
    }
    if let Some(v) = own(a, "machine_ref") {
        m.machine_ref = v;
    }
    Ok(())
}

/// `open`: the job open in this session, which keeps its material (JOB-88).
pub fn run(ctx: &Ctx, a: &Args, open: Option<&crate::jobfile::JobFile>) -> Result<Outcome, String> {
    let home = ctx.material_home();
    let mode = a.str("mode").unwrap_or_default();
    match mode.as_str() {
        "list" => {
            let l = material::current();
            let stored = home.file().is_file();
            let where_ = if stored { format!("{} list", home.label()) } else { format!("{} list (not stored yet: this PC's Settings → Materials seed it)", home.label()) };
            Ok(done(
                format!("{} materials, {where_}: {}.", l.materials.len(), l.materials.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(", ")),
                json!({
                    "materials": l.materials.iter().map(material_json).collect::<Vec<_>>(),
                    // MA12: the catalogue a material is added from.
                    "standards": CATALOGUE.iter().map(|e| {
                        let s = material::standard(e.number);
                        json!({"standard": e.label(), "number": format!("{:04}", e.number), "name": e.name, "category": if e.waterjet { "waterjet" } else { "punching" }, "kind": e.kind, "density": s.density, "catalogue_density": e.density, "program_name": s.program.0, "program_code": s.program.1, "machine_ref": s.machine_ref()})
                    }).collect::<Vec<_>>(),
                    "stored_in": home.file().display().to_string(),
                }),
            ))
        }
        "add" => {
            // MA21: from a standard of the catalogue (named as it unless a
            // name is given); MA22: blank, without one.
            let mut m = match a.str("standard") {
                Some(std) => catalogue::from_standard(material::standard_from(&std)?.number)?,
                None => catalogue::blank(),
            };
            if a.str("standard").is_none() && a.str("name").is_none_or(|n| n.trim().is_empty()) {
                return Err("material mode=add needs the new material's name (name), or the standard of the catalogue it is based on (standard, see material mode=list), whose name it then takes.".into());
            }
            apply(a, &mut m)?;
            let (done_, l) = change(ctx, &home, Change::Add(m), None)?;
            let Some(Done::Added(id)) = done_.into_iter().next() else { unreachable!() };
            let m = l.get(id).cloned().unwrap();
            Ok(done(format!("Added material {} ({}), based on {}: it is {} list now.", m.name, m.id, catalogue::label(m.standard), home.label()), material_json(&m)))
        }
        "edit" => {
            let which = a.str("material").ok_or("material mode=edit needs the material to change (material: its name, another name or its number; see material mode=list)")?;
            let id = super::ops::material_from(&which, &ctx.cfg)?;
            let before = material::name_or(id, "?");
            let was = material::current().get(id).cloned().ok_or_else(|| format!("Material {id} isn't in {} list any more: see material mode=list.", home.label()))?;
            let mut now = was.clone();
            apply(a, &mut now)?;
            let (_, l) = change(ctx, &home, Change::Edit { was, now }, None)?;
            let m = l.get(id).cloned().unwrap();
            // MA19: renaming changes nothing else. Files point at the number,
            // so every job, part and stock item in it shows the new name, and
            // takes it into its file only when it is next saved.
            let msg = if m.name != before {
                format!("Material {} renamed {}. Jobs, library parts and stock that use it show the new name; their files point at its number ({}) and are left as they are until next saved.", before, m.name, m.id)
            } else {
                format!("Material {} ({}) changed.", m.name, m.id)
            };
            Ok(done(msg, material_json(&m)))
        }
        "delete" => {
            let which = a.str("material").ok_or("material mode=delete needs the material to delete (material: its name, another name or its number; see material mode=list)")?;
            let id = super::ops::material_from(&which, &ctx.cfg)?;
            let (done_, _) = change(ctx, &home, Change::Delete(id), open)?;
            let Some(Done::Deleted(m)) = done_.into_iter().next() else { unreachable!() };
            Ok(done(format!("Deleted material {} ({}) from {} list. Its number is never given to another material.", m.name, m.id, home.label()), material_json(&m)))
        }
        m => Err(format!("material mode={m}: list, add, edit or delete")),
    }
}

/// Make one change to the list where it lives, as the window's Save
/// materials does (`crate::materials::apply`), and answer from the list as
/// stored.
fn change(ctx: &Ctx, home: &crate::materials::Home, c: Change, open: Option<&crate::jobfile::JobFile>) -> Result<(Vec<Done>, material::List), String> {
    let lib = ctx.library_at();
    let open: Vec<crate::materials::Open> = open.into_iter().map(Into::into).collect();
    let (d, l) = crate::materials::apply(home, &ctx.pc_dir, &ctx.cfg, || lib.saved(), &lib, &[c], &open)?;
    material::install(l.clone());
    Ok((d, l))
}
