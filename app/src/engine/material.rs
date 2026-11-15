//! The materials, by id: the one place that knows a material's name, weight
//! per volume, name and code in machine programs, machine-file key and other
//! names. Everything else asks here by material id.
//!
//! The answers come from the material list (MA14): the shop's own materials,
//! kept in the Library (`crate::materials` reads and writes its file) and
//! installed here with `install`. Before anything is installed, and on a PC
//! whose list was never stored, the list is this PC's first three materials
//! (0 mild steel, 1 stainless steel, 2 aluminium) as its settings describe
//! them (MA15). Files store the id, so a job, part or stock item saved before
//! keeps its material (spec #142, invariant 1).

use crate::settings::AppSettings;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A material's id, as jobs, parts and stock items store it.
pub type Id = usize;

/// The kind of metal a material is: it sets the stock pictures' look and the
/// estimated cutting speeds used when the machine file gives none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    MildSteel,
    Stainless,
    Aluminium,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::MildSteel => "mild steel",
            Kind::Stainless => "stainless steel",
            Kind::Aluminium => "aluminium",
        }
    }
}

/// An entry of the standard material catalogue a material is based on: it
/// gives the material its kind, its weight per volume and, where the material
/// has none of its own, its name and code in programs and its machine-file
/// key. Built in; the shop doesn't edit them.
pub struct Standard {
    /// Catalogue number (the machine file's material key, MA1/MTL2).
    pub number: u32,
    pub name: &'static str,
    pub kind: Kind,
    /// Weight per volume, g/cm³.
    pub density: f64,
    /// Name in machine programs (MA25); its code there is its number.
    pub program_name: &'static str,
    /// Short code in made-up sheet references ("ST3-2000x1000").
    pub short_code: &'static str,
}

/// MA18 (estimated): until the original's catalogue is studied (ticket #143),
/// the standards are the three the app always knew, numbered as the machine
/// file and the BLM number them.
pub const STANDARDS: [Standard; 3] = [
    Standard { number: 10, name: "Mild steel", kind: Kind::MildSteel, density: 7.85, program_name: "STEEL", short_code: "ST" },
    Standard { number: 100, name: "Stainless steel", kind: Kind::Stainless, density: 8.0, program_name: "STAINLESS", short_code: "SS" },
    Standard { number: 200, name: "Aluminium", kind: Kind::Aluminium, density: 2.70, program_name: "ALUMINIUM", short_code: "AL" },
];

impl Standard {
    /// "0100 Stainless steel".
    pub fn label(&self) -> String {
        format!("{:04} {}", self.number, self.name)
    }

    /// Its key in the machine file's material × thickness sections (MA24).
    pub fn machine_ref(&self) -> String {
        format!("{:04}", self.number)
    }

    /// MA25: its name and code in machine programs: the first three's
    /// STEEL, STAINLESS and ALUMINIUM (MA18), every other standard's name as
    /// the catalogue shows it; the code is its four-digit number (0010,
    /// 0300), as the original's LC5 post writes for its starter materials,
    /// each named as its standard (MA11).
    pub fn program(&self) -> (String, String) {
        (self.program_name.to_string(), format!("{:04}", self.number))
    }
}

/// The standard numbered `number`: one of the first three (MA18), else the
/// catalogue's (MA22), else mild steel's, as an unknown number always was.
pub fn standard(number: u32) -> &'static Standard {
    STANDARDS.iter().find(|s| s.number == number).or_else(|| super::catalogue::standards().iter().find(|s| s.number == number)).unwrap_or(&STANDARDS[0])
}

/// The standard a text names (MA21): a catalogue number ("110", "0110");
/// one of the first three by its name, the start of it or its kind, as
/// before ("stainless"); else a catalogue standard by its name or its
/// name in the catalogue's file, or the start of one name only; without
/// case.
pub fn standard_from(text: &str) -> Result<&'static Standard, String> {
    use super::catalogue::{CATALOGUE, entry};
    let t = norm(text);
    let found = t.parse::<u32>().ok().filter(|&n| STANDARDS.iter().any(|s| s.number == n) || entry(n).is_some()).map(standard).or_else(|| {
        let by_name: Vec<_> = STANDARDS.iter().filter(|s| norm(s.name) == t || norm(s.name).starts_with(&t) || s.kind.label() == t).collect();
        (by_name.len() == 1).then(|| by_name[0])
    }).or_else(|| {
        let exact: Vec<_> = CATALOGUE.iter().filter(|e| norm(e.name) == t || norm(e.file_name) == t).collect();
        let starts: Vec<_> = CATALOGUE.iter().filter(|e| norm(e.name).starts_with(&t)).collect();
        match (exact.as_slice(), starts.as_slice()) {
            ([e], _) | ([], [e]) => Some(standard(e.number)),
            _ => None,
        }
    });
    found.filter(|_| !t.is_empty()).ok_or_else(|| {
        format!(
            "'{}' is not a standard. The standards: {}, or any of the catalogue's {} by its number or name: {}.",
            text.trim(),
            STANDARDS.iter().map(|s| s.label()).collect::<Vec<_>>().join(", "),
            CATALOGUE.len(),
            CATALOGUE.iter().map(|e| e.label()).collect::<Vec<_>>().join(", ")
        )
    })
}

/// One of the shop's materials (MA16). A value left out (None) is its
/// standard's, so a new grade cuts, weighs and writes programs as its
/// standard until the shop gives it its own.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub id: Id,
    pub name: String,
    /// Other names it goes by (grades, norms, shop names; MA6).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub other_names: Vec<String>,
    /// The standard's catalogue number.
    pub standard: u32,
    /// Weight per volume, g/cm³.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<f64>,
    /// Name in machine programs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program_name: Option<String>,
    /// Code in machine programs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program_code: Option<String>,
    /// Key in the machine file's material × thickness sections, as typed: a
    /// catalogue number or the name the file uses; empty = the name in
    /// programs (MTL2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine_ref: Option<String>,
}

impl Material {
    pub fn standard(&self) -> &'static Standard {
        standard(self.standard)
    }

    pub fn kind(&self) -> Kind {
        self.standard().kind
    }

    pub fn density(&self) -> f64 {
        self.density.unwrap_or(self.standard().density)
    }

    /// MA25: name and code in machine programs: its own (an empty one too,
    /// written empty), else its standard's. Never its name shown: renaming a
    /// material changes no program (MA19).
    pub fn program(&self) -> (String, String) {
        let (n, c) = self.standard().program();
        (self.program_name.clone().unwrap_or(n), self.program_code.clone().unwrap_or(c))
    }

    /// The machine-file key (MTL2): the one typed, normalised, or the name in
    /// programs when that is empty; the standard's when it has none.
    pub fn machine_key(&self) -> String {
        let typed = self.machine_ref.clone().unwrap_or_else(|| self.standard().machine_ref());
        let k = crate::ct1::material_key(&typed);
        if k.is_empty() { self.program().0.trim().to_string() } else { k }
    }

    /// Its name and each other name, as compared (MA16).
    fn names(&self) -> impl Iterator<Item = String> + '_ {
        std::iter::once(norm(&self.name)).chain(self.other_names.iter().map(|o| norm(o)))
    }
}

/// The material list (MA14), as stored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct List {
    pub format: String,
    pub version: u32,
    /// The id the next new material gets: ids are never given twice, so a
    /// file never comes to mean a material it didn't have (MA16).
    pub next_id: Id,
    pub materials: Vec<Material>,
    /// The numbers of the materials deleted from it (MA20), so a write that
    /// picked one before it was deleted is refused rather than saved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deleted: Vec<Id>,
    /// Who changed it last, and when (as SV9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed: Option<crate::shared::Saved>,
    /// The names materials had before they were renamed, by number (MA31):
    /// a file saved before a rename still means its material, even once
    /// another material takes the old name. Never used to find a name from
    /// outside (MA32).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub former: Vec<(Id, String)>,
    /// Only the first three of a Library's list that can't be read now
    /// (MA28): names never stand for its other materials. Never stored.
    #[serde(skip)]
    pub offline: bool,
}

pub const FORMAT: &str = "nestpro-materials";
pub const VERSION: u32 = 1;

/// One of the app's first three materials.
struct Builtin {
    name: &'static str,
    standard: u32,
    /// Key in the machine file's material × thickness sections (MTL2): mild
    /// steel 0010, stainless 0100, aluminium 0210 (the aluminium-magnesium
    /// grades its other names list; pure aluminium is 0200 — our choice).
    machine_ref: &'static str,
    /// Other names it goes by (MA6; common grades and shop names, estimated).
    other_names: &'static str,
}

const OLD_THREE: [Builtin; 3] = [
    Builtin { name: "Mild steel", standard: 10, machine_ref: "0010", other_names: "Mild steel, S235, S235JR, S275, S355, DC01, CR, HR, MS" },
    Builtin { name: "Stainless steel", standard: 100, machine_ref: "0100", other_names: "Stainless, 304, 304L, 316, 316L, AISI304, AISI316, 1.4301, INOX, SS" },
    Builtin { name: "Aluminium", standard: 200, machine_ref: "0210", other_names: "Aluminum, AL, 5052, 5754, 6061, AlMg3" },
];

