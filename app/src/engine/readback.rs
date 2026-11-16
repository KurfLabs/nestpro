//! Reading an LC5 program set back before it goes out (BL39). The written text
//! is parsed on its own, with nothing shared with the writer, and every cut is
//! compared with the path it was planned to follow, one against the other in
//! both directions. A set that doesn't match is refused: an error in the writer
//! must never reach the machine looking like a finished program.

use crate::blm::BlmSet;
use crate::cnc::{Made, SheetPlan};
use crate::cutoff::{CutKind, CutLine};
use crate::nest::{Part, Placed, P};
use std::collections::HashMap;

/// How far a written cut may run from its planned path (mm): the arc fit's own
/// allowance (BL38) and the rounding of written coordinates.
pub fn tolerance() -> f64 {
    crate::blm::ARC_SPAN_TOL + 0.02
}
/// A move is split no finer than this while looking for where it leaves the
/// other path (mm), so what the check lets through is at most this much past
/// its tolerance.
const MIN_SPAN: f64 = 0.02;
/// How far a circle may flatten when an arc is rebuilt for the comparison (mm).
const ARC_SAMPLE_ERR: f64 = 0.002;
/// An arc's start and end must be this close to the same radius (mm).
const ARC_RADIUS_TOL: f64 = 0.02;
/// How close a written start must be to a planned one to be that part (mm).
const START_TOL: f64 = 0.01;

fn dist(a: P, b: P) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn inside<'a>(line: &'a str, head: &str) -> Option<&'a str> {
    line.strip_prefix(head)?.split(')').next()
}

fn numbers(list: &str, line: &str) -> Result<Vec<f64>, String> {
    list.split(',').map(|v| v.trim().parse::<f64>().map_err(|_| format!("can't read \"{line}\""))).collect()
}

/// The points along a written arc, without its start.
fn arc_path(from: P, to: P, i: f64, j: f64, ccw: bool) -> Result<Vec<P>, String> {
    let c = [from[0] + i, from[1] + j];
    let (r0, r1) = (dist(from, c), dist(to, c));
    if (r0 - r1).abs() > ARC_RADIUS_TOL || r0 <= 0.0 {
        return Err(format!("an arc to X{:.3} Y{:.3} doesn't end on its own circle ({r0:.3} and {r1:.3} mm)", to[0], to[1]));
    }
    let a0 = (from[1] - c[1]).atan2(from[0] - c[0]);
    let mut a1 = (to[1] - c[1]).atan2(to[0] - c[0]);
    let tau = std::f64::consts::TAU;
    if ccw && a1 <= a0 {
        a1 += tau;
    }
    if !ccw && a1 >= a0 {
        a1 -= tau;
    }
    let step = if r0 > ARC_SAMPLE_ERR { 2.0 * (1.0 - ARC_SAMPLE_ERR / r0).acos() } else { tau / 4.0 };
    let n = (((a1 - a0).abs() / step).ceil() as usize).clamp(1, 4000);
    let mut v: Vec<P> = (1..n)
        .map(|k| {
            let t = k as f64 / n as f64;
            let (r, a) = (r0 + (r1 - r0) * t, a0 + (a1 - a0) * t);
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        })
        .collect();
    v.push(to);
    Ok(v)
}

/// One cut: its path from the pierce on, and the compensation word it runs on.
#[derive(Clone, Debug, PartialEq)]
pub struct Cut {
    pub pts: Vec<P>,
    pub comp: Option<&'static str>,
}

/// The cuts of one written subprogram, in order.
pub fn written_paths(text: &str) -> Result<Vec<Cut>, String> {
    let mut out: Vec<Cut> = vec![];
    let mut cur: Option<P> = None;
    for raw in text.lines() {
        let l = raw.trim();
        if let Some(a) = inside(l, "NEXT_L(") {
            let v = numbers(a, l)?;
            if v.len() < 2 {
                return Err(format!("can't read \"{l}\""));
            }
            cur = Some([v[0], v[1]]);
            out.push(Cut { pts: vec![[v[0], v[1]]], comp: None });
        } else if let Some(a) = inside(l, "LASTLINE(") {
            let v = numbers(a, l)?;
            let (Some(from), Some(cut)) = (cur, out.last_mut()) else { return Err(format!("\"{l}\" before any pierce")) };
            if v.len() != 10 {
                return Err(format!("can't read \"{l}\""));
            }
            if dist(from, [v[4], v[5]]) > START_TOL {
                return Err(format!("\"{l}\" doesn't start where the cut before it ended"));
            }
            let to = [v[6], v[7]];
            if v[3] == 2.0 || v[3] == 3.0 {
                cut.pts.extend(arc_path(from, to, v[8], v[9], v[3] == 3.0)?);
            } else {
                cut.pts.push(to);
            }
            cur = Some(to);
        } else if l.starts_with("G01") || l.starts_with("G02") || l.starts_with("G03") {
            let (Some(from), Some(cut)) = (cur, out.last_mut()) else { return Err(format!("\"{l}\" before any pierce")) };
            let mut to = from;
            let (mut i, mut j) = (0.0, 0.0);
            for w in l.split_whitespace().skip(1) {
                match w {
                    "G41" => cut.comp = Some("G41"),
                    "G42" => cut.comp = Some("G42"),
                    _ => {}
                }
                let Some(field) = w.get(..1) else { continue };
                if matches!(field, "X" | "Y" | "I" | "J") {
                    let v: f64 = w[1..].parse().map_err(|_| format!("can't read \"{l}\""))?;
                    match field {
                        "X" => to[0] = v,
                        "Y" => to[1] = v,
                        "I" => i = v,
                        _ => j = v,
                    }
                }
            }
            if l.starts_with("G01") {
                cut.pts.push(to);
            } else {
                cut.pts.extend(arc_path(from, to, i, j, l.starts_with("G03"))?);
            }
            cur = Some(to);
        }
    }
    Ok(out)
}

/// A path's segments filed by place, so "is anything of it this close?" looks
/// at a few segments, however long the path.
struct Near<'a> {
    pts: &'a [P],
    cell: f64,
    cells: HashMap<(i64, i64), Vec<u32>>,
}

impl<'a> Near<'a> {
    fn new(pts: &'a [P], tol: f64) -> Near<'a> {
        let len: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
        let cell = (len / pts.len().max(1) as f64).clamp(1.0, 50.0).max(tol * 2.0);
        let mut cells: HashMap<(i64, i64), Vec<u32>> = HashMap::new();
        for (k, w) in pts.windows(2).enumerate() {
            let n = (dist(w[0], w[1]) / (cell / 2.0)).ceil() as usize;
            for s in 0..=n.max(1) {
                let t = s as f64 / n.max(1) as f64;
                let (cx, cy) = (((w[0][0] + (w[1][0] - w[0][0]) * t) / cell).floor() as i64, ((w[0][1] + (w[1][1] - w[0][1]) * t) / cell).floor() as i64);
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        let v = cells.entry((cx + dx, cy + dy)).or_default();
                        if v.last() != Some(&(k as u32)) {
                            v.push(k as u32);
                        }
                    }
                }
            }
        }
        Near { pts, cell, cells }
    }

    fn seg(&self, p: P, k: usize) -> f64 {
        let (a, b) = (self.pts[k], self.pts[k + 1]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = dx * dx + dy * dy;
        let t = if l == 0.0 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l).clamp(0.0, 1.0) };
        dist(p, [a[0] + t * dx, a[1] + t * dy])
    }

    fn within(&self, p: P, tol: f64) -> bool {
        let key = ((p[0] / self.cell).floor() as i64, (p[1] / self.cell).floor() as i64);
        self.cells.get(&key).is_some_and(|v| v.iter().any(|&k| self.seg(p, k as usize) <= tol))
    }

    /// Is one segment of the path close to both a and b? Then all of a–b is:
    /// the distance to a segment can't rise and fall back along a straight line.
    fn covers(&self, a: P, b: P, tol: f64) -> bool {
        let key = ((a[0] / self.cell).floor() as i64, (a[1] / self.cell).floor() as i64);
        self.cells.get(&key).is_some_and(|v| v.iter().any(|&k| self.seg(a, k as usize) <= tol && self.seg(b, k as usize) <= tol))
    }

    /// The first point of a–b that isn't within `tol`, and how far it is.
    fn stray(&self, a: P, b: P, tol: f64) -> Option<(P, f64)> {
        if self.covers(a, b, tol) {
            return None;
        }
        if dist(a, b) <= MIN_SPAN {
            return [a, b].into_iter().find(|&p| !self.within(p, tol)).map(|p| (p, self.distance(p)));
        }
        let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
        self.stray(a, mid, tol).or_else(|| self.stray(mid, b, tol))
    }

    /// The true distance, for the message when a point is out.
    fn distance(&self, p: P) -> f64 {
        (0..self.pts.len().saturating_sub(1)).map(|k| self.seg(p, k)).fold(f64::MAX, f64::min)
    }
}

