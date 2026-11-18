//! How the machine's 12 cut qualities show on screen and to agents: each
//! one's name from the machine's technology file (CQ4) or the shop's own in
//! Settings → Machines (CQ7), the shop's note on what it is for (CQ7), and
//! its colour from the original's colour settings (CQ5).

use std::collections::HashMap;
use std::path::Path;

use crate::nest::CutQuality;

/// The name that hides a quality from the pickers (CQ4).
const HIDE: &str = "<HIDE>";

/// The colours the original ships for qualities 1–12 (CQ5).
const SHIPPED_COLOURS: [[u8; 3]; CutQuality::COUNT] = [
    [255, 128, 64],
    [0, 128, 192],
    [255, 0, 255],
    [0, 128, 0],
    [128, 0, 128],
    [128, 64, 64],
    [0, 128, 255],
    [0, 128, 128],
    [0, 64, 128],
    [128, 128, 255],
    [128, 255, 255],
    [255, 128, 255],
];

/// The machine's cut qualities as they show: names, notes and colours.
#[derive(Clone, Debug, PartialEq)]
pub struct CutQualities {
    /// Quality 1's name first; empty = the file gives none.
    names: [String; CutQuality::COUNT],
    /// The shop's own names and notes (CQ7); empty = none.
    shop: ShopQualities,
    /// Quality 1's colour first, RGB.
    colours: [[u8; 3]; CutQuality::COUNT],
}

impl Default for CutQualities {
    /// No names, the shipped colours.
    fn default() -> Self {
        CutQualities { names: Default::default(), shop: ShopQualities::default(), colours: SHIPPED_COLOURS }
    }
}

/// Where a quality's name on screen comes from (CQ4, CQ7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameFrom {
    /// The shop's own name in Settings → Machines.
    Settings,
    /// The machine's technology file (A922–A933).
    MachineFile,
    /// Nobody names it: it shows as "Quality n".
    None,
}

impl NameFrom {
    /// For agents.
    pub fn key(self) -> &'static str {
        match self {
            NameFrom::Settings => "settings",
            NameFrom::MachineFile => "machine file",
            NameFrom::None => "none",
        }
    }
}

/// The shop's own names for a machine's qualities and a note on what each
/// is for (CQ7), kept with the machine in Settings → Machines: the machine
/// files the shop has often name none, so the pickers would show bare
/// numbers. A name given here wins over the machine file's; `<HIDE>` leaves
/// the quality out of the pickers, as in the machine file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShopQualities {
    pub names: [String; CutQuality::COUNT],
    pub notes: [String; CutQuality::COUNT],
}

impl ShopQualities {
    /// Nothing given for any quality.
    pub fn is_empty(&self) -> bool {
        self.names.iter().chain(&self.notes).all(|s| s.trim().is_empty())
    }

    /// One settings line: `[{"q":1,"name":"Small holes","note":"…"}, …]`,
    /// only the qualities given something.
    pub fn to_line(&self) -> String {
        let rows: Vec<serde_json::Value> = (0..CutQuality::COUNT)
            .filter(|&k| !self.names[k].trim().is_empty() || !self.notes[k].trim().is_empty())
            .map(|k| {
                let mut v = serde_json::json!({"q": k + 1});
                if !self.names[k].trim().is_empty() {
                    v["name"] = self.names[k].trim().into();
                }
                if !self.notes[k].trim().is_empty() {
                    v["note"] = self.notes[k].trim().into();
                }
                v
            })
            .collect();
        serde_json::to_string(&rows).unwrap_or_default()
    }

    /// Read a settings line written by `to_line`; anything it can't read is
    /// left empty.
    pub fn read(line: &str) -> ShopQualities {
        let mut s = ShopQualities::default();
        let Ok(serde_json::Value::Array(rows)) = serde_json::from_str::<serde_json::Value>(line) else { return s };
        for r in rows {
            let Some(k) = r["q"].as_u64().map(|q| q as usize).filter(|q| (1..=CutQuality::COUNT).contains(q)) else { continue };
            s.names[k - 1] = r["name"].as_str().unwrap_or("").trim().chars().take(MAX_NAME).collect();
            s.notes[k - 1] = r["note"].as_str().unwrap_or("").trim().chars().take(MAX_NOTE).collect();
        }
        s
    }
}

