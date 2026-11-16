//! OMAX waterjet path files (`.omx`). An OMAX waterjet doesn't run G-code: its
//! own software reads a list of points, each carrying the bow of the move that
//! starts there, an OMAX cut quality and the side the kerf goes on, and works
//! out speeds, pierces and the kerf offset itself. So the file holds the path
//! only, no technology (OX24). See MECHANICS.md "OMAX waterjet path files".

use crate::cnc::{Contour, SheetPlan};
use crate::nest::P;
use std::f64::consts::{PI, TAU};

/// OMAX quality numbers (OX38): traverse with the head down (also the file's
/// last record), etch, water only, lead in/out, traverse with the head up,
/// minimum taper.
pub const Q_TRAVERSE: i32 = 0;
pub const Q_ETCH: i32 = 6;
pub const Q_WATER_ONLY: i32 = 8;
pub const Q_LEAD: i32 = 9;
pub const Q_TRAVERSE_UP: i32 = 10;
pub const Q_MIN_TAPER: i32 = 11;
/// Free cut lines (a remnant line, a line drawn on the sheet) are cut at
/// quality 1, the quality the G-code program gives them too (OM2).
pub const Q_FREE_LINE: i32 = 1;

/// The file's extension (OX3).
pub const EXTENSION: &str = ".omx";

const MM_PER_INCH: f64 = 25.4;
/// Points closer than this (mm) are the same point: no zero-length records.
const SAME: f64 = 1e-6;

/// The file's first line, OMAX's own (OX1).
const BANNER: &str = "This is an OMAX (.OMX) file.  Do not modify the first 2 lines of this file. For information on this file format contact softwareengineering@omax.com or visit http://www.omax.com."; // public-check: allow (OMAX's own file banner, not shop data)

/// One record: the point a move starts at (sheet mm), the move's bow
/// (tan of a quarter of its sweep, counter-clockwise positive; 0 straight),
/// its OMAX quality and kerf side (0 on the line, 1 left, 2 right).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Record {
    pub at: P,
    pub bow: f64,
    pub q: i32,
    pub side: u8,
}

/// OX12: the OMAX quality of a contour's cut quality (1–12): 1–5 the same
/// number, 6 minimum taper, 7 water only. 8–12 mean nothing to OMAX (10 would
/// even be read as a traverse), so they have none.
pub fn quality(q: usize) -> Option<i32> {
    match q {
        1..=5 => Some(q as i32),
        6 => Some(Q_MIN_TAPER),
        7 => Some(Q_WATER_ONLY),
        _ => None,
    }
}

/// OX21, OX31: the side the OMAX software puts the kerf on — the contour's
/// scrap side from its real cutting direction (KC2), so it is right whichever
/// way the machine cuts holes: 1 left, 2 right; 0 (on the line) when the
/// path is already the offset path (system compensation) or open.
pub fn side(c: &Contour) -> u8 {
    if c.comp.system {
        return 0;
    }
    match c.comp.side {
        crate::kerf::Side::Left => 1,
        crate::kerf::Side::Right => 2,
        crate::kerf::Side::Off => 0,
    }
}

fn dist(a: P, b: P) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The path as it is written, record by record.
struct Path {
    recs: Vec<Record>,
    cur: P,
}

impl Path {
    /// One move from where the head is to `to`.
    fn seg(&mut self, to: P, bow: f64, q: i32, side: u8) {
        if dist(self.cur, to) < SAME {
            return;
        }
        self.recs.push(Record { at: self.cur, bow: bow.clamp(-1.0, 1.0), q, side });
        self.cur = to;
    }

    /// OX13: every traverse lifts the head (quality 10).
    fn traverse(&mut self, to: P) {
        self.seg(to, 0.0, Q_TRAVERSE_UP, 0);
    }

    /// Cut from where the head is through `pts`. Runs of points on one circle
    /// become arcs (the same fit as the LC5 programs, BL38), so the machine
    /// doesn't slow down at every short straight of a curve.
    fn cut(&mut self, pts: &[P], q: i32, side: u8) {
        let mut all = vec![self.cur];
        all.extend(pts.iter().copied());
        all.dedup_by(|b, a| dist(*a, *b) < SAME);
        if all.len() < 2 {
            return;
        }
        for m in crate::blm::fit_moves(&all) {
            match m {
                crate::blm::Move::Line(p) => self.seg(p, 0.0, q, side),
                crate::blm::Move::Arc { to, center, ccw } => {
                    let a0 = (self.cur[1] - center[1]).atan2(self.cur[0] - center[0]);
                    let a1 = (to[1] - center[1]).atan2(to[0] - center[0]);
                    let mut sweep = a1 - a0;
                    if ccw {
                        while sweep <= 0.0 {
                            sweep += TAU;
                        }
                    } else {
                        while sweep >= 0.0 {
                            sweep -= TAU;
                        }
                    }
                    self.around(center, sweep, q, side, Some(to));
                }
            }
        }
    }

