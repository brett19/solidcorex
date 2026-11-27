// Written for solidcorex. Apache-2.0.
//! The radial order of an edge's fins (research 63 §6.15): an edge with more than two fins
//! lists them counter-clockwise about its direction (the right-hand rule), and the faces
//! must leave the edge in that order.
//!
//! At a sample of the edge, each fin's face leaves the edge in the direction `d = σ N × T`:
//! `T` the edge's unit tangent, `N` the face's surface normal at the use's lift of the
//! sample, `σ` the use's sense. Taken in the fins' order, the counter-clockwise gaps from
//! each direction to the next add up to a whole number of turns: one turn when the order
//! is right, more when it is not. The check sums lower bounds on the gaps and refuses
//! only when they prove a second turn, so a refusal is a proof; a gap it cannot bound
//! counts as nothing.
//!
//! Each direction is known within its *spread*: the normal is taken at the lift, which
//! stands within the use's reach and the edge's of the true edge, so it is off by at most
//! the surface's largest curvature times that distance, plus rounding. Two directions
//! within their spreads of each other are faces tangent along the edge: they are ordered
//! by how each bends away from the edge (its normal curvature along `d`, with the use's
//! sense), which is the order every reading within the resolution shows past the edge's
//! tube. Where the bends tie too the pair is undecided, and the caller's order stands.
//!
//! The edge's midpoint is sampled first; a quarter and three quarters only when a pair
//! there is undecided. The rule runs after the faces' check, which has put each use's
//! pcurve on the edge.

use std::f64::consts::TAU;

use scx_curve::Curve;
use scx_num::certified::gamma;
use scx_num::fmath;
use scx_num::frame::Sense;
use scx_num::units::Resolution;
use scx_num::vector2d::Point2;
use scx_num::vector3d::Vec3;
use scx_nurbs::Approach;
use scx_surface::Surface;

use super::geometry::reach;
use super::{Sel, each};
use crate::finish::GeometryCause;
use crate::id::EdgeId;
use crate::record::{EdgeShape, Fin, UseGeom};
use crate::tables::Tables;

/// The fractions of the edge's interval sampled, in turn, while a sample is undecided.
const SAMPLES: [f64; 3] = [0.5, 0.25, 0.75];

/// The rounding of a leaving direction, in radians: the normal's and the tangent's
/// rounding, the cross product and its normalizing, and the angle between two (a few ulps
/// each; counted generously, as a NURBS normal's rounding is not bounded here).
fn rounding() -> f64 {
    gamma(64)
}

/// The relative rounding of a curvature computed from a surface's second partials (not
/// certified for NURBS surfaces; generous for the analytic kinds).
const CURVATURE_SLACK: f64 = 1e-9;

/// How a face leaves an edge at a sample.
#[derive(Debug, Clone, Copy)]
struct Leaving {
    /// The unit direction, normal to the edge's tangent.
    d: Vec3,
    /// How far, in radians, `d` may be from the true direction.
    spread: f64,
    /// The face's bend toward the counter-clockwise side, past `d`: its normal curvature
    /// along `d` with the use's sense. Larger is further counter-clockwise.
    bend: f64,
    /// How far `bend` may be from the true bend.
    bend_spread: f64,
}

/// What one sample shows of the fins' order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Order {
    /// One turn: the order is right.
    Right,
    /// Proved more than one turn.
    Wrong,
    /// Neither proved.
    Undecided,
}

/// The rule on the selected edges with more than two fins, in id order.
pub(super) fn check(
    t: &Tables,
    res: Resolution,
    edges: Sel<'_, EdgeId>,
) -> Result<(), GeometryCause> {
    each(&t.adjacency, edges, |id, adj| {
        if adj.fins.len() <= 2 {
            return Ok(());
        }
        edge_order(t, res, id, &adj.fins)
    })
}

fn edge_order(t: &Tables, res: Resolution, id: EdgeId, fins: &[Fin]) -> Result<(), GeometryCause> {
    let Some(edge) = t.edges.get(id) else {
        return Ok(());
    };
    // A collapsed edge has no direction to turn about.
    let EdgeShape::Curve { curve, interval } = &edge.shape else {
        return Ok(());
    };
    let re = reach(edge.accuracy.as_ref(), res);
    let (lo, hi) = (interval.min(), interval.max());
    for f in SAMPLES {
        let s = lo + (hi - lo) * f;
        match sample(t, res, id, curve, s, re, fins) {
            Order::Right => return Ok(()),
            Order::Wrong => return Err(GeometryCause::RadialOrder { edge: id, at: s }),
            Order::Undecided => {}
        }
    }
    Ok(())
}

