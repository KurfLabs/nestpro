//! Open a ByStar Fiber `.bvc` (written by BySoft, the original's post or
//! this app) as a nesting: a zip of a job file (`JobInfo.xml`), one cut
//! program (`.ccp`) per nest and maybe a preview picture (BV1–BV20). Each
//! part subprogram becomes a part (its cut contours, arcs included, and its
//! engraved lines), each `CallSubProgram` a placed part, each nest a sheet
//! (or that many identical sheets) and the remnant cut-offs remnant lines.
//! This reader shares nothing with a writer, so it can check one (BV20).

use crate::dxf::{Contour, ContourState, CustomGeo};
use crate::nest::{CutQuality, Placed, Spin, P};
use std::collections::BTreeMap;

/// Arcs are rebuilt with a point at least every this many degrees.
const ARC_STEP_DEG: f64 = 5.0;
/// A cut that comes back this close to where it started is closed (mm).
const CLOSE_TOL: f64 = 0.05;

/// A part read from a `.bvc`.
#[derive(Debug, Clone)]
pub struct BvcPart {
    pub name: String,
    /// The job file's quantity, else how many are placed.
    pub qty: u32,
    pub geo: CustomGeo,
}

/// One nest of a `.bvc`: a sheet, `count` times.
#[derive(Debug, Clone)]
pub struct BvcSheet {
    pub w: f64,
    pub h: f64,
    pub count: usize,
    pub placed: Vec<Placed>,
    /// Remnant and skeleton cut-offs, in sheet coordinates.
    pub cuts: Vec<Vec<P>>,
}

/// A `.bvc` read as a nesting (lengths in mm).
#[derive(Debug, Clone, Default)]
pub struct Bvc {
    pub name: String,
    pub material: String,
    pub thickness: f64,
    pub gas: String,
    pub parts: Vec<BvcPart>,
    pub sheets: Vec<BvcSheet>,
    pub notes: Vec<String>,
}

// ---------------------------------------------------------------------------
// A small XML reader: the elements in order with their attributes.
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Tag {
    name: String,
    attrs: Vec<(String, String)>,
    /// `</name>`.
    close: bool,
    /// `<name … />`.
    empty: bool,
}

impl Tag {
    fn get(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
    }
    fn num(&self, k: &str) -> Option<f64> {
        self.get(k).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite())
    }
    fn need(&self, k: &str) -> Result<f64, String> {
        self.num(k).ok_or_else(|| format!("a {} without a number {k}", self.name))
    }
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut o = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        o.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(j) = tail.find(';').filter(|&j| j <= 10) else {
            o.push('&');
            rest = &tail[1..];
            continue;
        };
        let ent = &tail[1..j];
        let c = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => ent.strip_prefix("#x").or_else(|| ent.strip_prefix("#X")).and_then(|h| u32::from_str_radix(h, 16).ok()).or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok())).and_then(char::from_u32),
        };
        match c {
            Some(c) => o.push(c),
            None => o.push_str(&tail[..=j]),
        }
        rest = &tail[j + 1..];
    }
    o.push_str(rest);
    o
}