    /// OX8: around `c` by `sweep` radians (counter-clockwise positive) from
    /// where the head is, in pieces of at most a half circle; `end` is where
    /// the last piece ends (else the point the sweep reaches).
    fn around(&mut self, c: P, sweep: f64, q: i32, side: u8, end: Option<P>) {
        let r = dist(self.cur, c);
        if r < SAME || sweep.abs() < 1e-12 {
            return;
        }
        let a0 = (self.cur[1] - c[1]).atan2(self.cur[0] - c[0]);
        let pieces = ((sweep.abs() / PI) - 1e-9).ceil().max(1.0) as usize;
        let step = sweep / pieces as f64;
        for i in 1..=pieces {
            let a = a0 + step * i as f64;
            let to = match end {
                Some(e) if i == pieces => e,
                _ => [c[0] + r * a.cos(), c[1] + r * a.sin()],
            };
            self.seg(to, (step / 4.0).tan(), q, side);
        }
    }
}

/// A closed contour that is one circle and is cut without a break: its
/// centre (the whole circle is then written as half circles, OX8).
fn whole_circle(c: &Contour) -> Option<P> {
    if !c.joints.is_empty() || !c.loops.is_empty() {
        return None;
    }
    match crate::machine::recognise(&c.points) {
        Some(crate::machine::StdShape::Circle { c: centre, .. }) => Some(centre),
        _ => None,
    }
}

/// One cut contour (OX19): traverse to the pierce point, lead-in at quality
/// 9, the contour at its own quality, the lead-out at 9. A micro-joint is a
/// traverse over the gap and the cut starts again after it (the OMAX
/// software pierces there by itself); a joint with leads leads out into the
/// scrap and back in, both at 9. Mirrors the G-code program's walk.
fn contour(p: &mut Path, c: &Contour, q: i32) {
    let side = side(c);
    let n = c.points.len();
    if n == 0 {
        return;
    }
    p.traverse(c.lead_in);
    if c.lead_path.len() >= 2 {
        p.cut(&c.lead_path[1..], Q_LEAD, side);
    }
    p.cut(&[c.points[0]], Q_LEAD, side);
    if let Some(centre) = whole_circle(c) {
        // Round and back to the start, then on by the overlap (or short of
        // the start by a negative one, KC13).
        let r = dist(c.points[0], centre);
        let dir = if crate::kerf::signed_area2(&c.points) >= 0.0 { 1.0 } else { -1.0 };
        p.around(centre, dir * (TAU * r + c.overlap) / r, q, side, None);
        if c.lead_out_path.len() >= 2 {
            p.cut(&c.lead_out_path[1..], Q_LEAD, side);
        }
        return;
    }
    let mut events: Vec<f64> = vec![];
    let mut vertex_at: Vec<f64> = vec![0.0];
    let mut acc = 0.0;
    for k in 0..n {
        acc += dist(c.points[k], c.points[(k + 1) % n]);
        events.push(acc);
        vertex_at.push(acc);
    }
    for j in &c.joints {
        events.push(j.0);
        events.push(j.1);
    }
    if c.overlap < 0.0 {
        let end = acc + c.overlap;
        events.retain(|&e| e < end - 1e-6);
        events.push(end);
    }
    events.sort_by(|a, b| a.partial_cmp(b).unwrap());
    events.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let mut run: Vec<P> = vec![];
    let mut prev = 0.0;
    let mut beam_on = true;
    let mut done_joint: Option<usize> = None;
    let mut ended = false;
    for d in events {
        let at = crate::cnc::point_at(&c.points, d);
        let mid = (prev + d) / 2.0;
        let joint_k = c.joints.iter().position(|j| mid > j.0 && mid < j.1);
        if joint_k.is_some() && joint_k == done_joint {
            prev = d;
            continue;
        }
        if let Some((out, back)) = joint_k.and_then(|k| c.joint_leads.get(k)).filter(|_| beam_on) {
            done_joint = joint_k;
            p.cut(&std::mem::take(&mut run), q, side);
            if out.len() >= 2 {
                p.cut(&out[1..], Q_LEAD, side);
            }
            // The closing joint: the contour ends with its lead-out (MJ5).
            if c.closing && joint_k == Some(c.joints.len() - 1) {
                ended = true;
                break;
            }
            if let Some(&b0) = back.first() {
                p.traverse(b0);
            }
            if back.len() >= 2 {
                p.cut(&back[1..2], Q_LEAD, side);
            }
            prev = d;
            continue;
        }
        if joint_k.is_some() {
            if beam_on {
                p.cut(&std::mem::take(&mut run), q, side);
                beam_on = false;
            }
            p.traverse(at);
        } else {
            beam_on = true;
            run.push(at);
            for (vi, pts) in &c.loops {
                if (vertex_at[*vi] - d).abs() < 1e-6 {
                    run.extend(pts.iter().copied());
                }
            }
        }
        prev = d;
    }
    if !ended {
        if c.overlap > 0.0 {
            run.push(crate::cnc::point_at(&c.points, c.overlap));
        }
        p.cut(&run, q, side);
        if c.lead_out_path.len() >= 2 {
            p.cut(&c.lead_out_path[1..], Q_LEAD, side);
        }
    }
}

