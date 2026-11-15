//! The standard material catalogue (MA12): the standards a material can be
//! based on, built in and not edited by the shop, and the two ways
//! Settings → Materials and the `material` operation start a new material:
//! from a standard (MA21) or blank (MA22).

use super::material::{Kind, Material, Standard};

/// One standard of the catalogue (MA12).
pub struct Entry {
    /// Catalogue number (a material's `standard`, and its machine-file key).
    pub number: u32,
    /// The name the catalogue's file gives it.
    pub file_name: &'static str,
    /// The name shown (in English where the file's is Spanish).
    pub name: &'static str,
    /// A waterjet standard (else punching, which covers the laser).
    pub waterjet: bool,
    /// Its kind; the waterjet standards have none.
    pub kind: Option<&'static str>,
    /// Weight per volume, kg/dm³ (= g/cm³); eight have none.
    pub density: Option<f64>,
}

const fn p(number: u32, file_name: &'static str, name: &'static str, kind: &'static str, density: f64) -> Entry {
    Entry { number, file_name, name, waterjet: false, kind: Some(kind), density: Some(density) }
}

const fn w(number: u32, file_name: &'static str, name: &'static str, density: Option<f64>) -> Entry {
    Entry { number, file_name, name, waterjet: true, kind: None, density }
}

/// MA12: the 34 standards the original ships, in its order. Two values are
/// corrected on purpose (the user's decision): wood weighs 0.8 (the
/// original: 800) and the aluminium kind is spelled "Aluminium alloy".
pub const CATALOGUE: [Entry; 34] = [
    p(10, "RST 37-2", "RST 37-2", "Carbon Steel", 7.8),
    p(20, "ST 42", "ST 42", "Carbon Steel", 7.9),
    p(30, "ST 44-2", "ST 44-2", "Carbon Steel", 7.9),
    p(40, "ST 52-3", "ST 52-3", "Carbon Steel", 7.9),
    p(50, "AP 12", "AP 12", "Carbon Steel", 7.9),
    p(60, "DC04", "DC04", "Carbon Steel", 7.9),
    p(70, "S 380 MC", "S 380 MC", "Carbon Steel", 7.9),
    p(80, "S 460 MC", "S 460 MC", "Carbon Steel", 7.9),
    p(100, "X5CrNi", "X5CrNi", "Stainless steel", 8.0),
    p(110, "X3CrNiMo 17-13-3", "X3CrNiMo 17-13-3", "Stainless steel", 8.0),
    p(200, "Al99", "Al99", "Aluminium alloy", 2.7),
    p(210, "AlMg3", "AlMg3", "Aluminium alloy", 2.7),
    p(300, "Galvanizado", "Galvanized", "Steel", 7.9),
    p(310, "DC01+ZE", "DC01+ZE", "Steel", 7.9),
    w(400, "Titanio", "Titanium", None),
    w(410, "Inconel 600", "Inconel", None),
    w(500, "Absorbedor acustico", "Acoustic Absorber", Some(2.1)),
    w(510, "Cobre", "Copper", Some(8.9)),
    w(520, "Gres", "Sandstone", Some(1.9)),
    w(530, "Nylon", "Nylon", Some(2.1)),
    w(540, "Madera", "Wood", Some(0.8)),
    w(550, "Marmol", "Marble", Some(2.8)),
    w(560, "Cristal", "Glass", Some(2.6)),
    w(570, "Alabastro", "Alabaster", Some(2.4)),
    w(580, "Pizarra", "Slate", Some(2.8)),
    w(590, "Laton", "Brass", Some(8.5)),
    w(600, "Arenisca", "Gritty", Some(2.6)),
    w(610, "Linoleo", "linoleum", Some(1.2)),
    w(620, "Metacrilato", "Plexiglas", None),
    w(630, "Goma", "Rubber", None),
    w(640, "Grafito", "Graphite", None),
    w(650, "Granito", "Granite", None),
    w(660, "Papel", "Paper", None),
    w(670, "PVC", "PVC", None),
];

impl Entry {
    /// "0110 X3CrNiMo 17-13-3".
    pub fn label(&self) -> String {
        format!("{:04} {}", self.number, self.name)
    }

    /// The app's kind for it (stock pictures, estimated speeds): steels as
    /// mild steel, stainless as stainless, aluminium alloys as aluminium;
    /// a standard with no kind as mild steel, as any unknown one always was
    /// (MA22, estimated).
    pub fn app_kind(&self) -> Kind {
        match self.kind {
            Some("Stainless steel") => Kind::Stainless,
            Some("Aluminium alloy") => Kind::Aluminium,
            _ => Kind::MildSteel,
        }
    }

    /// "Stainless steel · 8 kg/dm³" (what the page's standard picker shows
    /// beside the name).
    pub fn details(&self) -> String {
        let kind = self.kind.unwrap_or(if self.waterjet { "waterjet" } else { "no kind" });
        match self.density {
            Some(d) => format!("{kind} · {d} kg/dm³"),
            None => format!("{kind} · no weight given"),
        }
    }
}

