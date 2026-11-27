// Written for solidcorex. Apache-2.0.
//! The cheap geometry check (research 60 V1, 63 D10): at a fixed set of samples per edge and
//! per use, the geometry fits the structure within the recorded accuracies.
//!
//! - Edges, in id order: each curve's ends lie within its vertices' balls (the pin's Edge
//!   16, 17), and the curve is longer than its own accuracy (Edge 7).
//! - Faces, in id order, each loop and use in order:
//!   - each use's lifted ends lie within its vertices' balls;
//!   - each sample lies in the surface's domain, and lifts onto the edge's curve (onto the
//!     vertex, for a collapsed edge);
//!   - no pcurve jumps across a period between samples (Edgeuse 7);
//!   - at a pole, a pcurve only arrives or leaves: no run along the pole's line (Edgeuse
//!     20, the dog-leg) and no pass through it inside the use (Face 24);
//!   - each loop is closed in uv, so a seam is an edge and a pole's line is a collapsed
//!     edge's pcurve, and turns the way its place in the face says;
//!   - the loops span at most a period in each closed direction.
//!
//! Distances add (truth, and the pin's `tau_e + tau_v`): a curve end is within its edge's
//! reach of the true end and the vertex point within its own reach of the same place, so
//! the two are within the sum. A reach is the recorded accuracy, or the resolution's zone
//! where none is recorded (`None` means within the resolution, 01 §4.1).

use std::sync::Arc;

use scx_num::cert::Recorded;
use scx_num::domain::ParamDomain;
use scx_num::extent1d::Extent1d;
use scx_num::frame::Sense;
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::Point3;
use scx_nurbs::Approach;
use scx_surface::{Pole, Surface};

use crate::finish::GeometryCause;
use crate::id::{EdgeId, FaceId};
use crate::record::{EdgeGeom, EdgeShape, EdgeUse, FaceRecord, UseGeom};
use crate::tables::Tables;

/// Segments per edge and per use: `SEGMENTS + 1` samples, the ends exact.
const SEGMENTS: usize = 8;

/// Consecutive samples this fraction of a period apart in a closed direction are a jump
/// (the pin's Edgeuse 7 threshold, on its control points).
const JUMP: f64 = 0.9;

/// Probes toward an end at a pole, at `2^-1` to `2^-PROBES` of the interval from it: a
/// dog-leg shorter than an eighth of the use falls between the regular samples.
const PROBES: usize = 10;

/// The slack, relative to the period, on the span of a face's loops in a closed direction:
/// the rounding of a pcurve's ends at a seam, not a distance.
const WRAP_SLACK: f64 = 1e-9;

/// How far an entity may stand from what it stands for.
fn reach(recorded: Option<&Arc<Recorded>>, res: Resolution) -> f64 {
    let zone = res.zone().mm();
    recorded.map_or(zone, |r| r.acc.length().mm().max(zone))
}

/// `SEGMENTS + 1` parameters evenly across `over`, its ends exact.
fn params(over: Extent1d) -> [f64; SEGMENTS + 1] {
    let (lo, hi) = (over.min(), over.max());
    let mut ts = [0.0; SEGMENTS + 1];
    for (k, t) in ts.iter_mut().enumerate() {
        *t = match k {
            0 => lo,
            SEGMENTS => hi,
            _ => lo + (hi - lo) * (k as f64 / SEGMENTS as f64),
        };
    }
    ts
}

fn dist(a: Point3, b: Point3) -> f64 {
    (a - b).length()
}

/// The surface's speeds `(|S_u|, |S_v|)` at `uv`.
fn speeds(surface: &Surface, uv: Point2) -> (f64, f64) {
    let jet = surface.jet::<1>(uv, [Approach::Above; 2]);
    let len = |d: Option<scx_num::vector3d::Vec3>| d.map_or(0.0, scx_num::vector3d::Vec3::length);
    (len(jet.get(1, 0)), len(jet.get(0, 1)))
}

/// The surface's poles: a sphere's two, a cone's apex. A NURBS surface's collapsed
/// boundary rows are not found here (an open question in 63 §6.7).
fn poles(surface: &Surface) -> &'static [usize] {
    match surface {
        Surface::Sphere(_) => &[0, 1],
        Surface::Cone(_) => &[0],
        _ => &[],
    }
}

