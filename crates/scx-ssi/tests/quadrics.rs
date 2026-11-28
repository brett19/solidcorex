// Written for solidcorex. Apache-2.0.
//! The curved pairs off one axis (research 73 §4.2): parallel cylinders, two cylinders of one
//! radius whose axes meet, the Miller-Goldman cylinder and cone pairs, and a torus's
//! Villarceau planes. Each is held to SM0's truth (`scx_testkit::ssi_truth`) where it has one
//! (curves, nodes, touches), under rigid motions, and every pair to the surfaces' implicit
//! forms: every answer's point lies on both, and every point where a generator of the first
//! meets the second inside both extents lies on an answer's piece. Each form is also pushed
//! across its critical values: a solved answer within the tolerance, `General` past it.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic
)]

mod common;

use core::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

use common::*;
use scx_num::fmath;
use scx_num::frame::Sense;
use scx_num::units::{Accuracy, Length, Resolution};
use scx_num::vector3d::{Point3, Vec3};
use scx_ssi::analytic::{self, Contact, Intersection, IntersectionCurve, IntersectionPoint};
use scx_surface::{Closed, Surface};
use scx_testkit::ssi_truth::{self as st, Rigid, Surf, Truth};

fn solve_at(a: &Side, b: &Side, tol: f64) -> Closed<Intersection> {
    let acc = Accuracy::new(Length::from_mm(tol)).unwrap();
    let res = Resolution::new(Length::from_mm(tol)).unwrap();
    analytic::intersect(
        &a.surface,
        &a.extent,
        &b.surface,
        &b.extent,
        [acc, acc],
        res,
    )
}

fn solve(a: &Side, b: &Side) -> Closed<Intersection> {
    solve_at(a, b, TOL)
}

/// The pieces of a solved answer (an empty one has none).
fn pieces(
    name: &str,
    ans: Closed<Intersection>,
) -> (Vec<IntersectionCurve>, Vec<IntersectionPoint>) {
    match ans {
        Closed::Solved(Intersection::Pieces { curves, points }) => (curves, points),
        Closed::Solved(Intersection::Empty) => (Vec::new(), Vec::new()),
        other => panic!("{name}: {other:?}"),
    }
}

fn pa(q: [f64; 3]) -> Point3 {
    p(q[0], q[1], q[2])
}

fn va(q: [f64; 3]) -> Vec3 {
    v(q[0], q[1], q[2])
}

/// A unit vector square to `z`, fixed for a given `z`.
fn square_to(z: Vec3) -> Vec3 {
    let k = if z.x.abs() <= z.y.abs() && z.x.abs() <= z.z.abs() {
        v(1.0, 0.0, 0.0)
    } else if z.y.abs() <= z.z.abs() {
        v(0.0, 1.0, 0.0)
    } else {
        v(0.0, 0.0, 1.0)
    };
    *z.cross(k).normalized().unwrap()
}

/// A truth's surface as a side over the heights (a cylinder), the tube angles (a torus) or
/// the square `[-half, half]^2` (a plane).
fn side_of(s: &Surf, extent: [f64; 2]) -> Side {
    match *s {
        Surf::Cylinder {
            point,
            axis,
            radius,
        } => {
            let z = va(axis);
            cylinder(
                frame(pa(point), z, square_to(z)),
                radius,
                Sense::Forward,
                TURN,
                extent,
            )
        }
        Surf::Torus {
            centre,
            axis,
            major,
            minor,
        } => {
            let z = va(axis);
            torus(
                frame(pa(centre), z, square_to(z)),
                major,
                minor,
                Sense::Forward,
                TURN,
                extent,
            )
        }
        Surf::Plane { point, normal } => {
            let z = va(normal);
            plane_side(frame(pa(point), z, square_to(z)), extent, extent)
        }
        Surf::Sphere { .. } => panic!("no sphere here"),
    }
}

/// Samples of a piece, ends included.
fn samples(c: &IntersectionCurve, n: usize) -> Vec<Point3> {
    let (t0, t1) = (c.span.min(), c.span.max());
    (0..=n)
        .map(|i| c.curve.point(t0 + (t1 - t0) * i as f64 / n as f64))
        .collect()
}

/// How far the answer's points may sit from the truth (its rounding at the sizes here).
const NEAR: f64 = 1e-8;

