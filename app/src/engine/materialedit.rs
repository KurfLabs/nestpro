//! Changing the material list: the changes the `material` operation and
//! Settings → Materials make (add, edit, delete), applied together (MA21),
//! and the page's own copy of the list while it is being edited (`Draft`).
//! The page only calls these; `crate::materials::apply` stores the result
//! under the Library's write lock.

use super::catalogue;
use super::material::{self, Id, List, Material, Uses, KEPT};

/// One change to the material list.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    /// A new material (its id is given when it is added).
    Add(Material),
    /// A material's values as they were read (`was`) and as they are to be
    /// (`now`), by its id: only the values that differ are changed, so a
    /// change another PC made meanwhile to another value stays.
    Edit { was: Material, now: Material },
    /// Take a material out (MA20).
    Delete(Id),
}

/// What one change did.
#[derive(Clone, Debug, PartialEq)]
pub enum Done {
    Added(Id),
    Edited(Id),
    Deleted(Material),
}

impl List {
    /// MA21: make `changes` together, all or none. Deletions first (each
    /// refused as MA20 says, with what `uses` finds using it), then the
    /// edits, then the new materials. The names are checked once every edit
    /// is in place (MA16), so two materials can swap names in one save, and
    /// a name freed by a deletion or an edit can be taken by another
    /// material. The first refusal stops it, and the caller stores nothing.
    /// Gives what each change did, in the order given.
    pub fn apply(&mut self, changes: &[Change], mut uses: impl FnMut(Id) -> Result<Uses, String>) -> Result<Vec<Done>, String> {
        let mut done: Vec<Option<Done>> = vec![None; changes.len()];
        for (k, c) in changes.iter().enumerate() {
            if let Change::Delete(id) = c {
                let u = if *id >= KEPT && self.get(*id).is_some() { uses(*id)? } else { Uses::default() };
                done[k] = Some(Done::Deleted(self.remove(*id, &u)?));
            }
        }
        let mut edited = vec![];
        for (k, c) in changes.iter().enumerate() {
            if let Change::Edit { was, now } = c {
                let Some(x) = self.materials.iter_mut().find(|x| x.id == now.id) else {
                    return Err(format!("{} is no longer in the material list (deleted meanwhile, on another PC?): undo the changes to see the list as it is now.", if was.name.trim().is_empty() { format!("Material {}", now.id) } else { was.name.trim().to_string() }));
                };
                merge(x, was, now);
                x.name = x.name.split_whitespace().collect::<Vec<_>>().join(" ");
                edited.push(now.id);
                done[k] = Some(Done::Edited(now.id));
            }
        }
        for id in edited {
            let m = self.get(id).cloned().unwrap();
            let others = self.check_names(&m.name, &m.other_names, Some(id))?;
            if let Some(x) = self.materials.iter_mut().find(|x| x.id == id) {
                x.other_names = others;
            }
        }
        for (k, c) in changes.iter().enumerate() {
            if let Change::Add(m) = c {
                done[k] = Some(Done::Added(self.add(m.clone())?));
            }
        }
        Ok(done.into_iter().flatten().collect())
    }
}

/// Take into `x` (as stored) each value `now` changed from `was`.
fn merge(x: &mut Material, was: &Material, now: &Material) {
    fn take<T: PartialEq + Clone>(x: &mut T, was: &T, now: &T) {
        if was != now {
            *x = now.clone();
        }
    }
    take(&mut x.name, &was.name, &now.name);
    take(&mut x.other_names, &was.other_names, &now.other_names);
    take(&mut x.standard, &was.standard, &now.standard);
    take(&mut x.density, &was.density, &now.density);
    take(&mut x.program_name, &was.program_name, &now.program_name);
    take(&mut x.program_code, &was.program_code, &now.program_code);
    take(&mut x.machine_ref, &was.machine_ref, &now.machine_ref);
}

// ---------------------------------------------------------------------------
// Settings → Materials: the list as the page is editing it.
// ---------------------------------------------------------------------------

/// The values of a material the page edits as text, each blank meaning its
/// standard's (MA16).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Density,
    ProgramName,
    ProgramCode,
    MachineRef,
}

pub const FIELDS: [Field; 4] = [Field::Density, Field::ProgramName, Field::ProgramCode, Field::MachineRef];

impl Field {
    fn at(self) -> usize {
        self as usize
    }

    /// The material's own value, as text ("" when it has none).
    fn own(self, m: &Material) -> String {
        match self {
            Field::Density => m.density.map(|d| d.to_string()).unwrap_or_default(),
            Field::ProgramName => m.program_name.clone().unwrap_or_default(),
            Field::ProgramCode => m.program_code.clone().unwrap_or_default(),
            Field::MachineRef => m.machine_ref.clone().unwrap_or_default(),
        }
    }