/// One row of the shop's own "pick quality by size" table for a machine
/// (CQ9), kept in Settings → Machines: contours of a sheet of `material`
/// (its name; empty = any material) from `t_from` to `t_to` mm thick (0 =
/// no upper limit), with a size (radius of the circle with the same area)
/// from `r_from` up to `r_to` mm (0 = no upper limit), are cut at `quality`.
#[derive(Clone, Debug, PartialEq)]
pub struct SizeRule {
    pub material: String,
    pub t_from: f64,
    pub t_to: f64,
    pub r_from: f64,
    pub r_to: f64,
    pub quality: CutQuality,
}

impl Default for SizeRule {
    /// Any material and thickness, holes up to 5 mm radius at quality 1.
    fn default() -> Self {
        SizeRule { material: String::new(), t_from: 0.0, t_to: 0.0, r_from: 0.0, r_to: crate::nest::SMALL_HOLE_RADIUS_EXAMPLE, quality: CutQuality::new(1).unwrap_or(CutQuality::FALLBACK) }
    }
}

/// How near a thickness may be to a row's limits and still fit, mm (as the
/// machine file's sections, MTL3).
const T_TOL: f64 = 0.005;

impl SizeRule {
    /// The row is for a sheet of material `name` and `thickness` mm.
    pub fn fits_sheet(&self, name: &str, thickness: f64) -> bool {
        let m = self.material.trim();
        (m.is_empty() || m.eq_ignore_ascii_case(name.trim())) && thickness >= self.t_from - T_TOL && (self.t_to <= 0.0 || thickness <= self.t_to + T_TOL)
    }

    /// The row's upper size, mm, with "no limit" as infinity.
    pub fn r_upto(&self) -> f64 {
        if self.r_to <= 0.0 {
            f64::INFINITY
        } else {
            self.r_to
        }
    }

    /// The row's upper thickness, mm, with "no limit" as infinity.
    fn t_upto(&self) -> f64 {
        if self.t_to <= 0.0 {
            f64::INFINITY
        } else {
            self.t_to
        }
    }

    /// Why the row can never be used, in shop words: its material isn't in
    /// the list `materials` (renamed or removed; an empty list can't tell),
    /// or a range ends where it starts. None when it can.
    pub fn problem(&self, materials: &[String]) -> Option<String> {
        let m = self.material.trim();
        if !m.is_empty() && !materials.is_empty() && !materials.iter().any(|n| n.trim().eq_ignore_ascii_case(m)) {
            return Some(format!("there is no material named \"{m}\" now, so no sheet fits it. Pick its material again."));
        }
        if self.t_to > 0.0 && self.t_to < self.t_from {
            return Some("its thickness \"to\" is below \"from\", so no sheet fits it.".into());
        }
        if self.r_to > 0.0 && self.r_to <= self.r_from {
            return Some("its size \"up to\" isn't above \"from\", so no contour fits it.".into());
        }
        None
    }

    /// Some sheet and contour fit both this row and `o`.
    fn overlaps(&self, o: &SizeRule) -> bool {
        let (a, b) = (self.material.trim(), o.material.trim());
        (a.is_empty() || b.is_empty() || a.eq_ignore_ascii_case(b)) && self.t_from <= o.t_upto() && o.t_from <= self.t_upto() && self.r_from < o.r_upto() && o.r_from < self.r_upto()
    }
}

/// The first two rows some contour fits both of (CQ9): (earlier, later),
/// numbered from 0. Where both fit, the earlier is used. Rows that can't
/// be used (`SizeRule::problem`) are left out.
pub fn sizes_overlap(rules: &[SizeRule], materials: &[String]) -> Option<(usize, usize)> {
    let ok = |r: &SizeRule| r.problem(materials).is_none();
    (0..rules.len()).flat_map(|a| (a + 1..rules.len()).map(move |b| (a, b))).find(|&(a, b)| ok(&rules[a]) && ok(&rules[b]) && rules[a].overlaps(&rules[b]))
}

/// The shop's size rows for a sheet (CQ9): (size from, up to, quality) of
/// every row that fits its material and thickness, in table order; None
/// when none fits (the machine file's own size bands then apply, MTL10).
pub fn sheet_sizes(rules: &[SizeRule], material: &str, thickness: f64) -> Option<Vec<(f64, f64, CutQuality)>> {
    let v: Vec<(f64, f64, CutQuality)> = rules.iter().filter(|r| r.fits_sheet(material, thickness)).map(|r| (r.r_from, r.r_upto(), r.quality)).collect();
    (!v.is_empty()).then_some(v)
}

