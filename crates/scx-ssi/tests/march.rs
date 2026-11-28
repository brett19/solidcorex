// Written for solidcorex. Apache-2.0.
//! Row SM7 against the SM0 truths: the march for NURBS against NURBS. Every branch lies on
//! the truth curves and covers them, crossings meet at their nodes, and the answer is the
//! same bits every run, under any budget that suffices and with the operands swapped; the
//! budget and the stop end it.

#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::indexing_slicing
)]

use core::f64::consts::TAU;
use std::sync::Arc;

use scx_curve::Curve2;
use scx_num::budget::{Budget, Stop};
use scx_num::extent2d::Extent2d;
use scx_num::fmath::hypot;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Accuracy, Length, Resolution};
use scx_num::vector3d::{Point3, Vec3};
use scx_ssi::SsiError;
use scx_ssi::general::{End, Ends, NodeKind, OnSurface, Traced, intersect};
use scx_surface::{Cylinder, Plane, Sphere, Surface, Torus};
use scx_testkit::ssi_truth::{self as truth, Truth};

/// How far a branch may lie from the truth, and the truth from a branch's chords: the
/// coincidence (`2e-9` here) with room for the rounding of the NURBS images.
const ON: f64 = 1e-8;

/// The most a pcurve's certificate may claim: its chord bound `h^2 d2 / 8` takes the surface's
/// second partials over each run's uv hull and the step in the L1 norm, so it is looser than
/// the points (row `SM8` certifies per span).
const BOUND: f64 = 1e-6;

fn frame(o: [f64; 3], z: [f64; 3], x: [f64; 3]) -> Frame {
    let v = |a: [f64; 3]| Vec3::new(a[0], a[1], a[2]);
    Frame::new(Point3::new(o[0], o[1], o[2]), v(z), v(x)).unwrap()
}

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn acc(tol: f64) -> [Accuracy; 2] {
    let a = Accuracy::new(mm(tol)).unwrap();
    [a, a]
}

/// The coincidence of the accuracy test. The others need no more than [`COARSE`]: the
/// march's vertex count goes as `1/sqrt(tol)`, and they run it many times.
const FINE: f64 = 1e-9;
const COARSE: f64 = 1e-6;

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

