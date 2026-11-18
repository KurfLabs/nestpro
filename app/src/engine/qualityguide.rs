//! What each of the machine's cut qualities means for the sheet in hand
//! (CQ8): one guide that the cut technology panel, Quality by size, the
//! part's quality picker and the agents' `qualities` operation all show.
//!
//! A quality is only a number until someone says what it is: the machine
//! file may name it (CQ4), the shop may name it and say what it is for
//! (CQ7), the machine's technology table may give it a speed and pierce
//! mode for this material, thickness and gas (TT3, TT6), the machine
//! section's size bands may give it to contours of some sizes (MTL10), and
//! the post may use it for marks (BL22). The guide gathers all of that.

use crate::nest::{CutQuality, SheetSpec};
use crate::qualities::NameFrom;
use crate::techtable::SpeedSource;
use crate::units::LenUnit;
use serde_json::{json, Value};

/// A size band's upper end at or above this is "no upper end" (the machine
/// files write 99999).
const OPEN_END: f64 = 9999.0;

/// What is known about one quality for the sheet in hand.
#[derive(Clone, Debug, PartialEq)]
pub struct QualityInfo {
    pub q: CutQuality,
    /// As screens show it: "Slow (1)" or "Quality 1" (CQ4, CQ7).
    pub label: String,
    pub name_from: NameFrom,
    /// The shop's note on what it is for (CQ7).
    pub note: Option<String>,
    pub colour: [u8; 3],
    /// Listed in the pickers (not `<HIDE>`).
    pub offered: bool,
    /// Cutting speed from the machine's technology table, mm/min; None when
    /// the table gives this quality none for the sheet (or there is no table).
    pub speed: Option<f64>,
    /// The table's pierce mode for it (1 smooth, 2 fast, 4 booster).
    pub pierce: Option<u32>,
    /// The size bands (equivalent radius from, to, mm) that give it to a
    /// contour on Auto (MTL10).
    pub bands: Vec<(f64, f64)>,
    /// Auto cuts at it when nothing else sets a quality (CQ1).
    pub fallback: bool,
    /// The post makes every mark at it (BL22; LC5 programs only).
    pub marking: bool,
}

/// What every quality means on one machine for one material, thickness and gas.
#[derive(Clone, Debug, PartialEq)]
pub struct Guide {
    pub machine: String,
    /// The program goes to a BLM LC5 (the quality number is all it sends).
    pub lc5: bool,
    pub material: String,
    pub thickness: f64,
    /// "Oxygen (3)".
    pub gas: String,
    /// The sizes Auto picks a quality by for this sheet, in order: radius
    /// from, to (mm), quality (CQ9, MTL10). Empty = none.
    pub bands: Vec<(f64, f64, CutQuality)>,
    /// Where they come from: the shop's table or the machine file.
    pub bands_from: crate::ct1::SizesFrom,
    /// The speeds come from the machine's technology table.
    pub has_table: bool,
    /// Where the speeds come from, one line (TT, CTM8).
    pub speeds: String,
    pub rows: Vec<QualityInfo>,
}

/// What each quality means for `spec` cut on `machine` with the values `mt`
/// gives it (CQ8).
pub fn guide(spec: &SheetSpec, mt: &crate::ct1::MachineTech, machine: &crate::settings::Machine) -> Guide {
    let timing = crate::cnc::Timing::for_spec(spec, mt);
    let sp = &timing.speeds;
    let has_table = matches!(sp.source, SpeedSource::Table(..));
    let (bands, bands_from) = mt.size_bands();
    let lc5 = machine.output == crate::settings::Output::BlmSiemens;
    let names = &mt.qualities;
    let rows = CutQuality::all()
        .map(|q| {
            let n = q.number();
            QualityInfo {
                q,
                label: names.label(q),
                name_from: names.name_from(q),
                note: names.note(q).map(str::to_string),
                colour: names.colour(q),
                offered: names.offered(q),
                speed: (has_table && sp.from_table[n]).then(|| sp.cut_for(n)),
                pierce: if has_table { sp.pierce_mode(n) } else { None },
                bands: bands.iter().filter(|b| b.2 == q).map(|b| (b.0, b.1)).collect(),
                fallback: q == CutQuality::FALLBACK,
                marking: lc5 && n == crate::blm::QUALITY_MARK,
            }
        })
        .collect();
    let gas = crate::gas::JobGas::of(spec.gas, machine.gas_id, &machine.gas_name);
    Guide {
        machine: machine.name.clone(),
        lc5,
        material: super::material::name_or(spec.material, "?"),
        thickness: spec.thickness,
        gas: gas.label(),
        bands,
        bands_from,
        has_table,
        speeds: sp.label(),
        rows,
    }
}