fn pole(surface: &Surface, i: usize) -> Option<Pole> {
    match surface {
        Surface::Sphere(s) => s.poles().get(i).copied(),
        Surface::Cone(c) => (i == 0).then(|| c.pole()),
        _ => None,
    }
}

/// Whether `x` lies outside `dom` by more than `slack` parameter units.
fn outside(dom: &ParamDomain, x: f64) -> f64 {
    match *dom {
        ParamDomain::Bounded(e) => (e.min() - x).max(x - e.max()).max(0.0),
        ParamDomain::AtLeast { from, .. } => (from - x).max(0.0),
        ParamDomain::AtMost { to, .. } => (x - to).max(0.0),
        // A NaN coordinate is outside every domain.
        _ if x.is_nan() => f64::INFINITY,
        _ => 0.0,
    }
}

/// Every check, edges then faces.
pub(super) fn check(t: &Tables, res: Resolution) -> Result<(), GeometryCause> {
    for (id, edge) in t.edges.iter() {
        edge_check(t, res, id, edge)?;
    }
    for (id, face) in t.faces.iter() {
        face_check(t, res, id, face)?;
    }
    Ok(())
}

fn edge_check(
    t: &Tables,
    res: Resolution,
    id: EdgeId,
    edge: &EdgeGeom,
) -> Result<(), GeometryCause> {
    let EdgeShape::Curve { curve, interval } = &edge.shape else {
        return Ok(());
    };
    let re = reach(edge.accuracy.as_ref(), res);
    for (end, t_end) in [(0, interval.min()), (1, interval.max())] {
        let Some(&vertex) = edge.ends.get(end) else {
            continue;
        };
        let Some(v) = t.vertices.get(vertex) else {
            continue;
        };
        let gap = dist(curve.point(t_end), v.point);
        let allowed = re + reach(v.accuracy.as_ref(), res);
        if gap.is_nan() || gap > allowed {
            return Err(GeometryCause::EdgeEndOffVertex {
                edge: id,
                vertex,
                gap: Length::from_mm(gap),
                allowed: Length::from_mm(allowed),
            });
        }
    }
    let ps = params(*interval).map(|s| curve.point(s));
    let length: f64 = ps
        .windows(2)
        .map(|w| match w {
            [a, b] => dist(*a, *b),
            _ => 0.0,
        })
        .sum();
    if length.is_nan() || length <= re {
        return Err(GeometryCause::EdgeTooShort {
            edge: id,
            length: Length::from_mm(length),
            allowed: Length::from_mm(re),
        });
    }
    Ok(())
}

/// A use's uv ends, in its own direction, and its end's 3-D budget.
struct Joint {
    start: Point2,
    end: Point2,
    budget: f64,
}

fn face_check(
    t: &Tables,
    res: Resolution,
    id: FaceId,
    face: &FaceRecord,
) -> Result<(), GeometryCause> {
    let surface = &face.surface;
    let dom = surface.domain();
    // The hull of every sample, per closed direction.
    let mut hull: [Option<(f64, f64)>; 2] = [None, None];
    let mut grow = |uv: Point2| {
        for (h, x) in hull.iter_mut().zip([uv.x, uv.y]) {
            *h = Some(h.map_or((x, x), |(lo, hi)| (lo.min(x), hi.max(x))));
        }
    };
    for (lp, l) in face.loops.iter().enumerate() {
        let mut joints: Vec<Joint> = Vec::with_capacity(l.uses.len());
        // The loop's samples in its own direction, each use's last left to the next.
        let mut ring: Vec<Point2> = Vec::with_capacity(l.uses.len() * SEGMENTS);
        let mut has_area = false;
        for u in &l.uses {
            let Some(edge) = t.edges.get(u.edge) else {
                continue;
            };
            let joint = use_check(t, res, id, face, u, edge, &mut ring)?;
            has_area |= matches!(edge.shape, EdgeShape::Curve { .. });
            grow(joint.start);
            joints.push(joint);
        }
        for p in &ring {
            grow(*p);
        }
        closure(surface, id, lp, &joints)?;
        if has_area {
            let area = shoelace(&ring);
            // The first loop may run clockwise only on a surface with a direction without
            // both ends: the face then has no outer loop and is unbounded.
            let open = [dom.u, dom.v].iter().any(|d| {
                matches!(
                    d,
                    ParamDomain::AtLeast { .. }
                        | ParamDomain::AtMost { .. }
                        | ParamDomain::Unbounded
                )
            });
            let ok = if lp == 0 {
                area > 0.0 || (open && area < 0.0)
            } else {
                area < 0.0
            };
            if !ok {
                return Err(GeometryCause::LoopOrientation { face: id, lp });
            }
        }
    }
    for (direction, (d, h)) in [dom.u, dom.v].iter().zip(hull).enumerate() {
        if let (Some(period), Some((lo, hi))) = (d.period(), h)
            && hi - lo > period * (1.0 + WRAP_SLACK)
        {
            return Err(GeometryCause::FaceWraps {
                face: id,
                direction,
            });
        }
    }
    Ok(())
}

