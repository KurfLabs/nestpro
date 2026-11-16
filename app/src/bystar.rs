//! Writing programs for a ByStar Fiber laser (`.bvc`) as the original's
//! ByStar Fiber post writes them (BV21–BV55, BV56–BV62): per nest a cut
//! program (`<program>.ccp`, the machine's XML list of commands) with one
//! subprogram per placed part, and a job file (`JobInfo.xml`), zipped.
//!
//! The writer is a plain function over a sheet's plan; before anything is
//! written the files are read back by `bvc.rs`, which shares nothing with
//! this module, and compared with the plan (`engine::readback::check_bvc`).
//! Every switch of the original's post is here with its default (the
//! shipped value); the ones this writer doesn't follow yet are kept and shown
//! for later work, and a sheet they would change is refused, never written
//! another way (BV60).

use crate::cnc::{Made, SheetPlan};
use crate::cutoff::CutLine;
use crate::nest::{Part, Placed, SheetSpec, P};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;

// ---------------------------------------------------------------------------
// The post's switches (BV54 and the switch table).
// ---------------------------------------------------------------------------

/// What kind of value a switch takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// On above 0.5 (BV54).
    OnOff,
    /// A length, rounded to whole units (B003–B006, BV54).
    Length,
    /// A length used as it is.
    Size,
    /// One of the machine's laser qualities 1–12 (0 = none).
    Quality,
    /// 0, 1 or 2 (B036, B039).
    Choice,
    Text,
}

/// One switch of the original's ByStar Fiber post.
#[derive(Clone, Copy, Debug)]
pub struct SwitchDef {
    pub key: &'static str,
    pub what: &'static str,
    pub kind: Kind,
    /// The value the original ships with (BV54's table).
    pub default: &'static str,
    /// The rule that says what it does.
    pub rule: &'static str,
    /// What this writer does with it: `Built` it follows; `Later` it keeps
    /// and shows (and refuses a sheet it would change, BV60); `None` it has
    /// no effect on the files in the original either (BV45).
    pub status: Status,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Built,
    Later,
    NoEffect,
}

const fn sw(key: &'static str, what: &'static str, kind: Kind, default: &'static str, rule: &'static str, status: Status) -> SwitchDef {
    SwitchDef { key, what, kind, default, rule, status }
}

use Kind::*;
use Status::*;

/// Every switch of the post, in the file's order, with the original's
/// shipped value (BV54's switch table).
pub const SWITCHES: &[SwitchDef] = &[
    sw("B000", "Not used by the post", OnOff, "0", "BV45", NoEffect),
    sw("B001", "Measurement system text: inch (Imperial)", OnOff, "0", "BV25", Built),
    sw("B002", "Put every cut, cut-off lines too, into subprograms", OnOff, "0", "BV28", Built),
    sw("B003", "Cut-off lines: overrun past each sheet edge", Length, "0", "BV38", Built),
    sw("B004", "Cut-off lines: height sensing off for this distance before each edge", Length, "0", "BV38", Built),
    sw("B005", "Cut-off lines: run-up between the pierce and the sensing-off point", Length, "0", "BV38", Built),
    sw("B006", "Split the first move of a contour without a lead-in at this distance", Length, "0", "BV34", Built),
    sw("B007", "Quality that pierces Normal", Quality, "0", "BV33", Built),
    sw("B008", "Quality that pierces Fine", Quality, "0", "BV33", Built),
    sw("B009", "Quality that pierces NoTime", Quality, "0", "BV33", Built),
    sw("B010", "Keep only the .bvc, delete the loose files", OnOff, "0", "BV22", Built),
    sw("B011", "Compensation after the rapid instead of before it", OnOff, "0", "BV29", Built),
    sw("B012", "Compensation off after a mark", OnOff, "0", "BV31", Built),
    sw("B013", "Point marks as circles of this diameter", Size, "0.2", "BV40", Built),
    sw("B014", "Pierce mode from the technology table", OnOff, "0", "BV33", Built),
    sw("B015", "Operation mode from the technology table", OnOff, "0", "BV32", Built),
    sw("B016", "Subprogram name also as PartNumber; cut-off subprograms numbered", OnOff, "0", "BV26", Built),
    sw("B017", "All sheets of one run in one .bvc", OnOff, "0", "BV22", Built),
    sw("B018", "Split arcs of 360° or more into half circles", OnOff, "0", "BV35", Built),
    sw("B019", "Joint marker at the end: before the next rapid or cut-out end", OnOff, "0", "BV48", Later),
    sw("B020", "Only lines marked as cut-off lines are cut-offs", OnOff, "0", "BV39", Built),
    sw("B021", "Part call positions with 3 decimals instead of whole units", OnOff, "0", "BV52", Built),
    sw("B022", "Part id and quantity under 0.5 are fixed", OnOff, "0", "BV44", Built),
    sw("B023", "Pierce at a joint's lead-in", OnOff, "0", "BV48", Later),
    sw("B024", "Lead-in bracket and pierce at every contour start", OnOff, "0", "BV34", Built),
    sw("B025", "A cut-off line not starting on an edge gets its own rapid and pierce", OnOff, "0", "BV39", Built),
    sw("B026", "Quality used for marking: mode 2, Normal pierce", Quality, "0", "BV32", Built),
    sw("B027", "PartHeight in every subprogram header", OnOff, "0", "BV26", Built),
    sw("B028", "Cut-off subprograms named like part subprograms", OnOff, "0", "BV26", Built),
    sw("B029", "Empty ResidualSheetInfos in the job file", OnOff, "0", "BV43", Built),
    sw("B030", "Empty BigPartInfos in the job file", OnOff, "0", "BV43", Built),
    sw("B031", "No compensation on marks and point marks", OnOff, "0", "BV31", Built),
    sw("B032", "Close an open cut-out at the end of a subprogram", OnOff, "0", "BV28", Built),
    sw("B033", "Point-mark circles in marking mode 2", OnOff, "0", "BV40", Built),
    sw("B034", "Point marks as a cross of this size instead of a circle", Size, "0", "BV40", Later),
    sw("B035", "Two-line cut-off mode", OnOff, "0", "BV39", Later),
    sw("B036", "New marking treatment: compensation off, Interior, pierce Fine (2: Normal)", Choice, "0", "BV29", Built),
    sw("B037", "Never NoTime on partly cut common-cut contours", OnOff, "0", "BV33", Later),
    sw("B038", "New micro-joint treatment: stop at the gap and start again", OnOff, "0", "BV48", Built),
    sw("B039", "Height sensor: 1 not switched on for marks, 2 only in the cut-off sequence", Choice, "0", "BV37", Built),
    sw("B040", "Compensation after the lead-in", OnOff, "0", "BV31", Built),
    sw("B041", "ToolArrangement=\"Vertical\" in the header", OnOff, "0", "BV23", Built),
    sw("B042", "Quality that replaces quality 7", Quality, "0", "BV32", Built),
    sw("B043", "Quality that replaces quality 3", Quality, "0", "BV32", Built),
    sw("B044", "Pierce contours that have no lead-in", OnOff, "0", "BV29", Built),
    sw("B045", "Quality for cut-off lines", Quality, "0", "BV32", Built),
    sw("B046", "New cut-off management", OnOff, "0", "BV39", Later),
    sw("B047", "Joint marker before a joint cut in the contour's second-to-last place", OnOff, "0", "BV48", Later),
    sw("B048", "Per-edge cut-off distances R000–R007", OnOff, "0", "BV39", Later),
    sw("B049", "Joints always straight", OnOff, "0", "BV46", Built),
    sw("B050", "On/off points cut as a tiny cut when no circle is made", OnOff, "0", "BV55", Built),
    sw("C001", "Material user info 1", Text, "", "BV45", NoEffect),
    sw("C002", "Material user info 2", Text, "", "BV45", NoEffect),
    sw("C003", "Material user info 3", Text, "", "BV45", NoEffect),
    sw("C004", "Subprogram name prefix", Text, "", "BV26", Built),
    sw("C005", "Guid", Text, "", "BV45", NoEffect),
    sw("C006", "Job file parameter", Text, "", "BV45", NoEffect),
    sw("C007", "Job user info 1", Text, "", "BV45", NoEffect),
    sw("C008", "Job user info 2", Text, "", "BV45", NoEffect),
    sw("C009", "Job user info 3", Text, "", "BV45", NoEffect),
    sw("C010", "Part user info 1", Text, "", "BV45", NoEffect),
    sw("C011", "Part user info 2", Text, "", "BV45", NoEffect),
    sw("C012", "Part user info 3", Text, "", "BV45", NoEffect),
    sw("C013", "Part order info", Text, "", "BV45", NoEffect),
    sw("R000", "Cut-off distance inside the sheet, bottom edge", Size, "0", "BV39", Later),
    sw("R001", "Cut-off distance inside the sheet, left edge", Size, "0", "BV39", Later),
    sw("R002", "Cut-off distance inside the sheet, top edge", Size, "0", "BV39", Later),
    sw("R003", "Cut-off distance inside the sheet, right edge", Size, "0", "BV39", Later),
    sw("R004", "Cut-off distance outside the sheet, bottom edge", Size, "0", "BV39", Later),
    sw("R005", "Cut-off distance outside the sheet, left edge", Size, "0", "BV39", Later),
    sw("R006", "Cut-off distance outside the sheet, top edge", Size, "0", "BV39", Later),
    sw("R007", "Cut-off distance outside the sheet, right edge", Size, "0", "BV39", Later),
    // This app's own switches (the post has none for these), off by default.
    sw("X001", "This app's own: name the job's gas (ProcessGas) in the job file without a parameter-file name", OnOff, "0", "BV62", Built),
];

pub fn switch(key: &str) -> Option<&'static SwitchDef> {
    SWITCHES.iter().find(|s| s.key.eq_ignore_ascii_case(key))
}

/// Pierce modes the machine knows (BV12), by name.
pub const PIERCES: &[&str] = &["Normal", "Fine", "Drilled", "NoTime"];

/// A ByStar Fiber machine's post as Settings → Machines keeps it: the
/// original's post files, the switch values changed here (they win over the
/// files) and the technology table's ByStar Fiber columns changed from the
/// seed (BV58). Saved only when something is set, so settings saved before
/// load and save as they were (BV61).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MachinePost {
    /// The original's parameter file for this post (`.ps1`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ps1: String,
    /// Its post library (`.CF1`: machine tolerance A005, output unit A006).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cf1: String,
    /// Switch values changed here, by key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub set: BTreeMap<String, String>,
    /// Operation mode per quality (1–12) changed from the seed table.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub modes: BTreeMap<u8, i64>,
    /// Pierce mode per quality (0 = marking, 1–12) changed from the seed
    /// table; empty text = no pierce commands.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pierces: BTreeMap<u8, String>,
}

impl MachinePost {
    pub fn is_empty(&self) -> bool {
        *self == MachinePost::default()
    }

    /// Set switch `key` (any case) to `value`, checked against its kind.
    /// Setting the value the files give takes the change out again.
    pub fn set_switch(&mut self, key: &str, value: &str) -> Result<(), String> {
        let def = switch(key).ok_or_else(|| format!("{key} isn't one of the post's switches (B000–B050, C001–C013, R000–R007, and this app's X001)"))?;
        let v = value.trim();
        if def.kind != Text && v.parse::<f64>().map_or(true, |x| !x.is_finite()) {
            return Err(format!("{}: \"{v}\" isn't a number", def.key));
        }
        if def.kind == Text && v.contains(['"', '\r', '\n']) {
            return Err(format!("{}: a text can't hold quotes or line breaks", def.key));
        }
        self.set.remove(def.key);
        let file = Post::load(self).raw(def.key).to_string();
        if file != v {
            self.set.insert(def.key.to_string(), v.to_string());
        }
        Ok(())
    }
}

/// The seed technology table's ByStar Fiber columns (BV58, estimated): each
/// quality's operation mode is what BV32 gives with B015 off, every cut
/// pierces `Fine` and marking `Normal` (BV33 with B014 off), so turning B015
/// or B014 on changes nothing until the table is changed.
pub fn seed_mode(q: usize) -> i64 {
    match q {
        2 => 1,
        3..=7 => q as i64,
        _ => 0,
    }
}

pub fn seed_pierce(q: usize) -> &'static str {
    if q == 0 { "Normal" } else { "Fine" }
}