/// The quality the first row a contour of size `r` fits gives it (CQ9).
pub fn size_quality(rows: &[(f64, f64, CutQuality)], r: f64) -> Option<CutQuality> {
    rows.iter().find(|b| b.0 <= r && r < b.1).map(|b| b.2)
}

/// The shop's size rows as one settings line (JSON list).
pub fn sizes_to_line(rules: &[SizeRule]) -> String {
    let rows: Vec<serde_json::Value> = rules
        .iter()
        .map(|r| serde_json::json!({"material": r.material.trim(), "t_from": r.t_from, "t_to": r.t_to, "r_from": r.r_from, "r_to": r.r_to, "q": r.quality.number()}))
        .collect();
    serde_json::to_string(&rows).unwrap_or_default()
}

/// Read a settings line written by `sizes_to_line`; a row it can't read
/// (no quality 1–12) is left out.
pub fn sizes_read(line: &str) -> Vec<SizeRule> {
    let Ok(serde_json::Value::Array(rows)) = serde_json::from_str::<serde_json::Value>(line) else { return vec![] };
    let num = |v: &serde_json::Value| v.as_f64().filter(|x| x.is_finite() && *x >= 0.0).unwrap_or(0.0);
    rows.iter()
        .filter_map(|r| {
            let q = r["q"].as_u64().and_then(|q| CutQuality::new(q as usize))?;
            Some(SizeRule { material: r["material"].as_str().unwrap_or("").trim().to_string(), t_from: num(&r["t_from"]), t_to: num(&r["t_to"]), r_from: num(&r["r_from"]), r_to: num(&r["r_to"]), quality: q })
        })
        .collect()
}

/// Longest name the shop can give a quality, characters (the machine file's are 40).
pub const MAX_NAME: usize = 40;
/// Longest note on what a quality is for, characters.
pub const MAX_NOTE: usize = 200;

impl CutQualities {
    /// From the machine file's own values (CQ4): A922 names quality 1 …
    /// A933 quality 12, in quotes.
    pub fn from_codes(m: &HashMap<String, String>) -> CutQualities {
        let mut q = CutQualities::default();
        for (k, name) in q.names.iter_mut().enumerate() {
            if let Some(v) = m.get(&format!("A{}", 922 + k)) {
                *name = v.trim().trim_matches('"').trim().to_string();
            }
        }
        q
    }

    /// Take the colours of the original's colour settings (CQ5): its
    /// `LANTEKIN.CNI` lines `Cnnn R G B style width`, C041 for quality 1 …
    /// C052 for quality 12. A quality without a line keeps its colour.
    pub fn with_colour_settings(mut self, text: &str) -> CutQualities {
        for line in text.lines() {
            let mut w = line.split(';').next().unwrap_or("").split_whitespace();
            let Some(n) = w.next().and_then(|c| c.strip_prefix('C')).filter(|n| n.len() == 3).and_then(|n| n.parse::<usize>().ok()) else { continue };
            let rgb: Vec<u8> = w.take(3).filter_map(|v| v.parse::<i64>().ok()).map(|v| v.clamp(0, 255) as u8).collect();
            if (41..41 + CutQuality::COUNT).contains(&n) && rgb.len() == 3 {
                self.colours[n - 41] = [rgb[0], rgb[1], rgb[2]];
            }
        }
        self
    }

    /// The colours of the colour settings in a Lantek folder, when it has them.
    pub fn with_colours_in(self, lantek_dir: &Path) -> CutQualities {
        match std::fs::read(lantek_dir.join("LANTEKIN.CNI")) {
            Ok(b) => self.with_colour_settings(&b.iter().map(|&c| c as char).collect::<String>()),
            Err(_) => self,
        }
    }

    /// Lay the shop's own names and notes over the machine file's (CQ7).
    pub fn with_shop(mut self, shop: &ShopQualities) -> CutQualities {
        self.shop = shop.clone();
        self
    }

    /// The name in use for quality `q` as written: the shop's, else the
    /// machine file's; empty = none.
    fn raw(&self, q: CutQuality) -> &str {
        let k = q.number() - 1;
        let shop = self.shop.names[k].trim();
        if shop.is_empty() {
            self.names[k].as_str()
        } else {
            shop
        }
    }