/// Every element of `text`, in order. Declarations and comments are skipped;
/// text between elements is ignored (the formats carry everything in
/// attributes).
fn tags(text: &str) -> Result<Vec<Tag>, String> {
    let mut out = vec![];
    let mut rest = text;
    while let Some(i) = rest.find('<') {
        rest = &rest[i + 1..];
        if let Some(r) = rest.strip_prefix("!--") {
            let j = r.find("-->").ok_or("a comment that never ends")?;
            rest = &r[j + 3..];
            continue;
        }
        if rest.starts_with('?') || rest.starts_with('!') {
            let j = rest.find('>').ok_or("a declaration that never ends")?;
            rest = &rest[j + 1..];
            continue;
        }
        let close = rest.starts_with('/');
        if close {
            rest = &rest[1..];
        }
        let end = rest.find(|c: char| c.is_whitespace() || c == '>' || c == '/').ok_or("an element that never ends")?;
        let name = rest[..end].to_string();
        if name.is_empty() {
            return Err("an element without a name".into());
        }
        rest = &rest[end..];
        let mut attrs = vec![];
        let mut empty = false;
        loop {
            rest = rest.trim_start();
            if let Some(r) = rest.strip_prefix("/>") {
                empty = true;
                rest = r;
                break;
            }
            if let Some(r) = rest.strip_prefix('>') {
                rest = r;
                break;
            }
            let eq = rest.find('=').ok_or_else(|| format!("<{name}> isn't closed"))?;
            let key = rest[..eq].trim().to_string();
            if key.is_empty() || key.contains(|c: char| c == '<' || c == '>' || c.is_whitespace()) {
                return Err(format!("<{name}> has a broken attribute"));
            }
            let r = rest[eq + 1..].trim_start();
            let q = r.chars().next().filter(|c| *c == '"' || *c == '\'').ok_or_else(|| format!("<{name}> {key}= has no quoted value"))?;
            let r = &r[1..];
            let j = r.find(q).ok_or_else(|| format!("<{name}> {key}= is never closed"))?;
            attrs.push((key, unescape(&r[..j])));
            rest = &r[j + 1..];
        }
        out.push(Tag { name, attrs, close, empty });
    }
    Ok(out)
}

/// The elements of a whole document: each one closed in order, under one
/// root element named `root` (a file cut short or mixed up is refused).
fn document(text: &str, root: &str) -> Result<Vec<Tag>, String> {
    let all = tags(text)?;
    let mut open: Vec<&str> = vec![];
    let mut roots = 0;
    for t in &all {
        if open.is_empty() {
            if t.close {
                return Err(format!("</{}> closes nothing", t.name));
            }
            roots += 1;
            if roots > 1 || t.name != root {
                return Err(format!("it should hold one {root}, not <{}> here", t.name));
            }
        }
        if t.close {
            match open.pop() {
                Some(o) if o == t.name => {}
                Some(o) => return Err(format!("<{o}> is closed by </{}>", t.name)),
                None => return Err(format!("</{}> closes nothing", t.name)),
            }
        } else if !t.empty {
            open.push(&t.name);
        }
    }
    if let Some(o) = open.last() {
        return Err(format!("it ends before <{o}> is closed (cut short?)"));
    }
    if roots == 0 {
        return Err(format!("it has no {root}"));
    }
    Ok(all)
}

/// The file's text: UTF-8, else the PC's ANSI code page (BV4), read as Latin-1.
fn text_of(b: &[u8]) -> String {
    let b = b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(b);
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b.iter().map(|&c| c as char).collect(),
    }
}

// ---------------------------------------------------------------------------
// The job file (BV18).
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct JobInfo {
    name: String,
    material: String,
    thickness: f64,
    gas: String,
    /// PartInfo Id → (name, quantity).
    parts: BTreeMap<i64, (String, u32)>,
    /// PlanInfo Name → (multiplicity, SubProgramName → PartId).
    plans: Vec<(String, usize, BTreeMap<String, i64>)>,
}