fn run(
    pair: &[(Surface, Extent2d); 2],
    tol: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Traced, SsiError> {
    let [(a, ae), (b, be)] = pair;
    intersect(a, ae, b, be, acc(tol), res(), budget, stop)
}

fn polylines(t: &Traced) -> Vec<Vec<[f64; 3]>> {
    t.branches
        .iter()
        .map(|b| b.curve.points().to_vec())
        .collect()
}

fn cases() -> Vec<Case> {
    vec![
        tiny_loop(0.5),
        tiny_loop(1e-3),
        spheres(),
        cylinders(1.0, 2.0, 0.3),
        cylinders(1.0, 2.0, 1.6),
        cylinders(1.5, 1.5, 0.0),
    ]
}

/// The answer's bits: every branch's points, knots, pcurves and ends, and the nodes.
fn bits(t: &Traced) -> Vec<u64> {
    let mut out = Vec::new();
    for b in &t.branches {
        out.extend(b.curve.points().iter().flatten().map(|x| x.to_bits()));
        out.extend(b.curve.knots().iter().map(|x| x.to_bits()));
        for p in &b.pcurves {
            let Curve2::Nurbs(n) = p else { panic!("{p:?}") };
            out.extend(n.points().iter().flatten().map(|x| x.to_bits()));
        }
        out.push(match b.ends {
            Ends::Closed => 0,
            Ends::Open { start, end } => {
                let e = |x: End| match x {
                    End::Rim(r) => 1 + r as u64,
                    End::Node(n) => 100 + n as u64,
                };
                e(start) * 1000 + e(end)
            }
        });
        out.push(b.certificate.to_intersection.mm().to_bits());
    }
    for n in &t.nodes {
        out.extend([n.point.x, n.point.y, n.point.z].map(f64::to_bits));
    }
    out
}

#[test]
fn every_branch_is_on_the_truth_and_covers_it() {
    for c in cases() {
        let t = run(&as_nurbs(&c), FINE, &mut Budget::unlimited(), &Stop::new())
            .unwrap_or_else(|e| panic!("{}: {e:?}", c.name));
        assert!(!t.branches.is_empty(), "{}", c.name);
        let h = c.truth.hausdorff(&polylines(&t), 720);
        assert!(h.to_truth <= ON && h.from_truth <= ON, "{}: {h:?}", c.name);
        for b in &t.branches {
            // Each pcurve maps onto its surface's points of the branch.
            let [(a, _), (s, _)] = &as_nurbs(&c);
            for (k, (surf, p)) in [a, s].into_iter().zip(&b.pcurves).enumerate() {
                let Curve2::Nurbs(n) = p else { panic!() };
                for (uv, q) in n.points().iter().zip(b.curve.points()) {
                    let x = surf.point(scx_num::vector2d::Point2::new(uv[0], uv[1]));
                    let d = (x - Point3::new(q[0], q[1], q[2])).length();
                    let OnSurface::Within(bound) = b.certificate.on_surface[k] else {
                        panic!()
                    };
                    assert!(
                        d <= bound.mm() && bound.mm() <= BOUND,
                        "{}: {d} {bound:?}",
                        c.name
                    );
                }
            }
            let claim = b.certificate.to_intersection.mm();
            assert!(claim <= ON, "{}: {claim}", c.name);
        }
    }
}

#[test]
fn each_closed_curve_is_one_branch() {
    // The tiny loops and the cylinders' two loops lie inside the extents with no seam on
    // them: closed branches, one per truth loop. The spheres' circle crosses the seam of
    // the first sphere's uv once: one branch from the seam node back to it.
    for c in [
        tiny_loop(0.5),
        tiny_loop(1e-3),
        cylinders(1.0, 2.0, 0.3),
        spheres(),
    ] {
        let t = run(
            &as_nurbs(&c),
            COARSE,
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap();
        assert_eq!(t.branches.len(), c.truth.loops(), "{}: {t:?}", c.name);
        for b in &t.branches {
            match b.ends {
                Ends::Closed => {}
                Ends::Open {
                    start: End::Node(i),
                    end: End::Node(j),
                } if i == j => {
                    assert_eq!(t.nodes[i].kind, NodeKind::Seam, "{}", c.name);
                }
                e @ Ends::Open { .. } => panic!("{}: {e:?}", c.name),
            }
        }
    }
}

#[test]
fn seeds_where_a_coordinate_turns_are_met() {
    // A torus (radii 3 and 1, axis z) and the plane at height `z`: two circles about the
    // axis, of radii `3 +/- sqrt(1 - z^2)`, each the image of a `v` line. The survey's seeds
    // on the torus's piece edges include those where the plane's `x` along a circle turns
    // (the circles' least and greatest `x`); there the chord's two ends lie on one side of
    // the seed's coordinate by the chord's sag, which at these coincidences passes the uv
    // box's own margin. Each such seed must be met by the march that passes it, so each
    // circle is traced once (a seed missed there is marched again, met as used: `Stalled`).
    for z in [0.5, 0.7, -0.4] {
        let t = Torus::new(
            frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            mm(3.0),
            mm(1.0),
            Sense::Forward,
        )
        .unwrap();
        let p = Plane::new(frame([0.0, 0.0, z], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]));
        let pair = [
            (Surface::Torus(t), ex(0.0, 0.0, 1.0, 1.0)),
            (Surface::Plane(p), ex(-5.0, -5.0, 5.0, 5.0)),
        ]
        .map(|(s, e)| (Surface::Nurbs(Arc::new(s.to_nurbs(&e).unwrap())), e));
        let [(a, ae), (b, be)] = &pair;
        let off = (1.0 - z * z).sqrt();
        for tol in [1e-5, 1e-6, 1e-7] {
            let acc = Accuracy::new(mm(tol)).unwrap();
            let t = intersect(
                a,
                ae,
                b,
                be,
                [acc, acc],
                res(),
                &mut Budget::unlimited(),
                &Stop::new(),
            )
            .unwrap_or_else(|e| panic!("{z} {tol}: {e:?}"));
            let mut radii = Vec::new();
            for br in &t.branches {
                let closed = match br.ends {
                    Ends::Closed => true,
                    Ends::Open { start, end } => start == end,
                };
                assert!(closed, "{z} {tol}: {:?}", br.ends);
                let q = br.curve.points();
                let r0 = hypot(q[0][0], q[0][1]);
                let big = if r0 > 3.0 { 3.0 + off } else { 3.0 - off };
                for q in q {
                    let d = hypot(hypot(q[0], q[1]) - big, q[2] - z);
                    assert!(d <= tol, "{z} {tol}: {d}");
                }
                radii.push(big);
            }
            radii.sort_by(f64::total_cmp);
            assert_eq!(radii, [3.0 - off, 3.0 + off], "{z} {tol}");
        }
    }
}

#[test]
fn crossing_branches_meet_at_their_nodes() {
    // Steinmetz: two ellipses crossing at (0, +/- 1.5, 0): four arcs, each from node to node.
    let c = cylinders(1.5, 1.5, 0.0);
    let t = run(&as_nurbs(&c), FINE, &mut Budget::unlimited(), &Stop::new()).unwrap();
    let crossings: Vec<_> = t
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Crossing)
        .collect();
    assert_eq!(crossings.len(), 2, "{t:?}");
    for n in &crossings {
        let d = c
            .truth
            .nodes
            .iter()
            .map(|m| truth::dist([n.point.x, n.point.y, n.point.z], m.point))
            .fold(f64::MAX, f64::min);
        assert!(d <= 1e-6, "{n:?}");
    }
    // Each crossing node is the end of four arcs.
    for (i, n) in t.nodes.iter().enumerate() {
        if n.kind != NodeKind::Crossing {
            continue;
        }
        let ends = t
            .branches
            .iter()
            .map(|b| match b.ends {
                Ends::Open { start, end } => {
                    usize::from(start == End::Node(i)) + usize::from(end == End::Node(i))
                }
                Ends::Closed => 0,
            })
            .sum::<usize>();
        assert_eq!(ends, 4, "node {i}: {t:?}");
    }
}

