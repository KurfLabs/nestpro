//! Drawings for the Micro-joints window: the four joint types, one joint
//! with its sizes marked (the size boxes sit on these marks), the spacing
//! along a part and the small / medium / large size map. Drawn on the
//! dark Part view canvas with the Part view's colours, so a joint here
//! looks like a joint on the part.

use crate::nest::{AutoJoints, JointType};
use crate::theme::th;
use eframe::egui;
use egui::{Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2};

pub const LEAD_IN: Color32 = Color32::WHITE;
pub const LEAD_OUT: Color32 = Color32::from_rgb(129, 140, 248);
pub const JOINT: Color32 = Color32::from_rgb(250, 204, 21);
pub const PIERCE: Color32 = Color32::from_rgb(248, 80, 90);
const CUT: Color32 = Color32::from_rgb(205, 210, 222);
const OFF: Color32 = Color32::from_rgb(130, 134, 146);
/// The part's material, lighter than the canvas so the part reads as solid.
const MATERIAL: Color32 = Color32::from_rgb(44, 58, 82);
const LABEL: Color32 = Color32::from_rgb(150, 156, 172);
const DIM: Color32 = Color32::from_rgb(96, 165, 250);
/// Size classes, the same in the map and in the count grid.
pub const SMALL: Color32 = Color32::from_rgb(74, 222, 128);
pub const MEDIUM: Color32 = Color32::from_rgb(251, 191, 36);
pub const LARGE: Color32 = Color32::from_rgb(96, 165, 250);
/// A "no limit" length (rule MJ4 / P7 placeholder 99999.999).
pub const NO_LIMIT_MM: f64 = 99999.999;
/// A count at or above this is "no limit" (the machine files use 32767).
pub const NO_LIMIT_COUNT: i32 = 32767;

pub fn is_no_limit(mm: f64) -> bool {
    mm >= 99999.0
}

/// Maps drawing units (`w` wide) onto a screen rect.
struct Pic {
    o: Pos2,
    s: f32,
}

impl Pic {
    fn new(r: Rect, w: f32) -> Self {
        Pic { o: r.min, s: r.width() / w }
    }
    fn p(&self, x: f32, y: f32) -> Pos2 {
        self.o + Vec2::new(x, y) * self.s
    }
}

fn line(p: &Painter, a: Pos2, b: Pos2, w: f32, c: Color32) {
    p.line_segment([a, b], Stroke::new(w, c));
}

fn dashed(p: &Painter, pts: &[Pos2], c: Color32) {
    p.extend(Shape::dashed_line(pts, Stroke::new(1.2, c), 4.0, 3.0));
}

fn head(p: &Painter, tip: Pos2, from: Pos2, c: Color32, size: f32) {
    let d = (tip - from).normalized();
    let n = Vec2::new(-d.y, d.x);
    p.add(Shape::convex_polygon(vec![tip, tip - d * size + n * size * 0.5, tip - d * size - n * size * 0.5], c, Stroke::NONE));
}

/// A measured size: a line with a head at each end.
fn dim(p: &Painter, a: Pos2, b: Pos2) {
    line(p, a, b, 1.0, DIM);
    head(p, a, b, DIM, 6.0);
    head(p, b, a, DIM, 6.0);
}

fn label(p: &Painter, at: Pos2, align: Align2, s: &str, c: Color32) {
    p.text(at, align, s, FontId::proportional(10.5), c);
}

fn canvas(p: &Painter, r: Rect) {
    p.rect_filled(r, CornerRadius::same(6), th().canvas_b);
    p.rect_stroke(r, CornerRadius::same(6), Stroke::new(1.0, th().control_edge), egui::StrokeKind::Inside);
}