fn job_info(text: &str) -> Result<JobInfo, String> {
    let mut j = JobInfo::default();
    let mut seen = false;
    for t in document(text, "JobInfo")?.into_iter().filter(|t| !t.close) {
        match t.name.as_str() {
            "JobInfo" => {
                seen = true;
                j.name = t.get("Name").unwrap_or_default().trim().to_string();
                j.material = t.get("Material").unwrap_or_default().trim().to_string();
                j.thickness = t.num("Thickness").unwrap_or(0.0);
                j.gas = t.get("ProcessGas").unwrap_or_default().trim().to_string();
            }
            "PartInfo" => {
                if let Some(id) = t.num("Id") {
                    j.parts.insert(id as i64, (t.get("Name").unwrap_or_default().trim().to_string(), t.num("Quantity").unwrap_or(0.0).max(0.0) as u32));
                }
            }
            "PlanInfo" => j.plans.push((t.get("Name").unwrap_or_default().trim().to_string(), t.num("Multiplicity").unwrap_or(1.0).max(1.0) as usize, BTreeMap::new())),
            "PlanPartInfo" => {
                if let (Some(p), Some(id), Some(sub)) = (j.plans.last_mut(), t.num("PartId"), t.get("SubProgramName")) {
                    p.2.insert(sub.trim().to_string(), id as i64);
                }
            }
            _ => {}
        }
    }
    if !seen {
        return Err("its job file (JobInfo.xml) has no JobInfo".into());
    }
    Ok(j)
}

// ---------------------------------------------------------------------------
// The cut program (BV7–BV17).
// ---------------------------------------------------------------------------

/// One cut-out as cut: its points, closed or not, the operation mode it was
/// cut in and its kind (`BeginCutOut Contour`).
#[derive(Debug, Clone)]
struct Cut {
    pts: Vec<P>,
    closed: bool,
    mode: i64,
}

#[derive(Debug, Default)]
struct Sub {
    name: String,
    part_number: Option<i64>,
    cuts: Vec<Cut>,
}

#[derive(Debug, Default)]
struct Program {
    scale: f64,
    sheet: (f64, f64),
    thickness: f64,
    subs: Vec<Sub>,
    /// Cuts in the main program (cut-offs written outside a subprogram).
    main: Vec<Cut>,
    /// (subprogram name, x, y, rotation in degrees).
    calls: Vec<(String, f64, f64, f64)>,
    unknown: BTreeMap<String, usize>,
    full_circles: usize,
    joints: usize,
}

/// Where the commands of a program have got to.
#[derive(Default)]
struct Pen {
    pos: P,
    mode: i64,
    lead: bool,
    open: Option<(Vec<P>, bool)>,
}

impl Pen {
    /// Close the open cut-out into `out` (BV30).
    fn finish(&mut self, out: &mut Vec<Cut>) {
        let Some((mut pts, broken)) = self.open.take() else { return };
        if broken || pts.len() < 2 {
            return;
        }
        // Closed when it comes back to its start; a cut run on past the start
        // (an overlap before the lead-out) ends there.
        let back = (2..pts.len()).rev().find(|&k| dist(pts[k], pts[0]) < CLOSE_TOL);
        let closed = back.is_some();
        if let Some(k) = back {
            pts.truncate(k);
        }
        out.push(Cut { pts, closed, mode: self.mode });
    }
}

fn dist(a: P, b: P) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// BV14: the points of an arc from `a` to `b` turning `sweep` degrees
/// (positive counter-clockwise), `b` included; None for a full circle,
/// whose centre the file doesn't give.
fn arc(a: P, b: P, sweep: f64) -> Option<Vec<P>> {
    let l = dist(a, b);
    if sweep.abs() < 1e-9 || l < 1e-9 {
        return if sweep.abs() >= 359.0 { None } else { Some(vec![b]) };
    }
    let t = sweep.to_radians();
    let n = [-(b[1] - a[1]) / l, (b[0] - a[0]) / l];
    let k = (l / 2.0) / (t / 2.0).tan();
    let c = [(a[0] + b[0]) / 2.0 + n[0] * k, (a[1] + b[1]) / 2.0 + n[1] * k];
    let r = dist(a, c);
    let a0 = (a[1] - c[1]).atan2(a[0] - c[0]);
    let steps = ((sweep.abs() / ARC_STEP_DEG).ceil() as usize).max(1);
    let mut v: Vec<P> = (1..steps).map(|i| {
        let ang = a0 + t * i as f64 / steps as f64;
        [c[0] + r * ang.cos(), c[1] + r * ang.sin()]
    }).collect();
    v.push(b);
    Some(v)
}

