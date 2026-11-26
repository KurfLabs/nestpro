//! Modify → Break elements on a part's own drawing (MECHANICS BE1–BE13):
//! break an element at a point, cut away the piece of an element past where
//! it crosses a base element (Cut 1, Cut several — the overhang a drawing
//! leaves where a line runs past the one it should stop on), join two
//! crossing elements into a corner, and cut a bridge of a given width across
//! two elements. The Part view's Modify group and agents' `break-elements`
//! both call these, with picks as points on the part.
//!
//! The tools work on the part's contour list (`dxf::Contour`, CS1): each
//! contour's path is split into elements at its corners and at points where
//! a straight run was broken (BE2). After an edit the loose ends the edit
//! made are joined again, a chain that closes on itself becomes a closed
//! contour, and the outline, holes and open lines are numbered again (BE9).
//! An edit that would leave the part without a closed outline, or in
//! separate pieces, is refused and the part stays as it was (BE10).

use crate::cnc::ManualCut;
use crate::dxf::{self, Contour, ContourState, CustomGeo};
use crate::nest::{oriented, Part, Placed, Shape, ELEMENT_CORNER_DEG, P};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Ends closer than this are one point (BE9; the importer's join tolerance).
const JOIN: f64 = dxf::JOIN_TOL;
/// BE2: a point where the path turns less than this (degrees) ends an
/// element: a straight run drawn as two pieces, or broken with Break, is two
/// elements. Arcs drawn as short segments turn far more at each point.
pub const STRAIGHT_DEG: f64 = 1e-4;
/// How near a pick must be to an element (mm) when an agent doesn't say
/// (BE1). The Part view uses its capture square instead.
pub const PICK_TOL: f64 = 1.0;

fn dist(a: P, b: P) -> f64 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
}

fn cross(a: P, b: P) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}

/// How far the path turns at `v`, coming from `a` and going on to `b` (degrees).
fn turn_deg(a: P, v: P, b: P) -> f64 {
    let (u, w) = (sub(v, a), sub(b, v));
    let (lu, lw) = ((u[0] * u[0] + u[1] * u[1]).sqrt(), (w[0] * w[0] + w[1] * w[1]).sqrt());
    if lu < 1e-12 || lw < 1e-12 {
        return 0.0;
    }
    ((u[0] * w[0] + u[1] * w[1]) / (lu * lw)).clamp(-1.0, 1.0).acos().to_degrees()
}

/// BE2: does an element end at `v`? At a corner (over 20°, as everywhere
/// elements are picked: E1) or where a straight run was broken.
fn element_end(a: P, v: P, b: P) -> bool {
    let t = turn_deg(a, v, b);
    t > ELEMENT_CORNER_DEG || t < STRAIGHT_DEG
}

/// One contour's path as the tools measure along it: an open line from its
/// start; a closed contour from its first element end round and back to it
/// (its first point repeated at the end).
struct Track {
    pts: Vec<P>,
    /// Length along the path to each point.
    at: Vec<f64>,
    closed: bool,
}

impl Track {
    fn new(c: &Contour) -> Track {
        let mut pts = c.pts.clone();
        pts.dedup_by(|a, b| dist(*a, *b) < 1e-9);
        if c.closed {
            while pts.len() > 1 && dist(pts[0], pts[pts.len() - 1]) < 1e-9 {
                pts.pop();
            }
            let n = pts.len();
            if n >= 3 {
                let first = (0..n).find(|&i| element_end(pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n])).unwrap_or(0);
                pts.rotate_left(first);
            }
            if let Some(&p0) = pts.first() {
                pts.push(p0);
            }
        }
        let mut at = vec![0.0; pts.len()];
        for i in 1..pts.len() {
            at[i] = at[i - 1] + dist(pts[i - 1], pts[i]);
        }
        Track { pts, at, closed: c.closed }
    }

    fn len(&self) -> f64 {
        self.at.last().copied().unwrap_or(0.0)
    }

    /// The point `s` along the path.
    fn point(&self, s: f64) -> P {
        let n = self.pts.len();
        if n == 0 {
            return [0.0, 0.0];
        }
        for i in 1..n {
            if s <= self.at[i] || i == n - 1 {
                let l = self.at[i] - self.at[i - 1];
                let t = if l > 0.0 { ((s - self.at[i - 1]) / l).clamp(0.0, 1.0) } else { 0.0 };
                let (a, b) = (self.pts[i - 1], self.pts[i]);
                return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            }
        }
        self.pts[0]
    }

    /// The path from `a` to `b` along it (a < b).
    fn between(&self, a: f64, b: f64) -> Vec<P> {
        let mut out = vec![self.point(a)];
        for i in 0..self.pts.len() {
            if self.at[i] > a + 1e-9 && self.at[i] < b - 1e-9 {
                out.push(self.pts[i]);
            }
        }
        out.push(self.point(b));
        out.dedup_by(|x, y| dist(*x, *y) < 1e-9);
        out
    }

    /// The nearest point of segments `i0..i1` to `p`: (distance, length along).
    fn nearest(&self, i0: usize, i1: usize, p: P) -> (f64, f64) {
        let mut best = (f64::MAX, self.at[i0]);
        for i in i0..i1 {
            let (a, b) = (self.pts[i], self.pts[i + 1]);
            let ab = sub(b, a);
            let l2 = ab[0] * ab[0] + ab[1] * ab[1];
            let t = if l2 > 0.0 { (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0) } else { 0.0 };
            let q = [a[0] + ab[0] * t, a[1] + ab[1] * t];
            let d = dist(p, q);
            if d < best.0 {
                best = (d, self.at[i] + t * l2.sqrt());
            }
        }
        best
    }
}

/// One element the tools pick (BE2): segments `i0..i1` of contour
/// `contour`'s track. `ring`: a closed contour with no element end at all (a
/// circle), whose one element runs all the way round.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Elem {
    contour: usize,
    i0: usize,
    i1: usize,
    ring: bool,
}

/// Every contour's track and the elements on it. Left-out contours aren't on
/// the part, so their lines can't be picked or cut against (BE1).
struct Drawing {
    list: Vec<Contour>,
    tracks: Vec<Track>,
    elems: Vec<Elem>,
    /// Messages for the Part view, which shows no contour numbers: lines
    /// are named by what they are and points aren't given (BE1). Agents get
    /// numbers and points.
    view: bool,
}

impl Drawing {
    fn new(list: Vec<Contour>) -> Drawing {
        let tracks: Vec<Track> = list.iter().map(Track::new).collect();
        let mut elems = vec![];
        for (k, t) in tracks.iter().enumerate() {
            if list[k].state == ContourState::LeaveOut || t.pts.len() < 2 {
                continue;
            }
            let m = t.pts.len() - 1;
            let mut ends = vec![0];
            ends.extend((1..m).filter(|&i| element_end(t.pts[i - 1], t.pts[i], t.pts[i + 1])));
            ends.push(m);
            let ring = t.closed && {
                // Round the start too: is the first point an element end?
                let n = m;
                n >= 3 && ends.len() == 2 && !element_end(t.pts[n - 1], t.pts[0], t.pts[1])
            };
            for w in ends.windows(2) {
                elems.push(Elem { contour: k, i0: w[0], i1: w[1], ring });
            }
        }
        Drawing { list, tracks, elems, view: false }
    }

    /// Contour `k` as a message names it: "contour 3" for agents; in the
    /// Part view by what it is ("the outline", "the round hole", "the open
    /// line").
    fn name(&self, k: usize) -> String {
        if !self.view {
            return format!("contour {}", k + 1);
        }
        let c = &self.list[k];
        if Some(k) == dxf::outline_index(&self.list) {
            "the outline".into()
        } else if !c.closed {
            "the open line".into()
        } else if c.state != ContourState::Cut {
            format!("the {} contour", c.state.label().to_lowercase())
        } else if self.elems.iter().any(|e| e.contour == k && e.ring) {
            "the round hole".into()
        } else {
            "the hole".into()
        }
    }

    /// " at x, y" for agents; nothing in the Part view, which marks the spot.
    fn place(&self, q: P) -> String {
        if self.view {
            String::new()
        } else {
            format!(" at {}", at(q))
        }
    }

    /// The element nearest `p` within `tol` mm, and how far along its
    /// contour's track `p` falls (BE1).
    fn pick(&self, p: P, tol: f64, what: &str) -> Result<(Elem, f64), String> {
        let mut best: Option<(Elem, f64, f64)> = None;
        for e in &self.elems {
            let (d, s) = self.tracks[e.contour].nearest(e.i0, e.i1, p);
            if d <= tol && best.is_none_or(|b| d < b.1 - 1e-9) {
                best = Some((*e, d, s));
            }
        }
        best.map(|b| (b.0, b.2)).ok_or_else(|| format!("No line, arc or circle of the part within {} mm of {what} ({}, {}): pick on the element itself (part-info gives the part's contours).", num(tol), num(p[0]), num(p[1])))
    }

