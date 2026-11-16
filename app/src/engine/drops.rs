//! Standard drops (SD1–SD9): the leftover past the parts is cut into remnants
//! of the shop's standard sizes, so the rack only holds sizes the shop uses,
//! instead of one remnant of whatever is left.
//!
//! Works on the strip and L shapes of the automatic remnant line
//! (`remline::strip_region`). In the region's frame the parts sit at the low
//! X and Y side; the strip runs from the automatic line's place to the far
//! sheet edge. Drops are laid in up to two rows along the strip, one against
//! each long sheet edge, filled from the far edge in; an L's leg gets one row
//! against the sheet edge. Every cut starts at a sheet edge and ends at a
//! sheet edge or on another cut, and everything not in a drop stays joined to
//! the skeleton.

use crate::cutoff::{CutKind, CutLine};
use crate::nest::{Part, Placed, SheetFormat, SheetSpec, P};
use crate::remline::{StripRegion, TOL};
use serde::{Deserialize, Serialize};

/// The shop's standard drop sizes (Sheet Remnants options).
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DropSettings {
    /// Cut the leftover into standard drops (off: one remnant of whatever is
    /// left, as before).
    pub on: bool,
    /// The sizes, mm, either way round.
    pub sizes: Vec<[f64; 2]>,
    /// When no standard drop fits, keep the leftover as one remnant (off: it
    /// goes with the skeleton).
    pub keep_odd: bool,
}

/// Two drops filling the strip's width between them share one cut when less
/// than this (mm) would be left between their cuts, rather than leave a
/// sliver (SD4).
pub const SHARED_CUT_GAP: f64 = 1.0;
/// At most this many layouts are offered.
pub const MAX_LAYOUTS: usize = 8;
/// Search budget per row for the most-length fill.
const SEARCH_NODES: usize = 20_000;

/// One way of cutting the leftover into drops.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    /// Each drop as cut, sheet coordinates (x0, y0, x1, y1), and its size in
    /// the list (long side, short side).
    pub drops: Vec<([f64; 4], [f64; 2])>,
    /// The cuts, in the order they are cut (SD5).
    pub lines: Vec<CutLine>,
    /// The leftover's area (mm²): what the drops are cut from.
    pub free_mm2: f64,
    /// The first layout: what Cut off leftover makes (biggest drops first, SD2).
    pub auto: bool,
}

impl Layout {
    /// Drop area, mm².
    pub fn drop_mm2(&self) -> f64 {
        self.drops.iter().map(|(r, _)| (r[2] - r[0]) * (r[3] - r[1])).sum()
    }

    /// How much of the leftover the drops use (0..1).
    pub fn used(&self) -> f64 {
        if self.free_mm2 > 0.0 { (self.drop_mm2() / self.free_mm2).min(1.0) } else { 0.0 }
    }

    /// The drops by size, biggest first: (size, count).
    pub fn counts(&self) -> Vec<([f64; 2], usize)> {
        let mut v: Vec<([f64; 2], usize)> = vec![];
        for (_, s) in &self.drops {
            match v.iter_mut().find(|(t, _)| same(*t, *s)) {
                Some(e) => e.1 += 1,
                None => v.push((*s, 1)),
            }
        }
        v.sort_by(|a, b| (b.0[0] * b.0[1]).total_cmp(&(a.0[0] * a.0[1])));
        v
    }

    /// "2 × 1524 × 1219, 1 × 610 × 610" in the unit.
    pub fn summary(&self, unit: crate::units::LenUnit) -> String {
        self.counts().iter().map(|(s, n)| format!("{n} × {}", unit.size(s[0], s[1], 0))).collect::<Vec<_>>().join(", ")
    }
}

fn same(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
}

/// The list's sizes as (long, short), positive, without repeats, biggest
/// first; sizes under the minimum area of remnants are left out (they would
/// be scrap, SD1).
pub fn clean_sizes(sizes: &[[f64; 2]], min_area_m2: f64) -> Vec<[f64; 2]> {
    let mut v: Vec<[f64; 2]> = sizes
        .iter()
        .filter(|s| s[0] > 0.0 && s[1] > 0.0 && s[0].is_finite() && s[1].is_finite())
        .map(|s| [s[0].max(s[1]), s[0].min(s[1])])
        .filter(|s| s[0] * s[1] >= min_area_m2 * 1e6 - 1e-6)
        .collect();
    v.sort_by(|a, b| (b[0] * b[1]).total_cmp(&(a[0] * a[1])).then(b[0].total_cmp(&a[0])));
    v.dedup_by(|a, b| same(*a, *b));
    v
}

/// A row of drops: `across` wide, each `len` long (and its list size).
#[derive(Clone, Debug, PartialEq, Default)]
struct Row {
    across: f64,
    drops: Vec<(f64, [f64; 2])>,
}

/// Drops on the strip's two long edges and the L's leg.
#[derive(Clone, Debug, PartialEq, Default)]
struct Plan {
    low: Row,
    high: Row,
    leg: Row,
}

