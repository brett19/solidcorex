// Written for solidcorex. Apache-2.0.
//! A NURBS surface's nearest point through [`Surface::invert`] (63 §4.2, research 66):
//! - on random surfaces of every knot and weight kind, the foot is no farther than any of a
//!   dense uv sample, it is the surface's own point at its `uv`, and an interior foot is a
//!   stationary point (the residual is normal to the surface);
//! - on an unclamped B-spline, the same;
//! - on exact rational NURBS of a sphere, a torus and a cylinder, the distance is the
//!   analytic one, also for a query at the sphere's centre, where every point is nearest;
//! - the budget and the stop flag end the search with errors, and a search that finishes
//!   returns the same bits under any budget that lets it and any warm start's place.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic
)]

use core::f64::consts::FRAC_1_SQRT_2;

use scx_num::budget::{Budget, Exhausted, Stop};
use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::hash::Rng;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{Approach, KnotVector};
use scx_nurbs_surface::{Net, NurbsSurface, ParamAxis};
use scx_surface::{Foot, Inversion, Surface, SurfaceError};
use scx_testkit::sample::{self, Knots, Spec, Weights};

const ABOVE: [Approach; 2] = [Approach::Above, Approach::Above];

fn build(
    (pu, pv): (usize, usize),
    ku: Vec<f64>,
    kv: Vec<f64>,
    points: Vec<[f64; 3]>,
    weights: Option<Vec<f64>>,
) -> NurbsSurface<3> {
    let u = KnotVector::new(pu, ku).unwrap();
    let v = KnotVector::new(pv, kv).unwrap();
    let (rows, cols) = (u.point_count(), v.point_count());
    let w = weights.map(|w| Net::new(rows, cols, w).unwrap());
    NurbsSurface::new(u, v, Net::new(rows, cols, points).unwrap(), w).unwrap()
}

/// Random surfaces of every knot and weight kind.
fn randoms(seed: u64) -> Vec<NurbsSurface<3>> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::new();
    for (pu, pv) in [(1, 1), (2, 2), (3, 3), (2, 4), (5, 1)] {
        for knots in [
            Knots::Uniform,
            Knots::Random,
            Knots::Clustered,
            Knots::Multiple,
        ] {
            for weights in [Weights::Unit, Weights::Moderate, Weights::NearZero] {
                let spec = |degree| Spec {
                    degree,
                    count: degree + 3,
                    knots,
                };
                let d = sample::surface::<3>(&mut rng, (spec(pu), spec(pv)), weights);
                let u = KnotVector::new(d.degrees.0, d.knots_u.clone());
                let v = KnotVector::new(d.degrees.1, d.knots_v.clone());
                let (Ok(u), Ok(v)) = (u, v) else { continue };
                let (rows, cols) = (u.point_count(), v.point_count());
                let w = d.weights.clone().map(|w| Net::new(rows, cols, w).unwrap());
                let points = Net::new(rows, cols, d.points.clone()).unwrap();
                if let Ok(n) = NurbsSurface::new(u, v, points, w) {
                    out.push(n);
                }
            }
        }
    }
    assert!(out.len() > 40, "{}", out.len());
    out
}

fn solve(n: &NurbsSurface<3>, p: Point3, near: Option<Point2>) -> (Foot<Point2>, u64) {
    let mut b = Budget::unlimited();
    let s = Surface::from(n.clone());
    let f = s.invert(p, near, None, &mut b, &Stop::new()).unwrap();
    assert!(f.is_inside(), "{f:?}");
    (f.into_foot(), b.used())
}

fn point(n: &NurbsSurface<3>, uv: Point2) -> Point3 {
    let [x, y, z] = n.point(uv.to_array(), ABOVE).unwrap();
    Point3::new(x, y, z)
}

/// The least distance from `p` over a `k x k` grid of the domain.
fn sampled(n: &NurbsSurface<3>, p: Point3, k: usize) -> f64 {
    let e = n.domain();
    let (lo, hi) = (e.min(), e.max());
    let mut best = f64::INFINITY;
    for i in 0..=k {
        for j in 0..=k {
            let t = |a: f64, b: f64, s: usize| a + (b - a) * s as f64 / k as f64;
            let q = point(n, Point2::new(t(lo.x, hi.x, i), t(lo.y, hi.y, j)));
            best = best.min((p - q).length());
        }
    }
    best
}

