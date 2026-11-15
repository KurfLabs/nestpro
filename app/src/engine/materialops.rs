//! The agents' `material` operation: the material list (MA14–MA16), level
//! with Settings → Materials — list it, add a material based on a standard,
//! edit one. Every change goes through the same rules and write lock as the
//! window's.

use super::material::{self, Material, STANDARDS};
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
        "standard": s.label(), "kind": m.kind().label(),
        "density": m.density(), "program_name": pn, "program_code": pc, "machine_key": m.machine_key(),
        "machine_ref": m.machine_ref.clone().unwrap_or_else(|| s.machine_ref()),
        "from_standard": inherited,
    })
}

/// A text value for one of a material's own values: "standard" (any case)
/// goes back to the standard's (None); anything else is its own, even empty.
fn own(a: &Args, k: &str) -> Option<Option<String>> {
    a.str(k).map(|v| if v.trim().eq_ignore_ascii_case("standard") { None } else { Some(v.trim().to_string()) })
}

fn density(a: &Args) -> Result<Option<Option<f64>>, String> {
    let Some(v) = a.str("density") else { return Ok(None) };
    if v.trim().is_empty() || v.trim().eq_ignore_ascii_case("standard") {
        return Ok(Some(None));
    }
    match a.num("density")? {
        Some(d) if d > 0.0 && d < 30.0 => Ok(Some(Some(d))),
        Some(d) => Err(format!("density {d}: a weight per volume in g/cm³, above 0 and below 30 (e.g. 7.85 for steel)")),
        None => Ok(None),
    }
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

pub fn run(ctx: &Ctx, a: &Args) -> Result<Outcome, String> {
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
                    "standards": STANDARDS.iter().map(|s| json!({"standard": s.label(), "kind": s.kind.label(), "density": s.density, "program_name": s.program.0, "program_code": s.program.1, "machine_ref": s.machine_ref()})).collect::<Vec<_>>(),
                    "stored_in": home.file().display().to_string(),
                }),
            ))
        }
        "add" => {
            let name = a.str("name").ok_or("material mode=add needs the new material's name (name)")?;
            let std = a.str("standard").ok_or_else(|| format!("material mode=add needs the standard it is based on (standard): {}", STANDARDS.iter().map(|s| s.label()).collect::<Vec<_>>().join(", ")))?;
            let mut m = Material { id: 0, name, other_names: vec![], standard: material::standard_from(&std)?.number, density: None, program_name: None, program_code: None, machine_ref: None };
            apply(a, &mut m)?;
            let (id, l) = crate::materials::update(&home, &ctx.pc_dir, &ctx.cfg, || ctx.library_at().saved(), |l| l.add(m))?;
            material::install(l.clone());
            let m = l.get(id).cloned().unwrap();
            Ok(done(format!("Added material {} ({}), based on {}: it is {} list now.", m.name, m.id, m.standard().label(), home.label()), material_json(&m)))
        }
        "edit" => {
            let which = a.str("material").ok_or("material mode=edit needs the material to change (material: its name, another name or its number; see material mode=list)")?;
            let id = super::ops::material_from(&which, &ctx.cfg)?;
            let (_, l) = crate::materials::update(&home, &ctx.pc_dir, &ctx.cfg, || ctx.library_at().saved(), |l| {
                let mut m = l.get(id).cloned().ok_or_else(|| format!("Material {id} isn't in {} list any more: see material mode=list.", home.label()))?;
                apply(a, &mut m)?;
                l.replace(m)
            })?;
            material::install(l.clone());
            let m = l.get(id).cloned().unwrap();
            Ok(done(format!("Material {} ({}) changed.", m.name, m.id), material_json(&m)))
        }
        m => Err(format!("material mode={m}: list, add or edit")),
    }
}