/// A name as names are compared: without case, without spaces around it,
/// and with every run of spaces inside counted as one (MA16).
pub fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// A comma-separated list of other names, trimmed, without empty ones.
pub fn split_names(s: &str) -> Vec<String> {
    s.split(',').map(|x| x.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|x| !x.is_empty()).collect()
}

impl List {
    /// MA15: the list this PC's settings describe: the first three materials
    /// with ids 0 to 2, their names and codes in programs, machine-file keys
    /// and other names exactly as Settings → Materials had them. An other
    /// name that an earlier material (or its own name) already has is left
    /// out, so a name means one material (invariant 6); a lookup finds the
    /// same material it found before, since it always took the first.
    pub fn seed(cfg: &AppSettings) -> List {
        let mut l = List { format: FORMAT.into(), version: VERSION, next_id: OLD_THREE.len(), materials: vec![], deleted: vec![], changed: None, former: vec![], offline: false };
        for (id, b) in OLD_THREE.iter().enumerate() {
            let (pn, pc) = cfg.materials.get(id).cloned().unwrap_or_else(|| default_program(id));
            let typed = cfg.material_aliases.get(id).cloned().unwrap_or_else(|| b.other_names.to_string());
            let mut m = Material {
                id,
                name: b.name.to_string(),
                other_names: vec![],
                standard: b.standard,
                density: None,
                program_name: Some(pn),
                program_code: Some(pc),
                machine_ref: Some(cfg.material_refs.get(id).cloned().unwrap_or_else(|| b.machine_ref.to_string())),
            };
            for o in split_names(&typed) {
                let n = norm(&o);
                if n != norm(&m.name) && !m.other_names.iter().any(|x| norm(x) == n) && l.holder(&n, None).is_none() && !OLD_THREE[id + 1..].iter().any(|x| norm(x.name) == n) {
                    m.other_names.push(o);
                }
            }
            l.materials.push(m);
        }
        if cfg.starter_materials {
            l.add_starters();
        }
        l
    }

    /// MA29: a new install's list: after the first three (as above), the
    /// rest of the original's starter materials (MA11) with numbers 3 to 7.
    /// The first three keep the values a new install always gave them, so
    /// their programs don't change (invariant 2); the original's names for
    /// them (RST 37-2, X5CrNi, Al99) are other names of theirs. A starter's
    /// name taken off the first three's other names (AlMg3), so a name
    /// means one material.
    fn add_starters(&mut self) {
        for (id, n) in [(0, "RST 37-2"), (1, "X5CrNi"), (2, "Al99")] {
            if self.holder(&norm(n), None).is_none() {
                if let Some(m) = self.materials.iter_mut().find(|m| m.id == id) {
                    m.other_names.push(n.to_string());
                }
            }
        }
        // MA11: stainless steel's weight per volume and kind (corrected), its
        // own name and catalogue number in programs and as machine table key.
        let film = Material {
            id: 0,
            name: "X5CrNi + Film".into(),
            other_names: vec![],
            standard: 100,
            density: None,
            program_name: Some("X5CrNi + Film".into()),
            program_code: Some("3060".into()),
            machine_ref: Some("3060".into()),
        };
        let rest = [210, 300, 510, 590].map(|n| super::catalogue::from_standard(n).expect("a starter's standard is in the catalogue"));
        for m in std::iter::once(film).chain(rest) {
            let n = norm(&m.name);
            for x in &mut self.materials {
                x.other_names.retain(|o| norm(o) != n);
            }
            let _ = self.add(m);
        }
    }

    pub fn get(&self, id: Id) -> Option<&Material> {
        self.materials.iter().find(|m| m.id == id)
    }

    /// The material other than `except` that has this name or other name
    /// (already normalised).
    fn holder(&self, n: &str, except: Option<Id>) -> Option<&Material> {
        self.materials.iter().filter(|m| Some(m.id) != except).find(|m| m.names().any(|x| x == n))
    }

    /// MA16: a name and its other names, checked against every other
    /// material's: each means one material. Refused naming the material that
    /// has it. Gives the other names cleaned (trimmed, each once, without
    /// the material's own name).
    pub fn check_names(&self, name: &str, other_names: &[String], id: Option<Id>) -> Result<Vec<String>, String> {
        let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        if name.is_empty() {
            return Err("A material needs a name.".into());
        }
        if let Some(m) = self.holder(&norm(&name), id) {
            return Err(format!("The name '{name}' is already {}. Pick another name, or edit that material.", used_by(m, &norm(&name))));
        }
        let mut out: Vec<String> = vec![];
        for o in other_names {
            let o = o.split_whitespace().collect::<Vec<_>>().join(" ");
            let n = norm(&o);
            if n.is_empty() || n == norm(&name) || out.iter().any(|x| norm(x) == n) {
                continue;
            }
            if let Some(m) = self.holder(&n, id) {
                return Err(format!("The other name '{o}' is already {}. Take it off {} first (material mode=edit), or leave it out.", used_by(m, &n), m.name));
            }
            out.push(o);
        }
        Ok(out)
    }

    /// Add a new material, with the next id. Refused when a name is taken (MA16).
    pub fn add(&mut self, mut m: Material) -> Result<Id, String> {
        m.other_names = self.check_names(&m.name, &m.other_names, None)?;
        m.name = m.name.split_whitespace().collect::<Vec<_>>().join(" ");
        m.id = self.next_id.max(self.materials.iter().map(|x| x.id + 1).max().unwrap_or(0));
        self.next_id = m.id + 1;
        let id = m.id;
        self.materials.push(m);
        Ok(id)
    }

    /// Put a changed material in place of the one with its id. Refused when
    /// a name is taken (MA16).
    pub fn replace(&mut self, mut m: Material) -> Result<(), String> {
        if self.get(m.id).is_none() {
            return Err(format!("There's no material {} in the list.", m.id));
        }
        m.other_names = self.check_names(&m.name, &m.other_names, Some(m.id))?;
        m.name = m.name.split_whitespace().collect::<Vec<_>>().join(" ");
        if let Some(x) = self.materials.iter_mut().find(|x| x.id == m.id) {
            *x = m;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Joining a Library (MA28).
// ---------------------------------------------------------------------------

/// What joining a Library did with this PC's own materials.
#[derive(Clone, Debug, PartialEq)]
pub struct Joined {
    /// The Library's list with this PC's own materials added.
    pub list: List,
    /// This PC's materials added to it: (its number on this PC, its number
    /// in the Library).
    pub added: Vec<(Id, Id)>,
    /// This PC's materials taken as one the Library already has.
    pub same: Vec<(Id, Id)>,
}

impl List {
    /// MA28: this PC's own list `pc` joining the Library's (`self`). The
    /// Library's list wins: its materials keep their numbers and values. The
    /// first three are the same material on every PC (invariant 1), whatever
    /// they are called. Each other material of this PC is taken as the
    /// Library material one of its names (its name or an other name,
    /// compared as MA16) already belongs to — the one holding its name
    /// first, else the first in the list — and is otherwise added with the
    /// Library's next number and every value it had. Files keep working
    /// through the name stored beside the number (MA17, MA31).
    pub fn merge(&self, pc: &List) -> Joined {
        let mut list = self.clone();
        let (mut added, mut same) = (vec![], vec![]);
        for m in &pc.materials {
            if m.id < KEPT && self.get(m.id).is_some() {
                same.push((m.id, m.id));
                continue;
            }
            let own: Vec<String> = m.names().collect();
            let found = self.materials.iter().find(|x| x.names().any(|n| n == own[0])).or_else(|| self.materials.iter().find(|x| x.names().any(|n| own.contains(&n))));
            if let Some(x) = found {
                same.push((m.id, x.id));
                continue;
            }
            // No name of it is in the Library's list; an other name an
            // earlier added material took is left out (invariant 6).
            let taken = |l: &List, n: &str| l.holder(&norm(n), None).is_some();
            let mut copy = m.clone();
            copy.other_names.retain(|o| !taken(&list, o));
            if taken(&list, &copy.name) {
                continue;
            }
            if let Ok(id) = list.add(copy) {
                added.push((m.id, id));
            }
        }
        // MA31: the PC's name for each material it brought, and its names
        // before a rename, go with it as former names of the Library's
        // number, so a PC job saved under one of them still finds it (they
        // aren't names the list shows, invariant 6). The first three are
        // read by number.
        for &(from, to) in same.iter().chain(&added).filter(|x| x.0 >= KEPT) {
            let pc_names = pc.get(from).map(|m| m.name.clone()).into_iter().chain(pc.former.iter().filter(|f| f.0 == from).map(|f| f.1.clone()));
            for n in pc_names.collect::<Vec<_>>() {
                let k = norm(&n);
                let shown = list.get(to).is_some_and(|x| x.names().any(|y| y == k));
                if !k.is_empty() && !shown && !list.former.iter().any(|(i, f)| *i == to && norm(f) == k) {
                    list.former.push((to, n));
                }
            }
        }
        Joined { list, added, same }
    }
}

// ---------------------------------------------------------------------------
// Picking a material: the Material ▾ everywhere (MA23).
// ---------------------------------------------------------------------------

/// The kinds a Material ▾ groups materials under, in the order it shows
/// them (MA23).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    CarbonSteel,
    Stainless,
    Aluminium,
    Galvanized,
    CopperBrass,
    Other,
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::CarbonSteel => "Carbon steel",
            Group::Stainless => "Stainless steel",
            Group::Aluminium => "Aluminium",
            Group::Galvanized => "Galvanized",
            Group::CopperBrass => "Copper and brass",
            Group::Other => "Other",
        }
    }
}

