// Written for solidcorex. Apache-2.0.
//! Row SM5 against the SM0 truths: loop-free proofs, critical points and seeds
//! ([`Survey`]). A closed loop is never in a pair proved loop-free without a seed on it,
//! every seed lies on the truth curve, and every truth curve has a seed; the plans are the
//! same bits every run, the same seeds with the operands swapped, and bounded by the budget
//! and the stop.

#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::indexing_slicing
)]

use core::f64::consts::TAU;
use std::sync::Arc;

use scx_num::budget::{Budget, Stop};
use scx_num::extent2d::Extent2d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Accuracy, Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_ssi::SsiError;
use scx_ssi::general::{CriticalKind, Loops, Plan, Seed, Site, Survey, intersect};
use scx_surface::{Cylinder, Plane, Sphere, Surface};
use scx_testkit::ssi_truth::{self as truth, Truth};

/// How far a seed may lie from the truth: well inside the coincidence of 1e-9 mm (the
/// root's enclosure and the point's rounding).
const ON: f64 = 1e-9;

fn frame(o: [f64; 3], z: [f64; 3], x: [f64; 3]) -> Frame {
    let v = |a: [f64; 3]| Vec3::new(a[0], a[1], a[2]);
    Frame::new(Point3::new(o[0], o[1], o[2]), v(z), v(x)).unwrap()
}

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn acc() -> [Accuracy; 2] {
    let a = Accuracy::new(mm(1e-9)).unwrap();
    [a, a]
}

fn res() -> Resolution {
    Resolution::new(mm(1e-9)).unwrap()
}

fn ex(u0: f64, v0: f64, u1: f64, v1: f64) -> Extent2d {
    Extent2d::from_bounds(u0, v0, u1, v1).unwrap()
}

/// A tilted frame axis, so no pole or seam of a sphere's uv lies on a symmetry of the case.
const TILT: [f64; 3] = [0.3, -0.4, 0.866_025_403_784_438_6];

fn sphere(c: [f64; 3], r: f64) -> (Surface, Extent2d) {
    let s = Sphere::new(frame(c, TILT, [0.0, 0.908, 0.419]), mm(r), Sense::Forward).unwrap();
    (Surface::Sphere(s), ex(0.0, 0.0, 1.0, 1.0))
}

struct Case {
    name: &'static str,
    a: (Surface, Extent2d),
    b: (Surface, Extent2d),
    truth: Truth,
}

/// A plane at height `3 - delta` through a sphere of radius 3: a loop of radius
/// `sqrt(delta (6 - delta))`.
fn tiny_loop(delta: f64) -> Case {
    let h = 3.0 - delta;
    let p = Plane::new(frame([0.1, 0.2, h], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]));
    Case {
        name: "tiny_loop",
        a: sphere([0.0; 3], 3.0),
        b: (Surface::Plane(p), ex(-5.0, -5.0, 5.0, 5.0)),
        truth: truth::tiny_loop(3.0, delta).unwrap(),
    }
}

/// Two spheres crossing in a circle.
fn spheres() -> Case {
    let (c1, c2) = ([0.0; 3], [1.2, 0.7, -0.5]);
    Case {
        name: "sphere_sphere",
        a: sphere(c1, 2.0),
        b: sphere(c2, 1.5),
        truth: truth::sphere_sphere(c1, 2.0, c2, 1.5).unwrap(),
    }
}

/// Two perpendicular cylinders, radii `r` (axis z) and `big` (axis x, offset `e` in y):
/// two loops, a figure eight, one loop or the Steinmetz pair (`truth::cylinders_perpendicular`).
fn cylinders(r: f64, big: f64, e: f64) -> Case {
    let a = Cylinder::new(
        frame([0.0; 3], [0.0, 0.0, 1.0], [0.955, 0.296, 0.0]),
        mm(r),
        Sense::Forward,
    )
    .unwrap();
    let b = Cylinder::new(
        frame([0.0, e, 0.0], [1.0, 0.0, 0.0], [0.0, 0.389, 0.921]),
        mm(big),
        Sense::Forward,
    )
    .unwrap();
    Case {
        name: "cylinders_perpendicular",
        // Extents off the symmetric heights: an edge of a piece tangent to the other surface
        // (the Steinmetz case's z = r circle) is a double root its seed solve refuses.
        a: (Surface::Cylinder(a), ex(0.0, -2.9, TAU * r, 3.3)),
        b: (Surface::Cylinder(b), ex(0.0, -3.1, TAU * big, 2.7)),
        truth: truth::cylinders_perpendicular(r, big, e).unwrap(),
    }
}

