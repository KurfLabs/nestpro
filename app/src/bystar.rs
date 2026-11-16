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
    sw("B002", "Put every cut, cut-off lines too, into subprograms", OnOff, "0", "BV28", Later),
    sw("B003", "Cut-off lines: overrun past each sheet edge", Length, "0", "BV38", Later),
    sw("B004", "Cut-off lines: height sensing off for this distance before each edge", Length, "0", "BV38", Later),
    sw("B005", "Cut-off lines: run-up between the pierce and the sensing-off point", Length, "0", "BV38", Later),
    sw("B006", "Split the first move of a contour without a lead-in at this distance", Length, "0", "BV34", Built),
    sw("B007", "Quality that pierces Normal", Quality, "0", "BV33", Built),
    sw("B008", "Quality that pierces Fine", Quality, "0", "BV33", Built),
    sw("B009", "Quality that pierces NoTime", Quality, "0", "BV33", Built),
    sw("B010", "Keep only the .bvc, delete the loose files", OnOff, "0", "BV22", Built),
    sw("B011", "Compensation after the rapid instead of before it", OnOff, "0", "BV29", Built),
    sw("B012", "Compensation off after a mark", OnOff, "0", "BV31", Later),
    sw("B013", "Point marks as circles of this diameter", Size, "0.2", "BV40", Later),
    sw("B014", "Pierce mode from the technology table", OnOff, "0", "BV33", Built),
    sw("B015", "Operation mode from the technology table", OnOff, "0", "BV32", Built),
    sw("B016", "Subprogram name also as PartNumber; cut-off subprograms numbered", OnOff, "0", "BV26", Built),
    sw("B017", "All sheets of one run in one .bvc", OnOff, "0", "BV22", Built),
    sw("B018", "Split arcs of 360° or more into half circles", OnOff, "0", "BV35", Built),
    sw("B019", "Joint marker at the end: before the next rapid or cut-out end", OnOff, "0", "BV48", Later),
    sw("B020", "Only lines marked as cut-off lines are cut-offs", OnOff, "0", "BV39", Later),
    sw("B021", "Part call positions with 3 decimals instead of whole units", OnOff, "0", "BV52", Built),
    sw("B022", "Part id and quantity under 0.5 are fixed", OnOff, "0", "BV44", Built),
    sw("B023", "Pierce at a joint's lead-in", OnOff, "0", "BV48", Later),
    sw("B024", "Lead-in bracket and pierce at every contour start", OnOff, "0", "BV34", Built),
    sw("B025", "A cut-off line not starting on an edge gets its own rapid and pierce", OnOff, "0", "BV39", Later),
    sw("B026", "Quality used for marking: mode 2, Normal pierce", Quality, "0", "BV32", Built),
    sw("B027", "PartHeight in every subprogram header", OnOff, "0", "BV26", Later),
    sw("B028", "Cut-off subprograms named like part subprograms", OnOff, "0", "BV26", Later),
    sw("B029", "Empty ResidualSheetInfos in the job file", OnOff, "0", "BV43", Built),
    sw("B030", "Empty BigPartInfos in the job file", OnOff, "0", "BV43", Built),
    sw("B031", "No compensation on marks and point marks", OnOff, "0", "BV31", Built),
    sw("B032", "Close an open cut-out at the end of a subprogram", OnOff, "0", "BV28", Later),
    sw("B033", "Point-mark circles in marking mode 2", OnOff, "0", "BV40", Later),
    sw("B034", "Point marks as a cross of this size instead of a circle", Size, "0", "BV40", Later),
    sw("B035", "Two-line cut-off mode", OnOff, "0", "BV39", Later),
    sw("B036", "New marking treatment: compensation off, Interior, pierce Fine (2: Normal)", Choice, "0", "BV29", Later),
    sw("B037", "Never NoTime on partly cut common-cut contours", OnOff, "0", "BV33", Later),
    sw("B038", "New micro-joint treatment: stop at the gap and start again", OnOff, "0", "BV48", Later),
    sw("B039", "Height sensor: 1 not switched on for marks, 2 only in the cut-off sequence", Choice, "0", "BV37", Built),
    sw("B040", "Compensation after the lead-in", OnOff, "0", "BV31", Built),
    sw("B041", "ToolArrangement=\"Vertical\" in the header", OnOff, "0", "BV23", Built),
    sw("B042", "Quality that replaces quality 7", Quality, "0", "BV32", Built),
    sw("B043", "Quality that replaces quality 3", Quality, "0", "BV32", Built),
    sw("B044", "Pierce contours that have no lead-in", OnOff, "0", "BV29", Built),
    sw("B045", "Quality for cut-off lines", Quality, "0", "BV32", Later),
    sw("B046", "New cut-off management", OnOff, "0", "BV39", Later),
    sw("B047", "Joint marker before a joint cut in the contour's second-to-last place", OnOff, "0", "BV48", Later),
    sw("B048", "Per-edge cut-off distances R000–R007", OnOff, "0", "BV39", Later),
    sw("B049", "Joints always straight", OnOff, "0", "BV46", Later),
    sw("B050", "On/off points cut as a tiny cut when no circle is made", OnOff, "0", "BV55", Later),
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
        Post { values, from, tol: num("A005").unwrap_or(0.005), unit: num("A006").unwrap_or(1.0), modes, pierces, file_error: None }
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