/// The foot is the surface's point at its `uv`, no farther than a dense sample, and, inside
/// the domain, a stationary point.
fn check_foot(n: &NurbsSurface<3>, p: Point3, f: &Foot<Point2>, scale: f64) {
    assert_eq!(f.point, point(n, f.at), "{f:?}");
    assert_eq!(f.distance, (p - f.point).length());
    let s = sampled(n, p, 60);
    assert!(
        f.distance <= s + 1e-12 * scale,
        "foot {} sample {s} at {p:?}",
        f.distance
    );
    // Along each direction where the foot is not at a knot or the domain's edge (where the
    // surface may crease or end), the residual is normal to the surface.
    if f.distance <= 1e-6 * scale {
        return;
    }
    let j = n.jet::<1>(f.at.to_array(), ABOVE).unwrap();
    let v = |a: [f64; 3]| Vec3::new(a[0], a[1], a[2]);
    let r = f.point - p;
    for (axis, t, d) in [
        (ParamAxis::U, f.at.x, v(j.get(1, 0).unwrap())),
        (ParamAxis::V, f.at.y, v(j.get(0, 1).unwrap())),
    ] {
        let knots = n.knots(axis);
        let span = knots.last().unwrap() - knots.first().unwrap();
        if knots.iter().any(|&k| (k - t).abs() <= 1e-9 * span) {
            continue;
        }
        // A collapsed edge (a tiny weight's): the surface hardly moves along the direction,
        // whose partial is rounding.
        if d.length() <= 1e-6 * scale {
            continue;
        }
        let e = n.domain();
        let (lo, hi) = match axis {
            ParamAxis::U => (e.min().x, e.max().x),
            ParamAxis::V => (e.min().y, e.max().y),
        };
        if t <= lo + 1e-9 * span || t >= hi - 1e-9 * span {
            continue;
        }
        let c = r.dot(d) / (r.length() * d.length());
        assert!(c.abs() < 1e-6, "cos {c} along {axis:?} at {f:?}");
    }
}

#[test]
fn random_surfaces_give_the_global_nearest() {
    let mut rng = Rng::new(0x54);
    let mut work = Vec::new();
    for n in randoms(0x66) {
        for _ in 0..6 {
            let p = Point3::new(
                1.6 * rng.next_signed_unit(),
                1.6 * rng.next_signed_unit(),
                1.6 * rng.next_signed_unit(),
            );
            let (f, used) = solve(&n, p, None);
            check_foot(&n, p, &f, 4.0);
            work.push(used);
        }
    }
    work.sort_unstable();
    let at = |q: usize| work[(work.len() - 1) * q / 100];
    println!(
        "nurbs invert work over {} queries: median {} p90 {} max {}",
        work.len(),
        at(50),
        at(90),
        at(100)
    );
}

#[test]
fn an_unclamped_bspline_gives_the_global_nearest() {
    // A bicubic on uniform knots 0..=9 with no end multiplicity: the domain is [3, 6]^2.
    let mut rng = Rng::new(9);
    let knots: Vec<f64> = (0..10).map(f64::from).collect();
    let points = (0..36)
        .map(|_| {
            [
                rng.next_signed_unit(),
                rng.next_signed_unit(),
                rng.next_signed_unit(),
            ]
        })
        .collect();
    let n = build((3, 3), knots.clone(), knots, points, None);
    assert_eq!(n.domain().min().to_array(), [3.0, 3.0]);
    for _ in 0..20 {
        let p = Point3::new(
            rng.next_signed_unit(),
            rng.next_signed_unit(),
            rng.next_signed_unit(),
        );
        let (f, _) = solve(&n, p, None);
        check_foot(&n, p, &f, 2.0);
    }
}

// ---------------------------------------------------------------- exact analytic NURBS

