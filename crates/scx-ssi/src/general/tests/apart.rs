// Written for solidcorex. Apache-2.0.
//! The proofs of apartness (73 §4.3, row SM4): separated and near-touching pairs are proved
//! empty, touching and crossing pairs are never dropped, and a dropped pair is apart by
//! sampling. The truths are testkit's closed forms (`scx_testkit::ssi`): whether the whole
//! surfaces meet, and by the pair's construction how far apart they are.

#![allow(clippy::unwrap_used, clippy::panic, clippy::float_cmp)]

use core::f64::consts::TAU;
use std::sync::Arc;

use scx_num::budget::{Budget, Exhausted, Stop};
use scx_num::extent2d::Extent2d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Accuracy, Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Cylinder, Plane, Sphere, Surface};
use scx_testkit::analytic as tk;
use scx_testkit::curve_surface::SurfaceKind;
use scx_testkit::ssi::{Section, intersect as truth};

use super::{acc, res};
use crate::SsiError;
use crate::general::{Candidates, Traced, intersect};

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

const TOL: f64 = 1e-9;

/// An axis frame at `o` whose z is `z` and x is `x` (both unit, orthogonal).
fn fr(o: [f64; 3], z: [f64; 3], x: [f64; 3]) -> (Frame, tk::Frame) {
    let y = [
        z[1] * x[2] - z[2] * x[1],
        z[2] * x[0] - z[0] * x[2],
        z[0] * x[1] - z[1] * x[0],
    ];
    let f = Frame::new(
        Point3::new(o[0], o[1], o[2]),
        Vec3::new(z[0], z[1], z[2]),
        Vec3::new(x[0], x[1], x[2]),
    )
    .unwrap();
    (f, tk::Frame { origin: o, x, y, z })
}

/// A surface over its extent with the truth's kind.
type Part = (Surface, Extent2d, SurfaceKind);

