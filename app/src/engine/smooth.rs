//! Smoothing a drawing (SM10–SM21): a radius or a round hole drawn as many
//! short straight lines comes back as true arcs, so the machine cuts a smooth
//! curve instead of facets. The same routine runs on import (Settings →
//! General → Smoothing on import, off by default) and from the part screen's Smooth command.
//!
//! A contour is walked from its first point. From each point the longest run
//! of lines that lies on one circle within the tolerance becomes one arc; a
//! point no arc starts from keeps its line. Every arc starts and ends on points
//! of the drawing, so the contour stays joined, and is less than a half circle.
//! Contours keep their numbers, so a part's technology stays on them.
//!
//! The app keeps every curve as points (arcs split within 0.1 mm, DF4); a
//! smoothed arc is split the same way, so the posts that write arcs (BLM, OMAX)
//! find it again as one arc.

use crate::dxf::{Contour, CustomGeo};
use crate::nest::{oriented, Part, Placed, Shape, P};

/// How closely lines must follow a circle to become an arc (SM14–SM16).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Smooth {
    /// The largest gap, mm, between the arc and any point of the lines it
    /// replaces, their middles included (SM15).
    pub tol: f64,
    /// Lines that would need a larger radius than this (mm) stay lines (SM14).
    pub max_r: f64,
    /// An arc replaces at least this many lines, and never fewer than 2 (SM16).
    pub min_lines: usize,
}

impl Smooth {
    /// The import option's values (SM10; the original's 0.05 mm and 50 000).
    pub const IMPORT: Smooth = Smooth { tol: 0.05, max_r: 50_000.0, min_lines: 0 };
    /// The Smooth command's values (SM21: 0.5 mm, the most common in the
    /// original's starting settings; the radius as on import, estimated).
    pub const COMMAND: Smooth = Smooth { tol: 0.5, max_r: 50_000.0, min_lines: 0 };

    /// A refusal for values that can't smooth anything, in shop terms.
    pub fn check(&self) -> Result<(), String> {
        if !(self.tol > 0.0 && self.tol.is_finite()) {
            return Err(format!("Smoothing tolerance {}: give the largest gap allowed between a line and its arc, in mm, more than 0 (e.g. 0.05).", self.tol));
        }
        if self.tol > MAX_TOL {
            return Err(format!("Smoothing tolerance {} mm is more than {MAX_TOL} mm: an arc that far from the drawing changes the part. Give a smaller tolerance.", self.tol));
        }
        if !(self.max_r > 0.0 && self.max_r.is_finite()) {
            return Err(format!("Largest radius {}: give the largest arc radius to make, in mm, more than 0 (e.g. 50000).", self.max_r));
        }
        Ok(())
    }
}

impl Default for Smooth {
    fn default() -> Self {
        Smooth::IMPORT
    }
}

/// Smoothing in Settings: on import (and with what), and the Smooth command's
/// values, kept from one use to the next.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SmoothSettings {
    /// Smooth every imported drawing (SM10; off, as in the original).
    pub on_import: bool,
    pub import: Smooth,
    pub command: Smooth,
}

impl Default for SmoothSettings {
    fn default() -> Self {
        SmoothSettings { on_import: false, import: Smooth::IMPORT, command: Smooth::COMMAND }
    }
}

impl SmoothSettings {
    /// Values kept in their ranges (a hand-edited settings file).
    pub fn sane(mut self) -> Self {
        let fix = |s: Smooth, d: Smooth| Smooth {
            tol: if s.tol.is_finite() && s.tol > 0.0 && s.tol <= MAX_TOL { s.tol } else { d.tol },
            max_r: if s.max_r.is_finite() && s.max_r > 0.0 { s.max_r } else { d.max_r },
            min_lines: s.min_lines.min(MAX_MIN_LINES),
        };
        self.import = fix(self.import, Smooth::IMPORT);
        self.command = fix(self.command, Smooth::COMMAND);
        self
    }

    /// Take the original's own values: its DXF reading settings (`dxf`:
    /// A039 smooth on import, A040 fewest elements, A042 tolerance, A043
    /// largest radius; SM10) and its drawing settings (`drawing`: A190 fewest
    /// elements and A192 tolerance of the Fitting command, SM21). Either text
    /// may be empty.
    pub fn import_cfg(&mut self, dxf: &str, drawing: &str) {
        let val = |cfg: &str, k: &str| cfg.lines().find(|l| l.split_whitespace().next() == Some(k)).and_then(|l| l.split_whitespace().nth(1)).map(str::to_string);
        let num = |cfg: &str, k: &str| val(cfg, k).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite() && *v >= 0.0);
        if let Some(v) = val(dxf, "A039") {
            self.on_import = v.eq_ignore_ascii_case("ON");
        }
        if let Some(v) = num(dxf, "A040") {
            self.import.min_lines = v as usize;
        }
        if let Some(v) = num(dxf, "A042").filter(|v| *v > 0.0) {
            self.import.tol = v;
        }
        if let Some(v) = num(dxf, "A043").filter(|v| *v > 0.0) {
            self.import.max_r = v;
        }
        if let Some(v) = num(drawing, "A190") {
            self.command.min_lines = v as usize;
        }
        if let Some(v) = num(drawing, "A192").filter(|v| *v > 0.0) {
            self.command.tol = v;
        }
        *self = self.sane();
    }
}