    /// Element `e` and its neighbours on the same straight line: a straight
    /// edge drawn in pieces, or broken with Break, is one line for the Cut
    /// base and for telling picks apart (BE4, BE14).
    fn straight_run(&self, e: Elem) -> Vec<Elem> {
        let t = &self.tracks[e.contour];
        if e.ring || t.pts.len() < 3 {
            return vec![e];
        }
        let m = t.pts.len() - 1;
        let straight_at = |i: usize| -> bool {
            let (a, b) = if i == 0 || i == m {
                if !t.closed {
                    return false;
                }
                (t.pts[m - 1], t.pts[1])
            } else {
                (t.pts[i - 1], t.pts[i + 1])
            };
            turn_deg(a, t.pts[i], b) < STRAIGHT_DEG
        };
        let same: Vec<Elem> = self.elems.iter().copied().filter(|x| x.contour == e.contour).collect();
        let mut run = vec![e];
        let mut cur = e;
        while straight_at(cur.i1) {
            let at = if cur.i1 == m { 0 } else { cur.i1 };
            match same.iter().find(|x| x.i0 == at) {
                Some(&n) if !run.contains(&n) => {
                    run.push(n);
                    cur = n;
                }
                _ => break,
            }
        }
        cur = e;
        while straight_at(cur.i0) {
            let at = if cur.i0 == 0 { m } else { cur.i0 };
            match same.iter().find(|x| x.i1 == at) {
                Some(&p) if !run.contains(&p) => {
                    run.insert(0, p);
                    cur = p;
                }
                _ => break,
            }
        }
        run
    }

    /// Like `pick`, but an element already picked (`avoid`) only counts when
    /// no other element is within reach (BE14): the second click of Cut,
    /// Join and Bridges lands on the other line where the two meet.
    fn pick_other(&self, p: P, tol: f64, what: &str, avoid: &[Elem]) -> Result<(Elem, f64), String> {
        let mut best: Option<(Elem, f64, f64)> = None;
        for e in self.elems.iter().filter(|e| !avoid.contains(e)) {
            let (d, s) = self.tracks[e.contour].nearest(e.i0, e.i1, p);
            if d <= tol && best.is_none_or(|b| d < b.1 - 1e-9) {
                best = Some((*e, d, s));
            }
        }
        match best {
            Some(b) => Ok((b.0, b.2)),
            None => self.pick(p, tol, what),
        }
    }

    fn range(&self, e: &Elem) -> (f64, f64) {
        let t = &self.tracks[e.contour];
        (t.at[e.i0], t.at[e.i1])
    }

    /// Where elements `a` and `b` cross or touch (within 0.01 mm, so a line
    /// ending on another counts): the length along `a`'s track, along `b`'s,
    /// and the point (BE3).
    fn crossings(&self, a: &Elem, b: &Elem) -> Vec<(f64, f64, P)> {
        let (ta, tb) = (&self.tracks[a.contour], &self.tracks[b.contour]);
        let mut out: Vec<(f64, f64, P)> = vec![];
        for i in a.i0..a.i1 {
            for j in b.i0..b.i1 {
                if a.contour == b.contour && i == j {
                    continue;
                }
                let (p0, p1, q0, q1) = (ta.pts[i], ta.pts[i + 1], tb.pts[j], tb.pts[j + 1]);
                let (r, s) = (sub(p1, p0), sub(q1, q0));
                let (lr, ls) = (dist(p0, p1), dist(q0, q1));
                let den = cross(r, s);
                if lr < 1e-12 || ls < 1e-12 || den.abs() <= 1e-12 * lr * ls {
                    continue;
                }
                let qp = sub(q0, p0);
                let (t, u) = (cross(qp, s) / den, cross(qp, r) / den);
                let (et, eu) = (JOIN / lr, JOIN / ls);
                if t < -et || t > 1.0 + et || u < -eu || u > 1.0 + eu {
                    continue;
                }
                let (t, u) = (t.clamp(0.0, 1.0), u.clamp(0.0, 1.0));
                let (mut sa, mut sb) = (ta.at[i] + t * lr, tb.at[j] + u * ls);
                // A circle's start and end are one point: one touch there,
                // not two.
                if a.ring && ta.len() - sa < JOIN {
                    sa = 0.0;
                }
                if b.ring && tb.len() - sb < JOIN {
                    sb = 0.0;
                }
                // Two neighbouring segments of one contour meet at their shared point
                // (a closed contour's last and first too): that is no crossing.
                if a.contour == b.contour && ((sa - sb).abs() < JOIN || (ta.closed && ((sa - sb).abs() - ta.len()).abs() < JOIN)) {
                    continue;
                }
                if !out.iter().any(|x| (x.0 - sa).abs() < JOIN) {
                    out.push((sa, sb, [p0[0] + r[0] * t, p0[1] + r[1] * t]));
                }
            }
        }
        out.sort_by(|x, y| x.0.total_cmp(&y.0));
        out
    }

    /// The stretch of element `e` round `s` between the crossings `xs`
    /// (lengths along its track) or the element's own ends: what Cut and Join
    /// take away (BE4, BE6). A circle's stretch can run through its start,
    /// so it comes as one or two (from, to) pieces.
    fn portion(&self, e: &Elem, s: f64, xs: &[f64]) -> Result<Vec<(f64, f64)>, String> {
        let (s0, s1) = self.range(e);
        let len = s1 - s0;
        let apart = |x: f64| if e.ring { (x - s).abs().min(len - (x - s).abs()) } else { (x - s).abs() };
        if xs.iter().any(|&x| apart(x) < JOIN) {
            return Err("The pick is right on the crossing: pick on the piece to take away.".into());
        }
        let below = xs.iter().copied().filter(|&x| x < s - 1e-9).fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x))));
        let above = xs.iter().copied().filter(|&x| x > s + 1e-9).fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.min(x))));
        let cuts = if e.ring {
            if xs.len() < 2 {
                return Err("It only touches the other element once, so there is no piece of the circle between two crossings to take away.".into());
            }
            match (below, above) {
                (Some(lo), Some(hi)) => vec![(lo, hi)],
                _ => {
                    let (lo, hi) = (xs.iter().copied().fold(f64::MAX, f64::min), xs.iter().copied().fold(f64::MIN, f64::max));
                    vec![(hi, s1), (s0, lo)]
                }
            }
        } else {
            vec![(below.unwrap_or(s0), above.unwrap_or(s1))]
        };
        if cuts.iter().map(|c| c.1 - c.0).sum::<f64>() < JOIN {
            return Err("The pick is right on the crossing: pick on the piece to take away.".into());
        }
        Ok(cuts)
    }
}

fn num(v: f64) -> String {
    crate::nest::angle_num(v)
}

fn at(p: P) -> String {
    format!("{}, {}", num(p[0]), num(p[1]))
}

/// The message's first letter in capitals.
fn cap(s: &str) -> String {
    s.chars().take(1).flat_map(char::to_uppercase).chain(s.chars().skip(1)).collect()
}

/// One contour while an edit is put together.
#[derive(Clone)]
struct Item {
    pts: Vec<P>,
    closed: bool,
    state: ContourState,
    quality: Option<crate::nest::CutQuality>,
    /// The contour (number from 0) it is, unchanged; `None` for a piece the
    /// edit made.
    was: Option<usize>,
    /// Where it sorts: the contour it came from.
    from: usize,
    /// It holds a piece of the old outline (BE10).
    of_outline: bool,
    /// Every piece of it came from an open line (or a bridge's side): only
    /// such a chain is cut once it closes (BE9).
    from_open: bool,
}

/// What an edit does to the contour list before it is tidied (BE9).
#[derive(Default)]
struct Change {
    /// Stretches (from, to along the track) taken out of a contour.
    cut: BTreeMap<usize, Vec<(f64, f64)>>,
    /// A contour split into pieces that stay apart (Break on an open line).
    split: BTreeMap<usize, Vec<Vec<P>>>,
    /// A contour whose points changed while it stays the same contour
    /// (Break on a closed contour).
    redrawn: BTreeMap<usize, Vec<P>>,
    /// New lines (a bridge's sides) and their state.
    added: Vec<(Vec<P>, ContourState)>,
    /// Join the edit's loose ends again (not after Break, whose two lines
    /// meet on purpose).
    rejoin: bool,
    /// Points the edit put an element end at (Break), for the Part view to mark.
    marks: Vec<P>,
    /// What to do instead when the edit would open the outline (BE10).
    open_hint: &'static str,
}

/// The stretches of `t` left when `cuts` are taken out; a closed contour's
/// first and last stretches are one piece through its start.
fn kept(t: &Track, cuts: &[(f64, f64)]) -> Vec<Vec<P>> {
    let mut cuts: Vec<(f64, f64)> = cuts.to_vec();
    cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let l = t.len();
    let mut spans: Vec<(f64, f64)> = vec![];
    let mut from = 0.0;
    for (a, b) in cuts {
        if a - from > JOIN {
            spans.push((from, a));
        }
        from = from.max(b);
    }
    if l - from > JOIN {
        spans.push((from, l));
    }
    let mut pieces: Vec<Vec<P>> = spans.iter().map(|&(a, b)| t.between(a, b)).collect();
    let through_start = spans.len() >= 2 && spans[0].0 <= 1e-9 && spans[spans.len() - 1].1 >= l - 1e-9;
    if t.closed && through_start {
        let first = pieces.remove(0);
        pieces.last_mut().unwrap().extend(first.into_iter().skip(1));
    }
    pieces
}

fn signed_area(pts: &[P]) -> f64 {
    dxf::signed_area(pts)
}

fn inside(poly: &[P], p: P) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            c = !c;
        }
    }
    c
}

/// Do two closed paths cross (not just touch)?
fn rings_cross(a: &[P], b: &[P]) -> bool {
    let seg = |v: &[P], i: usize| (v[i], v[(i + 1) % v.len()]);
    (0..a.len()).any(|i| {
        let (p0, p1) = seg(a, i);
        let r = sub(p1, p0);
        (0..b.len()).any(|j| {
            let (q0, q1) = seg(b, j);
            let s = sub(q1, q0);
            let den = cross(r, s);
            if den.abs() < 1e-12 {
                return false;
            }
            let qp = sub(q0, p0);
            let (t, u) = (cross(qp, s) / den, cross(qp, r) / den);
            let (et, eu) = (JOIN / dist(p0, p1).max(1e-12), JOIN / dist(q0, q1).max(1e-12));
            t > et && t < 1.0 - et && u > eu && u < 1.0 - eu
        })
    })
}