/// The first point of `a` that is further than `tol` from `b`, and how far.
fn first_stray(a: &[P], b: &[P], tol: f64) -> Option<(P, f64)> {
    if b.len() < 2 {
        return a.first().map(|&p| (p, f64::MAX));
    }
    let near = Near::new(b, tol);
    if let [only] = a {
        return (!near.within(*only, tol)).then(|| (*only, near.distance(*only)));
    }
    a.windows(2).find_map(|w| near.stray(w[0], w[1], tol))
}

/// Twice the area a path encloses, the sign saying which way round it runs.
fn turned_area(p: &[P]) -> f64 {
    (0..p.len()).map(|i| p[i][0] * p[(i + 1) % p.len()][1] - p[(i + 1) % p.len()][0] * p[i][1]).sum()
}

/// The point a share `f` of the way along a path.
fn along(p: &[P], f: f64) -> P {
    let total: f64 = p.windows(2).map(|w| dist(w[0], w[1])).sum();
    let mut left = total * f;
    for w in p.windows(2) {
        let d = dist(w[0], w[1]);
        if left <= d && d > 0.0 {
            let t = left / d;
            return [w[0][0] + (w[1][0] - w[0][0]) * t, w[0][1] + (w[1][1] - w[0][1]) * t];
        }
        left -= d;
    }
    p[p.len() - 1]
}

/// Do two paths over the same ground visit it in another order? An arc is a
/// little longer than its chords, hence the slack. For paths whose enclosed area
/// can't say: a figure-eight encloses nothing either way round.
fn other_order(w: &[P], e: &[P]) -> bool {
    let len = |p: &[P]| p.windows(2).map(|s| dist(s[0], s[1])).sum::<f64>();
    let slack = 1.0 + 0.02 * len(w).max(len(e));
    (1..10).any(|k| dist(along(w, k as f64 / 10.0), along(e, k as f64 / 10.0)) > slack)
}

/// What's wrong with one written cut against its plan, in words.
fn compare(w: &Cut, e: &Cut, tol: f64) -> Option<String> {
    if let Some((p, d)) = first_stray(&w.pts, &e.pts, tol) {
        return Some(format!("the written cut runs {d:.2} mm from the planned one near X{:.1} Y{:.1}", p[0], p[1]));
    }
    if let Some((p, d)) = first_stray(&e.pts, &w.pts, tol) {
        return Some(format!("the planned cut is {d:.2} mm from anything written near X{:.1} Y{:.1}", p[0], p[1]));
    }
    if w.comp != e.comp {
        return Some(format!("it is written with compensation {} where {} is planned, so the part would come out a kerf off size", w.comp.unwrap_or("off"), e.comp.unwrap_or("off")));
    }
    // The same path the other way round cuts on the wrong side.
    let (aw, ae) = (turned_area(&w.pts), turned_area(&e.pts));
    // An area smaller than the tolerance times the length is just the difference
    // between arcs and their chords, and says nothing about direction.
    let length: f64 = e.pts.windows(2).map(|s| dist(s[0], s[1])).sum();
    let reversed = if aw.abs().min(ae.abs()) > tol * length { aw * ae < 0.0 } else { other_order(&w.pts, &e.pts) };
    let (lw, le) = (w.pts.last(), e.pts.last());
    if reversed || matches!((lw, le), (Some(&a), Some(&b)) if dist(a, b) > tol) {
        return Some("it is written running the other way round from the plan".into());
    }
    None
}

/// What a part should cut, absolute on the sheet, in the writer's order: its
/// marks and contours in its cut order (CO9; with no order set by hand, the
/// marks first), each contour from the pierce, closed. A path the writer has to
/// leave out (no length) isn't expected.
fn planned_paths(plan: &SheetPlan, part_no: usize) -> Vec<Cut> {
    let same = |a: &P, b: &P| (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6;
    let mut out = vec![];
    for made in plan.part_order(part_no) {
        let c = match made {
            Made::Mark(j) => {
                let mut p = plan.marks[j].1.clone();
                p.dedup_by(|a, b| same(a, b));
                if p.len() >= 2 {
                    out.push(Cut { pts: p, comp: None });
                }
                continue;
            }
            Made::Contour(k) if plan.contours[k].punch.is_none() => &plan.contours[k],
            Made::Contour(_) => continue,
        };
        let mut body = c.points.clone();
        if body.len() > 1 {
            body.push(c.points[0]);
        }
        body.dedup_by(|a, b| same(a, b));
        if body.len() < 2 {
            continue;
        }
        let mut p = if c.lead_path.len() >= 2 { c.lead_path.clone() } else { vec![] };
        p.extend(body);
        p.dedup_by(|a, b| same(a, b));
        out.push(Cut { pts: p, comp: c.comp.word().map(|(side, _)| side.word()) });
    }
    out
}

/// What the LC5 files can't carry yet (MAC-30): a part planned with these would
/// be cut differently from what the screen and the times show.
fn unsupported(parts: &[Part], sheet: &[Placed], plan: &SheetPlan) -> Vec<String> {
    let mut seen: Vec<(String, &str)> = vec![];
    for c in &plan.contours {
        let name = sheet.get(c.part_no.wrapping_sub(1)).and_then(|pl| parts.get(pl.part)).map_or("?", |p| p.name.as_str());
        for (has, what) in [(!c.joints.is_empty(), "micro-joints"), (!c.loops.is_empty(), "corner loops"), (c.punch.is_some(), "punched holes")] {
            if has && !seen.iter().any(|s| s.0 == name && s.1 == what) {
                seen.push((name.to_string(), what));
            }
        }
    }
    seen.into_iter().map(|(n, w)| format!("part {n} has {w}, which LC5 programs can't carry yet; take them off the part first")).collect()
}

/// The cut-off lines the writer puts in the set: both kinds, and only with some
/// length (a line from a point to itself has nothing to cut).
fn written_cut_lines(cuts: &[CutLine]) -> Vec<&CutLine> {
    let by = |kind| cuts.iter().filter(move |c| c.kind == kind && c.pts.iter().any(|q| dist(*q, c.pts[0]) > 1e-6));
    by(CutKind::Free).chain(by(CutKind::Remnant)).collect()
}

/// How far a written cut-off strays from its line (MAC-42: its ends run past the
/// sheet edge, so only the line it runs on is compared, not where it stops).
fn cut_line_stray(w: &[P], line: &CutLine, tol: f64) -> Option<f64> {
    if let [a, b] = line.pts[..] {
        let l = dist(a, b);
        let off = |p: &P| ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])).abs() / l;
        return w.iter().map(off).find(|d| *d > tol);
    }
    first_stray(w, &line.pts, tol).map(|h| h.1)
}