/// Most lines an arc can be asked to replace at least (the original's own
/// limit for its element counts).
pub const MAX_MIN_LINES: usize = 32_767;

/// Largest tolerance taken (mm): beyond it smoothing reshapes the part.
pub const MAX_TOL: f64 = 10.0;
/// Two points this close (mm) are one point (the drawing's joining
/// tolerance, SM14).
const PT_TOL: f64 = crate::dxf::JOIN_TOL;
/// How far a run is first grown at a time, in points (SM13).
const GROW: usize = 29;
/// The middle points of a quick try, counted from the run's start (SM13).
const QUICK: [usize; 3] = [7, 14, 21];
/// How far under a half circle (radians) an arc must stay (SM15).
const HALF_EPS: f64 = 1e-6;
/// Most one line of an arc may be longer than the next (SM12).
const LENGTH_RATIO: f64 = 2.0;
/// Most pieces one smoothed arc is split into (as DX2 splits an arc).
const MAX_PIECES: usize = 720;
/// SM12 (own): the sharpest turn between two lines of one arc, 30°.
const MAX_TURN: f64 = std::f64::consts::PI / 6.0 + 1e-6;

/// One fitted arc: centre, radius, turning direction.
#[derive(Clone, Copy, Debug)]
struct Arc {
    c: P,
    r: f64,
    ccw: bool,
}

fn dist(a: P, b: P) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Distance from `p` to the segment a–b.
fn seg_dist(p: P, a: P, b: P) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    dist(p, [a[0] + t * dx, a[1] + t * dy])
}

/// Distance from `p` to the whole line through a and b.
fn line_dist(p: P, a: P, b: P) -> f64 {
    let l = dist(a, b);
    if l <= 0.0 {
        return dist(p, a);
    }
    ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])).abs() / l
}

/// The angle of `p` round the arc's centre, measured from `from` the way the
/// arc turns, in [0, 2π).
fn rel(a: &Arc, from: f64, p: P) -> f64 {
    let ang = (p[1] - a.c[1]).atan2(p[0] - a.c[0]);
    let d = if a.ccw { ang - from } else { from - ang };
    d.rem_euclid(std::f64::consts::TAU)
}

/// Do the lines pts[s..=e] follow `arc` from pts[s] to pts[e] (SM15)? Returns
/// the sum of each line's largest gap to the circle (smaller fits better).
fn follows(pts: &[P], s: usize, e: usize, arc: &Arc, o: &Smooth) -> Option<f64> {
    use std::f64::consts::{PI, TAU};
    const EPS: f64 = 1e-9;
    let from = (pts[s][1] - arc.c[1]).atan2(pts[s][0] - arc.c[0]);
    let sweep = rel(arc, from, pts[e]);
    // SM15: always less than a half circle (a half circle exactly, to the
    // rounding of its points, is not).
    if sweep <= EPS || sweep >= PI - HALF_EPS {
        return None;
    }
    // SM12 (own): a run breaks where one line is more than twice as long as
    // the next, as contours break into elements (a straight meeting a curve).
    // The original's runs end at the drawing's own arcs; ours are points, so
    // without this a straight edge beside a radius could bow by up to the
    // tolerance. A radius drawn as lines has lines of one length.
    // Nor does it go round a corner: where the lines turn more than 30° the
    // corner is meant (a hexagon hole stays a hexagon).
    for k in s..e.saturating_sub(1) {
        let (l0, l1) = (dist(pts[k], pts[k + 1]), dist(pts[k + 1], pts[k + 2]));
        if l0.max(l1) > LENGTH_RATIO * l0.min(l1) {
            return None;
        }
        let (u, v) = ([pts[k + 1][0] - pts[k][0], pts[k + 1][1] - pts[k][1]], [pts[k + 2][0] - pts[k + 1][0], pts[k + 2][1] - pts[k + 1][1]]);
        if (u[0] * v[1] - u[1] * v[0]).atan2(u[0] * v[0] + u[1] * v[1]).abs() > MAX_TURN {
            return None;
        }
    }
    let gap = |p: P| (dist(p, arc.c) - arc.r).abs();
    let over = |g: f64| g > o.tol;
    let inside = |t: f64| t <= sweep + EPS || t >= TAU - EPS;
    let mut score = 0.0;
    for k in s..=e {
        if over(gap(pts[k])) || !inside(rel(arc, from, pts[k])) {
            return None;
        }
    }
    for k in s..e {
        let (a, b) = (pts[k], pts[k + 1]);
        // A line is furthest outside the circle at an end, furthest inside
        // where it passes nearest the centre (its sag).
        let worst = gap(a).max(gap(b)).max(arc.r - seg_dist(arc.c, a, b));
        if over(worst) {
            return None;
        }
        let mid = rel(arc, from, [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]);
        if mid <= 0.0 || mid >= sweep {
            return None;
        }
        score += worst;
    }
    Some(score)
}