/// A post as the writer uses it: every switch's value (changed here, else
/// the post file's, else the original's default), the CF1's tolerance and
/// output unit, and the technology table's columns.
#[derive(Clone, Debug, PartialEq)]
pub struct Post {
    values: BTreeMap<&'static str, String>,
    /// Where each value came from: "set", "file" or "default".
    pub from: BTreeMap<&'static str, &'static str>,
    /// CF1 A005: the machine tolerance, mm (0.005 without one).
    pub tol: f64,
    /// CF1 A006: mm per program unit (1 = mm, 25.4 = inch).
    pub unit: f64,
    /// Operation mode by quality (index 1–12).
    pub modes: [i64; 13],
    /// Pierce text by quality (index 0 = marking, 1–12).
    pub pierces: [String; 13],
    /// CF1 A004: the point cycle's type (1 punch-mark point, 2 on/off point; BV55).
    pub point_type: i64,
    /// The post file couldn't be read: why.
    pub file_error: Option<String>,
}

impl Default for Post {
    fn default() -> Self {
        Post::parse(None, None, &MachinePost::default())
    }
}

/// The value of a `Kxxx value ; comment` line: a quoted text, or the first
/// word after the key.
fn line_value(text: &str, key: &str) -> Option<String> {
    let l = text.lines().find(|l| l.split_whitespace().next().is_some_and(|k| k.eq_ignore_ascii_case(key)))?;
    let rest = l.trim_start()[key.len()..].trim_start();
    if let Some(q) = rest.strip_prefix('"') {
        return Some(q.split('"').next().unwrap_or("").to_string());
    }
    rest.split([';', ' ', '\t']).next().map(|s| s.trim().to_string())
}

fn read_file(path: &str) -> Result<Option<String>, String> {
    if path.trim().is_empty() {
        return Ok(None);
    }
    // The original's files are in the PC's code page: read as Latin-1, so
    // a byte never stops the read.
    std::fs::read(path.trim()).map(|b| Some(b.iter().map(|&c| c as char).collect())).map_err(|e| format!("{}: {e}", path.trim()))
}

impl Post {
    /// The post of a machine's settings, its files read now.
    pub fn load(m: &MachinePost) -> Post {
        let (ps1, e1) = match read_file(&m.ps1) {
            Ok(t) => (t, None),
            Err(e) => (None, Some(e)),
        };
        let (cf1, e2) = match read_file(&m.cf1) {
            Ok(t) => (t, None),
            Err(e) => (None, Some(e)),
        };
        let mut p = Post::parse(ps1.as_deref(), cf1.as_deref(), m);
        p.file_error = e1.or(e2);
        p
    }

    /// BV54: with a parameter file, a switch missing from it is 0 (B013 is
    /// 10); without one, every switch has the original's shipped value.
    /// Values changed in Settings win.
    pub fn parse(ps1: Option<&str>, cf1: Option<&str>, m: &MachinePost) -> Post {
        let mut values = BTreeMap::new();
        let mut from = BTreeMap::new();
        for d in SWITCHES {
            let (v, f) = if let Some(v) = m.set.get(d.key) {
                (v.clone(), "set")
            } else if let Some(text) = ps1 {
                match line_value(text, d.key) {
                    Some(v) => (v, "file"),
                    None if d.key == "B013" => ("10".into(), "file"),
                    None if d.kind == Text => (String::new(), "file"),
                    None => ("0".into(), "file"),
                }
            } else {
                (d.default.to_string(), "default")
            };
            values.insert(d.key, v);
            from.insert(d.key, f);
        }
        let num = |k: &str| cf1.and_then(|t| line_value(t, k)).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite() && *v > 0.0);
        let mut modes = [0i64; 13];
        let mut pierces: [String; 13] = Default::default();
        for q in 0..=12 {
            modes[q] = m.modes.get(&(q as u8)).copied().unwrap_or(seed_mode(q));
            pierces[q] = m.pierces.get(&(q as u8)).cloned().unwrap_or_else(|| seed_pierce(q).to_string());
        }
        let point_type = cf1.and_then(|t| line_value(t, "A004")).and_then(|v| v.parse::<f64>().ok()).map_or(2, |v| v.round() as i64);
        Post { values, from, tol: num("A005").unwrap_or(0.005), unit: num("A006").unwrap_or(1.0), modes, pierces, point_type, file_error: None }
    }

    pub fn raw(&self, key: &str) -> &str {
        self.values.get(key).map_or("", |s| s.as_str())
    }

    pub fn num(&self, key: &str) -> f64 {
        self.raw(key).trim().parse::<f64>().ok().filter(|v| v.is_finite()).unwrap_or(0.0)
    }

    /// BV54: an on/off switch is on above 0.5.
    pub fn on(&self, key: &str) -> bool {
        self.num(key) > 0.5
    }

    /// BV54: B003–B009, B036, B039 are rounded to the nearest whole number,
    /// halves down (0.5 → 0, 2.5 → 2, −0.5 → −1).
    pub fn whole(&self, key: &str) -> i64 {
        (self.num(key) - 0.5).ceil() as i64
    }

    /// BV54: B026, B042, B043 are cut to whole numbers (2.9 → 2).
    pub fn cut(&self, key: &str) -> i64 {
        self.num(key).trunc() as i64
    }

    /// BV54: B039 2 = no switching; any other non-zero value acts as 1.
    pub fn sensor_mode(&self) -> i64 {
        match self.whole("B039") {
            0 => 0,
            2 => 2,
            _ => 1,
        }
    }

    /// Whether a switch is at its shipped value (BV60 refuses a sheet a
    /// changed switch would change that the writer doesn't follow yet).
    pub fn changed(&self, key: &str) -> bool {
        let d = switch(key).map_or("0", |d| d.default);
        let (a, b) = (self.raw(key).trim(), d);
        match (a.parse::<f64>(), b.parse::<f64>()) {
            (Ok(x), Ok(y)) => x != y,
            _ => a != b,
        }
    }

    /// BV32: the operation mode (the machine's technology column) of laser
    /// quality `q` on a cut contour: B042 replaces quality 7 and B043
    /// quality 3; the B026 marking quality → 2; with B015 the technology
    /// table's value, else 2→1, 3–7 themselves, any other → 0.
    pub fn mode(&self, q: usize) -> i64 {
        let mut q = q as i64;
        if q == 7 && self.cut("B042") >= 1 {
            q = self.cut("B042");
        } else if q == 3 && self.cut("B043") >= 1 {
            q = self.cut("B043");
        }
        if self.is_marking(q as usize) {
            return 2;
        }
        if self.on("B015") {
            return usize::try_from(q).ok().and_then(|q| self.modes.get(q)).copied().unwrap_or(0);
        }
        match q {
            2 => 1,
            3..=7 => q,
            _ => 0,
        }
    }

    /// BV32, BV33: quality `q` is the B026 marking quality.
    pub fn is_marking(&self, q: usize) -> bool {
        let m = self.cut("B026");
        m >= 1 && m as usize == q
    }

    /// BV33: the pierce mode of a cut in quality `q` (`mark`: a marking
    /// line), None = no pierce commands at all (B014 with an empty table
    /// text). B014 off: `Fine` for cutting, `Normal` for marking; then the
    /// qualities B009, B008, B007 name (Normal wins over Fine over NoTime);
    /// the B026 marking quality `Normal`.
    pub fn pierce(&self, q: usize, mark: bool) -> Option<String> {
        if self.on("B014") {
            let t = self.pierces.get(if mark { 0 } else { q }).map_or("", |s| s.trim());
            return (!t.is_empty()).then(|| t.to_string());
        }
        if mark {
            return Some("Normal".into());
        }
        let mut p = "Fine";
        let is = |k: &str| self.whole(k) >= 1 && self.whole(k) as usize == q;
        if is("B009") {
            p = "NoTime";
        }
        if is("B008") {
            p = "Fine";
        }
        if is("B007") {
            p = "Normal";
        }
        if self.is_marking(q) {
            p = "Normal";
        }
        Some(p.into())
    }
}

// ---------------------------------------------------------------------------
// Numbers (BV52).
// ---------------------------------------------------------------------------

/// Below this a computed value is float noise and is 0 (BV62).
pub const NOISE: f64 = 1e-12;

fn digits(v: f64, dec: u32) -> String {
    let v = if v.abs() < NOISE { 0.0 } else { v };
    let neg = v < 0.0;
    let s = v.abs() * 10f64.powi(dec as i32);
    let f = s.floor();
    // Halves toward zero (in size).
    let r = if s - f > 0.5 { f + 1.0 } else { f } as u128;
    let p = 10u128.pow(dec);
    let (int, frac) = (r / p, r % p);
    let mut out = format!("{}{int}", if neg { "-" } else { "" });
    if dec > 0 && frac > 0 {
        let f = format!("{frac:0w$}", w = dec as usize);
        out.push('.');
        out.push_str(f.trim_end_matches('0'));
    }
    out
}

/// BV52: a coordinate, sweep, thickness or size: up to 9 decimals, trailing
/// zeros dropped, no point when whole, halves toward zero; the sign is the
/// value's before rounding (a tiny negative is `-0`).
pub fn num(v: f64) -> String {
    digits(v, 9)
}

/// The value `num` writes, as a number.
pub fn rounded(v: f64, dec: u32) -> f64 {
    digits(v, dec).parse::<f64>().unwrap_or(0.0)
}

/// BV52: a whole number (rotation, call position without B021).
pub fn whole(v: f64) -> String {
    digits(v, 0)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// The cut program (`.ccp`, BV23–BV35, BV51–BV53).
// ---------------------------------------------------------------------------

/// One command as written: its name and attributes (values as written).
#[derive(Clone, Debug, PartialEq)]
struct Cmd(&'static str, Vec<(&'static str, String)>);

fn cmd(n: &'static str) -> Cmd {
    Cmd(n, vec![])
}

fn at(n: &'static str, p: P) -> Cmd {
    Cmd(n, vec![("X", num(p[0])), ("Y", num(p[1]))])
}

fn val(n: &'static str, v: impl Into<String>) -> Cmd {
    Cmd(n, vec![("Value", v.into())])
}

/// A move along a cut in program units: a line, or an arc to its end with
/// its centre and signed sweep in degrees (counter-clockwise positive).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mv {
    Line(P),
    Arc { to: P, c: P, sweep: f64 },
}

impl Mv {
    fn end(&self) -> P {
        match *self {
            Mv::Line(p) | Mv::Arc { to: p, .. } => p,
        }
    }
}

fn d(a: P, b: P) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The moves through `pts` (from pts[0], program units of `unit` mm): runs
/// on one circle become arcs as the BLM post fits them (BL38, its
/// tolerances in mm), the rest lines.
fn moves(pts: &[P], unit: f64) -> Vec<Mv> {
    let mut v: Vec<P> = pts.iter().map(|p| [p[0] * unit, p[1] * unit]).collect();
    v.dedup_by(|b, a| d(*a, *b) < 1e-9);
    if v.len() < 2 {
        return vec![];
    }
    let mut out = vec![];
    let mut from = v[0];
    let back = |p: P| [p[0] / unit, p[1] / unit];
    for m in crate::blm::fit_moves(&v) {
        let (end, mv) = match m {
            crate::blm::Move::Line(p) => (p, Mv::Line(back(p))),
            crate::blm::Move::Arc { to, center, ccw } => {
                let a0 = (from[1] - center[1]).atan2(from[0] - center[0]);
                let a1 = (to[1] - center[1]).atan2(to[0] - center[0]);
                let tau = std::f64::consts::TAU;
                let s = if ccw { (a1 - a0).rem_euclid(tau) } else { -(a0 - a1).rem_euclid(tau) };
                (to, Mv::Arc { to: back(to), c: back(center), sweep: s.to_degrees() })
            }
        };
        out.push(mv);
        from = end;
    }
    out
}

fn turn(p: P, c: P, deg: f64) -> P {
    let (s, k) = deg.to_radians().sin_cos();
    let v = [p[0] - c[0], p[1] - c[1]];
    [c[0] + v[0] * k - v[1] * s, c[1] + v[0] * s + v[1] * k]
}

/// BV35: B018 splits an arc of 360° or more into 180° pieces (and what is
/// left); arcs under 360° stay whole.
fn split_full(from: P, m: Mv, b018: bool) -> Vec<Mv> {
    let Mv::Arc { to, c, sweep } = m else { return vec![m] };
    if !b018 || sweep.abs() < 360.0 {
        return vec![m];
    }
    let mut out = vec![];
    let (mut left, mut at) = (sweep, from);
    while left.abs() > 1e-9 {
        let piece = if left.abs() > 180.0 { 180.0f64.copysign(left) } else { left };
        let end = if (left - piece).abs() <= 1e-9 { to } else { turn(at, c, piece) };
        out.push(Mv::Arc { to: end, c, sweep: piece });
        at = end;
        left -= piece;
    }
    out
}