fn program(text: &str) -> Result<Program, String> {
    let mut p = Program { scale: 1.0, ..Default::default() };
    let mut seen = false;
    let mut sub: Option<Sub> = None;
    let mut sub_pen = Pen::default();
    let mut main_pen = Pen::default();
    for t in document(text, "CuttingControlProgram")? {
        if t.name == "SubProgram" {
            if let Some(mut s) = sub.take() {
                sub_pen.finish(&mut s.cuts);
                p.subs.push(s);
            }
            if !t.close && !t.empty {
                // A new subprogram: the program's state starts again.
                sub_pen = Pen::default();
                sub = Some(Sub { name: t.get("Name").unwrap_or_default().trim().to_string(), part_number: t.num("PartNumber").map(|v| v as i64), cuts: vec![] });
            }
            continue;
        }
        if t.close {
            continue;
        }
        let s = p.scale;
        let pen = if sub.is_some() { &mut sub_pen } else { &mut main_pen };
        let cuts = match sub.as_mut() {
            Some(s) => &mut s.cuts,
            None => &mut p.main,
        };
        match t.name.as_str() {
            "CuttingControlProgram" => {
                seen = true;
                // BV8, BV52: an Imperial program is in inches.
                p.scale = if t.get("MeasurementSystem").is_some_and(|v| v.eq_ignore_ascii_case("Imperial")) { 25.4 } else { 1.0 };
                p.thickness = t.num("Thickness").unwrap_or(0.0) * p.scale;
            }
            "ApplicationFlat" => p.sheet = (t.num("SheetWidth").unwrap_or(0.0) * s, t.num("SheetHeight").unwrap_or(0.0) * s),
            "Technology" | "SubPrograms" | "ControlCommands" => {}
            "SetOperationMode" => pen.mode = t.need("Value")? as i64,
            "MoveLineTo" => {
                // A rapid ends any cut-out left open (B038's joints, BV45).
                pen.finish(cuts);
                pen.pos = [t.need("X")? * s, t.need("Y")? * s];
            }
            "LineTo" => {
                let to = [t.need("X")? * s, t.need("Y")? * s];
                if let Some((pts, _)) = pen.open.as_mut().filter(|_| !pen.lead) {
                    pts.push(to);
                }
                pen.pos = to;
            }
            "ArcTo" => {
                let to = [t.need("X")? * s, t.need("Y")? * s];
                let sweep = t.need("SweepAngle")?;
                match arc(pen.pos, to, sweep) {
                    Some(pts) => {
                        if let Some((v, _)) = pen.open.as_mut().filter(|_| !pen.lead) {
                            v.extend(pts);
                        }
                    }
                    None => {
                        // BV14: a full circle in one arc has no centre.
                        if let Some((_, broken)) = pen.open.as_mut() {
                            *broken = true;
                        }
                        p.full_circles += 1;
                    }
                }
                pen.pos = to;
            }
            "BeginLeadIn" | "BeginLeadOut" => pen.lead = true,
            "EndLeadIn" | "EndLeadOut" => pen.lead = false,
            "BeginCutOut" => {
                // A lead-in whose end isn't written ends here (BV27).
                pen.lead = false;
                pen.finish(cuts);
                pen.open = Some((vec![pen.pos], false));
            }
            "EndCutOut" => {
                pen.lead = false;
                pen.finish(cuts);
            }
            "ExecMicrojoint" => p.joints += 1,
            "CallSubProgram" => {
                let name = t.get("SubProgramName").ok_or("a CallSubProgram without a SubProgramName")?.trim().to_string();
                p.calls.push((name, t.need("X")? * s, t.need("Y")? * s, t.num("Rotation").unwrap_or(0.0)));
            }
            "SetToolCorrection" | "SetPiercingMode" | "ExecPiercing" | "SetSensingSensor" | "ExecHighPositioning" | "ExecDwell" | "ExecOptionalStop" | "ExecOptionalStopWithoutLifting" | "SetClamps" => {}
            other => *p.unknown.entry(other.to_string()).or_default() += 1,
        }
    }
    if !seen {
        return Err("it has no CuttingControlProgram".into());
    }
    if let Some(mut s) = sub.take() {
        sub_pen.finish(&mut s.cuts);
        p.subs.push(s);
    }
    main_pen.finish(&mut p.main);
    Ok(p)
}