/// The lead angle, from the contour on the side away from the gap to the
/// lead (rule AM6: the lean is D / tan A).
fn angle_arc(p: &Painter, foot: Pos2, lead_end: Pos2, r: f32) {
    let a0 = std::f32::consts::PI;
    let a1 = (lead_end - foot).angle().rem_euclid(std::f32::consts::TAU);
    let pts: Vec<Pos2> = (0..=10).map(|k| a0 + (a1 - a0) * k as f32 / 10.0).map(|a| foot + Vec2::angled(a) * r).collect();
    p.add(Shape::line(pts, Stroke::new(1.0, DIM)));
}

/// The little picture on a joint-type tile (drawn in an 80 × 40 box).
pub fn type_tile(p: &Painter, r: Rect, jt: JointType) {
    canvas(p, r);
    let q = Pic::new(r, 80.0);
    let w = 2.2 * q.s;
    let mat = MATERIAL;
    if jt == JointType::Corner {
        p.rect_filled(Rect::from_min_max(q.p(0.0, 16.0), q.p(50.0, 40.0)), CornerRadius::ZERO, mat);
        line(p, q.p(6.0, 16.0), q.p(42.0, 16.0), w, CUT);
        line(p, q.p(50.0, 24.0), q.p(50.0, 38.0), w, CUT);
        line(p, q.p(42.0, 16.0), q.p(33.0, 4.0), w, LEAD_OUT);
        line(p, q.p(64.0, 31.0), q.p(50.0, 24.0), w, LEAD_IN);
        p.circle_filled(q.p(50.0, 16.0), 2.6 * q.s, JOINT);
        return;
    }
    p.rect_filled(Rect::from_min_max(q.p(0.0, 25.0), q.p(80.0, 40.0)), CornerRadius::ZERO, mat);
    line(p, q.p(5.0, 25.0), q.p(33.0, 25.0), w, CUT);
    line(p, q.p(47.0, 25.0), q.p(75.0, 25.0), w, CUT);
    match jt {
        JointType::Leads => {
            line(p, q.p(33.0, 25.0), q.p(33.0, 7.0), w, LEAD_OUT);
            line(p, q.p(47.0, 7.0), q.p(47.0, 25.0), w, LEAD_IN);
        }
        JointType::LeadsInclined => {
            line(p, q.p(33.0, 25.0), q.p(22.0, 7.0), w, LEAD_OUT);
            line(p, q.p(58.0, 7.0), q.p(47.0, 25.0), w, LEAD_IN);
        }
        _ => {}
    }
    p.circle_filled(q.p(40.0, 25.0), 2.6 * q.s, JOINT);
}

/// Where the size boxes go on the joint drawing (screen points).
#[derive(Default)]
pub struct Marks {
    pub gap: Pos2,
    pub lead_out: Option<Pos2>,
    pub lead_in: Option<Pos2>,
    pub angle: Option<Pos2>,
}

/// Height of the joint drawing for a type, at width `w`.
pub fn joint_height(jt: JointType, w: f32) -> f32 {
    let h = match jt {
        JointType::Gap => 96.0,
        JointType::Corner => 136.0,
        _ => 124.0,
    };
    h * w / 336.0
}