/// BV34: B006 splits the first move of a contour without a lead-in at that
/// distance from its start, at most half its length.
fn split_first(from: P, m: Mv, len: f64) -> Vec<Mv> {
    match m {
        Mv::Line(b) => {
            let l = d(from, b);
            let t = len.min(l / 2.0);
            if t <= 0.0 || l <= 0.0 {
                return vec![m];
            }
            let k = t / l;
            vec![Mv::Line([from[0] + (b[0] - from[0]) * k, from[1] + (b[1] - from[1]) * k]), m]
        }
        Mv::Arc { to, c, sweep } => {
            let l = d(from, c) * sweep.to_radians().abs();
            let t = len.min(l / 2.0);
            if t <= 0.0 || l <= 0.0 {
                return vec![m];
            }
            let s1 = sweep * t / l;
            let mid = turn(from, c, s1);
            vec![Mv::Arc { to: mid, c, sweep: s1 }, Mv::Arc { to, c, sweep: sweep - s1 }]
        }
    }
}

#[cfg(test)]
thread_local! {
    /// A writer fault for the read-back's tests: the first contour line
    /// written is moved this far in X (mm).
    pub static FAULT: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
    /// A writer fault: the cut program's text is changed by this before it
    /// is zipped.
    pub static EDIT: std::cell::Cell<Option<fn(String) -> String>> = const { std::cell::Cell::new(None) };
    /// A writer fault: every sheet folds into the first nest.
    pub static FOLD_ALL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn fault() -> f64 {
    #[cfg(test)]
    return FAULT.with(|f| f.get());
    #[cfg(not(test))]
    0.0
}

/// A part's place on the sheet as the program writes it (BV27, BV56): the
/// call's position and rotation and how a sheet point is put in the
/// subprogram's coordinates.
#[derive(Clone, Copy, Debug)]
struct Frame {
    /// The call's X, Y and rotation, in program units, as written.
    x: f64,
    y: f64,
    rot: f64,
}

impl Frame {
    /// BV56: the call is the part's own origin on the sheet and its turn,
    /// both rounded as BV52 writes them; the subprogram is written about
    /// the rounded call, so every cut lands exactly where it is planned.
    fn of(pl: &Placed, unit: f64, b021: bool) -> Frame {
        let o = pl.map([0.0, 0.0]);
        let dec = if b021 { 3 } else { 0 };
        let a = if pl.mirror { 0.0 } else { pl.angle() };
        let rot = rounded(a, 0).rem_euclid(360.0);
        Frame { x: rounded(o[0] / unit, dec), y: rounded(o[1] / unit, dec), rot }
    }

    /// A sheet point (mm) in the subprogram's coordinates (program units).
    fn local(&self, p: P, unit: f64) -> P {
        let v = [p[0] / unit - self.x, p[1] / unit - self.y];
        let (s, c) = exact_sin_cos(-self.rot);
        [v[0] * c - v[1] * s, v[0] * s + v[1] * c]
    }

    /// A subprogram point back on the sheet (program units).
    fn place(&self, p: P) -> P {
        let (s, c) = exact_sin_cos(self.rot);
        [self.x + p[0] * c - p[1] * s, self.y + p[0] * s + p[1] * c]
    }
}

/// sin and cos of a whole-degree angle, exact for quarter turns.
fn exact_sin_cos(deg: f64) -> (f64, f64) {
    match deg.rem_euclid(360.0) {
        a if a == 0.0 => (0.0, 1.0),
        a if a == 90.0 => (1.0, 0.0),
        a if a == 180.0 => (0.0, -1.0),
        a if a == 270.0 => (-1.0, 0.0),
        a => a.to_radians().sin_cos(),
    }
}

/// What the writer keeps between contours of one program (BV37, BV51).
struct State {
    mode: Option<i64>,
    sensor: bool,
}

/// One step of a contour's cut-out as written: a move, a micro-joint marker
/// at the gap's middle (BV36, BV46), or with B038 the end of the cut-out at a
/// gap and a new start at the gap's far end (BV48).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    Mv(Mv),
    Joint,
    Gap(P),
}

/// The moves through `pts` as `moves` gives them, with the distance along
/// `pts` (mm) at each move's end: where a joint given as a distance along the
/// drawn contour falls on the moves.
fn moves_ends(pts: &[P], unit: f64) -> (Vec<Mv>, Vec<f64>) {
    let mut v: Vec<P> = pts.iter().map(|p| [p[0] * unit, p[1] * unit]).collect();
    v.dedup_by(|b, a| d(*a, *b) < 1e-9);
    let mv = moves(pts, unit);
    let mut cum = vec![0.0; v.len()];
    for i in 1..v.len() {
        cum[i] = cum[i - 1] + d(v[i - 1], v[i]);
    }
    let mut ends = vec![];
    let mut i = 0;
    for m in &mv {
        let e = m.end();
        let e = [e[0] * unit, e[1] * unit];
        let j = (i + 1..v.len()).find(|&j| d(v[j], e) < 1e-6).unwrap_or(v.len() - 1);
        ends.push(cum[j]);
        i = j;
    }
    (mv, ends)
}

/// The point a share `t` of the way along move `m` from `from`.
fn mv_at(from: P, m: &Mv, t: f64) -> P {
    match *m {
        Mv::Line(b) => [from[0] + (b[0] - from[0]) * t, from[1] + (b[1] - from[1]) * t],
        Mv::Arc { c, sweep, .. } => turn(from, c, sweep * t),
    }
}

/// The piece of move `m` (from `from`) between shares `t0` and `t1`, as a move from the `t0` point.
fn mv_piece(from: P, m: &Mv, t0: f64, t1: f64) -> Mv {
    match *m {
        Mv::Line(_) => Mv::Line(mv_at(from, m, t1)),
        Mv::Arc { c, sweep, .. } => Mv::Arc { to: mv_at(from, m, t1), c, sweep: sweep * (t1 - t0) },
    }
}

/// BV36, BV46–BV49, BV63: a contour's moves with its micro-joints. Each
/// joint is a gap (start and end, mm along the drawn contour). B038 off: the
/// cut runs on to the gap's middle, taken along the move it lies on (an arc's
/// middle along the arc), and a joint marker is written there; the cut goes
/// on from the middle, an arc with its true sweep from there (the owner's
/// answer on #122, BV47). A held cut and the half gap on one move are one
/// move; a gap across a corner follows the corner. B049: a gap that lies on
/// one arc is cut straight (its chord) to the chord's middle. B038 on: the
/// cut-out stops at the gap's start and starts again at its end.
fn joint_steps(from: P, mv: &[Mv], ends: &[f64], joints: &[(f64, f64)], b038: bool, b049: bool) -> Vec<Step> {
    #[derive(Clone, Copy, PartialEq)]
    enum Ev {
        Mid,
        Chord(usize),
        GapStart,
        GapEnd,
    }
    let mut js: Vec<(f64, f64)> = joints.to_vec();
    js.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut ev: Vec<(f64, Ev)> = vec![];
    for (k, &(a, b)) in js.iter().enumerate() {
        if b038 {
            ev.push((a, Ev::GapStart));
            ev.push((b, Ev::GapEnd));
        } else {
            if b049 {
                ev.push((a, Ev::Chord(k)));
            }
            ev.push(((a + b) / 2.0, Ev::Mid));
        }
    }
    ev.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Where a distance along the drawn contour falls: (move, share of it).
    let locate = |s: f64| -> (usize, f64) {
        let k = ends.iter().position(|&e| e >= s - 1e-9).unwrap_or(ends.len().saturating_sub(1));
        let s0 = if k == 0 { 0.0 } else { ends[k - 1] };
        let t = if ends[k] - s0 > 0.0 { ((s - s0) / (ends[k] - s0)).clamp(0.0, 1.0) } else { 1.0 };
        (k, t)
    };
    let starts: Vec<P> = std::iter::once(from).chain(mv.iter().map(Mv::end)).collect();
    let mut out = vec![];
    let mut skip = false;
    // B049: the chord's middle the next piece ends at instead.
    let mut chord: Option<P> = None;
    let mut e = 0;
    for (k, m) in mv.iter().enumerate() {
        let mut t0 = 0.0;
        while e < ev.len() && locate(ev[e].0).0 == k {
            let t = locate(ev[e].0).1;
            let piece = mv_piece(starts[k], m, t0, t);
            let long = d(mv_at(starts[k], m, t0), piece.end()) > 1e-9;
            match (ev[e].1, chord.take()) {
                (Ev::Mid, Some(cm)) => {
                    out.push(Step::Mv(Mv::Line(cm)));
                    out.push(Step::Joint);
                }
                (ev_k, _) => {
                    let split = match ev_k {
                        // B049 only on an arc whose gap lies on it: else as without B049.
                        Ev::Chord(j) => {
                            let (kb, tb) = locate(js[j].1);
                            matches!(m, Mv::Arc { .. }) && kb == k && {
                                chord = Some({
                                    let (pa, pb) = (piece.end(), mv_at(starts[k], m, tb));
                                    [(pa[0] + pb[0]) / 2.0, (pa[1] + pb[1]) / 2.0]
                                });
                                true
                            }
                        }
                        _ => true,
                    };
                    if !split {
                        e += 1;
                        continue;
                    }
                    if !skip && long {
                        out.push(Step::Mv(piece));
                    }
                    match ev_k {
                        Ev::Mid => out.push(Step::Joint),
                        Ev::GapStart => skip = true,
                        Ev::GapEnd => {
                            skip = false;
                            out.push(Step::Gap(piece.end()));
                        }
                        Ev::Chord(_) => {}
                    }
                }
            }
            t0 = t;
            e += 1;
        }
        if !skip && t0 < 1.0 {
            let piece = mv_piece(starts[k], m, t0, 1.0);
            if d(mv_at(starts[k], m, t0), piece.end()) > 1e-9 {
                out.push(Step::Mv(piece));
            }
        }
    }
    out
}

/// What a contour start writes (BV29–BV33), at the contour's first start and,
/// with B038, after each joint gap (BV48).
struct Start<'a> {
    post: &'a Post,
    f: &'a Frame,
    w: f64,
    h: f64,
    /// Engraving: a mark or the B026 quality (BV30, BV31, BV37, B036).
    marking: bool,
    mode: i64,
    comp: Option<&'static str>,
    side: &'static str,
    pierce: Option<String>,
}

/// The height sensor at a pierce (or cut start) at `at` (subprogram units):
/// on inside the sheet, off on or past its edge (BV37).
fn sensor_rule(out: &mut Vec<Cmd>, st: &mut State, s: &Start, at: P, marking: bool) {
    let u = s.post.unit;
    let p = s.f.place(at);
    let pm = [p[0] * u, p[1] * u];
    let tol = s.post.tol;
    let inside = pm[0] > tol && pm[1] > tol && pm[0] < s.w - tol && pm[1] < s.h - tol;
    match s.post.sensor_mode() {
        2 => {}
        m => {
            if inside && !st.sensor && !(m == 1 && marking) {
                out.push(val("SetSensingSensor", "On"));
                st.sensor = true;
            } else if !inside && st.sensor {
                out.push(val("SetSensingSensor", "Off"));
                st.sensor = false;
            }
        }
    }
}

/// Moves written as `LineTo` / `ArcTo` (B018 splitting full turns, BV35).
fn write_moves(out: &mut Vec<Cmd>, mv: &[Mv], from: &mut P, post: &Post, fault_once: &mut bool) {
    for m in mv {
        for piece in split_full(*from, *m, post.on("B018")) {
            match piece {
                Mv::Line(p) => {
                    let mut p = p;
                    if *fault_once && fault() != 0.0 {
                        p[0] += fault() / post.unit;
                        *fault_once = false;
                    }
                    out.push(at("LineTo", p));
                }
                Mv::Arc { to, sweep, .. } => out.push(Cmd("ArcTo", vec![("X", num(to[0])), ("Y", num(to[1])), ("SweepAngle", num(sweep))])),
            }
            *from = piece.end();
        }
    }
}