// ---------------------------------------------------------------------------
// From cut programs to parts and sheets.
// ---------------------------------------------------------------------------

/// BV32 read backwards: the cut quality an operation mode comes from (the
/// post's default mapping); None for marking (mode 2) and modes it never
/// writes.
pub fn quality_of_mode(mode: i64) -> Option<CutQuality> {
    match mode {
        0 => CutQuality::new(1),
        1 => CutQuality::new(2),
        3..=7 => CutQuality::new(mode as usize),
        _ => None,
    }
}

/// What makes two subprograms the same part: their contours, states and
/// cut qualities, to a hundredth of a millimetre.
fn shape_key(g: &CustomGeo) -> String {
    let mut k = String::new();
    for c in &g.contours {
        k += &format!("{}:{}:{:?}:", c.state.key(), c.closed, c.quality.map(|q| q.number()));
        for p in &c.pts {
            k += &format!("{},{};", (p[0] * 100.0).round() as i64, (p[1] * 100.0).round() as i64);
        }
        k.push('|');
    }
    k
}

fn area(p: &[P]) -> f64 {
    let n = p.len();
    (0..n).map(|i| p[i][0] * p[(i + 1) % n][1] - p[(i + 1) % n][0] * p[i][1]).sum::<f64>() / 2.0
}

/// A part subprogram's geometry, moved so its outline's box starts at 0, 0,
/// and that offset. None when it has no closed cut.
fn part_geo(cuts: &[Cut], notes: &mut Vec<String>, name: &str) -> Option<(CustomGeo, P)> {
    let is_mark = |c: &Cut| c.mode == 2;
    let closed: Vec<&Cut> = cuts.iter().filter(|c| c.closed && !is_mark(c)).collect();
    let oi = (0..closed.len()).max_by(|&a, &b| area(&closed[a].pts).abs().total_cmp(&area(&closed[b].pts).abs()))?;
    let o = closed[oi];
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for q in &o.pts {
        lo = [lo[0].min(q[0]), lo[1].min(q[1])];
        hi = [hi[0].max(q[0]), hi[1].max(q[1])];
    }
    let sh = |l: &[P]| l.iter().map(|q| [q[0] - lo[0], q[1] - lo[1]]).collect::<Vec<P>>();
    let cut = |c: &Cut| {
        let mut k = Contour::new(sh(&c.pts), true, ContourState::Cut);
        // Each contour's operation mode is its cut quality (BV32).
        k.quality = quality_of_mode(c.mode);
        k
    };
    let mut contours: Vec<Contour> = closed.iter().enumerate().filter(|(k, _)| *k != oi).map(|(_, c)| cut(c)).collect();
    contours.extend(cuts.iter().filter(|c| c.closed && is_mark(c)).map(|c| Contour::new(sh(&c.pts), true, ContourState::Mark)));
    contours.push(cut(o));
    let mut open_cut = 0;
    for c in cuts.iter().filter(|c| !c.closed) {
        let state = if is_mark(c) {
            ContourState::Mark
        } else {
            open_cut += 1;
            ContourState::NoCut
        };
        contours.push(Contour::new(sh(&c.pts), false, state));
    }
    if open_cut > 0 {
        notes.push(format!("{name}: {open_cut} open cut line(s) kept as No cut"));
    }
    let mut g = CustomGeo { outline: vec![], holes: vec![], fills: vec![], hole_fills: vec![], marks: vec![], texts: vec![], w: hi[0] - lo[0], h: hi[1] - lo[1], contours, hand_order: vec![] };
    g.apply_states();
    Some((g, lo))
}