/// One joint, large, with its sizes marked: the gap, how far each lead goes
/// out (perpendicular distance) and the lead angle. Returns where each
/// size's box belongs.
pub fn joint(p: &Painter, r: Rect, jt: JointType) -> Marks {
    canvas(p, r);
    let q = Pic::new(r, 336.0);
    let w = 2.4;
    let mat = MATERIAL;
    label(p, q.p(8.0, 6.0), Align2::LEFT_TOP, "scrap", LABEL);
    let arrow = |x: f32, y: f32| head(p, q.p(x + 6.0, y), q.p(x, y), CUT, 8.0);
    if jt == JointType::Corner {
        p.rect_filled(Rect::from_min_max(q.p(0.0, 64.0), q.p(190.0, 136.0)), CornerRadius::ZERO, mat);
        label(p, q.p(8.0, 130.0), Align2::LEFT_BOTTOM, "part", LABEL);
        line(p, q.p(14.0, 64.0), q.p(168.0, 64.0), w, CUT);
        arrow(80.0, 64.0);
        line(p, q.p(190.0, 86.0), q.p(190.0, 134.0), w, CUT);
        head(p, q.p(190.0, 120.0), q.p(190.0, 112.0), CUT, 8.0);
        line(p, q.p(168.0, 64.0), q.p(146.0, 24.0), w, LEAD_OUT);
        line(p, q.p(234.0, 108.0), q.p(190.0, 86.0), w, LEAD_IN);
        // Beam off from the lead-out's end round to the re-pierce.
        let pts: Vec<Pos2> = (0..=16)
            .map(|k| {
                let t = k as f32 / 16.0;
                let (a, c, b) = ((146.0, 24.0), (260.0, 16.0), (234.0, 108.0));
                let u = 1.0 - t;
                q.p(u * u * a.0 + 2.0 * u * t * c.0 + t * t * b.0, u * u * a.1 + 2.0 * u * t * c.1 + t * t * b.1)
            })
            .collect();
        dashed(p, &pts, OFF);
        p.circle_stroke(q.p(234.0, 108.0), 4.5, Stroke::new(1.5, PIERCE));
        label(p, q.p(242.0, 112.0), Align2::LEFT_CENTER, "re-pierce", PIERCE);
        dashed(p, &[q.p(168.0, 64.0), q.p(190.0, 64.0), q.p(190.0, 86.0)], JOINT);
        p.circle_filled(q.p(190.0, 64.0), 3.2, JOINT);
        dim(p, q.p(130.0, 24.0), q.p(130.0, 64.0));
        dim(p, q.p(190.0, 120.0), q.p(234.0, 120.0));
        label(p, q.p(186.0, 68.0), Align2::RIGHT_TOP, "corner", LABEL);
        angle_arc(p, q.p(168.0, 64.0), q.p(146.0, 24.0), 18.0 * q.s);
        return Marks { gap: q.p(150.0, 92.0), lead_out: Some(q.p(84.0, 44.0)), lead_in: Some(q.p(284.0, 84.0)), angle: Some(q.p(104.0, 12.0)) };
    }
    let y = if jt == JointType::Gap { 52.0 } else { 82.0 };
    p.rect_filled(Rect::from_min_max(q.p(0.0, y), q.p(336.0, y + 44.0)), CornerRadius::ZERO, mat);
    label(p, q.p(8.0, y + 38.0), Align2::LEFT_BOTTOM, "part", LABEL);
    line(p, q.p(14.0, y), q.p(148.0, y), w, CUT);
    line(p, q.p(188.0, y), q.p(322.0, y), w, CUT);
    arrow(64.0, y);
    arrow(270.0, y);
    p.circle_filled(q.p(168.0, y), 3.2, JOINT);
    dim(p, q.p(148.0, y + 12.0), q.p(188.0, y + 12.0));
    let gap = q.p(168.0, y + 28.0);
    if jt == JointType::Gap {
        dashed(p, &[q.p(148.0, y), q.p(188.0, y)], OFF);
        label(p, q.p(168.0, y - 10.0), Align2::CENTER_BOTTOM, "beam off, straight on", LABEL);
        return Marks { gap, ..Default::default() };
    }
    let top = 30.0;
    let (out_end, in_start) = if jt == JointType::Leads { (148.0, 188.0) } else { (118.0, 218.0) };
    line(p, q.p(148.0, y), q.p(out_end, top), w, LEAD_OUT);
    line(p, q.p(in_start, top), q.p(188.0, y), w, LEAD_IN);
    head(p, q.p(188.0, y), q.p(in_start, top), LEAD_IN, 8.0);
    dashed(p, &[q.p(out_end, top), q.p(in_start, top)], OFF);
    p.circle_stroke(q.p(in_start, top), 4.5, Stroke::new(1.5, PIERCE));
    label(p, q.p(in_start + 8.0, top - 4.0), Align2::LEFT_BOTTOM, "re-pierce", PIERCE);
    let (xo, xi) = (out_end - 18.0, in_start + 18.0);
    dim(p, q.p(xo, top), q.p(xo, y));
    dim(p, q.p(xi, top), q.p(xi, y));
    label(p, q.p(xo - 6.0, top + 2.0), Align2::RIGHT_TOP, "lead-out", LEAD_OUT);
    label(p, q.p(xi + 6.0, top + 2.0), Align2::LEFT_TOP, "lead-in", LEAD_IN);
    let mid = (top + y) / 2.0 + 6.0;
    let mut m = Marks { gap, lead_out: Some(q.p(xo - 36.0, mid)), lead_in: Some(q.p(xi + 36.0, mid)), angle: None };
    if jt == JointType::LeadsInclined {
        angle_arc(p, q.p(148.0, y), q.p(out_end, top), 22.0 * q.s);
        m.angle = Some(q.p(168.0, (top + y) / 2.0));
    }
    m
}