    /// The name for quality `q`, when the shop or the machine gives one (CQ4, CQ7).
    pub fn name(&self, q: CutQuality) -> Option<&str> {
        let n = self.raw(q);
        (!n.is_empty() && n != HIDE).then_some(n)
    }

    /// The machine file's own name for quality `q`, when it gives one.
    pub fn file_name(&self, q: CutQuality) -> Option<&str> {
        let n = self.names[q.number() - 1].as_str();
        (!n.is_empty() && n != HIDE).then_some(n)
    }

    /// Where quality `q`'s name comes from (CQ4, CQ7).
    pub fn name_from(&self, q: CutQuality) -> NameFrom {
        let k = q.number() - 1;
        if !self.shop.names[k].trim().is_empty() {
            NameFrom::Settings
        } else if self.file_name(q).is_some() {
            NameFrom::MachineFile
        } else {
            NameFrom::None
        }
    }

    /// The shop's note on what quality `q` is for (CQ7), when it gave one.
    pub fn note(&self, q: CutQuality) -> Option<&str> {
        let n = self.shop.notes[q.number() - 1].trim();
        (!n.is_empty()).then_some(n)
    }

    /// Whether the pickers offer quality `q` (CQ4, CQ7): not when the name in
    /// use is `<HIDE>`. The machine still has it: size bands and agents may
    /// still give it.
    pub fn offered(&self, q: CutQuality) -> bool {
        self.raw(q) != HIDE
    }

    /// Quality `q` as screens and agents show it (CQ4, CQ7): "Slow (1)" when
    /// the shop or the machine names it, else "Quality 1".
    pub fn label(&self, q: CutQuality) -> String {
        match self.name(q) {
            Some(n) => format!("{n} ({})", q.number()),
            None => q.label(),
        }
    }

    /// Quality `q`'s colour, RGB (CQ5).
    pub fn colour(&self, q: CutQuality) -> [u8; 3] {
        self.colours[q.number() - 1]
    }

    /// Quality `q`'s colour as `#rrggbb`.
    pub fn colour_hex(&self, q: CutQuality) -> String {
        hex(self.colour(q))
    }
}