/// The circle through pts[s], pts[m] and pts[e], if it can be an arc (SM14).
fn candidate(pts: &[P], s: usize, m: usize, e: usize, o: &Smooth) -> Option<Arc> {
    let (a, b, c) = (pts[s], pts[m], pts[e]);
    // Three points in a row make no arc.
    if line_dist(c, a, b) <= PT_TOL {
        return None;
    }
    let (centre, r) = crate::partview::circle_through(a, b, c)?;
    if r > o.max_r || !r.is_finite() {
        return None;
    }
    let ccw = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]) > 0.0;
    Some(Arc { c: centre, r, ccw })
}

/// A quick try at an arc over pts[s..=e]: the middle point 7, 14 or 21 points
/// on, the first that fits (SM13).
fn quick(pts: &[P], s: usize, e: usize, o: &Smooth) -> bool {
    QUICK.iter().map(|k| s + k).filter(|&m| m < e).any(|m| candidate(pts, s, m, e, o).is_some_and(|a| follows(pts, s, e, &a, o).is_some()))
}

/// The best arc over pts[s..=e]: the middle point a quarter, half and three
/// quarters along, the one whose lines keep closest to it (SM14).
fn fit(pts: &[P], s: usize, e: usize, o: &Smooth) -> Option<Arc> {
    let n = e - s + 1;
    if dist(pts[s], pts[e]) <= PT_TOL {
        return None;
    }
    let step = (n / 4).max(1);
    let mut best: Option<(f64, Arc)> = None;
    for m in [s + step, s + 2 * step, s + 3 * step] {
        if m <= s || m >= e || (n == 3 && m != s + 1) {
            continue;
        }
        if let Some(a) = candidate(pts, s, m, e, o) {
            if let Some(score) = follows(pts, s, e, &a, o) {
                if best.is_none_or(|(b, _)| score < b) {
                    best = Some((score, a));
                }
            }
        }
    }
    best.map(|b| b.1)
}

/// SM10: smooth a drawing on import when Settings → General → Smoothing on
/// import is on; the note for the import's list, when anything changed.
pub fn on_import(g: &mut CustomGeo, s: &SmoothSettings, name: &str) -> Option<String> {
    if !s.on_import {
        return None;
    }
    let done = smooth_geo(g, None, &s.import);
    (!done.contours.is_empty()).then(|| format!("{name}: smoothed, {}", done.summary()))
}

/// What smoothing did to one contour.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Count {
    /// Arcs made.
    pub arcs: usize,
    /// Lines the arcs replaced.
    pub lines: usize,
}

/// Smooth one path (SM12, SM13, SM16, SM17): pts[0]…pts[last], a closed
/// contour with its first point repeated at the end. Returns the new path
/// (same first and last point) and what it did.
fn smooth_path(pts: &[P], o: &Smooth) -> (Vec<P>, Count) {
    let mut out = vec![];
    let mut count = Count::default();
    if pts.len() < 3 {
        return (pts.to_vec(), count);
    }
    let last = pts.len() - 1;
    out.push(pts[0]);
    let mut s = 0;
    while s < last {
        // Grow 29 points at a time while a quick try fits…
        let mut start = last;
        let mut e = s + GROW;
        while e < last {
            if !quick(pts, s, e, o) {
                start = e;
                break;
            }
            e += GROW;
        }
        // …then back one point at a time to the longest run that fits.
        let mut made = None;
        for e in (s + 1..=start).rev() {
            let n = e - s + 1;
            if n < 3 || n <= o.min_lines {
                break;
            }
            if let Some(a) = fit(pts, s, e, o) {
                made = Some((e, a));
                break;
            }
        }
        match made {
            Some((e, a)) => {
                let a0 = (pts[s][1] - a.c[1]).atan2(pts[s][0] - a.c[0]);
                let sweep = rel(&a, a0, pts[e]);
                let sweep = if a.ccw { sweep } else { -sweep };
                // SM17 (own): split as the app splits arcs (DX2), but in at
                // least four pieces, so the posts find it again as an arc even
                // when the BLM post keeps the contour's last move straight; and
                // within the tolerance, for a radius too large for the posts'
                // arcs.
                let fine = if a.r > o.tol { (sweep.abs() / (2.0 * (1.0 - o.tol / a.r).acos())).ceil() as usize } else { 0 };
                let n = crate::dxf::arc_segments(a.r, sweep).max(fine.min(MAX_PIECES)).max(crate::blm::ARC_MIN_MOVES + 1);
                let mut arc: Vec<P> = (0..=n).map(|i| a0 + sweep * i as f64 / n as f64).map(|t| [a.c[0] + a.r * t.cos(), a.c[1] + a.r * t.sin()]).collect();
                // SM17: the arc ends exactly on the drawing's point.
                if let Some(l) = arc.last_mut() {
                    *l = pts[e];
                }
                out.extend_from_slice(&arc[1..]);
                count.arcs += 1;
                count.lines += e - s;
                s = e;
            }
            None => {
                out.push(pts[s + 1]);
                s += 1;
            }
        }
    }
    (out, count)
}