/// The colour key under the joint drawing.
pub fn legend(ui: &mut egui::Ui, leads: bool) {
    let mut items: Vec<(&str, Color32, bool)> = vec![("cut", CUT, false)];
    if leads {
        items.push(("lead-out", LEAD_OUT, false));
        items.push(("lead-in", LEAD_IN, false));
    }
    items.push(("beam off", OFF, false));
    items.push(("gap", JOINT, true));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (name, c, dot) in items {
            let (r, _) = ui.allocate_exact_size(Vec2::new(18.0, 12.0), egui::Sense::hover());
            let p = ui.painter();
            p.rect_filled(r, CornerRadius::same(3), th().canvas_b);
            if dot {
                p.circle_filled(r.center(), 3.0, c);
            } else {
                p.line_segment([r.left_center(), r.right_center()], Stroke::new(2.0, c));
            }
            ui.label(egui::RichText::new(name).size(11.0).color(th().text_dim));
            ui.add_space(6.0);
        }
    });
}

/// A part with a hole and its joints spaced along both, one every so far.
pub fn spacing(p: &Painter, r: Rect) {
    canvas(p, r);
    let q = Pic::new(r, 336.0);
    let part = Rect::from_min_max(q.p(22.0, 14.0), q.p(314.0, 74.0));
    p.rect_filled(part, CornerRadius::same(2), MATERIAL);
    p.rect_stroke(part, CornerRadius::same(2), Stroke::new(2.0, CUT), egui::StrokeKind::Middle);
    let hc = q.p(256.0, 44.0);
    p.circle_filled(hc, 14.0 * q.s, th().canvas_b);
    p.circle_stroke(hc, 14.0 * q.s, Stroke::new(2.0, CUT));
    for (x, y) in [(72.0, 14.0), (152.0, 14.0), (232.0, 14.0), (112.0, 74.0), (212.0, 74.0), (22.0, 44.0), (314.0, 44.0)] {
        p.circle_filled(q.p(x, y), 3.2, JOINT);
    }
    p.circle_filled(q.p(256.0, 30.0), 3.0, JOINT);
    p.circle_filled(q.p(256.0, 58.0), 3.0, JOINT);
    dim(p, q.p(72.0, 6.0), q.p(152.0, 6.0));
    label(p, q.p(112.0, 40.0), Align2::CENTER_CENTER, "one every …", Color32::from_rgb(207, 225, 255));
}