/// "soft", "fast", "booster" or the bare code (the post's pierce modes, BL22).
pub fn pierce_word(p: u32) -> String {
    match p {
        1 => "soft".into(),
        2 => "fast".into(),
        4 => "booster".into(),
        p => format!("type {p}"),
    }
}

/// A size band's sizes in shop words: "radius up to 5 mm", "radius 5–15 mm",
/// "radius 15 mm and up".
pub fn band_words(from: f64, to: f64, u: LenUnit) -> String {
    match (from <= 0.0, to >= OPEN_END) {
        (true, true) => "any size".into(),
        (true, false) => format!("radius up to {}", u.show(to)),
        (false, true) => format!("radius {} and up", u.show(from)),
        (false, false) => format!("radius {}–{}", u.num(from), u.show(to)),
    }
}

/// A speed in the unit the shop works in.
pub fn speed_words(mm_min: f64, u: LenUnit) -> String {
    match u {
        LenUnit::Mm => format!("{:.0} mm/min", mm_min),
        _ => format!("{:.0} in/min", mm_min / 25.4),
    }
}

impl QualityInfo {
    /// Something is known about it for this sheet: a name, a note, a
    /// table setting, a size band, or it is the fallback or the marking one.
    pub fn known(&self) -> bool {
        self.name_from != NameFrom::None || self.note.is_some() || self.speed.is_some() || self.pierce.is_some() || !self.bands.is_empty() || self.fallback || self.marking
    }

    /// Short facts for a picker row: "Auto: radius up to 5 mm · 1500 mm/min".
    pub fn facts(&self, u: LenUnit) -> Vec<String> {
        let mut f: Vec<String> = self.bands.iter().map(|b| format!("Auto: {}", band_words(b.0, b.1, u))).collect();
        if self.fallback {
            f.push("Auto's default".into());
        }
        if self.marking {
            f.push("etching".into());
        }
        if let Some(s) = self.speed {
            f.push(speed_words(s, u));
        }
        if let Some(p) = self.pierce {
            f.push(format!("{} pierce", pierce_word(p)));
        }
        f
    }

    /// One line under its name: the shop's note, else the facts, else
    /// that nothing is known.
    pub fn summary(&self, u: LenUnit) -> String {
        if let Some(n) = &self.note {
            return n.clone();
        }
        let f = self.facts(u);
        if f.is_empty() {
            "Not set up".into()
        } else {
            f.join(" · ")
        }
    }

    /// The hover card's sentences: what it is for, when Auto gives it,
    /// what the machine's table gives it, and what the program does with it.
    pub fn explain(&self, g: &Guide, u: LenUnit) -> Vec<String> {
        let n = self.q.number();
        let mut s = vec![];
        if let Some(note) = &self.note {
            s.push(note.clone());
        }
        if !self.bands.is_empty() {
            let b: Vec<String> = self.bands.iter().map(|b| band_words(b.0, b.1, u)).collect();
            s.push(format!("Auto uses it for contours with a {}.", b.join(", ")));
        }
        if self.fallback {
            s.push("Auto uses it when nothing else picks a quality.".into());
        }
        if self.marking {
            s.push("Etchings are made at this quality.".into());
        }
        match (g.has_table, self.speed, self.pierce) {
            (true, Some(v), p) => s.push(format!("On {}: {}{}.", g.sheet(u), speed_words(v, u), p.map_or(String::new(), |p| format!(", {} pierce", pierce_word(p))))),
            (true, None, Some(p)) => s.push(format!("On {}: {} pierce, no speed given.", g.sheet(u), pierce_word(p))),
            (true, None, None) => s.push(format!("The machine has no settings for it on {}.", g.sheet(u))),
            (false, ..) => {}
        }
        if g.lc5 {
            s.push(format!("The program only sends the number {n}. The laser cuts with its own settings saved under {n} for this material and thickness."));
        } else {
            s.push("It sets the contour's lead-ins, speed and cut time.".into());
        }
        s
    }
}

impl Guide {
    /// "Mild steel 3 mm, Oxygen (3)": the sheet the qualities are for.
    pub fn sheet(&self, u: LenUnit) -> String {
        format!("{} {}, {}", self.material, u.thickness(self.thickness), self.gas)
    }

    /// "LC5 · Mild steel 3 mm, Oxygen (3)": what the panel shows the qualities for.
    pub fn context(&self, u: LenUnit) -> String {
        format!("{} · {}", self.machine, self.sheet(u))
    }