/// A point that tells which side of the outline a contour is on: the middle
/// of its longest segment (a vertex can lie on the outline itself).
fn probe(pts: &[P], closed: bool) -> P {
    let n = pts.len();
    let segs = if closed { n } else { n.saturating_sub(1) };
    let mut best = (0.0, pts[0]);
    for i in 0..segs {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        if dist(a, b) > best.0 {
            best = (dist(a, b), [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]);
        }
    }
    best.1
}

/// The new contour list after `ch`, in CS1 order (closed contours inside the
/// outline, the outline, the open lines), and for each old contour its new
/// number, or `None` where the edit changed it (BE9). Refused when the part
/// would have no closed outline or would fall apart (BE10).
fn tidy(d: &Drawing, ch: Change) -> Result<(Vec<Contour>, Vec<Option<usize>>), String> {
    let ch_hint = ch.open_hint;
    let old = &d.list;
    let old_outline = dxf::outline_index(old);
    let mut items: Vec<Item> = vec![];
    let mut fresh: Vec<bool> = vec![];
    for (k, c) in old.iter().enumerate() {
        let item = |pts: Vec<P>, closed: bool, was: Option<usize>| Item { pts, closed, state: c.state, quality: c.quality, was, from: k, of_outline: Some(k) == old_outline, from_open: !c.closed };
        if let Some(cuts) = ch.cut.get(&k) {
            for p in kept(&d.tracks[k], cuts) {
                items.push(item(p, false, None));
                fresh.push(true);
            }
        } else if let Some(parts) = ch.split.get(&k) {
            for p in parts {
                items.push(item(p.clone(), false, None));
                fresh.push(false);
            }
        } else if let Some(pts) = ch.redrawn.get(&k) {
            items.push(item(pts.clone(), c.closed, Some(k)));
            fresh.push(false);
        } else {
            items.push(item(c.pts.clone(), c.closed, Some(k)));
            fresh.push(false);
        }
    }
    for (pts, state) in ch.added {
        items.push(Item { pts, closed: false, state, quality: None, was: None, from: usize::MAX, of_outline: false, from_open: true });
        fresh.push(true);
    }
    if ch.rejoin {
        // Join open pieces of the same state end to end where one of them is
        // the edit's, until none meet; a chain whose ends meet closes (BE9).
        loop {
            let mut joined = false;
            // Only pairs with a piece the edit made can meet anew; any of
            // their four pairs of ends may meet.
            'find: for i in (0..items.len()).filter(|&i| fresh[i]) {
                for j in 0..items.len() {
                    if i == j || items[i].closed || items[j].closed || items[i].pts.is_empty() || items[j].pts.is_empty() || items[i].state != items[j].state {
                        continue;
                    }
                    let (a, b) = (&items[i].pts, &items[j].pts);
                    let (as_, ae, bs, be) = (a[0], a[a.len() - 1], b[0], b[b.len() - 1]);
                    let rev = |v: &Vec<P>| v.iter().rev().copied().collect::<Vec<P>>();
                    let (head, mut tail) = if dist(ae, bs) < JOIN {
                        (a.clone(), b.clone())
                    } else if dist(ae, be) < JOIN {
                        (a.clone(), rev(b))
                    } else if dist(as_, be) < JOIN {
                        (b.clone(), a.clone())
                    } else if dist(as_, bs) < JOIN {
                        (rev(a), b.clone())
                    } else {
                        continue;
                    };
                    tail.remove(0);
                    let mut it = items[i].clone();
                    it.pts = head;
                    it.pts.extend(tail);
                    it.was = None;
                    it.from = it.from.min(items[j].from);
                    it.of_outline |= items[j].of_outline;
                    it.from_open &= items[j].from_open;
                    if it.quality != items[j].quality {
                        it.quality = None;
                    }
                    let (hi, lo) = (i.max(j), i.min(j));
                    items.remove(hi);
                    fresh.remove(hi);
                    items[lo] = it;
                    fresh[lo] = true;
                    joined = true;
                    break 'find;
                }
            }
            if !joined {
                break;
            }
        }
        for (it, &f) in items.iter_mut().zip(&fresh) {
            if f && !it.closed && it.pts.len() >= 4 && dist(it.pts[0], it.pts[it.pts.len() - 1]) < JOIN {
                it.pts.pop();
                it.closed = true;
                // Open lines on a cut layer come in as No cut only because
                // they are open (CS2); closed now, they are cut (BE9).
                if it.state == ContourState::NoCut && it.from_open {
                    it.state = ContourState::Cut;
                }
            }
        }
    }
    // Pieces too small to be anything go.
    items.retain(|it| if it.closed { it.pts.len() >= 3 && signed_area(&it.pts).abs() > JOIN * JOIN } else { it.pts.len() >= 2 && crate::leads::path_len(&it.pts) >= JOIN });
    items.sort_by_key(|it| it.from);
    // The outline stays the outline; one the edit changed must close again
    // from its own pieces (a hole can't take its place).
    let outline = match old_outline.and_then(|o| items.iter().position(|it| it.was == Some(o))) {
        Some(i) => i,
        None => items
            .iter()
            .enumerate()
            .filter(|(_, it)| it.closed && it.state == ContourState::Cut && (it.of_outline || old_outline.is_none()))
            .max_by(|a, b| signed_area(&a.1.pts).abs().total_cmp(&signed_area(&b.1.pts).abs()))
            .map(|x| x.0)
            .ok_or_else(|| {
                let o = old_outline.filter(|_| !d.view).map_or(String::new(), |o| format!(" (contour {})", o + 1));
                if d.view {
                    format!("That would open the part's outline.{ch_hint}")
                } else {
                    format!("That would open the part's outline{o}: it would no longer close, so there would be no part to cut out.{ch_hint}")
                }
            })?,
    };
    let ring = items[outline].pts.clone();
    if items.iter().enumerate().any(|(i, it)| i != outline && it.closed && !inside(&ring, probe(&it.pts, true))) {
        return Err("That would split the part into separate pieces (a closed contour would end up outside the outline). Cutting a part in two isn't built yet.".into());
    }
    // A changed outline, or a hole the edit made, must not run through
    // another closed contour (a bridge across a second hole).
    let closed: Vec<usize> = (0..items.len()).filter(|&i| items[i].closed).collect();
    for &a in closed.iter().filter(|&&a| items[a].was.is_none()) {
        if closed.iter().any(|&b| b != a && rings_cross(&items[a].pts, &items[b].pts)) {
            return Err("That would run a cut through another hole. Pick a place where the bridge crosses no other hole.".into());
        }
    }
    let mut order: Vec<usize> = (0..items.len()).filter(|&i| i != outline && items[i].closed).collect();
    order.push(outline);
    order.extend((0..items.len()).filter(|&i| !items[i].closed));
    let mut map = vec![None; old.len()];
    let mut list = vec![];
    for (n, &i) in order.iter().enumerate() {
        let it = &items[i];
        if let Some(k) = it.was {
            map[k] = Some(n);
        }
        let mut c = Contour::new(it.pts.clone(), it.closed, it.state);
        // A quality set by hand stays only on a contour that is cut (CQ6).
        c.quality = it.quality.filter(|_| it.closed && it.state == ContourState::Cut);
        list.push(c);
    }
    Ok((list, map))
}

/// What an edit did.
#[derive(Debug)]
pub struct Edited {
    /// In the shop's words.
    pub message: String,
    /// The same, short: what changed, for the note in the Part view.
    pub short: String,
    /// Copies of the part were nested: they keep their place, but the part
    /// cuts other contours now, so its nest is out of date.
    pub nested: bool,
}

/// A part's contour list to edit: an imported part's own, or a built-in
/// shape's holes and outline as a drawing of its own (BE11).
fn drawing_of(part: &Part) -> Drawing {
    Drawing::new(super::contours::list(part))
}