/// Everything wrong with a written set, in shop terms. Empty = it cuts what
/// was planned.
pub fn check_set(set: &BlmSet, parts: &[Part], sheet: &[Placed], plan: &SheetPlan, cuts: &[CutLine]) -> Vec<String> {
    let mut issues = unsupported(parts, sheet, plan);
    let tol = tolerance();
    let name_of = |i: usize| sheet.get(i).and_then(|pl| parts.get(pl.part)).map_or("?", |p| p.name.as_str());
    let main = set.spf.iter().find(|f| f.0.eq_ignore_ascii_case(&format!("{}.SPF", set.nest)));
    let Some((_, main)) = main else {
        issues.push("the set has no sheet program".into());
        return issues;
    };
    let mut planned: Vec<Option<Vec<Cut>>> = (0..sheet.len()).map(|i| Some(planned_paths(plan, i + 1)).filter(|p| !p.is_empty())).collect();
    let mut lines: Vec<Option<&CutLine>> = written_cut_lines(cuts).into_iter().map(Some).collect();
    let mut memo: HashMap<String, Vec<Cut>> = HashMap::new();
    let mut orig = ([0.0, 0.0], 0.0f64);
    for raw in main.lines() {
        let l = raw.trim();
        if let Some(a) = inside(l, "ORIG(") {
            match numbers(a, l) {
                Ok(v) if v.len() == 3 => orig = ([v[0], v[1]], v[2]),
                _ => issues.push(format!("can't read \"{l}\"")),
            }
            continue;
        }
        if !l.contains("CALL\"") {
            continue;
        }
        let stem = l.rsplit("_n_").next().unwrap_or("").trim_end_matches('"').trim_end_matches("_spf").to_uppercase();
        if !memo.contains_key(&stem) {
            let Some(file) = set.spf.iter().find(|f| f.0.to_uppercase() == format!("{stem}.SPF")) else {
                issues.push(format!("the sheet program calls {stem}, which isn't in the set"));
                continue;
            };
            match written_paths(&file.1) {
                Ok(p) => {
                    memo.insert(stem.clone(), p);
                }
                Err(e) => {
                    issues.push(format!("{stem}: {e}"));
                    memo.insert(stem.clone(), vec![]);
                    continue;
                }
            }
        }
        let (s, c) = orig.1.to_radians().sin_cos();
        let place = |p: &P| [orig.0[0] + p[0] * c - p[1] * s, orig.0[1] + p[0] * s + p[1] * c];
        let written: Vec<Cut> = memo[&stem].iter().map(|w| Cut { pts: w.pts.iter().map(place).collect(), comp: w.comp }).collect();
        let Some(first) = written.first().and_then(|w| w.pts.first()).copied() else { continue };
        // Which part this call is: one that starts there. Two parts can share
        // a start, so the one it fully matches wins.
        let fails = |want: &[Cut]| -> Option<String> {
            if want.len() != written.len() {
                return Some(format!("it is written with {} cuts, but {} are planned", written.len(), want.len()));
            }
            written.iter().zip(want).enumerate().find_map(|(k, (w, e))| compare(w, e, tol).map(|m| format!("cut {}: {m}", k + 1)))
        };
        let starts: Vec<usize> = (0..planned.len()).filter(|&i| planned[i].as_ref().is_some_and(|e| dist(e[0].pts[0], first) <= START_TOL)).collect();
        if let Some(&i) = starts.iter().find(|&&i| fails(planned[i].as_deref().unwrap_or(&[])).is_none()).or(starts.first()) {
            if let Some(m) = fails(planned[i].as_deref().unwrap_or(&[])) {
                issues.push(format!("part {} (sheet position {}): {m}", name_of(i), i + 1));
            }
            planned[i] = None;
            continue;
        }
        // Not a part: it must be a planned cut-off, placed at that line's start.
        let line = lines.iter().position(|e| e.is_some_and(|e| dist(e.pts[0], orig.0) <= START_TOL));
        let Some(k) = line else {
            issues.push(format!("the sheet program calls {stem} at X{:.1} Y{:.1}, which is no planned part or cut-off line", orig.0[0], orig.0[1]));
            continue;
        };
        if let Some(line) = lines[k].take() {
            let d = written.iter().find_map(|w| cut_line_stray(&w.pts, line, tol));
            if let Some(d) = d {
                issues.push(format!("the cut-off line from X{:.1} Y{:.1} is written {d:.2} mm off its planned line", orig.0[0], orig.0[1]));
            }
            // Compensation would move it off the line the numbers show.
            if written.iter().any(|w| w.comp.is_some()) {
                issues.push(format!("the cut-off line from X{:.1} Y{:.1} is written with compensation on", orig.0[0], orig.0[1]));
            }
        }
    }
    for (i, left) in planned.iter().enumerate() {
        if left.is_some() {
            issues.push(format!("part {} (sheet position {}) isn't in the sheet program", name_of(i), i + 1));
        }
    }
    for line in lines.iter().flatten() {
        issues.push(format!("the cut-off line from X{:.1} Y{:.1} isn't in the sheet program", line.pts[0][0], line.pts[0][1]));
    }
    issues
}

/// The refusal message for the operator.
pub fn refusal(sheet_no: usize, issues: &[String]) -> String {
    let shown: Vec<&str> = issues.iter().take(3).map(String::as_str).collect();
    let more = issues.len().saturating_sub(shown.len());
    let rest = if more > 0 { format!(" (and {more} more)") } else { String::new() };
    format!("The LC5 program for sheet {sheet_no} wouldn't cut what's planned, so nothing was exported: {}{rest}.", shown.join("; "))
}

// ---------------------------------------------------------------------------
// The ByStar Fiber laser's `.bvc` (BV59, BV60).
// ---------------------------------------------------------------------------

/// What one planned cut of a ByStar Fiber program should read back as: its
/// path from the pierce (lead-in, contour, overlap and lead-out), and the
/// commands around it the rules give (BV29–BV33), its joint markers at the
/// middles of its gaps, and every gap of its contour, which only a marked
/// pass may run through (BV63).
struct Want {
    cut: Cut,
    mode: i64,
    side: crate::bvc::Side,
    pierce: Option<String>,
    /// B038 off: a joint marker at each gap's middle, in order.
    markers: Vec<P>,
    /// Each gap's middle and how near a cut may pass it without cutting it.
    gaps: Vec<(P, f64)>,
}

/// Points around a circle of radius `r` about `c` from angle 0, ccw.
fn circle(c: P, r: f64) -> Vec<P> {
    (0..=72).map(|k| {
        let a = std::f64::consts::TAU * k as f64 / 72.0;
        [c[0] + r * a.cos(), c[1] + r * a.sin()]
    }).collect()
}