/// Small / medium / large as zones of contour width × height, drawn to the
/// limits set: a contour is small inside the small box, large when it
/// reaches the large size both ways, otherwise medium (rule P7).
pub fn size_map(p: &Painter, r: Rect, aj: &AutoJoints) {
    canvas(p, r);
    let a = Rect::from_min_max(r.min + Vec2::new(16.0, 6.0), r.max - Vec2::new(6.0, 16.0));
    let has_small = aj.small[0] > 0.0 && aj.small[1] > 0.0;
    let has_large = !is_no_limit(aj.large[0]) && !is_no_limit(aj.large[1]);
    let mut ext = 1.0f64;
    if has_large {
        ext = ext.max(aj.large[0].max(aj.large[1]) * 1.4);
    }
    if has_small {
        ext = ext.max(aj.small[0].max(aj.small[1]) * 2.5);
    }
    let at = |w: f64, h: f64| Pos2::new(a.left() + (w / ext).min(1.0) as f32 * a.width(), a.bottom() - (h / ext).min(1.0) as f32 * a.height());
    p.rect_filled(a, CornerRadius::ZERO, MEDIUM.gamma_multiply(0.18));
    let mut m_label = a.center();
    if has_large {
        let lr = Rect::from_min_max(Pos2::new(at(aj.large[0], 0.0).x, a.top()), Pos2::new(a.right(), at(0.0, aj.large[1]).y));
        p.rect_filled(lr, CornerRadius::ZERO, LARGE.gamma_multiply(0.32));
        label(p, lr.center(), Align2::CENTER_CENTER, "L", LARGE);
        m_label = Pos2::new(a.left() + a.width() * 0.3, a.top() + a.height() * 0.3);
    }
    if has_small {
        let sr = Rect::from_min_max(Pos2::new(a.left(), at(0.0, aj.small[1]).y), Pos2::new(at(aj.small[0], 0.0).x, a.bottom()));
        p.rect_filled(sr, CornerRadius::ZERO, SMALL.gamma_multiply(0.34));
        label(p, sr.center(), Align2::CENTER_CENTER, "S", SMALL);
    }
    label(p, m_label, Align2::CENTER_CENTER, if has_small || has_large { "M" } else { "all M" }, MEDIUM);
    p.line_segment([a.left_top(), a.left_bottom()], Stroke::new(1.0, LABEL));
    p.line_segment([a.left_bottom(), a.right_bottom()], Stroke::new(1.0, LABEL));
    p.text(Pos2::new(a.center().x, r.bottom() - 2.0), Align2::CENTER_BOTTOM, "width →", FontId::proportional(9.5), LABEL);
    p.text(Pos2::new(r.left() + 3.0, a.top()), Align2::LEFT_TOP, "↑", FontId::proportional(10.0), LABEL);
    p.text(Pos2::new(r.left() + 3.0, a.top() + 12.0), Align2::LEFT_TOP, "h", FontId::proportional(9.5), LABEL);
}

/// How a (fewest, most) pair reads in the count grid, using the general
/// limits for −1, and whether it follows them.
pub fn range_text(pair: [i32; 2], aj: &AutoJoints) -> (String, bool) {
    let follows = pair[0] < 0 && pair[1] < 0;
    let lo = if pair[0] < 0 { aj.min as i64 } else { pair[0] as i64 };
    let hi = if pair[1] < 0 { aj.max as i64 } else { pair[1] as i64 };
    let t = if hi == 0 {
        "none".to_string()
    } else if hi >= NO_LIMIT_COUNT as i64 {
        if lo == 0 { "any".into() } else { format!("{lo} or more") }
    } else if lo == hi {
        lo.to_string()
    } else {
        format!("{lo}–{hi}")
    };
    (t, follows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_cells_read_in_words() {
        let aj = AutoJoints::default();
        assert_eq!(range_text([-1, -1], &aj), ("any".into(), true));
        assert_eq!(range_text([0, 0], &aj).0, "none");
        assert_eq!(range_text([0, 2], &aj).0, "0–2");
        assert_eq!(range_text([2, 2], &aj).0, "2");
        assert_eq!(range_text([2, 32767], &aj).0, "2 or more");
        // −1 on one side takes the general limit for that side.
        let aj = AutoJoints { min: 1, max: 4, ..AutoJoints::default() };
        assert_eq!(range_text([-1, 3], &aj), ("1–3".into(), false));
    }
}