/// A part placed by its subprogram's call: its frame turned by `rot` degrees
/// (counter-clockwise) and put at `at` (BV15, BV17). `outline` is the part's
/// outline as stored (its box from 0, 0), `lo` where that box was in the
/// subprogram's frame.
pub fn placed_from_call(part: usize, outline: &[P], lo: P, at: P, rot: f64) -> Placed {
    let a = rot.rem_euclid(360.0);
    let (s, c) = a.to_radians().sin_cos();
    let turn = |p: P| [p[0] * c - p[1] * s, p[0] * s + p[1] * c];
    let (mut m, mut mx) = ([f64::MAX; 2], [f64::MIN; 2]);
    for q in outline.iter().map(|&q| turn(q)) {
        m = [m[0].min(q[0]), m[1].min(q[1])];
        mx = [mx[0].max(q[0]), mx[1].max(q[1])];
    }
    let rl = turn(lo);
    let (x, y) = (at[0] + rl[0] + m[0], at[1] + rl[1] + m[1]);
    let (w, h) = (mx[0] - m[0], mx[1] - m[1]);
    let q = (a / 90.0).round();
    if (a - q * 90.0).abs() < 1e-6 {
        return Placed { part, x, y, w, h, turns: (q as i64).rem_euclid(4) as u8, mirror: false, spin: Spin::NONE };
    }
    let turns = (a / 90.0).floor();
    Placed { part, x, y, w, h, turns: turns as u8 % 4, mirror: false, spin: Spin { deg: a - turns * 90.0, dx: -m[0], dy: -m[1] } }
}