/// BV29–BV34: a contour's start up to its `BeginCutOut`: the mode (BV51),
/// the sensor, compensation, the rapid, the lead-in with its pierce or, without
/// one, the pierce B044 or B036 asks for. Returns whether it opened a lead-in
/// bracket (B006 splits only a start without one).
fn begin(out: &mut Vec<Cmd>, st: &mut State, s: &Start, start: P, lead: &[Mv]) -> bool {
    let post = s.post;
    if st.mode != Some(s.mode) {
        out.push(val("SetOperationMode", s.mode.to_string()));
        st.mode = Some(s.mode);
    }
    sensor_rule(out, st, s, start, s.marking);
    let bracket = !lead.is_empty() || post.on("B024");
    let comp_at_cut = post.on("B040") && bracket;
    if let (Some(c), false, false) = (s.comp, post.on("B011"), comp_at_cut) {
        out.push(val("SetToolCorrection", c));
    }
    out.push(at("MoveLineTo", start));
    if let (Some(c), true, false) = (s.comp, post.on("B011"), comp_at_cut) {
        out.push(val("SetToolCorrection", c));
    }
    let pierce = |out: &mut Vec<Cmd>| {
        if let Some(p) = &s.pierce {
            out.push(val("SetPiercingMode", p.clone()));
            out.push(cmd("ExecPiercing"));
        }
    };
    if bracket {
        out.push(cmd("BeginLeadIn"));
        pierce(out);
        let mut from = start;
        let mut no_fault = false;
        write_moves(out, lead, &mut from, post, &mut no_fault);
        out.push(cmd("EndLeadIn"));
    } else if post.on("B044") || (s.marking && post.whole("B036") != 0) {
        pierce(out);
    }
    if let (Some(c), true) = (s.comp, comp_at_cut) {
        out.push(val("SetToolCorrection", c));
    }
    out.push(Cmd("BeginCutOut", vec![("Contour", s.side.into())]));
    bracket
}

/// BV34: B006 splits the first move of a start without a lead-in.
fn b006(from: P, mv: &mut Vec<Mv>, post: &Post) {
    let b = post.whole("B006") as f64;
    if b > 0.0 && !mv.is_empty() {
        let first = mv.remove(0);
        let mut s = split_first(from, first, b);
        s.extend(mv.drain(..));
        *mv = s;
    }
}

/// BV40, BV55, BV65: a point mark at `p` (subprogram units): a circle of
/// B013's diameter, or with B050 an on/off point as a tiny cut. Empty when
/// the switches make nothing of it (refused before writing, BV60).
fn point_mark(out: &mut Vec<Cmd>, st: &mut State, s: &Start, p: P) {
    let post = s.post;
    let dia = post.num("B013");
    if dia > post.tol {
        let r = dia / 2.0;
        let mode = if post.on("B033") { 2 } else { 3 };
        out.push(val("SetOperationMode", mode.to_string()));
        st.mode = Some(mode);
        sensor_rule(out, st, s, p, mode == 2);
        out.push(at("MoveLineTo", p));
        let comp = (!post.on("B031")).then_some("Right");
        if let Some(c) = comp {
            out.push(val("SetToolCorrection", c));
        }
        out.push(cmd("BeginLeadIn"));
        if let Some(pm) = post.pierce(0, true) {
            out.push(val("SetPiercingMode", pm));
            out.push(cmd("ExecPiercing"));
        }
        out.push(at("LineTo", [p[0] + r, p[1]]));
        out.push(cmd("EndLeadIn"));
        out.push(Cmd("BeginCutOut", vec![("Contour", "Unknown".into())]));
        out.push(Cmd("ArcTo", vec![("X", num(p[0] - r)), ("Y", num(p[1])), ("SweepAngle", "180".into())]));
        out.push(Cmd("ArcTo", vec![("X", num(p[0] + r)), ("Y", num(p[1])), ("SweepAngle", "180".into())]));
        out.push(cmd("EndCutOut"));
        if comp.is_some() {
            out.push(val("SetToolCorrection", "None"));
        }
    } else if post.on("B050") && post.point_type == 2 {
        out.push(val("SetOperationMode", "0"));
        st.mode = Some(0);
        sensor_rule(out, st, s, p, false);
        out.push(at("MoveLineTo", p));
        out.push(Cmd("BeginCutOut", vec![("Contour", "Interior".into())]));
        out.push(val("SetPiercingMode", "Fine"));
        out.push(cmd("ExecPiercing"));
        out.push(at("LineTo", [p[0] - 0.05, p[1]]));
        out.push(cmd("EndCutOut"));
    }
}

/// What one placed part's subprogram cuts, in writing order: its marks and
/// contours in its cut order (CO9).
#[allow(clippy::too_many_arguments)]
fn part_commands(plan: &SheetPlan, part_no: usize, f: &Frame, post: &Post, w: f64, h: f64, st: &mut State, first_fault: &mut bool) -> Vec<Cmd> {
    let u = post.unit;
    let loc = |p: P| f.local(p, u);
    let mut out = vec![];
    // BV51: the mode is forgotten at each new subprogram (each new part).
    st.mode = None;
    let b036 = post.whole("B036");
    for made in plan.part_order(part_no) {
        let (q, mark, inner, lead, body, closed, out_path, comp_left, joints) = match made {
            Made::Mark(j) => (0usize, true, false, vec![], plan.marks[j].1.clone(), false, vec![], false, vec![]),
            Made::Contour(k) => {
                let c = &plan.contours[k];
                if c.punch.is_some() {
                    continue;
                }
                let mut tail = vec![];
                if c.overlap > 0.0 {
                    tail.push(crate::cnc::point_at(&c.points, c.overlap));
                }
                tail.extend(c.lead_out_path.iter().skip(1).copied());
                let left = c.comp.word().is_some_and(|w| w.0 == crate::kerf::Side::Left);
                (c.quality, false, c.inner, if c.lead_path.len() >= 2 { c.lead_path.clone() } else { vec![] }, c.points.clone(), true, tail, left, c.joints.clone())
            }
        };
        let marking = mark || post.is_marking(q);
        // BV40, BV55: a mark that is one point is a point mark.
        if mark && body.len() == 1 {
            let s = Start { post, f, w, h, marking: true, mode: 0, comp: None, side: "Unknown", pierce: None };
            point_mark(&mut out, st, &s, loc(body[0]));
            continue;
        }
        let mut body: Vec<P> = body.into_iter().map(loc).collect();
        if closed && body.len() > 1 {
            body.push(body[0]);
        }
        body.dedup_by(|b, a| d(*a, *b) < 1e-9);
        if body.len() < 2 {
            continue;
        }
        let lead: Vec<P> = lead.into_iter().map(loc).collect();
        let has_lead = lead.len() >= 2 && !mark;
        let start = if has_lead { lead[0] } else { body[0] };
        let new_marking = marking && b036 != 0;
        // BV31: compensation Left for a left side, Right for any other; marks
        // too unless B031; none on engraving with B036 (BV65).
        let comp = if new_marking || (marking && post.on("B031")) { None } else { Some(if comp_left && !mark { "Left" } else { "Right" }) };
        // BV30: Exterior for a part's outline, Interior for a hole, Unknown
        // for marking (and the B026 quality); B036 engraving Interior.
        let side = if new_marking { "Interior" } else if marking { "Unknown" } else if inner { "Interior" } else { "Exterior" };
        // BV33: the pierce mode; B036 engraving Fine (2: Normal).
        let pierce = if new_marking { Some(if b036 == 2 { "Normal" } else { "Fine" }.to_string()) } else { post.pierce(q, marking) };
        let s = Start { post, f, w, h, marking, mode: if mark { 2 } else { post.mode(q) }, comp, side, pierce };
        let lead_mv = if has_lead { moves(&lead, u) } else { vec![] };
        let bracket = begin(&mut out, st, &s, start, &lead_mv);
        let mut from = body[0];
        let (mv, ends) = moves_ends(&body, u);
        let mut steps: Vec<Step> = if joints.is_empty() { mv.into_iter().map(Step::Mv).collect() } else { joint_steps(from, &mv, &ends, &joints, post.on("B038"), post.on("B049")) };
        // BV34: B006 splits the first move of a start without a lead-in.
        let split_first_of = |steps: &mut Vec<Step>, at: usize, from: P| {
            if let Some(Step::Mv(m)) = steps.get(at).copied() {
                let mut v = vec![m];
                b006(from, &mut v, post);
                steps.splice(at..=at, v.into_iter().map(Step::Mv));
            }
        };
        if !bracket {
            split_first_of(&mut steps, 0, from);
        }
        let mut i = 0;
        while i < steps.len() {
            match steps[i] {
                Step::Mv(m) => write_moves(&mut out, &[m], &mut from, post, first_fault),
                Step::Joint => out.push(cmd("ExecMicrojoint")),
                // BV48 (B038): the cut-out ends at the gap; the cut after it
                // starts like a new contour, without a lead-in.
                Step::Gap(p) => {
                    out.push(cmd("EndCutOut"));
                    if comp.is_some() {
                        out.push(val("SetToolCorrection", "None"));
                    }
                    let again = Start { pierce: s.pierce.clone(), ..s };
                    let bracket = begin(&mut out, st, &again, p, &[]);
                    from = p;
                    if !bracket {
                        split_first_of(&mut steps, i + 1, p);
                    }
                }
            }
            i += 1;
        }
        // BV34, BV57: the overlap and lead-out in their bracket, before EndCutOut.
        let tail: Vec<P> = out_path.into_iter().map(loc).collect();
        if !tail.is_empty() {
            out.push(cmd("BeginLeadOut"));
            let mut t = vec![from];
            t.extend(tail);
            let mut no_fault = false;
            write_moves(&mut out, &moves(&t, u), &mut from, post, &mut no_fault);
            out.push(cmd("EndLeadOut"));
        }
        out.push(cmd("EndCutOut"));
        if comp.is_some() {
            out.push(val("SetToolCorrection", "None"));
        }
    }
    out
}

/// BV64: the quality a cut-off line is cut in: B045's when set, else 1 (the
/// machine's plain cutting column, mode 0; MAC-81).
pub fn cutoff_quality(post: &Post) -> usize {
    if post.num("B045") > 0.5 { post.num("B045").trunc() as usize } else { 1 }
}

/// One piece of a cut-off line on the sheet (mm): where it is pierced, how,
/// and its moves, each with the height sensor switched after it (BV38).
#[derive(Clone, Debug, PartialEq)]
pub struct CutOffPiece {
    pub pierce_at: P,
    /// None = no pierce command (a line starting inside the sheet without B025).
    pub pierce: Option<String>,
    pub runs: Vec<(Vec<P>, Option<bool>)>,
}

/// The sheet edge nearest `p` (mm), as (signed distance inside, outward normal).
fn nearest_edge(p: P, w: f64, h: f64) -> (f64, P) {
    [(p[0], [-1.0, 0.0]), (w - p[0], [1.0, 0.0]), (p[1], [0.0, -1.0]), (h - p[1], [0.0, 1.0])].into_iter().min_by(|a, b| a.0.total_cmp(&b.0)).unwrap()
}

fn path_len(p: &[P]) -> f64 {
    p.windows(2).map(|w| d(w[0], w[1])).sum()
}

/// The path from distance `s0` to `s1` along `p` (mm).
fn sub_path(p: &[P], s0: f64, s1: f64) -> Vec<P> {
    let at = |s: f64| {
        let mut acc = 0.0;
        for w in p.windows(2) {
            let l = d(w[0], w[1]);
            if acc + l >= s && l > 0.0 {
                let t = ((s - acc) / l).clamp(0.0, 1.0);
                return [w[0][0] + (w[1][0] - w[0][0]) * t, w[0][1] + (w[1][1] - w[0][1]) * t];
            }
            acc += l;
        }
        *p.last().unwrap()
    };
    let mut out = vec![at(s0)];
    let mut acc = 0.0;
    for w in p.windows(2) {
        acc += d(w[0], w[1]);
        if acc > s0 + 1e-9 && acc < s1 - 1e-9 {
            out.push(w[1]);
        }
    }
    out.push(at(s1));
    out.dedup_by(|b, a| d(*a, *b) < 1e-9);
    out
}