/// What one placed part's subprogram cuts, in writing order: its marks and
/// contours in its cut order (CO9).
fn part_commands(plan: &SheetPlan, part_no: usize, f: &Frame, post: &Post, w: f64, h: f64, st: &mut State, first_fault: &mut bool) -> Vec<Cmd> {
    let u = post.unit;
    let loc = |p: P| f.local(p, u);
    let mut out = vec![];
    // BV51: the mode is forgotten at each new subprogram (each new part).
    st.mode = None;
    for made in plan.part_order(part_no) {
        let (q, mark, inner, lead, body, closed, out_path, comp_left) = match made {
            Made::Mark(j) => (0usize, true, false, vec![], plan.marks[j].1.clone(), false, vec![], false),
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
                (c.quality, false, c.inner, if c.lead_path.len() >= 2 { c.lead_path.clone() } else { vec![] }, c.points.clone(), true, tail, left)
            }
        };
        let marking = mark || post.is_marking(q);
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
        // BV32: the operation mode, written only when it changes (BV51).
        let mode = if mark { 2 } else { post.mode(q) };
        if st.mode != Some(mode) {
            out.push(val("SetOperationMode", mode.to_string()));
            st.mode = Some(mode);
        }
        // BV37: the height sensor at the contour start, by where it pierces.
        let pierce_at = f.place(start);
        let pm = [pierce_at[0] * u, pierce_at[1] * u];
        let inside = pm[0] > post.tol && pm[1] > post.tol && pm[0] < w - post.tol && pm[1] < h - post.tol;
        match post.sensor_mode() {
            2 => {}
            s => {
                if inside && !st.sensor && !(s == 1 && marking) {
                    out.push(val("SetSensingSensor", "On"));
                    st.sensor = true;
                } else if !inside && st.sensor {
                    out.push(val("SetSensingSensor", "Off"));
                    st.sensor = false;
                }
            }
        }
        // BV31: compensation Left for a left side, Right for any other; marks
        // too unless B031.
        let comp = if marking && post.on("B031") { None } else { Some(if comp_left && !mark { "Left" } else { "Right" }) };
        // BV29, BV34: a lead-in bracket and pierce with a lead-in, or with B024.
        let bracket = has_lead || post.on("B024");
        let comp_at_cut = post.on("B040") && bracket;
        if let (Some(c), false, false) = (comp, post.on("B011"), comp_at_cut) {
            out.push(val("SetToolCorrection", c));
        }
        out.push(at("MoveLineTo", start));
        if let (Some(c), true, false) = (comp, post.on("B011"), comp_at_cut) {
            out.push(val("SetToolCorrection", c));
        }
        let pierce = post.pierce(q, marking);
        let pierce_cmds = |out: &mut Vec<Cmd>| {
            if let Some(p) = &pierce {
                out.push(val("SetPiercingMode", p.clone()));
                out.push(cmd("ExecPiercing"));
            }
        };
        let mut from = start;
        let write_moves = |out: &mut Vec<Cmd>, mv: &[Mv], from: &mut P, first_fault: &mut bool| {
            for m in mv {
                for piece in split_full(*from, *m, post.on("B018")) {
                    match piece {
                        Mv::Line(p) => {
                            let mut p = p;
                            if *first_fault && fault() != 0.0 {
                                p[0] += fault() / u;
                                *first_fault = false;
                            }
                            out.push(at("LineTo", p));
                        }
                        Mv::Arc { to, sweep, .. } => out.push(Cmd("ArcTo", vec![("X", num(to[0])), ("Y", num(to[1])), ("SweepAngle", num(sweep))])),
                    }
                    *from = piece.end();
                }
            }
        };
        if bracket {
            out.push(cmd("BeginLeadIn"));
            pierce_cmds(&mut out);
            if has_lead {
                let mut no_fault = false;
                write_moves(&mut out, &moves(&lead, u), &mut from, &mut no_fault);
            }
            out.push(cmd("EndLeadIn"));
        } else if post.on("B044") {
            pierce_cmds(&mut out);
        }
        if let (Some(c), true) = (comp, comp_at_cut) {
            out.push(val("SetToolCorrection", c));
        }
        // BV30: Exterior for a part's outline, Interior for a hole, Unknown
        // for marking (and the B026 quality).
        let side = if marking { "Unknown" } else if inner { "Interior" } else { "Exterior" };
        out.push(Cmd("BeginCutOut", vec![("Contour", side.into())]));
        let mut mv = moves(&body, u);
        if !bracket && !mv.is_empty() {
            let b006 = post.whole("B006") as f64;
            if b006 > 0.0 {
                let first = mv.remove(0);
                let mut s = split_first(from, first, b006);
                s.extend(mv);
                mv = s;
            }
        }
        write_moves(&mut out, &mv, &mut from, first_fault);
        // BV34, BV57: the overlap and lead-out in their bracket, before EndCutOut.
        let tail: Vec<P> = out_path.into_iter().map(loc).collect();
        if !tail.is_empty() {
            out.push(cmd("BeginLeadOut"));
            let mut t = vec![from];
            t.extend(tail);
            let mut no_fault = false;
            write_moves(&mut out, &moves(&t, u), &mut from, &mut no_fault);
            out.push(cmd("EndLeadOut"));
        }
        out.push(cmd("EndCutOut"));
        if comp.is_some() {
            out.push(val("SetToolCorrection", "None"));
        }
    }
    out
}