    fn clear(self, m: &mut Material) {
        match self {
            Field::Density => m.density = None,
            Field::ProgramName => m.program_name = None,
            Field::ProgramCode => m.program_code = None,
            Field::MachineRef => m.machine_ref = None,
        }
    }
}

/// A weight per volume as typed: blank (or "standard") is the standard's;
/// else a number above 0 and below 30, g/cm³ (a comma for the point too).
pub fn parse_density(text: &str) -> Result<Option<f64>, String> {
    let t = text.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("standard") {
        return Ok(None);
    }
    match t.replace(',', ".").parse::<f64>() {
        Ok(d) if d > 0.0 && d < 30.0 => Ok(Some(d)),
        _ => Err(format!("density {t}: a weight per volume in g/cm³, above 0 and below 30 (e.g. 7.85 for steel)")),
    }
}

/// One material on the page.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// The material as stored (None: added on the page, not saved yet).
    pub was: Option<Material>,
    /// Its values as the page has them now (name, other names as typed in
    /// `other_names`, standard; the four fields as typed in `texts`).
    pub m: Material,
    pub other_names: String,
    pub texts: [String; 4],
    /// Marked to be deleted when the page is saved.
    pub delete: bool,
    /// Tells rows apart while the page is open (new rows have no id).
    pub key: u64,
}

impl Row {
    fn of(m: &Material, key: u64) -> Row {
        Row { was: Some(m.clone()), m: m.clone(), other_names: m.other_names.join(", "), texts: FIELDS.map(|f| f.own(m)), delete: false, key }
    }

    fn new(m: Material, key: u64) -> Row {
        Row { was: None, ..Row::of(&m, key) }
    }

    pub fn text(&mut self, f: Field) -> &mut String {
        &mut self.texts[f.at()]
    }

    /// The material as the row says, or why it can't be: a value typed
    /// differently from the one shown becomes its own; one cleared is its
    /// standard's again; one left alone stays as it was (an empty value of
    /// its own too, MA15).
    pub fn material(&self) -> Result<Material, String> {
        let mut m = self.m.clone();
        m.other_names = material::split_names(&self.other_names);
        let shown = self.was.as_ref().unwrap_or(&self.m);
        for f in FIELDS {
            let typed = &self.texts[f.at()];
            if *typed == f.own(shown) {
                continue;
            }
            if typed.trim().is_empty() {
                f.clear(&mut m);
                continue;
            }
            let t = typed.trim().to_string();
            match f {
                Field::Density => m.density = parse_density(&t).map_err(|e| format!("{}: {e}", self.label()))?,
                Field::ProgramName => m.program_name = Some(t),
                Field::ProgramCode => m.program_code = Some(t),
                Field::MachineRef => m.machine_ref = Some(t),
            }
        }
        Ok(m)
    }

    /// What a blank field gives (shown greyed in it): the value of its
    /// standard (as the row has it now).
    pub fn standard_value(&self, f: Field) -> String {
        let mut m = self.m.clone();
        f.clear(&mut m);
        match f {
            Field::Density => format!("{}", m.density()),
            Field::ProgramName => m.program().0,
            Field::ProgramCode => m.program().1,
            Field::MachineRef => m.standard().machine_ref(),
        }
    }

    /// Changed on the page since it was read (or new, or to be deleted).
    pub fn changed(&self) -> bool {
        self.delete || self.was.is_none() || self.material().map_or(true, |m| Some(&m) != self.was.as_ref())
    }

    pub fn label(&self) -> String {
        let n = self.m.name.trim();
        if !n.is_empty() {
            n.to_string()
        } else if self.was.is_none() {
            "The new material".to_string()
        } else {
            format!("Material {}", self.m.id)
        }
    }

    /// Material 0 to 2, or saved: kept by the list (deleting needs the MA20 check).
    pub fn is_new(&self) -> bool {
        self.was.is_none()
    }
}

/// The material list as Settings → Materials is editing it: nothing is
/// stored until it is saved (`changes`, then `crate::materials::apply`),
/// and undoing goes back to the list as stored.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Draft {
    pub rows: Vec<Row>,
    /// The list it was read from (its materials, to know when another PC
    /// changed it).
    pub read: Vec<Material>,
    next_key: u64,
}