impl Plan {
    fn area(&self) -> f64 {
        [&self.low, &self.high, &self.leg].iter().map(|r| r.drops.iter().map(|d| d.0 * r.across).sum::<f64>()).sum()
    }
    fn count(&self) -> usize {
        self.low.drops.len() + self.high.drops.len() + self.leg.drops.len()
    }
    /// The drop sizes, sorted, to tell layouts apart.
    fn key(&self) -> Vec<(i64, i64, u8)> {
        let mut v: Vec<(i64, i64, u8)> = [(&self.low, 0u8), (&self.high, 1), (&self.leg, 2)]
            .iter()
            .flat_map(|(r, k)| r.drops.iter().map(move |d| ((d.1[0] * 100.0).round() as i64, (d.1[1] * 100.0).round() as i64, *k)))
            .collect();
        v.sort();
        v
    }
}

/// The room a row has: `len` along it, from the far edge to the line's place.
/// Drops of `l` each take `l` + one cut; the last cut needs half a cut.
fn fits(used: f64, l: f64, len: f64, k: f64) -> bool {
    used + l + k - k / 2.0 <= len + 1e-6
}

/// The lengths that go with `across` (either way round).
fn lens_for(sizes: &[[f64; 2]], across: f64) -> Vec<(f64, [f64; 2])> {
    let mut v: Vec<(f64, [f64; 2])> = vec![];
    for s in sizes {
        for (l, a) in [(s[0], s[1]), (s[1], s[0])] {
            if (a - across).abs() < 1e-6 && !v.iter().any(|x| (x.0 - l).abs() < 1e-6) {
                v.push((l, *s));
            }
        }
    }
    v.sort_by(|a, b| b.0.total_cmp(&a.0));
    v
}

/// Biggest first: the longest drop that still fits, again and again (SD2).
fn fill_greedy(opts: &[(f64, [f64; 2])], len: f64, k: f64) -> Vec<(f64, [f64; 2])> {
    let mut out = vec![];
    let mut used = 0.0;
    while let Some(&d) = opts.iter().find(|d| fits(used, d.0, len, k)) {
        used += d.0 + k;
        out.push(d);
    }
    out
}

/// The fill that uses the most length (fewest drops on a tie), within a
/// search budget.
fn fill_best(opts: &[(f64, [f64; 2])], len: f64, k: f64) -> Vec<(f64, [f64; 2])> {
    fn go(opts: &[(f64, [f64; 2])], from: usize, used: f64, len: f64, k: f64, cur: &mut Vec<(f64, [f64; 2])>, best: &mut (f64, Vec<(f64, [f64; 2])>), nodes: &mut usize) {
        *nodes += 1;
        let total: f64 = cur.iter().map(|d| d.0).sum();
        if total > best.0 + 1e-6 || ((total - best.0).abs() <= 1e-6 && cur.len() < best.1.len()) {
            *best = (total, cur.clone());
        }
        if *nodes > SEARCH_NODES {
            return;
        }
        for i in from..opts.len() {
            if fits(used, opts[i].0, len, k) {
                cur.push(opts[i]);
                go(opts, i, used + opts[i].0 + k, len, k, cur, best, nodes);
                cur.pop();
            }
        }
    }
    let mut best = (0.0, fill_greedy(opts, len, k));
    best.0 = best.1.iter().map(|d| d.0).sum();
    let mut nodes = 0;
    go(opts, 0, 0.0, len, k, &mut vec![], &mut best, &mut nodes);
    best.1
}

/// The ways across a row can be: every side of every size up to `max`.
fn acrosses(sizes: &[[f64; 2]], max: f64) -> Vec<f64> {
    let mut v: Vec<f64> = sizes.iter().flat_map(|s| [s[0], s[1]]).filter(|a| *a <= max + 1e-6).collect();
    v.sort_by(|a, b| b.total_cmp(a));
    v.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    v
}

/// The room in the region: the strip's length and width, and the leg's
/// length (along the frame's Y) and greatest width.
#[derive(Clone, Copy, Debug)]
struct Room {
    strip_len: f64,
    strip_across: f64,
    leg: Option<(f64, f64)>,
    k: f64,
}

impl Room {
    fn new(r: &StripRegion, k: f64) -> Room {
        Room {
            strip_len: r.w - r.x,
            strip_across: r.h,
            // The leg's drops keep their side cut on the parts' side of the strip's line.
            leg: r.leg_y.map(|y| (r.h - y, r.x - k / 2.0)).filter(|l| l.0 > 0.0 && l.1 > 0.0),
            k,
        }
    }
    /// The high row's room once the low row is `low` wide (its cut shared
    /// when they fill the width, SD4).
    fn high_max(&self, low: f64) -> f64 {
        if low <= 0.0 { self.strip_across } else { self.strip_across - low }
    }
}