/// Twice the signed area of the closed polygon `ring`.
fn shoelace(ring: &[Point2]) -> f64 {
    let n = ring.len();
    (0..n)
        .filter_map(|i| Some((ring.get(i)?, ring.get((i + 1) % n)?)))
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum()
}

/// Each use ends where the next starts in uv, within the two ends' budgets taken to uv by
/// the surface's larger speed there: the tighter of the two, so a pole's line is crossed
/// only by a pcurve along it.
fn closure(
    surface: &Surface,
    id: FaceId,
    lp: usize,
    joints: &[Joint],
) -> Result<(), GeometryCause> {
    let n = joints.len();
    for (i, a) in joints.iter().enumerate() {
        let Some(b) = joints.get((i + 1) % n) else {
            continue;
        };
        let gap = (a.end.x - b.start.x).abs().max((a.end.y - b.start.y).abs());
        if gap == 0.0 {
            continue;
        }
        let (su, sv) = speeds(surface, a.end);
        let speed = su.max(sv);
        let allowed = if speed > 0.0 {
            (a.budget + b.budget) / speed
        } else {
            0.0
        };
        if gap.is_nan() || gap > allowed {
            return Err(GeometryCause::LoopOpenInUv {
                face: id,
                lp,
                at: i,
                gap,
                allowed,
            });
        }
    }
    Ok(())
}