/// A colour as `#rrggbb`.
pub fn hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(n: usize) -> CutQuality {
        CutQuality::new(n).unwrap()
    }

    #[test]
    fn the_shops_names_win_over_the_machine_files_and_notes_say_what_each_is_for() {
        let m: HashMap<String, String> = [("A922", "\"Slow\""), ("A924", "\"Fast\""), ("A931", "\"<HIDE>\"")].into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let file = CutQualities::from_codes(&m);
        let mut shop = ShopQualities::default();
        shop.names[0] = "Small holes".into();
        shop.notes[0] = "Round holes under 10 mm".into();
        shop.notes[2] = "Outsides".into();
        let all = file.clone().with_shop(&shop);
        assert_eq!(all.label(q(1)), "Small holes (1)");
        assert_eq!((all.name_from(q(1)), all.file_name(q(1))), (NameFrom::Settings, Some("Slow")));
        assert_eq!((all.label(q(3)), all.name_from(q(3)), all.note(q(3))), ("Fast (3)".to_string(), NameFrom::MachineFile, Some("Outsides")));
        assert_eq!((all.label(q(2)), all.name_from(q(2)), all.note(q(2))), ("Quality 2".to_string(), NameFrom::None, None));
        assert!(!all.offered(q(10)), "the machine file's <HIDE> still hides");
        shop.names[9] = "Centre mark".into();
        assert!(file.with_shop(&shop).offered(q(10)), "a shop name over <HIDE> brings it back");
    }

    #[test]
    fn the_shops_size_rows_fit_by_material_and_thickness_and_the_first_size_row_wins() {
        let rows = vec![
            SizeRule { material: "Mild steel".into(), t_from: 2.0, t_to: 6.0, r_from: 0.0, r_to: 5.0, quality: q(1) },
            SizeRule { material: "".into(), t_from: 0.0, t_to: 0.0, r_from: 0.0, r_to: 15.0, quality: q(2) },
            SizeRule { material: "Stainless steel".into(), t_from: 0.0, t_to: 0.0, r_from: 0.0, r_to: 3.0, quality: q(4) },
        ];
        let mild = sheet_sizes(&rows, "mild steel", 3.0).unwrap();
        assert_eq!(mild, vec![(0.0, 5.0, q(1)), (0.0, 15.0, q(2))], "the material's name in any case; any material too");
        assert_eq!((size_quality(&mild, 4.9), size_quality(&mild, 5.0), size_quality(&mild, 15.0)), (Some(q(1)), Some(q(2)), None));
        assert_eq!(sheet_sizes(&rows, "Mild steel", 6.004).map(|v| v.len()), Some(2), "within 0.005 mm of the limit");
        assert_eq!(sheet_sizes(&rows, "Mild steel", 8.0).map(|v| v.len()), Some(1), "past 6 mm only the any-material row");
        assert_eq!(sheet_sizes(&rows[..1], "Aluminium", 3.0), None, "nothing fits: the machine file decides");
        let line = sizes_to_line(&rows);
        assert_eq!(sizes_read(&line), rows);
        assert!(sizes_read(r#"[{"q":13},{"material":"x"}]"#).is_empty());
        assert!(sizes_read("not a list").is_empty());
    }

    #[test]
    fn a_size_row_that_can_never_be_used_or_is_partly_hidden_by_an_earlier_one_is_told() {
        let list: Vec<String> = vec!["Mild steel".into(), "Stainless steel".into()];
        let row = |m: &str, t: (f64, f64), r: (f64, f64)| SizeRule { material: m.into(), t_from: t.0, t_to: t.1, r_from: r.0, r_to: r.1, quality: q(1) };
        assert_eq!(row("mild steel", (0.0, 0.0), (0.0, 5.0)).problem(&list), None, "names in any case");
        assert!(row("Mild steel (old)", (0.0, 0.0), (0.0, 5.0)).problem(&list).unwrap().contains("no material named \"Mild steel (old)\""));
        assert_eq!(row("Anything", (0.0, 0.0), (0.0, 5.0)).problem(&[]), None, "no list: can't tell");
        assert!(row("", (6.0, 3.0), (0.0, 5.0)).problem(&list).unwrap().contains("thickness"));
        assert!(row("", (0.0, 0.0), (5.0, 5.0)).problem(&list).unwrap().contains("size"));
        assert_eq!(row("", (6.0, 0.0), (5.0, 0.0)).problem(&list), None, "0 = no upper limit");

        let small = row("Mild steel", (2.0, 6.0), (0.0, 5.0));
        // Sizes that only touch, other materials and other thicknesses don't overlap.
        assert_eq!(sizes_overlap(&[small.clone(), row("", (0.0, 0.0), (5.0, 15.0))], &list), None);
        assert_eq!(sizes_overlap(&[small.clone(), row("Stainless steel", (0.0, 0.0), (0.0, 5.0))], &list), None);
        assert_eq!(sizes_overlap(&[small.clone(), row("", (8.0, 0.0), (0.0, 5.0))], &list), None);
        assert_eq!(sizes_overlap(&[small.clone(), row("Stainless steel", (0.0, 0.0), (0.0, 3.0)), row("", (0.0, 0.0), (0.0, 15.0))], &list), Some((0, 2)));
        assert_eq!(sizes_overlap(&[row("Gone", (0.0, 0.0), (0.0, 5.0)), small], &list), None, "a row that fits nothing hides nothing");
    }

    #[test]
    fn the_shops_names_and_notes_read_back_from_their_settings_line() {
        let mut shop = ShopQualities::default();
        assert!(shop.is_empty());
        assert_eq!(shop.to_line(), "[]");
        shop.names[0] = " Small holes ".into();
        shop.notes[8] = "Etching \"part numbers\"".into();
        let line = shop.to_line();
        assert_eq!(line, r#"[{"name":"Small holes","q":1},{"note":"Etching \"part numbers\"","q":9}]"#);
        let back = ShopQualities::read(&line);
        assert_eq!((back.names[0].as_str(), back.notes[8].as_str()), ("Small holes", "Etching \"part numbers\""));
        assert!(ShopQualities::read("not a list").is_empty());
        assert!(ShopQualities::read(r#"[{"q":13,"name":"x"},{"q":0,"name":"y"}]"#).is_empty(), "no quality outside 1–12");
    }
}