    /// What Auto does for this sheet (CQ1, CQ9, MTL10), in shop words.
    pub fn auto_sentence(&self, u: LenUnit, names: &crate::qualities::CutQualities) -> String {
        let fb = names.label(CutQuality::FALLBACK);
        if self.bands.is_empty() {
            return format!("Auto cuts everything at {fb}: nothing picks by size for {}. Add sizes in Settings → Machines → Pick quality by size.", self.sheet(u));
        }
        let b: Vec<String> = self.bands.iter().map(|b| format!("{} → {}", band_words(b.0, b.1, u), names.label(b.2))).collect();
        let whose = if self.bands_from == crate::ct1::SizesFrom::Settings { " (your table in Settings → Machines)" } else { "" };
        format!("Auto picks by contour size{whose}: {}. Anything else: {fb}.", b.join(", "))
    }

    /// The quality for `q`.
    pub fn row(&self, q: CutQuality) -> &QualityInfo {
        &self.rows[q.number() - 1]
    }

    /// The questions a programmer asks about qualities, each with its answer
    /// for this machine and sheet (the panels' "About cut qualities").
    pub fn about(&self, u: LenUnit, names: &crate::qualities::CutQualities) -> Vec<(&'static str, String)> {
        let program = if self.lc5 { " The program only sends the number; the laser uses its own settings for it." } else { "" };
        vec![
            ("What is a quality?", format!("A saved set of cutting settings (power, gas, focus, speed), numbered 1–12. Every cut contour gets one.{program}")),
            (
                "Does it change with the machine?",
                format!("Yes. Number 1 can mean different settings on another machine, material or thickness. Shown here: {}.", self.context(u)),
            ),
            ("Does Auto pick the right one?", format!("{} A quality picked by hand, or set for the part, wins over Auto.", self.auto_sentence(u, names))),
            ("When do I pick one by hand?", "Rarely. Leave it on Auto unless a contour needs a different finish, e.g. a small hole that must come out clean and round (a slower quality).".into()),
            ("Where do I name them?", "Settings → Machines → Cut qualities: a name and a short note for each one your shop uses.".into()),
        ]
    }