/// The answer against the truth: its curves on the truth's curves and the truth's on its,
/// each crossing piece along `n_a x n_b`, its points the truth's nodes and touches (each a
/// tangent point, at every node inside both extents), and no piece running through a node.
fn against_truth(name: &str, a: &Side, b: &Side, t: &Truth) {
    let (curves, points) = pieces(name, solve(a, b));
    let on = |q: Point3| {
        assert!(
            t.residual(arr(q)) <= NEAR,
            "{name}: {q:?} off the surfaces by {}",
            t.residual(arr(q))
        );
    };
    for c in &curves {
        for q in samples(c, 64) {
            on(q);
            let (_, _, d) = t.nearest(arr(q)).unwrap();
            assert!(d <= NEAR, "{name}: {q:?} {d} from the truth's curves");
        }
        if c.contact == Contact::Transversal {
            let m = 0.5 * (c.span.min() + c.span.max());
            let q = c.curve.point(m);
            let h = 1e-6;
            let along = c.curve.point(m + h) - c.curve.point(m - h);
            let want = normal_at(a, q).unwrap().cross(normal_at(b, q).unwrap());
            assert!(
                along.dot(want) > 0.0,
                "{name}: a piece runs against n_a x n_b"
            );
        }
    }
    for tc in &t.curves {
        for s in tc.samples(256) {
            let q = pa(s);
            if !(inside(a, q, -MARGIN) && inside(b, q, -MARGIN)) {
                continue;
            }
            let d = curves
                .iter()
                .map(|c| distance_to(&c.curve, c.span.min(), c.span.max(), q))
                .fold(f64::INFINITY, f64::min);
            assert!(d <= NEAR, "{name}: truth point {q:?} {d} from the answer");
        }
    }
    let marks: Vec<Point3> = t
        .nodes
        .iter()
        .map(|n| pa(n.point))
        .chain(t.touches.iter().map(|&q| pa(q)))
        .filter(|&q| inside(a, q, -MARGIN) && inside(b, q, -MARGIN))
        .collect();
    for &m in &marks {
        assert!(
            points
                .iter()
                .any(|x| x.point.distance(m) <= NEAR && x.contact == Contact::Tangent),
            "{name}: node or touch {m:?} not answered: {points:?}"
        );
    }
    for x in &points {
        assert!(
            marks.iter().any(|m| m.distance(x.point) <= NEAR),
            "{name}: point {:?} is no node or touch",
            x.point
        );
    }
    no_piece_through(name, &curves, &marks);
}

/// No piece passes through a branch point inside its span: it is cut there.
fn no_piece_through(name: &str, curves: &[IntersectionCurve], marks: &[Point3]) {
    for c in curves {
        let (t0, t1) = (c.span.min(), c.span.max());
        let (p0, p1) = (c.curve.point(t0), c.curve.point(t1));
        for &m in marks {
            let (d, at) = nearest_on(&c.curve, t0, t1, m);
            if d <= NEAR {
                assert!(
                    at.distance(p0).min(at.distance(p1)) <= 1e-6,
                    "{name}: a piece runs through the branch point {m:?}"
                );
            }
        }
    }
}

/// The rigid motions the truths are placed by.
fn motions() -> Vec<Rigid> {
    vec![
        Rigid::translation([0.0; 3]),
        Rigid::turn([1.0, 2.0, 3.0], 0.7, [0.3, -1.2, 2.5]),
        Rigid::turn([-0.4, 0.1, 1.0], 2.9, [-5.0, 0.25, 1.0]),
    ]
}

// ------------------------------------------------------------------ parallel cylinders

fn parallel_sides(t: &Truth, half: f64) -> (Side, Side) {
    (side_of(&t.a, [-half, half]), side_of(&t.b, [-half, half]))
}

#[test]
fn parallel_cylinders_match_the_truth() {
    let half = 3.0;
    for (r1, r2, e) in [
        (1.0, 1.0, 1.0),
        (1.0, 0.5, 1.2),
        (2.0, 0.7, 1.9),
        (1.0, 1.0, 2.0),
        (1.5, 0.5, 2.0),
        (1.5, 0.5, 1.0),
        (1.0, 1.0, 2.5),
        (1.0, 0.25, 0.5),
    ] {
        let t0 = st::cylinders_parallel(r1, r2, e, half).unwrap();
        for m in motions() {
            let t = t0.placed(&m);
            let (a, b) = parallel_sides(&t, half);
            let name = format!("parallel {r1} {r2} {e}");
            against_truth(&name, &a, &b, &t);
            against_truth(&name, &b, &a, &t);
            let (curves, _) = pieces(&name, solve(&a, &b));
            let want = if t.tangent {
                Contact::Tangent
            } else {
                Contact::Transversal
            };
            assert_eq!(curves.len(), t.curves.len(), "{name}");
            assert!(curves.iter().all(|c| c.contact == want), "{name}");
        }
    }
}

/// The lens between two parallel cylinders is `r1 + r2 - e` deep (or `e - |r1 - r2|` for one
/// inside the other): no deeper than the tolerance it is one tangent line, a miss within the
/// tolerance is the same touch, and past it the pair is empty.
#[test]
fn parallel_cylinders_across_their_tangencies() {
    let half = 2.0;
    for (r1, r2, base) in [(1.0, 0.5, 1.5), (1.0, 0.5, 0.5), (0.7, 0.7, 1.4)] {
        for (k, want) in [
            (-4.0, Some(2)),
            (-0.5, Some(1)),
            (0.0, Some(1)),
            (0.5, Some(1)),
            (4.0, Some(0)),
        ] {
            // Outer tangency: closer is deeper; inner: farther is deeper.
            let e = if base == r1 + r2 {
                base + k * TOL
            } else {
                base - k * TOL
            };
            let t = st::cylinders_parallel(r1, r2, e, half).unwrap();
            let (a, b) = parallel_sides(&t, half);
            let name = format!("parallel {r1} {r2} {base} {k}");
            let (curves, points) = pieces(&name, solve(&a, &b));
            assert!(points.is_empty(), "{name}");
            assert_eq!(Some(curves.len()), want, "{name}: {curves:?}");
            if want == Some(1) {
                assert_eq!(curves[0].contact, Contact::Tangent, "{name}");
            } else if want == Some(2) {
                against_truth(&name, &a, &b, &t);
            }
        }
    }
}