/// Where a path first comes `inset` mm inside the sheet's outline (mm along it).
fn entry(p: &[P], w: f64, h: f64, inset: f64) -> Option<f64> {
    let (lo, hi) = ([inset, inset], [w - inset, h - inset]);
    let mut acc = 0.0;
    for s in p.windows(2) {
        let (a, b) = (s[0], s[1]);
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        let mut ok = true;
        for k in 0..2 {
            let dk = b[k] - a[k];
            if dk.abs() < 1e-12 {
                if a[k] < lo[k] - 1e-9 || a[k] > hi[k] + 1e-9 {
                    ok = false;
                }
            } else {
                let (ta, tb) = ((lo[k] - a[k]) / dk, (hi[k] - a[k]) / dk);
                t0 = t0.max(ta.min(tb));
                t1 = t1.min(ta.max(tb));
            }
        }
        let l = d(a, b);
        if ok && t0 <= t1 + 1e-12 {
            return Some(acc + t0 * l);
        }
        acc += l;
    }
    None
}

/// BV38, BV49, BV64: a cut-off line (a remnant line or a free cut path) as it
/// is cut on the sheet (mm), with B003 (overrun), B004 (sensing off) and
/// B005 (run-up) in mm. A line starting on or past an edge (or within B004 +
/// B005 of one) is pierced where it first comes B004 + B005 inside the
/// sheet, cut back to its start and B003 past it with sensing off for the
/// last B005 back, the head lifted, then pierced again (no-time) and cut on
/// to its end, sensing off B004 before the far edge and running B003 past
/// it. A line starting further inside is cut from its start, pierced only
/// with B025. A far end inside the sheet just ends there.
pub fn cutoff_pieces(pts: &[P], w: f64, h: f64, b003: f64, b004: f64, b005: f64, post: &Post) -> Vec<CutOffPiece> {
    let mut p: Vec<P> = pts.to_vec();
    p.dedup_by(|b, a| d(*a, *b) < 1e-9);
    if p.len() < 2 {
        return vec![];
    }
    let tol = post.tol;
    let len = path_len(&p);
    let first_pierce = post.pierce(cutoff_quality(post), false);
    let ext = |a: P, b: P, by: f64| {
        let l = d(a, b);
        [b[0] + (b[0] - a[0]) / l * by, b[1] + (b[1] - a[1]) / l * by]
    };
    let (start_in, _) = nearest_edge(p[0], w, h);
    let (end_in, _) = nearest_edge(p[p.len() - 1], w, h);
    let from_edge = start_in <= b004 + b005 + tol;
    let to_edge = end_in <= tol;
    // Where it is pierced (mm along the line).
    let sp = if from_edge {
        match entry(&p, w, h, b004 + b005) {
            Some(s) if s <= len / 2.0 => s,
            _ => len / 4.0,
        }
    } else {
        0.0
    };
    let pierce_at = sub_path(&p, sp, sp)[0];
    let mut out = vec![];
    // The far piece: from the pierce on to the end and past a far edge.
    let mut far: Vec<(Vec<P>, Option<bool>)> = vec![];
    if to_edge {
        let off = if len - b004 >= sp { len - b004 } else { sp + (len - sp) / 2.0 };
        let run_up = (sp + b005).min(off);
        far.push((sub_path(&p, sp, run_up), None));
        far.push((sub_path(&p, run_up, off), Some(false)));
        let n = p.len();
        let mut tail = sub_path(&p, off, len);
        if b003 > 0.0 {
            // Out past the edge in one move with the line's last stretch.
            let e = ext(p[n - 2], p[n - 1], b003);
            if tail.len() >= 2 { *tail.last_mut().unwrap() = e } else { tail.push(e) }
        }
        far.push((tail, Some(true)));
    } else {
        far.push((sub_path(&p, sp, len), None));
    }
    // BV38: a piece back to the start edge and past it, also when the pierce
    // is on the edge itself but B003 runs it past (S4).
    if from_edge && (sp > 1e-9 || b003 > 0.0) {
        // Back to the start edge and past it.
        let back: Vec<P> = sub_path(&p, 0.0, sp).into_iter().rev().collect();
        let off = b005.min(sp);
        let mut tail = sub_path(&back, off, sp);
        if b003 > 0.0 {
            let e = ext(p[1], p[0], b003);
            if tail.len() >= 2 { *tail.last_mut().unwrap() = e } else { tail.push(e) }
        }
        out.push(CutOffPiece { pierce_at, pierce: first_pierce, runs: vec![(sub_path(&back, 0.0, off), Some(false)), (tail, Some(true))] });
        out.push(CutOffPiece { pierce_at, pierce: Some("NoTime".into()), runs: far });
    } else {
        let pierce = if from_edge || post.on("B025") { first_pierce } else { None };
        out.push(CutOffPiece { pierce_at, pierce, runs: far });
    }
    out
}

/// BV64: the sheet's cut-off lines (free cut paths, then the remnant line, as
/// cut after the parts) as one subprogram's commands, in its own coordinates
/// (`f`), each line in the BV38 sequence.
fn cutoff_commands(lines: &[&CutLine], f: &Frame, post: &Post, w: f64, h: f64, st: &mut State) -> Vec<Cmd> {
    let u = post.unit;
    let mm = |k: &str| post.whole(k).max(0) as f64 * u;
    let mut out = vec![];
    st.mode = None;
    let q = cutoff_quality(post);
    let s = Start { post, f, w, h, marking: false, mode: post.mode(q), comp: Some("Right"), side: "Exterior", pierce: None };
    for line in lines {
        let pieces = cutoff_pieces(&line.pts, w, h, mm("B003"), mm("B004"), mm("B005"), post);
        let n = pieces.len();
        for (k, pc) in pieces.iter().enumerate() {
            let at_l = f.local(pc.pierce_at, u);
            if k == 0 {
                // BV38, BV51: the mode always, compensation off, then the
                // contour start's sensor and compensation (BV29, BV31).
                out.push(val("SetOperationMode", s.mode.to_string()));
                st.mode = Some(s.mode);
                out.push(val("SetToolCorrection", "None"));
                sensor_rule(&mut out, st, &s, at_l, false);
                if !post.on("B011") {
                    out.push(val("SetToolCorrection", "Right"));
                }
                out.push(at("MoveLineTo", at_l));
                if post.on("B011") {
                    out.push(val("SetToolCorrection", "Right"));
                }
            } else {
                out.push(at("MoveLineTo", at_l));
            }
            out.push(Cmd("BeginCutOut", vec![("Contour", "Exterior".into())]));
            if let Some(pm) = &pc.pierce {
                out.push(val("SetPiercingMode", pm.clone()));
                out.push(cmd("ExecPiercing"));
            }
            let mut from = at_l;
            for (run, sensor) in &pc.runs {
                let pts: Vec<P> = run.iter().map(|&p| f.local(p, u)).collect();
                if pts.len() >= 2 && d(pts[0], *pts.last().unwrap()) + path_len(&pts) > 1e-9 {
                    let mut no_fault = false;
                    write_moves(&mut out, &moves(&pts, u), &mut from, post, &mut no_fault);
                }
                if let Some(on) = sensor {
                    out.push(val("SetSensingSensor", if *on { "On" } else { "Off" }));
                    st.sensor = *on;
                }
            }
            out.push(cmd("EndCutOut"));
            out.push(cmd("ExecHighPositioning"));
            if k + 1 == n {
                out.push(val("SetToolCorrection", "None"));
            }
        }
    }
    out
}

/// The cut lines a sheet's program cuts, in order (free paths, then the
/// remnant line), those with some length only.
pub fn cut_lines_in_order(cuts: &[CutLine]) -> Vec<&CutLine> {
    let by = |kind| cuts.iter().filter(move |c: &&CutLine| c.kind == kind && c.pts.iter().any(|q| d(*q, c.pts[0]) > 1e-6));
    by(crate::cutoff::CutKind::Free).chain(by(crate::cutoff::CutKind::Remnant)).collect()
}

/// BV27, BV64: the cut-off subprogram's call: moved B003 outward from the
/// edge the first line starts on (on the sheet origin when it starts inside).
fn cutoff_frame(lines: &[&CutLine], post: &Post, w: f64, h: f64) -> Frame {
    let u = post.unit;
    let dec = if post.on("B021") { 3 } else { 0 };
    let b003 = post.whole("B003").max(0) as f64;
    let mut v = [0.0, 0.0];
    if let Some(l) = lines.first() {
        let (inside, n) = nearest_edge(l.pts[0], w, h);
        let b = (post.whole("B004") + post.whole("B005")).max(0) as f64 * u;
        if inside <= b + post.tol {
            v = [n[0] * b003, n[1] * b003];
        }
    }
    Frame { x: rounded(v[0], dec), y: rounded(v[1], dec), rot: 0.0 }
}

/// One subprogram written: its name, the part it cuts and how many times the
/// nest calls it.
#[derive(Clone, Debug, PartialEq)]
pub struct SubOut {
    pub name: String,
    /// The part it cuts (`CUTOFF` for the sheet's cut-off lines).
    pub part: usize,
    pub calls: usize,
}

/// `SubOut::part` of the subprogram that cuts a sheet's cut-off lines (BV64).
pub const CUTOFF: usize = usize::MAX;

/// One nest's cut program.
#[derive(Clone, Debug)]
pub struct Ccp {
    pub text: String,
    pub subs: Vec<SubOut>,
    /// BV23: PlanWidth / PlanHeight, mm.
    pub extent: [f64; 2],
}

/// One sheet to write: its parts, plan and size.
pub struct SheetIn<'a> {
    pub parts: &'a [Part],
    pub sheet: &'a [Placed],
    pub spec: &'a SheetSpec,
    pub plan: &'a SheetPlan,
    pub cuts: &'a [CutLine],
}

/// The largest |x| and |y| of a move's end and, on an arc, its extreme
/// points (BV23), in program units.
fn reach(from: P, m: &Mv, ext: &mut [f64; 2]) {
    let mut pts = vec![m.end()];
    if let Mv::Arc { c, sweep, .. } = *m {
        let r = d(from, c);
        let a0 = (from[1] - c[1]).atan2(from[0] - c[0]).to_degrees();
        for k in 0..8 {
            let a = k as f64 * 90.0 - 360.0;
            for a in [a, a + 360.0, a + 720.0] {
                let t = if sweep > 0.0 { a - a0 } else { a0 - a };
                if t > 0.0 && t < sweep.abs() {
                    pts.push([c[0] + r * a.to_radians().cos(), c[1] + r * a.to_radians().sin()]);
                }
            }
        }
    }
    for p in pts {
        ext[0] = ext[0].max(p[0].abs());
        ext[1] = ext[1].max(p[1].abs());
    }
}

/// BV56: the subprograms and calls of a sheet, in writing order.
fn subprograms(s: &SheetIn, post: &Post) -> (Vec<(Vec<Cmd>, usize, usize)>, Vec<(usize, Frame)>, [f64; 2]) {
    let u = post.unit;
    let mut st = State { mode: None, sensor: false };
    let mut order: Vec<usize> = (0..s.sheet.len()).collect();
    // Parts in the order they are finished (the outside last, MM2), as the
    // BLM post calls them.
    order.sort_by_key(|&i| s.plan.contours.iter().rposition(|c| c.part_no == i + 1).unwrap_or(usize::MAX));
    let mut variants: Vec<(Vec<Cmd>, usize, usize)> = vec![];
    let mut calls = vec![];
    let mut ext = [0.0f64; 2];
    let mut first_fault = true;
    // The parts, then the sheet's cut-off lines in one subprogram (BV64).
    let mut items: Vec<(usize, Frame)> = order.iter().map(|&i| (s.sheet[i].part, Frame::of(&s.sheet[i], u, post.on("B021")))).collect();
    let lines = cut_lines_in_order(s.cuts);
    if !lines.is_empty() {
        items.push((CUTOFF, cutoff_frame(&lines, post, s.spec.w, s.spec.h)));
    }
    for (n, &(part, f)) in items.iter().enumerate() {
        // A copy whose commands come out the same shares the subprogram;
        // working it out mustn't change what the program has written so far.
        let mut probe = State { mode: st.mode, sensor: st.sensor };
        let mut ff = first_fault;
        let cmds = if part == CUTOFF { cutoff_commands(&lines, &f, post, s.spec.w, s.spec.h, &mut probe) } else { part_commands(s.plan, order[n] + 1, &f, post, s.spec.w, s.spec.h, &mut probe, &mut ff) };
        // BV37: the sensor is as the call leaves it, whether its subprogram
        // is new or one written before with the same commands.
        st = probe;
        let k = match variants.iter().position(|v| v.1 == part && v.0 == cmds) {
            Some(k) => k,
            None => {
                first_fault = ff;
                variants.push((cmds, part, 0));
                variants.len() - 1
            }
        };
        variants[k].2 += 1;
        // BV23: every cut end (and arc extreme) on the sheet.
        let mut at_pt: Option<P> = None;
        for c in &variants[k].0 {
            let xy = || -> Option<P> { Some([c.1.iter().find(|a| a.0 == "X")?.1.parse().ok()?, c.1.iter().find(|a| a.0 == "Y")?.1.parse().ok()?]) };
            match c.0 {
                "MoveLineTo" => at_pt = xy().map(|p| f.place(p)),
                "LineTo" => {
                    if let Some(p) = xy() {
                        let q = f.place(p);
                        reach(at_pt.unwrap_or(q), &Mv::Line(q), &mut ext);
                        at_pt = Some(q);
                    }
                }
                "ArcTo" => {
                    if let (Some(p), Some(sw)) = (xy(), c.1.iter().find(|a| a.0 == "SweepAngle").and_then(|a| a.1.parse::<f64>().ok())) {
                        let q = f.place(p);
                        let from = at_pt.unwrap_or(q);
                        if let Some(cn) = crate::bvc::arc_centre(from, q, sw) {
                            reach(from, &Mv::Arc { to: q, c: cn, sweep: sw }, &mut ext);
                        } else {
                            reach(from, &Mv::Line(q), &mut ext);
                        }
                        at_pt = Some(q);
                    }
                }
                _ => {}
            }
        }
        calls.push((k, f));
    }
    (variants, calls, [ext[0] * u, ext[1] * u])
}