/// One use's checks; pushes its samples onto `ring` and gives its joint.
fn use_check(
    t: &Tables,
    res: Resolution,
    id: FaceId,
    face: &FaceRecord,
    u: &EdgeUse,
    edge: &EdgeGeom,
    ring: &mut Vec<Point2>,
) -> Result<Joint, GeometryCause> {
    let surface = &face.surface;
    let dom = surface.domain();
    let rb = reach(u.bound.as_ref(), res);
    let re = reach(edge.accuracy.as_ref(), res);
    let (first, last) = (edge.start(u.sense), edge.end(u.sense));
    let vertex = |v| {
        t.vertices
            .get(v)
            .map(|r| (r.point, reach(r.accuracy.as_ref(), res)))
    };
    let cause = |at: f64| (id, u.edge, u.sense, at);
    let out = |uv: Point2| outside(&dom.u, uv.x).max(outside(&dom.v, uv.y));
    let off_domain = |uv: Point2, at: f64| -> Result<(), GeometryCause> {
        let over = out(uv);
        if over > 0.0 {
            let (su, sv) = speeds(surface, uv);
            if over.is_nan() || over * su.max(sv) > rb || !over.is_finite() {
                let (face, edge, sense, at) = cause(at);
                return Err(GeometryCause::OutsideDomain {
                    face,
                    edge,
                    sense,
                    at,
                });
            }
        }
        Ok(())
    };
    let end_off = |uv: Point2, v, at: f64| -> Result<f64, GeometryCause> {
        let Some((p, rv)) = vertex(v) else {
            return Ok(rb + re);
        };
        let gap = dist(surface.point(uv), p);
        let allowed = rb + re + rv;
        if gap.is_nan() || gap > allowed {
            let (face, edge, sense, _) = cause(at);
            return Err(GeometryCause::UseEndOffVertex {
                face,
                edge,
                sense,
                vertex: v,
                gap: Length::from_mm(gap),
                allowed: Length::from_mm(allowed),
            });
        }
        Ok(allowed)
    };
    let (curve, interval) = match &u.geom {
        UseGeom::Point(p) => {
            off_domain(*p, 0.0)?;
            end_off(*p, first, 0.0)?;
            let budget = end_off(*p, last, 0.0)?;
            ring.push(*p);
            return Ok(Joint {
                start: *p,
                end: *p,
                budget,
            });
        }
        UseGeom::Pcurve { curve, interval } => (curve, *interval),
    };
    let ts = params(interval);
    let uvs = ts.map(|s| curve.point(s));
    let (t_first, uv_first, t_last, uv_last) = match u.sense {
        Sense::Forward => (ts[0], uvs[0], ts[SEGMENTS], uvs[SEGMENTS]),
        Sense::Reversed => (ts[SEGMENTS], uvs[SEGMENTS], ts[0], uvs[0]),
    };
    end_off(uv_first, first, t_first)?;
    let budget = end_off(uv_last, last, t_last)?;
    for (&s, &uv) in ts.iter().zip(&uvs) {
        off_domain(uv, s)?;
        let lift = surface.point(uv);
        let (target, allowed) = match &edge.shape {
            EdgeShape::Curve { curve: c, .. } => (Some(c.point(s)), rb),
            EdgeShape::Collapsed => match vertex(first) {
                Some((p, rv)) => (Some(p), rb + re + rv),
                None => (None, 0.0),
            },
        };
        if let Some(target) = target {
            let gap = dist(lift, target);
            if gap.is_nan() || gap > allowed {
                let (face, edge, sense, at) = cause(s);
                return Err(GeometryCause::PcurveOffEdge {
                    face,
                    edge,
                    sense,
                    at,
                    gap: Length::from_mm(gap),
                    allowed: Length::from_mm(allowed),
                });
            }
        }
    }
    for (k, w) in uvs.windows(2).enumerate() {
        let [a, b] = w else { continue };
        for (d, delta) in [dom.u, dom.v].iter().zip([b.x - a.x, b.y - a.y]) {
            if let Some(period) = d.period()
                && delta.abs() >= JUMP * period
            {
                let (face, edge, sense, at) = cause(ts.get(k + 1).copied().unwrap_or(0.0));
                return Err(GeometryCause::PeriodJump {
                    face,
                    edge,
                    sense,
                    at,
                });
            }
        }
    }
    if matches!(edge.shape, EdgeShape::Curve { .. }) {
        for &i in poles(surface) {
            if let Some(p) = pole(surface, i) {
                pole_check(surface, p, rb + re, curve, &ts, &uvs, cause)?;
            }
        }
    }
    match u.sense {
        Sense::Forward => ring.extend(uvs.iter().take(SEGMENTS)),
        Sense::Reversed => ring.extend(uvs.iter().rev().take(SEGMENTS)),
    }
    Ok(Joint {
        start: uv_first,
        end: uv_last,
        budget,
    })
}