/// OM1: what in a sheet's plan an OMAX file can't say: punched contours
/// (a waterjet can't punch) and cut qualities with no OMAX quality (8–12).
/// Export and the agents' `program` stop on them.
pub fn unsupported(plan: &SheetPlan) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for c in &plan.contours {
        let msg = if c.punch.is_some() {
            format!("part #{} has punched contours, which a waterjet can't make: pick a waterjet machine kind in Settings → Machines", c.part_no)
        } else if quality(c.quality).is_none() {
            format!("part #{} has a contour at cut quality {}, which has no OMAX quality: use 1–5 (or 6 minimum taper, 7 water only)", c.part_no, c.quality)
        } else {
            continue;
        };
        if !out.contains(&msg) {
            out.push(msg);
        }
    }
    out
}

/// The records of one sheet (OX6, OX19): start at the sheet's corner, every
/// marking line before its contour as in the G-code program (etch, on the
/// line), each contour, the free cut lines and remnant marks (`lines`, as
/// `engine::program_blocks` gives them), then the last point at quality 0.
pub fn records(plan: &SheetPlan, lines: &[(String, Vec<P>)]) -> Vec<Record> {
    let mut p = Path { recs: vec![], cur: [0.0, 0.0] };
    let mut written = vec![false; plan.marks.len()];
    for (k, c) in plan.contours.iter().enumerate() {
        for j in plan.marks_due(c.part_no, k, &mut written) {
            let m = &plan.marks[j].1;
            if let Some(&first) = m.first() {
                p.traverse(first);
                p.cut(&m[1..], Q_ETCH, 0);
            }
        }
        if c.punch.is_some() {
            continue;
        }
        let Some(q) = quality(c.quality) else { continue };
        contour(&mut p, c, q);
    }
    for (label, pts) in lines {
        let Some(&first) = pts.first() else { continue };
        p.traverse(first);
        p.cut(&pts[1..], if label.ends_with("MARK") { Q_ETCH } else { Q_FREE_LINE }, 0);
    }
    let end = p.cur;
    p.recs.push(Record { at: end, bow: 0.0, q: Q_TRAVERSE, side: 0 });
    p.recs
}

/// A number as the file writes it: inches or bow with exactly 9 decimals,
/// never `-0.000000000` (OX7).
fn num(v: f64) -> String {
    let v = if v.abs() < 5e-10 { 0.0 } else { v };
    format!("{v:.9}")
}

/// One record line (OX6, OX39): `[0], X, Y, Z, tilt start, tilt end, bow,
/// quality, side, R, R, R, 0, 0, 0,[END]`, X and Y in inches, flat cutting
/// (Z and the tilts 0, no extra data).
pub fn record_line(r: &Record) -> String {
    format!(
        "[0], {}, {}, 0.000000000, 0.000000000, 0.000000000, {}, {}, {}, R, R, R, 0, 0, 0,[END]",
        num(r.at[0] / MM_PER_INCH),
        num(r.at[1] / MM_PER_INCH),
        num(r.bow),
        r.q,
        r.side
    )
}