/// BV26: the k-th subprogram's name (from 1): the C004 prefix and the count.
pub fn sub_name(post: &Post, k: usize) -> String {
    format!("{}{k}", post.raw("C004"))
}

/// BV23–BV35, BV51–BV53: the cut program of one sheet. Ids count from 1 in
/// writing order (BV24); two spaces per level, CRLF, no BOM (BV53).
pub fn cut_program(s: &SheetIn, post: &Post) -> Ccp {
    let u = post.unit;
    let (variants, calls, extent) = subprograms(s, post);
    let mut id = 0usize;
    let mut next = || {
        id += 1;
        id
    };
    let mut t = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n");
    let unit = if post.on("B001") { "Imperial" } else { "Metric" };
    let tool = if post.on("B041") { " ToolArrangement=\"Vertical\"" } else { "" };
    let _ = write!(t, "<CuttingControlProgram MeasurementSystem=\"{unit}\" Thickness=\"{}\"{tool}>\r\n", num(s.spec.thickness / u));
    t.push_str("  <Technology />\r\n");
    let _ = write!(t, "  <ApplicationFlat PlanWidth=\"{}\" PlanHeight=\"{}\" SheetWidth=\"{}\" SheetHeight=\"{}\" />\r\n", num(extent[0] / u), num(extent[1] / u), num(s.spec.w / u), num(s.spec.h / u));
    t.push_str("  <SubPrograms>\r\n");
    let line = |t: &mut String, ind: usize, c: &Cmd, id: usize| {
        let _ = write!(t, "{}<{} Id=\"{id}\"", "  ".repeat(ind), c.0);
        for (k, v) in &c.1 {
            let _ = write!(t, " {k}=\"{}\"", esc(v));
        }
        t.push_str(" />\r\n");
    };
    let mut subs = vec![];
    for (k, (cmds, part, n)) in variants.iter().enumerate() {
        if *part == CUTOFF {
            // BV26, BV64: named ResidualSheetCuts (numbered like the parts'
            // with B016 or B028), with the extent of its cuts.
            let name = if post.on("B016") || post.on("B028") { sub_name(post, k + 1) } else { "ResidualSheetCuts".to_string() };
            let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
            for c in cmds.iter().filter(|c| matches!(c.0, "MoveLineTo" | "LineTo" | "ArcTo")) {
                let v = |k: &str| c.1.iter().find(|a| a.0 == k).and_then(|a| a.1.parse::<f64>().ok()).unwrap_or(0.0);
                let p = [v("X"), v("Y")];
                lo = [lo[0].min(p[0]), lo[1].min(p[1])];
                hi = [hi[0].max(p[0]), hi[1].max(p[1])];
            }
            let _ = write!(t, "    <SubProgram Id=\"{}\" Name=\"{}\" PartWidth=\"{}\" PartHeight=\"{}\">\r\n", next(), esc(&name), num(hi[0] - lo[0]), num(hi[1] - lo[1]));
            t.push_str("      <ControlCommands>\r\n");
            for c in cmds {
                line(&mut t, 4, c, next());
            }
            t.push_str("      </ControlCommands>\r\n    </SubProgram>\r\n");
            subs.push(SubOut { name, part: CUTOFF, calls: *n });
            continue;
        }
        let name = sub_name(post, k + 1);
        let p = &s.parts[*part];
        let (pw, ph) = p.bbox();
        // BV26: PartNumber = the part's place in the part list (B016: the name).
        let number = if post.on("B016") { name.clone() } else { (part + 1).to_string() };
        let _ = write!(t, "    <SubProgram Id=\"{}\" Name=\"{}\" PartNumber=\"{}\" PartWidth=\"{}\" PartHeight=\"{}\">\r\n", next(), esc(&name), esc(&number), num(pw / u), num(ph / u));
        t.push_str("      <ControlCommands>\r\n");
        for c in cmds {
            line(&mut t, 4, c, next());
        }
        t.push_str("      </ControlCommands>\r\n    </SubProgram>\r\n");
        subs.push(SubOut { name, part: *part, calls: *n });
    }
    t.push_str("  </SubPrograms>\r\n  <ControlCommands>\r\n");
    line(&mut t, 2, &val("SetClamps", "Off"), next());
    let b021 = post.on("B021");
    for (k, f) in &calls {
        let pos = |v: f64| if b021 { digits(v, 3) } else { whole(v) };
        let c = Cmd("CallSubProgram", vec![("SubProgramName", subs[*k].name.clone()), ("X", pos(f.x)), ("Y", pos(f.y)), ("Rotation", whole(f.rot))]);
        line(&mut t, 2, &c, next());
    }
    t.push_str("  </ControlCommands>\r\n</CuttingControlProgram>\r\n");
    #[cfg(test)]
    if let Some(f) = EDIT.with(|e| e.get()) {
        t = f(t);
    }
    Ccp { text: t, subs, extent }
}

/// BV60: what this writer can't write yet on a sheet, in shop terms. A sheet
/// with any of it is refused, never written another way.
pub fn unsupported(s: &SheetIn, post: &Post) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut add = |m: String| {
        if !out.contains(&m) {
            out.push(m);
        }
    };
    let name = |no: usize| s.sheet.get(no.wrapping_sub(1)).and_then(|pl| s.parts.get(pl.part)).map_or("?", |p| p.name.as_str()).to_string();
    for c in &s.plan.contours {
        let n = name(c.part_no);
        // BV63: joints with their own leads aren't written yet (B019, B023).
        if c.joint_leads.iter().any(|(o, i)| o.len() >= 2 || i.len() >= 2) || c.closing {
            add(format!("part {n} has micro-joints with their own leads, which the ByStar Fiber laser's programs don't carry yet; use a joint type without leads (gap) for this machine"));
        }
        if !c.joints.is_empty() && post.changed("B047") {
            add(format!("switch B047 is changed, and part {n} has micro-joints, which this writer doesn't write that way yet; set B047 back to 0"));
        }
        if !c.loops.is_empty() {
            add(format!("part {n} has corner loops, which the ByStar Fiber laser's programs don't carry yet; take them off the part first"));
        }
        if c.punch.is_some() {
            add(format!("part {n} has punched holes, which a ByStar Fiber laser can't make"));
        }
        if c.punch.is_none() && (c.comp.system || c.comp.word().is_none()) {
            add(format!("part {n} is planned without the machine's own kerf compensation, but the ByStar Fiber laser compensates every cut itself (it is told a side for each); set the sheet's kerf compensation to the machine's (control) for this machine"));
        }
        if c.overlap < 0.0 {
            add(format!("part {n} has cuts that stop short of their start, which the ByStar Fiber laser's programs don't carry yet"));
        }
    }
    // BV64: the cut-off modes this writer doesn't follow yet.
    if !cut_lines_in_order(s.cuts).is_empty() {
        for k in ["B035", "B046", "B048"] {
            if post.on(k) {
                add(format!("switch {k} is on, and the sheet has remnant or cut-off lines, which this writer doesn't cut that way yet; set {k} back to 0"));
            }
        }
    }
    // BV65: point marks the switches would write as nothing, or as a cross.
    for (no, m) in &s.plan.marks {
        if m.len() == 1 {
            let n = name(*no);
            if post.num("B034") > post.tol {
                add(format!("switch B034 is set, and part {n} has point marks, which this writer doesn't write as a cross yet; set B034 back to 0"));
            } else if post.num("B013") <= post.tol && !(post.on("B050") && post.point_type == 2) {
                add(format!("part {n} has point marks, which these switches write as nothing (B013 at or below the machine tolerance, without B050 for an on/off point); set B013 to the circle's diameter"));
            }
        }
    }
    for c in &s.plan.contours {
        if c.punch.is_none() && post.on("B014") && post.pierces.get(c.quality).is_some_and(|p| !p.trim().is_empty() && !PIERCES.contains(&p.trim())) {
            add(format!("the technology table's pierce for quality {} is \"{}\", which isn't one of the machine's pierce modes ({})", c.quality, post.pierces[c.quality].trim(), PIERCES.join(", ")));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The job file (`JobInfo.xml`, BV43, BV44) and the `.bvc` (BV21, BV22).
// ---------------------------------------------------------------------------

/// The job's own fields for its job file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JobText {
    pub name: String,
    pub material: String,
    pub thickness: f64,
    /// The job's gas as its programs name it (GS11); empty = none.
    pub gas: String,
    pub guid: String,
}

/// One nest: the sheets it is (identical sheets fold into one, BV59), its
/// program name and cut program.
#[derive(Clone, Debug)]
pub struct Nest {
    pub name: String,
    /// The sheets it cuts, by their place in the list given to `write`.
    pub sheets: Vec<usize>,
    pub ccp: Ccp,
    pub seconds: f64,
    pub size: [f64; 2],
}

/// One `.bvc` to write: its file name and entries, and the nests in it.
#[derive(Clone, Debug)]
pub struct BvcFile {
    pub file: String,
    pub entries: Vec<(String, String)>,
    pub nests: Vec<usize>,
}

/// Everything one machine gets for a job (BV21, BV22, BV59).
#[derive(Clone, Debug)]
pub struct Written {
    pub nests: Vec<Nest>,
    pub bvcs: Vec<BvcFile>,
    /// BV21, BV22: the loose files left beside the `.bvc` (none with B010).
    pub loose: Vec<(String, String)>,
}

/// A new Guid, no braces (BV43).
pub fn new_guid() -> String {
    use std::hash::{Hash, Hasher};
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (t, std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)).hash(&mut h);
    let a = h.finish();
    a.hash(&mut h);
    let b = h.finish();
    let x = format!("{a:016x}{b:016x}");
    // Version 4, variant 10.
    format!("{}-{}-4{}-{}{}-{}", &x[0..8], &x[8..12], &x[13..16], ["8", "9", "a", "b"][(b & 3) as usize], &x[17..20], &x[20..32])
}

/// The program name of a nest whose first sheet is `sheet_no`: as the ISO
/// post names a sheet's program (after the job, CFT A007), no extension.
pub fn program_name(job: &str, sheet_no: usize) -> String {
    let j: String = job.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase();
    format!("{}X{sheet_no}", if j.is_empty() { "JOB".into() } else { j })
}