/// MA23 (estimated): the group of a standard, from the catalogue's kind for
/// its number (MA11, MA12): Carbon Steel 0010–0080; Stainless steel 0100,
/// 0110 and the film-covered stainless 3060; Aluminium alloy 0200, 0210;
/// Steel, the zinc-coated 0300 and 0310, as Galvanized; copper 0510 and
/// brass 0590. Every other number (the waterjet entries, which have no
/// kind, and numbers the catalogue doesn't have) is Other.
pub fn group_of(standard: u32) -> Group {
    match standard {
        10 | 20 | 30 | 40 | 50 | 60 | 70 | 80 => Group::CarbonSteel,
        100 | 110 | 3060 => Group::Stainless,
        200 | 210 => Group::Aluminium,
        300 | 310 => Group::Galvanized,
        510 | 590 => Group::CopperBrass,
        _ => Group::Other,
    }
}

/// One material a Material ▾ offers.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub id: Id,
    pub name: String,
    /// The other name the typed text was found in, when the name itself
    /// doesn't hold it ("DEMO 316 Stainless — AISI 316").
    pub matched: Option<String>,
}

/// One group of a Material ▾: its kind and its materials.
#[derive(Clone, Debug, PartialEq)]
pub struct Choices {
    pub group: Group,
    pub choices: Vec<Choice>,
}

/// A text as typed text is matched: `norm`, and accented letters as plain
/// ones ("brossé" → "brosse").
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in norm(s).chars() {
        match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => out.push('a'),
            'ç' | 'ć' | 'č' => out.push('c'),
            'ď' | 'đ' => out.push('d'),
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => out.push('e'),
            'ğ' => out.push('g'),
            'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => out.push('i'),
            'ł' | 'ľ' | 'ĺ' => out.push('l'),
            'ñ' | 'ń' | 'ň' => out.push('n'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => out.push('o'),
            'ř' | 'ŕ' => out.push('r'),
            'ś' | 'š' | 'ş' => out.push('s'),
            'ť' | 'ţ' => out.push('t'),
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => out.push('u'),
            'ý' | 'ÿ' => out.push('y'),
            'ź' | 'ż' | 'ž' => out.push('z'),
            'ß' => out.push_str("ss"),
            'æ' => out.push_str("ae"),
            'œ' => out.push_str("oe"),
            c => out.push(c),
        }
    }
    out
}

/// MA23: what a Material ▾ offers for a typed text: the materials whose
/// name, or one of whose other names, holds every word typed (in any order,
/// ignoring case, spaces and accents), grouped by kind in `Group` order and
/// by name within a kind. Nothing typed offers every material; a kind none
/// of them is in isn't shown.
pub fn choose(list: &List, text: &str) -> Vec<Choices> {
    let words: Vec<String> = fold(text).split(' ').filter(|w| !w.is_empty()).map(str::to_string).collect();
    let holds = |s: &str| {
        let s = fold(s);
        words.iter().all(|w| s.contains(w.as_str()))
    };
    let mut found: Vec<(Group, String, Choice)> = vec![];
    for m in &list.materials {
        let matched = if holds(&m.name) {
            None
        } else if let Some(o) = m.other_names.iter().find(|o| holds(o)) {
            Some(o.clone())
        } else {
            continue;
        };
        found.push((group_of(m.standard), fold(&m.name), Choice { id: m.id, name: m.name.clone(), matched }));
    }
    found.sort_by(|a, b| (a.0, &a.1, a.2.id).cmp(&(b.0, &b.1, b.2.id)));
    let mut out: Vec<Choices> = vec![];
    for (g, _, c) in found {
        match out.last_mut() {
            Some(last) if last.group == g => last.choices.push(c),
            _ => out.push(Choices { group: g, choices: vec![c] }),
        }
    }
    out
}

/// MA20: materials 0 to 2 (mild steel, stainless steel, aluminium) are what
/// every file saved before the material list means (invariant 1), so they
/// are never deleted, whatever they are called now. The original lets any
/// unused material go (MA10); spec #142 keeps these three.
pub const KEPT: Id = 3;

/// What uses a material (MA20): the saved jobs, library parts and stock
/// items (sheets and remnants) of the Library, and the jobs open here, each
/// by name, in the order found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Uses {
    /// Jobs open in this window or agent session (JOB-88), saved into the
    /// Library or not.
    pub open: Vec<String>,
    pub jobs: Vec<String>,
    pub parts: Vec<String>,
    pub stock: Vec<String>,
}

/// How many names a refusal gives of each kind of use before counting the
/// rest (MA20).
pub const NAMED: usize = 3;

impl Uses {
    pub fn is_empty(&self) -> bool {
        self.open.is_empty() && self.jobs.is_empty() && self.parts.is_empty() && self.stock.is_empty()
    }

    /// "the job open here (DEMO-7), 4 jobs (A, B, C and 1 more) and 1 stock item (S1)".
    pub fn words(&self) -> String {
        let few = |v: &[String]| {
            let shown = v.iter().take(NAMED).cloned().collect::<Vec<_>>().join(", ");
            if v.len() > NAMED { format!("{shown} and {} more", v.len() - NAMED) } else { shown }
        };
        let count = |v: &[String], one: &str, many: &str| (!v.is_empty()).then(|| format!("{} {} ({})", v.len(), if v.len() == 1 { one } else { many }, few(v)));
        let open = (!self.open.is_empty()).then(|| format!("{} ({})", if self.open.len() == 1 { "the job open here" } else { "the jobs open here" }, few(&self.open)));
        let all: Vec<String> = [open, count(&self.jobs, "job", "jobs"), count(&self.parts, "library part", "library parts"), count(&self.stock, "stock item", "stock items")].into_iter().flatten().collect();
        match all.split_last() {
            Some((last, [])) => last.clone(),
            Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
            None => String::new(),
        }
    }
}

impl List {
    /// MA20: take a material out of the list. Refused for materials 0 to 2,
    /// and while anything uses it (`uses`, read under the same write lock),
    /// saying where; nothing that uses a material is changed with it (MA10).
    /// Its number is never given again (MA16), so a file that still names
    /// it shows `?` and keeps its number and name (JOB-86) rather than
    /// coming to mean another material.
    pub fn remove(&mut self, id: Id, uses: &Uses) -> Result<Material, String> {
        let m = self.get(id).cloned().ok_or_else(|| format!("There's no material {id} in the list."))?;
        if id < KEPT {
            return Err(format!(
                "{} can't be deleted: it is one of the first three materials (0 mild steel, 1 stainless steel, 2 aluminium), which every job, part and stock sheet saved before the material list means. You can rename it (material mode=edit name=…).",
                m.name
            ));
        }
        if !uses.is_empty() {
            return Err(format!("{} can't be deleted: it is used by {}. Give them another material first, or keep it and rename it (material mode=edit name=…).", m.name, uses.words()));
        }
        self.next_id = self.next_id.max(id + 1);
        self.materials.retain(|x| x.id != id);
        self.deleted.push(id);
        Ok(m)
    }
}

fn used_by(m: &Material, n: &str) -> String {
    if norm(&m.name) == n { format!("the name of material {} ({})", m.id, m.name) } else { format!("another name of {} (material {})", m.name, m.id) }
}

// ---------------------------------------------------------------------------
// The list the answers come from.
// ---------------------------------------------------------------------------

static SHARED: std::sync::RwLock<Option<Arc<List>>> = std::sync::RwLock::new(None);

#[cfg(test)]
thread_local! {
    /// A test's own list (tests run side by side; the shared one stays the seed).
    static LOCAL: std::cell::RefCell<Option<Arc<List>>> = const { std::cell::RefCell::new(None) };
}

/// The list the answers come from now.
pub fn current() -> Arc<List> {
    #[cfg(test)]
    if let Some(l) = LOCAL.with(|l| l.borrow().clone()) {
        return l;
    }
    if let Some(l) = SHARED.read().ok().and_then(|g| g.clone()) {
        return l;
    }
    Arc::new(List::seed(&AppSettings::default()))
}

/// Make `list` the one every answer comes from (in a test: on the calling
/// thread only).
pub fn install(list: List) {
    #[cfg(test)]
    {
        LOCAL.with(|l| *l.borrow_mut() = Some(Arc::new(list)));
    }
    #[cfg(not(test))]
    if let Ok(mut g) = SHARED.write() {
        *g = Some(Arc::new(list));
    }
}

fn with<T>(id: Id, f: impl FnOnce(&Material) -> T) -> Option<T> {
    current().get(id).map(f)
}