/// A part's planned cuts on the sheet, in its cut order (CO9), as the
/// ByStar Fiber rules say they are written.
fn bystar_wanted(plan: &SheetPlan, part_no: usize, post: &crate::bystar::Post) -> Vec<Want> {
    use crate::bvc::Side;
    let same = |a: &P, b: &P| (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6;
    let b036 = post.whole("B036");
    let mut out = vec![];
    for made in plan.part_order(part_no) {
        match made {
            Made::Mark(j) => {
                let mut p = plan.marks[j].1.clone();
                p.dedup_by(|a, b| same(a, b));
                // BV40, BV55: a point mark, a circle or a tiny cut.
                if let [c] = p[..] {
                    let dia = post.num("B013") * post.unit;
                    if post.num("B013") > post.tol {
                        let r = dia / 2.0;
                        let mut pts = vec![c];
                        pts.extend(circle(c, r));
                        let comp = (!post.on("B031")).then_some("Right");
                        out.push(Want { cut: Cut { pts, comp }, mode: if post.on("B033") { 2 } else { 3 }, side: Side::Unknown, pierce: post.pierce(0, true), markers: vec![], gaps: vec![] });
                    } else {
                        let pts = vec![c, [c[0] - 0.05 * post.unit, c[1]]];
                        out.push(Want { cut: Cut { pts, comp: None }, mode: 0, side: Side::Interior, pierce: Some("Fine".into()), markers: vec![], gaps: vec![] });
                    }
                    continue;
                }
                if p.len() < 2 {
                    continue;
                }
                // BV31, BV33, BV65: B036 engraving is uncompensated, Interior
                // and pierced Fine (2: Normal).
                if b036 != 0 {
                    let pierce = Some(if b036 == 2 { "Normal" } else { "Fine" }.to_string());
                    out.push(Want { cut: Cut { pts: p, comp: None }, mode: 2, side: Side::Interior, pierce, markers: vec![], gaps: vec![] });
                    continue;
                }
                let bracket = post.on("B024");
                let pierce = if bracket || post.on("B044") { post.pierce(0, true) } else { None };
                let comp = (!post.on("B031")).then_some("Right");
                out.push(Want { cut: Cut { pts: p, comp }, mode: 2, side: Side::Unknown, pierce, markers: vec![], gaps: vec![] });
            }
            Made::Contour(k) if plan.contours[k].punch.is_none() => {
                let c = &plan.contours[k];
                let mut body = c.points.clone();
                if body.len() > 1 {
                    body.push(c.points[0]);
                }
                body.dedup_by(|a, b| same(a, b));
                if body.len() < 2 {
                    continue;
                }
                let lead = c.lead_path.len() >= 2;
                let marking = post.is_marking(c.quality);
                let left = c.comp.word().is_some_and(|w| w.0 == crate::kerf::Side::Left);
                let new_marking = marking && b036 != 0;
                let comp = if new_marking || (marking && post.on("B031")) { None } else { Some(if left { "Left" } else { "Right" }) };
                let side = if new_marking { Side::Interior } else if marking { Side::Unknown } else if c.inner { Side::Interior } else { Side::Exterior };
                let pierce_of = |starts_with_lead: bool| {
                    if !(starts_with_lead || post.on("B024") || post.on("B044") || new_marking) {
                        None
                    } else if new_marking {
                        Some(if b036 == 2 { "Normal" } else { "Fine" }.to_string())
                    } else {
                        post.pierce(c.quality, marking)
                    }
                };
                // BV63: each gap's middle; a cut may come no nearer than a
                // quarter of the gap without cutting it.
                let gaps: Vec<(P, f64)> = c.joints.iter().map(|j| (crate::cnc::point_at(&c.points, (j.0 + j.1) / 2.0), ((j.1 - j.0) / 4.0).min(tolerance()))).collect();
                let mode = post.mode(c.quality);
                if !c.joints.is_empty() && post.on("B038") {
                    // B038: a cut-out per beam run, each later one started
                    // like a contour without a lead-in (BV48).
                    for (n, run) in crate::cnc::beam_runs(c).into_iter().enumerate() {
                        let mut run = run;
                        run.dedup_by(|a, b| same(a, b));
                        out.push(Want { cut: Cut { pts: run, comp }, mode, side, pierce: pierce_of(n == 0 && lead), markers: vec![], gaps: gaps.clone() });
                    }
                    continue;
                }
                let mut p = if lead { c.lead_path.clone() } else { vec![] };
                p.extend(body);
                if c.overlap > 0.0 {
                    p.push(crate::cnc::point_at(&c.points, c.overlap));
                }
                p.extend(c.lead_out_path.iter().skip(1).copied());
                p.dedup_by(|a, b| same(a, b));
                let mut order: Vec<&(f64, f64)> = c.joints.iter().collect();
                order.sort_by(|a, b| a.0.total_cmp(&b.0));
                let markers = order.iter().map(|j| crate::cnc::point_at(&c.points, (j.0 + j.1) / 2.0)).collect();
                out.push(Want { cut: Cut { pts: p, comp }, mode, side, pierce: pierce_of(lead), markers, gaps });
            }
            Made::Contour(_) => {}
        }
    }
    out
}

/// How many times a path passes within `r` of `g`. Two moves that both
/// come near are one pass only when the point they share is near too: a cut
/// that leaves the gap and comes back through it passes it again (S1).
fn passes(path: &[P], g: P, r: f64) -> usize {
    let near = |a: P, b: P| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = dx * dx + dy * dy;
        let t = if l == 0.0 { 0.0 } else { (((g[0] - a[0]) * dx + (g[1] - a[1]) * dy) / l).clamp(0.0, 1.0) };
        dist(g, [a[0] + t * dx, a[1] + t * dy]) <= r
    };
    let mut n = 0;
    let mut was = false;
    for w in path.windows(2) {
        let now = near(w[0], w[1]);
        if now && !(was && dist(w[0], g) <= r) {
            n += 1;
        }
        was = now;
    }
    if path.len() == 1 && dist(path[0], g) <= r {
        n = 1;
    }
    n
}

/// A cut as read from the `.bvc`, on the sheet: its whole path from where it
/// starts (the pierce, or the cut-out's start), leads included.
fn bystar_read(c: &crate::bvc::Cut, place: &dyn Fn(P) -> P) -> (Cut, Option<i64>) {
    use crate::bvc::Correction;
    let mut pts: Vec<P> = vec![];
    for path in [c.lead_in.as_ref(), Some(&c.path), c.lead_out.as_ref()].into_iter().flatten() {
        for q in path.points() {
            let q = place(q);
            if pts.last().is_none_or(|l: &P| dist(*l, q) > 1e-6) {
                pts.push(q);
            }
        }
    }
    let comp = match c.correction {
        Correction::Left => Some("Left"),
        Correction::Right => Some("Right"),
        Correction::None => None,
    };
    (Cut { pts, comp }, c.mode)
}

/// One sheet of a ByStar Fiber write, for the read-back.
pub struct BystarSheet<'a> {
    pub sheet_no: usize,
    pub parts: &'a [Part],
    pub sheet: &'a [Placed],
    pub spec: &'a crate::nest::SheetSpec,
    pub plan: &'a SheetPlan,
    /// Its remnant and free cut lines (BV64).
    pub cuts: &'a [CutLine],
}

/// The first point of a cut (placed with `place`) the head runs off the
/// sheet with the height sensor on (BV37, BV64): `entry` is the sensor as
/// the subprogram had it before the cut.
fn sensor_off_sheet(c: &crate::bvc::Cut, place: &dyn Fn(P) -> P, entry: bool, w: f64, h: f64, tol: f64) -> Option<P> {
    let off = |q: P| q[0] < -tol || q[1] < -tol || q[0] > w + tol || q[1] > h + tol;
    let mut on = c.sensor.unwrap_or(entry);
    if let Some(l) = &c.lead_in {
        if let Some(q) = l.points().into_iter().map(place).find(|q| on && off(*q)) {
            return Some(q);
        }
    }
    let mut e = 0;
    let mut pts = c.path.points();
    if let Some(l) = &c.lead_out {
        pts.extend(l.points().into_iter().skip(1));
    }
    for q in pts {
        if on && off(place(q)) {
            return Some(place(q));
        }
        while e < c.sensing.len() && dist(c.sensing[e].0, q) < 1e-6 {
            on = c.sensing[e].1;
            e += 1;
        }
    }
    None
}