/// The unit circle in four quarter arcs over `[0, 1]`: knots, `(cos, sin)` points, weights.
fn circle() -> (Vec<f64>, Vec<(f64, f64)>, Vec<f64>) {
    let w = FRAC_1_SQRT_2;
    (
        vec![
            0.0, 0.0, 0.0, 0.25, 0.25, 0.5, 0.5, 0.75, 0.75, 1.0, 1.0, 1.0,
        ],
        vec![
            (1.0, 0.0),
            (1.0, 1.0),
            (0.0, 1.0),
            (-1.0, 1.0),
            (-1.0, 0.0),
            (-1.0, -1.0),
            (0.0, -1.0),
            (1.0, -1.0),
            (1.0, 0.0),
        ],
        vec![1.0, w, 1.0, w, 1.0, w, 1.0, w, 1.0],
    )
}

/// `f(v) (cos, sin)(u) + g(v) z` about the world `z` axis, the profile `(f, g)` a rational
/// curve given as knots, degree, points and weights.
fn revolve(pv: usize, kv: Vec<f64>, prof: &[(f64, f64)], wv: &[f64]) -> NurbsSurface<3> {
    let (ku, around, wu) = circle();
    let mut pts = Vec::new();
    let mut ws = Vec::new();
    for (&(c, s), &a) in around.iter().zip(&wu) {
        for (&(f, g), &b) in prof.iter().zip(wv) {
            pts.push([f * c, f * s, g]);
            ws.push(a * b);
        }
    }
    build((2, pv), ku, kv, pts, Some(ws))
}

fn sphere(r: f64) -> NurbsSurface<3> {
    let w = FRAC_1_SQRT_2;
    let prof = [(0.0, -r), (r, -r), (r, 0.0), (r, r), (0.0, r)];
    revolve(
        2,
        vec![0.0, 0.0, 0.0, 0.5, 0.5, 1.0, 1.0, 1.0],
        &prof,
        &[1.0, w, 1.0, w, 1.0],
    )
}

fn torus(major: f64, minor: f64) -> NurbsSurface<3> {
    let (kv, c, wv) = circle();
    let prof: Vec<(f64, f64)> = c
        .iter()
        .map(|&(a, b)| (major + minor * a, minor * b))
        .collect();
    revolve(2, kv, &prof, &wv)
}

fn cylinder(r: f64, h: f64) -> NurbsSurface<3> {
    revolve(1, vec![-h, -h, h, h], &[(r, -h), (r, h)], &[1.0, 1.0])
}

#[test]
fn exact_analytic_nurbs_give_the_analytic_distance() {
    let mut rng = Rng::new(0xa7);
    let (s, t, c) = (sphere(1.5), torus(2.0, 0.5), cylinder(1.0, 2.0));
    let mut q = || {
        Point3::new(
            3.0 * rng.next_signed_unit(),
            3.0 * rng.next_signed_unit(),
            3.0 * rng.next_signed_unit(),
        )
    };
    for _ in 0..40 {
        let p = q();
        let (f, _) = solve(&s, p, None);
        let want = ((p - Point3::ORIGIN).length() - 1.5).abs();
        assert!(
            (f.distance - want).abs() <= 1e-11,
            "sphere {} want {want} at {p:?}",
            f.distance
        );
        check_foot(&s, p, &f, 3.0);

        let (f, _) = solve(&t, p, None);
        let rho = (p.x * p.x + p.y * p.y).sqrt();
        let want = (fmath::hypot(rho - 2.0, p.z) - 0.5).abs();
        assert!(
            (f.distance - want).abs() <= 1e-11,
            "torus {} want {want} at {p:?}",
            f.distance
        );
        check_foot(&t, p, &f, 5.0);

        let p = Point3::new(p.x, p.y, p.z / 2.0);
        let (f, _) = solve(&c, p, None);
        let want = ((p.x * p.x + p.y * p.y).sqrt() - 1.0).abs();
        assert!(
            (f.distance - want).abs() <= 1e-11,
            "cylinder {} want {want} at {p:?}",
            f.distance
        );
    }
}

#[test]
fn a_query_at_the_sphere_centre_is_certified_at_once() {
    let s = sphere(1.5);
    let (f, used) = solve(&s, Point3::ORIGIN, None);
    assert!((f.distance - 1.5).abs() <= 1e-14, "{f:?}");
    // Every patch's bound is the radius at its root: no subdivision is needed.
    assert!(used < 200, "work {used}");
    println!("sphere centre: work {used}");
}

