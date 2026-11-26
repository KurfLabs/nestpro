//! Grain direction on parts (GR1–GR6): a part drawn the way the sheet's grain
//! must run through it, and turning a part's own drawing so it is.
//!
//! The sheet's grain runs along its X, the length (the first number of a
//! 3000 × 1500 sheet); remnants keep the axes of the sheet they came from. A
//! part kept to the grain is nested only at 0° and 180° (a half turn still
//! runs the same way along the grain), or only at 0° when it is fixed.

use crate::dxf::{triangulate, CustomGeo};
use crate::nest::{oriented, Part, Placed, Shape, Spin, P};

/// How a part may be turned against the sheet's grain (GR2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lock {
    /// The part's own rotations decide (R1).
    Free,
    /// Kept to the grain: 0° and 180°.
    Grain,
    /// Kept to the grain and never turned: 0° only.
    Fixed,
}

impl Lock {
    pub const ALL: [Lock; 3] = [Lock::Free, Lock::Grain, Lock::Fixed];

    /// The word agents use.
    pub fn key(self) -> &'static str {
        match self {
            Lock::Free => "free",
            Lock::Grain => "grain",
            Lock::Fixed => "fixed",
        }
    }

    pub fn from_key(s: &str) -> Option<Lock> {
        Lock::ALL.into_iter().find(|l| l.key() == s.trim().to_ascii_lowercase())
    }

    /// For people.
    pub fn label(self) -> &'static str {
        match self {
            Lock::Free => "Free",
            Lock::Grain => "Keep grain (0°, 180°)",
            Lock::Fixed => "Fixed (0° only)",
        }
    }

    /// What it does, in a sentence.
    pub fn help(self) -> &'static str {
        match self {
            Lock::Free => "No grain direction: the nester turns the part any way its rotations allow.",
            Lock::Grain => "The grain runs along the part's X as drawn: the nester uses 0° or 180° only, never across the grain.",
            Lock::Fixed => "The grain runs along the part's X as drawn, and the part is never turned: 0° only.",
        }
    }
}

/// The part's grain lock (GR2): Fixed is a grain part whose 180° isn't allowed.
pub fn lock_of(p: &Part) -> Lock {
    if !p.rots.grain {
        Lock::Free
    } else if p.rots.plain[2] || p.rots.mirrored[2] {
        Lock::Grain
    } else {
        Lock::Fixed
    }
}

/// Set the part's grain lock (GR2). The 90° and 270° permissions are left as
/// they were, so going back to Free gives the part its old rotations again.
pub fn set_lock(p: &mut Part, lock: Lock) {
    let was = lock_of(p);
    let r = &mut p.rots;
    match lock {
        Lock::Free => {
            if was == Lock::Fixed {
                r.plain[2] = true;
                r.mirrored[2] = r.mirrored[0];
            }
            r.grain = false;
        }
        Lock::Grain => {
            r.grain = true;
            r.plain[0] = true;
            r.plain[2] = true;
            let m = r.mirrored[0] || r.mirrored[2];
            r.mirrored[0] = m;
            r.mirrored[2] = m;
        }
        Lock::Fixed => {
            r.grain = true;
            r.plain[0] = true;
            r.plain[2] = false;
            r.mirrored[2] = false;
        }
    }
}

/// What turning a part's drawing did.
#[derive(Debug)]
pub struct Turned {
    pub message: String,
    /// Copies of the part were already on sheets: they stay where they are,
    /// but the nest may now go against the grain.
    pub nested: bool,
}