/// Put the new contour list on part `i` (BE9, BE11, BE12): leads, joints and
/// the cut order set by hand stay on the contours the edit didn't change and
/// go from the ones it did; hand picks on the sheets follow; the part's
/// drawing is moved back to start at 0, 0 when its outline's box moved, and
/// nested copies keep their place on the sheet.
fn commit(parts: &mut [Part], sheets: &mut [Vec<Placed>], manual: &mut [Vec<ManualCut>], i: usize, old: &[Contour], list: Vec<Contour>, map: &[Option<usize>]) -> bool {
    let mut p = parts[i].clone();
    let mut geo: CustomGeo = match (&p.shape, &p.custom) {
        (Shape::Custom, Some(g)) => (**g).clone(),
        _ => super::grain::drawn(&p),
    };
    let before = dxf::cut_order(old);
    let after = dxf::cut_order(&list);
    let cut_moved: Vec<Option<usize>> = before.iter().map(|&k| map.get(k).copied().flatten().and_then(|n| after.iter().position(|&a| a == n))).collect();
    let cut_changed = before.len() != after.len() || cut_moved.iter().enumerate().any(|(j, m)| *m != Some(j));
    let outline_cut = dxf::outline_index(&list).and_then(|o| after.iter().position(|&a| a == o));
    // Where the outline's box starts now: the drawing moves back to 0, 0.
    let o = dxf::outline_index(&list).map(|k| list[k].pts.clone()).unwrap_or_default();
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for q in &o {
        for a in 0..2 {
            lo[a] = lo[a].min(q[a]);
            hi[a] = hi[a].max(q[a]);
        }
    }
    let shift = if lo[0].abs() > 1e-9 || lo[1].abs() > 1e-9 { lo } else { [0.0, 0.0] };
    let mv = |q: &mut P| {
        q[0] -= shift[0];
        q[1] -= shift[1];
    };
    geo.contours = list;
    geo.contours.iter_mut().for_each(|c| c.pts.iter_mut().for_each(mv));
    geo.texts.iter_mut().for_each(|t| mv(&mut t.at));
    p.texts.iter_mut().for_each(|t| mv(&mut t.at));
    geo.hand_order = geo.hand_order.iter().map(|step| step.iter().filter_map(|&k| map.get(k).copied().flatten()).collect::<Vec<usize>>()).filter(|s| !s.is_empty()).collect();
    (geo.w, geo.h) = (hi[0] - lo[0], hi[1] - lo[1]);
    geo.apply_states();
    if let Some(t) = p.tech.as_mut() {
        let follow = |s: &mut Option<(u8, P)>| {
            *s = s.and_then(|(ci, q)| cut_moved.get(ci as usize).copied().flatten().and_then(|n| u8::try_from(n).ok()).map(|n| (n, [q[0] - shift[0], q[1] - shift[1]])));
        };
        t.starts.iter_mut().for_each(follow);
        t.joint_picks.iter_mut().for_each(follow);
        for e in t.lead_edits.iter_mut() {
            *e = e.and_then(|x| cut_moved.get(x.contour as usize).copied().flatten().and_then(|n| u8::try_from(n).ok()).map(|n| crate::nest::LeadEdit { contour: n, at: [x.at[0] - shift[0], x.at[1] - shift[1]], ..x }));
        }
    }
    p.shape = Shape::Custom;
    (p.w, p.h) = (geo.w, geo.h);
    p.custom = Some(Arc::new(geo));
    // Nested copies keep their place: the same points land on the same spots.
    let mut nested = false;
    for pl in sheets.iter_mut().flatten().filter(|pl| pl.part == i) {
        nested = true;
        let mut np = oriented(&p, i, pl.angle(), pl.mirror);
        let (want, got) = (pl.map(shift), np.map([0.0, 0.0]));
        np.x += want[0] - got[0];
        np.y += want[1] - got[1];
        *pl = np;
    }
    parts[i] = p;
    // Hand picks on the sheets follow their contours; a part taken whole
    // by a sweep or box stays taken, on its new outline (BE12).
    if cut_changed {
        for (sheet, picks) in sheets.iter().zip(manual.iter_mut()) {
            picks.retain_mut(|m| {
                if !sheet.get(m.placed).is_some_and(|pl| pl.part == i) {
                    return true;
                }
                let to = if m.whole.is_some() { outline_cut } else { cut_moved.get(m.contour).copied().flatten() };
                to.map(|c| m.contour = c).is_some()
            });
        }
    }
    nested && cut_changed
}

/// Holes cut and open lines on a contour list.
fn counts(l: &[Contour]) -> (usize, usize) {
    let o = dxf::outline_index(l);
    let holes = (0..l.len()).filter(|&k| Some(k) != o && l[k].closed && l[k].state == ContourState::Cut).count();
    let open = l.iter().filter(|c| !c.closed && c.state != ContourState::LeaveOut).count();
    (holes, open)
}

/// What changed, counted the way the shop sees a part.
fn summary(old: &[Contour], new: &[Contour]) -> Vec<String> {
    let ((h0, o0), (h1, o1)) = (counts(old), counts(new));
    let mut out = vec![];
    if h0 != h1 {
        out.push(format!("holes cut {h0} → {h1}"));
    }
    if o0 != o1 {
        out.push(format!("open lines {o0} → {o1}"));
    }
    out
}

fn finish(parts: &mut [Part], sheets: &mut [Vec<Placed>], manual: &mut [Vec<ManualCut>], i: usize, d: &Drawing, ch: Change, what: String) -> Result<Edited, String> {
    let (list, map) = tidy(d, ch)?;
    let more = summary(&d.list, &list);
    let (more, now) = if more.is_empty() { (String::new(), String::new()) } else { (format!(" The part now has {}.", more.join(", ")), format!(" Now {}.", more.join(", "))) };
    let nested = commit(parts, sheets, manual, i, &d.list, list, &map);
    let mut message = format!("Part {} '{}': {what}{more}", i + 1, parts[i].name);
    if nested {
        message.push_str(" Its nested copies stay where they are; it cuts other contours now, so nest again or check its sheets.");
    }
    let mut short = cap(&what);
    short.push_str(&now);
    Ok(Edited { message, short, nested })
}

fn part_drawing(parts: &[Part], i: usize) -> Result<Drawing, String> {
    let p = parts.get(i).ok_or_else(|| format!("There is no part {} (the job has {}).", i + 1, parts.len()))?;
    Ok(drawing_of(p))
}

/// An element the tools pick (BE1, BE2): its points, and the point on it
/// nearest the click. The Part view keeps that point as the pick, so zooming
/// between clicks doesn't lose it.
#[derive(Clone, Debug, PartialEq)]
pub struct Picked {
    pub pts: Vec<P>,
    pub at: P,
}

/// The element the tools would pick at `p`, for the Part view to light up
/// under the pointer. Elements picked already (at the points `before`) only
/// count when nothing else is within reach (BE14).
pub fn element_at(part: &Part, p: P, tol: f64, before: &[P]) -> Option<Picked> {
    let d = drawing_of(part);
    let avoid = d.picked(before);
    let (e, s) = d.pick_other(p, tol, "", &avoid).ok()?;
    let t = &d.tracks[e.contour];
    Some(Picked { pts: t.pts[e.i0..=e.i1].to_vec(), at: t.point(s) })
}

/// The base line Cut would take at `p`: the element and its neighbours on
/// the same straight line, as points (BE4).
pub fn base_at(part: &Part, p: P, tol: f64) -> Option<Picked> {
    let d = drawing_of(part);
    let (e, s) = d.pick(p, tol, "").ok()?;
    let t = &d.tracks[e.contour];
    let mut pts: Vec<P> = vec![];
    for x in d.straight_run(e) {
        let seg = &t.pts[x.i0..=x.i1];
        let skip = usize::from(pts.last().is_some_and(|&l| dist(l, seg[0]) < 1e-9));
        pts.extend(seg.iter().skip(skip));
    }
    Some(Picked { pts, at: t.point(s) })
}

impl Drawing {
    /// The elements at earlier picks (points on them).
    fn picked(&self, before: &[P]) -> Vec<Elem> {
        before.iter().filter_map(|&q| self.pick(q, JOIN, "").ok()).flat_map(|x| self.straight_run(x.0)).collect()
    }
}

/// One edit with the Break elements tools, as the clicks that make it (points
/// on the part, mm).
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    Break(P),
    Cut { base: P, pieces: Vec<P> },
    Join(P, P),
    Bridge { first: P, second: P, width: f64 },
}

fn plan(d: &Drawing, edit: &Edit, tol: f64) -> Result<(Change, String), String> {
    match edit {
        Edit::Break(p) => plan_break(d, *p, tol),
        Edit::Cut { base, pieces } => plan_cut(d, *base, pieces, tol),
        Edit::Join(a, b) => plan_join(d, *a, *b, tol),
        Edit::Bridge { first, second, width } => plan_bridge(d, *first, *second, *width, tol),
    }
}

/// Make the edit on part `i` (one of the tools below), with messages for
/// agents (contour numbers and points).
pub fn apply(parts: &mut [Part], sheets: &mut [Vec<Placed>], manual: &mut [Vec<ManualCut>], i: usize, edit: &Edit, tol: f64) -> Result<Edited, String> {
    apply_as(parts, sheets, manual, i, edit, tol, false)
}

/// The same, with messages for the Part view: lines named by what they are.
pub fn apply_in_view(parts: &mut [Part], sheets: &mut [Vec<Placed>], manual: &mut [Vec<ManualCut>], i: usize, edit: &Edit, tol: f64) -> Result<Edited, String> {
    apply_as(parts, sheets, manual, i, edit, tol, true)
}

#[allow(clippy::too_many_arguments)]
fn apply_as(parts: &mut [Part], sheets: &mut [Vec<Placed>], manual: &mut [Vec<ManualCut>], i: usize, edit: &Edit, tol: f64, view: bool) -> Result<Edited, String> {
    let mut d = part_drawing(parts, i)?;
    d.view = view;
    let (ch, what) = plan(&d, edit, tol)?;
    finish(parts, sheets, manual, i, &d, ch, what)
}

/// What an edit would do, drawn before the click that makes it (BE15).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preview {
    /// Stretches of the part's lines it would take away.
    pub removed: Vec<Vec<P>>,
    /// Lines it would add (a bridge's two sides).
    pub added: Vec<Vec<P>>,
    /// Points it would put an element end at (Break).
    pub marks: Vec<P>,
    /// Holes cut before and after: fewer after means a hole opens (BE15).
    pub holes: (usize, usize),
}

/// The edit worked out on `part` without changing it: what it would take
/// away and add, or why it would be refused — the same refusals the edit
/// gives, the part opening or splitting included (BE10, BE15).
pub fn preview(part: &Part, edit: &Edit, tol: f64) -> Result<Preview, String> {
    let mut d = drawing_of(part);
    d.view = true;
    let (ch, _) = plan(&d, edit, tol)?;
    let removed = ch.cut.iter().flat_map(|(&k, cuts)| cuts.iter().map(move |&(a, b)| (k, a, b))).map(|(k, a, b)| d.tracks[k].between(a, b)).collect();
    let added = ch.added.iter().map(|x| x.0.clone()).collect();
    let marks = ch.marks.clone();
    let (list, _) = tidy(&d, ch)?;
    let holes = (counts(&d.list).0, counts(&list).0);
    Ok(Preview { removed, added, marks, holes })
}