/// JOB-86 (estimated): an id that isn't in the list (a hand-edited file, one
/// from a newer version or another Library) keeps the values it always had
/// where one was always given: those of the old last material, aluminium.
fn nearest(id: Id) -> &'static Builtin {
    &OLD_THREE[id.min(OLD_THREE.len() - 1)]
}

/// Every material's id, in list order.
pub fn ids() -> Vec<Id> {
    current().materials.iter().map(|m| m.id).collect()
}

/// How many materials the list has.
pub fn count() -> usize {
    current().materials.len()
}

pub fn exists(id: Id) -> bool {
    current().get(id).is_some()
}

/// The material's name as the app shows it ("Stainless steel").
pub fn name(id: Id) -> Option<String> {
    with(id, |m| m.name.clone())
}

/// The material's name, or `fallback` for an id that isn't in the list.
pub fn name_or(id: Id, fallback: &str) -> String {
    name(id).unwrap_or_else(|| fallback.to_string())
}

/// The material's name; an id that isn't in the list gives the old last
/// material's (as the program header, the stock window and the import notes
/// always did).
pub fn nearest_name(id: Id) -> String {
    name(id).unwrap_or_else(|| nearest(id).name.to_string())
}

pub fn kind(id: Id) -> Kind {
    with(id, |m| m.kind()).unwrap_or_else(|| standard(nearest(id).standard).kind)
}

/// Short code for a sheet reference made up from material, thickness and
/// size (its standard's); an id that isn't in the list gives the old last
/// material's.
pub fn short_code(id: Id) -> &'static str {
    with(id, |m| m.standard().short_code).unwrap_or_else(|| standard(nearest(id).standard).short_code)
}

/// Weight per volume, g/cm³.
pub fn density(id: Id) -> f64 {
    with(id, |m| m.density()).unwrap_or_else(|| standard(nearest(id).standard).density)
}

/// The first three materials' names and codes in machine programs as a new
/// install has them; any other id is written as mild steel.
pub fn default_program(id: Id) -> (String, String) {
    OLD_THREE.get(id).map_or(&STANDARDS[0], |b| standard(b.standard)).program()
}

/// The material's name and code in machine programs; an id that isn't in
/// the list is written as mild steel.
pub fn program(id: Id) -> (String, String) {
    with(id, |m| m.program()).unwrap_or_else(|| default_program(id))
}

/// The name in programs, or the material's own name when that is empty (the
/// material text on remnant marks and stock references).
pub fn program_name_or_name(id: Id) -> String {
    with(id, |m| Some(m.program().0).filter(|n| !n.is_empty()).unwrap_or_else(|| m.name.clone())).unwrap_or_else(|| nearest_name(id))
}

/// The material's machine-file key (MTL2); none for an id that isn't in the
/// list.
pub fn machine_key(id: Id) -> String {
    with(id, |m| m.machine_key()).unwrap_or_default()
}

/// The material a name means exactly (MA32 steps 1 and 2), compared as
/// names are (MA16): a material's name or one of its other names, which
/// only one material has; else a material's name or code in programs (its
/// own, or its standard's when it has none), the first in the list when
/// several share it. As in the original (MA6), another name gives its main
/// material; an unknown name gives None.
pub fn find(text: &str) -> Option<Id> {
    let n = norm(text);
    if n.is_empty() {
        return None;
    }
    let l = current();
    l.materials
        .iter()
        .find(|m| m.names().any(|x| x == n))
        .or_else(|| {
            l.materials.iter().find(|m| {
                let (p, c) = m.program();
                norm(&p) == n || norm(&c) == n
            })
        })
        .map(|m| m.id)
}

/// MA32: the shortest name a look-alike or a grade compares.
const LIKE_MIN: usize = 3;

/// A look-alike (MA32 step 4): the longest material name the text holds
/// ("X5CrNi + Film 2B" → "X5CrNi + Film", not "X5CrNi"); else the first
/// material whose name holds the text. Names and texts shorter than three
/// characters are only ever found exactly (step 1).
pub fn look_alike(text: &str) -> Option<Id> {
    let t = norm(text);
    let l = current();
    let mut best: Option<(usize, Id)> = None;
    for m in &l.materials {
        let n = norm(&m.name);
        let len = n.chars().count();
        if len >= LIKE_MIN && t.contains(&n) && best.is_none_or(|(b, _)| len > b) {
            best = Some((len, m.id));
        }
    }
    best.map(|b| b.1).or_else(|| (t.chars().count() >= LIKE_MIN).then(|| l.materials.iter().find(|m| norm(&m.name).contains(&t)).map(|m| m.id)).flatten())
}

/// A grade (MA32 step 3): the text starts with another name of three
/// characters or more holding a digit ("S355J2" → the material with other
/// name S355); the longest such other name wins, then the first in the list.
fn grade(text: &str) -> Option<Id> {
    let t = norm(text);
    let l = current();
    let mut best: Option<(usize, Id)> = None;
    for m in &l.materials {
        for x in m.other_names.iter().map(|o| norm(o)) {
            let len = x.chars().count();
            if len >= LIKE_MIN && x.chars().any(|c| c.is_ascii_digit()) && t.starts_with(&x) && best.is_none_or(|(b, _)| len > b) {
                best = Some((len, m.id));
            }
        }
    }
    best.map(|b| b.1)
}

/// MA32: the material a name from outside means — an XML import's or a
/// drawing file's material, an opened program's material header, a file
/// name's material field, an ERP job's material. Searches the whole list:
/// `find` (names and other names, then names and codes in programs); then a
/// grade; then a look-alike. None when nothing matches: each import then
/// does what it always did (the job's material stays, and it says so).
/// A plain number is a name like any other here, never a material number:
/// files from outside number materials their own way.
pub fn like(text: &str) -> Option<Id> {
    find(text).or_else(|| grade(text)).or_else(|| look_alike(text))
}

/// MA17: the material a file means by the id and name it stored. The id,
/// when the list has it; else the name (or one of the other names) finds
/// it; else the id stays, and the screens show `?` (JOB-86). A file saved
/// before names were stored has only the id. A name more than one material
/// has or had (MA31) picks none: the id stays.
pub fn resolve(id: Id, name: &str) -> Id {
    resolve_read(id, name).unwrap_or(id)
}