/// Biggest first (SD2): the biggest drop that fits sets the first row's
/// width, which is filled with the longest drops of that width that still
/// fit; then the same in what's left of the width, then the leg.
fn plan_greedy(sizes: &[[f64; 2]], room: &Room) -> Plan {
    let k = room.k;
    let row = |max_across: f64, len: f64| -> Row {
        let first = sizes
            .iter()
            .flat_map(|s| [(s[0], s[1]), (s[1], s[0])])
            .filter(|(l, a)| *a <= max_across + 1e-6 && fits(0.0, *l, len, k))
            .max_by(|x, y| (x.0 * x.1).total_cmp(&(y.0 * y.1)).then(x.1.total_cmp(&y.1)));
        match first {
            Some((_, a)) => Row { across: a, drops: fill_greedy(&lens_for(sizes, a), len, k) },
            None => Row::default(),
        }
    };
    let low = row(room.strip_across, room.strip_len);
    let high = if low.drops.is_empty() { Row::default() } else { row(room.high_max(low.across), room.strip_len) };
    let leg = room.leg.map(|(len, max)| row(max, len)).unwrap_or_default();
    Plan { low, high, leg }
}

/// One size only, the way round that gives the most drops.
fn plan_only(s: [f64; 2], room: &Room) -> Plan {
    let k = room.k;
    let mut best = Plan::default();
    for (l, a) in [(s[0], s[1]), (s[1], s[0])] {
        let opts = [(l, s)];
        let row = |max: f64, len: f64| if a <= max + 1e-6 { Row { across: a, drops: fill_greedy(&opts, len, k) } } else { Row::default() };
        let low = row(room.strip_across, room.strip_len);
        let high = if low.drops.is_empty() { Row::default() } else { row(room.high_max(low.across), room.strip_len) };
        let leg = room.leg.map(|(len, max)| {
            // The leg takes the size either way round.
            let a1 = Row { across: a, drops: if a <= max + 1e-6 { fill_greedy(&opts, len, k) } else { vec![] } };
            let a2 = Row { across: l, drops: if l <= max + 1e-6 { fill_greedy(&[(a, s)], len, k) } else { vec![] } };
            if a2.drops.len() > a1.drops.len() { a2 } else { a1 }
        });
        let p = Plan { low, high, leg: leg.unwrap_or_default() };
        if p.count() > best.count() {
            best = p;
        }
    }
    best
}

/// The most drop area: every width for each row, each filled for the most
/// length.
fn plan_most(sizes: &[[f64; 2]], room: &Room) -> Plan {
    let k = room.k;
    let best_row = |max: f64, len: f64| -> Row {
        acrosses(sizes, max)
            .into_iter()
            .map(|a| Row { across: a, drops: fill_best(&lens_for(sizes, a), len, k) })
            .max_by(|x, y| (x.drops.iter().map(|d| d.0).sum::<f64>() * x.across).total_cmp(&(y.drops.iter().map(|d| d.0).sum::<f64>() * y.across)).then(y.drops.len().cmp(&x.drops.len())))
            .unwrap_or_default()
    };
    let mut best = Plan::default();
    for a in acrosses(sizes, room.strip_across) {
        let low = Row { across: a, drops: fill_best(&lens_for(sizes, a), room.strip_len, k) };
        if low.drops.is_empty() {
            continue;
        }
        let high = best_row(room.high_max(a), room.strip_len);
        let p = Plan { low, high, leg: Row::default() };
        if p.area() > best.area() + 1e-6 || ((p.area() - best.area()).abs() <= 1e-6 && p.count() < best.count()) {
            best = p;
        }
    }
    best.leg = room.leg.map(|(len, max)| best_row(max, len)).unwrap_or_default();
    best
}

/// One size first, then biggest first in what's left.
fn plan_first(s: [f64; 2], sizes: &[[f64; 2]], room: &Room) -> Plan {
    let k = room.k;
    let mut best = Plan::default();
    for (l, a) in [(s[0], s[1]), (s[1], s[0])] {
        if a > room.strip_across + 1e-6 || !fits(0.0, l, room.strip_len, k) {
            continue;
        }
        let mut drops = vec![(l, s)];
        let rest = room.strip_len - (l + k);
        drops.extend(fill_greedy(&lens_for(sizes, a), rest + 0.0, k));
        let low = Row { across: a, drops };
        let g = plan_greedy(sizes, &Room { strip_across: room.high_max(a), ..*room });
        let p = Plan { low, high: g.low, leg: g.leg };
        if p.area() > best.area() {
            best = p;
        }
    }
    best
}

/// The drops as cut, in the region's frame: (x0, y0, x1, y1), list size.
fn place(plan: &Plan, r: &StripRegion, k: f64) -> Vec<([f64; 4], [f64; 2])> {
    let (w, h) = (r.w, r.h);
    let mut out = vec![];
    // Along the strip from the far edge in.
    let along = |row: &Row, y0: f64, y1: f64, out: &mut Vec<([f64; 4], [f64; 2])>| {
        let mut x1 = w;
        for d in &row.drops {
            out.push(([x1 - d.0, y0, x1, y1], d.1));
            x1 -= d.0 + k;
        }
    };
    if !plan.low.drops.is_empty() {
        along(&plan.low, 0.0, plan.low.across, &mut out);
    }
    if !plan.high.drops.is_empty() {
        let mut y0 = h - plan.high.across;
        let low_cut = if plan.low.drops.is_empty() { None } else { Some(plan.low.across + k / 2.0) };
        if let Some(c) = low_cut {
            // SD4: too little between the two rows' cuts — they share one.
            if y0 - k / 2.0 - c < SHARED_CUT_GAP {
                y0 = c + k / 2.0;
            }
        }
        along(&plan.high, y0, h, &mut out);
    }
    if !plan.leg.drops.is_empty() {
        // Down the leg from the far edge, against the sheet edge at x = 0.
        let mut y1 = h;
        for d in &plan.leg.drops {
            out.push(([0.0, y1 - d.0, plan.leg.across, y1], d.1));
            y1 -= d.0 + k;
        }
    }
    out
}