/// Cut-offs as written (BV39: cut back out to the start, then from the same
/// pierce across) joined into one line from start to end.
fn join_cutoffs(cuts: Vec<Vec<P>>) -> Vec<Vec<P>> {
    let mut out: Vec<Vec<P>> = vec![];
    for c in cuts {
        if let Some(last) = out.last_mut() {
            if last.len() >= 2 && c.len() >= 2 && dist(last[0], c[0]) < CLOSE_TOL {
                let mut joined: Vec<P> = last.iter().rev().cloned().collect();
                joined.extend(c.into_iter().skip(1));
                *last = joined;
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Read a `.bvc` file's bytes.
pub fn read(bytes: &[u8]) -> Result<Bvc, String> {
    let files = crate::progimport::read_zip(bytes).map_err(|_| "it isn't a .bvc: not a zip archive".to_string())?;
    let base = |n: &str| n.rsplit(['/', '\\']).next().unwrap_or(n).to_string();
    let job = files.iter().find(|(n, _)| base(n).eq_ignore_ascii_case("JobInfo.xml")).ok_or("it has no job file (JobInfo.xml)")?;
    let job = job_info(&text_of(&job.1)).map_err(|e| format!("its job file can't be read: {e}"))?;
    let ccps: Vec<&(String, Vec<u8>)> = files.iter().filter(|(n, _)| n.to_lowercase().ends_with(".ccp")).collect();
    if ccps.is_empty() {
        return Err("it holds no cut program (.ccp)".into());
    }
    let mut b = Bvc { name: job.name.clone(), material: job.material.clone(), thickness: job.thickness, gas: job.gas.clone(), ..Default::default() };
    // (PartInfo Id or the subprogram's own name, its shape) → part.
    let mut part_of: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut qty_given = std::collections::BTreeSet::new();
    let mut unknown: BTreeMap<String, usize> = BTreeMap::new();
    let (mut full_circles, mut joints) = (0, 0);
    for (fname, data) in ccps {
        let stem = base(fname);
        let stem = stem.rsplit_once('.').map_or(stem.as_str(), |s| s.0).to_string();
        let p = program(&text_of(data)).map_err(|e| format!("its cut program {stem}.ccp can't be read: {e}"))?;
        for (k, n) in &p.unknown {
            *unknown.entry(k.clone()).or_default() += n;
        }
        full_circles += p.full_circles;
        joints += p.joints;
        if b.thickness <= 0.0 {
            b.thickness = p.thickness;
        }
        let plan = job.plans.iter().find(|pl| pl.0.eq_ignore_ascii_case(&stem));
        let mut sheet = BvcSheet { w: p.sheet.0, h: p.sheet.1, count: plan.map_or(1, |pl| pl.1), placed: vec![], cuts: vec![] };
        let mut cut_offs: Vec<Vec<P>> = p.main.iter().map(|c| c.pts.clone()).collect();
        // Each subprogram: a part (with its offset), or cut-offs (no part).
        let mut subs: BTreeMap<&str, Option<(usize, P)>> = BTreeMap::new();
        for s in &p.subs {
            // BV54: the vaporize-all copies are the same parts cut once more.
            if s.name.starts_with("Vaporization_") {
                subs.insert(&s.name, None);
                continue;
            }
            // The nest's own list of which subprogram is which part first; a
            // part number (BV16) otherwise. A nest the job file doesn't list
            // (BV2: with B017 only the last nest's job file is kept) keeps its
            // subprograms' own names and counts its parts.
            let local = plan.is_none() && !job.plans.is_empty();
            let id = if local { None } else { plan.and_then(|pl| pl.2.get(&s.name).copied()).or(s.part_number) };
            let named = id.map(|id| job.parts.get(&id).cloned().ok_or_else(|| format!("its subprogram {} names part {id}, which isn't in its job file", s.name))).transpose()?;
            if named.is_none() && (s.name.eq_ignore_ascii_case("ResidualSheetCuts") || s.cuts.iter().all(|c| !c.closed)) {
                subs.insert(&s.name, None);
                continue;
            }
            let label = named.as_ref().map_or_else(|| s.name.clone(), |n| n.0.clone());
            let Some((geo, lo)) = part_geo(&s.cuts, &mut b.notes, &label) else {
                b.notes.push(format!("subprogram {} has no closed cut and was left out", s.name));
                subs.insert(&s.name, None);
                continue;
            };
            // The same part in another nest is the same part only when it is
            // drawn and cut the same way; otherwise it opens as a part of its own.
            let key = (id.map_or_else(|| format!("sub:{stem}:{}", s.name), |i| format!("id:{i}")), shape_key(&geo));
            let k = match part_of.get(&key) {
                Some(&k) => k,
                None => {
                    let same_name = b.parts.iter().filter(|p| p.name == label || p.name.starts_with(&format!("{label} ("))).count();
                    let name = if same_name == 0 { label.clone() } else { format!("{label} ({})", same_name + 1) };
                    if same_name > 0 {
                        b.notes.push(format!("{label} is drawn or cut differently in nest {stem}: it opens as {name}"));
                    }
                    // The job file's quantity goes to the first part of its Id;
                    // a variant counts its placements.
                    let qty = if qty_given.insert(key.0.clone()) { named.as_ref().map_or(0, |n| n.1) } else { 0 };
                    b.parts.push(BvcPart { name, qty, geo });
                    part_of.insert(key, b.parts.len() - 1);
                    b.parts.len() - 1
                }
            };
            subs.insert(&s.name, Some((k, lo)));
        }
        for (name, x, y, rot) in p.calls.iter().filter(|c| !c.0.starts_with("Vaporization_")) {
            let target = subs.get(name.as_str()).ok_or_else(|| format!("its cut program {stem}.ccp calls subprogram {name}, which isn't in it"))?;
            match target {
                Some((k, lo)) => sheet.placed.push(placed_from_call(*k, &b.parts[*k].geo.outline, *lo, [*x, *y], *rot)),
                None => {
                    // A cut-off subprogram: its cuts in sheet coordinates.
                    let s = p.subs.iter().find(|s| &s.name == name).expect("listed");
                    let (sn, cs) = rot.to_radians().sin_cos();
                    for c in &s.cuts {
                        cut_offs.push(c.pts.iter().map(|q| [x + q[0] * cs - q[1] * sn, y + q[0] * sn + q[1] * cs]).collect());
                    }
                }
            }
        }
        sheet.cuts = join_cutoffs(cut_offs);
        b.sheets.push(sheet);
    }
    // Parts without a quantity in the job file: as many as are placed.
    for (k, part) in b.parts.iter_mut().enumerate() {
        if part.qty == 0 {
            part.qty = b.sheets.iter().map(|s| s.count * s.placed.iter().filter(|p| p.part == k).count()).sum::<usize>() as u32;
        }
    }
    if full_circles > 0 {
        b.notes.push(format!("{full_circles} full circle(s) written as one arc can't be rebuilt (the file gives no centre) and were left out"));
    }
    if joints > 0 {
        b.notes.push(format!("{joints} micro-joint marker(s) read as part of the cut"));
    }
    if !unknown.is_empty() {
        let n: usize = unknown.values().sum();
        b.notes.push(format!("{n} command(s) this reader doesn't know were skipped: {}", unknown.iter().map(|(k, v)| format!("{k} ×{v}")).collect::<Vec<_>>().join(", ")));
    }
    Ok(b)
}

/// Read a `.bvc` file.
pub fn open(path: &std::path::Path) -> Result<Bvc, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    read(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arcs_come_back_from_end_point_and_sweep() {
        // A half circle counter-clockwise from (10, 0) to (-10, 0): centre 0, 0.
        let pts = arc([10.0, 0.0], [-10.0, 0.0], 180.0).unwrap();
        assert!(pts.iter().all(|p| (dist(*p, [0.0, 0.0]) - 10.0).abs() < 1e-9));
        assert!(pts[pts.len() / 2][1] > 9.9, "over the top: {pts:?}");
        // The same clockwise goes under.
        let pts = arc([10.0, 0.0], [-10.0, 0.0], -180.0).unwrap();
        assert!(pts[pts.len() / 2][1] < -9.9);
        // A quarter, and a full circle that can't be rebuilt.
        let q = arc([10.0, 0.0], [0.0, 10.0], 90.0).unwrap();
        assert!(q.iter().all(|p| (dist(*p, [0.0, 0.0]) - 10.0).abs() < 1e-9));
        assert!(arc([10.0, 0.0], [10.0, 0.0], 360.0).is_none());
    }

    #[test]
    fn calls_place_the_part_by_position_and_rotation() {
        // A 100 × 50 box drawn from (5, 5) in its subprogram.
        let outline = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 50.0], [0.0, 50.0]];
        let p = placed_from_call(0, &outline, [5.0, 5.0], [200.0, 300.0], 90.0);
        assert_eq!((p.turns, p.spin.is_none()), (1, true));
        // The subprogram's point (5, 5) turned 90° lands at (195, 305).
        let corner = p.map([0.0, 0.0]);
        assert!(dist(corner, [195.0, 305.0]) < 1e-9, "{corner:?}");
        let p = placed_from_call(0, &outline, [0.0, 0.0], [0.0, 0.0], 30.0);
        let tip = p.map([100.0, 0.0]);
        let want = [100.0 * 30f64.to_radians().cos(), 100.0 * 30f64.to_radians().sin()];
        assert!(dist(tip, want) < 1e-9, "{tip:?}");
    }

    #[test]
    fn the_xml_reader_takes_attributes_and_entities() {
        let t = tags("<?xml version=\"1.0\"?><!-- c --><A B=\"x &amp; y\" C='2'/><D></D>").unwrap();
        assert_eq!(t.len(), 3);
        assert_eq!((t[0].get("B"), t[0].num("C"), t[0].empty), (Some("x & y"), Some(2.0), true));
        assert!(t[2].close);
        assert!(tags("<A B=\"x />").is_err());
    }
}