/// The pole rules for one pcurve of an edge with a curve. A sample is at the pole when it
/// lifts within `tol` of it. A use may arrive at or leave the pole at an end; then every
/// sample at the pole near that end (the regular samples nearer it than the other end, and
/// probes toward it) has the end's `u`, within `tol` taken to uv by the surface's `v` speed
/// there; one that does not runs along the pole's line (a dog-leg). A use with no end at
/// the pole and a regular sample at it passes the pole inside itself.
fn pole_check(
    surface: &Surface,
    p: Pole,
    tol: f64,
    curve: &scx_curve::Curve2,
    ts: &[f64; SEGMENTS + 1],
    uvs: &[Point2; SEGMENTS + 1],
    cause: impl Fn(f64) -> (FaceId, EdgeId, Sense, f64),
) -> Result<(), GeometryCause> {
    let at_pole = |uv: Point2| dist(surface.point(uv), p.point) <= tol;
    let ends = [(0, ts[0], uvs[0]), (SEGMENTS, ts[SEGMENTS], uvs[SEGMENTS])];
    let on: [bool; 2] = ends.map(|(_, _, uv)| at_pole(uv));
    if on == [false, false] {
        for (&s, &uv) in ts.iter().zip(uvs).take(SEGMENTS).skip(1) {
            if at_pole(uv) {
                let (face, edge, sense, at) = cause(s);
                return Err(GeometryCause::PoleNotVertex {
                    face,
                    edge,
                    sense,
                    at,
                });
            }
        }
        return Ok(());
    }
    let span = ts[SEGMENTS] - ts[0];
    for (side, &(k_end, t_end, uv_end)) in ends.iter().enumerate() {
        if !on.get(side).copied().unwrap_or(false) {
            continue;
        }
        let (_, sv) = speeds(surface, Point2::new(uv_end.x, p.v));
        let tol_u = if sv > 0.0 { tol / sv } else { 0.0 };
        let toward = if side == 0 { 1.0 } else { -1.0 };
        // The regular samples on this end's half, then the probes toward it.
        let half = ts
            .iter()
            .zip(uvs)
            .enumerate()
            .filter(|&(k, _)| k.abs_diff(k_end) * 2 <= SEGMENTS)
            .map(|(_, (&s, &uv))| (s, uv));
        // Halving is exact, so the probes are the same on every platform.
        let probes = (0..PROBES).scan(span, |h, _| {
            *h *= 0.5;
            let s = t_end + toward * *h;
            Some((s, curve.point(s)))
        });
        for (s, uv) in half.chain(probes) {
            let du = (uv.x - uv_end.x).abs();
            if at_pole(uv) && (du.is_nan() || du > tol_u) {
                let (face, edge, sense, at) = cause(s);
                return Err(GeometryCause::DogLeg {
                    face,
                    edge,
                    sense,
                    at,
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use scx_curve::{Curve2, Line2};
    use scx_num::frame::Frame;
    use scx_num::vector2d::Vec2;
    use scx_surface::Sphere;
    use scx_testkit::topology::{dog_leg, passes_pole};

    fn sphere() -> Surface {
        Surface::Sphere(Sphere::new(Frame::WORLD, Length::from_mm(1.0), Sense::Forward).unwrap())
    }

    fn uv_line(o: (f64, f64), d: (f64, f64)) -> Curve2 {
        Curve2::Line(Line2::new(Point2::new(o.0, o.1), Vec2::new(d.0, d.1)).unwrap())
    }

    /// The pole rules for `curve` over `[0, 1]` on the unit sphere, both poles.
    fn poles_of(curve: &Curve2) -> Result<(), GeometryCause> {
        let s = sphere();
        let ts = params(Extent1d::new(0.0, 1.0).unwrap());
        let uvs = ts.map(|t| curve.point(t));
        let cause = |at| (FaceId::from_raw(0), EdgeId::from_raw(0), Sense::Forward, at);
        for &i in poles(&s) {
            if let Some(p) = pole(&s, i) {
                pole_check(&s, p, 2e-5, curve, &ts, &uvs, cause)?;
            }
        }
        Ok(())
    }

    /// The oracle's verdicts on 1001 samples of `curve` at the north pole: (dog-leg, pass).
    fn oracle(curve: &Curve2) -> (bool, bool) {
        let uv: Vec<[f64; 2]> = (0..=1000)
            .map(|k| {
                let p = curve.point(f64::from(k) / 1000.0);
                [p.x, p.y]
            })
            .collect();
        (
            dog_leg(&uv, 1.0, 1e-12, 1e-12).is_some(),
            passes_pole(&uv, 1.0, 1e-12).is_some(),
        )
    }

    #[test]
    fn a_pcurve_arriving_at_a_pole_passes() {
        let up = uv_line((0.25, 0.5), (0.0, 0.5));
        assert_eq!(poles_of(&up), Ok(()));
        assert_eq!(oracle(&up), (false, false));
    }

    #[test]
    fn a_pcurve_along_a_poles_line_is_a_dog_leg() {
        // Every sample lifts to the north pole; the use's u moves from 0 to 1/2.
        let leg = uv_line((0.0, 1.0), (0.5, 0.0));
        assert!(matches!(
            poles_of(&leg),
            Err(GeometryCause::DogLeg { at, .. }) if at == 0.125
        ));
        assert_eq!(oracle(&leg), (true, false));
    }

    #[test]
    fn a_pcurve_through_a_pole_inside_the_use_is_refused() {
        // Up through v = 1 at t = 1/2, neither end at the pole.
        let through = uv_line((0.25, 0.5), (0.0, 1.0));
        assert!(matches!(
            poles_of(&through),
            Err(GeometryCause::PoleNotVertex { at, .. }) if at == 0.5
        ));
        assert_eq!(oracle(&through), (false, true));
    }
}