/// Smooth one contour; None when nothing changed.
pub fn smooth_contour(c: &Contour, o: &Smooth) -> Option<(Contour, Count)> {
    // SM12: a contour of one line has nothing to smooth.
    if c.pts.len() < 3 {
        return None;
    }
    let path = c.path();
    let (mut pts, count) = smooth_path(&path, o);
    if count.arcs == 0 {
        return None;
    }
    if c.closed {
        pts.pop();
    }
    Some((Contour { pts, ..c.clone() }, count))
}

/// What smoothing a drawing did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Smoothed {
    /// The contours that changed (numbers from 0).
    pub contours: Vec<usize>,
    pub count: Count,
    /// How far the drawing moved to keep its box starting at 0,0 (mm).
    pub shift: P,
}

impl Smoothed {
    /// What it did, in shop terms.
    pub fn summary(&self) -> String {
        if self.contours.is_empty() {
            return "nothing smoothed".into();
        }
        let c = self.contours.len();
        format!("{} short line(s) became {} arc(s) on {c} contour{}", self.count.lines, self.count.arcs, if c == 1 { "" } else { "s" })
    }
}

/// Smooth contours `picks` of a drawing (all when None) and keep its box at
/// 0,0 (an arc bulges past the corners of the lines it replaces).
pub fn smooth_geo(g: &mut CustomGeo, picks: Option<&[usize]>, o: &Smooth) -> Smoothed {
    let mut list = g.contour_list();
    let mut done = Smoothed::default();
    for (k, c) in list.iter_mut().enumerate() {
        if picks.is_some_and(|p| !p.contains(&k)) {
            continue;
        }
        if let Some((new, n)) = smooth_contour(c, o) {
            *c = new;
            done.contours.push(k);
            done.count.arcs += n.arcs;
            done.count.lines += n.lines;
        }
    }
    if done.contours.is_empty() {
        return done;
    }
    g.contours = list;
    g.apply_states();
    // The part's box is its outline's.
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in &g.outline {
        for i in 0..2 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    if lo[0] <= hi[0] {
        let d = lo;
        if d != [0.0, 0.0] {
            let mv = |p: &mut P| {
                p[0] -= d[0];
                p[1] -= d[1];
            };
            g.contours.iter_mut().for_each(|c| c.pts.iter_mut().for_each(mv));
            g.texts.iter_mut().for_each(|t| mv(&mut t.at));
            g.apply_states();
        }
        (g.w, g.h) = (hi[0] - lo[0], hi[1] - lo[1]);
        done.shift = d;
    }
    done
}

/// What smoothing a part of a job did.
#[derive(Clone, Debug, PartialEq)]
pub struct PartSmoothed {
    pub message: String,
    pub done: Smoothed,
    /// Copies of it are nested (their sheets need checking: an arc bulges
    /// outward by up to the tolerance).
    pub nested: bool,
}

/// Smooth part `i`'s contours `picks` (numbers from 0; all when None), as the
/// part screen's Smooth command and agents' `smooth` do (SM21). Its picked
/// lead-ins, micro-joints and moved leads, and its nested copies, keep their
/// places.
pub fn smooth_part(parts: &mut [Part], sheets: &mut [Vec<Placed>], i: usize, picks: Option<&[usize]>, o: &Smooth) -> Result<PartSmoothed, String> {
    o.check()?;
    let Some(old) = parts.get(i) else { return Err(format!("There is no part {} (the job has {}).", i + 1, parts.len())) };
    let geo = match (&old.shape, &old.custom) {
        (Shape::Custom, Some(g)) => (**g).clone(),
        _ => return Err(format!("'{}' is a built-in shape: its arcs are exact already. Smoothing is for imported drawings.", old.name)),
    };
    if let Some(p) = picks {
        let n = geo.contour_list().len();
        if let Some(&k) = p.iter().find(|&&k| k >= n) {
            return Err(format!("'{}' has {n} contours; there is no contour {} (part-info lists them).", old.name, k + 1));
        }
    }
    let mut geo = geo;
    let done = smooth_geo(&mut geo, picks, o);
    let what = |p: &Part| format!("Smoothed '{}' (tolerance {} mm): {}.", p.name, crate::nest::angle_num(o.tol), done.summary());
    if done.contours.is_empty() {
        let message = format!("'{}' not smoothed: no run of its lines lies on one circle within {} mm. A radius drawn with few lines sags more between its points; give a larger tolerance.", old.name, crate::nest::angle_num(o.tol));
        return Ok(PartSmoothed { message, done, nested: false });
    }
    let mut p = old.clone();
    let d = done.shift;
    let mv = |q: &mut P| {
        q[0] -= d[0];
        q[1] -= d[1];
    };
    p.texts.iter_mut().for_each(|t| mv(&mut t.at));
    if let Some(tech) = p.tech.as_mut() {
        tech.starts.iter_mut().flatten().for_each(|s| mv(&mut s.1));
        tech.joint_picks.iter_mut().flatten().for_each(|s| mv(&mut s.1));
        tech.lead_edits.iter_mut().flatten().for_each(|e| mv(&mut e.at));
    }
    (p.w, p.h) = (geo.w, geo.h);
    p.custom = Some(std::sync::Arc::new(geo));
    // Nested copies stay put: the same drawing points land on the same spots.
    let mut nested = false;
    for pl in sheets.iter_mut().flatten().filter(|pl| pl.part == i) {
        nested = true;
        let mut np = oriented(&p, i, pl.angle(), pl.mirror);
        let (want, got) = (pl.map([0.0, 0.0]), np.map([-d[0], -d[1]]));
        np.x += want[0] - got[0];
        np.y += want[1] - got[1];
        *pl = np;
    }
    let mut message = what(&p);
    // LE8: a lead set for All the same contours stays on the holes still of
    // its shape; a hole smoothed differently (its points start elsewhere) is
    // cut with the automatic lead again, and the message says so.
    let (before, after) = (super::hand_leads(old), super::hand_leads(&p));
    let lost: Vec<usize> = (0..before.len()).filter(|&j| before[j].is_some() && after.get(j).copied().flatten().is_none()).collect();
    if !lost.is_empty() {
        let nos: Vec<String> = lost.iter().map(|&j| super::contour_number(&p, j).to_string()).collect();
        message.push_str(&format!(" Contour(s) {} no longer match the hole whose lead they shared (All the same contours), so they take the automatic lead: set theirs again.", nos.join(", ")));
    }
    if nested {
        message.push_str(" Its nested copies stay where they are; an arc can bulge out by up to the tolerance, so check their gaps or nest again.");
    }
    parts[i] = p;
    Ok(PartSmoothed { message, done, nested })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dxf::ContourState;
    use std::f64::consts::{PI, TAU};

    /// A circle of radius r as an n-sided polygon (corners on the circle).
    fn polygon(c: P, r: f64, n: usize) -> Vec<P> {
        (0..n).map(|k| k as f64 * TAU / n as f64).map(|a| [c[0] + r * a.cos(), c[1] + r * a.sin()]).collect()
    }

    fn max_gap(pts: &[P], c: P, r: f64) -> f64 {
        let mut path = pts.to_vec();
        path.push(pts[0]);
        path.windows(2).map(|w| (dist(w[0], c) - r).abs().max(r - seg_dist(c, w[0], w[1]))).fold(0.0, f64::max)
    }

    /// SM15, SM18: a fine polygon hole comes back as arcs each under a half
    /// circle, with every point and every chord within the arc smoothness of
    /// the true circle (the app's own 0.1 mm), through the same start point.
    #[test]
    fn a_fine_polygon_hole_becomes_arcs_under_a_half_circle() {
        let c = Contour::new(polygon([50.0, 50.0], 20.0, 360), true, ContourState::Cut);
        // Each 1° chord sags 0.0030 mm: within 0.05.
        let (s, n) = smooth_contour(&c, &Smooth::IMPORT).expect("smoothed");
        // Two arcs of 179°; the last two lines stay, their points too nearly
        // in a row (within 0.01 mm) to make an arc.
        assert_eq!(n, Count { arcs: 2, lines: 358 });
        assert_eq!(s.pts[0], c.pts[0]);
        assert!(s.pts.len() < 360, "{} points", s.pts.len());
        assert!(max_gap(&s.pts, [50.0, 50.0], 20.0) <= crate::dxf::ARC_CHORD_ERR + 1e-6);
    }

    /// SM15: a coarse polygon whose chords sag more than the tolerance stays
    /// as it is; a looser tolerance smooths it.
    #[test]
    fn the_tolerance_counts_the_sag_of_each_line() {
        // 24 sides on Ø100: each chord sags 0.43 mm.
        let c = Contour::new(polygon([0.0, 0.0], 50.0, 24), true, ContourState::Cut);
        assert!(smooth_contour(&c, &Smooth::IMPORT).is_none());
        let loose = Smooth { tol: 0.5, ..Smooth::IMPORT };
        let (s, n) = smooth_contour(&c, &loose).expect("smoothed at 0.5 mm");
        assert_eq!(n.lines, 24);
        assert!(s.pts.len() > 24, "split finer than the drawing: {}", s.pts.len());
        // The new points lie on the circle through the corners.
        assert!(s.pts.iter().all(|&p| (dist(p, [0.0, 0.0]) - 50.0).abs() < 0.01));
    }

    /// SM12, SM14: straight edges stay straight (three points in a row make
    /// no arc, and a long line next to the short ones of a radius never joins
    /// its arc), sharp corners stay corners, and a rounded corner drawn as
    /// lines becomes one arc between them, at any tolerance.
    #[test]
    fn a_rounded_corner_drawn_as_lines_becomes_one_arc() {
        // A 100 × 60 plate with its top-right corner rounded R10 in 18 lines,
        // its straight edges drawn as several pieces.
        let mut pts = vec![[0.0, 0.0], [50.0, 0.0], [100.0, 0.0], [100.0, 25.0], [100.0, 50.0]];
        for k in 1..18 {
            let a = PI / 2.0 * k as f64 / 18.0;
            pts.push([90.0 + 10.0 * a.cos(), 50.0 + 10.0 * a.sin()]);
        }
        pts.extend([[90.0, 60.0], [40.0, 60.0], [0.0, 60.0], [0.0, 30.0]]);
        let c = Contour::new(pts.clone(), true, ContourState::Cut);
        for tol in [0.5, 2.54] {
            assert_eq!(smooth_contour(&c, &Smooth { tol, ..Smooth::IMPORT }).unwrap().1, Count { arcs: 1, lines: 18 }, "at {tol} mm");
        }
        let (s, n) = smooth_contour(&c, &Smooth::IMPORT).expect("smoothed");
        assert_eq!(n, Count { arcs: 1, lines: 18 });
        // The corners and straight pieces are all still there.
        for q in [[0.0, 0.0], [50.0, 0.0], [100.0, 0.0], [100.0, 25.0], [100.0, 50.0], [90.0, 60.0], [40.0, 60.0], [0.0, 60.0], [0.0, 30.0]] {
            assert!(s.pts.contains(&q), "{q:?} lost");
        }
        // The arc's points lie on R10.
        assert!(s.pts.iter().filter(|p| p[0] > 90.0 && p[1] > 50.0).all(|&p| (dist(p, [90.0, 50.0]) - 10.0).abs() < 1e-6));
    }

    /// SM12 (own): a corner turning more than 30° stays a corner: a small
    /// hexagon hole, whose chords sag under the tolerance, is not rounded.
    #[test]
    fn a_hexagon_hole_stays_a_hexagon() {
        // 5.5 mm across flats: each side sags 0.43 mm from the circle through
        // its corners, under the 0.5 mm of the Smooth command.
        let c = Contour::new(polygon([10.0, 10.0], 5.5 / 3f64.sqrt(), 6), true, ContourState::Cut);
        assert!(smooth_contour(&c, &Smooth::COMMAND).is_none());
        assert!(smooth_contour(&c, &Smooth { tol: 2.54, ..Smooth::COMMAND }).is_none());
    }

    /// SM17 (own): a short arc comes back in at least three pieces, so the
    /// BLM and OMAX posts write it as one arc move, not two straight ones.
    #[test]
    fn a_short_arc_goes_to_the_machine_as_an_arc() {
        // A 20° bend of R10 drawn as 20 lines between two straight edges.
        let mut pts = vec![[0.0, -40.0], [10.0, -40.0]];
        for k in 0..=20 {
            let a = (20.0 * k as f64 / 20.0).to_radians();
            pts.push([10.0 * a.cos(), 10.0 * a.sin()]);
        }
        pts.push([0.0, 30.0]);
        let c = Contour::new(pts, true, ContourState::Cut);
        let (s, n) = smooth_contour(&c, &Smooth::IMPORT).expect("smoothed");
        assert_eq!(n, Count { arcs: 1, lines: 20 });
        let arcs = |pts: &[P]| crate::blm::fit_moves(pts).iter().filter(|m| matches!(m, crate::blm::Move::Arc { .. })).count();
        assert_eq!(arcs(&s.path()), 1, "{:?}", s.pts);
        // The same bend ending the contour: the BLM post writes the last move
        // as a straight, and the rest is still one arc.
        let mut pts: Vec<P> = vec![[10.0 * 20f64.to_radians().cos(), 10.0 * 20f64.to_radians().sin()], [0.0, 30.0], [0.0, -40.0], [10.0, -40.0]];
        pts.extend((0..20).map(|k| (k as f64).to_radians()).map(|a| [10.0 * a.cos(), 10.0 * a.sin()]));
        let c = Contour::new(pts, true, ContourState::Cut);
        let (s, n) = smooth_contour(&c, &Smooth::IMPORT).expect("smoothed");
        assert_eq!(n, Count { arcs: 1, lines: 20 });
        let path = s.path();
        assert_eq!(arcs(&path[..path.len() - 1]), 1, "{:?}", s.pts);
    }

    /// SM17 (own): a radius too large for the posts' arcs leaves them as
    /// lines, so its pieces stay within the tolerance.
    #[test]
    fn a_very_large_radius_stays_within_the_tolerance() {
        // 1° of R10 000 in 50 lines, smoothed at 0.001 mm.
        let r = 10_000.0;
        let mut pts: Vec<P> = (0..=50).map(|k| (89.5 + k as f64 / 50.0).to_radians()).map(|a| [r * a.cos(), r * a.sin() - r]).collect();
        pts.extend([[-200.0, -100.0], [200.0, -100.0]]);
        let c = Contour::new(pts, true, ContourState::Cut);
        let o = Smooth { tol: 0.001, ..Smooth::IMPORT };
        let (s, n) = smooth_contour(&c, &o).expect("smoothed");
        assert!(n.arcs >= 1, "{n:?}");
        let on = |p: P| (dist(p, [0.0, -r]) - r).abs() < 1e-5;
        let sag = s.path().windows(2).filter(|w| on(w[0]) && on(w[1])).map(|w| r - seg_dist([0.0, -r], w[0], w[1])).fold(0.0, f64::max);
        assert!(sag <= o.tol + 1e-9, "sags {sag} mm");
    }

    /// SM15: a tolerance under the joining tolerance (0.01 mm) is held to:
    /// 4° chords of R10 sag 0.006 mm, too much at 0.001 mm.
    #[test]
    fn a_tolerance_under_a_hundredth_is_held_to() {
        let c = Contour::new(polygon([0.0, 0.0], 10.0, 90), true, ContourState::Cut);
        assert!(smooth_contour(&c, &Smooth { tol: 0.001, ..Smooth::IMPORT }).is_none());
        assert!(smooth_contour(&c, &Smooth { tol: 0.01, ..Smooth::IMPORT }).is_some());
    }

    /// SM16: fewer lines than the minimum stay lines.
    #[test]
    fn an_arc_replaces_at_least_the_minimum_of_lines() {
        let mut pts = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 50.0]];
        for k in 1..4 {
            let a = PI / 2.0 * k as f64 / 4.0;
            pts.push([90.0 + 10.0 * a.cos(), 50.0 + 10.0 * a.sin()]);
        }
        pts.extend([[90.0, 60.0], [0.0, 60.0]]);
        let c = Contour::new(pts, true, ContourState::Cut);
        let o = Smooth { tol: 0.5, ..Smooth::IMPORT };
        assert_eq!(smooth_contour(&c, &o).unwrap().1, Count { arcs: 1, lines: 4 });
        assert!(smooth_contour(&c, &Smooth { min_lines: 10, ..o }).is_none());
    }

    /// SM14: a run that would need a radius over the largest stays lines.
    #[test]
    fn a_radius_over_the_largest_stays_lines() {
        let mut pts: Vec<P> = (0..=20).map(|k| PI / 2.0 + 0.002 * (k as f64 - 10.0)).map(|a| [1000.0 * a.cos(), 1000.0 * a.sin() - 1000.0]).collect();
        pts.extend([[20.0, -50.0], [-20.0, -50.0]]);
        let c = Contour::new(pts, true, ContourState::Cut);
        assert!(smooth_contour(&c, &Smooth::IMPORT).is_some());
        assert!(smooth_contour(&c, &Smooth { max_r: 500.0, ..Smooth::IMPORT }).is_none());
    }

    /// A round hole stays round: smoothed, its points spread unevenly over
    /// its arcs, it is still recognised as a circle of the same size (so it
    /// keeps a round hole's lead and cut quality).
    #[test]
    fn a_smoothed_hole_is_still_a_round_hole() {
        let c = Contour::new(polygon([70.0, 60.0], 30.0, 24), true, ContourState::Cut);
        let before = crate::machine::recognise(&c.pts);
        let (s, _) = smooth_contour(&c, &Smooth::COMMAND).unwrap();
        let after = crate::machine::recognise(&s.pts);
        match (before, after) {
            (Some(crate::machine::StdShape::Circle { d: d0, .. }), Some(crate::machine::StdShape::Circle { c, d })) => {
                assert!((d - d0).abs() < 0.01 && dist(c, [70.0, 60.0]) < 0.01, "Ø{d0} → Ø{d} at {c:?}");
            }
            other => panic!("not round both times: {other:?}"),
        }
    }

    /// LE8: two holes of one shape that start at different corners smooth
    /// into different points; a lead they shared is then lost from the other,
    /// and the message says which.
    #[test]
    fn a_shared_lead_lost_by_smoothing_is_named() {
        let hole = |at: P, from: usize| {
            let mut v = polygon(at, 10.0, 24);
            v.rotate_left(from);
            v
        };
        let outline = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 50.0], [0.0, 50.0]];
        let holes = vec![hole([25.0, 25.0], 0), hole([75.0, 25.0], 5)];
        // Holes first, the outline last (CS1).
        let mut contours: Vec<Contour> = holes.iter().map(|h| Contour::new(h.clone(), true, ContourState::Cut)).collect();
        contours.push(Contour::new(outline.clone(), true, ContourState::Cut));
        let mut g = CustomGeo { outline, holes, fills: vec![], hole_fills: vec![], marks: vec![], texts: vec![], w: 100.0, h: 50.0, contours, hand_order: vec![] };
        g.apply_states();
        let mut parts = vec![Part { name: "DEMO-001".into(), shape: Shape::Custom, w: 100.0, h: 50.0, qty: 1, rotate: true, color: 0, tech: None, texts: vec![], group: String::new(), erp: vec![], custom: Some(std::sync::Arc::new(g)), rots: Default::default() }];
        let spec = crate::nest::SheetSpec { w: 1000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: None, pick: Default::default(), sheet_list: vec![] };
        assert_eq!(super::super::same_holes(&parts[0], 0), vec![1]);
        super::super::move_lead_start(&mut parts, 0, &spec, 0, [35.0, 25.0], 0.0, true, crate::units::LenUnit::Mm).unwrap();
        assert!(parts[0].tech.as_ref().unwrap().lead_edits.iter().flatten().any(|e| e.same));
        let first = parts[0].clone();
        let done = smooth_part(&mut parts, &mut [], 0, None, &Smooth::COMMAND).unwrap();
        assert!(super::super::same_holes(&parts[0], 0).is_empty());
        let no = super::super::contour_number(&parts[0], 1);
        assert!(done.message.contains(&format!("Contour(s) {no} no longer match")), "{}", done.message);
        // A hole with its own lead keeps it, and isn't named.
        let mut parts = vec![first.clone()];
        super::super::move_lead_start(&mut parts, 0, &spec, 1, [85.0, 25.0], 0.0, false, crate::units::LenUnit::Mm).unwrap();
        let done = smooth_part(&mut parts, &mut [], 0, None, &Smooth::COMMAND).unwrap();
        assert!(!done.message.contains("no longer match"), "{}", done.message);
    }

    /// Smoothing a nested part keeps its copies on the same spots, and its
    /// box at 0,0.
    #[test]
    fn a_nested_part_keeps_its_place() {
        // 22 sides: no corner at the top and bottom, so the arcs bulge past the box.
        let outline = polygon([50.0, 50.0], 50.0, 22);
        let lo = outline.iter().fold([f64::MAX; 2], |m, p| [m[0].min(p[0]), m[1].min(p[1])]);
        let outline: Vec<P> = outline.iter().map(|p| [p[0] - lo[0], p[1] - lo[1]]).collect();
        let mut g = CustomGeo { outline: outline.clone(), holes: vec![], fills: vec![], hole_fills: vec![], marks: vec![], texts: vec![], w: 0.0, h: 0.0, contours: vec![Contour::new(outline, true, ContourState::Cut)], hand_order: vec![] };
        g.apply_states();
        let hi = g.outline.iter().fold([f64::MIN; 2], |m, p| [m[0].max(p[0]), m[1].max(p[1])]);
        (g.w, g.h) = (hi[0], hi[1]);
        let mut parts = vec![Part { name: "DEMO-001".into(), shape: Shape::Custom, w: g.w, h: g.h, qty: 1, rotate: true, color: 0, tech: None, texts: vec![], group: String::new(), erp: vec![], custom: Some(std::sync::Arc::new(g)), rots: Default::default() }];
        let pl = oriented(&parts[0], 0, 90.0, false);
        let pl = Placed { x: 200.0, y: 300.0, ..pl };
        let mut sheets = vec![vec![pl]];
        // A corner of the drawing, on the sheet before.
        let corner = parts[0].custom.as_ref().unwrap().outline[0];
        let before = sheets[0][0].map(corner);
        let done = smooth_part(&mut parts, &mut sheets, 0, None, &Smooth { tol: 1.0, ..Smooth::IMPORT }).unwrap();
        assert!(done.nested && done.done.contours == vec![0], "{done:?}");
        let g = parts[0].custom.as_ref().unwrap();
        let lo = g.outline.iter().fold([f64::MAX; 2], |m, p| [m[0].min(p[0]), m[1].min(p[1])]);
        assert!(lo[0].abs() < 1e-9 && lo[1].abs() < 1e-9, "box starts at {lo:?}");
        assert!(done.done.shift[0] < 0.0 || done.done.shift[1] < 0.0, "the arcs bulge past the corners: {:?}", done.done.shift);
        let moved = [corner[0] - done.done.shift[0], corner[1] - done.done.shift[1]];
        let after = sheets[0][0].map(moved);
        assert!(dist(before, after) < 1e-6, "{before:?} → {after:?}");
    }
}