pub fn entry(number: u32) -> Option<&'static Entry> {
    CATALOGUE.iter().find(|e| e.number == number)
}

/// A standard's label wherever a material's standard is shown: the
/// catalogue's ("0100 X5CrNi"), else its number ("9999, not in the
/// catalogue").
pub fn label(number: u32) -> String {
    entry(number).map_or_else(|| format!("{number:04}, not in the catalogue"), Entry::label)
}

/// The kind a material's standard gives, as the page shows it: the
/// catalogue's ("Carbon Steel", "Aluminium alloy"), "—" for one with none.
pub fn kind_name(number: u32) -> &'static str {
    entry(number).and_then(|e| e.kind).unwrap_or("—")
}

/// MA22 (estimated): the values a catalogue standard other than the app's
/// first three (MA18) gives the materials based on it where they have none
/// of their own: its kind (as `app_kind`), its weight per volume (mild
/// steel's 7.85 when the catalogue gives none), its own number as the
/// machine-file key, and the sheet-reference code of its kind. Its name and
/// code in programs stay mild steel's (STEEL, 0010), as for any standard the
/// app didn't know: what posts write for the catalogue is ticket #149's.
pub(crate) fn standards() -> &'static [Standard] {
    static ALL: std::sync::OnceLock<Vec<Standard>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let steel = &super::material::STANDARDS[0];
        CATALOGUE
            .iter()
            .filter(|e| !super::material::STANDARDS.iter().any(|s| s.number == e.number))
            .map(|e| {
                let kind = e.app_kind();
                let like = super::material::STANDARDS.iter().find(|s| s.kind == kind).unwrap_or(steel);
                Standard { number: e.number, name: e.name, kind, density: e.density.unwrap_or(steel.density), program: steel.program, short_code: like.short_code }
            })
            .collect()
    })
}

/// MA21: a new material from catalogue standard `number`, as Add from
/// standard makes it: named as the standard and based on it, every value
/// left blank, so its weight per volume, machine-file key and name and code
/// in programs are the standard's (shown greyed on the page) until the shop
/// types its own, and a material that didn't override one follows its
/// standard (spec #142). Refused for a number the catalogue doesn't have.
pub fn from_standard(number: u32) -> Result<Material, String> {
    let e = entry(number).ok_or_else(|| format!("{number:04} isn't in the material catalogue: see material mode=list (standards)."))?;
    Ok(Material { name: e.name.to_string(), ..blank_on(e.number) })
}

fn blank_on(standard: u32) -> Material {
    Material { id: 0, name: String::new(), other_names: vec![], standard, density: None, program_name: None, program_code: None, machine_ref: None }
}

/// The standard a blank material is based on until one is picked (MA22).
pub const BLANK_STANDARD: u32 = 10;

/// MA22 (estimated): a new blank material: no name yet (it can't be saved
/// without one, MA8), based on the first standard (0010) with every value
/// its standard's, since a material always has a standard (MA16). The
/// original's blank material has no catalogue number and every number 0.
pub fn blank() -> Material {
    blank_on(BLANK_STANDARD)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MA12 as `app/mechanics.json` records it: the rule's own table.
    fn recorded() -> Vec<serde_json::Value> {
        let all: serde_json::Value = serde_json::from_str(include_str!("../../mechanics.json")).unwrap();
        let cat = all["values"].as_array().unwrap().iter().find(|r| r["name"] == "material catalogue" && r["source"] == "MA12").expect("MA12's catalogue in mechanics.json");
        cat["value"].as_array().unwrap().clone()
    }

    #[test]
    fn the_catalogue_is_the_34_standards_of_ma12() {
        let rec = recorded();
        assert_eq!((CATALOGUE.len(), rec.len()), (34, 34));
        for (e, r) in CATALOGUE.iter().zip(&rec) {
            assert_eq!(format!("{:04}", e.number), r["number"].as_str().unwrap());
            assert_eq!((e.file_name, e.name), (r["name"].as_str().unwrap(), r["shown"].as_str().unwrap()), "{}", e.number);
            assert_eq!(if e.waterjet { "waterjet" } else { "punching" }, r["category"].as_str().unwrap(), "{}", e.number);
            assert_eq!(e.kind.unwrap_or(""), r["kind"].as_str().unwrap(), "{}", e.number);
            assert_eq!(e.density, r["density"].as_f64(), "{}", e.number);
        }
        // One entry, as the rule's table in MECHANICS.md writes it.
        let e = entry(110).unwrap();
        assert_eq!((e.label(), e.kind, e.density), ("0110 X3CrNiMo 17-13-3".to_string(), Some("Stainless steel"), Some(8.0)));
        // The two corrections the user chose (MA12).
        assert_eq!((entry(540).unwrap().density, entry(200).unwrap().kind), (Some(0.8), Some("Aluminium alloy")));
    }
}