/// The fins' order at parameter `s` of the edge.
fn sample(
    t: &Tables,
    res: Resolution,
    id: EdgeId,
    curve: &Curve,
    s: f64,
    re: f64,
    fins: &[Fin],
) -> Order {
    let Some(tan) = curve.tangent_dir(s, Approach::Above) else {
        return Order::Undecided;
    };
    let tan = tan.as_vec();
    let leaving: Vec<Option<Leaving>> = fins
        .iter()
        .map(|fin| {
            let face = t.faces.get(fin.face)?;
            let u = face
                .loops
                .iter()
                .flat_map(|l| &l.uses)
                .find(|u| u.edge == id && u.sense == fin.sense)?;
            let UseGeom::Pcurve { curve: pc, .. } = &u.geom else {
                return None;
            };
            let rho = reach(u.bound.as_ref(), res) + re;
            leaves(&face.surface, pc.point(s), fin.sense, tan, rho)
        })
        .collect();
    order(tan, &leaving)
}

/// How a face on `surface` leaves the edge at `uv`, its use in `sense`, the edge's unit
/// tangent `tan`; `rho` bounds the distance from the lift to the true edge.
fn leaves(surface: &Surface, uv: Point2, sense: Sense, tan: Vec3, rho: f64) -> Option<Leaving> {
    let side = [Approach::Above; 2];
    let n = surface.normal(uv, side)?.as_vec();
    let d = (n.cross(tan) * sense.sign()).normalized()?.as_vec();
    let jet = surface.jet::<2>(uv, side);
    let (su, sv) = (jet.get(1, 0)?, jet.get(0, 1)?);
    let (suu, suv, svv) = (jet.get(2, 0)?, jet.get(1, 1)?, jet.get(0, 2)?);
    // The first and second fundamental forms, the second with the sensed normal.
    let (e, f, g) = (su.dot(su), su.dot(sv), sv.dot(sv));
    let (l, m, nn) = (suu.dot(n), suv.dot(n), svv.dot(n));
    let det = e * g - f * f;
    if !det.is_finite() || det <= 0.0 {
        return None;
    }
    // The largest principal curvature, from the mean and Gaussian curvatures.
    let mean = (l * g - 2.0 * m * f + nn * e) / (2.0 * det);
    let gauss = (l * nn - m * m) / det;
    let kmax = mean.abs() + (mean * mean - gauss).max(0.0).sqrt();
    // `d` in uv: it is in the tangent plane, so `J w = d` has the least-squares solution.
    let (a, b) = (su.dot(d), sv.dot(d));
    let (w1, w2) = ((g * a - f * b) / det, (e * b - f * a) / det);
    let first = e * w1 * w1 + 2.0 * f * w1 * w2 + g * w2 * w2;
    let second = l * w1 * w1 + 2.0 * m * w1 * w2 + nn * w2 * w2;
    if !first.is_finite() || first <= 0.0 {
        return None;
    }
    let bend = sense.sign() * second / first;
    let spread = kmax * rho + rounding();
    if !(spread.is_finite() && bend.is_finite()) {
        return None;
    }
    Some(Leaving {
        d,
        spread,
        // Moving the sample by `rho` and turning `d` by its spread moves the bend by
        // about twice `kmax^2 rho`; doubled again for the change of curvature along the
        // surface, which the jet does not bound.
        bend_spread: 4.0 * kmax * kmax * rho + CURVATURE_SLACK * kmax.max(bend.abs()),
        bend,
    })
}