/// The cuts that free the drops, in the frame: each side of a drop not on the
/// sheet edge, half a cut out from it, joined into straight lines; ends on the
/// sheet edge run 2 × tolerance past it (as the automatic line's do).
fn cuts(drops: &[([f64; 4], [f64; 2])], w: f64, h: f64, k: f64) -> Vec<(P, P)> {
    let on = |v: f64, edge: f64| (v - edge).abs() <= TOL;
    let half = k / 2.0;
    // (vertical?, at, from, to)
    let mut segs: Vec<(bool, f64, f64, f64)> = vec![];
    for (r, _) in drops {
        let [x0, y0, x1, y1] = *r;
        let ext = |v: f64, lo: bool, edge_lo: f64, edge_hi: f64| if (lo && on(v, edge_lo)) || (!lo && on(v, edge_hi)) { v } else if lo { v - half } else { v + half };
        let (ya, yb) = (ext(y0, true, 0.0, h), ext(y1, false, 0.0, h));
        let (xa, xb) = (ext(x0, true, 0.0, w), ext(x1, false, 0.0, w));
        if !on(x0, 0.0) {
            segs.push((true, x0 - half, ya, yb));
        }
        if !on(x1, w) {
            segs.push((true, x1 + half, ya, yb));
        }
        if !on(y0, 0.0) {
            segs.push((false, y0 - half, xa, xb));
        }
        if !on(y1, h) {
            segs.push((false, y1 + half, xa, xb));
        }
    }
    // Join the pieces on one line that overlap or touch.
    segs.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.total_cmp(&b.2)));
    let mut joined: Vec<(bool, f64, f64, f64)> = vec![];
    for s in segs {
        match joined.last_mut() {
            Some(j) if j.0 == s.0 && (j.1 - s.1).abs() < 1e-6 && s.2 <= j.3 + 1e-6 => j.3 = j.3.max(s.3),
            _ => joined.push(s),
        }
    }
    let e = 2.0 * TOL;
    joined
        .into_iter()
        .map(|(vert, at, a, b)| {
            let lim = if vert { h } else { w };
            let a = if on(a, 0.0) { -e } else { a };
            let b = if on(b, lim) { lim + e } else { b };
            if vert { ([at, a], [at, b]) } else { ([a, at], [b, at]) }
        })
        .collect()
}

/// A layout from a plan: the drops and their cuts on the sheet, each cut
/// starting at its sheet-edge end, in cutting order.
fn layout(plan: &Plan, r: &StripRegion, k: f64, auto: bool) -> Layout {
    let placed = place(plan, r, k);
    let drops: Vec<([f64; 4], [f64; 2])> = placed
        .iter()
        .map(|(b, s)| {
            let (p, q) = (r.to_sheet([b[0], b[1]]), r.to_sheet([b[2], b[3]]));
            ([p[0].min(q[0]), p[1].min(q[1]), p[0].max(q[0]), p[1].max(q[1])], *s)
        })
        .collect();
    let (sw, sh) = if r.transpose { (r.h, r.w) } else { (r.w, r.h) };
    let at_edge = |p: P| p[0] <= TOL * 3.0 || p[1] <= TOL * 3.0 || p[0] >= sw - TOL * 3.0 || p[1] >= sh - TOL * 3.0;
    let lines: Vec<CutLine> = cuts(&placed, r.w, r.h, k)
        .into_iter()
        .map(|(a, b)| {
            let (a, b) = (r.to_sheet(a), r.to_sheet(b));
            let pts = if at_edge(a) || !at_edge(b) { vec![a, b] } else { vec![b, a] };
            CutLine { pts, kind: CutKind::Remnant }
        })
        .collect();
    let order = crate::cutoff::stops_first(&lines);
    let lines = order.into_iter().map(|i| lines[i].clone()).collect();
    let free_mm2 = (r.w - r.x) * r.h + r.leg_y.map_or(0.0, |y| r.x * (r.h - y));
    Layout { drops, lines, free_mm2, auto }
}