/// The case's surfaces as NURBS: where a side has an implicit form, `intersect` takes the
/// field path (row `SM6a`), so the survey's answer is seen on NURBS against NURBS.
fn as_nurbs(c: &Case) -> [(Surface, Extent2d); 2] {
    [&c.a, &c.b].map(|(s, e)| (Surface::Nurbs(Arc::new(s.to_nurbs(e).unwrap())), *e))
}

fn plans_of(c: &Case, budget: &mut Budget, stop: &Stop) -> Result<Vec<Plan>, SsiError> {
    let ((a, ae), (b, be)) = (&c.a, &c.b);
    let survey = Survey::new(a, ae, b, be, acc(), res(), budget, stop)?;
    survey.plans(budget, stop)
}

fn swapped(c: &Case) -> Case {
    Case {
        name: c.name,
        a: c.b.clone(),
        b: c.a.clone(),
        truth: c.truth.clone(),
    }
}

fn near(a: Point2, b: Point2) -> bool {
    (a.x - b.x).abs().max((a.y - b.y).abs()) <= 1e-9
}

fn arr(p: Point3) -> [f64; 3] {
    [p.x, p.y, p.z]
}

/// Every seed of the plans, edges and rays, in order.
fn seeds(plans: &[Plan]) -> Vec<Seed> {
    let mut out = Vec::new();
    for p in plans {
        out.extend(p.seeds.iter().copied());
        if let Loops::Critical { points, .. } = &p.loops {
            for c in points {
                out.extend(c.ray.iter().copied());
            }
        }
    }
    out
}

fn general_cases() -> Vec<Case> {
    vec![
        tiny_loop(0.5),
        tiny_loop(1e-3),
        spheres(),
        cylinders(1.0, 2.0, 0.3),
        cylinders(1.0, 2.0, 1.6),
    ]
}

#[test]
fn every_seed_is_on_the_truth_and_every_truth_curve_has_one() {
    for c in general_cases() {
        let mut budget = Budget::unlimited();
        let plans = plans_of(&c, &mut budget, &Stop::new()).unwrap_or_else(|e| {
            panic!("{}: {e}", c.name);
        });
        let all = seeds(&plans);
        let mut seeded = vec![false; c.truth.curves.len()];
        for s in &all {
            let p = arr(s.point);
            let r = c.truth.residual(p);
            let (i, _, d) = c.truth.nearest(p).unwrap();
            assert!(
                r <= ON && d <= ON,
                "{}: {s:?} residual {r}, distance {d}",
                c.name
            );
            seeded[i] = true;
        }
        assert!(seeded.iter().all(|&b| b), "{}: {seeded:?}", c.name);
        // No two seeds are one root.
        for (i, s) in all.iter().enumerate() {
            assert!(
                !all[..i].iter().any(|o| o.same_as(s, 1e-9)),
                "{}: {s:?}",
                c.name
            );
        }
    }
}

#[test]
fn a_loop_inside_one_pair_is_never_proved_loop_free() {
    // The 1e-3 loop (radius 0.077) lies inside one pair of pieces: it has no edge seed, so
    // the pair holding it is not loop-free and its ray carries the seed.
    let c = tiny_loop(1e-3);
    let plans = plans_of(&c, &mut Budget::unlimited(), &Stop::new()).unwrap();
    let held: Vec<&Plan> = plans.iter().filter(|p| !p.loops.loop_free()).collect();
    assert!(!held.is_empty());
    for p in &held {
        let Loops::Critical { points, .. } = &p.loops else {
            panic!("{p:?}");
        };
        assert!(points.iter().any(|c| c.may_surround() && !c.ray.is_empty()));
    }
    let rays = seeds(&plans)
        .into_iter()
        .filter(|s| s.site == Site::Ray)
        .count();
    let edges = plans.iter().map(|p| p.seeds.len()).sum::<usize>();
    assert!(rays + edges > 0);
    if edges == 0 {
        assert!(rays > 0);
    }
}

#[test]
fn the_plans_are_the_same_bits_every_run_and_under_any_budget_that_suffices() {
    for c in [tiny_loop(0.5), spheres()] {
        let mut used = Budget::unlimited();
        let first = plans_of(&c, &mut used, &Stop::new()).unwrap();
        assert_eq!(
            plans_of(&c, &mut Budget::unlimited(), &Stop::new()).unwrap(),
            first
        );
        let mut just = Budget::new(used.used());
        assert_eq!(plans_of(&c, &mut just, &Stop::new()).unwrap(), first);
        assert_eq!(just.used(), used.used());
    }
}