/// The order the leaving directions show, in the fins' order, about `tan`.
fn order(tan: Vec3, leaving: &[Option<Leaving>]) -> Order {
    let n = leaving.len();
    // Bounds on the sum of the counter-clockwise gaps.
    let (mut lower, mut upper) = (0.0, 0.0);
    let mut undecided = false;
    for (i, a) in leaving.iter().enumerate() {
        let (Some(a), Some(b)) = (a, leaving.get((i + 1) % n).copied().flatten()) else {
            undecided = true;
            upper += TAU;
            continue;
        };
        let spread = a.spread + b.spread;
        let theta = fmath::atan2(a.d.cross(b.d).dot(tan), a.d.dot(b.d));
        if theta.abs() > spread {
            let gap = if theta > 0.0 { theta } else { theta + TAU };
            lower += (gap - spread).max(0.0);
            upper += gap + spread;
            continue;
        }
        // Tangent along the edge: the bends order them.
        let bends = b.bend - a.bend;
        if bends > a.bend_spread + b.bend_spread {
            // `b` just past `a`.
            upper += spread;
        } else if -bends > a.bend_spread + b.bend_spread {
            // `b` just before `a`: all but a whole turn.
            lower += TAU - spread;
            upper += TAU;
        } else {
            undecided = true;
            upper += TAU;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let slack = TAU * gamma(2 * n + 4);
    if lower > TAU + slack {
        Order::Wrong
    } else if !undecided && upper < 2.0 * TAU - slack {
        Order::Right
    } else {
        Order::Undecided
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use scx_num::frame::Frame;
    use scx_num::units::Length;
    use scx_num::vector3d::Point3;
    use scx_surface::{Cylinder, Plane};

    #[allow(clippy::unnecessary_wraps)] // Rings mix known and unknown directions.
    fn flat(d: Vec3, bend: f64) -> Option<Leaving> {
        Some(Leaving {
            d,
            spread: rounding(),
            bend,
            bend_spread: CURVATURE_SLACK,
        })
    }

    const X: Vec3 = Vec3::new(1.0, 0.0, 0.0);
    const Y: Vec3 = Vec3::new(0.0, 1.0, 0.0);
    const Z: Vec3 = Vec3::new(0.0, 0.0, 1.0);

    #[test]
    fn directions_in_turning_order_are_right() {
        // About +x, counter-clockwise: +y, +z, -y, -z.
        let ring = [flat(Y, 0.0), flat(Z, 0.0), flat(-Y, 0.0), flat(-Z, 0.0)];
        assert_eq!(order(X, &ring), Order::Right);
        let swapped = [flat(Y, 0.0), flat(-Y, 0.0), flat(Z, 0.0), flat(-Z, 0.0)];
        assert_eq!(order(X, &swapped), Order::Wrong);
        // Reversing the whole ring turns it the other way: three turns' worth of gaps.
        let back = [flat(-Z, 0.0), flat(-Y, 0.0), flat(Z, 0.0), flat(Y, 0.0)];
        assert_eq!(order(X, &back), Order::Wrong);
    }

    #[test]
    fn an_unknown_direction_only_ever_lets_an_order_stand() {
        let ring = [flat(Y, 0.0), None, flat(-Y, 0.0), flat(-Z, 0.0)];
        assert_eq!(order(X, &ring), Order::Undecided);
        // Still proved wrong: +y, -y, +z, -z with one gap unknown is over a turn already.
        let wrong = [
            flat(Y, 0.0),
            flat(-Y, 0.0),
            flat(Z, 0.0),
            flat(-Z, 0.0),
            None,
        ];
        assert_eq!(order(X, &wrong), Order::Wrong);
    }

    /// The world plane, and a unit cylinder along x lying on it, touching along the x axis.
    fn plane_and_cylinder() -> (Surface, Surface) {
        let plane = Surface::Plane(Plane::new(Frame::WORLD));
        let frame = Frame::new(Point3::new(0.0, 0.0, 1.0), X, Y).unwrap();
        let cyl = Cylinder::new(frame, Length::from_mm(1.0), Sense::Forward).unwrap();
        (plane, Surface::Cylinder(cyl))
    }

    #[test]
    fn faces_tangent_along_the_edge_are_ordered_by_their_bend() {
        let (plane, cyl) = plane_and_cylinder();
        let rho = 1e-6;
        // The plane's half `y > 0` (a forward use about +x) and its half `y < 0`.
        let up = leaves(&plane, Point2::new(0.0, 0.0), Sense::Forward, X, rho).unwrap();
        let down = leaves(&plane, Point2::new(0.0, 0.0), Sense::Reversed, X, rho).unwrap();
        // The cylinder at its foot, the point at (0, 0, 0): its outward normal is -z, so a
        // reversed use leaves along +y, bending up (counter-clockwise about +x).
        let foot = (0..4)
            .map(|k| Point2::new(f64::from(k) * std::f64::consts::FRAC_PI_2, 0.0))
            .find(|&uv| (cyl.point(uv) - Point3::new(0.0, 0.0, 0.0)).length() < 1e-12)
            .unwrap();
        let roll = leaves(&cyl, foot, Sense::Reversed, X, rho).unwrap();
        assert!((roll.d - Y).length() < 1e-12 && (up.d - Y).length() < 1e-12);
        assert!((down.d + Y).length() < 1e-12);
        assert!((roll.bend - 1.0).abs() < 1e-9 && up.bend == 0.0);
        // The plane, then the cylinder just above it, then the plane's other half.
        let right = [Some(up), Some(roll), Some(down)];
        assert_eq!(order(X, &right), Order::Right);
        let wrong = [Some(roll), Some(up), Some(down)];
        assert_eq!(order(X, &wrong), Order::Wrong);
    }

    #[test]
    fn faces_tangent_with_the_same_bend_leave_the_order_to_the_caller() {
        let (plane, _) = plane_and_cylinder();
        let up = leaves(&plane, Point2::new(0.0, 0.0), Sense::Forward, X, 1e-6).unwrap();
        let down = leaves(&plane, Point2::new(0.0, 0.0), Sense::Reversed, X, 1e-6).unwrap();
        // Two coincident halves: either order of the pair stands.
        assert_eq!(
            order(X, &[Some(up), Some(up), Some(down)]),
            Order::Undecided
        );
    }
}