/// BV43, BV44: the job file for `nests` (of `all`).
pub fn job_info(job: &JobText, parts: &[Part], all: &[Nest], nests: &[usize], post: &Post) -> String {
    let mut t = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n");
    // BV43: the post names the gas only with the technology's parameter
    // file, which the app doesn't know yet; X001 (ours, BV62) names it anyway.
    let gas = if job.gas.is_empty() || !post.on("X001") { String::new() } else { format!(" ProcessGas=\"{}\"", esc(&job.gas)) };
    let _ = write!(t, "<JobInfo Guid=\"{}\" Name=\"{}\" Material=\"{}\" Thickness=\"{}\" MeasurementSystem=\"Metric\" Parameter=\"\"{gas}>\r\n", esc(&job.guid), esc(&job.name), esc(&job.material), num(job.thickness));
    // The parts the nests place, in the part list's order (BV44: the id is
    // the place in the list).
    let mut used: Vec<usize> = nests.iter().flat_map(|&n| all[n].ccp.subs.iter().map(|s| s.part)).filter(|&p| p != CUTOFF).collect();
    used.sort();
    used.dedup();
    let qty = |p: usize| {
        let q = parts[p].qty as i64;
        if post.on("B022") && q < 1 { 1 } else { q }
    };
    t.push_str("  <PartInfos>\r\n");
    for &p in &used {
        let _ = write!(t, "    <PartInfo Id=\"{}\" Name=\"{}\" Quantity=\"{}\" />\r\n", p + 1, esc(&parts[p].name), qty(p));
    }
    t.push_str("  </PartInfos>\r\n  <MaterialInfos>\r\n");
    // One sheet size per id (BV59).
    let mut sizes: Vec<([f64; 2], usize)> = vec![];
    for &n in nests {
        let s = all[n].size;
        let count = all[n].sheets.len();
        match sizes.iter_mut().find(|x| x.0 == s) {
            Some(x) => x.1 += count,
            None => sizes.push((s, count)),
        }
    }
    for (k, (s, n)) in sizes.iter().enumerate() {
        let _ = write!(t, "    <MaterialInfo Id=\"{}\" Quantity=\"{n}\" SizeX=\"{}\" SizeY=\"{}\" FormatType=\"Sheet\" ArticleInfo=\"\" />\r\n", k + 1, num(s[0]), num(s[1]));
    }
    t.push_str("  </MaterialInfos>\r\n  <PlanInfos>\r\n");
    for &n in nests {
        let nest = &all[n];
        let mat = sizes.iter().position(|x| x.0 == nest.size).unwrap_or(0) + 1;
        let secs = whole(nest.seconds);
        let _ = write!(
            t,
            "    <PlanInfo Multiplicity=\"{}\" Name=\"{}\" MaterialId=\"{mat}\" EstimatedCuttingTime=\"{secs}\" SizeX=\"{}\" SizeY=\"{}\">\r\n",
            nest.sheets.len(),
            esc(&nest.name),
            num(nest.ccp.extent[0]),
            num(nest.ccp.extent[1])
        );
        t.push_str("      <PlanPartInfos>\r\n");
        // BV64: the cut-off subprogram cuts no part: no PlanPartInfo.
        for (k, s) in nest.ccp.subs.iter().enumerate().filter(|s| s.1.part != CUTOFF) {
            let id = s.part + 1;
            let id = if post.on("B022") && id < 1 { (k + 1).min(parts.len()) } else { id };
            let _ = write!(t, "        <PlanPartInfo PartId=\"{id}\" Quantity=\"{}\" SubProgramName=\"{}\" />\r\n", qty(s.part), esc(&s.name));
        }
        t.push_str("      </PlanPartInfos>\r\n");
        if post.on("B029") {
            t.push_str("      <ResidualSheetInfos />\r\n");
        }
        if post.on("B030") {
            t.push_str("      <BigPartInfos />\r\n");
        }
        let _ = write!(t, "      <SortInfos />\r\n      <EstimatedCuttingTime>{secs}</EstimatedCuttingTime>\r\n    </PlanInfo>\r\n");
    }
    t.push_str("  </PlanInfos>\r\n</JobInfo>\r\n");
    t
}

/// BV21, BV22, BV59: a machine's `.bvc` files for a job's sheets. Identical
/// sheets fold into one nest with their count; each nest gets its own `.bvc`,
/// or with B017 every nest goes into the first one's, with one job file for
/// them all.
pub fn write(job: &JobText, sheets: &[SheetIn], sheet_nos: &[usize], post: &Post) -> Written {
    let mut nests: Vec<Nest> = vec![];
    for (i, s) in sheets.iter().enumerate() {
        let ccp = cut_program(s, post);
        let size = [s.spec.w, s.spec.h];
        #[cfg(test)]
        let all = FOLD_ALL.with(|f| f.get());
        #[cfg(not(test))]
        let all = false;
        match nests.iter_mut().find(|n| all || (n.ccp.text == ccp.text && n.size == size)) {
            Some(n) => n.sheets.push(i),
            None => nests.push(Nest { name: program_name(&job.name, sheet_nos[i]), sheets: vec![i], ccp, seconds: s.plan.seconds, size }),
        }
    }
    let parts = sheets.first().map_or(&[][..], |s| s.parts);
    let mut bvcs = vec![];
    let mut loose = vec![];
    let all: Vec<usize> = (0..nests.len()).collect();
    if post.on("B017") && !nests.is_empty() {
        let info = job_info(job, parts, &nests, &all, post);
        let mut entries = vec![(format!("{}.ccp", nests[0].name), nests[0].ccp.text.clone()), ("JobInfo.xml".to_string(), info.clone())];
        for n in &nests[1..] {
            entries.push((format!("{}.ccp", n.name), n.ccp.text.clone()));
        }
        loose.extend(nests.iter().map(|n| (format!("{}.ccp", n.name), n.ccp.text.clone())));
        loose.push(("JobInfo.xml".to_string(), info));
        bvcs.push(BvcFile { file: format!("{}.bvc", nests[0].name), entries, nests: all });
    } else {
        for (k, n) in nests.iter().enumerate() {
            let info = job_info(job, parts, &nests, &[k], post);
            let ccp = (format!("{}.ccp", n.name), n.ccp.text.clone());
            loose.push(ccp.clone());
            bvcs.push(BvcFile { file: format!("{}.bvc", n.name), entries: vec![ccp, ("JobInfo.xml".to_string(), info.clone())], nests: vec![k] });
            // BV21: one JobInfo.xml in the folder, rewritten for each program.
            loose.retain(|f| f.0 != "JobInfo.xml");
            loose.push(("JobInfo.xml".to_string(), info));
        }
    }
    if post.on("B010") {
        loose.clear();
    }
    Written { nests, bvcs, loose }
}

#[cfg(test)]
mod bystar_tests {
    use super::*;

    #[test]
    fn numbers_are_written_as_bv52_says() {
        assert_eq!(num(25.0), "25");
        assert_eq!(num(0.5), "0.5");
        assert_eq!(num(12.345), "12.345");
        assert_eq!(num(-3.25), "-3.25");
        assert_eq!(num(1.0 / 3.0), "0.333333333");
        assert_eq!(num(2.0 / 3.0), "0.666666667");
        // −0.0 is 0; a tiny negative keeps its sign.
        assert_eq!(num(-0.0), "0");
        assert_eq!(num(-0.0000000001), "-0");
        // Float noise from our own sums counts as 0 (BV62).
        assert_eq!(num(-1e-15), "0");
        // Halves toward zero.
        assert_eq!(whole(12.5), "12");
        assert_eq!(whole(12.6), "13");
        assert_eq!(whole(-12.5), "-12");
        assert_eq!(digits(1.0005, 3), "1");
        assert_eq!(digits(1.0006, 3), "1.001");
    }

    #[test]
    fn switches_read_as_bv54_says() {
        let p = Post::default();
        assert_eq!(p.raw("B013"), "0.2", "shipped value without a file");
        assert!(SWITCHES.iter().all(|d| !p.changed(d.key)));
        let file = "B001 0 ; metric\nB006 2.5 ; split\nB007 0.5\nB015 0.6 ; on\nB042 7.9\nC004 \"P\" ; prefix\n";
        let p = Post::parse(Some(file), Some("A005 0.01\nA006 25.4 ; inch\n"), &MachinePost::default());
        assert_eq!(p.raw("B013"), "10", "missing from the file");
        assert_eq!(p.whole("B006"), 2, "halves down");
        assert_eq!(p.whole("B007"), 0);
        assert!(p.on("B015"));
        assert_eq!(p.cut("B042"), 7);
        assert_eq!(p.raw("C004"), "P");
        assert_eq!((p.tol, p.unit), (0.01, 25.4));
        let mut m = MachinePost::default();
        m.set.insert("B015".into(), "0".into());
        assert!(!Post::parse(Some(file), None, &m).on("B015"), "a value set in Settings wins over the file");
        let neg = Post::parse(Some("B039 -0.5\nB036 3\n"), None, &MachinePost::default());
        assert_eq!(neg.whole("B039"), -1);
        assert_eq!(neg.sensor_mode(), 1, "any other non-zero B039 acts as 1");
    }

    #[test]
    fn each_quality_gets_the_mode_bv32_gives() {
        let off = Post::default();
        let want = [0, 1, 3, 4, 5, 6, 7, 0, 0, 0, 0, 0];
        for q in 1..=12 {
            assert_eq!(off.mode(q), want[q - 1], "quality {q}, B015 off");
        }
        // B015 on: the technology table's column, the seed's until changed.
        let mut m = MachinePost::default();
        m.set.insert("B015".into(), "1".into());
        let on = Post::parse(None, None, &m);
        for q in 1..=12 {
            assert_eq!(on.mode(q), want[q - 1], "quality {q}, B015 on, seed table");
        }
        for q in 1..=12u8 {
            m.modes.insert(q, 10 + q as i64);
        }
        let on = Post::parse(None, None, &m);
        for q in 1..=12 {
            assert_eq!(on.mode(q), 10 + q as i64, "quality {q}, B015 on, changed table");
        }
        // B042 / B043 replace 7 / 3; B026 marks.
        let mut m = MachinePost::default();
        m.set.insert("B042".into(), "2".into());
        m.set.insert("B043".into(), "5.7".into());
        m.set.insert("B026".into(), "9".into());
        let p = Post::parse(None, None, &m);
        assert_eq!((p.mode(7), p.mode(3), p.mode(9)), (1, 5, 2));
    }

    #[test]
    fn pierce_modes_follow_bv33() {
        let p = Post::default();
        assert_eq!(p.pierce(3, false).as_deref(), Some("Fine"));
        assert_eq!(p.pierce(0, true).as_deref(), Some("Normal"));
        let mut m = MachinePost::default();
        for k in ["B007", "B008", "B009"] {
            m.set.insert(k.into(), "4".into());
        }
        m.set.insert("B009".into(), "5".into());
        let p = Post::parse(None, None, &m);
        assert_eq!(p.pierce(4, false).as_deref(), Some("Normal"), "Normal wins over Fine");
        assert_eq!(p.pierce(5, false).as_deref(), Some("NoTime"));
        let mut m = MachinePost::default();
        m.set.insert("B014".into(), "1".into());
        m.pierces.insert(2, String::new());
        m.pierces.insert(3, "Drilled".into());
        let p = Post::parse(None, None, &m);
        assert_eq!(p.pierce(2, false), None, "empty table text: no pierce");
        assert_eq!(p.pierce(3, false).as_deref(), Some("Drilled"));
    }

    #[test]
    fn full_turns_split_only_with_b018() {
        let c = [0.0, 0.0];
        let full = Mv::Arc { to: [10.0, 0.0], c, sweep: 360.0 };
        assert_eq!(split_full([10.0, 0.0], full, false), vec![full]);
        let two = split_full([10.0, 0.0], full, true);
        assert_eq!(two.len(), 2);
        assert!(matches!(two[0], Mv::Arc { sweep, to, .. } if sweep == 180.0 && d(to, [-10.0, 0.0]) < 1e-9));
        let three_quarters = Mv::Arc { to: [0.0, -10.0], c, sweep: 270.0 };
        assert_eq!(split_full([10.0, 0.0], three_quarters, true), vec![three_quarters], "180–360° stays whole");
        let back = split_full([10.0, 0.0], Mv::Arc { to: [0.0, 10.0], c, sweep: -450.0 }, true);
        let sweeps: Vec<f64> = back.iter().map(|m| if let Mv::Arc { sweep, .. } = m { *sweep } else { 0.0 }).collect();
        assert_eq!(sweeps, vec![-180.0, -180.0, -90.0]);
    }

    #[test]
    fn the_first_move_splits_at_b006_at_most_half_way() {
        let s = split_first([0.0, 0.0], Mv::Line([10.0, 0.0]), 2.0);
        assert_eq!(s, vec![Mv::Line([2.0, 0.0]), Mv::Line([10.0, 0.0])]);
        let s = split_first([0.0, 0.0], Mv::Line([3.0, 0.0]), 2.0);
        assert_eq!(s[0], Mv::Line([1.5, 0.0]));
    }

    // -----------------------------------------------------------------------
    // #127: joints, point marks, cut-offs at the writer's seam, each file
    // read back by the reader as an export does.
    // -----------------------------------------------------------------------