/// The materials that have the name `n` (as compared, MA16) as their name
/// or an other name, or had it before a rename or as a joining PC's name
/// (MA31), each once.
fn holders(l: &List, n: &str) -> Vec<Id> {
    let mut ids: Vec<Id> = l.materials.iter().filter(|m| m.names().any(|x| x == n)).map(|m| m.id).chain(l.former.iter().filter(|(i, f)| norm(f) == n && l.get(*i).is_some()).map(|x| x.0)).collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// MA31: a name more than one material has or had: it means none of them
/// when a file is read.
pub fn ambiguous(name: &str) -> bool {
    holders(&current(), &norm(name)).len() > 1
}

/// MA17, MA31: the material a file's number and stored name mean; None
/// when the name is one more than one material has or had and the number
/// isn't one of them (it can't be told which: the file's material is
/// unknown).
pub fn resolve_read(id: Id, name: &str) -> Option<Id> {
    let n = norm(name);
    if n.is_empty() || id < KEPT {
        return Some(id);
    }
    let l = current();
    // The number's own names, and the names it had before (MA31), keep it.
    let own = l.get(id).is_some_and(|m| m.names().any(|x| x == n)) || l.former.iter().any(|(i, f)| *i == id && norm(f) == n);
    // MA28: a Library's list that can't be read: only the first three are
    // known here, and this PC's names for them never stand for the
    // Library's other materials.
    if own || l.offline {
        return Some(id);
    }
    // MA31: the number has another material, or none: the file is from
    // another list (a PC that joined a Library, another Library), and the
    // one material with that name is its material.
    match holders(&l, &n).as_slice() {
        [one] => Some(*one),
        [] if l.get(id).is_some() => Some(id),
        [] => Some(find(name).unwrap_or(id)),
        _ => None,
    }
}

/// Added to a job file's material number while the job is open, when its
/// name can't say which material it is (MA31): no material has the number
/// so made, so the job shows `?` (MA30), and its file gets its own number
/// back when saved (`file_id`).
pub const UNRESOLVED: Id = 1_000_000_000;

/// The number a job's file stores for material `id` (the file's own for
/// an unresolved one).
pub fn file_id(id: Id) -> Id {
    if id >= UNRESOLVED { id - UNRESOLVED } else { id }
}

/// MA17, MA31: a job's material from its file's number and stored name:
/// unresolved (shown `?`, MA30) when the name can't say which.
pub fn read_job(id: Id, name: &str) -> Id {
    let id = file_id(id);
    resolve_read(id, name).unwrap_or(UNRESOLVED + id)
}

/// MA30: what a job whose material the list doesn't have says, from its
/// number and the name it came with (`read`): None when the list has it.
/// The job is never switched to another material by itself.
pub fn unknown_note(id: Id, read: &str) -> Option<String> {
    if exists(id) {
        return None;
    }
    let id = file_id(id);
    let r = read.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(if r.is_empty() {
        format!("This job's material (number {id}) isn't in the material list. The job keeps it until another material is picked.")
    } else if ambiguous(&r) {
        format!("This job's material, {r} (number {id}), can't be told: more than one material in the list has or had that name. The job keeps it until another material is picked.")
    } else {
        format!("This job's material, {r}, isn't in the material list. The job keeps it (number {id}) until another material is picked, or {r} is added to the list.")
    })
}

/// MA30: the material that adding a job's unknown material to the list
/// makes: named as the job has it, based on mild steel (0010) with every
/// value its standard's, as a blank material is (MA22), until the shop sets
/// its standard and values in Settings → Materials.
pub fn for_unknown(read: &str) -> Material {
    Material { name: read.split_whitespace().collect::<Vec<_>>().join(" "), ..super::catalogue::blank() }
}

/// The name a file stores beside a material id (invariant 3): the list's,
/// or the one it was read with when the list doesn't have the id.
pub fn stored_name(id: Id, read: &str) -> String {
    name(id).unwrap_or_else(|| read.to_string())
}

/// The names materials not in the list were read with, by id (MA17): a job
/// in the window keeps the one it was opened with, and the one of a stock
/// sheet whose material it took, so each is saved back with its own id only.
pub type ReadNames = Vec<(Id, String)>;

/// The names read for `id` and `name` (nothing for an empty name).
pub fn names_read(id: Id, name: &str) -> ReadNames {
    if name.trim().is_empty() { vec![] } else { vec![(id, name.to_string())] }
}

/// Remember `name` as read for `id` (an empty name changes nothing).
pub fn remember_read(read: &mut ReadNames, id: Id, name: &str) {
    if !name.trim().is_empty() {
        read.retain(|x| x.0 != id);
        read.push((id, name.to_string()));
    }
}

/// The name read for `id`, or empty when none was read for it.
pub fn read_name(id: Id, read: &[(Id, String)]) -> String {
    read.iter().rev().find(|x| x.0 == id).map(|x| x.1.clone()).unwrap_or_default()
}

/// What the list is, as text that changes when it does (to know when an
/// import's preview must be read again).
pub fn fingerprint() -> String {
    format!("{:?}", current().materials)
}

// This PC's Settings → Materials lines in its settings file: what seeds the
// list (MA15). Kept as they were; the list is the only source afterwards.

/// Names and codes in programs, by id, for a new install.
pub fn default_pc_programs() -> Vec<(String, String)> {
    (0..OLD_THREE.len()).map(default_program).collect()
}

/// Other names, by id, for a new install.
pub fn default_pc_other_names() -> Vec<String> {
    OLD_THREE.iter().map(|b| b.other_names.to_string()).collect()
}

/// Machine-file keys as typed, by id, for a new install.
pub fn default_pc_machine_refs() -> Vec<String> {
    OLD_THREE.iter().map(|b| b.machine_ref.to_string()).collect()
}

/// The settings file's material lines.
pub fn write_pc(cfg: &AppSettings, s: &mut String, esc: impl Fn(&str) -> String) {
    for (k, (n, id)) in cfg.materials.iter().enumerate() {
        s.push_str(&format!("material{k} = {}|{}\n", esc(n), esc(id)));
    }
    for (k, a) in cfg.material_aliases.iter().enumerate() {
        s.push_str(&format!("material_alias{k} = {}\n", esc(a)));
    }
    for (k, r) in cfg.material_refs.iter().enumerate() {
        s.push_str(&format!("material_ref{k} = {}\n", esc(r)));
    }
    if cfg.starter_materials {
        s.push_str(&format!("{START_LINE} = {STARTERS_WORD}\n"));
    }
}

/// The settings line that says a new install's list starts with the
/// starter materials (MA29); settings saved before it have none.
const START_LINE: &str = "material_list";
const STARTERS_WORD: &str = "starter";

/// Read one `[general]` line of the settings file if it is a material line
/// (a line for a material past the first three is ignored). False when it
/// isn't one.
pub fn read_pc(cfg: &mut AppSettings, k: &str, v: String) -> bool {
    if k == START_LINE {
        cfg.starter_materials = v.trim() == STARTERS_WORD;
    } else if let Some(i) = k.strip_prefix("material_ref") {
        if let Some(r) = i.parse::<usize>().ok().and_then(|i| cfg.material_refs.get_mut(i)) {
            *r = v;
        }
    } else if let Some(i) = k.strip_prefix("material_alias") {
        if let Some(a) = i.parse::<usize>().ok().and_then(|i| cfg.material_aliases.get_mut(i)) {
            *a = v;
        }
    } else if let Some(i) = k.strip_prefix("material") {
        if let (Some(m), Some((n, id))) = (i.parse::<usize>().ok().and_then(|i| cfg.materials.get_mut(i)), v.split_once('|')) {
            *m = (n.to_string(), id.to_string());
        }
    } else {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded(text: &str) {
        install(List::seed(&AppSettings::parse(text)));
    }

    #[test]
    fn the_old_three_answer_by_id() {
        seeded("");
        assert_eq!(ids(), vec![0, 1, 2]);
        assert_eq!(current().materials.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), vec!["Mild steel", "Stainless steel", "Aluminium"]);
        assert_eq!(name(1).as_deref(), Some("Stainless steel"));
        assert_eq!((density(0), density(1), density(2)), (7.85, 8.0, 2.70));
        assert_eq!((kind(0), kind(1), kind(2)), (Kind::MildSteel, Kind::Stainless, Kind::Aluminium));
        assert_eq!(default_program(0), ("STEEL".to_string(), "0010".to_string()));
        assert_eq!(default_program(2), ("ALUMINIUM".to_string(), "0200".to_string()));
        assert_eq!(machine_key(2), "##MATEXP0210##");
        assert_eq!((short_code(0), short_code(1), short_code(2)), ("ST", "SS", "AL"));
    }

    #[test]
    fn an_id_past_the_list_has_no_name_and_keeps_each_old_fallback() {
        // JOB-86 (estimated): no name; where a value was always given, the
        // same value as before (weights and cutting estimates of the last
        // material, programs as mild steel, no machine-file key).
        seeded("");
        assert!(!exists(3) && exists(2));
        assert_eq!(name(7), None);
        assert_eq!(name_or(7, "?"), "?");
        assert_eq!(nearest_name(7), "Aluminium");
        assert_eq!((density(7), kind(7)), (2.70, Kind::Aluminium));
        assert_eq!(short_code(7), "AL");
        assert_eq!(default_program(7), ("STEEL".to_string(), "0010".to_string()));
        assert_eq!(program(7), ("STEEL".to_string(), "0010".to_string()));
        assert_eq!(machine_key(7), "");
        assert_eq!(program_name_or_name(7), "Aluminium");
    }

    #[test]
    fn before_settings_exist_the_defaults_answer() {
        seeded("");
        assert_eq!(program(1), ("STAINLESS".to_string(), "0100".to_string()));
        assert_eq!(ids().into_iter().map(machine_key).collect::<Vec<_>>(), vec!["##MATEXP0010##", "##MATEXP0100##", "##MATEXP0210##"]);
        assert_eq!(find("aisi316"), Some(1));
        assert_eq!(find("STEEL"), Some(0), "the name in programs");
        assert_eq!(program_name_or_name(2), "ALUMINIUM");
    }

    #[test]
    fn renamed_materials_and_extra_other_names_come_from_this_pcs_settings() {
        seeded("[general]\nmaterial1 = DEMO-INOX|0101\nmaterial_alias1 = DEMO-316, DEMO-A2\nmaterial_ref1 = DEMO\nmaterial2 = |0200\nmaterial_ref2 =\n");
        assert_eq!(program(1), ("DEMO-INOX".to_string(), "0101".to_string()));
        assert_eq!((find(" demo-a2 "), find("0101"), find("AISI316")), (Some(1), Some(1), None));
        assert_eq!(name(1).as_deref(), Some("Stainless steel"), "the name shown stays the app's");
        // An empty machine-file key is the name in programs (MTL2); an empty
        // name in programs gives the material's own name on remnant marks.
        assert_eq!(ids().into_iter().map(machine_key).collect::<Vec<_>>(), vec!["##MATEXP0010##", "DEMO", ""]);
        assert_eq!(program_name_or_name(2), "Aluminium");
        // A grade that starts with another name holding a digit, then a look-alike.
        assert_eq!(like("DEMO-316L"), Some(1));
        assert_eq!(like("aluminium 5754"), Some(2));
        assert_eq!(like("DEMO-COPPER"), None);
    }

    #[test]
    fn a_seed_with_a_name_twice_keeps_it_on_the_first_material_only() {
        // MA15: an other name typed on two materials stays on the first,
        // which a lookup always found; a material's own name always wins.
        seeded("[general]\nmaterial_alias0 = DEMO-X, Stainless steel, demo-x\nmaterial_alias1 = demo-x , DEMO-Y\nmaterial_alias2 =\n");
        let l = current();
        assert_eq!(l.get(0).unwrap().other_names, vec!["DEMO-X"]);
        assert_eq!(l.get(1).unwrap().other_names, vec!["DEMO-Y"]);
        assert!(l.get(2).unwrap().other_names.is_empty());
        assert_eq!((find("demo-x"), find("stainless steel")), (Some(0), Some(1)));
    }

    #[test]
    fn a_new_grade_takes_its_standards_values_until_it_has_its_own() {
        seeded("");
        let mut l = (*current()).clone();
        let id = l.add(Material { id: 0, name: " DEMO  316 ".into(), other_names: vec!["DEMO-316".into(), "demo 316".into()], standard: 100, density: None, program_name: None, program_code: None, machine_ref: None }).unwrap();
        assert_eq!(id, 3);
        install(l);
        assert_eq!(name(3).as_deref(), Some("DEMO 316"));
        assert_eq!(current().get(3).unwrap().other_names, vec!["DEMO-316"], "its own name isn't another name");
        assert_eq!((kind(3), density(3), short_code(3)), (Kind::Stainless, 8.0, "SS"));
        assert_eq!(program(3), ("STAINLESS".to_string(), "0100".to_string()));
        assert_eq!(machine_key(3), "##MATEXP0100##");
        assert_eq!(resolve(9, "demo-316"), 3, "a file from another Library finds it by name");
        assert_eq!((resolve(1, "DEMO 316"), resolve(9, "")), (1, 9), "the id first; an id alone stays");
    }

    #[test]
    fn every_standard_has_its_own_name_and_code_in_programs() {
        // MA25: the first three as always; the catalogue's others their name
        // as shown and their number; MA27: a number the catalogue lacks as
        // mild steel.
        let p = |n: u32| standard(n).program();
        let pair = |a: &str, b: &str| (a.to_string(), b.to_string());
        assert_eq!([p(10), p(100), p(200)], [pair("STEEL", "0010"), pair("STAINLESS", "0100"), pair("ALUMINIUM", "0200")]);
        assert_eq!([p(300), p(510), p(110), p(40), p(670)], [pair("Galvanized", "0300"), pair("Copper", "0510"), pair("X3CrNiMo 17-13-3", "0110"), pair("ST 52-3", "0040"), pair("PVC", "0670")]);
        let odd = Material { id: 9, name: "DEMO odd".into(), other_names: vec![], standard: 9999, density: None, program_name: None, program_code: None, machine_ref: None };
        assert_eq!((odd.program(), odd.machine_key(), odd.density(), odd.kind()), (pair("STEEL", "0010"), "##MATEXP0010##".to_string(), 7.85, Kind::MildSteel));
        assert_eq!(group_of(9999), Group::Other);
    }

    /// The seed plus made-up grades: DEMO 316 (stainless, its own name and
    /// code in programs), DEMO 304 (stainless, its standard's), DEMO-X
    /// (stainless, named only by its other name) and DEMO MILD + FILM.
    fn with_demo_grades() {
        seeded("");
        let mut l = (*current()).clone();
        let m = |name: &str, other: &[&str], standard: u32, program: Option<(&str, &str)>| Material {
            id: 0,
            name: name.into(),
            other_names: other.iter().map(|s| s.to_string()).collect(),
            standard,
            density: None,
            program_name: program.map(|p| p.0.to_string()),
            program_code: program.map(|p| p.1.to_string()),
            machine_ref: None,
        };
        assert_eq!(l.add(m("DEMO 316", &["AISI 316", "DEMO-316", "DEMO1.4401"], 100, Some(("DEMO316", "0316")))), Ok(3));
        assert_eq!(l.add(m("DEMO 304", &[], 100, None)), Ok(4));
        assert_eq!(l.add(m("DEMO STEEL", &["DEMO-S3"], 10, None)), Ok(5));
        assert_eq!(l.add(m("DEMO STEEL + FILM", &["DEMO-S35"], 10, Some(("DEMO-FILM", "0010")))), Ok(6));
        install(l);
    }

    #[test]
    fn a_name_from_outside_finds_any_material_of_the_list() {
        // MA32 steps 1 and 2: names, other names, names and codes in programs.
        with_demo_grades();
        assert_eq!(like("aisi 316"), Some(3), "another name, any capitals");
        assert_eq!(like("  AISI   316 "), Some(3), "spaces around and inside");
        assert_eq!(like("AISI316"), Some(1), "the seed's own other name stays stainless");
        assert_eq!(like("demo 316"), Some(3), "its name");
        assert_eq!((like("DEMO316"), like("0316")), (Some(3), Some(3)), "its name and code in programs");
        assert_eq!(like("demo1.4401"), Some(3), "an other name only it has");
        // A code shared by several: the first in the list (lowest number).
        assert_eq!(like("0100"), Some(1), "DEMO 304 writes 0100 too, but stainless is first");
        assert_eq!(like("STAINLESS"), Some(1));
        assert_eq!(like("0010"), Some(0), "DEMO STEEL + FILM writes 0010 too; mild steel is first");
        assert_eq!(like("demo-film"), Some(6));
        // A name wins over another material's name in programs (names mean
        // one material; names in programs may repeat).
        let mut l = (*current()).clone();
        let id = l.add(Material { id: 0, name: "DEMO316".into(), other_names: vec![], standard: 100, density: None, program_name: None, program_code: None, machine_ref: None }).unwrap();
        install(l);
        assert_eq!((like("demo316"), like("0316")), (Some(id), Some(3)));
    }

    #[test]
    fn a_name_from_outside_falls_back_to_a_grade_then_a_look_alike() {
        with_demo_grades();
        // Step 3: the longest other name the grade starts with.
        assert_eq!(like("DEMO-316L"), Some(3));
        assert_eq!(like("DEMO-S355"), Some(6), "DEMO-S35 is longer than DEMO-S3");
        assert_eq!(like("DEMO-S3X"), Some(5));
        assert_eq!(like("S355J2"), Some(0), "the seed's S355");
        // Step 4: the longest name the text holds, then a name holding it.
        assert_eq!(like("DEMO STEEL + FILM 2B"), Some(6));
        assert_eq!(like("DEMO STEEL 2B"), Some(5));
        assert_eq!(like("Aluminium 5754"), Some(2));
        assert_eq!(like("demo 30"), Some(4), "the first name that holds it");
        // Short texts are found only exactly; nothing else gives None.
        assert_eq!((like("ST"), like("al"), like("1")), (None, Some(2), None), "AL is an other name of aluminium");
        assert_eq!((like(""), like("   "), like("DEMO-COPPER")), (None, None, None));
        // A plain number is never read as a material number here.
        assert_eq!((like("0"), like("2"), like("4")), (None, None, None));
    }

    #[test]
    fn a_material_taken_off_the_list_is_found_no_more() {
        with_demo_grades();
        let mut l = (*current()).clone();
        l.materials.retain(|m| m.id != 3);
        install(l);
        assert_eq!((like("aisi 316"), like("DEMO316"), like("0316"), like("DEMO 316")), (None, None, None, None));
        assert_eq!(like("DEMO-316L"), None);
    }

    /// A made-up shop list: the old three, then grades on several standards
    /// (0300 galvanized, 0510 copper, 0590 brass, 0400 titanium, and a number
    /// the catalogue doesn't have).
    fn demo_list() -> List {
        let mut l = List::seed(&AppSettings::parse(""));
        let mut add = |name: &str, std: u32, other: &[&str]| {
            l.add(Material { id: 0, name: name.into(), other_names: other.iter().map(|s| s.to_string()).collect(), standard: std, density: None, program_name: None, program_code: None, machine_ref: None }).unwrap()
        };
        add("DEMO 316 Stainless", 100, &["DEMO-AISI 316"]);
        add("DEMO 304 Stainless", 100, &[]);
        add("DEMO A36", 10, &[]);
        add("DEMO 5052 Aluminium", 210, &["DEMO-AlMg2.5"]);
        add("DEMO Galvanized G90", 300, &[]);
        add("DEMO Copper", 510, &[]);
        add("DEMO Brass", 590, &[]);
        add("DEMO Titanium", 400, &[]);
        add("DEMO Mystery", 9999, &[]);
        l
    }

    /// The groups and their materials' names, as the Material ▾ shows them.
    fn shown(groups: &[Choices]) -> Vec<(&'static str, Vec<String>)> {
        groups.iter().map(|g| (g.group.label(), g.choices.iter().map(|c| c.name.clone()).collect())).collect()
    }

    #[test]
    fn the_material_list_groups_every_material_by_its_standards_kind() {
        // MA23: with nothing typed, every material, grouped by the kind of
        // its standard (MA12) in a fixed order, by name within a kind;
        // a kind with one material is a group of one; empty kinds aren't shown.
        let g = choose(&demo_list(), "");
        assert_eq!(
            shown(&g),
            vec![
                ("Carbon steel", vec!["DEMO A36".to_string(), "Mild steel".into()]),
                ("Stainless steel", vec!["DEMO 304 Stainless".into(), "DEMO 316 Stainless".into(), "Stainless steel".into()]),
                ("Aluminium", vec!["Aluminium".into(), "DEMO 5052 Aluminium".into()]),
                ("Galvanized", vec!["DEMO Galvanized G90".into()]),
                ("Copper and brass", vec!["DEMO Brass".into(), "DEMO Copper".into()]),
                ("Other", vec!["DEMO Mystery".into(), "DEMO Titanium".into()]),
            ]
        );
        assert!(g.iter().flat_map(|g| &g.choices).all(|c| c.matched.is_none()), "nothing typed, no other name shown");
        assert_eq!(g[0].choices[1].id, 0, "each choice carries its material's id");
    }

    #[test]
    fn the_old_three_are_three_groups_of_one() {
        let g = choose(&List::seed(&AppSettings::parse("")), "");
        assert_eq!(shown(&g), vec![("Carbon steel", vec!["Mild steel".to_string()]), ("Stainless steel", vec!["Stainless steel".into()]), ("Aluminium", vec!["Aluminium".into()])]);
    }

    #[test]
    fn typing_filters_by_name_without_case_or_extra_spaces() {
        let g = choose(&demo_list(), "  demo   316 ");
        assert_eq!(shown(&g), vec![("Stainless steel", vec!["DEMO 316 Stainless".to_string()])]);
        // Every word typed, in any order.
        assert_eq!(shown(&choose(&demo_list(), "STAINLESS 304")), vec![("Stainless steel", vec!["DEMO 304 Stainless".to_string()])]);
        assert!(choose(&demo_list(), "DEMO-NOTHING").is_empty(), "nothing matches: no groups");
        assert!(choose(&demo_list(), "   ").iter().map(|g| g.choices.len()).sum::<usize>() == 12, "only spaces: everything");
    }

    #[test]
    fn an_other_name_finds_its_material_and_says_which_matched() {
        // Neither name holds "aisi": both are found by an other name.
        let g = choose(&demo_list(), "aisi 316");
        assert_eq!(shown(&g), vec![("Stainless steel", vec!["DEMO 316 Stainless".to_string(), "Stainless steel".into()])]);
        let by = |n: &str| g[0].choices.iter().find(|c| c.name == n).unwrap().matched.clone();
        assert_eq!(by("DEMO 316 Stainless").as_deref(), Some("DEMO-AISI 316"));
        assert_eq!(by("Stainless steel").as_deref(), Some("AISI316"), "the first other name holding every word");
        // A name that matches shows no other name.
        assert_eq!(choose(&demo_list(), "copper")[0].choices[0].matched, None);
        // Only an other name matches: "AlMg2.5" is no material's name.
        assert_eq!(shown(&choose(&demo_list(), "almg2.5")), vec![("Aluminium", vec!["DEMO 5052 Aluminium".to_string()])]);
    }

    #[test]
    fn accents_are_ignored_both_ways() {
        let mut l = demo_list();
        l.add(Material { id: 0, name: "DEMO Acier inoxydable brossé".into(), other_names: vec![], standard: 100, density: None, program_name: None, program_code: None, machine_ref: None }).unwrap();
        assert_eq!(shown(&choose(&l, "brosse")), vec![("Stainless steel", vec!["DEMO Acier inoxydable brossé".to_string()])]);
        assert_eq!(shown(&choose(&l, "MYSTÉRY")), vec![("Other", vec!["DEMO Mystery".to_string()])]);
    }

    #[test]
    fn a_standard_the_catalogue_lacks_is_other() {
        assert_eq!(group_of(9999), Group::Other);
        assert_eq!(group_of(400), Group::Other, "the waterjet entries have no kind");
        assert_eq!((group_of(10), group_of(80), group_of(110), group_of(3060), group_of(200), group_of(310), group_of(590)), (Group::CarbonSteel, Group::CarbonSteel, Group::Stainless, Group::Stainless, Group::Aluminium, Group::Galvanized, Group::CopperBrass));
    }

    #[test]
    fn a_deleted_material_is_not_offered_and_a_file_naming_it_shows_a_question_mark() {
        // MA20 + MA23: deleted, it leaves the list (its number is kept in
        // `deleted`, never given again); the Material ▾ doesn't offer it,
        // not even by typing its old name, and a job still naming its number
        // shows `?` in the field (JOB-86).
        let mut l = demo_list();
        let id = l.materials.iter().find(|m| m.name == "DEMO Galvanized G90").unwrap().id;
        l.remove(id, &Uses::default()).unwrap();
        assert!(l.deleted.contains(&id));
        let g = choose(&l, "");
        assert!(g.iter().all(|g| g.group != Group::Galvanized), "its kind had only it");
        assert!(g.iter().flat_map(|g| &g.choices).all(|c| c.id != id));
        assert!(choose(&l, "galvanized g90").is_empty());
        install(l);
        assert_eq!(name_or(id, "?"), "?");
    }

    #[test]
    fn a_long_list_keeps_every_material_in_name_order() {
        let mut l = List::seed(&AppSettings::parse(""));
        for k in (0..300).rev() {
            l.add(Material { id: 0, name: format!("DEMO-{k:03}"), other_names: vec![], standard: [10, 100, 200, 300][k % 4], density: None, program_name: None, program_code: None, machine_ref: None }).unwrap();
        }
        let g = choose(&l, "");
        assert_eq!(g.iter().map(|g| g.choices.len()).sum::<usize>(), 303);
        assert_eq!(g[3].group, Group::Galvanized);
        assert_eq!(g[3].choices.first().map(|c| c.name.as_str()), Some("DEMO-003"));
        assert_eq!(g[3].choices.last().map(|c| c.name.as_str()), Some("DEMO-299"));
        assert_eq!(choose(&l, "demo-15").iter().map(|g| g.choices.len()).sum::<usize>(), 10, "DEMO-150 to DEMO-159 (not DEMO-015)");
    }

    #[test]
    fn names_are_unique_without_case_or_spaces() {
        seeded("");
        let mut l = (*current()).clone();
        let m = |name: &str, other: &[&str]| Material { id: 0, name: name.into(), other_names: other.iter().map(|s| s.to_string()).collect(), standard: 100, density: None, program_name: None, program_code: None, machine_ref: None };
        let e = l.add(m("  stainless   STEEL ", &[])).unwrap_err();
        assert!(e.contains("the name of material 1 (Stainless steel)"), "{e}");
        let e = l.add(m("aisi316", &[])).unwrap_err();
        assert!(e.contains("another name of Stainless steel (material 1)"), "{e}");
        let e = l.add(m("DEMO 316", &["AISI316"])).unwrap_err();
        assert!(e.contains("'AISI316'") && e.contains("Stainless steel"), "{e}");
        let e = l.add(m("DEMO 316", &["mild steel"])).unwrap_err();
        assert!(e.contains("material 0 (Mild steel)"), "{e}");
        assert_eq!(l.materials.len(), 3, "nothing added");
        assert_eq!(l.add(m("", &[])).unwrap_err(), "A material needs a name.");
    }

    // -----------------------------------------------------------------------
    // Joining a Library (MA28): the Library's list wins; this PC's own
    // materials are added unless one of their names is already there.
    // -----------------------------------------------------------------------

    fn grade(name: &str, standard: u32, other: &[&str]) -> Material {
        Material { id: 0, name: name.into(), other_names: other.iter().map(|s| s.to_string()).collect(), standard, density: None, program_name: None, program_code: None, machine_ref: None }
    }

    /// The old three, then `grades`, as a list stores them.
    fn list_with(grades: &[Material]) -> List {
        let mut l = List::seed(&AppSettings::parse(""));
        for g in grades {
            l.add(g.clone()).unwrap();
        }
        l
    }

    fn names(l: &List) -> Vec<&str> {
        l.materials.iter().map(|m| m.name.as_str()).collect()
    }

    #[test]
    fn joining_a_library_keeps_its_list_and_adds_the_pcs_own_grades() {
        let pc = list_with(&[grade("DEMO 316", 100, &[])]);
        let lib = list_with(&[grade("DEMO 316L", 100, &[])]);
        let j = lib.merge(&pc);
        assert_eq!(names(&j.list), ["Mild steel", "Stainless steel", "Aluminium", "DEMO 316L", "DEMO 316"]);
        // The PC's DEMO 316 had number 3, the Library's DEMO 316L's: it gets a new one.
        assert_eq!(j.list.get(4).map(|m| m.name.as_str()), Some("DEMO 316"));
        assert_eq!(j.added, [(3, 4)]);
        assert_eq!(j.same, [(0, 0), (1, 1), (2, 2)]);
        assert_eq!(j.list.next_id, 5);
    }

    #[test]
    fn a_name_that_differs_only_in_case_or_spaces_is_the_same_material() {
        let pc = list_with(&[grade("  demo   316 ", 100, &[])]);
        let lib = list_with(&[grade("DEMO 316", 100, &[])]);
        let j = lib.merge(&pc);
        assert_eq!(names(&j.list), ["Mild steel", "Stainless steel", "Aluminium", "DEMO 316"]);
        assert_eq!((j.added.len(), j.same.last()), (0, Some(&(3, 3))));
        assert_eq!(j.list, lib, "nothing changed");
    }

    #[test]
    fn a_pc_material_whose_other_name_is_a_library_name_is_that_material() {
        // Either way round: the PC's other name is the Library's name, or the
        // PC's name is one of the Library material's other names.
        let pc = list_with(&[grade("DEMO Inox 316", 100, &["DEMO 316"]), grade("DEMO 1.4404", 100, &[])]);
        let lib = list_with(&[grade("DEMO 304", 100, &[]), grade("DEMO 316", 100, &["DEMO 1.4404"])]);
        let j = lib.merge(&pc);
        assert_eq!(j.list.materials, lib.materials, "nothing added");
        assert_eq!(j.same[3..], [(3, 4), (4, 4)], "both are the Library's DEMO 316");
        // MA31: the PC's name goes with it, unseen, so the PC's jobs find it.
        assert_eq!(j.list.former, [(4, "DEMO Inox 316".to_string())]);
    }

    #[test]
    fn a_name_wins_over_an_other_name_when_two_library_materials_match() {
        let pc = list_with(&[grade("DEMO 316", 100, &["DEMO 304"])]);
        let lib = list_with(&[grade("DEMO 304", 100, &[]), grade("DEMO 316", 100, &[])]);
        assert_eq!(lib.merge(&pc).same.last(), Some(&(3, 4)));
    }

    #[test]
    fn the_first_three_are_the_same_material_whatever_they_are_called() {
        let mut pc = list_with(&[]);
        pc.materials[0].name = "DEMO Plate".into();
        pc.materials[2].other_names = vec!["DEMO-ALU".into()];
        let lib = list_with(&[grade("DEMO Plate", 10, &[])]);
        let j = lib.merge(&pc);
        assert_eq!(j.same, [(0, 0), (1, 1), (2, 2)], "by number, not by the name another material has");
        assert_eq!(j.list, lib, "the Library's values stand");
    }

    #[test]
    fn an_added_material_keeps_its_values_and_only_its_free_other_names() {
        let mut g = grade("DEMO 904L", 100, &["DEMO-904", "DEMO-X"]);
        g.density = Some(8.1);
        g.program_name = Some("DEMO904".into());
        g.machine_ref = Some("0110".into());
        let pc = list_with(&[g]);
        let lib = list_with(&[]);
        let j = lib.merge(&pc);
        let m = j.list.get(3).unwrap();
        assert_eq!((m.name.as_str(), m.density, m.program_name.as_deref(), m.machine_ref.as_deref()), ("DEMO 904L", Some(8.1), Some("DEMO904"), Some("0110")));
        assert_eq!(m.other_names, ["DEMO-904", "DEMO-X"]);
    }

    #[test]
    fn joining_twice_adds_nothing_more() {
        let pc = list_with(&[grade("DEMO 316", 100, &[]), grade("DEMO A36", 10, &[])]);
        let lib = list_with(&[grade("DEMO 316L", 100, &[])]);
        let once = lib.merge(&pc).list;
        let twice = once.merge(&pc);
        assert_eq!(twice.list, once);
        assert!(twice.added.is_empty());
    }

    #[test]
    fn a_library_number_deleted_is_never_given_to_a_pc_material() {
        let mut lib = list_with(&[grade("DEMO Gone", 100, &[])]);
        lib.remove(3, &Uses::default()).unwrap();
        let pc = list_with(&[grade("DEMO 316", 100, &[])]);
        let j = lib.merge(&pc);
        assert_eq!(j.added, [(3, 4)]);
        assert_eq!(j.list.deleted, [3]);
    }

    // -----------------------------------------------------------------------
    // A file's material by its number and name (MA17, MA31).
    // -----------------------------------------------------------------------

    #[test]
    fn a_pc_job_reopens_on_its_own_grade_after_joining() {
        // The PC's DEMO 316 was number 3; in the Library 3 is DEMO 316L and
        // DEMO 316 became 4. The name stored beside the number finds it.
        let pc = list_with(&[grade("DEMO 316", 100, &["DEMO-316"])]);
        let lib = list_with(&[grade("DEMO 316L", 100, &[]), grade("DEMO 1.4404", 100, &["DEMO-1.4404"])]);
        install(lib.merge(&pc).list);
        assert_eq!(resolve(3, "DEMO 316"), 5);
        assert_eq!(resolve(3, "  demo 316 "), 5, "compared as names are");
        assert_eq!(resolve(3, "DEMO 316L"), 3, "the number's own name: itself");
        // Found only by an other name.
        assert_eq!(resolve(3, "DEMO-1.4404"), 4);
        // Gone by number and by name: the number stays (the screens show ?).
        assert_eq!(resolve(41, "DEMO 904L"), 41);
        assert_eq!(resolve(3, "DEMO 904L"), 3, "a number the list has and a name nothing has: a rename");
        // Files saved before names were stored: the number alone.
        assert_eq!(resolve(3, ""), 3);
        // The first three are themselves whatever name is stored beside them.
        assert_eq!(resolve(1, "DEMO 316"), 1);
    }

    #[test]
    fn a_renamed_material_keeps_its_files_even_when_its_old_name_is_taken_again() {
        let mut l = list_with(&[grade("DEMO 304", 100, &[])]);
        let was = l.get(3).unwrap().clone();
        let now = Material { name: "DEMO 304 Brushed".into(), ..was.clone() };
        l.apply(&[crate::engine::materialedit::Change::Edit { was, now }], |_| Ok(Uses::default())).unwrap();
        l.add(grade("DEMO 304", 100, &[])).unwrap();
        install(l);
        // A job saved before the rename stored (3, "DEMO 304"): still material 3.
        assert_eq!(resolve(3, "DEMO 304"), 3);
        // A file from elsewhere naming DEMO 304 by a number we don't have:
        // the name is the new one's and was 3's, so it can't be told which
        // (MA31): the number stays (unknown), never a guess.
        assert!(ambiguous("DEMO 304"));
        assert_eq!(resolve_read(9, "DEMO 304"), None);
        assert_eq!(resolve(9, "DEMO 304"), 9);
        assert_eq!(read_job(9, "DEMO 304"), UNRESOLVED + 9);
        assert_eq!(file_id(read_job(9, "DEMO 304")), 9);
    }

    // -----------------------------------------------------------------------
    // The starting list (MA29).
    // -----------------------------------------------------------------------

    fn settings_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("nestpro_material_start_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_new_install_starts_with_the_starter_materials() {
        let d = settings_dir("fresh");
        let cfg = AppSettings::load_from(&d.join("settings.ini"));
        install(List::seed(&cfg));
        let l = current();
        assert_eq!(names(&l), ["Mild steel", "Stainless steel", "Aluminium", "X5CrNi + Film", "AlMg3", "Galvanized", "Copper", "Brass"]);
        assert_eq!(l.next_id, 8);
        // The first three are as a new install always had them (invariant 2).
        assert_eq!((program(0), program(1), program(2)), (("STEEL".into(), "0010".into()), ("STAINLESS".into(), "0100".into()), ("ALUMINIUM".into(), "0200".into())));
        assert_eq!((machine_key(0), machine_key(2), density(2)), ("##MATEXP0010##".to_string(), "##MATEXP0210##".to_string(), 2.70));
        // The original's names for them find them.
        assert_eq!((find("RST 37-2"), find("x5crni"), find("Al99")), (Some(0), Some(1), Some(2)));
        // MA11: the film-covered stainless weighs as stainless (8.0, corrected)
        // and writes its own name and number; Copper is spelt right.
        assert_eq!((kind(3), density(3), program(3), machine_key(3)), (Kind::Stainless, 8.0, ("X5CrNi + Film".to_string(), "3060".to_string()), "##MATEXP3060##".to_string()));
        assert_eq!((density(4), program(4), machine_key(4)), (2.7, ("AlMg3".to_string(), "0210".to_string()), "##MATEXP0210##".to_string()));
        assert_eq!((density(5), program(5)), (7.9, ("Galvanized".to_string(), "0300".to_string())));
        assert_eq!((density(6), program(6), find("copper")), (8.9, ("Copper".to_string(), "0510".to_string()), Some(6)));
        assert_eq!((density(7), program(7)), (8.5, ("Brass".to_string(), "0590".to_string())));
        // A name means one material: AlMg3 is the starter, no longer one of
        // aluminium's other names.
        assert_eq!(find("almg3"), Some(4));
        assert!(!l.get(2).unwrap().other_names.iter().any(|o| o == "AlMg3"));
        // Saved and read again, it still starts so.
        cfg.save_to(&d.join("settings.ini")).unwrap();
        assert_eq!(List::seed(&AppSettings::load_from(&d.join("settings.ini"))).materials.len(), 8);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn settings_saved_before_keep_their_three() {
        let d = settings_dir("old");
        let f = d.join("settings.ini");
        std::fs::write(&f, "[general]\ntheme = dark\nmaterial0 = STEEL|0010\nmaterial1 = STAINLESS|0100\nmaterial2 = ALUMINIUM|0200\n").unwrap();
        let l = List::seed(&AppSettings::load_from(&f));
        assert_eq!(names(&l), ["Mild steel", "Stainless steel", "Aluminium"]);
        // Empty values stay empty, exactly (MA15).
        std::fs::write(&f, "[general]\nmaterial0 = |\nmaterial_alias0 =\nmaterial_ref0 =\n").unwrap();
        let l = List::seed(&AppSettings::load_from(&f));
        let m = l.get(0).unwrap();
        assert_eq!((l.materials.len(), m.program_name.as_deref(), m.program_code.as_deref(), m.machine_ref.as_deref(), m.other_names.len()), (3, Some(""), Some(""), Some(""), 0));
        let _ = std::fs::remove_dir_all(&d);
    }
}