/// BV64: what's wrong with the written cut-off subprogram (its cuts on the
/// sheet) against the sheet's planned cut lines: every written cut lies on a
/// planned line or runs on straight past its end by at most B003; every line
/// is cut end to end, and past each end on the sheet's edge by B003; its
/// first cut is pierced (unless it starts inside the sheet without B025),
/// every further one pierced no-time where the first was; every cut runs
/// Exterior, compensated Right, in the cut-off mode, and ends with a head lift.
fn cutoff_issues(label: &str, written: &[(Cut, &crate::bvc::Cut)], place: &dyn Fn(P) -> P, lines: &[&CutLine], spec: &crate::nest::SheetSpec, post: &crate::bystar::Post) -> Vec<String> {
    use crate::bvc::{Correction, Piercing, Side};
    let tol = tolerance();
    let mut issues = vec![];
    let b003 = post.whole("B003").max(0) as f64 * post.unit;
    let near_edge = |q: P, by: f64| q[0] <= by || q[1] <= by || q[0] >= spec.w - by || q[1] >= spec.h - by;
    let q = if post.num("B045") > 0.5 { post.num("B045").trunc() as usize } else { 1 };
    let mode = post.mode(q);
    // Each line with its straight run-ons past both ends.
    let ext = |a: P, b: P| {
        let l = dist(a, b);
        [b[0] + (b[0] - a[0]) / l * b003, b[1] + (b[1] - a[1]) / l * b003]
    };
    let mut allowed: Vec<(Vec<P>, Vec<P>)> = vec![];
    for l in lines {
        let mut p = l.pts.clone();
        p.dedup_by(|a, b| dist(*a, *b) < 1e-9);
        let n = p.len();
        let (s, e) = (ext(p[1], p[0]), ext(p[n - 2], p[n - 1]));
        // What must be cut: the line, and past each end on an edge.
        let mut must = p.clone();
        if near_edge(p[0], tol) {
            must.insert(0, s);
        }
        if near_edge(p[n - 1], tol) {
            must.push(e);
        }
        let mut may = p;
        may.insert(0, s);
        may.push(e);
        allowed.push((may, must));
    }
    let mut mine: Vec<Vec<usize>> = vec![vec![]; lines.len()];
    for (k, (w, _)) in written.iter().enumerate() {
        match allowed.iter().position(|(may, _)| first_stray(&w.pts, may, tol).is_none()) {
            Some(i) => mine[i].push(k),
            None => issues.push(format!("{label}: a cut-off cut from X{:.1} Y{:.1} runs off every planned cut-off line", w.pts[0][0], w.pts[0][1])),
        }
    }
    for (i, l) in lines.iter().enumerate() {
        let at = format!("{label}: the cut-off line from X{:.1} Y{:.1}", l.pts[0][0], l.pts[0][1]);
        let cuts: Vec<&(Cut, &crate::bvc::Cut)> = mine[i].iter().map(|&k| &written[k]).collect();
        if cuts.is_empty() {
            issues.push(format!("{at} isn't in the cut program"));
            continue;
        }
        // Cut end to end, and past each end on an edge.
        let must = &allowed[i].1;
        let total: f64 = must.windows(2).map(|w| dist(w[0], w[1])).sum();
        let n = (total / 0.5).ceil().max(1.0) as usize;
        let nears: Vec<Near> = cuts.iter().filter(|c| c.0.pts.len() >= 2).map(|c| Near::new(&c.0.pts, tol)).collect();
        if let Some(gap) = (0..=n).map(|k| along(must, k as f64 / n as f64)).find(|q| !nears.iter().any(|nr| nr.within(*q, tol))) {
            issues.push(format!("{at} isn't cut near X{:.1} Y{:.1}: it would miss the sheet's edge or leave part of the line uncut", gap[0], gap[1]));
        }
        let d = (post.whole("B004") + post.whole("B005")).max(0) as f64 * post.unit;
        let starts_inside = !near_edge(l.pts[0], d + post.tol);
        // S2, BV38, BV49: where every pierce of this line must be, worked out
        // here on its own: its start when it starts inside; else where it
        // first comes B004 + B005 inside the sheet, or a quarter of the way
        // along when that isn't in its first half.
        let mut line = l.pts.clone();
        line.dedup_by(|a, b| dist(*a, *b) < 1e-9);
        let len: f64 = line.windows(2).map(|w| dist(w[0], w[1])).sum();
        let pierce_at = if starts_inside {
            line[0]
        } else {
            let (lo, hi) = ([d, d], [spec.w - d, spec.h - d]);
            let mut gone = 0.0;
            let mut s_in: Option<f64> = None;
            for w in line.windows(2) {
                let (mut t0, mut t1) = (0.0f64, 1.0f64);
                for k in 0..2 {
                    let dk = w[1][k] - w[0][k];
                    if dk.abs() < 1e-12 {
                        if w[0][k] < lo[k] - 1e-9 || w[0][k] > hi[k] + 1e-9 {
                            t0 = 2.0;
                        }
                    } else {
                        let (u, v) = ((lo[k] - w[0][k]) / dk, (hi[k] - w[0][k]) / dk);
                        t0 = t0.max(u.min(v));
                        t1 = t1.min(u.max(v));
                    }
                }
                if t0 <= t1 + 1e-12 {
                    s_in = Some(gone + t0 * dist(w[0], w[1]));
                    break;
                }
                gone += dist(w[0], w[1]);
            }
            let s = match s_in {
                Some(s) if s <= len / 2.0 => s,
                _ => len / 4.0,
            };
            along(&line, if len > 0.0 { s / len } else { 0.0 })
        };
        let first_pierce = post.pierce(q, false);
        for (k, (_, raw)) in cuts.iter().enumerate() {
            // BV33: no first pierce for a line starting inside without B025,
            // nor when the table's pierce is empty (S5); a re-pierce always.
            let want = if k > 0 { 1 } else if (starts_inside && !post.on("B025")) || first_pierce.is_none() { 0 } else { 1 };
            if raw.pierces.len() != want {
                issues.push(format!("{at} is pierced {} time(s) in its cut {} where {want} is planned", raw.pierces.len(), k + 1));
                continue;
            }
            for (p0, _) in &raw.pierces {
                let p0 = place(*p0);
                if dist(p0, pierce_at) > tol {
                    issues.push(format!("{at} is pierced at X{:.1} Y{:.1} where BV38 pierces it at X{:.1} Y{:.1}", p0[0], p0[1], pierce_at[0], pierce_at[1]));
                }
            }
            if want == 1 {
                let how = raw.pierce.map(|p| p.1);
                let ok = if k == 0 { how.map(|h| format!("{h:?}")) == first_pierce } else { how == Some(Piercing::NoTime) };
                if !ok {
                    issues.push(format!("{at}: its cut {} is pierced {:?} where {} is planned", k + 1, how, if k == 0 { "the cut-off's pierce (BV33)" } else { "a no-time pierce (BV38)" }));
                }
            }
            if raw.mode != Some(mode) || raw.modes.iter().any(|m| *m != Some(mode)) {
                issues.push(format!("{at} is cut in operation mode {:?} where {mode} is planned", raw.mode));
            }
            if raw.correction != Correction::Right || raw.corrections.iter().any(|c| *c != Correction::Right) {
                issues.push(format!("{at} is cut with compensation {:?} where Right is planned (BV31)", raw.correction));
            }
            if raw.side != Side::Exterior {
                issues.push(format!("{at} is written {:?} where Exterior is planned", raw.side));
            }
            if !raw.lift {
                issues.push(format!("{at}: its cut {} isn't followed by a head lift", k + 1));
            }
            if !raw.joints.is_empty() {
                issues.push(format!("{at} has micro-joint(s) the plan doesn't have"));
            }
        }
    }
    issues
}