/// Break (BE5): split the element picked at `p` there. An open line becomes
/// two lines that each keep its state; a closed contour stays closed and cut
/// the same, with an element end at the point.
pub fn break_at(parts: &mut [Part], sheets: &mut [Vec<Placed>], manual: &mut [Vec<ManualCut>], i: usize, p: P, tol: f64) -> Result<Edited, String> {
    apply(parts, sheets, manual, i, &Edit::Break(p), tol)
}

fn plan_break(d: &Drawing, p: P, tol: f64) -> Result<(Change, String), String> {
    let (e, mut s) = d.pick(p, tol, "the break point")?;
    let k = e.contour;
    let t = &d.tracks[k];
    if t.closed {
        let (s0, s1) = d.range(&e);
        // A circle's start is no element end: it is broken beside it too.
        if !e.ring && ((s - s0).abs() < JOIN || (s1 - s).abs() < JOIN) {
            return Err(format!("{} already has an element end{}: nothing to break.", cap(&d.name(k)), d.place(t.point(s))));
        }
        if e.ring && s1 - s < 3.0 * JOIN {
            s = s0;
        }
        // On a point of an arc drawn in short lines, which can't be an
        // element end, the break goes just beside it (BE5).
        if t.at.iter().any(|&a| (a - s).abs() < JOIN) {
            s = if s + 3.0 * JOIN < s1 - JOIN { s + 2.0 * JOIN } else { s - 2.0 * JOIN };
        }
        let q = t.point(s);
        let mut ch = Change { marks: vec![q], ..Change::default() };
        let mut pts = t.between(0.0, s);
        pts.extend(t.between(s, t.len()).into_iter().skip(1));
        pts.pop();
        ch.redrawn.insert(k, pts);
        let what = if d.view { format!("{} broken here: it stays one closed contour, cut the same, with an element end there.", d.name(k)) } else { format!("{} broken{}: it stays closed and is cut the same, with an element end there.", d.name(k), d.place(q)) };
        Ok((ch, what))
    } else {
        let q = t.point(s);
        let mut ch = Change { marks: vec![q], ..Change::default() };
        if s < JOIN || t.len() - s < JOIN {
            return Err(format!("The point{} is the end of {} already: pick the break point along the line.", d.place(q), d.name(k)));
        }
        ch.split.insert(k, vec![t.between(0.0, s), t.between(s, t.len())]);
        Ok((ch, format!("{} broken{} into two lines.", d.name(k), d.place(q))))
    }
}

/// Cut 1 and Cut several (BE4): for each pick in `pieces`, take away the
/// piece of the element picked there that runs past (or between) where it
/// crosses the base element picked at `base` — an overhanging line is
/// snipped back to the line it should stop on. Contours left with loose ends
/// that meet join again (BE9).
pub fn cut(parts: &mut [Part], sheets: &mut [Vec<Placed>], manual: &mut [Vec<ManualCut>], i: usize, base: P, pieces: &[P], tol: f64) -> Result<Edited, String> {
    apply(parts, sheets, manual, i, &Edit::Cut { base, pieces: pieces.to_vec() }, tol)
}

fn plan_cut(d: &Drawing, base: P, pieces: &[P], tol: f64) -> Result<(Change, String), String> {
    if pieces.is_empty() {
        return Err("Pick the base element, then the piece (or pieces) to cut away.".into());
    }
    let (b, _) = d.pick(base, tol, "the base element")?;
    let base_run = d.straight_run(b);
    let mut ch = Change { rejoin: true, open_hint: " Pick the outline as the base, then the piece that sticks out.", ..Change::default() };
    let mut len = 0.0;
    for &q in pieces {
        let (e, s) = d.pick_other(q, tol, "the piece to cut", &base_run)?;
        if base_run.contains(&e) {
            return Err(format!("The piece picked{} is on the base line itself: pick a piece of another line that crosses it.", d.place(q)));
        }
        let mut xs: Vec<f64> = base_run.iter().flat_map(|bb| d.crossings(&e, bb)).map(|x| x.0).collect();
        xs.sort_by(f64::total_cmp);
        xs.dedup_by(|a, b| (*a - *b).abs() < JOIN);
        if xs.is_empty() {
            return Err(format!("{}{} doesn't cross or touch the base element, so there's nothing to cut it back to.", cap(&d.name(e.contour)), d.place(q)));
        }
        let cuts = d.portion(&e, s, &xs).map_err(|m| if d.view { m } else { format!("At {}: {m}", at(q)) })?;
        len += cuts.iter().map(|c| c.1 - c.0).sum::<f64>();
        ch.cut.entry(e.contour).or_default().extend(cuts);
    }
    let what = if pieces.len() == 1 { "1 piece".to_string() } else { format!("{} pieces", pieces.len()) };
    Ok((ch, format!("cut away {what} ({} mm) past the base {}.", num(len), if d.view { "line" } else { "element" })))
}

/// Join (BE6): two crossing elements become a corner. The piece of each
/// element round its pick, up to where they cross, is taken away.
pub fn join(parts: &mut [Part], sheets: &mut [Vec<Placed>], manual: &mut [Vec<ManualCut>], i: usize, first: P, second: P, tol: f64) -> Result<Edited, String> {
    apply(parts, sheets, manual, i, &Edit::Join(first, second), tol)
}

fn plan_join(d: &Drawing, first: P, second: P, tol: f64) -> Result<(Change, String), String> {
    let (a, sa) = d.pick(first, tol, "the first element")?;
    let (b, sb) = d.pick_other(second, tol, "the second element", &d.straight_run(a))?;
    if a == b {
        return Err("Both picks are on the same element: pick the piece to take away on each of two crossing elements.".into());
    }
    let xs = d.crossings(&a, &b);
    if xs.is_empty() {
        return Err("The two elements don't cross or touch, so they can't be joined at a corner. Lengthening lines to meet isn't built yet.".into());
    }
    let mut ch = Change { rejoin: true, open_hint: " Use Cut 1 instead: click the outline, then the piece that sticks out.", ..Change::default() };
    let pa = d.portion(&a, sa, &xs.iter().map(|x| x.0).collect::<Vec<_>>()).map_err(|m| format!("First element: {m}"))?;
    let pb = d.portion(&b, sb, &xs.iter().map(|x| x.1).collect::<Vec<_>>()).map_err(|m| format!("Second element: {m}"))?;
    ch.cut.entry(a.contour).or_default().extend(pa);
    ch.cut.entry(b.contour).or_default().extend(pb);
    let corner = xs.iter().min_by(|x, y| (dist(x.2, first) + dist(x.2, second)).total_cmp(&(dist(y.2, first) + dist(y.2, second)))).map(|x| x.2).unwrap();
    Ok((ch, format!("the two elements joined at a corner{}.", d.place(corner))))
}

/// Where the line through `o` along `dir` crosses element `e`: the crossing
/// nearest `o` + `dir` × `want` (length along the element's track, point).
fn line_hits(d: &Drawing, e: &Elem, o: P, dir: P, want: f64) -> Option<(f64, P)> {
    let t = &d.tracks[e.contour];
    let mut best: Option<(f64, f64, P)> = None;
    for i in e.i0..e.i1 {
        let (q0, q1) = (t.pts[i], t.pts[i + 1]);
        let s = sub(q1, q0);
        let ls = dist(q0, q1);
        let den = cross(dir, s);
        if ls < 1e-12 || den.abs() <= 1e-12 * ls {
            continue;
        }
        let qo = sub(q0, o);
        let (lam, u) = (cross(qo, s) / den, cross(qo, dir) / den);
        if !(-1e-9..=1.0 + 1e-9).contains(&u) {
            continue;
        }
        let u = u.clamp(0.0, 1.0);
        let off = (lam - want).abs();
        if best.is_none_or(|b| off < b.0) {
            best = Some((off, t.at[i] + u * ls, [q0[0] + s[0] * u, q0[1] + s[1] * u]));
        }
    }
    best.map(|b| (b.1, b.2))
}

/// The stretch of element `e` between `x` and `y` (lengths along its track)
/// that holds `s`: on a circle it may run through the start.
fn stretch(d: &Drawing, e: &Elem, x: f64, y: f64, s: f64) -> Vec<(f64, f64)> {
    let (lo, hi) = (x.min(y), x.max(y));
    if !e.ring || (lo..=hi).contains(&s) {
        return vec![(lo, hi)];
    }
    let (s0, s1) = d.range(e);
    vec![(hi, s1), (s0, lo)]
}

/// Bridges (BE7, BE8): cut a bridge `width` mm wide from the first element
/// to the second. It runs square off the first element from where it was
/// picked to the second; its two sides become lines, and the pieces of both
/// elements between the sides are taken away. Across a hole it leaves a strip
/// of material holding the hole's two halves apart (two holes); from a hole
/// to the outline it opens the hole to the outside.
pub fn bridge(parts: &mut [Part], sheets: &mut [Vec<Placed>], manual: &mut [Vec<ManualCut>], i: usize, first: P, second: P, width: f64, tol: f64) -> Result<Edited, String> {
    apply(parts, sheets, manual, i, &Edit::Bridge { first, second, width }, tol)
}