/// Turn part `i`'s own drawing `deg` degrees counter-clockwise (90, 180 or
/// 270; negative turns clockwise), with everything programmed on it (GR3):
/// contours, marking lines, texts, lead-ins picked or moved by hand and
/// micro-joints. A part that had no grain lock is kept to the grain from now
/// on (GR4). Copies already nested stay exactly where they are on the sheet
/// (GR5).
pub fn turn_part(parts: &mut [Part], sheets: &mut [Vec<Placed>], i: usize, deg: i32) -> Result<Turned, String> {
    let Some(old) = parts.get(i) else { return Err(format!("There is no part {} (the job has {}).", i + 1, parts.len())) };
    if deg % 90 != 0 {
        return Err(format!("A part's drawing turns in quarter turns: say 90, 180, 270 or -90, not {deg}. To place a nested part at another angle, turn it on the sheet (turn angle=…)."));
    }
    let k = (deg / 90).rem_euclid(4) as u8;
    if k == 0 {
        return Err("A turn of 0° leaves the part as it is: say 90 (left), -90 (right) or 180.".into());
    }
    let mut p = old.clone();
    let mut geo = match (&p.shape, &p.custom) {
        (Shape::Custom, Some(g)) => (**g).clone(),
        _ => drawn(&p),
    };
    let (w, h) = (geo.w, geo.h);
    let (nw, nh) = if k % 2 == 1 { (h, w) } else { (w, h) };
    // The quarter turn a placement makes, about the part's own box (R1).
    let t = Placed { part: i, x: 0.0, y: 0.0, w: nw, h: nh, turns: k, mirror: false, spin: Spin::NONE };
    let f = |q: P| t.map(q);
    let all = |v: &mut Vec<P>| v.iter_mut().for_each(|q| *q = f(*q));
    all(&mut geo.outline);
    geo.holes.iter_mut().for_each(all);
    geo.fills.iter_mut().for_each(all);
    geo.hole_fills.iter_mut().for_each(all);
    geo.marks.iter_mut().for_each(all);
    geo.contours.iter_mut().for_each(|c| all(&mut c.pts));
    let turn_text = |x: &mut crate::nest::PartText| {
        x.at = f(x.at);
        x.angle = crate::nest::norm_angle(x.angle + 90.0 * k as f64);
    };
    geo.texts.iter_mut().for_each(turn_text);
    p.texts.iter_mut().for_each(turn_text);
    (geo.w, geo.h) = (nw, nh);
    if let Some(tech) = p.tech.as_mut() {
        for s in tech.starts.iter_mut().flatten() {
            s.1 = f(s.1);
        }
        for s in tech.joint_picks.iter_mut().flatten() {
            s.1 = f(s.1);
        }
        for e in tech.lead_edits.iter_mut().flatten() {
            e.at = f(e.at);
        }
    }
    p.shape = Shape::Custom;
    p.custom = Some(std::sync::Arc::new(geo));
    (p.w, p.h) = (nw, nh);
    let locked = lock_of(&p) == Lock::Free;
    if locked {
        set_lock(&mut p, Lock::Grain);
    }

    // Copies on sheets keep their place: the same points land on the same
    // spots, the placement turning the other way to make up for it (a
    // mirrored copy the same way, as the mirror comes first).
    let mut nested = false;
    for pl in sheets.iter_mut().flatten().filter(|pl| pl.part == i) {
        nested = true;
        let angle = if pl.mirror { pl.angle() + 90.0 * k as f64 } else { pl.angle() - 90.0 * k as f64 };
        let mut np = oriented(&p, i, angle, pl.mirror);
        let (want, got) = (pl.map([0.0, 0.0]), np.map(f([0.0, 0.0])));
        np.x += want[0] - got[0];
        np.y += want[1] - got[1];
        *pl = np;
    }
    parts[i] = p;

    let p = &parts[i];
    let how = match deg.rem_euclid(360) {
        90 => "90° left",
        180 => "180°",
        _ => "90° right",
    };
    let u = |v: f64| crate::nest::angle_num(v);
    let mut message = format!("Turned '{}' {how}: it is now {} × {} mm, and the sheet's grain runs along its X ({} mm).", p.name, u(p.w), u(p.h), u(p.w));
    if locked {
        message.push_str(" It is kept to the grain from now on (0° and 180° only); Grain → Free lets the nester turn it again.");
    }
    if nested {
        message.push_str(" Its nested copies stay where they are: nest again so they follow the grain.");
    }
    Ok(Turned { message, nested })
}