#[test]
fn the_answer_is_the_same_bits_every_run_under_any_budget_and_either_way_round() {
    for c in cases() {
        let pair = as_nurbs(&c);
        let first = run(&pair, COARSE, &mut Budget::unlimited(), &Stop::new()).unwrap();
        let mut used = Budget::unlimited();
        assert_eq!(
            bits(&run(&pair, COARSE, &mut used, &Stop::new()).unwrap()),
            bits(&first)
        );
        let enough = used.used();
        assert_eq!(
            bits(&run(&pair, COARSE, &mut Budget::new(enough), &Stop::new()).unwrap()),
            bits(&first),
            "{}",
            c.name
        );
        // One unit short: the budget, never a shorter answer.
        assert!(matches!(
            run(&pair, COARSE, &mut Budget::new(enough - 1), &Stop::new()),
            Err(SsiError::Budget(_))
        ));
        let [p, q] = pair;
        let rev = run(&[q, p], COARSE, &mut Budget::unlimited(), &Stop::new()).unwrap();
        assert_eq!(rev.branches.len(), first.branches.len(), "{}", c.name);
        for (x, y) in first.branches.iter().zip(&rev.branches) {
            // The same points, the curve reversed; the pcurves and rims swapped.
            let mut pts = y.curve.points().to_vec();
            pts.reverse();
            assert_eq!(x.curve.points(), &pts[..], "{}", c.name);
            let [ya, yb] = &y.pcurves;
            let [Curve2::Nurbs(xa), Curve2::Nurbs(xb)] = &x.pcurves else {
                panic!()
            };
            let [Curve2::Nurbs(ya), Curve2::Nurbs(yb)] = [ya, yb] else {
                panic!()
            };
            let rev_pts = |n: &scx_nurbs::NurbsCurve<2>| {
                let mut v = n.points().to_vec();
                v.reverse();
                v
            };
            assert_eq!(xa.points(), &rev_pts(yb)[..], "{}", c.name);
            assert_eq!(xb.points(), &rev_pts(ya)[..], "{}", c.name);
            // One side's knots are the other's as `T - t`, reversed (the march builds the
            // canonical order and the other is its mirror).
            let mirror = |a: &[f64]| -> Vec<u64> {
                let t = a.last().copied().unwrap();
                a.iter().rev().map(|k| (t - k).to_bits()).collect()
            };
            let bits = |a: &[f64]| a.iter().map(|k| k.to_bits()).collect::<Vec<_>>();
            let (kx, ky) = (x.curve.knots(), y.curve.knots());
            assert!(
                mirror(kx) == bits(ky) || mirror(ky) == bits(kx),
                "{}",
                c.name
            );
            assert_eq!(x.certificate.to_intersection, y.certificate.to_intersection);
        }
        let pts = |t: &Traced| t.nodes.iter().map(|n| n.point).collect::<Vec<_>>();
        assert_eq!(pts(&rev), pts(&first), "{}", c.name);
    }
}

#[test]
fn the_budget_and_the_stop_end_the_march() {
    let pair = as_nurbs(&spheres());
    let mut survey = Budget::unlimited();
    run(&pair, COARSE, &mut survey, &Stop::new()).unwrap();
    // Most of the work is the march's steps; half the budget ends inside it.
    let mut half = Budget::new(survey.used() / 2);
    assert!(matches!(
        run(&pair, COARSE, &mut half, &Stop::new()),
        Err(SsiError::Budget(_))
    ));
    let stop = Stop::new();
    stop.set();
    assert_eq!(
        run(&pair, COARSE, &mut Budget::unlimited(), &stop).unwrap_err(),
        SsiError::Stopped
    );
}