    use crate::nest::{Shape, SheetSpec};

    fn spec() -> SheetSpec {
        SheetSpec { w: 1000.0, h: 500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: None, pick: Default::default() }
    }

    /// A made-up 200 × 100 rectangle DEMO-001 at (100, 100) (or a Ø100 disc),
    /// its plan changed by `edit`, written with switches `set` and read back:
    /// (the cut program, the read-back's issues).
    fn demo(disc: bool, set: &[(&str, &str)], cuts: &[CutLine], edit: impl Fn(&mut SheetPlan)) -> (String, Vec<String>) {
        let (shape, w, h) = if disc { (Shape::Disc, 100.0, 100.0) } else { (Shape::Rectangle, 200.0, 100.0) };
        let parts = vec![Part { name: "DEMO-001".into(), shape, w, h, qty: 1, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let placed = vec![Placed { part: 0, x: 100.0, y: 100.0, w, h, turns: 0, mirror: false, spin: Default::default() }];
        let sp = spec();
        let mut plan = crate::cnc::plan(&parts, &placed, &sp);
        edit(&mut plan);
        let mut m = MachinePost::default();
        for (k, v) in set {
            m.set.insert(k.to_string(), v.to_string());
        }
        let post = Post::parse(None, None, &m);
        let ins = [SheetIn { parts: &parts, sheet: &placed, spec: &sp, plan: &plan, cuts }];
        let job = JobText { name: "DEMO".into(), thickness: 3.0, ..Default::default() };
        let w = write(&job, &ins, &[1], &post);
        let files: Vec<(String, Vec<u8>, Vec<usize>)> = w.bvcs.iter().map(|f| (f.file.clone(), crate::blm::zip(&f.entries), f.nests.clone())).collect();
        let nests: Vec<(String, Vec<usize>)> = w.nests.iter().map(|n| (n.name.clone(), n.sheets.clone())).collect();
        let back = [crate::engine::readback::BystarSheet { sheet_no: 1, parts: &parts, sheet: &placed, spec: &sp, plan: &plan, cuts }];
        let mut issues = crate::engine::readback::check_bvc(&files, &nests, &back, 3.0, &post);
        issues.extend(unsupported(&ins[0], &post));
        (w.nests[0].ccp.text.clone(), issues)
    }

    /// The commands of a cut program as (name, X, Y) in order (X, Y NaN when none).
    fn cmds(t: &str) -> Vec<(String, f64, f64)> {
        t.lines()
            .filter_map(|l| {
                let l = l.trim().strip_prefix('<')?.strip_suffix("/>")?;
                let name = l.split(' ').next()?.to_string();
                let v = |k: &str| l.split(&format!(" {k}=\"")).nth(1).and_then(|r| r.split('"').next()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(f64::NAN);
                Some((name, v("X"), v("Y")))
            })
            .collect()
    }

    /// The local end points the joint markers follow (the move before each).
    fn markers(t: &str) -> Vec<P> {
        let c = cmds(t);
        (1..c.len()).filter(|&k| c[k].0 == "ExecMicrojoint").map(|k| [c[k - 1].1, c[k - 1].2]).collect()
    }

    #[test]
    fn joints_at_a_contours_start_its_end_and_several_on_one_side_are_each_marked_at_their_middle() {
        // BV63: the outline 200 × 100 from its start; joints 1 mm wide.
        let per = 600.0;
        let cases: [(&str, Vec<(f64, f64)>); 3] = [("start", vec![(0.0, 1.0)]), ("end", vec![(per - 1.0, per)]), ("several", vec![(20.0, 21.0), (60.0, 61.0), (150.0, 152.0)])];
        for (what, js) in cases {
            let (t, issues) = demo(false, &[], &[], |p| {
                let k = p.contours.iter().position(|c| !c.inner).unwrap();
                assert!((crate::nest::perimeter(&p.contours[k].points) - per).abs() < 1e-6);
                p.contours[k].joints = js.clone();
            });
            assert_eq!(issues, Vec::<String>::new(), "{what}: {t}");
            let plan = {
                let parts = vec![Part { name: "DEMO-001".into(), shape: Shape::Rectangle, w: 200.0, h: 100.0, qty: 1, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
                let placed = vec![Placed { part: 0, x: 100.0, y: 100.0, w: 200.0, h: 100.0, turns: 0, mirror: false, spin: Default::default() }];
                crate::cnc::plan(&parts, &placed, &spec())
            };
            let c = plan.contours.iter().find(|c| !c.inner).unwrap();
            // The call is at the part's origin (100, 100), turned 0°.
            let want: Vec<P> = js.iter().map(|j| crate::cnc::point_at(&c.points, (j.0 + j.1) / 2.0)).map(|p| [p[0] - 100.0, p[1] - 100.0]).collect();
            let got = markers(&t);
            assert_eq!(got.len(), want.len(), "{what}: {t}");
            for (g, w) in got.iter().zip(&want) {
                assert!(d(*g, *w) < 1e-6, "{what}: marker at {g:?}, gap middle {w:?}");
            }
            // No rapid and no pierce inside the cut-out (BV36).
            let body = &t[t.find("BeginCutOut").unwrap()..t.find("EndCutOut").unwrap()];
            assert!(!body.contains("MoveLineTo") && !body.contains("ExecPiercing"), "{what}");
        }
    }

    #[test]
    fn after_a_joint_on_an_arc_the_arc_is_written_with_its_true_sweep_from_the_gaps_middle() {
        // The owner's answer on #122 (BV47): a Ø100 disc, one joint on its arc.
        let (t, issues) = demo(true, &[], &[], |p| {
            let k = p.contours.iter().position(|c| !c.inner).unwrap();
            p.contours[k].joints = vec![(100.0, 102.0)];
        });
        assert_eq!(issues, Vec::<String>::new(), "{t}");
        let c = cmds(&t);
        let k = c.iter().position(|x| x.0 == "ExecMicrojoint").unwrap();
        // The cut to the gap's middle is an arc (BV46), and the next one too.
        assert_eq!((c[k - 1].0.as_str(), c[k + 1].0.as_str()), ("ArcTo", "ArcTo"), "{t}");
        // The circle the arc after the joint makes, from the marker (where the
        // path is) to its end, has the disc's own centre and radius.
        let line_k = t.lines().position(|l| l.contains("ExecMicrojoint")).unwrap();
        let next = t.lines().nth(line_k + 1).unwrap();
        let sw: f64 = next.split("SweepAngle=\"").nth(1).unwrap().split('"').next().unwrap().parse().unwrap();
        let from = [c[k - 1].1, c[k - 1].2];
        let to = [c[k + 1].1, c[k + 1].2];
        let cn = crate::bvc::arc_centre(from, to, sw).unwrap();
        assert!(d(cn, [50.0, 50.0]) < 0.01 && (d(from, cn) - 50.0).abs() < 0.01 && (d(to, cn) - 50.0).abs() < 0.01, "centre {cn:?}: {t}");
    }

    #[test]
    fn b038_restarts_after_each_gap_and_b049_cuts_an_arc_gap_straight() {
        let (t, issues) = demo(true, &[("B038", "1")], &[], |p| {
            let k = p.contours.iter().position(|c| !c.inner).unwrap();
            p.contours[k].joints = vec![(100.0, 102.0)];
        });
        assert_eq!(issues, Vec::<String>::new(), "{t}");
        // BV48: the outline in two cut-outs (the disc's hole is the third),
        // the second after a rapid, no marker, no pierce.
        assert_eq!((t.matches("<BeginCutOut").count(), t.matches("ExecMicrojoint").count(), t.matches("<ExecPiercing").count()), (3, 0, 2), "{t}");
        let c = cmds(&t);
        let e = c.iter().enumerate().filter(|x| x.1 .0 == "EndCutOut").nth(1).unwrap().0;
        assert_eq!(c[e + 1].0, "SetToolCorrection");
        assert!(c[e..].iter().position(|x| x.0 == "MoveLineTo").unwrap() < c[e..].iter().position(|x| x.0 == "BeginCutOut").unwrap());
        // B044 pierces the restart too.
        let (t, issues) = demo(true, &[("B038", "1"), ("B044", "1")], &[], |p| {
            let k = p.contours.iter().position(|c| !c.inner).unwrap();
            p.contours[k].joints = vec![(100.0, 102.0)];
        });
        assert_eq!(issues, Vec::<String>::new(), "{t}");
        assert_eq!(t.matches("<ExecPiercing").count(), 3, "{t}");
        // B049: the gap on the arc is cut straight to its chord's middle.
        let (t, issues) = demo(true, &[("B049", "1")], &[], |p| {
            let k = p.contours.iter().position(|c| !c.inner).unwrap();
            p.contours[k].joints = vec![(100.0, 102.0)];
        });
        assert_eq!(issues, Vec::<String>::new(), "{t}");
        let c = cmds(&t);
        let k = c.iter().position(|x| x.0 == "ExecMicrojoint").unwrap();
        assert_eq!((c[k - 2].0.as_str(), c[k - 1].0.as_str(), c[k + 1].0.as_str()), ("ArcTo", "LineTo", "ArcTo"), "{t}");
        let mid = [c[k - 1].1, c[k - 1].2];
        assert!(d(mid, [50.0, 50.0]) < 50.0 - 1e-4, "the chord's middle is inside the circle: {mid:?}");
    }

    #[test]
    fn point_marks_are_circles_or_tiny_cuts_as_the_switches_say() {
        // The app makes no point marks yet: one put in the plan by hand.
        let pt = |p: &mut SheetPlan| p.marks.push((1, vec![[150.0, 150.0]]));
        // BV40: B013 0.2 (shipped): a circle of 0.2 in mode 3, pierced, two half arcs.
        let (t, issues) = demo(false, &[], &[], pt);
        assert_eq!(issues, Vec::<String>::new(), "{t}");
        let c = cmds(&t);
        let k = c.iter().position(|x| x.0 == "MoveLineTo").unwrap();
        let names: Vec<&str> = c[k - 2..k + 12].iter().map(|x| x.0.as_str()).collect();
        assert_eq!(names, ["SetOperationMode", "SetSensingSensor", "MoveLineTo", "SetToolCorrection", "BeginLeadIn", "SetPiercingMode", "ExecPiercing", "LineTo", "EndLeadIn", "BeginCutOut", "ArcTo", "ArcTo", "EndCutOut", "SetToolCorrection"], "{t}");
        assert_eq!(t.lines().find(|l| l.contains("SetOperationMode")).map(|l| l.contains("Value=\"3\"")), Some(true), "{t}");
        assert_eq!([c[k + 5].1, c[k + 5].2], [50.1, 50.0], "the lead-in to x + d/2");
        assert!(t.contains("SweepAngle=\"180\""));
        // B033: marking mode 2; B031: no compensation.
        let (t, issues) = demo(false, &[("B033", "1"), ("B031", "1"), ("B013", "3")], &[], pt);
        assert_eq!(issues, Vec::<String>::new(), "{t}");
        let first = t.find("Value=\"2\"").unwrap();
        assert!(first < t.find("MoveLineTo").unwrap() && !t[..t.find("EndCutOut").unwrap()].contains("Value=\"Right\""), "{t}");
        assert!(t.contains("X=\"48.5\" Y=\"50\" SweepAngle=\"180\""), "a 3 mm circle: {t}");
        // BV55: B013 at the tolerance, B050: an on/off point as a tiny cut.
        let (t, issues) = demo(false, &[("B013", "0"), ("B050", "1")], &[], pt);
        assert_eq!(issues, Vec::<String>::new(), "{t}");
        let c = cmds(&t);
        let k = c.iter().position(|x| x.0 == "MoveLineTo").unwrap();
        let names: Vec<&str> = c[k..k + 6].iter().map(|x| x.0.as_str()).collect();
        assert_eq!(names, ["MoveLineTo", "BeginCutOut", "SetPiercingMode", "ExecPiercing", "LineTo", "EndCutOut"], "{t}");
        assert!(t.contains("Contour=\"Interior\"") && t.contains("X=\"49.95\" Y=\"50\""), "{t}");
        // Nothing, or a cross: refused, not written another way.
        let (_, issues) = demo(false, &[("B013", "0")], &[], pt);
        assert!(issues.iter().any(|m| m.contains("point marks") && m.contains("as nothing")), "{issues:?}");
        let (_, issues) = demo(false, &[("B034", "2")], &[], pt);
        assert!(issues.iter().any(|m| m.contains("B034")), "{issues:?}");
    }
}