// ---------------------------------------------------------------- budget, stop, determinism

fn wavy() -> NurbsSurface<3> {
    randoms(0x66).swap_remove(30)
}

#[test]
fn the_budget_ends_the_search() {
    let n = Surface::from(wavy());
    let p = Point3::new(0.3, -0.2, 0.9);
    let mut full = Budget::unlimited();
    let want = n.invert(p, None, None, &mut full, &Stop::new()).unwrap();
    let used = full.used();
    // Exactly the work used finishes with the same bits; one unit less does not finish.
    let mut b = Budget::new(used);
    assert_eq!(n.invert(p, None, None, &mut b, &Stop::new()), Ok(want));
    let mut b = Budget::new(used - 1);
    let e = n.invert(p, None, None, &mut b, &Stop::new());
    assert_eq!(
        e,
        Err(SurfaceError::Exhausted(Exhausted { limit: used - 1 }))
    );
    let mut b = Budget::new(0);
    assert!(matches!(
        n.invert(p, None, None, &mut b, &Stop::new()),
        Err(SurfaceError::Exhausted(_))
    ));
}

#[test]
fn the_stop_flag_ends_the_search_and_a_clear_one_changes_nothing() {
    let n = Surface::from(wavy());
    let p = Point3::new(-0.4, 0.5, 0.1);
    let stop = Stop::new();
    let a = n
        .invert(p, None, None, &mut Budget::unlimited(), &stop)
        .unwrap();
    let b = n
        .invert(p, None, None, &mut Budget::unlimited(), &Stop::new())
        .unwrap();
    assert_eq!(a, b);
    stop.set();
    assert_eq!(
        n.invert(p, None, None, &mut Budget::unlimited(), &stop),
        Err(SurfaceError::Stopped)
    );
    // The analytic arms take neither.
    let plane = Surface::from(scx_surface::Plane::new(scx_num::frame::Frame::WORLD));
    assert!(
        plane
            .invert(p, None, None, &mut Budget::new(0), &stop)
            .is_ok()
    );
}

#[test]
fn a_warm_start_moves_only_the_work() {
    let n = wavy();
    let mut rng = Rng::new(3);
    for _ in 0..10 {
        let p = Point3::new(
            rng.next_signed_unit(),
            rng.next_signed_unit(),
            rng.next_signed_unit(),
        );
        let (f, cold) = solve(&n, p, None);
        let (g, warm) = solve(&n, p, Some(f.at));
        assert!(
            (f.distance - g.distance).abs() <= 1e-12 * 4.0,
            "{f:?} {g:?}"
        );
        let (h, _) = solve(&n, p, Some(Point2::new(9.0, -9.0)));
        assert!(
            (f.distance - h.distance).abs() <= 1e-12 * 4.0,
            "{f:?} {h:?}"
        );
        println!("cold {cold} warm {warm}");
    }
}

#[test]
fn the_foot_is_placed_against_the_domain() {
    let s = Surface::from(sphere(1.0));
    let mut b = Budget::unlimited();
    let stop = Stop::new();
    let p = Point3::new(0.0, 0.0, 3.0); // nearest the north pole, v = 1
    let low = Extent2d::from_bounds(0.0, 0.0, 1.0, 0.5).unwrap();
    let f = s.invert(p, None, Some(&low), &mut b, &stop).unwrap();
    assert!(matches!(f, Inversion::Outside(_)), "{f:?}");
    assert!((f.foot().distance - 2.0).abs() <= 1e-12);
    let all = Extent2d::from_bounds(0.0, 0.0, 1.0, 1.0).unwrap();
    assert!(
        s.invert(p, None, Some(&all), &mut b, &stop)
            .unwrap()
            .is_inside()
    );
    let nan = s
        .invert(Point3::new(f64::NAN, 0.0, 0.0), None, None, &mut b, &stop)
        .unwrap();
    assert!(!nan.is_inside() && nan.foot().distance.is_nan());
}