/// A tilt of the second axis within the tolerance over its reach is parallel; past it the
/// pair is `General` (no other closed form has unequal radii off one axis).
#[test]
fn parallel_cylinders_tilted_past_the_tolerance_are_general() {
    let half = 2.0;
    let t = st::cylinders_parallel(1.0, 0.6, 1.1, half).unwrap();
    let reach = half + 0.6 + 1.1;
    for (k, solved) in [(0.3, true), (3.0, false)] {
        let tilted = t.placed(&Rigid::turn([1.0, 0.0, 0.0], 0.0, [0.0; 3]));
        let (a, _) = parallel_sides(&tilted, half);
        let angle = k * TOL / reach;
        let z = v(0.0, fmath::sin(angle), fmath::cos(angle));
        let b = cylinder(
            frame(p(1.1, 0.0, 0.0), z, v(1.0, 0.0, 0.0)),
            0.6,
            Sense::Forward,
            TURN,
            [-half, half],
        );
        let ans = solve(&a, &b);
        assert_eq!(
            matches!(ans, Closed::Solved(_)),
            solved,
            "tilt {k}: {ans:?}"
        );
    }
}

// ---------------------------------------------------------- equal cylinders, axes meeting

#[test]
fn steinmetz_matches_the_truth() {
    for r in [1.0, 0.3, 4.0] {
        let t0 = st::cylinders_perpendicular(r, r, 0.0).unwrap();
        assert_eq!(t0.nodes.len(), 2);
        for m in motions() {
            let t = t0.placed(&m);
            let a = side_of(&t.a, [-2.0 * r, 2.0 * r]);
            let b = side_of(&t.b, [-2.0 * r, 2.0 * r]);
            let name = format!("steinmetz {r}");
            against_truth(&name, &a, &b, &t);
            against_truth(&name, &b, &a, &t);
            let (curves, points) = pieces(&name, solve(&a, &b));
            // Two ellipses, each cut at both nodes.
            assert_eq!(curves.len(), 4, "{name}");
            assert_eq!(points.len(), 2, "{name}");
            assert!(curves.iter().all(|c| class(&c.curve) == Class::Ellipse));
        }
    }
}

/// A Steinmetz pair clipped so only one node is inside both extents: the pieces are cut at
/// that node and the other is no point of the answer.
#[test]
fn steinmetz_clipped_to_one_node() {
    let t = st::cylinders_perpendicular(1.0, 1.0, 0.0).unwrap();
    // The nodes are at y = +-1: the second cylinder's angles over the half y > 0.
    let a = side_of(&t.a, [-2.0, 2.0]);
    let Surf::Cylinder {
        point,
        axis,
        radius,
    } = t.b
    else {
        panic!()
    };
    let b = cylinder(
        frame(pa(point), va(axis), v(0.0, 1.0, 0.0)),
        radius,
        Sense::Forward,
        [-FRAC_PI_2, FRAC_PI_2],
        [-2.0, 2.0],
    );
    against_truth("steinmetz half", &a, &b, &t);
    let (_, points) = pieces("steinmetz half", solve(&a, &b));
    assert_eq!(points.len(), 1);
}

/// A sample of every point where a generator of `a` (a cylinder or cone) meets `b` (a
/// cylinder or cone) inside both extents by `MARGIN`, from the quadratic along the generator.
fn generator_points(a: &Side, b: &Side) -> Vec<Point3> {
    let (oa, xa, ya, za) = axes_of(&a.surface).unwrap();
    let (r, k) = match &a.surface {
        Surface::Cylinder(c) => (c.radius().mm(), 0.0),
        Surface::Cone(c) => (c.radius().mm(), fmath::tan(c.half_angle())),
        _ => panic!(),
    };
    let (ob, _, _, zb) = axes_of(&b.surface).unwrap();
    // b as |(x - c) x zb|^2 = (rb + kb (x - ob) . zb)^2.
    let (rb, kb) = match &b.surface {
        Surface::Cylinder(c) => (c.radius().mm(), 0.0),
        Surface::Cone(c) => (c.radius().mm(), fmath::tan(c.half_angle())),
        _ => panic!(),
    };
    let mut out = Vec::new();
    let n = 720;
    for i in 0..n {
        let th = TAU * i as f64 / n as f64;
        let radial = xa * fmath::cos(th) + ya * fmath::sin(th);
        // The generator: x(h) = oa + za h + radial (r + k h).
        let x0 = oa + radial * r;
        let dx = za + radial * k;
        // f(h) = |w x zb|^2 - (rb + kb w.zb)^2, w = x(h) - ob: a quadratic.
        let w0 = x0 - ob;
        let (c0, c1) = (w0.cross(zb), dx.cross(zb));
        let (s0, s1) = (rb + kb * w0.dot(zb), kb * dx.dot(zb));
        let qa = c1.dot(c1) - s1 * s1;
        let qb = 2.0 * (c0.dot(c1) - s0 * s1);
        let qc = c0.dot(c0) - s0 * s0;
        let disc = qb * qb - 4.0 * qa * qc;
        if disc < 0.0 || qa.abs() < 1e-14 {
            continue;
        }
        for sg in [1.0, -1.0] {
            let h = (-qb + sg * disc.sqrt()) / (2.0 * qa);
            let q = x0 + dx * h;
            // On b's own nappe.
            if rb + kb * (q - ob).dot(zb) < 0.0 {
                continue;
            }
            if inside(a, q, -MARGIN) && inside(b, q, -MARGIN) {
                out.push(q);
            }
        }
    }
    out
}

