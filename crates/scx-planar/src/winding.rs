// Written for solidcorex. Apache-2.0.
//! Winding numbers and signed areas of closed loops of lines and arcs.

use core::f64::consts::{PI, TAU};

use scx_num::certified::Verdict;
use scx_num::eft::two_sum;
use scx_num::fmath;
use scx_num::frame::Sense;
use scx_num::vector2d::Point2;

use crate::cert::{IP, Iv, greater};
use crate::segment::Segment;

/// The number of times the closed loop winds about `p` counter-clockwise, for a point more
/// than the resolution from every segment; `None` when it is not decided.
///
/// The angle swept as seen from `p`, summed: a line sweeps the signed angle between its
/// ends, `atan2(cross, dot)`; an arc sweeps the same for its chord, plus a full turn (in
/// its sense) when `p` is in the circular segment between chord and arc. The chord's side
/// is read from the same float `cross` that `atan2` reads, so a point on the chord line
/// gets `pi` or `-pi` with the matching correction and the sum is the same either way.
/// Each term is within a few ulps of its angle for any `p` off the segment, so the sum is
/// rounded to the nearest whole turn when within a quarter turn of it.
pub(crate) fn winding(segments: &[Segment], p: Point2) -> Option<i64> {
    let mut sum = 0.0;
    let mut comp = 0.0;
    for seg in segments {
        if let Segment::Circle {
            centre,
            radius,
            sense,
        } = *seg
        {
            // A whole turn in its sense about a point inside it, none about one outside.
            let r2 = Iv::exact(radius).sq();
            match greater(r2, IP::exact(p).sub(IP::exact(centre)).norm2()) {
                Verdict::Yes => {
                    let (s, e) = two_sum(sum, sense.sign() * TAU);
                    sum = s;
                    comp += e;
                }
                Verdict::No => {}
                Verdict::Undecided => return None,
            }
            continue;
        }
        let a = seg.start() - p;
        let b = seg.end() - p;
        let cross = a.x * b.y - a.y * b.x;
        let dot = a.x * b.x + a.y * b.y;
        let mut term = fmath::atan2(cross, dot);
        if let Segment::Arc {
            centre,
            start,
            end,
            sense,
        } = *seg
        {
            // A counter-clockwise arc bulges right of its chord, a clockwise one left.
            let arc_side = match sense {
                Sense::Forward => cross.is_sign_negative(),
                Sense::Reversed => !cross.is_sign_negative(),
            };
            if arc_side {
                let ic = IP::exact(centre);
                let r2 = IP::exact(start)
                    .sub(ic)
                    .norm2()
                    .hull(IP::exact(end).sub(ic).norm2());
                match greater(r2, IP::exact(p).sub(ic).norm2()) {
                    Verdict::Yes => term += sense.sign() * TAU,
                    Verdict::No => {}
                    Verdict::Undecided => return None,
                }
            }
        }
        let (s, e) = two_sum(sum, term);
        sum = s;
        comp += e;
    }
    let turns = (sum + comp) / TAU;
    let k = turns.round();
    ((turns - k).abs() < 0.25).then_some(k as i64)
}

/// The signed area the loop encloses, positive counter-clockwise: the shoelace sum of the
/// chords about the loop's first vertex plus, per arc, the signed circular segment
/// `r^2 (t - sin t) / 2` of its swept angle `t`, and for a whole circle `pi r^2` in its
/// sense; with a bound on the rounding of the sum.
pub(crate) fn signed_area(segments: &[Segment]) -> (f64, f64) {
    let Some(o) = segments.first().map(Segment::start) else {
        return (0.0, 0.0);
    };
    let mut sum = 0.0;
    let mut comp = 0.0;
    let mut mag = 0.0;
    for seg in segments {
        if let Segment::Circle { radius, sense, .. } = *seg {
            // Alone in its loop (checked first), so no chord term: the disc in its sense.
            let disc = PI * radius * radius;
            let (s, e) = two_sum(sum, sense.sign() * disc);
            sum = s;
            comp += e;
            mag += disc;
            continue;
        }
        let a = seg.start() - o;
        let b = seg.end() - o;
        let mut term = 0.5 * (a.x * b.y - a.y * b.x);
        mag += 0.5 * (a.x * b.y).abs() + 0.5 * (a.y * b.x).abs();
        if let Segment::Arc {
            centre,
            start,
            end,
            sense,
        } = *seg
        {
            let u = start - centre;
            let w = end - centre;
            let t0 = fmath::atan2(u.x * w.y - u.y * w.x, u.x * w.x + u.y * w.y);
            let t = match sense {
                Sense::Forward if t0 <= 0.0 => t0 + TAU,
                Sense::Reversed if t0 >= 0.0 => t0 - TAU,
                _ => t0,
            };
            let r2 = 0.5 * (u.length_squared() + w.length_squared());
            let seg_area = 0.5 * r2 * t_minus_sin(t);
            term += seg_area;
            // The swept angle is within a few ulps absolute (atan2 of rounded vectors), so
            // the segment moves by `r^2 (1 - cos t) / 2` per unit of its error; `t - sin t`
            // itself rounds by ulps of `t` off the series.
            let slope = 4.0 * r2 * (1.0 - fmath::cos(t));
            let direct = if t.abs() < 1e-2 { 0.0 } else { r2 * t.abs() };
            mag += seg_area.abs() + slope + direct;
        }
        let (s, e) = two_sum(sum, term);
        sum = s;
        comp += e;
    }
    // Each term carries a few ulps of its parts; 16 per segment is generous.
    let bound = 16.0 * f64::EPSILON * mag;
    (sum + comp, bound)
}

/// `t - sin t`, by its series where the difference cancels.
fn t_minus_sin(t: f64) -> f64 {
    if t.abs() < 1e-2 {
        let t2 = t * t;
        t * t2 / 6.0 * (1.0 - t2 / 20.0 * (1.0 - t2 / 42.0 * (1.0 - t2 / 72.0)))
    } else {
        t - fmath::sin(t)
    }
}

/// `true` when the loop's area, with its bound, is proved positive or negative.
pub(crate) fn orientation(segments: &[Segment]) -> Option<bool> {
    let (a, bound) = signed_area(segments);
    let iv = Iv(scx_num::interval::Ivl::new(a - bound, a + bound));
    match (iv.pos(), iv.neg_sign()) {
        (Verdict::Yes, _) => Some(true),
        (_, Verdict::Yes) => Some(false),
        _ => None,
    }
}