/// BV59, BV60: read every `.bvc` of a write back with the reader (`bvc.rs`,
/// which shares nothing with the writer) and compare it with the plan: each
/// nest's sheet, run count and thickness, and every part's every cut with
/// its planned path both ways, the compensation and operation mode in force
/// while it runs, its cut-out side and pierce; each planned sheet once (every
/// sheet of a folded nest compared); the height sensor through the calls
/// (BV37); no command the plan doesn't have. `files` are the `.bvc` files as they will be written
/// (name, bytes, the nests in it); `nests` are (program name, the sheets it
/// cuts). Empty = they cut the plan.
pub fn check_bvc(files: &[(String, Vec<u8>, Vec<usize>)], nests: &[(String, Vec<usize>)], sheets: &[BystarSheet], thickness: f64, post: &crate::bystar::Post) -> Vec<String> {
    use crate::bvc::Piercing;
    let tol = tolerance();
    let mut issues = vec![];
    // Every planned sheet is cut by exactly one nest (its run count is
    // checked against the sheets that nest stands for, each compared below).
    let mut seen = vec![0usize; sheets.len()];
    for (_, _, in_it) in files {
        for &n in in_it {
            for &k in nests.get(n).map_or(&[][..], |x| x.1.as_slice()) {
                if let Some(c) = seen.get_mut(k) {
                    *c += 1;
                }
            }
        }
    }
    for (k, c) in seen.iter().enumerate() {
        if *c != 1 {
            issues.push(format!("sheet {} is in {c} nest(s) of the files, not one", sheets[k].sheet_no));
        }
    }
    for (file, bytes, in_it) in files {
        let read = match crate::bvc::read(bytes) {
            Ok(r) => r,
            Err(e) => {
                issues.push(format!("{file} doesn't read back: {e}"));
                continue;
            }
        };
        if (read.job.thickness - thickness).abs() > 0.001 {
            issues.push(format!("{file}: its job file says the sheets are {} mm thick, but they are {thickness} mm", read.job.thickness));
        }
        for &n in in_it {
            let (name, ks) = &nests[n];
            let Some(nest) = read.nests.iter().find(|x| x.plan.name == *name) else {
                issues.push(format!("{file}: nest {name} isn't in its job file"));
                continue;
            };
            if nest.plan.multiplicity != ks.len() as i64 {
                issues.push(format!("nest {name} is run {} time(s), but {} identical sheet(s) are planned", nest.plan.multiplicity, ks.len()));
            }
            let p = &nest.program;
            // Commands the reader skipped or passed over: none is planned
            // (the main program's clamps aside).
            for (what, m) in [("unknown", &p.unknown), ("not read", &p.unread), ("not planned", &p.passed)] {
                for (c, count) in m {
                    issues.push(format!("nest {name}: {count} {c} command(s) ({what})"));
                }
            }
            // The main program sets up the clamps once (BV23), nothing else.
            for (c, count) in p.main_passed.iter().filter(|(c, n)| !(c.as_str() == "SetClamps Value=Off" && **n == 1)) {
                issues.push(format!("nest {name}: {count} {c} command(s) in the main program (not planned)"));
            }
            if p.stray_pierces > 0 {
                issues.push(format!("nest {name}: {} pierce(s) no cut follows", p.stray_pierces));
            }
            // BV64: a head lift only right after a cut-off cut (each checked
            // with its cut); any other is one the plan doesn't have.
            let kept: usize = p.subs.iter().map(|s| s.cuts.iter().filter(|c| c.lift).count()).sum();
            if p.lifts != kept {
                issues.push(format!("nest {name}: {} head lift(s) (ExecHighPositioning) the plan doesn't have", p.lifts.abs_diff(kept)));
            }
            // BV59: each sheet the nest stands for is compared with it.
            for &k in ks {
                let Some(s) = sheets.get(k) else { continue };
                let label = format!("sheet {}", s.sheet_no);
                if (p.sheet[0] - s.spec.w).abs() > 0.001 || (p.sheet[1] - s.spec.h).abs() > 0.001 {
                    issues.push(format!("{label}: the program's sheet reads {:.3} × {:.3} mm, the plan's is {} × {} mm", p.sheet[0], p.sheet[1], s.spec.w, s.spec.h));
                }
                if (p.thickness - thickness).abs() > 0.001 {
                    issues.push(format!("{label}: the program's thickness reads {:.3} mm, the sheet is {thickness} mm", p.thickness));
                }
                let name_of = |i: usize| s.sheet.get(i).and_then(|pl| s.parts.get(pl.part)).map_or("?", |p| p.name.as_str());
                let mut planned: Vec<Option<Vec<Want>>> = (0..s.sheet.len()).map(|i| Some(bystar_wanted(s.plan, i + 1, post)).filter(|w| !w.is_empty())).collect();
                // BV37: the height sensor as the calls leave it, from off.
                let mut sensor = false;
                let lines = crate::bystar::cut_lines_in_order(s.cuts);
                let mut lines_done = lines.is_empty();
                for call in &p.calls {
                    let sub = &p.subs[call.sub];
                    let (sn, cs) = call.rotation.to_radians().sin_cos();
                    let place = move |q: P| [call.x + q[0] * cs - q[1] * sn, call.y + q[0] * sn + q[1] * cs];
                    let entry = sensor;
                    sensor = sub.sensor_end.unwrap_or(entry);
                    let mode = post.sensor_mode();
                    for c in sub.cuts.iter().filter(|_| mode != 2) {
                        // At each pierce (S9: every one), else where the cut-out starts.
                        let start = (c.lead_in.as_ref().map_or(c.path.start, |l| l.start), c.sensor);
                        let at_each: Vec<(P, Option<bool>)> = if c.pierces.is_empty() { vec![start] } else { c.pierces.clone() };
                        for (q, state) in at_each {
                            let on = state.unwrap_or(entry);
                            let at = place(q);
                            let inside = at[0] > post.tol && at[1] > post.tol && at[0] < s.spec.w - post.tol && at[1] < s.spec.h - post.tol;
                            let exempt = mode == 1 && c.mode == Some(2);
                            // BV38: a cut-off's no-time re-pierce comes after the
                            // sequence's own `SetSensingSensor On`, on the start
                            // edge when B004 + B005 is 0 (S4); the cut-off check
                            // keeps the sensor off past the sheet.
                            let re_pierce = c.lift && c.pierce.is_some_and(|p| p.1 == Piercing::NoTime) && on && !inside;
                            if inside != on && !(exempt && inside) && !re_pierce {
                                issues.push(format!("{label}: the height sensor is {} at the pierce at X{:.1} Y{:.1}, {} the sheet (BV37)", if on { "on" } else { "off" }, at[0], at[1], if inside { "inside" } else { "on the edge of" }));
                            }
                        }
                    }
                    // BV64: never on while the head is off the sheet.
                    let mut on_in = entry;
                    for c in &sub.cuts {
                        if let Some(q) = sensor_off_sheet(c, &place, on_in, s.spec.w, s.spec.h, post.tol) {
                            issues.push(format!("{label}: the height sensor is on off the sheet at X{:.1} Y{:.1}", q[0], q[1]));
                        }
                        on_in = c.sensing.last().map_or(c.sensor.unwrap_or(on_in), |e| e.1);
                    }
                    let written: Vec<(Cut, &crate::bvc::Cut)> = sub.cuts.iter().map(|c| (bystar_read(c, &place).0, c)).collect();
                    let Some(first) = written.first().and_then(|w| w.0.pts.first()).copied() else {
                        issues.push(format!("{label}: subprogram {} cuts nothing", sub.name));
                        continue;
                    };
                    let fails = |want: &[Want]| -> Option<String> {
                        if want.len() != written.len() {
                            return Some(format!("it is written with {} cuts, but {} are planned", written.len(), want.len()));
                        }
                        written.iter().zip(want).enumerate().find_map(|(k, ((w, raw), e))| {
                            let m = compare(w, &e.cut, tol).or_else(|| {
                                if raw.mode != Some(e.mode) {
                                    return Some(format!("it is written in operation mode {} where {} is planned", raw.mode.map_or("none".into(), |m| m.to_string()), e.mode));
                                }
                                // S3: what is in force while the cut-out's moves run.
                                let want = match e.cut.comp {
                                    Some("Left") => crate::bvc::Correction::Left,
                                    Some("Right") => crate::bvc::Correction::Right,
                                    _ => crate::bvc::Correction::None,
                                };
                                if let Some(c) = raw.corrections.iter().find(|c| **c != want) {
                                    return Some(format!("its cut-out runs with compensation {c:?} where {want:?} is planned"));
                                }
                                if let Some(m) = raw.modes.iter().find(|m| **m != Some(e.mode)) {
                                    return Some(format!("its cut-out runs in operation mode {} where {} is planned", m.map_or("none".into(), |m| m.to_string()), e.mode));
                                }
                                // BV63: a joint marker at each gap's middle, in order.
                                let marks: Vec<P> = raw.joints.iter().map(|q| place(*q)).collect();
                                if marks.len() != e.markers.len() {
                                    return Some(format!("it has {} micro-joint marker(s) where {} are planned, so a joint gap would be cut through or one made that isn't planned", marks.len(), e.markers.len()));
                                }
                                if let Some((m, _)) = marks.iter().zip(&e.markers).find(|(m, x)| dist(**m, **x) > tol) {
                                    return Some(format!("its micro-joint marker at X{:.1} Y{:.1} isn't at a planned gap's middle", m[0], m[1]));
                                }
                                // Each gap is passed once, at its marker (where the
                                // written path is), or never with B038.
                                let through = if e.markers.is_empty() { 0 } else { 1 };
                                let at = |g: P| marks.iter().copied().find(|m| dist(*m, g) <= tol).unwrap_or(g);
                                if let Some((g, _)) = e.gaps.iter().find(|(g, r)| passes(&w.pts, at(*g), *r) > through) {
                                    return Some(format!("it cuts through the micro-joint gap at X{:.1} Y{:.1}", g[0], g[1]));
                                }
                                if raw.lift {
                                    return Some("it lifts the head where the plan doesn't".into());
                                }
                                if raw.side != e.side {
                                    return Some(format!("its cut-out is written {:?} where {:?} is planned", raw.side, e.side));
                                }
                                let got = raw.pierce.map(|p| match p.1 {
                                    Piercing::Normal => "Normal",
                                    Piercing::Fine => "Fine",
                                    Piercing::Drilled => "Drilled",
                                    Piercing::NoTime => "NoTime",
                                });
                                if got != e.pierce.as_deref() {
                                    return Some(format!("it is pierced {} where {} is planned", got.unwrap_or("not at all"), e.pierce.as_deref().unwrap_or("no pierce")));
                                }
                                // S9: as many pierces as planned, each where planned.
                                let planned_n = usize::from(e.pierce.is_some());
                                if raw.pierces.len() != planned_n {
                                    return Some(format!("it is pierced {} time(s) where {planned_n} is planned", raw.pierces.len()));
                                }
                                if raw.pierces.iter().any(|(at, _)| dist(place(*at), e.cut.pts[0]) > tol) {
                                    return Some("it is pierced away from its planned pierce point".into());
                                }
                                None
                            });
                            m.map(|m| format!("cut {}: {m}", k + 1))
                        })
                    };
                    let starts: Vec<usize> = (0..planned.len()).filter(|&i| planned[i].as_ref().is_some_and(|e| dist(e[0].cut.pts[0], first) <= START_TOL)).collect();
                    let Some(&i) = starts.iter().find(|&&i| fails(planned[i].as_deref().unwrap_or(&[])).is_none()).or(starts.first()) else {
                        // BV64: the sheet's cut-off subprogram, once.
                        if !lines_done {
                            lines_done = true;
                            issues.extend(cutoff_issues(&label, &written, &place, &lines, s.spec, post));
                            continue;
                        }
                        issues.push(format!("{label}: subprogram {} is placed at X{:.1} Y{:.1}, where no part is planned", sub.name, call.x, call.y));
                        continue;
                    };
                    if let Some(m) = fails(planned[i].as_deref().unwrap_or(&[])) {
                        issues.push(format!("{label}, part {} (sheet position {}): {m}", name_of(i), i + 1));
                    }
                    // The job file must name the part this subprogram cuts.
                    let part_id = nest.plan.parts.iter().find(|pp| pp.sub.eq_ignore_ascii_case(&sub.name)).map(|pp| pp.part_id);
                    let named = part_id.and_then(|id| read.job.parts.iter().find(|q| q.id == id)).map(|q| q.name.as_str());
                    if named != Some(name_of(i)) {
                        issues.push(format!("{label}: the job file says subprogram {} cuts part {}, but it cuts {}", sub.name, named.unwrap_or("(none)"), name_of(i)));
                    }
                    planned[i] = None;
                }
                for (i, left) in planned.iter().enumerate() {
                    if left.is_some() {
                        issues.push(format!("{label}, part {} (sheet position {}) isn't in the cut program", name_of(i), i + 1));
                    }
                }
                if !lines_done {
                    issues.push(format!("{label}: its remnant or cut-off lines aren't in the cut program"));
                }
            }
        }
    }
    issues
}