impl Draft {
    pub fn of(list: &List) -> Draft {
        let rows: Vec<Row> = list.materials.iter().enumerate().map(|(k, m)| Row::of(m, k as u64)).collect();
        Draft { next_key: rows.len() as u64, rows, read: list.materials.clone() }
    }

    fn push(&mut self, m: Material) -> usize {
        self.rows.push(Row::new(m, self.next_key));
        self.next_key += 1;
        self.rows.len() - 1
    }

    /// MA21: add a material from catalogue standard `number`. Gives its row.
    pub fn add_from_standard(&mut self, number: u32) -> Result<usize, String> {
        Ok(self.push(catalogue::from_standard(number)?))
    }

    /// MA22: add a blank material. Gives its row.
    pub fn add_blank(&mut self) -> usize {
        self.push(catalogue::blank())
    }

    /// Delete the material of row `k`: a new one goes at once; a stored one
    /// is marked to go when saved, unless `check` (MA20: what uses it,
    /// looked at now) refuses, which is given back and nothing is marked.
    pub fn delete(&mut self, k: usize, check: impl FnOnce(Id) -> Result<(), String>) -> Result<(), String> {
        let Some(r) = self.rows.get_mut(k) else { return Ok(()) };
        if r.is_new() {
            self.rows.remove(k);
            return Ok(());
        }
        check(r.m.id)?;
        r.delete = true;
        Ok(())
    }

    /// Keep a material marked to be deleted.
    pub fn keep(&mut self, k: usize) {
        if let Some(r) = self.rows.get_mut(k) {
            r.delete = false;
        }
    }

    pub fn changed(&self) -> bool {
        self.rows.iter().any(Row::changed)
    }

    /// The changes to store, or the first row that can't be.
    pub fn changes(&self) -> Result<Vec<Change>, String> {
        let mut out = vec![];
        for r in &self.rows {
            if r.delete {
                out.push(Change::Delete(r.m.id));
            } else if r.is_new() {
                out.push(Change::Add(r.material()?));
            } else if r.changed() {
                out.push(Change::Edit { was: r.was.clone().unwrap(), now: r.material()? });
            }
        }
        Ok(out)
    }

    /// Reset to defaults: the first three materials' names and codes in
    /// programs, machine-file keys and other names as `defaults` (a new
    /// install's list) has them; their names stay. Stored when saved.
    pub fn reset_first_three(&mut self, defaults: &List) {
        for r in self.rows.iter_mut().filter(|r| !r.is_new() && r.m.id < KEPT) {
            if let Some(d) = defaults.get(r.m.id) {
                r.texts = FIELDS.map(|f| f.own(d));
                r.other_names = d.other_names.join(", ");
            }
        }
    }

    /// Read again from `list` when nothing is typed and it changed (another
    /// PC saved it, or an agent).
    pub fn follow(&mut self, list: &List) {
        if !self.changed() && self.read != list.materials {
            *self = Draft::of(list);
        }
    }
}

/// MA20, looked at before a deletion is marked on the page: refused for
/// materials 0 to 2 and while `uses` finds something using it, with the
/// refusal the save would give. Saving looks again under the write lock.
pub fn check_delete(list: &List, id: Id, uses: impl FnOnce(Id) -> Result<Uses, String>) -> Result<(), String> {
    let u = if id >= KEPT && list.get(id).is_some() { uses(id)? } else { Uses::default() };
    list.clone().remove(id, &u).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AppSettings;

    #[test]
    fn a_value_left_alone_stays_as_it_was_even_an_empty_one_of_its_own() {
        // MA15: a PC's settings can give a material an empty value of its own
        // (aluminium's machine table key here); the page shows it blank, and
        // saving another change keeps it empty rather than the standard's.
        let l = List::seed(&AppSettings::parse("[general]\nmaterial_ref2 =\n"));
        let mut d = Draft::of(&l);
        assert_eq!(d.rows[2].m.machine_ref.as_deref(), Some(""));
        assert_eq!(d.rows[2].text(Field::MachineRef).as_str(), "");
        d.rows[2].m.name = "DEMO Alu".into();
        let Change::Edit { now, .. } = &d.changes().unwrap()[0] else { panic!() };
        assert_eq!((now.name.as_str(), now.machine_ref.as_deref()), ("DEMO Alu", Some("")));
        // Typed and cleared again: the standard's.
        *d.rows[2].text(Field::MachineRef) = "DEMO".into();
        d.rows[2].text(Field::MachineRef).clear();
        assert_eq!(d.rows[2].material().unwrap().machine_ref.as_deref(), Some(""), "back as shown: left alone");
        assert!(d.rows[2].standard_value(Field::MachineRef) == "0200");
    }
}