/// Every way offered of cutting the region into drops (SD3): biggest first
/// (the automatic one) first, then one size only for each size, one size
/// first, and the most drop area; the same drops only once, at most
/// MAX_LAYOUTS. Empty when no drop fits.
pub fn layouts_in(r: &StripRegion, sizes: &[[f64; 2]], k: f64) -> Vec<Layout> {
    let room = Room::new(r, k);
    let mut plans: Vec<Plan> = vec![plan_greedy(sizes, &room)];
    plans.push(plan_most(sizes, &room));
    for s in sizes {
        plans.push(plan_only(*s, &room));
        plans.push(plan_first(*s, sizes, &room));
    }
    // Each with one row only too, for when a second row's shared cut would
    // leave its drops under the minimum area (SD4, SD5).
    let one_row: Vec<Plan> = plans.iter().filter(|p| !p.high.drops.is_empty()).map(|p| Plan { high: Row::default(), ..p.clone() }).collect();
    plans.extend(one_row);
    let mut seen: Vec<Vec<(i64, i64, u8)>> = vec![];
    let mut kept: Vec<Plan> = vec![];
    for p in plans {
        if p.count() == 0 || seen.contains(&p.key()) {
            continue;
        }
        seen.push(p.key());
        kept.push(p);
    }
    // After the automatic one: the most drop area first, then fewer drops.
    if kept.len() > 2 {
        kept[1..].sort_by(|a, b| b.area().total_cmp(&a.area()).then(a.count().cmp(&b.count())));
    }
    let auto_first = kept.first().map(|p| *p == plan_greedy(sizes, &room)).unwrap_or(false);
    kept.into_iter().enumerate().map(|(i, p)| layout(&p, r, k, i == 0 && auto_first)).collect()
}

/// SD5: the remnants this layout's cuts would leave on the sheet — its real
/// outline, with its parts — are exactly its drops, each at least the minimum
/// area of remnants. Cuts that happen to line up can close off an odd piece,
/// a shared cut can take a drop under the minimum, and a shaped remnant can
/// lack material where a drop would be: such a layout isn't offered.
fn leaves_only_drops(l: &Layout, parts: &[Part], sheet: &[Placed], f: &SheetFormat, k: f64, min_area_m2: f64) -> bool {
    let area = |d: &[f64; 4]| (d[2] - d[0]) * (d[3] - d[1]);
    if l.drops.iter().any(|(d, _)| area(d) < min_area_m2 * 1e6 - 1e-6) {
        return false;
    }
    // Every piece of 1 mm² or more counts.
    let pieces = crate::remnant::sheet_remnants(parts, sheet, f, &l.lines, k, 1e-6, false);
    let matches = |o: &crate::stock::Outline, d: &[f64; 4]| {
        let (lo, hi) = o.outer.iter().fold(([f64::MAX; 2], [f64::MIN; 2]), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])]));
        (o.area_mm2() - area(d)).abs() < 1.0 && [lo[0] - d[0], lo[1] - d[1], hi[0] - d[2], hi[1] - d[3]].iter().all(|e| e.abs() < 0.05)
    };
    pieces.len() == l.drops.len() && l.drops.iter().all(|(d, _)| pieces.iter().any(|o| matches(o, d)))
}

/// What Cut off leftover would do with standard drops on this sheet.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// The shape isn't a strip or an L: the usual line instead (SD7).
    NotStrip,
    /// No leftover past the parts wide enough for a remnant.
    NoRoom,
    /// The leftover is there but no standard drop fits it: (length, width) of
    /// the leftover strip (SD6).
    NoneFits(f64, f64),
}

/// The layouts for a sheet (`f` its format: a remnant's real outline) with
/// the shop's sizes, or why there are none. Only layouts that leave exactly
/// their drops (SD5); the first is what Cut off leftover makes.
pub fn layouts(parts: &[Part], sheet: &[Placed], f: &SheetFormat, spec: &SheetSpec, line: &crate::remline::LineSettings, sizes: &[[f64; 2]], min_area_m2: f64) -> Result<Vec<Layout>, Outcome> {
    if !line.shape.uses_min_xy() || line.shape == crate::remline::LineShape::OneStep {
        return Err(Outcome::NotStrip);
    }
    let kerf = crate::kerf::kerf_mm(spec);
    let r = crate::remline::strip_region(sheet, spec, line, kerf / 2.0).ok_or(Outcome::NoRoom)?;
    let v: Vec<Layout> = layouts_in(&r, &clean_sizes(sizes, min_area_m2), kerf)
        .into_iter()
        .filter(|l| leaves_only_drops(l, parts, sheet, f, kerf, min_area_m2))
        .take(MAX_LAYOUTS)
        .enumerate()
        .map(|(i, l)| Layout { auto: i == 0, ..l })
        .collect();
    if v.is_empty() {
        return Err(Outcome::NoneFits(r.w - r.x, r.h));
    }
    Ok(v)
}