/// The implicit check: every answer point on both surfaces, every generator point on the
/// answer, each crossing piece along `n_a x n_b`, and every branch point a point of the
/// answer that no piece runs through. Returns the pieces.
fn against_surfaces(
    name: &str,
    a: &Side,
    b: &Side,
) -> (Vec<IntersectionCurve>, Vec<IntersectionPoint>) {
    let (curves, points) = pieces(name, solve(a, b));
    let off = |q: Point3| {
        scx_testkit::curve_surface::distance(&a.kind, arr(q))
            .unwrap()
            .abs()
            .max(
                scx_testkit::curve_surface::distance(&b.kind, arr(q))
                    .unwrap()
                    .abs(),
            )
    };
    for c in &curves {
        for q in samples(c, 64) {
            assert!(off(q) <= NEAR, "{name}: {q:?} off by {}", off(q));
            assert!(inside(a, q, 1e-7) && inside(b, q, 1e-7), "{name}: outside");
        }
        if c.contact == Contact::Transversal {
            let m = 0.5 * (c.span.min() + c.span.max());
            let q = c.curve.point(m);
            let along = c.curve.point(m + 1e-6) - c.curve.point(m - 1e-6);
            let want = normal_at(a, q).unwrap().cross(normal_at(b, q).unwrap());
            assert!(along.dot(want) > 0.0, "{name}: against n_a x n_b");
        }
    }
    let owed = generator_points(a, b);
    assert!(
        owed.len() >= 100,
        "{name}: only {} generator points",
        owed.len()
    );
    for q in owed {
        let d = curves
            .iter()
            .map(|c| distance_to(&c.curve, c.span.min(), c.span.max(), q))
            .fold(f64::INFINITY, f64::min);
        // Near a branch point a generator's two roots are rounding-close and inexact.
        let near_branch = points.iter().any(|x| x.point.distance(q) <= 1e-4);
        assert!(
            d <= NEAR || near_branch,
            "{name}: generator point {q:?} {d} from the answer"
        );
    }
    for x in &points {
        assert!(off(x.point) <= NEAR, "{name}: point off");
        assert_eq!(x.contact, Contact::Tangent, "{name}");
        let na = normal_at(a, x.point).unwrap();
        let nb = normal_at(b, x.point).unwrap();
        assert!(
            na.cross(nb).length() <= 1e-7,
            "{name}: the surfaces cross at a point"
        );
    }
    let marks: Vec<Point3> = points.iter().map(|x| x.point).collect();
    no_piece_through(name, &curves, &marks);
    (curves, points)
}