/// OX40: the header's date — days since 1899-12-30 and the fraction of the
/// day (UTC), the serial OMAX's own files carry.
pub fn date_now() -> f64 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    25_569.0 + secs / 86_400.0
}

/// A sheet's `.omx` file (OX1, OX40): OMAX's banner, `2` (tool path data),
/// file version `2` as OMAX's own layout program writes it, the date, six
/// reserved lines, a comment block saying what the sheet is, an empty
/// variables block, then the records. Lines end CR LF.
#[allow(clippy::too_many_arguments)]
pub fn program(sheet_no: usize, placed_count: usize, spec: &crate::nest::SheetSpec, plan: &SheetPlan, lines: &[(String, Vec<P>)], date: f64) -> String {
    let mut s = String::new();
    let mut put = |t: &str| {
        s.push_str(t);
        s.push_str("\r\n");
    };
    put(BANNER);
    put("2");
    put("2");
    let d = format!("{date:.8}");
    put(d.trim_end_matches('0').trim_end_matches('.'));
    for _ in 0..6 {
        put("[Reserved]");
    }
    put("[COMMENT]");
    put(&format!("Sheet {sheet_no}: {:.0} x {:.0} x {} mm {}", spec.w, spec.h, spec.thickness, crate::engine::material::nearest_name(spec.material)));
    put(&format!("{placed_count} parts, {} pierces, cut {:.0} mm", plan.pierces, plan.cut_len));
    put("[END]");
    put("[VARIABLES]");
    put("[END]");
    for r in records(plan, lines) {
        put(&record_line(&r));
    }
    s
}

/// Read a `.omx` file's records back (sheet mm): every `[0], …,[END]` line.
pub fn read(text: &str) -> Vec<Record> {
    let mut out = vec![];
    for l in text.lines() {
        let l = l.trim();
        let Some(body) = l.strip_prefix("[0],").and_then(|b| b.strip_suffix(",[END]")) else { continue };
        let f: Vec<&str> = body.split(',').map(str::trim).collect();
        if f.len() < 8 {
            continue;
        }
        let v = |i: usize| f[i].parse::<f64>().unwrap_or(f64::NAN);
        out.push(Record { at: [v(0) * MM_PER_INCH, v(1) * MM_PER_INCH], bow: v(5), q: v(6) as i32, side: v(7) as u8 });
    }
    out
}

/// Length (mm) of the move from `a` to `b` with bow `bow`.
fn move_len(a: P, b: P, bow: f64) -> f64 {
    let chord = dist(a, b);
    let sweep = 4.0 * bow.atan();
    if sweep.abs() < 1e-9 {
        return chord;
    }
    let r = chord / (2.0 * (sweep / 2.0).sin().abs());
    r * sweep.abs()
}

fn is_traverse(q: i32) -> bool {
    q == Q_TRAVERSE || q == Q_TRAVERSE_UP
}

/// What a file cuts: through-cut length (mm, leads included, etching and
/// traverses not) and pierces (a cut that starts after a traverse, or the
/// first one).
pub fn cut_and_pierces(recs: &[Record]) -> (f64, usize) {
    let (mut len, mut pierces) = (0.0, 0);
    let mut after_traverse = true;
    for w in recs.windows(2) {
        let r = w[0];
        if is_traverse(r.q) {
            after_traverse = true;
            continue;
        }
        if r.q == Q_ETCH {
            after_traverse = true;
            continue;
        }
        if after_traverse {
            pierces += 1;
            after_traverse = false;
        }
        len += move_len(r.at, w[1].at, r.bow);
    }
    (len, pierces)
}