/// A part of one of the built-in shapes as a drawing of its own, so it can be
/// turned: the same outline and holes, cut the same way.
pub(crate) fn drawn(p: &Part) -> CustomGeo {
    let g = p.geometry();
    let (w, h) = p.bbox();
    CustomGeo {
        fills: triangulate(&g.outline),
        hole_fills: g.holes.iter().flat_map(|q| triangulate(q)).collect(),
        outline: g.outline,
        holes: g.holes,
        marks: vec![],
        texts: p.texts.clone(),
        w,
        h,
        contours: vec![],
        hand_order: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nest::{turn_allowed, Shape};

    fn part(shape: Shape, w: f64, h: f64) -> Part {
        Part { name: "DEMO-001".into(), shape, w, h, qty: 4, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
    }

    #[test]
    fn a_grain_part_is_nested_at_0_and_180_only_and_a_fixed_one_at_0() {
        let mut p = part(Shape::Rectangle, 200.0, 100.0);
        p.rots.mirrored = [true; 4];
        set_lock(&mut p, Lock::Grain);
        assert_eq!(lock_of(&p), Lock::Grain);
        let ok = |p: &Part, m| (0..4u8).filter(|&k| turn_allowed(p, k, m)).collect::<Vec<_>>();
        assert_eq!((ok(&p, false), ok(&p, true)), (vec![0, 2], vec![0, 2]));
        assert!(!crate::nest::angle_allowed(&p, 30.0, false), "no free angles across the grain");
        set_lock(&mut p, Lock::Fixed);
        assert_eq!((lock_of(&p), ok(&p, false), ok(&p, true)), (Lock::Fixed, vec![0], vec![0]));
        // Free gives the part its old rotations back.
        set_lock(&mut p, Lock::Free);
        assert_eq!((lock_of(&p), ok(&p, false)), (Lock::Free, vec![0, 1, 2, 3]));
    }

    #[test]
    fn turning_an_l_bracket_turns_its_drawing_and_keeps_it_to_the_grain() {
        let mut parts = vec![part(Shape::LBracket, 200.0, 160.0)];
        let before = parts[0].geometry();
        let done = turn_part(&mut parts, &mut [], 0, 90).unwrap();
        let p = &parts[0];
        assert_eq!((p.shape, p.w, p.h, lock_of(p)), (Shape::Custom, 160.0, 200.0, Lock::Grain));
        assert!(done.message.contains("kept to the grain"), "{}", done.message);
        // A quarter turn left: the long leg that ran along X now runs up.
        let after = p.geometry();
        assert_eq!(after.outline.len(), before.outline.len());
        for (a, b) in before.outline.iter().zip(&after.outline) {
            assert!((b[0] - (160.0 - a[1])).abs() < 1e-9 && (b[1] - a[0]).abs() < 1e-9, "{a:?} → {b:?}");
        }
        // Four quarter turns bring it back.
        for _ in 0..3 {
            turn_part(&mut parts, &mut [], 0, -270).unwrap();
        }
        let back = parts[0].geometry();
        for (a, b) in before.outline.iter().zip(&back.outline) {
            assert!((a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9);
        }
    }

    #[test]
    fn nested_copies_stay_on_the_same_spot_after_the_drawing_turns() {
        let mut parts = vec![part(Shape::LBracket, 200.0, 160.0)];
        let spot = |pl: &Placed, p: &Part| p.geometry().outline.iter().map(|&q| pl.map(q)).collect::<Vec<P>>();
        for (turns, mirror) in [(0, false), (1, false), (2, true), (3, true)] {
            let mut ps = parts.clone();
            let mut pl = oriented(&ps[0], 0, turns as f64 * 90.0, mirror);
            (pl.x, pl.y) = (500.0, 300.0);
            let mut sheets = vec![vec![pl]];
            let want = spot(&pl, &ps[0]);
            for deg in [90, 180, -90] {
                let done = turn_part(&mut ps, &mut sheets, 0, deg).unwrap();
                assert!(done.nested);
                let mut got = spot(&sheets[0][0], &ps[0]);
                let mut want = want.clone();
                let key = |a: &P, b: &P| (a[0], a[1]).partial_cmp(&(b[0], b[1])).unwrap();
                got.sort_by(key);
                want.sort_by(key);
                for (a, b) in want.iter().zip(&got) {
                    assert!((a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6, "turns {turns} mirror {mirror} by {deg}: {a:?} vs {b:?}");
                }
            }
        }
        parts.clear();
    }

    #[test]
    fn picked_lead_ins_and_texts_turn_with_the_drawing() {
        let mut p = part(Shape::Rectangle, 200.0, 100.0);
        let mut t = crate::nest::PartTech::default();
        t.starts[0] = Some((0, [200.0, 50.0]));
        p.tech = Some(t);
        p.texts.push(crate::nest::PartText { text: "#REF".into(), at: [10.0, 10.0], height: 10.0, angle: 0.0, curve: 0.0, kind: crate::nest::TextKind::Special });
        let mut parts = vec![p];
        turn_part(&mut parts, &mut [], 0, 90).unwrap();
        let p = &parts[0];
        assert_eq!(p.tech.as_ref().unwrap().starts[0], Some((0, [50.0, 200.0])));
        assert_eq!((p.texts[0].at, p.texts[0].angle), ([90.0, 10.0], 90.0));
    }

    #[test]
    fn a_turn_that_is_not_a_quarter_turn_is_refused_with_what_to_do() {
        let mut parts = vec![part(Shape::Rectangle, 200.0, 100.0)];
        assert!(turn_part(&mut parts, &mut [], 0, 45).unwrap_err().contains("turn angle="));
        assert!(turn_part(&mut parts, &mut [], 0, 360).is_err());
        assert!(turn_part(&mut parts, &mut [], 3, 90).is_err());
    }
}