/// The refusal for the operator when a ByStar Fiber `.bvc` doesn't read back
/// as planned (BV59) or a sheet has what the writer can't write yet (BV60).
pub fn bystar_refusal(issues: &[String]) -> String {
    let shown: Vec<&str> = issues.iter().take(3).map(String::as_str).collect();
    let more = issues.len().saturating_sub(shown.len());
    let rest = if more > 0 { format!(" (and {more} more)") } else { String::new() };
    format!("The ByStar Fiber laser's .bvc wouldn't cut what's planned, so nothing was exported: {}{rest}.", shown.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nest::{Part, Placed, Shape, SheetSpec};

    fn spec() -> SheetSpec {
        SheetSpec { w: 1000.0, h: 500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: None, pick: Default::default() }
    }

    fn job() -> (Vec<Part>, Vec<Placed>, SheetPlan, BlmSet) {
        let parts = vec![Part { name: "DEMO-001".into(), shape: Shape::Disc, w: 200.0, h: 200.0, qty: 2, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let placed = vec![Placed { part: 0, x: 10.0, y: 10.0, w: 200.0, h: 200.0, turns: 0, mirror: false, spin: Default::default() }, Placed { part: 0, x: 300.0, y: 10.0, w: 200.0, h: 200.0, turns: 0, mirror: false, spin: Default::default() }];
        let plan = crate::cnc::plan(&parts, &placed, &spec());
        let set = crate::blm::sheet_set(1, &parts, &placed, &spec(), &plan, &[], &crate::blm::BlmSettings::for_spec(&spec(), "LC5", "J"));
        (parts, placed, plan, set)
    }

    fn edit(set: &mut BlmSet, sub: bool, f: impl Fn(&str) -> String) {
        let file = set.spf.iter_mut().find(|x| x.0.contains("70") == sub && !x.0.contains("LIS")).unwrap();
        file.1 = f(&file.1);
    }

    #[test]
    fn a_set_that_cuts_the_plan_passes() {
        let (parts, placed, plan, set) = job();
        assert_eq!(check_set(&set, &parts, &placed, &plan, &[]), Vec::<String>::new());
    }

    #[test]
    fn a_bent_edge_a_moved_part_or_a_lost_cut_are_all_found() {
        let (parts, placed, plan, mut bad) = job();
        // BL39: an arc swapped the other way round bulges off the part.
        edit(&mut bad, true, |t| if t.contains("G03") { t.replacen("G03", "G02", 1) } else { t.replacen("G02", "G03", 1) });
        let out = check_set(&bad, &parts, &placed, &plan, &[]);
        assert!(out.iter().any(|m| m.contains("DEMO-001") && m.contains(" mm from ")), "{out:?}");

        // A placement shifted 1 mm is no longer that part.
        let (_, _, _, mut moved) = job();
        edit(&mut moved, false, |t| t.replacen("ORIG(", "ORIG(1", 1));
        let out = check_set(&moved, &parts, &placed, &plan, &[]);
        assert!(!out.is_empty(), "a moved part must not pass");

        // A sheet program that lost a part's call.
        let (_, _, _, mut lost) = job();
        edit(&mut lost, false, |t| t.lines().filter(|l| !l.starts_with("N2 CALL")).collect::<Vec<_>>().join("\r\n"));
        let out = check_set(&lost, &parts, &placed, &plan, &[]);
        assert!(out.iter().any(|m| m.contains("isn't in the sheet program")), "{out:?}");
    }

    #[test]
    fn a_small_shift_inside_the_part_is_found_though_another_cut_runs_nearby() {
        // One cut is compared with its own plan: a copy of the outline 1 mm off
        // doesn't pass because some other path on the sheet is close.
        let square = |x: f64| vec![[x, 0.0], [x + 100.0, 0.0], [x + 100.0, 100.0], [x, 100.0], [x, 0.0]];
        assert!(first_stray(&square(1.0), &square(0.0), tolerance()).is_some());
        assert!(first_stray(&square(0.05), &square(0.0), tolerance()).is_none());
    }

    #[test]
    fn a_missing_or_extra_short_move_is_found() {
        // Corners alone would match: all four lie on both paths.
        let sq = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]];
        let open = &sq[..4];
        assert!(first_stray(&sq, open, tolerance()).is_some(), "the closing move is missing");
        let cross = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 0.0]];
        assert!(first_stray(&cross, &sq, tolerance()).is_some(), "a diagonal across the square isn't planned");
    }

    #[test]
    fn a_long_straight_cut_over_many_small_ones_passes_quickly() {
        let fine: Vec<P> = (0..=30000).map(|k| [k as f64 * 0.1, 0.0]).collect();
        assert!(first_stray(&[[0.0, 0.0], [3000.0, 0.0]], &fine, tolerance()).is_none());
        assert!(first_stray(&fine, &[[0.0, 0.0], [3000.0, 0.0]], tolerance()).is_none());
    }

    #[test]
    fn the_wrong_compensation_or_direction_is_found() {
        let sq = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 50.0], [0.0, 50.0], [0.0, 0.0]];
        let e = Cut { pts: sq.clone(), comp: Some("G41") };
        assert_eq!(compare(&e.clone(), &e, tolerance()), None);
        assert!(compare(&Cut { pts: sq.clone(), comp: Some("G42") }, &e, tolerance()).unwrap().contains("compensation"));
        let back: Vec<P> = sq.iter().rev().copied().collect();
        assert!(compare(&Cut { pts: back, comp: Some("G41") }, &e, tolerance()).unwrap().contains("other way round"));
        // A figure-eight encloses nothing, either way round.
        let eight = vec![[0.0, 0.0], [100.0, 100.0], [100.0, 0.0], [0.0, 100.0], [0.0, 0.0], [-100.0, 100.0], [-100.0, 0.0], [0.0, 100.0], [0.0, 0.0]];
        let e8 = Cut { pts: eight.clone(), comp: None };
        assert_eq!(compare(&e8.clone(), &e8, tolerance()), None);
        let back8: Vec<P> = eight.iter().rev().copied().collect();
        assert!(compare(&Cut { pts: back8, comp: None }, &e8, tolerance()).unwrap().contains("other way round"));
    }

    #[test]
    fn a_part_called_in_place_of_a_cut_off_line_is_found() {
        // BL39: the counts balance, but that call isn't the planned line.
        let (parts, placed, plan, _) = job();
        let line = CutLine { pts: vec![[0.0, 300.0], [1000.0, 300.0]], kind: CutKind::Remnant };
        let cuts = vec![line.clone()];
        let set = crate::blm::sheet_set(1, &parts, &placed, &spec(), &plan, &cuts, &crate::blm::BlmSettings::for_spec(&spec(), "LC5", "J"));
        assert_eq!(check_set(&set, &parts, &placed, &plan, &cuts), Vec::<String>::new());
        let mut bad = set;
        let main = bad.spf.iter_mut().find(|f| !f.0.contains("70") && !f.0.contains("LIS")).unwrap();
        let part_call = main.1.lines().find(|l| l.contains("CALL")).unwrap().to_string();
        let last_call = main.1.lines().filter(|l| l.contains("CALL")).last().unwrap().to_string();
        main.1 = main.1.replace(&last_call, &part_call);
        let out = check_set(&bad, &parts, &placed, &plan, &cuts);
        assert!(out.iter().any(|m| m.contains("cut-off")), "{out:?}");
    }

    #[test]
    fn an_s_shaped_mark_with_nearly_equal_lobes_passes() {
        // Arcs and chords enclose a little more or less: no sign of direction.
        let lobe = |r: f64, cx: f64, up: bool| -> Vec<P> {
            (0..=32).map(|k| {
                let a = std::f64::consts::PI * (1.0 - k as f64 / 32.0);
                [cx + r * a.cos(), if up { r * a.sin() } else { -r * a.sin() }]
            }).collect()
        };
        let mut planned = lobe(10.0, 10.0, true);
        planned.extend(lobe(10.002, 30.002, false).into_iter().skip(1));
        let w = Cut { pts: planned.iter().map(|p| [p[0], p[1] + 0.001]).collect(), comp: None };
        assert_eq!(compare(&w, &Cut { pts: planned, comp: None }, tolerance()), None);
    }

    #[test]
    fn a_cut_off_line_written_with_compensation_is_found() {
        let (parts, placed, plan, _) = job();
        let cuts = vec![CutLine { pts: vec![[0.0, 300.0], [1000.0, 300.0]], kind: CutKind::Remnant }];
        let mut set = crate::blm::sheet_set(1, &parts, &placed, &spec(), &plan, &cuts, &crate::blm::BlmSettings::for_spec(&spec(), "LC5", "J"));
        let file = set.spf.iter_mut().find(|f| f.1.contains("HD_DWN_OFF")).unwrap();
        file.1 = file.1.replace("G01 G40", "G01 G41");
        let out = check_set(&set, &parts, &placed, &plan, &cuts);
        assert!(out.iter().any(|m| m.contains("compensation on")), "{out:?}");
    }

    #[test]
    fn a_cut_line_with_no_length_is_not_expected() {
        let (parts, placed, plan, _) = job();
        let cuts = vec![CutLine { pts: vec![[900.0, 400.0], [900.0, 400.0]], kind: CutKind::Free }];
        let set = crate::blm::sheet_set(1, &parts, &placed, &spec(), &plan, &cuts, &crate::blm::BlmSettings::for_spec(&spec(), "LC5", "J"));
        assert_eq!(check_set(&set, &parts, &placed, &plan, &cuts), Vec::<String>::new());
    }

    #[test]
    fn punched_holes_are_refused() {
        let (parts, placed, mut plan, set) = job();
        plan.contours[0].punch = Some(crate::machine::Punch { tool: None, hits: vec![] });
        let out = check_set(&set, &parts, &placed, &plan, &[]);
        assert!(out.iter().any(|m| m.contains("punched holes")), "{out:?}");
    }

    #[test]
    fn planned_micro_joints_are_refused_until_the_files_can_carry_them() {
        // MAC-30: they'd be cut free with no warning otherwise.
        let (parts, placed, mut plan, set) = job();
        plan.contours[0].joints.push((10.0, 0.5));
        let out = check_set(&set, &parts, &placed, &plan, &[]);
        assert!(out.iter().any(|m| m.contains("DEMO-001") && m.contains("micro-joints")), "{out:?}");
    }

    #[test]
    fn unreadable_lines_and_arcs_off_their_circle_are_reported() {
        assert!(written_paths("NEXT_L(0,0,0,2,3)\r\nG01 Xabc Y1\r\n").is_err());
        assert!(written_paths("G01 X1 Y1\r\n").is_err());
        assert!(written_paths("NEXT_L(0,0,0,2,3)\r\nG03 X10 Y0 I1 J0\r\n").is_err());
        assert!(written_paths("NEXT_L(0,0,0,2,3)\r\nLASTLINE(0,0,0,1,5,5,6,6,0,0)\r\n").is_err());
        let p = written_paths("NEXT_L(0,0,0,2,3)\r\nG01 X10\r\nG03 X20 Y0 I5 J0\r\nLASTLINE(0,0,0,1,20,0,30,0,0,0)\r\n").unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].pts.last(), Some(&[30.0, 0.0]));
        // Counter-clockwise from left to right runs under the centre.
        assert!(p[0].pts.iter().any(|q| (q[1] + 5.0).abs() < 0.01), "{:?}", &p[0].pts[..4]);
    }
}