fn poly_len(pts: &[P]) -> f64 {
    pts.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// OM3: before a sheet's file is written it is read back and compared with
/// the plan — the same pierces, and a cut length within 2 % + 0.5 mm (arcs
/// are written as true arcs where the plan holds chords). Empty when they
/// agree.
pub fn check(text: &str, plan: &SheetPlan, lines: &[(String, Vec<P>)]) -> Vec<String> {
    let recs = read(text);
    let mut issues = vec![];
    if recs.last().is_none_or(|r| r.q != Q_TRAVERSE) {
        issues.push("the file doesn't end with a quality 0 record".to_string());
    }
    let (mut want_len, mut want_pierces) = (0.0, 0);
    for c in plan.contours.iter().filter(|c| c.punch.is_none() && quality(c.quality).is_some()) {
        for run in crate::cnc::beam_runs(c) {
            want_len += poly_len(&run);
            want_pierces += 1;
        }
    }
    for (_, pts) in lines.iter().filter(|(l, p)| !l.ends_with("MARK") && p.len() >= 2) {
        want_len += poly_len(pts);
        want_pierces += 1;
    }
    let (len, pierces) = cut_and_pierces(&recs);
    if pierces != want_pierces {
        issues.push(format!("the file pierces {pierces} times where the plan pierces {want_pierces}"));
    }
    if (len - want_len).abs() > 0.02 * want_len + 0.5 {
        issues.push(format!("the file cuts {len:.0} mm where the plan cuts {want_len:.0} mm"));
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(x: f64, y: f64, w: f64, h: f64) -> Vec<P> {
        vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]]
    }

    fn circle(c: P, r: f64, n: usize, ccw: bool) -> Vec<P> {
        (0..n)
            .map(|i| {
                let a = TAU * i as f64 / n as f64 * if ccw { 1.0 } else { -1.0 };
                [c[0] + r * a.cos(), c[1] + r * a.sin()]
            })
            .collect()
    }

    fn contour(points: Vec<P>, inner: bool, lead: Vec<P>, quality: usize) -> Contour {
        let side = crate::kerf::side_for(&points, inner, true);
        Contour {
            lead_in: lead.first().copied().unwrap_or(points[0]),
            lead_path: lead,
            lead_out: None,
            lead_out_path: vec![],
            loops: vec![],
            points,
            inner,
            part_no: 1,
            joints: vec![],
            joint_leads: vec![],
            manual: false,
            feed: 1000.0,
            punch: None,
            overlap: 0.0,
            closing: false,
            comp: crate::kerf::ContourComp { side, control: true, corrector: 1, system: false, radius: 0.4 },
            quality,
            time: Default::default(),
            pierces: 1,
            contour: 0,
            lead_rec: 0,
            lead_hand: false,
            lead_spots: Default::default(),
        }
    }

    fn spec() -> crate::nest::SheetSpec {
        crate::nest::SheetSpec { w: 3000.0, h: 1500.0, margin: 10.0, gap: 5.0, material: 0, thickness: 10.0, level: 3, machine: crate::machine::MachineKind::Waterjet, kerf: Default::default(), gas: None, pick: Default::default() }
    }

    fn plan(contours: Vec<Contour>) -> SheetPlan {
        let pierces = contours.len();
        SheetPlan {
            contours,
            kerf_issues: vec![],
            marks: vec![],
            mark_before: vec![],
            mark_len: 0.0,
            cut_len: 0.0,
            rapid_len: 0.0,
            pierces,
            seconds: 0.0,
            times: Default::default(),
            timing: crate::cnc::plan(&[], &[], &spec()).timing,
        }
    }

    #[test]
    fn a_plate_with_a_hole_reads_as_omax_writes_it() {
        // A 100 × 50 plate cut clockwise, scrap on its left; a Ø20 hole cut
        // clockwise, scrap (the slug) on its right (OX31).
        let hole = contour(circle([50.0, 25.0], 10.0, 72, false), true, vec![[55.0, 25.0], [60.0, 25.0]], 3);
        let outside = contour(vec![[0.0, 0.0], [0.0, 50.0], [100.0, 50.0], [100.0, 0.0]], false, vec![[0.0, -5.0], [0.0, 0.0]], 3);
        let p = plan(vec![hole, outside]);
        let recs = records(&p, &[]);
        let qs: Vec<(i32, u8)> = recs.iter().map(|r| (r.q, r.side)).collect();
        // Traverse to the hole's pierce point, its lead-in, two half circles,
        // traverse, the outside's lead-in, four sides, the end.
        assert_eq!(qs, vec![(10, 0), (9, 2), (3, 2), (3, 2), (10, 0), (9, 1), (3, 1), (3, 1), (3, 1), (3, 1), (0, 0)], "{recs:?}");
        // Half circles clockwise: bow −1.
        assert!((recs[2].bow + 1.0).abs() < 1e-9 && (recs[3].bow + 1.0).abs() < 1e-9, "{recs:?}");
        assert_eq!(recs[2].at, [60.0, 25.0]);
        let text = program(1, 1, &spec(), &p, &[], 46342.5);
        let lines: Vec<&str> = text.split("\r\n").collect();
        assert!(lines[0].starts_with("This is an OMAX (.OMX) file."));
        assert_eq!((lines[1], lines[2], lines[3]), ("2", "2", "46342.5"));
        assert_eq!(lines[4..10], ["[Reserved]"; 6]);
        // 55 mm = 2.165354331 in; the hole's lead-in record.
        assert!(text.contains("[0], 2.165354331, 0.984251969, 0.000000000, 0.000000000, 0.000000000, 0.000000000, 9, 2, R, R, R, 0, 0, 0,[END]\r\n"), "{text}");
        assert!(text.ends_with(",[END]\r\n") && !text.contains("-0.000000000"));
        // Read back, it cuts what the plan cuts.
        assert_eq!(read(&text).len(), recs.len());
        assert!(check(&text, &p, &[]).is_empty(), "{:?}", check(&text, &p, &[]));
    }

    #[test]
    fn a_hole_cut_the_other_way_has_its_kerf_on_the_left() {
        // OX31: the side follows the cutting direction, not a fixed rule.
        let hole = contour(circle([50.0, 25.0], 10.0, 72, true), true, vec![], 2);
        let recs = records(&plan(vec![hole]), &[]);
        assert!(recs.iter().filter(|r| r.q == 2).all(|r| r.side == 1 && r.bow > 0.0), "{recs:?}");
    }

    #[test]
    fn rounded_corners_become_arcs_and_long_arcs_are_split() {
        // A slot: two straights and two half circles of radius 10, drawn as
        // chords; each half circle comes back as one arc of bow ±1 or as two
        // pieces, never as dozens of straights.
        let mut pts = vec![];
        for i in 0..=36 {
            let a = -PI / 2.0 + PI * i as f64 / 36.0;
            pts.push([60.0 + 10.0 * a.cos(), 20.0 + 10.0 * a.sin()]);
        }
        for i in 0..=36 {
            let a = PI / 2.0 + PI * i as f64 / 36.0;
            pts.push([20.0 + 10.0 * a.cos(), 20.0 + 10.0 * a.sin()]);
        }
        let c = contour(pts, false, vec![], 3);
        let p = plan(vec![c]);
        let recs = records(&p, &[]);
        assert!(recs.len() < 10, "{recs:?}");
        assert!(recs.iter().all(|r| r.bow.abs() <= 1.0 + 1e-12));
        assert!(check(&program(1, 1, &spec(), &p, &[], 1.0), &p, &[]).is_empty());
    }

    #[test]
    fn micro_joints_are_traverses_and_the_cut_starts_again() {
        let mut c = contour(sq(10.0, 10.0, 100.0, 50.0), false, vec![], 3);
        c.joints = vec![(40.0, 41.0)];
        let p = plan(vec![c]);
        let recs = records(&p, &[]);
        let qs: Vec<i32> = recs.iter().map(|r| r.q).collect();
        assert_eq!(qs, vec![10, 3, 10, 3, 3, 3, 3, 0], "{recs:?}");
        assert_eq!(recs[2].at, [50.0, 10.0]);
        let (_, pierces) = cut_and_pierces(&recs);
        assert_eq!(pierces, 2);
    }

    #[test]
    fn qualities_without_an_omax_meaning_stop_the_file() {
        assert_eq!((quality(1), quality(5), quality(6), quality(7), quality(8), quality(10)), (Some(1), Some(5), Some(11), Some(8), None, None));
        let p = plan(vec![contour(sq(0.0, 0.0, 10.0, 10.0), false, vec![], 10)]);
        let why = unsupported(&p);
        assert_eq!(why.len(), 1);
        assert!(why[0].contains("quality 10"), "{why:?}");
    }

    #[test]
    fn cut_lines_are_cut_on_the_line_and_remnant_marks_etched() {
        let p = plan(vec![]);
        let lines = vec![("REMNANT MARK".to_string(), vec![[10.0, 10.0], [20.0, 10.0]]), ("REMNANT LINE".to_string(), vec![[0.0, 600.0], [3000.0, 600.0]])];
        let recs = records(&p, &lines);
        let qs: Vec<(i32, u8)> = recs.iter().map(|r| (r.q, r.side)).collect();
        assert_eq!(qs, vec![(10, 0), (6, 0), (10, 0), (1, 0), (0, 0)]);
        let text = program(1, 0, &spec(), &p, &lines, 1.0);
        assert!(check(&text, &p, &lines).is_empty(), "{:?}", check(&text, &p, &lines));
        // A file that lost the line is caught.
        assert!(!check(&text, &p, &[lines[1].clone(), lines[1].clone()]).is_empty());
    }
}