/// Two cylinders of radius `r` whose axes meet at `o` at angle `phi`, the first along `z`.
fn equal_cylinders(r: f64, phi: f64, o: Point3, h: f64) -> (Side, Side) {
    let a = cylinder(
        frame(o, v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
        r,
        Sense::Forward,
        TURN,
        [-h, h],
    );
    let b = cylinder(
        frame(
            o,
            v(fmath::sin(phi), 0.0, fmath::cos(phi)),
            v(0.0, 1.0, 0.0),
        ),
        r,
        Sense::Forward,
        TURN,
        [-h, h],
    );
    (a, b)
}

#[test]
fn equal_cylinders_at_any_angle() {
    for phi in [FRAC_PI_2, FRAC_PI_4, 1.2, 2.5, 0.3] {
        let (a, b) = equal_cylinders(1.0, phi, p(0.5, -0.25, 2.0), 12.0);
        let name = format!("equal cylinders at {phi}");
        let (curves, points) = against_surfaces(&name, &a, &b);
        against_surfaces(&name, &b, &a);
        assert_eq!(points.len(), 2, "{name}");
        assert_eq!(curves.len(), 4, "{name}");
        // The semi-axes of the ellipses: r / sin(phi / 2) and r / cos(phi / 2).
        let mut long: Vec<f64> = curves
            .iter()
            .map(|c| match &c.curve {
                scx_curve::Curve::Ellipse(e) => e.semi_x().mm().max(e.semi_y().mm()),
                other => panic!("{name}: {other:?}"),
            })
            .collect();
        long.sort_by(f64::total_cmp);
        let (s, c) = (fmath::sin(0.5 * phi), fmath::cos(0.5 * phi));
        let want = [
            1.0 / s.max(c),
            1.0 / s.max(c),
            1.0 / s.min(c),
            1.0 / s.min(c),
        ];
        for (got, w) in long.iter().zip(want) {
            assert!((got - w).abs() <= 1e-9, "{name}: {long:?} against {want:?}");
        }
    }
}

/// A radius apart or an axis off by half the tolerance is the same pair (its nodes); by
/// twice the tolerance it is `General`.
#[test]
fn equal_cylinders_across_the_tolerance() {
    for (k, solved) in [(0.0, true), (0.4, true), (2.0, false)] {
        let (a, _) = equal_cylinders(1.0, 1.1, p(0.0, 0.0, 0.0), 4.0);
        let b = cylinder(
            frame(
                p(0.0, 0.0, 0.0),
                v(fmath::sin(1.1_f64), 0.0, fmath::cos(1.1_f64)),
                v(0.0, 1.0, 0.0),
            ),
            1.0 + k * TOL,
            Sense::Forward,
            TURN,
            [-4.0, 4.0],
        );
        let ans = solve(&a, &b);
        assert_eq!(
            matches!(ans, Closed::Solved(_)),
            solved,
            "radius {k}: {ans:?}"
        );
        // The axis moved along their common perpendicular, y.
        let b = cylinder(
            frame(
                p(0.0, k * TOL, 0.0),
                v(fmath::sin(1.1_f64), 0.0, fmath::cos(1.1_f64)),
                v(0.0, 1.0, 0.0),
            ),
            1.0,
            Sense::Forward,
            TURN,
            [-4.0, 4.0],
        );
        let ans = solve(&a, &b);
        assert_eq!(
            matches!(ans, Closed::Solved(_)),
            solved,
            "offset {k}: {ans:?}"
        );
        if let Closed::Solved(Intersection::Pieces { points, .. }) = ans {
            assert_eq!(points.len(), 2, "offset {k}");
        }
    }
}

// ----------------------------------------------------------------------- Miller-Goldman

/// A cone about the sphere of radius `rho` at `o`, its axis along `u`, its apex `dist` from
/// `o` against `u` (opening along `u`), over heights `h` from `o`.
fn cone_about(o: Point3, u: Vec3, rho: f64, dist: f64, h: [f64; 2]) -> Side {
    let al = fmath::asin(rho / dist);
    let r = dist * fmath::tan(al);
    cone(frame(o, u, square_to(u)), r, al, Sense::Forward, TURN, h)
}

#[test]
fn miller_goldman_cylinder_and_cone() {
    let o = p(0.25, 0.5, -1.0);
    let a = cylinder(
        frame(o, v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
        1.0,
        Sense::Forward,
        TURN,
        [-6.0, 6.0],
    );
    for (tilt, dist) in [(0.9, 2.5), (FRAC_PI_2, 1.6), (0.5, 4.0), (2.2, 3.0)] {
        let u = v(fmath::sin(tilt), 0.0, fmath::cos(tilt));
        let b = cone_about(o, u, 1.0, dist, [-dist + 1e-3, 5.0]);
        let name = format!("cylinder, cone {tilt} {dist}");
        let (curves, points) = against_surfaces(&name, &a, &b);
        against_surfaces(&name, &b, &a);
        assert!(!curves.is_empty(), "{name}");
        assert_eq!(points.len(), 2, "{name}");
    }
}

#[test]
fn miller_goldman_two_cones() {
    let o = p(-0.5, 0.0, 0.75);
    for (rho, da, db, tilt) in [
        (1.0, 2.0, 3.0, 1.0_f64),
        (0.5, 1.5, 0.8, 2.0),
        (1.0, 4.0, 4.0, 0.6),
    ] {
        let a = cone_about(o, v(0.0, 0.0, 1.0), rho, da, [-da + 1e-3, 6.0]);
        let u = v(
            fmath::sin(tilt),
            fmath::cos(tilt) * 0.6,
            fmath::cos(tilt) * 0.8,
        );
        let u = *u.normalized().unwrap();
        let b = cone_about(o, u, rho, db, [-db + 1e-3, 6.0]);
        let name = format!("two cones {rho} {da} {db} {tilt}");
        let (curves, points) = against_surfaces(&name, &a, &b);
        against_surfaces(&name, &b, &a);
        assert!(!curves.is_empty(), "{name}");
        assert_eq!(points.len(), 2, "{name}");
    }
}

/// The circles of contact cross (two branch points) or miss (none) as the cone's axis turns
/// past its half-angle from the cylinder's: the cone's circle on the unit sphere is
/// `pi/2 - alpha` about its axis, the cylinder's is the equator. Where they touch, a
/// generator of the cone runs along the cylinder, tangent to it: `General`.
#[test]
fn miller_goldman_branch_points_merge_and_part() {
    let o = p(0.0, 0.0, 0.0);
    let a = cylinder(
        frame(o, v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
        1.0,
        Sense::Forward,
        TURN,
        [-6.0, 6.0],
    );
    let dist = 2.5_f64;
    let al = fmath::asin(1.0 / dist);
    let cone_at = |tilt: f64| {
        let u = v(fmath::sin(tilt), 0.0, fmath::cos(tilt));
        cone_about(o, u, 1.0, dist, [-dist + 1e-3, 5.0])
    };
    assert_eq!(solve(&a, &cone_at(al)), Closed::General);
    for (tilt, want) in [(al + 0.2, 2), (al - 0.2, 0)] {
        let b = cone_at(tilt);
        let name = format!("contact circles at {tilt}");
        let (curves, points) = against_surfaces(&name, &a, &b);
        assert!(!curves.is_empty(), "{name}");
        assert_eq!(points.len(), want, "{name}: {points:?}");
    }
}

/// A cone whose inscribed sphere is off the cylinder's by half the tolerance is the same
/// pair; by twice it, `General`.
#[test]
fn miller_goldman_across_the_tolerance() {
    let o = p(0.0, 0.0, 0.0);
    let a = cylinder(
        frame(o, v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
        1.0,
        Sense::Forward,
        TURN,
        [-6.0, 6.0],
    );
    let u = v(fmath::sin(0.8_f64), 0.0, fmath::cos(0.8_f64));
    for (k, solved) in [
        (0.0, true),
        (0.4, true),
        (-0.4, true),
        (2.0, false),
        (-2.0, false),
    ] {
        let b = cone_about(o, u, 1.0 + k * TOL, 2.5, [-2.4, 5.0]);
        let ans = solve(&a, &b);
        assert_eq!(matches!(ans, Closed::Solved(_)), solved, "rho {k}: {ans:?}");
    }
}

/// Two cylinders whose axes meet but whose radii differ, or whose axes miss: no closed form.
#[test]
fn other_crossed_cylinders_are_general() {
    let (a, _) = equal_cylinders(1.0, 1.0, p(0.0, 0.0, 0.0), 4.0);
    for (r, off) in [(1.5, 0.0), (1.0, 0.25), (0.5, 0.0)] {
        let b = cylinder(
            frame(
                p(0.0, off, 0.0),
                v(fmath::sin(1.0_f64), 0.0, fmath::cos(1.0_f64)),
                v(0.0, 1.0, 0.0),
            ),
            r,
            Sense::Forward,
            TURN,
            [-4.0, 4.0],
        );
        assert_eq!(solve(&a, &b), Closed::General, "{r} {off}");
    }
}

// ------------------------------------------------------------------------- Villarceau

fn villarceau_sides(t: &Truth, big: f64, small: f64) -> (Side, Side) {
    (
        side_of(&t.a, TURN),
        side_of(&t.b, [-2.0 * (big + small), 2.0 * (big + small)]),
    )
}

#[test]
fn villarceau_matches_the_truth() {
    for (big, small) in [(2.0, 1.0), (3.0, 0.5), (1.0, 0.9)] {
        let t0 = st::torus_villarceau(big, small).unwrap();
        for m in motions() {
            let t = t0.placed(&m);
            let (a, b) = villarceau_sides(&t, big, small);
            let name = format!("villarceau {big} {small}");
            against_truth(&name, &a, &b, &t);
            against_truth(&name, &b, &a, &t);
            let (curves, points) = pieces(&name, solve(&a, &b));
            assert_eq!(curves.len(), 4, "{name}");
            assert_eq!(points.len(), 2, "{name}");
            assert!(curves.iter().all(|c| class(&c.curve) == Class::Circle));
        }
    }
}

/// Planes through a torus's centre (the difftest's tori, its `bitangent` cases among them):
/// at the bitangent tilt the Villarceau circles, at the equator two circles, at any other
/// tilt `General`, and never `Empty` while the plane's square reaches the tube. Inside the
/// hole (a square closer to the centre than `big - small`) the section is empty, rightly.
#[test]
fn planes_through_the_torus_centre() {
    for (big, small) in [
        (2.239825293870964, 0.41452704416627156),
        (3.9845306506997993, 1.255209407596167),
        (4.182438453853573, 2.4112502391765753),
    ] {
        let torus = Surf::Torus {
            centre: [0.3, -0.2, 0.1],
            axis: varr(*v(-0.17, 0.01, 0.98).normalized().unwrap()),
            major: big,
            minor: small,
        };
        let a = side_of(&torus, TURN);
        let Surf::Torus { centre, axis, .. } = torus else {
            panic!()
        };
        let (z, o) = (va(axis), pa(centre));
        let x = square_to(z);
        let y = z.cross(x);
        let bitangent = fmath::asin(small / big);
        for turn in [0.0, 1.0, 2.5, 4.0] {
            let (st, ct) = (fmath::sin(turn), fmath::cos(turn));
            for tilt in [
                bitangent,
                0.0,
                bitangent + 0.05,
                bitangent - 0.05,
                1.0,
                FRAC_PI_2,
            ] {
                let n = (x * ct + y * st) * fmath::sin(tilt) + z * fmath::cos(tilt);
                let name = format!("torus {big} {small}, tilt {tilt}, turn {turn}");
                let plane = |h: f64| {
                    side_of(
                        &Surf::Plane {
                            point: arr(o),
                            normal: varr(n),
                        },
                        [-h, h],
                    )
                };
                let ans = solve(&a, &plane(2.0 * (big + small)));
                assert!(
                    !matches!(ans, Closed::Solved(Intersection::Empty)),
                    "{name}: {ans:?}"
                );
                let hole = solve(&a, &plane(0.6 * (big - small)));
                if tilt == bitangent {
                    let (curves, points) = pieces(&name, ans);
                    assert_eq!((curves.len(), points.len()), (4, 2), "{name}");
                    assert_eq!(hole, Closed::Solved(Intersection::Empty), "{name}");
                } else if tilt == 0.0 || tilt == FRAC_PI_2 {
                    assert!(matches!(ans, Closed::Solved(_)), "{name}: {ans:?}");
                } else {
                    assert_eq!(ans, Closed::General, "{name}");
                }
            }
        }
    }
}

/// The plane turned or moved off the bitangent one by half the tolerance over the torus's
/// reach is the same pair; by twice, `General` (a spiric section).
#[test]
fn villarceau_across_the_tolerance() {
    let (big, small) = (2.0, 0.75);
    let t = st::torus_villarceau(big, small).unwrap();
    let Surf::Plane { normal, .. } = t.b else {
        panic!()
    };
    let a = side_of(&t.a, TURN);
    let n = va(normal);
    for (k, solved) in [(0.0, true), (0.4, true), (2.0, false)] {
        // Turned about x, the line through the circles' centres.
        let turn = k * TOL / (big + small);
        let m = Rigid::turn([1.0, 0.0, 0.0], turn, [0.0; 3]);
        let b = side_of(
            &Surf::Plane {
                point: [0.0; 3],
                normal: m.vector(normal),
            },
            [-6.0, 6.0],
        );
        let ans = solve(&a, &b);
        assert_eq!(
            matches!(ans, Closed::Solved(_)),
            solved,
            "turn {k}: {ans:?}"
        );
        let b = side_of(
            &Surf::Plane {
                point: arr(Point3::ORIGIN + n * (k * TOL)),
                normal,
            },
            [-6.0, 6.0],
        );
        let ans = solve(&a, &b);
        assert_eq!(
            matches!(ans, Closed::Solved(_)),
            solved,
            "offset {k}: {ans:?}"
        );
    }
}

/// A torus cut to part of its tube angle: its sides are no linear functions on the circles,
/// so the pair is `General`.
#[test]
fn villarceau_on_part_of_the_tube_is_general() {
    let t = st::torus_villarceau(2.0, 1.0).unwrap();
    let (_, b) = villarceau_sides(&t, 2.0, 1.0);
    let a = side_of(&t.a, [0.0, PI]);
    assert_eq!(solve(&a, &b), Closed::General);
}

// ------------------------------------------------------------------ the depth rule

/// SM10's class-7 cases are the depth rule at the tolerance asked (73 §16.3): a plane into a
/// unit cylinder `5e-7` deep, its lines `2e-3` apart, and a plane `5e-9` into a sphere. At
/// the corpus's tolerance (1e-6) the strip and the cap are no deeper than it and answer one
/// tangent line and one touch; at a tolerance finer than the depth, the two lines and the
/// circle the truth has.
#[test]
fn the_depth_rule_resolves_at_a_finer_tolerance() {
    let cyl = cylinder(
        frame(p(0.0, 0.0, 0.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
        1.0,
        Sense::Forward,
        TURN,
        [-1.0, 1.0],
    );
    let depth = 5e-7;
    let flat = plane_side(
        frame(p(1.0 - depth, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0)),
        [-2.0, 2.0],
        [-2.0, 2.0],
    );
    let (coarse, _) = pieces("strip coarse", solve_at(&cyl, &flat, 1e-6));
    assert_eq!(coarse.len(), 1);
    assert_eq!(coarse[0].contact, Contact::Tangent);
    let (fine, _) = pieces("strip fine", solve_at(&cyl, &flat, 1e-8));
    assert_eq!(fine.len(), 2);
    assert!(fine.iter().all(|c| c.contact == Contact::Transversal));

    let ball = sphere(
        frame(p(0.0, 0.0, 0.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
        1.0,
        Sense::Forward,
        TURN,
        [-FRAC_PI_2, FRAC_PI_2],
    );
    let cap = plane_side(
        frame(p(0.0, 0.0, 1.0 - 5e-9), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
        [-2.0, 2.0],
        [-2.0, 2.0],
    );
    let (curves, points) = pieces("cap coarse", solve_at(&ball, &cap, 1e-6));
    assert!(curves.is_empty() && points.len() == 1);
    let (curves, points) = pieces("cap fine", solve_at(&ball, &cap, 1e-10));
    assert!(points.is_empty() && curves.len() == 1);
}

// ---------------------------------------------------------------- a sphere touching off axis

/// A sphere touching a cylinder or ring torus off its axis (BO22): the one touch point
/// where the centre stands the radii's sum from the spine (or their difference, the sphere
/// inside), within the tolerance either way; nothing past it; a crossing is `General`. The
/// same in either order, and under a rigid motion.
#[test]
fn a_sphere_touches_a_cylinder_or_torus_off_its_axis() {
    let cyl = cylinder(world(), 2.0, Sense::Forward, TURN, [-10.0, 10.0]);
    let ball = |c: [f64; 3], r: f64| {
        sphere(
            frame(pa(c), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
            r,
            Sense::Forward,
            TURN,
            [-FRAC_PI_2, FRAC_PI_2],
        )
    };
    let touch = |name: &str, a: &Side, b: &Side, at: [f64; 3]| {
        for (x, y) in [(a, b), (b, a)] {
            let (curves, points) = pieces(name, solve(x, y));
            assert!(curves.is_empty(), "{name}: {curves:?}");
            assert_eq!(points.len(), 1, "{name}");
            assert_eq!(points[0].contact, Contact::Tangent, "{name}");
            assert!(
                points[0].point.distance(pa(at)) < 1e-12,
                "{name}: {points:?}"
            );
        }
    };
    let empty = |name: &str, a: &Side, b: &Side| {
        for (x, y) in [(a, b), (b, a)] {
            assert_eq!(solve(x, y), Closed::Solved(Intersection::Empty), "{name}");
        }
    };
    // Outside, on the seam ruling's side and a quarter turn from it.
    touch(
        "outside",
        &cyl,
        &ball([5.0, 0.0, 3.0], 3.0),
        [2.0, 0.0, 3.0],
    );
    touch(
        "quarter",
        &cyl,
        &ball([0.0, 5.0, 3.0], 3.0),
        [0.0, 2.0, 3.0],
    );
    // Missed or pressed in by half the tolerance: the same touch.
    touch(
        "miss",
        &cyl,
        &ball([5.0 + 0.5 * TOL, 0.0, 3.0], 3.0),
        [2.0, 0.0, 3.0],
    );
    touch(
        "press",
        &cyl,
        &ball([5.0 - 0.5 * TOL, 0.0, 3.0], 3.0),
        [2.0, 0.0, 3.0],
    );
    // Inside, against the wall.
    touch("inside", &cyl, &ball([1.0, 0.0, 3.0], 1.0), [2.0, 0.0, 3.0]);
    // Apart by more than the tolerance, outside and inside.
    empty("apart", &cyl, &ball([5.0 + 4.0 * TOL, 0.0, 3.0], 3.0));
    empty("within", &cyl, &ball([0.5, 0.0, 3.0], 1.0));
    // Pressed in by more: a quartic, not a closed form here.
    assert!(solve(&cyl, &ball([4.5, 0.0, 3.0], 3.0)).is_general());
    // The touch outside the cylinder's extent: nothing within the extents.
    let short = cylinder(world(), 2.0, Sense::Forward, TURN, [-1.0, 1.0]);
    empty("past the rim", &short, &ball([5.0, 0.0, 3.0], 3.0));

    // A ring torus, R = 5, r = 1.5: outside the tube at 45 degrees round its outer
    // equator's meridian, and inside the tube.
    let ring = torus(world(), 5.0, 1.5, Sense::Forward, TURN, TURN);
    let a = FRAC_PI_4;
    let (c, s) = (fmath::cos(a), fmath::sin(a));
    touch(
        "tube outside",
        &ring,
        &ball([5.0 + 3.5 * c, 0.0, 3.5 * s], 2.0),
        [5.0 + 1.5 * c, 0.0, 1.5 * s],
    );
    touch(
        "tube inside",
        &ring,
        &ball([5.0 + 0.5 * c, 0.0, 0.5 * s], 1.0),
        [5.0 + 1.5 * c, 0.0, 1.5 * s],
    );
    // In the hole, touching the inner equator.
    touch("hole", &ring, &ball([0.0, 1.0, 0.0], 2.5), [0.0, 3.5, 0.0]);
    empty("tube apart", &ring, &ball([5.0, 0.0, 4.0], 2.0));
    assert!(solve(&ring, &ball([5.0, 0.0, 3.0], 2.0)).is_general());

    // A rigid motion carries the touch with it.
    let o = p(1.0, -2.0, 0.5);
    let (z, x) = (va([0.3, -0.4, 0.866]), va([0.8, 0.6, 0.0]));
    let tf = frame(o, z, x);
    let y = *tf.z().cross(*tf.x()).normalized().unwrap();
    let to = |l: [f64; 3]| o + *tf.x() * l[0] + y * l[1] + *tf.z() * l[2];
    let moved = cylinder(tf, 2.0, Sense::Forward, TURN, [-10.0, 10.0]);
    let mb = sphere(
        frame(to([0.0, 5.0, 3.0]), z, x),
        3.0,
        Sense::Forward,
        TURN,
        [-FRAC_PI_2, FRAC_PI_2],
    );
    touch("moved", &moved, &mb, arr(to([0.0, 2.0, 3.0])));
}