    /// For agents (`qualities`).
    pub fn to_json(&self, u: LenUnit, names: &crate::qualities::CutQualities) -> Value {
        let mm = |x: f64| (x * 1000.0).round() / 1000.0;
        json!({
            "machine": self.machine, "lc5": self.lc5, "material": self.material, "thickness": self.thickness, "gas": self.gas,
            "speeds": self.speeds, "has_table": self.has_table,
            "auto": self.auto_sentence(u, names), "sizes_from": self.bands_from.key(),
            "size_bands": self.bands.iter().map(|b| json!({"from_r": mm(b.0), "to_r": if b.1 >= OPEN_END { Value::Null } else { json!(mm(b.1)) }, "quality": b.2.number(), "name": names.label(b.2)})).collect::<Vec<_>>(),
            "about": self.about(u, names).into_iter().map(|(q, a)| json!({"question": q, "answer": a})).collect::<Vec<_>>(),
            "qualities": self.rows.iter().map(|r| json!({
                "quality": r.q.number(), "name": r.label, "name_from": r.name_from.key(), "note": r.note, "colour": crate::qualities::hex(r.colour),
                "offered": r.offered, "known": r.known(),
                "cut_mm_min": r.speed.map(|v| v.round()), "pierce": r.pierce.map(pierce_word),
                "auto_bands": r.bands.iter().map(|b| json!({"from_r": mm(b.0), "to_r": if b.1 >= OPEN_END { Value::Null } else { json!(mm(b.1)) }})).collect::<Vec<_>>(),
                "fallback": r.fallback, "marking": r.marking,
                "summary": r.summary(u), "explain": r.explain(self, u),
            })).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nest::CutQuality;

    fn q(n: usize) -> CutQuality {
        CutQuality::new(n).unwrap()
    }

    /// A 3 mm mild steel sheet.
    fn sheet() -> SheetSpec {
        SheetSpec { w: 1000.0, h: 500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: None, pick: Default::default() }
    }

    fn lc5() -> crate::settings::Machine {
        crate::settings::Machine { name: "Demo LC5".into(), output: crate::settings::Output::BlmSiemens, gas_id: 3, gas_name: "Oxygen".into(), ..Default::default() }
    }

    #[test]
    fn with_no_table_bands_or_names_only_the_fallback_and_marking_qualities_are_known() {
        let spec = sheet();
        let g = guide(&spec, &crate::ct1::MachineTech::default(), &lc5());
        assert!(g.bands.is_empty() && !g.has_table);
        let known: Vec<usize> = g.rows.iter().filter(|r| r.known()).map(|r| r.q.number()).collect();
        assert_eq!(known, [3, 9]);
        assert_eq!(g.row(q(1)).summary(LenUnit::Mm), "Not set up");
        assert_eq!(g.row(q(3)).summary(LenUnit::Mm), "Auto's default");
        assert_eq!(g.row(q(9)).summary(LenUnit::Mm), "etching");
        let names = crate::qualities::CutQualities::default();
        assert!(g.auto_sentence(LenUnit::Mm, &names).starts_with("Auto cuts everything at Quality 3: nothing picks by size"), "{}", g.auto_sentence(LenUnit::Mm, &names));
        assert!(g.row(q(1)).explain(&g, LenUnit::Mm).iter().any(|s| s.contains("only sends the number 1")));
    }

    #[test]
    fn an_iso_machine_marks_with_no_quality_of_its_own() {
        let m = crate::settings::Machine { output: crate::settings::Output::Iso, ..lc5() };
        let g = guide(&sheet(), &crate::ct1::MachineTech::default(), &m);
        assert!(!g.lc5 && g.rows.iter().all(|r| !r.marking));
    }

    #[test]
    fn size_bands_say_which_sizes_auto_cuts_at_each_quality() {
        let ct1 = "Z000 \"*\"\nZ001 -1\nZ002 99\nZ010 0\nZ011 5\nZ012 0\nZ010 5\nZ011 15\nZ012 1\nZ010 15\nZ011 99999\nZ012 2\nZ999 0\n";
        let mt = crate::ct1::MachineTech::from_text(ct1).for_job(&crate::ct1::JobKeys::for_job("0010", 3.0, None));
        let g = guide(&sheet(), &mt, &lc5());
        assert_eq!(g.bands, vec![(0.0, 5.0, q(1)), (5.0, 15.0, q(2)), (15.0, 99999.0, q(3))]);
        assert_eq!(g.row(q(1)).facts(LenUnit::Mm), ["Auto: radius up to 5 mm"]);
        assert_eq!(g.row(q(2)).facts(LenUnit::Mm), ["Auto: radius 5–15 mm"]);
        assert_eq!(g.row(q(3)).facts(LenUnit::Mm)[0], "Auto: radius 15 mm and up");
        let names = crate::qualities::CutQualities::default();
        let auto = g.auto_sentence(LenUnit::Mm, &names);
        assert!(auto.contains("radius up to 5 mm → Quality 1, radius 5–15 mm → Quality 2, radius 15 mm and up → Quality 3"), "{auto}");
        assert_eq!(g.bands_from, crate::ct1::SizesFrom::MachineFile);
    }

    #[test]
    fn the_shops_size_table_wins_over_the_machine_files_for_the_sheets_it_fits() {
        let ct1 = "Z000 \"*\"\nZ001 -1\nZ002 99\nZ010 0\nZ011 5\nZ012 0\nZ999 0\n";
        let mut mt = crate::ct1::MachineTech::from_text(ct1);
        let rule = |m: &str, t_to: f64, r_to: f64, n: usize| crate::qualities::SizeRule { material: m.into(), t_from: 0.0, t_to, r_from: 0.0, r_to, quality: q(n) };
        mt.size_rules = std::sync::Arc::new(vec![rule("Mild steel", 6.0, 8.0, 2), rule("", 0.0, 0.0, 4)]);
        let keys = crate::ct1::JobKeys::for_job("0010", 3.0, None);
        let t = mt.for_job(&keys).with_sheet_sizes("Mild steel", 3.0);
        assert_eq!((t.band_quality(7.0), t.band_quality(9.0)), (Some(2), Some(4)), "first row that fits, then the any-size row");
        let g = guide(&sheet(), &t, &lc5());
        assert_eq!(g.bands_from, crate::ct1::SizesFrom::Settings);
        assert!(g.auto_sentence(LenUnit::Mm, &crate::qualities::CutQualities::default()).contains("(your table in Settings → Machines): radius up to 8 mm → Quality 2, any size → Quality 4"));
        // A stainless sheet: only the any-material row fits.
        let t = mt.for_job(&keys).with_sheet_sizes("Stainless steel", 10.0);
        assert_eq!(t.band_quality(1.0), Some(4));
        // No row fits: the machine file's band again.
        mt.size_rules = std::sync::Arc::new(vec![rule("Mild steel", 6.0, 8.0, 2)]);
        let t = mt.for_job(&keys).with_sheet_sizes("Aluminium", 3.0);
        assert_eq!((t.band_quality(4.0), t.size_bands().1), (Some(1), crate::ct1::SizesFrom::MachineFile));
    }
}