/// One subprogram written: its name, the part it cuts and how many times the
/// nest calls it.
#[derive(Clone, Debug, PartialEq)]
pub struct SubOut {
    pub name: String,
    pub part: usize,
    pub calls: usize,
}

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
    for &i in &order {
        let pl = &s.sheet[i];
        let f = Frame::of(pl, u, post.on("B021"));
        // A copy whose commands come out the same shares the subprogram;
        // working it out mustn't change what the program has written so far.
        let mut probe = State { mode: st.mode, sensor: st.sensor };
        let mut ff = first_fault;
        let cmds = part_commands(s.plan, i + 1, &f, post, s.spec.w, s.spec.h, &mut probe, &mut ff);
        // BV37: the sensor is as the call leaves it, whether its subprogram
        // is new or one written before with the same commands.
        st = probe;
        let k = match variants.iter().position(|v| v.1 == pl.part && v.0 == cmds) {
            Some(k) => k,
            None => {
                first_fault = ff;
                variants.push((cmds, pl.part, 0));
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
        if !c.joints.is_empty() {
            add(format!("part {n} has micro-joints, which the ByStar Fiber laser's programs don't carry yet; take them off the part first"));
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
    if s.cuts.iter().any(|c| c.pts.iter().any(|q| d(*q, c.pts[0]) > 1e-6)) {
        add("the sheet has remnant or cut-off lines, which the ByStar Fiber laser's programs don't carry yet; take them off the sheet first".into());
    }
    // BV60: a switch the writer doesn't follow yet, changed, that would
    // change this sheet's program.
    // A contour in the B026 marking quality is marking too (BV30–BV33).
    if !s.plan.marks.is_empty() || s.plan.contours.iter().any(|c| c.punch.is_none() && post.is_marking(c.quality)) {
        for k in ["B012", "B036"] {
            if post.changed(k) {
                add(format!("switch {k} is changed, and the sheet has marking, which this writer doesn't write that way yet; set {k} back to {}", switch(k).map_or("0", |d| d.default)));
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
    let mut used: Vec<usize> = nests.iter().flat_map(|&n| all[n].ccp.subs.iter().map(|s| s.part)).collect();
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
        for (k, s) in nest.ccp.subs.iter().enumerate() {
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
}