/// Cut off leftover with the shop's settings (SD2, SD6, SD7): with standard
/// drops on, the biggest-first drops; otherwise — or when the shape isn't a
/// strip or an L, or no drop fits and odd remnants are kept — the usual
/// automatic line. Ok(lines, what was made) or Err(why nothing).
pub fn auto_cut(parts: &[Part], sheet: &[Placed], f: &SheetFormat, spec: &SheetSpec, st: &crate::stock::StockSettings, unit: crate::units::LenUnit) -> Result<(Vec<CutLine>, String), String> {
    let ls = &st.line;
    let usual = || {
        let lines = crate::remline::auto_lines(parts, sheet, spec, ls, crate::kerf::kerf_mm(spec) / 2.0);
        if lines.is_empty() { Err(format!("No remnant: less than the minimum width ({}) is left past the parts.", unit.len(ls.min_width, 0))) } else { Ok((lines, ls.shape.label().to_string())) }
    };
    if !st.drops.on {
        return usual();
    }
    match layouts(parts, sheet, f, spec, ls, &st.drops.sizes, st.min_remnant_m2) {
        Ok(v) => Ok((v[0].lines.clone(), format!("Standard drops, {}", v[0].summary(unit)))),
        Err(Outcome::NotStrip) => usual().map(|(l, what)| (l, format!("{what} (standard drops work with the strip and L shapes only)"))),
        Err(Outcome::NoneFits(..)) if st.drops.keep_odd => usual().map(|(l, what)| (l, format!("{what} (no standard drop fits, so the leftover is one remnant)"))),
        Err(Outcome::NoneFits(l, w)) => Err(format!("No standard drop fits the {} left past the parts — it goes with the skeleton.", unit.size(l, w, 0))),
        Err(_) => usual(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remline::{LineSettings, LineShape};

    const IN: f64 = 25.4;

    fn region(w: f64, h: f64, x: f64) -> StripRegion {
        StripRegion { transpose: false, mirror_x: false, mirror_y: false, w, h, x, leg_y: None }
    }

    fn sz(a: f64, b: f64) -> [f64; 2] {
        [a * IN, b * IN]
    }

    fn names(l: &Layout) -> String {
        l.counts().iter().map(|(s, n)| format!("{n} × {:.0} × {:.0}", s[0] / IN, s[1] / IN)).collect::<Vec<_>>().join(", ")
    }

    /// A cut that stops on the middle of another is cut before it (SD5).
    fn stops_come_first(lines: &[CutLine]) {
        for (i, a) in lines.iter().enumerate() {
            for (j, b) in lines.iter().enumerate() {
                let end = a.pts[1];
                let mid = crate::cutoff::polyline_dist(end, &b.pts) <= 0.01 && [b.pts[0], b.pts[1]].iter().all(|e| (e[0] - end[0]).hypot(e[1] - end[1]) > 0.01);
                assert!(i == j || !mid || i < j, "cut {i} stops on cut {j} but comes after it: {lines:?}");
            }
        }
    }

    /// SD2: 100 × 60 in left past the parts, the biggest drop first.
    #[test]
    fn biggest_first_then_the_others() {
        let sizes = clean_sizes(&[sz(96.0, 60.0), sz(48.0, 60.0), sz(24.0, 60.0), sz(24.0, 24.0)], 0.1);
        // Sheet 160 in long, the parts' line at 60 in: 100 in left.
        let v = layouts_in(&region(160.0 * IN, 60.0 * IN, 60.0 * IN), &sizes, 0.3);
        assert!(v[0].auto);
        assert_eq!(names(&v[0]), "1 × 96 × 60");
        let all: Vec<String> = v.iter().map(names).collect();
        assert!(all.contains(&"2 × 60 × 48".to_string()), "{all:?}");
        assert!(all.contains(&"4 × 60 × 24".to_string()), "{all:?}");
        // 24 × 24 fill two rows of four (12 in of the width left between).
        assert!(all.contains(&"8 × 24 × 24".to_string()), "{all:?}");
    }

    /// SD1: each drop is cut to its size, the cut half a cut outside it.
    #[test]
    fn drops_are_exact_and_cut_from_the_far_edge() {
        let k = 0.3;
        let sizes = clean_sizes(&[sz(48.0, 60.0)], 0.1);
        let v = layouts_in(&region(160.0 * IN, 60.0 * IN, 60.0 * IN), &sizes, k);
        let l = &v[0];
        assert_eq!(l.drops.len(), 2);
        let (a, b) = (l.drops[0].0, l.drops[1].0);
        assert!((a[2] - 160.0 * IN).abs() < 1e-9 && (a[2] - a[0] - 48.0 * IN).abs() < 1e-9 && (a[3] - a[1] - 60.0 * IN).abs() < 1e-9);
        assert!((a[0] - b[2] - k).abs() < 1e-9, "one cut between them");
        // Two cuts, both edge to edge across the strip.
        assert_eq!(l.lines.len(), 2);
        for c in &l.lines {
            assert!(c.pts[0][1] < 0.0 && c.pts[1][1] > 60.0 * IN, "{:?}", c.pts);
        }
        assert!((l.lines.iter().map(|c| c.pts[0][0]).fold(f64::MAX, f64::min) - (b[0] - k / 2.0)).abs() < 1e-9);
    }

    /// SD4: two rows of 24 in fill a 48 in strip with one shared cut; the
    /// cross cuts stop on it.
    #[test]
    fn stacked_drops_share_the_cut_between_rows() {
        let k = 0.3;
        let sizes = clean_sizes(&[sz(24.0, 24.0)], 0.1);
        let v = layouts_in(&region(96.0 * IN, 48.0 * IN, 40.0 * IN), &sizes, k);
        let l = &v[0];
        assert_eq!(l.drops.len(), 4, "{}", names(l));
        let horiz: Vec<&CutLine> = l.lines.iter().filter(|c| (c.pts[0][1] - c.pts[1][1]).abs() < 1e-9).collect();
        assert_eq!(horiz.len(), 1, "one cut between the rows");
        let y = horiz[0].pts[0][1];
        assert!((y - (24.0 * IN + k / 2.0)).abs() < 1e-9);
        // It starts at the far edge and stops at the near drops' side cut.
        assert!(horiz[0].pts[0][0] > 96.0 * IN);
        // Every cut starts on the sheet edge; the ones that stop meet another cut.
        for (i, c) in l.lines.iter().enumerate() {
            assert!(c.pts[0][0] > 96.0 * IN - 1e-6 || c.pts[0][1] < 0.0 || c.pts[0][1] > 48.0 * IN, "starts on the edge: {:?}", c.pts);
            let end = c.pts[1];
            let edge = end[0] > 96.0 * IN || end[1] < 0.0 || end[1] > 48.0 * IN;
            assert!(edge || crate::cutoff::meets(end, i, &l.lines).is_some(), "stops on a cut: {:?}", c.pts);
        }
        stops_come_first(&l.lines);
        // The top row is short by the cut.
        let top = l.drops.iter().find(|d| d.0[3] > 47.0 * IN).unwrap().0;
        assert!((top[3] - top[1] - (24.0 * IN - k)).abs() < 1e-9);
    }

    /// SD3: the remnants the cuts leave are exactly the drops; the rest stays
    /// with the skeleton.
    #[test]
    fn the_cuts_leave_exactly_the_drops() {
        use crate::nest::{Shape, SheetFormat};
        let k = 0.3;
        let parts = vec![Part { name: "A".into(), shape: Shape::Rectangle, w: 1000.0, h: 900.0, qty: 1, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let sheet = vec![Placed { part: 0, x: 5.0, y: 5.0, w: 1000.0, h: 900.0, turns: 0, mirror: false, spin: Default::default() }];
        let sizes = clean_sizes(&[[600.0, 600.0], [600.0, 400.0]], 0.1);
        let r = region(3000.0, 1500.0, 1015.0);
        for l in layouts_in(&r, &sizes, k) {
            let got = crate::remnant::sheet_remnants(&parts, &sheet, &SheetFormat::rect(3000.0, 1500.0), &l.lines, k, 0.01, false);
            assert_eq!(got.len(), l.drops.len(), "{}: {got:?}", names(&l));
            for (d, _) in &l.drops {
                let a = (d[2] - d[0]) * (d[3] - d[1]);
                assert!(got.iter().any(|o| (o.area_mm2() - a).abs() < 1.0 && o.outer.len() == 4), "{d:?} in {}", names(&l));
            }
        }
    }

    /// SD3: an L's leg gets drops too, against the sheet edge.
    #[test]
    fn an_l_fills_its_leg() {
        let sizes = clean_sizes(&[[500.0, 400.0]], 0.1);
        let r = StripRegion { leg_y: Some(900.0), ..region(3000.0, 1500.0, 2000.0) };
        let l = &layouts_in(&r, &sizes, 0.3)[0];
        let leg: Vec<_> = l.drops.iter().filter(|d| d.0[2] < 2000.0).collect();
        assert!(!leg.is_empty(), "{:?}", l.drops);
        assert!(leg.iter().all(|d| d.0[0].abs() < 1e-9 && d.0[1] >= 900.0));
    }

    /// The frame turns back onto the sheet: a strip on the left of a sheet
    /// whose parts are on the right.
    #[test]
    fn a_mirrored_strip_lands_on_the_far_side() {
        let r = StripRegion { mirror_x: true, ..region(3000.0, 1500.0, 1000.0) };
        let l = &layouts_in(&r, &clean_sizes(&[[1500.0, 1000.0]], 0.1), 0.3)[0];
        assert_eq!(l.drops.len(), 1);
        assert!(l.drops[0].0[0].abs() < 1e-9 && (l.drops[0].0[2] - 1000.0).abs() < 1e-9, "{:?}", l.drops);
        // The cut starts on the sheet's bottom edge.
        assert!(l.lines.iter().all(|c| c.pts[0][1] < 0.0));
    }

    /// SD6, SD7: nothing fits, other shapes, and drops off.
    #[test]
    fn what_cut_off_leftover_does_with_drops() {
        let spec = SheetSpec { w: 3000.0, h: 1500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: None, pick: Default::default() };
        let parts = vec![Part { name: "A".into(), shape: crate::nest::Shape::Rectangle, w: 2500.0, h: 1400.0, qty: 1, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let sheet = vec![Placed { part: 0, x: 5.0, y: 5.0, w: 2500.0, h: 1400.0, turns: 0, mirror: false, spin: Default::default() }];
        let mut st = crate::stock::StockSettings { line: LineSettings { shape: LineShape::VerticalStrip, ..Default::default() }, ..Default::default() };
        st.drops = DropSettings { on: true, sizes: vec![[600.0, 600.0]], keep_odd: false };
        // About 485 mm left: no 600 drop fits.
        let e = auto_cut(&parts, &sheet, &SheetFormat::rect(3000.0, 1500.0), &spec, &st, crate::units::LenUnit::Mm).unwrap_err();
        assert!(e.contains("No standard drop fits") && e.contains("skeleton"), "{e}");
        st.drops.keep_odd = true;
        assert_eq!(auto_cut(&parts, &sheet, &SheetFormat::rect(3000.0, 1500.0), &spec, &st, crate::units::LenUnit::Mm).unwrap().0.len(), 1);
        st.drops = DropSettings { on: true, sizes: vec![[450.0, 1500.0], [400.0, 400.0]], keep_odd: false };
        let (lines, what) = auto_cut(&parts, &sheet, &SheetFormat::rect(3000.0, 1500.0), &spec, &st, crate::units::LenUnit::Mm).unwrap();
        assert!(what.starts_with("Standard drops, 1 × 1500 × 450"), "{what}");
        assert_eq!(lines.len(), 1, "one 450 × 1500 drop: {lines:?}");
        st.line.shape = LineShape::Staircase;
        assert!(auto_cut(&parts, &sheet, &SheetFormat::rect(3000.0, 1500.0), &spec, &st, crate::units::LenUnit::Mm).unwrap().1.contains("strip and L shapes only"));
        st.drops.on = false;
        st.line.shape = LineShape::VerticalStrip;
        assert_eq!(auto_cut(&parts, &sheet, &SheetFormat::rect(3000.0, 1500.0), &spec, &st, crate::units::LenUnit::Mm).unwrap().1, "Vertical strip");
    }

    fn plate(w: f64, h: f64) -> Part {
        Part { name: "A".into(), shape: crate::nest::Shape::Rectangle, w, h, qty: 1, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
    }

    /// SD5: a layout whose cuts line up and close off an odd piece, or whose
    /// shared cut takes a drop under the minimum area, isn't offered.
    #[test]
    fn layouts_that_would_store_odd_pieces_are_left_out() {
        let spec = |w: f64, h: f64| SheetSpec { w, h, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: None, pick: Default::default() };
        let k = crate::kerf::kerf_mm(&spec(3000.0, 1500.0));
        // An L: the strip from x 1599.55, the leg from y 900; the leg's
        // 1599.4 wide drop puts its side cut on the strip's line.
        let parts = vec![plate(1584.55, 885.0)];
        let sheet = vec![Placed { part: 0, x: 5.0, y: 5.0, w: 1584.55, h: 885.0, turns: 0, mirror: false, spin: Default::default() }];
        let line = LineSettings { shape: LineShape::LVerticalFirst, ..Default::default() };
        let f = SheetFormat::rect(3000.0, 1500.0);
        let sizes = [[1000.0, 700.0], [1000.0, 500.0], [1599.4, 500.0]];
        let r = crate::remline::strip_region(&sheet, &spec(3000.0, 1500.0), &line, k / 2.0).unwrap();
        assert!((r.x - 1599.55).abs() < 1e-9 && r.leg_y == Some(900.0), "{r:?}");
        let all = layouts_in(&r, &clean_sizes(&sizes, 0.1), k);
        assert!(all.iter().any(|l| !leaves_only_drops(l, &parts, &sheet, &f, k, 0.1)), "the case to guard against");
        let v = layouts(&parts, &sheet, &f, &spec(3000.0, 1500.0), &line, &sizes, 0.1).unwrap();
        for l in &v {
            assert_eq!(crate::remnant::sheet_remnants(&parts, &sheet, &f, &l.lines, k, 0.1, false).len(), l.drops.len(), "{}", names(l));
        }
        assert!(v[0].auto);
        // Two 500 wide rows in a 1000 wide strip: the second row's 500 × 200
        // drops would come out under 0.1 m².
        let parts = vec![plate(1000.0, 900.0)];
        let sheet = vec![Placed { part: 0, x: 5.0, y: 5.0, w: 1000.0, h: 900.0, turns: 0, mirror: false, spin: Default::default() }];
        let line = LineSettings { shape: LineShape::VerticalStrip, ..Default::default() };
        let f = SheetFormat::rect(3000.0, 1000.0);
        let v = layouts(&parts, &sheet, &f, &spec(3000.0, 1000.0), &line, &[[500.0, 200.0]], 0.1).unwrap();
        assert!(v[0].drops.len() >= 9, "one row of 500 × 200 still fits: {}", names(&v[0]));
        for l in &v {
            assert!(l.drops.iter().all(|(d, _)| (d[2] - d[0]) * (d[3] - d[1]) >= 0.1e6 - 1e-6), "{}", names(l));
            assert_eq!(crate::remnant::sheet_remnants(&parts, &sheet, &f, &l.lines, k, 0.1, false).len(), l.drops.len());
        }
    }
}