#[test]
fn swapping_the_operands_gives_the_same_seeds() {
    for c in [tiny_loop(0.5), spheres(), cylinders(1.0, 2.0, 0.3)] {
        let fwd = seeds(&plans_of(&c, &mut Budget::unlimited(), &Stop::new()).unwrap());
        let rev = seeds(&plans_of(&swapped(&c), &mut Budget::unlimited(), &Stop::new()).unwrap());
        // Rays depend on which surface is first; edge seeds do not.
        let edges = |v: &[Seed]| v.iter().filter(|s| s.site != Site::Ray).count();
        assert_eq!(edges(&fwd), edges(&rev), "{}", c.name);
        for s in fwd.iter().filter(|s| s.site != Site::Ray) {
            let hit = rev.iter().any(|o| {
                let d = o.point - s.point;
                d.dot(d).sqrt() <= ON && near(o.uv[0], s.uv[1]) && near(o.uv[1], s.uv[0])
            });
            assert!(hit, "{}: {s:?}", c.name);
        }
    }
}

#[test]
fn the_budget_and_the_stop_end_the_survey() {
    let c = spheres();
    let mut used = Budget::unlimited();
    plans_of(&c, &mut used, &Stop::new()).unwrap();
    let n = used.used();
    assert!(n > 2);
    for limit in [0, 1, n / 2, n - 1] {
        let r = plans_of(&c, &mut Budget::new(limit), &Stop::new());
        assert!(matches!(r, Err(SsiError::Budget(_))), "{limit}: {r:?}");
    }
    let stop = Stop::new();
    stop.set();
    let r = plans_of(&c, &mut Budget::new(n), &stop);
    assert_eq!(r, Err(SsiError::Stopped));
}

#[test]
fn contacts_are_classified_and_refused_or_answered_by_kind() {
    // External spheres touching at one point: a touch, answered.
    let (c1, c2) = ([0.0; 3], [1.8, 2.4, 0.0]);
    let c = Case {
        name: "touch",
        a: sphere(c1, 1.0),
        b: sphere(c2, 2.0),
        truth: truth::sphere_sphere(c1, 1.0, c2, 2.0).unwrap(),
    };
    assert_eq!(c.truth.touches.len(), 1);
    let plans = plans_of(&c, &mut Budget::unlimited(), &Stop::new()).unwrap();
    let kinds: Vec<CriticalKind> = plans
        .iter()
        .flat_map(|p| match &p.loops {
            Loops::Critical { points, .. } => points.iter().map(|c| c.kind).collect(),
            _ => Vec::new(),
        })
        .collect();
    assert_eq!(kinds, [CriticalKind::Touch]);
    assert!(seeds(&plans).is_empty());
    let [(a, ae), (b, be)] = &as_nurbs(&c);
    let traced = intersect(
        a,
        ae,
        b,
        be,
        acc(),
        res(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap();
    assert_eq!(traced.touches.len(), 1);
    let at = arr(traced.touches[0].point);
    let d = truth::dist(at, c.truth.touches[0]);
    assert!(d <= 1e-6, "{d}");
    // The Steinmetz pair crosses at two nodes: refused where the marcher is needed.
    let c = cylinders(1.5, 1.5, 0.0);
    let plans = plans_of(&c, &mut Budget::unlimited(), &Stop::new()).unwrap();
    let nodes: Vec<Point3> = plans
        .iter()
        .flat_map(|p| match &p.loops {
            Loops::Critical { points, .. } => points
                .iter()
                .filter(|c| c.kind == CriticalKind::Node)
                .map(|c| c.point)
                .collect(),
            _ => Vec::new(),
        })
        .collect();
    assert_eq!(nodes.len(), 2, "{plans:?}");
    for (n, t) in nodes.iter().zip(&c.truth.nodes) {
        let d = c
            .truth
            .nodes
            .iter()
            .map(|t| truth::dist(arr(*n), t.point))
            .fold(f64::MAX, f64::min);
        assert!(d <= 1e-6, "{n:?} {t:?}");
    }
    let [(a, ae), (b, be)] = &as_nurbs(&c);
    let e = intersect(
        a,
        ae,
        b,
        be,
        acc(),
        res(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap_err();
    assert_eq!(e.name(), "ssi_untraced");
}