fn sphere(c: [f64; 3], r: f64) -> Part {
    let (f, t) = fr(c, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let s = Sphere::new(f, mm(r), Sense::Forward).unwrap();
    (
        Surface::Sphere(s),
        Extent2d::from_bounds(0.0, 0.0, 1.0, 1.0).unwrap(),
        SurfaceKind::Sphere(tk::Sphere {
            frame: t,
            radius: r,
        }),
    )
}

/// The same sphere as its exact rational NURBS form.
fn nurbs_sphere(c: [f64; 3], r: f64) -> Part {
    let (s, e, k) = sphere(c, r);
    let Surface::Sphere(sp) = &s else { panic!() };
    let n = sp.to_nurbs(&e).unwrap();
    (Surface::Nurbs(Arc::new(n)), e, k)
}

/// The plane `z = h`.
fn plane_z(h: f64) -> Part {
    let (f, t) = fr([0.0, 0.0, h], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    (
        Surface::Plane(Plane::new(f)),
        Extent2d::from_bounds(-8.0, -8.0, 8.0, 8.0).unwrap(),
        SurfaceKind::Plane(tk::Plane { frame: t }),
    )
}

/// The plane `x = h`.
fn plane_x(h: f64) -> Part {
    let (f, t) = fr([h, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    (
        Surface::Plane(Plane::new(f)),
        Extent2d::from_bounds(-8.0, -8.0, 8.0, 8.0).unwrap(),
        SurfaceKind::Plane(tk::Plane { frame: t }),
    )
}

/// The cylinder of radius `r` about the z axis, over a whole turn and `|z| <= 4`.
fn cylinder(r: f64) -> Part {
    let (f, t) = fr([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let c = Cylinder::new(f, mm(r), Sense::Forward).unwrap();
    (
        Surface::Cylinder(c),
        Extent2d::from_bounds(0.0, -4.0, TAU * r, 4.0).unwrap(),
        SurfaceKind::Cylinder(tk::Cylinder {
            frame: t,
            radius: r,
        }),
    )
}

fn run(a: &Part, b: &Part) -> Result<Traced, SsiError> {
    intersect(
        &a.0,
        &a.1,
        &b.0,
        &b.1,
        acc(),
        res(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
}

/// How many piece pairs the proofs (boxes, slabs, field signs) leave.
fn kept(a: &Part, b: &Part) -> usize {
    Candidates::new(
        &a.0,
        &a.1,
        &b.0,
        &b.1,
        acc(),
        res(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .pairs()
    .len()
}

/// Whether the infinite surfaces meet, by the truth.
fn truly_meet(a: &Part, b: &Part) -> bool {
    match truth(&a.2, &b.2).unwrap() {
        Section::Empty => false,
        Section::Coincident | Section::Curves(_) => true,
    }
}

/// `2^-k`, exact.
fn p2(k: i32) -> f64 {
    scx_num::fmath::powi(2.0, -k)
}

/// The separated pairs, each with the gap between the surfaces by construction: sphere over
/// a plane, two spheres, a cylinder beside a plane; the spheres also as NURBS.
fn apart_pairs(g: f64) -> Vec<(&'static str, Part, Part)> {
    vec![
        ("sphere over plane", sphere([0.0; 3], 3.0), plane_z(3.0 + g)),
        (
            "plane under sphere",
            plane_z(-3.0 - g),
            sphere([0.0; 3], 3.0),
        ),
        (
            "nurbs sphere over plane",
            nurbs_sphere([0.0; 3], 3.0),
            plane_z(3.0 + g),
        ),
        (
            "two spheres",
            sphere([0.0; 3], 1.0),
            sphere([2.0 + g, 0.0, 0.0], 1.0),
        ),
        (
            "two nurbs spheres",
            nurbs_sphere([0.0; 3], 1.0),
            nurbs_sphere([2.0 + g, 0.0, 0.0], 1.0),
        ),
        ("cylinder beside plane", cylinder(1.0), plane_x(1.0 + g)),
    ]
}

#[test]
fn separated_pairs_are_proved_empty_where_the_truth_is_empty() {
    for g in [4.0, 1.0, 1e-2, p2(10), p2(20)] {
        for (name, a, b) in apart_pairs(g) {
            assert!(!truly_meet(&a, &b), "truth: {name} at {g}");
            let t = run(&a, &b).unwrap_or_else(|e| panic!("{name} at {g}: {e:?}"));
            assert!(t.is_empty(), "{name} at {g}");
        }
    }
}

#[test]
fn touching_and_crossing_pairs_are_never_dropped() {
    // Gaps at, inside and past the coincidence (1e-9): the truth meets for g <= 0 and the
    // surfaces are within the coincidence of meeting for 0 < g <= 1e-9. No proof (the field
    // signs of row SM6a included) drops every pair of such surfaces; a touch within the
    // coincidence is refused where it lies, or answered as a touch (NURBS against NURBS, row
    // SM5), never answered empty. (A crossing is traced: `field`.)
    for g in [0.0, p2(40), -p2(40), -p2(20), -1e-2, -0.5] {
        for (name, a, b) in apart_pairs(g) {
            if g <= 0.0 {
                assert!(truly_meet(&a, &b), "truth: {name} at {g}");
            }
            assert!(kept(&a, &b) > 0, "{name} at {g}: every pair dropped");
            if g < -p2(40) {
                continue;
            }
            let (xa, xb) = (a.0.box_over(a.1).unwrap(), b.0.box_over(b.1).unwrap());
            let shared = xa.intersect(xb).unwrap_or_else(|| xa.union(xb));
            let within = shared.union(xa).union(xb);
            let at = match run(&a, &b) {
                Err(SsiError::Unresolved { at, size }) => {
                    assert!(size.mm() > 0.0 && size.mm() <= within.max_length());
                    vec![at]
                }
                // NURBS against NURBS: a touch solved and answered (row SM5), or a curve
                // (a circle about 1e-6 across at `-2^-40`) marched (row SM7).
                Ok(t) if !t.touches.is_empty() => t.touches.iter().map(|t| t.point).collect(),
                Ok(t) if !t.branches.is_empty() => t
                    .branches
                    .iter()
                    .flat_map(|b| {
                        b.curve
                            .points()
                            .iter()
                            .map(|&[x, y, z]| Point3::new(x, y, z))
                    })
                    .collect(),
                out => panic!("{name} at {g} was dropped or not refused: {out:?}"),
            };
            for p in at {
                assert!(within.contains_point(p, 0.0), "{name} at {g}: {p:?}");
            }
        }
    }
}

#[test]
fn a_refusal_at_a_touch_names_the_touch() {
    // Sphere and plane tangent at the pole (0, 0, 3): the pair that survives down to the
    // resolution is the one holding it.
    let (a, b) = (sphere([0.0; 3], 3.0), plane_z(3.0));
    let Err(SsiError::Unresolved { at, size }) = run(&a, &b) else {
        panic!("not refused")
    };
    // Within the coincidence of the plane: the sphere's disc of radius sqrt(2 R tol).
    assert!(
        (at - Point3::new(0.0, 0.0, 3.0)).length() <= size.mm() + 2.0 * (6.0 * TOL).sqrt(),
        "{at:?} {size:?}"
    );
    // Two spheres touching at (1, 0, 0), as NURBS too.
    for (a, b) in [
        (sphere([0.0; 3], 1.0), sphere([2.0, 0.0, 0.0], 1.0)),
        (
            nurbs_sphere([0.0; 3], 1.0),
            nurbs_sphere([2.0, 0.0, 0.0], 1.0),
        ),
    ] {
        let Err(SsiError::Unresolved { at, size }) = run(&a, &b) else {
            panic!("not refused")
        };
        assert!(
            (at - Point3::new(1.0, 0.0, 0.0)).length() <= size.mm() + 2.0 * (2.0 * TOL).sqrt(),
            "{at:?} {size:?}"
        );
    }
}

#[test]
fn halving_proves_what_the_cells_cannot() {
    // Two unit spheres 2^-4 apart along a diagonal (the facing octants' boxes overlap, so
    // some cell pairs survive; along an axis the exact boxes would prove it, see
    // `near_touching_analytic_spheres_are_cleared_at_the_cells`), and halving clears every
    // one of them.
    let d = (2.0 + p2(4)) / core::f64::consts::SQRT_2;
    let (a, b) = (sphere([0.0; 3], 1.0), sphere([d, d, 0.0], 1.0));
    let mut budget = Budget::unlimited();
    let c = Candidates::new(
        &a.0,
        &a.1,
        &b.0,
        &b.1,
        acc(),
        res(),
        &mut budget,
        &Stop::new(),
    )
    .unwrap();
    let cell_level = budget.used();
    assert!(!c.pairs().is_empty(), "the cells alone proved it");
    for &p in c.pairs() {
        assert!(
            c.settle(p, 24, &mut budget, &Stop::new())
                .unwrap()
                .is_none(),
            "{p:?}"
        );
    }
    assert!(budget.used() > cell_level);
}

/// Points of a piece over a grid of its uv.
fn samples(s: &Surface, uv: &Extent2d) -> Vec<Point3> {
    let n = 6;
    let mut out = Vec::new();
    for i in 0..=n {
        for j in 0..=n {
            let t = |lo: f64, hi: f64, k: usize| lo + (hi - lo) * (k as f64 / n as f64);
            out.push(s.point(Point2::new(
                t(uv.x_min(), uv.x_max(), i),
                t(uv.y_min(), uv.y_max(), j),
            )));
        }
    }
    out
}

#[test]
fn a_dropped_pair_is_apart_by_sampling_and_the_survivors_are_the_rest() {
    // Every cell pair of a general pair (the pin's tpipe, and a NURBS sphere with a plane
    // through it) is either a survivor or proved apart: the sampled distance between the
    // proved ones exceeds the coincidence.
    let tpipe = {
        let [(a, ae), (b, be)] = super::tpipe();
        [(a, ae), (b, be)]
    };
    let nsp = super::nurbs_sphere_and_plane();
    for pair in [tpipe, nsp] {
        let [(a, ae), (b, be)] = &pair;
        let c = Candidates::new(
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
        // The cells by halving the extents' grids is not exposed: use the survivors and the
        // whole as the check that none of them is a sampled-close pair that was dropped.
        // A pair of random-ish sub-rectangles dropped by `apart` is far by sampling.
        let mut dropped = 0;
        let mut kept = 0;
        for i in 0..4 {
            for j in 0..4 {
                let ua = Extent2d::from_bounds(
                    ae.x_min() + ae.x_length() * f64::from(i) / 4.0,
                    ae.y_min(),
                    ae.x_min() + ae.x_length() * f64::from(i + 1) / 4.0,
                    ae.y_max(),
                )
                .unwrap();
                let ub = Extent2d::from_bounds(
                    be.x_min() + be.x_length() * f64::from(j) / 4.0,
                    be.y_min(),
                    be.x_min() + be.x_length() * f64::from(j + 1) / 4.0,
                    be.y_max(),
                )
                .unwrap();
                let p = crate::general::PiecePair::new(
                    c.piece_of_a(ua).unwrap(),
                    c.piece_of_b(ub).unwrap(),
                );
                if c.apart(&p) {
                    dropped += 1;
                    let (pa, pb) = (samples(a, &ua), samples(b, &ub));
                    let near = pa
                        .iter()
                        .flat_map(|x| pb.iter().map(move |y| (*x - *y).length()))
                        .fold(f64::INFINITY, f64::min);
                    assert!(near > c.tolerance(), "dropped but {near} apart");
                } else {
                    kept += 1;
                }
            }
        }
        assert_eq!(dropped + kept, 16);
    }
}

#[test]
fn the_answer_is_deterministic_and_independent_of_the_budget() {
    // A crossing's trace is the same check in `field`, at a coarser coincidence.
    for g in [p2(10), 0.0] {
        for (name, a, b) in apart_pairs(g) {
            let first = run(&a, &b);
            assert_eq!(run(&a, &b), first, "{name} at {g}");
            let swapped = run(&b, &a);
            assert_eq!(first.is_ok(), swapped.is_ok(), "{name} at {g}");
            let mut counted = Budget::unlimited();
            let again = intersect(
                &a.0,
                &a.1,
                &b.0,
                &b.1,
                acc(),
                res(),
                &mut counted,
                &Stop::new(),
            );
            assert_eq!(again, first);
            // Exactly the work it did suffices, and one unit less does not.
            let used = counted.used();
            let mut exact = Budget::new(used);
            let same = intersect(
                &a.0,
                &a.1,
                &b.0,
                &b.1,
                acc(),
                res(),
                &mut exact,
                &Stop::new(),
            );
            assert_eq!(same, first, "{name} at {g}");
            let mut short = Budget::new(used - 1);
            let cut = intersect(
                &a.0,
                &a.1,
                &b.0,
                &b.1,
                acc(),
                res(),
                &mut short,
                &Stop::new(),
            );
            assert_eq!(
                cut,
                Err(SsiError::Budget(Exhausted { limit: used - 1 })),
                "{name} at {g}"
            );
        }
    }
}

#[test]
fn a_set_stop_ends_the_proofs() {
    let (a, b) = (sphere([0.0; 3], 1.0), sphere([2.0 + p2(10), 0.0, 0.0], 1.0));
    let stop = Stop::new();
    stop.set();
    let e = intersect(
        &a.0,
        &a.1,
        &b.0,
        &b.1,
        acc(),
        res(),
        &mut Budget::unlimited(),
        &stop,
    );
    assert_eq!(e, Err(SsiError::Stopped));
}

#[test]
fn the_tolerance_is_the_coincidence() {
    let (a, b) = (sphere([0.0; 3], 1.0), sphere([2.0, 0.0, 0.0], 1.0));
    let c = Candidates::new(
        &a.0,
        &a.1,
        &b.0,
        &b.1,
        acc(),
        res(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap();
    assert_eq!(c.tolerance(), TOL);
    // A coarser accuracy widens what survives: two spheres 1e-7 apart are apart at 1e-9 and
    // within the coincidence at 1e-6.
    let gap = [sphere([0.0; 3], 1.0), sphere([2.0 + 1e-7, 0.0, 0.0], 1.0)];
    let coarse = Accuracy::new(mm(1e-6)).unwrap();
    let ok = intersect(
        &gap[0].0,
        &gap[0].1,
        &gap[1].0,
        &gap[1].1,
        acc(),
        res(),
        &mut Budget::unlimited(),
        &Stop::new(),
    );
    assert!(ok.unwrap().is_empty());
    let kept = intersect(
        &gap[0].0,
        &gap[0].1,
        &gap[1].0,
        &gap[1].1,
        [coarse, coarse],
        Resolution::new(mm(1e-9)).unwrap(),
        &mut Budget::unlimited(),
        &Stop::new(),
    );
    assert!(matches!(kept, Err(SsiError::Unresolved { .. })), "{kept:?}");
}

/// The cylinder of radius `r` about the line through `(x, 0)` along z, `|z| <= 4`.
fn cylinder_at(x: f64, r: f64) -> Part {
    let (f, t) = fr([x, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let c = Cylinder::new(f, mm(r), Sense::Forward).unwrap();
    (
        Surface::Cylinder(c),
        Extent2d::from_bounds(0.0, -4.0, TAU * r, 4.0).unwrap(),
        SurfaceKind::Cylinder(tk::Cylinder {
            frame: t,
            radius: r,
        }),
    )
}

/// The research-73 truths (`ssi_truth`, row SM0) decide each pair: no contact is proved
/// empty, a touch is refused where it lies, and a curve is never dropped (it is traced:
/// `field`).
#[test]
fn the_sm0_truths_decide_spheres_and_parallel_cylinders() {
    use scx_testkit::ssi_truth::{cylinders_parallel, sphere_sphere};
    // Two spheres 1 and 1.5 at distance d.
    for d in [
        4.0,
        2.6,
        2.5 + p2(10),
        2.5,
        2.4,
        1.0,
        0.6,
        0.5 + p2(10),
        0.5,
        0.4,
    ] {
        let truth = sphere_sphere([0.0; 3], 1.0, [d, 0.0, 0.0], 1.5).unwrap();
        let (a, b) = (sphere([0.0; 3], 1.0), sphere([d, 0.0, 0.0], 1.5));
        let crossing = !truth.is_empty() && d != 2.5 && d != 0.5;
        let out = if crossing {
            Ok(Traced::default())
        } else {
            run(&a, &b)
        };
        if truth.is_empty() {
            // Apart (d > 2.5) or one inside the other (d < 0.5): no contact. Only the first
            // is a proof by apartness.
            if d > 2.5 {
                assert_eq!(out.map(|t| t.is_empty()), Ok(true), "spheres at {d}");
            }
        } else if d == 2.5 || d == 0.5 {
            // A touch: refused where it lies (row SM6b).
            assert!(
                matches!(out, Err(SsiError::Unresolved { .. })),
                "spheres at {d}: {out:?}"
            );
        } else {
            // A circle: never dropped (traced in `field`).
            assert!(kept(&a, &b) > 0, "spheres at {d}");
        }
    }
    // Two parallel cylinders of radii 1 and 0.5 with axes e apart.
    for e in [3.0, 1.6, 1.5 + p2(8), 1.5, 1.2, 0.5] {
        let truth = cylinders_parallel(1.0, 0.5, e, 4.0).unwrap();
        let (a, b) = (cylinder_at(0.0, 1.0), cylinder_at(e, 0.5));
        let crossing = !truth.is_empty() && e != 1.5 && e != 0.5;
        let out = if crossing {
            Ok(Traced::default())
        } else {
            run(&a, &b)
        };
        if truth.is_empty() {
            if e > 1.5 {
                assert_eq!(out.map(|t| t.is_empty()), Ok(true), "cylinders at {e}");
            }
        } else if crossing {
            assert!(kept(&a, &b) > 0, "cylinders at {e}");
        } else {
            assert!(
                matches!(out, Err(SsiError::Unresolved { .. })),
                "cylinders at {e}: {out:?}"
            );
        }
    }
}

#[test]
fn near_touching_analytic_spheres_are_cleared_at_the_cells() {
    // The sphere's boxes are as tight as the quarter arcs' monotone cos and sin: the facing
    // octants of two unit spheres `g` apart are apart by their boxes, so the cells clear it
    // with no halving (73 §20: it took 4,305 pair tests at `2^-10`).
    for k in [10, 20, 29] {
        let (a, b) = (sphere([0.0; 3], 1.0), sphere([2.0 + p2(k), 0.0, 0.0], 1.0));
        let mut budget = Budget::unlimited();
        let t = intersect(
            &a.0,
            &a.1,
            &b.0,
            &b.1,
            acc(),
            res(),
            &mut budget,
            &Stop::new(),
        )
        .unwrap();
        assert!(t.is_empty(), "2^-{k}");
        assert_eq!(budget.used(), 1, "2^-{k}: the cells alone prove it");
    }
}

#[test]
fn a_tilted_near_touch_is_still_cheap() {
    // Spheres in tilted frames, centres on a line at the touching distance plus `g`: the
    // frame's mixed axes cost the boxes their tightness, but the proof stays near the cells.
    let tilt = |c: [f64; 3]| {
        let (f, t) = fr(c, [0.6, 0.0, 0.8], [0.0, 1.0, 0.0]);
        let s = Sphere::new(f, mm(1.0), Sense::Forward).unwrap();
        (
            Surface::Sphere(s),
            Extent2d::from_bounds(0.0, 0.0, 1.0, 1.0).unwrap(),
            SurfaceKind::Sphere(tk::Sphere {
                frame: t,
                radius: 1.0,
            }),
        )
    };
    for k in [10, 20] {
        let (a, b) = (tilt([0.0; 3]), tilt([0.0, 2.0 + p2(k), 0.0]));
        let mut budget = Budget::unlimited();
        let t = intersect(
            &a.0,
            &a.1,
            &b.0,
            &b.1,
            acc(),
            res(),
            &mut budget,
            &Stop::new(),
        )
        .unwrap();
        assert!(t.is_empty(), "2^-{k}");
        assert!(budget.used() < 400, "2^-{k}: {} units", budget.used());
    }
}