fn plan_bridge(d: &Drawing, first: P, second: P, width: f64, tol: f64) -> Result<(Change, String), String> {
    if !(width.is_finite() && width > JOIN) {
        return Err(format!("Give the bridge's width in mm (more than {} mm).", num(JOIN)));
    }
    let (a, sa) = d.pick(first, tol, "the first element")?;
    let (b, sb) = d.pick_other(second, tol, "the second element", &d.straight_run(a))?;
    let (ka, kb) = (a.contour, b.contour);
    if d.list[ka].state != d.list[kb].state {
        return Err(format!("{} is {} and {} is {}: a bridge joins two contours that are machined the same way.", cap(&d.name(ka)), d.list[ka].state.label(), d.name(kb), d.list[kb].state.label()));
    }
    let (ta, tb) = (&d.tracks[ka], &d.tracks[kb]);
    let q1 = ta.point(sa);
    // Square off the first element where it was picked, towards the second pick.
    let seg = (a.i0..a.i1).find(|&j| ta.at[j + 1] >= sa - 1e-9).unwrap_or(a.i0);
    let along = sub(ta.pts[seg + 1], ta.pts[seg]);
    let la = dist(ta.pts[seg], ta.pts[seg + 1]).max(1e-12);
    let mut n = [-along[1] / la, along[0] / la];
    let p2 = tb.point(sb);
    if (p2[0] - q1[0]) * n[0] + (p2[1] - q1[1]) * n[1] < 0.0 {
        n = [-n[0], -n[1]];
    }
    // Where that line meets the second element; else the second element's
    // point nearest the first pick.
    let q2 = match line_hits(d, &b, q1, n, dist(q1, p2)).filter(|h| dist(h.1, q1) > JOIN && ((h.1[0] - q1[0]) * n[0] + (h.1[1] - q1[1]) * n[1]) > 0.0) {
        Some(h) => h.1,
        None => {
            let (_, s) = tb.nearest(b.i0, b.i1, q1);
            tb.point(s)
        }
    };
    let span = dist(q1, q2);
    if span <= JOIN {
        return Err("The two elements touch there: pick two elements with a gap between them to bridge.".into());
    }
    let dir = [(q2[0] - q1[0]) / span, (q2[1] - q1[1]) / span];
    let side = [-dir[1], dir[0]];
    let mut hits = vec![];
    for sign in [1.0, -1.0] {
        let o = [q1[0] + side[0] * sign * width / 2.0, q1[1] + side[1] * sign * width / 2.0];
        let ha = line_hits(d, &a, o, dir, 0.0);
        let hb = line_hits(d, &b, o, dir, span);
        match (ha, hb) {
            (Some(ha), Some(hb)) if dist(ha.1, hb.1) > JOIN => hits.push((ha, hb)),
            _ => {
                return Err(format!(
                    "A bridge {} mm wide doesn't fit there: one of its sides runs off the end of an element. Pick nearer the middle of the elements, or make it narrower.",
                    num(width)
                ))
            }
        }
    }
    let ((a1, b1), (a2, b2)) = (hits[0], hits[1]);
    let (_, sq2) = tb.nearest(b.i0, b.i1, q2);
    let mut ch = Change { rejoin: true, ..Change::default() };
    ch.cut.entry(ka).or_default().extend(stretch(d, &a, a1.0, a2.0, sa));
    ch.cut.entry(kb).or_default().extend(stretch(d, &b, b1.0, b2.0, sq2));
    let state = d.list[ka].state;
    ch.added.push((vec![a1.1, b1.1], state));
    ch.added.push((vec![a2.1, b2.1], state));
    let whom = if ka == kb { format!("across {}", d.name(ka)) } else { format!("from {} to {}", d.name(ka), d.name(kb)) };
    // Across a hole the strip between the sides is left as material; through
    // material (a hole to the outline) it is cut out (BE7, BE8).
    let c = [(a1.1[0] + b1.1[0] + a2.1[0] + b2.1[0]) / 4.0, (a1.1[1] + b1.1[1] + a2.1[1] + b2.1[1]) / 4.0];
    let o = dxf::outline_index(&d.list);
    let solid = o.is_some_and(|o| inside(&d.list[o].pts, c)) && !d.list.iter().enumerate().any(|(k, h)| Some(k) != o && h.closed && h.state == ContourState::Cut && inside(&h.pts, c));
    let what = if solid { format!("{} mm strip cut out {whom}.", num(width)) } else { format!("bridge {} mm wide left {whom}: the strip between stays as material.", num(width)) };
    Ok((ch, what))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nest::Part;

    fn part_of(contours: Vec<Contour>) -> Part {
        let mut g = CustomGeo { outline: vec![], holes: vec![], fills: vec![], hole_fills: vec![], marks: vec![], texts: vec![], w: 100.0, h: 50.0, contours, hand_order: vec![] };
        g.apply_states();
        Part { name: "DEMO-001".into(), shape: Shape::Custom, w: 100.0, h: 50.0, qty: 1, rotate: true, color: 0, tech: None, custom: Some(Arc::new(g)), rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<P> {
        vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    }

    fn cut_c(pts: Vec<P>) -> Contour {
        Contour::new(pts, true, ContourState::Cut)
    }

    fn line(pts: Vec<P>) -> Contour {
        Contour::new(pts, false, ContourState::NoCut)
    }

    fn run<F: FnOnce(&mut [Part], &mut [Vec<Placed>], &mut [Vec<ManualCut>]) -> Result<Edited, String>>(p: Part, f: F) -> (Result<Edited, String>, Part) {
        let mut parts = vec![p];
        let mut sheets: Vec<Vec<Placed>> = vec![vec![]];
        let mut manual: Vec<Vec<ManualCut>> = vec![vec![]];
        let r = f(&mut parts, &mut sheets, &mut manual);
        (r, parts.remove(0))
    }

    fn contours(p: &Part) -> Vec<Contour> {
        super::super::contours::list(p)
    }

    #[test]
    fn cut_one_snips_a_line_hanging_past_the_outline_edge() {
        // BE4: a line from inside the part runs 10 mm past the hole's top edge.
        let p = part_of(vec![cut_c(rect(20.0, 10.0, 60.0, 30.0)), cut_c(rect(0.0, 0.0, 100.0, 50.0)), line(vec![[40.0, 20.0], [40.0, 40.0]])]);
        let (r, p) = run(p, |ps, sh, m| cut(ps, sh, m, 0, [40.0, 30.0], &[[40.0, 35.0]], PICK_TOL));
        let r = r.unwrap();
        assert!(r.message.contains("cut away 1 piece (10 mm)"), "{}", r.message);
        let l = contours(&p);
        assert_eq!(l.len(), 3);
        assert_eq!(l[2].pts, vec![[40.0, 20.0], [40.0, 30.0]], "the line now stops on the hole's edge");
        assert!(l[0].closed && l[1].closed);
    }

    #[test]
    fn cut_removes_a_stub_that_only_touches_the_base() {
        // A stub standing on the hole's edge goes whole (BE3: touching counts).
        let p = part_of(vec![cut_c(rect(20.0, 10.0, 60.0, 30.0)), cut_c(rect(0.0, 0.0, 100.0, 50.0)), line(vec![[30.0, 30.0], [30.0, 45.0]])]);
        let (r, p) = run(p, |ps, sh, m| cut(ps, sh, m, 0, [25.0, 30.0], &[[30.0, 40.0]], PICK_TOL));
        assert!(r.unwrap().message.contains("open lines 1 → 0"));
        assert_eq!(contours(&p).len(), 2);
    }

    #[test]
    fn the_preview_shows_what_a_cut_takes_away_and_leaves_the_part_alone() {
        // BE15: the stretch past the hole's edge, before the click; the part unchanged.
        let p = part_of(vec![cut_c(rect(20.0, 10.0, 60.0, 30.0)), cut_c(rect(0.0, 0.0, 100.0, 50.0)), line(vec![[40.0, 20.0], [40.0, 40.0]])]);
        let before = contours(&p);
        let pv = preview(&p, &Edit::Cut { base: [40.0, 30.0], pieces: vec![[40.0, 35.0]] }, PICK_TOL).unwrap();
        assert_eq!(pv.removed, vec![vec![[40.0, 30.0], [40.0, 40.0]]]);
        assert!(pv.added.is_empty() && pv.marks.is_empty());
        assert_eq!(pv.holes, (1, 1));
        assert_eq!(contours(&p), before);
        // A refusal comes before the click too: cutting the outline open.
        let q = part_of(vec![cut_c(rect(0.0, 0.0, 100.0, 50.0)), line(vec![[50.0, -10.0], [50.0, 20.0]])]);
        let pv = preview(&q, &Edit::Cut { base: [50.0, 10.0], pieces: vec![[30.0, 0.0]] }, PICK_TOL);
        assert!(pv.unwrap_err().contains("open the part's outline"));
        // Break marks its point; a bridge shows its two sides.
        let pv = preview(&p, &Edit::Break([40.0, 25.0]), PICK_TOL).unwrap();
        assert_eq!(pv.marks, vec![[40.0, 25.0]]);
        let pv = preview(&p, &Edit::Bridge { first: [40.0, 30.0], second: [40.0, 10.0], width: 4.0 }, PICK_TOL);
        assert_eq!(pv.map(|v| v.added.len()), Ok(2));
    }

    #[test]
    fn the_second_pick_takes_the_other_line_where_the_two_meet() {
        // BE14: a click by the stub's foot, nearer the base, still takes the stub.
        let p = part_of(vec![cut_c(rect(20.0, 10.0, 60.0, 30.0)), cut_c(rect(0.0, 0.0, 100.0, 50.0)), line(vec![[30.0, 30.0], [30.0, 45.0]])]);
        let picked = element_at(&p, [30.5, 30.2], PICK_TOL, &[[25.0, 30.0]]).unwrap();
        assert_eq!(picked.pts, vec![[30.0, 30.0], [30.0, 45.0]]);
        assert_eq!(picked.at, [30.0, 30.2]);
        let (r, p) = run(p, |ps, sh, m| cut(ps, sh, m, 0, [25.0, 30.0], &[[30.5, 30.2]], PICK_TOL));
        assert!(r.unwrap().short.starts_with("Cut away 1 piece"));
        assert_eq!(contours(&p).len(), 2);
    }

    #[test]
    fn a_straight_edge_drawn_in_two_pieces_is_one_base_line() {
        // BE4: the base is the whole straight edge, so a line crossing the
        // other piece still cuts back to it, and the whole edge lights up.
        let p = part_of(vec![cut_c(vec![[0.0, 0.0], [100.0, 0.0], [100.0, 50.0], [50.0, 50.0], [0.0, 50.0]]), line(vec![[20.0, 40.0], [20.0, 60.0]])]);
        let b = base_at(&p, [80.0, 50.0], PICK_TOL).unwrap();
        assert_eq!(b.pts.first().zip(b.pts.last()), Some((&[100.0, 50.0], &[0.0, 50.0])), "{:?}", b.pts);
        let (r, p) = run(p, |ps, sh, m| cut(ps, sh, m, 0, [80.0, 50.0], &[[20.0, 55.0]], PICK_TOL));
        assert!(r.unwrap().message.contains("cut away 1 piece (10 mm)"));
        assert_eq!(contours(&p)[1].pts, vec![[20.0, 40.0], [20.0, 50.0]]);
    }

    /// A round hole of 36 points round (50, 25), r 10, starting at (60, 25).
    fn round(r: f64) -> Vec<P> {
        (0..36).map(|k| (k as f64 * 10.0).to_radians()).map(|a| [50.0 + r * a.cos(), 25.0 + r * a.sin()]).collect()
    }

    #[test]
    fn join_takes_two_lines_that_both_start_at_the_crossing() {
        // BE6, BE9: the kept pieces both start at the corner; they become one line.
        let p = part_of(vec![cut_c(rect(0.0, 0.0, 100.0, 50.0)), line(vec![[30.0, 20.0], [30.0, 40.0]]), line(vec![[20.0, 30.0], [40.0, 30.0]])]);
        let (r, p) = run(p, |ps, sh, m| join(ps, sh, m, 0, [30.0, 25.0], [25.0, 30.0], PICK_TOL));
        r.unwrap();
        let open: Vec<Contour> = contours(&p).into_iter().filter(|c| !c.closed).collect();
        assert_eq!(open.len(), 1, "{open:?}");
        assert_eq!(open[0].pts.len(), 3);
    }

    #[test]
    fn a_pick_right_on_the_crossing_is_refused() {
        let p = part_of(vec![cut_c(rect(20.0, 10.0, 60.0, 30.0)), cut_c(rect(0.0, 0.0, 100.0, 50.0)), line(vec![[40.0, 20.0], [40.0, 40.0]])]);
        let (r, _) = run(p, |ps, sh, m| cut(ps, sh, m, 0, [25.0, 30.0], &[[40.0, 30.0]], PICK_TOL));
        assert!(r.unwrap_err().contains("right on the crossing"));
    }

    #[test]
    fn a_line_touching_a_round_hole_once_cuts_nothing_off_it() {
        // BE3: one touch at the circle's own start is one, not two.
        let p = part_of(vec![cut_c(round(10.0)), cut_c(rect(0.0, 0.0, 100.0, 50.0)), line(vec![[60.0, 10.0], [60.0, 40.0]])]);
        let (r, _) = run(p, |ps, sh, m| cut(ps, sh, m, 0, [60.0, 35.0], &[[40.0, 25.0]], PICK_TOL));
        assert!(r.unwrap_err().contains("only touches"));
    }

    #[test]
    fn break_on_a_point_of_a_round_hole_breaks_beside_it() {
        // BE5: a point of a circle drawn in short lines can't be an element end.
        let p = part_of(vec![cut_c(round(10.0)), cut_c(rect(0.0, 0.0, 100.0, 50.0))]);
        let at = round(10.0)[9];
        let (r, p) = run(p, |ps, sh, m| break_at(ps, sh, m, 0, at, PICK_TOL));
        assert!(r.unwrap().message.contains("stays closed"));
        assert_eq!(contours(&p)[0].pts.len(), 37);
    }

    #[test]
    fn trimming_an_imported_hole_overhang_makes_it_a_cut_hole() {
        // BE9 with CS2: the hole's lines come in open (No cut) because the
        // top one runs past the left side; cut back, the hole closes and is cut.
        let line = |a: [f64; 4]| format!("0\nLINE\n8\n0\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], a[2], a[3]);
        let mut d = String::from("0\nSECTION\n2\nENTITIES\n");
        for l in [[0.0, 0.0, 100.0, 0.0], [100.0, 0.0, 100.0, 50.0], [100.0, 50.0, 0.0, 50.0], [0.0, 50.0, 0.0, 0.0], [20.0, 10.0, 40.0, 10.0], [40.0, 10.0, 40.0, 30.0], [40.0, 30.0, 15.0, 30.0], [20.0, 30.0, 20.0, 10.0]] {
            d.push_str(&line(l));
        }
        d.push_str("0\nENDSEC\n0\nEOF\n");
        let (g, _) = dxf::parse(&d).unwrap();
        let p = Part { name: "DEMO-001".into(), shape: Shape::Custom, w: g.w, h: g.h, qty: 1, rotate: true, color: 0, tech: None, custom: Some(Arc::new(g)), rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        assert!(contours(&p).iter().any(|c| !c.closed && c.state == ContourState::NoCut), "the hole comes in open");
        let (r, p) = run(p, |ps, sh, m| cut(ps, sh, m, 0, [20.0, 20.0], &[[17.0, 30.0]], PICK_TOL));
        assert!(r.unwrap().message.contains("holes cut 0 → 1"));
        let l = contours(&p);
        assert_eq!(l.len(), 2);
        assert!(l[0].closed && l[0].state == ContourState::Cut);
    }

    #[test]
    fn a_pick_just_before_a_round_hole_start_is_on_the_crossing() {
        // A line through the circle's start: a pick 0.005 mm before it is on it.
        let p = part_of(vec![cut_c(round(10.0)), cut_c(rect(0.0, 0.0, 100.0, 50.0)), line(vec![[30.0, 25.0], [70.0, 25.0]])]);
        let a: f64 = -0.0005;
        let (r, _) = run(p, |ps, sh, m| cut(ps, sh, m, 0, [35.0, 25.0], &[[50.0 + 10.0 * a.cos(), 25.0 + 10.0 * a.sin()]], PICK_TOL));
        assert!(r.unwrap_err().contains("right on the crossing"));
    }

    #[test]
    fn break_at_a_round_hole_start_breaks_beside_it() {
        let p = part_of(vec![cut_c(round(10.0)), cut_c(rect(0.0, 0.0, 100.0, 50.0))]);
        let (r, p) = run(p, |ps, sh, m| break_at(ps, sh, m, 0, [60.0, 25.0], PICK_TOL));
        r.unwrap();
        assert_eq!(contours(&p)[0].pts.len(), 37);
    }

    #[test]
    fn a_bridge_between_two_holes_through_a_third_is_refused() {
        let p = part_of(vec![cut_c(rect(40.0, 5.0, 60.0, 15.0)), cut_c(rect(45.0, 20.0, 60.0, 30.0)), cut_c(rect(40.0, 35.0, 60.0, 45.0)), cut_c(rect(0.0, 0.0, 100.0, 50.0))]);
        let (r, _) = run(p, |ps, sh, m| bridge(ps, sh, m, 0, [50.0, 15.0], [50.0, 35.0], 3.0, PICK_TOL));
        assert!(r.unwrap_err().contains("another hole"));
    }

    #[test]
    fn a_bridge_across_a_no_cut_contour_leaves_it_no_cut() {
        // BE9: only chains of open lines become cut when they close.
        let p = part_of(vec![Contour::new(rect(40.0, 20.0, 60.0, 30.0), true, ContourState::NoCut), cut_c(rect(0.0, 0.0, 100.0, 50.0))]);
        let (r, p) = run(p, |ps, sh, m| bridge(ps, sh, m, 0, [50.0, 30.0], [50.0, 20.0], 3.0, PICK_TOL));
        r.unwrap();
        let l = contours(&p);
        assert_eq!(l.iter().filter(|c| c.closed && c.state == ContourState::NoCut).count(), 2, "{l:?}");
    }

    #[test]
    fn a_bridge_through_a_second_hole_is_refused() {
        // BE10: from the outline down to one hole, crossing another on the way.
        let p = part_of(vec![cut_c(rect(40.0, 10.0, 60.0, 20.0)), cut_c(rect(46.0, 30.0, 60.0, 40.0)), cut_c(rect(0.0, 0.0, 100.0, 50.0))]);
        let (r, _) = run(p, |ps, sh, m| bridge(ps, sh, m, 0, [50.0, 50.0], [50.0, 20.0], 3.0, PICK_TOL));
        assert!(r.unwrap_err().contains("another hole"));
    }

    #[test]
    fn cut_several_takes_a_piece_of_each_pick() {
        let p = part_of(vec![cut_c(rect(0.0, 0.0, 100.0, 50.0)), line(vec![[10.0, 10.0], [90.0, 10.0]]), line(vec![[20.0, 5.0], [20.0, 20.0]]), line(vec![[50.0, 5.0], [50.0, 20.0]])]);
        let (r, p) = run(p, |ps, sh, m| cut(ps, sh, m, 0, [70.0, 10.0], &[[20.0, 7.0], [50.0, 7.0]], PICK_TOL));
        assert!(r.unwrap().message.contains("2 pieces (10 mm)"));
        let l = contours(&p);
        assert!(l.iter().any(|c| c.pts == vec![[20.0, 10.0], [20.0, 20.0]]));
        assert!(l.iter().any(|c| c.pts == vec![[50.0, 10.0], [50.0, 20.0]]));
    }

    #[test]
    fn a_hole_whose_side_overshoots_closes_once_the_overhang_is_cut() {
        // BE9: a hole that didn't close (its top line runs 5 mm past the right
        // side) closes when the overhang is cut and becomes a hole that is cut.
        let u = Contour::new(vec![[60.0, 30.0], [60.0, 10.0], [20.0, 10.0], [20.0, 30.0]], false, ContourState::Cut);
        let top = Contour::new(vec![[20.0, 30.0], [65.0, 30.0]], false, ContourState::Cut);
        let p = part_of(vec![cut_c(rect(0.0, 0.0, 100.0, 50.0)), u, top]);
        let (r, p) = run(p, |ps, sh, m| cut(ps, sh, m, 0, [60.0, 20.0], &[[63.0, 30.0]], PICK_TOL));
        assert!(r.unwrap().message.contains("holes cut 0 → 1"));
        let l = contours(&p);
        assert_eq!(l.len(), 2);
        assert!(l[0].closed && l[0].state == ContourState::Cut);
        assert!((dxf::signed_area(&l[0].pts).abs() - 800.0).abs() < 1e-6);
        assert_eq!(p.geometry().holes.len(), 1, "the new hole is cut");
    }

    #[test]
    fn cutting_the_outline_open_is_refused() {
        // BE10: the base is the line, the pick on the outline: that would open it.
        let p = part_of(vec![cut_c(rect(0.0, 0.0, 100.0, 50.0)), line(vec![[50.0, -10.0], [50.0, 20.0]])]);
        let before = contours(&p);
        let (r, p) = run(p, |ps, sh, m| cut(ps, sh, m, 0, [50.0, 10.0], &[[30.0, 0.0]], PICK_TOL));
        assert!(r.unwrap_err().contains("open the part's outline"));
        assert_eq!(contours(&p), before);
    }

    #[test]
    fn opening_the_outline_is_called_that_even_with_holes_inside() {
        // BE10: a hole doesn't take the opened outline's place, so the
        // refusal names the outline, not a split part.
        let p = part_of(vec![cut_c(rect(120.0, 20.0, 180.0, 60.0)), cut_c(rect(40.0, 20.0, 60.0, 40.0)), cut_c(rect(0.0, 0.0, 200.0, 100.0)), line(vec![[100.0, 100.0], [100.0, 80.0]])]);
        let (r, _) = run(p, |ps, sh, m| join(ps, sh, m, 0, [100.0, 85.0], [60.0, 100.0], PICK_TOL));
        assert!(r.unwrap_err().contains("open the part's outline"));
    }

    #[test]
    fn join_makes_a_corner_of_two_crossing_lines() {
        // BE6: the help's picture: a cross; the left of the level line and the
        // top of the upright go, a corner is left.
        let p = part_of(vec![cut_c(rect(0.0, 0.0, 100.0, 50.0)), line(vec![[10.0, 20.0], [50.0, 20.0]]), line(vec![[30.0, 5.0], [30.0, 40.0]])]);
        let (r, p) = run(p, |ps, sh, m| join(ps, sh, m, 0, [15.0, 20.0], [30.0, 35.0], PICK_TOL));
        assert!(r.unwrap().message.contains("corner at 30, 20"));
        let l = contours(&p);
        assert_eq!(l.len(), 2, "the two pieces left meet, so they are one line");
        let mut pts = l[1].pts.clone();
        if pts[0] != [50.0, 20.0] {
            pts.reverse();
        }
        assert_eq!(pts, vec![[50.0, 20.0], [30.0, 20.0], [30.0, 5.0]]);
    }

    #[test]
    fn break_splits_an_open_line_and_keeps_a_closed_contour_closed() {
        let p = part_of(vec![cut_c(rect(0.0, 0.0, 100.0, 50.0)), line(vec![[10.0, 20.0], [50.0, 20.0]])]);
        let (r, p) = run(p, |ps, sh, m| break_at(ps, sh, m, 0, [30.0, 20.2], PICK_TOL));
        assert!(r.unwrap().message.contains("into two lines"));
        let l = contours(&p);
        assert_eq!(l.len(), 3, "the two halves stay apart (BE5)");
        assert_eq!((l[1].pts.clone(), l[2].pts.clone()), (vec![[10.0, 20.0], [30.0, 20.0]], vec![[30.0, 20.0], [50.0, 20.0]]));
        let (r, p) = run(p, |ps, sh, m| break_at(ps, sh, m, 0, [100.0, 25.0], PICK_TOL));
        assert!(r.unwrap().message.contains("stays closed"));
        let l = contours(&p);
        assert!(l[0].closed && l[0].pts.len() == 5);
        // The broken side is two elements now: Cut 1 can take one of them whole.
        let d = drawing_of(&p);
        assert_eq!(d.elems.iter().filter(|e| e.contour == 0).count(), 5);
    }

    #[test]
    fn a_bridge_across_a_hole_leaves_two_holes() {
        // BE7: the help's picture: picks on a slot's top and bottom, a bridge
        // 4 mm wide: two holes with a strip of material between them.
        let p = part_of(vec![cut_c(rect(20.0, 10.0, 80.0, 30.0)), cut_c(rect(0.0, 0.0, 100.0, 50.0))]);
        let (r, p) = run(p, |ps, sh, m| bridge(ps, sh, m, 0, [50.0, 30.0], [48.0, 10.0], 4.0, PICK_TOL));
        assert!(r.unwrap().message.contains("holes cut 1 → 2"));
        let l = contours(&p);
        assert_eq!(l.len(), 3);
        let mut areas: Vec<f64> = l[..2].iter().map(|c| dxf::signed_area(&c.pts).abs()).collect();
        areas.sort_by(f64::total_cmp);
        assert!((areas[0] - 560.0).abs() < 1e-6 && (areas[1] - 560.0).abs() < 1e-6, "{areas:?}: two 28 × 20 holes");
        assert_eq!(p.geometry().holes.len(), 2);
    }

    #[test]
    fn a_bridge_across_a_round_hole() {
        let circle: Vec<P> = (0..64).map(|k| {
            let a = k as f64 * std::f64::consts::TAU / 64.0;
            [50.0 + 15.0 * a.cos(), 25.0 + 15.0 * a.sin()]
        }).collect();
        let p = part_of(vec![cut_c(circle), cut_c(rect(0.0, 0.0, 100.0, 50.0))]);
        let (r, p) = run(p, |ps, sh, m| bridge(ps, sh, m, 0, [50.0, 40.0], [50.0, 10.0], 3.0, PICK_TOL));
        assert!(r.unwrap().message.contains("holes cut 1 → 2"));
        assert_eq!(p.geometry().holes.len(), 2);
    }

    #[test]
    fn a_bridge_splitting_the_part_is_refused() {
        // BE10: across the outline itself the part would fall in two.
        let p = part_of(vec![cut_c(rect(0.0, 0.0, 100.0, 50.0))]);
        let (r, _) = run(p, |ps, sh, m| bridge(ps, sh, m, 0, [50.0, 50.0], [50.0, 0.0], 4.0, PICK_TOL));
        assert!(r.unwrap_err().contains("separate pieces"));
    }

    #[test]
    fn a_built_in_shape_becomes_a_drawing_of_its_own_when_edited() {
        // BE11: a plate with a hole; a bridge across the hole.
        let p = Part { name: "DEMO-002".into(), shape: Shape::PlateHole, w: 100.0, h: 60.0, ..part_of(vec![]) };
        let n = contours(&p).len();
        let hole = contours(&p)[0].pts.clone();
        let (cx, cy) = (hole.iter().map(|q| q[0]).sum::<f64>() / hole.len() as f64, hole.iter().map(|q| q[1]).sum::<f64>() / hole.len() as f64);
        let r = hole.iter().map(|q| dist(*q, [cx, cy])).fold(0.0, f64::max);
        let (res, p) = run(p, |ps, sh, m| bridge(ps, sh, m, 0, [cx, cy + r], [cx, cy - r], 2.0, 0.5));
        res.unwrap();
        assert_eq!(p.shape, Shape::Custom);
        assert_eq!(contours(&p).len(), n + 1);
    }

    #[test]
    fn a_nested_copy_keeps_its_place_and_leads_on_unchanged_contours_stay() {
        // BE12: the edit takes a line off; the hole keeps its picked lead-in.
        let mut p = part_of(vec![cut_c(rect(20.0, 10.0, 60.0, 30.0)), cut_c(rect(0.0, 0.0, 100.0, 50.0)), line(vec![[30.0, 30.0], [30.0, 45.0]])]);
        let mut t = crate::nest::PartTech::default();
        t.starts[0] = Some((0, [40.0, 10.0]));
        p.tech = Some(t);
        let pl = oriented(&p, 0, 90.0, false);
        let mut parts = vec![p];
        let mut sheets = vec![vec![Placed { x: 300.0, y: 200.0, ..pl }]];
        let mut manual: Vec<Vec<ManualCut>> = vec![vec![]];
        let spot = sheets[0][0].map([20.0, 10.0]);
        cut(&mut parts, &mut sheets, &mut manual, 0, [25.0, 30.0], &[[30.0, 40.0]], PICK_TOL).unwrap();
        assert_eq!(sheets[0][0].map([20.0, 10.0]), spot);
        assert_eq!(parts[0].tech.as_ref().unwrap().starts[0], Some((0, [40.0, 10.0])));
    }
}
