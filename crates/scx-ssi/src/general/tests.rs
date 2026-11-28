// Written for solidcorex. Apache-2.0.
//! Unit tests: the general intersector's plumbing (budget, stop, the typed refusal, the
//! closed forms first) and its determinism. The proofs of apartness are in `apart`.

#![allow(clippy::unwrap_used, clippy::panic, clippy::float_cmp)]

use core::f64::consts::TAU;
use std::sync::Arc;

use scx_num::budget::{Budget, Exhausted, Stop, Stopped};
use scx_num::cert::{Claim, Method, Reads};
use scx_num::error::ErrorKind;
use scx_num::extent2d::Extent2d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Accuracy, Length, Resolution};
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Cylinder, Plane, Sphere, Surface};

use super::{Certificate, OnSurface, Traced, intersect};
use crate::{Answer, SingularKind, SsiError};

fn frame(o: [f64; 3], z: [f64; 3], x: [f64; 3]) -> Frame {
    let [ox, oy, oz] = o;
    let [zx, zy, zz] = z;
    let [xx, xy, xz] = x;
    Frame::new(
        Point3::new(ox, oy, oz),
        Vec3::new(zx, zy, zz),
        Vec3::new(xx, xy, xz),
    )
    .unwrap()
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

/// The pin's `tpipe` walls: an R=2 cylinder along z and an r=1.5 one along y, crossing at
/// right angles, each over a whole turn (`u` is arc length). Unequal radii, so a quartic: no closed form (`General`).
fn tpipe() -> [(Surface, Extent2d); 2] {
    let pipe = Cylinder::new(
        frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        mm(2.0),
        Sense::Forward,
    )
    .unwrap();
    let branch = Cylinder::new(
        frame([0.0; 3], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        mm(1.5),
        Sense::Forward,
    )
    .unwrap();
    [
        (
            Surface::Cylinder(pipe),
            Extent2d::from_bounds(0.0, -10.0, TAU * 2.0, 10.0).unwrap(),
        ),
        (
            Surface::Cylinder(branch),
            Extent2d::from_bounds(0.0, 0.0, TAU * 1.5, 10.0).unwrap(),
        ),
    ]
}

/// A NURBS sphere of radius 3 (its exact rational form) against a plane through it: a
/// NURBS side has no closed form here.
fn nurbs_sphere_and_plane() -> [(Surface, Extent2d); 2] {
    let s = Sphere::new(
        frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        mm(3.0),
        Sense::Forward,
    )
    .unwrap();
    let e = Extent2d::from_bounds(0.0, 0.0, 1.0, 1.0).unwrap();
    let n = s.to_nurbs(&e).unwrap();
    let p = Plane::new(frame([0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]));
    [
        (Surface::Nurbs(Arc::new(n)), e),
        (
            Surface::Plane(p),
            Extent2d::from_bounds(-5.0, -5.0, 5.0, 5.0).unwrap(),
        ),
    ]
}

/// A pair with both sides as their NURBS images: no implicit form on either side, so the
/// field path (row `SM6a`) does not apply and the pair waits for the march (row `SM7`).
fn as_nurbs(pair: [(Surface, Extent2d); 2]) -> [(Surface, Extent2d); 2] {
    pair.map(|(s, e)| (Surface::Nurbs(Arc::new(s.to_nurbs(&e).unwrap())), e))
}

/// The pairs the general method marches (row SM7): NURBS against NURBS.
fn marched_pairs() -> [[(Surface, Extent2d); 2]; 2] {
    [as_nurbs(tpipe()), as_nurbs(nurbs_sphere_and_plane())]
}

fn run(
    pair: &[(Surface, Extent2d); 2],
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Traced, SsiError> {
    let [(a, ae), (b, be)] = pair;
    intersect(a, ae, b, be, acc(), res(), budget, stop)
}

/// Points of the answer's branches: eight per span of each curve, and its ends.
fn points(t: &Traced) -> Vec<Point3> {
    t.branches
        .iter()
        .flat_map(|b| {
            let mut ks = b.curve.knots().to_vec();
            ks.dedup();
            let mut ts: Vec<f64> = ks
                .iter()
                .zip(ks.iter().skip(1))
                .flat_map(|(&a, &b)| (0..8).map(move |i| a + (b - a) * f64::from(i) / 8.0))
                .collect();
            ts.extend(ks.last());
            ts.into_iter().map(move |u| {
                let [x, y, z] = b.curve.point(u, scx_nurbs::Approach::Above).unwrap();
                Point3::new(x, y, z)
            })
        })
        .collect()
}

/// How far `p` lies from the pair's two surfaces, by their closed forms (`tpipe` first,
/// then `nurbs_sphere_and_plane`).
fn off(which: usize, p: Point3) -> f64 {
    if which == 0 {
        let (r, q) = (
            (p.x * p.x + p.y * p.y).sqrt(),
            (p.x * p.x + p.z * p.z).sqrt(),
        );
        (r - 2.0).abs().max((q - 1.5).abs())
    } else {
        let r = (p.x * p.x + p.y * p.y + p.z * p.z).sqrt();
        (r - 3.0).abs().max((p.z - 1.0).abs())
    }
}

#[test]
fn a_general_pair_is_marched_onto_its_curve() {
    for (which, pair) in marched_pairs().into_iter().enumerate() {
        let mut budget = Budget::unlimited();
        let t = run(&pair, &mut budget, &Stop::new()).unwrap();
        // tpipe: two loops, each across the seam of both cylinders' uv; the sphere and the
        // plane: one circle.
        assert!(!t.branches.is_empty());
        for p in points(&t) {
            assert!(off(which, p) <= 1e-8, "{p:?}");
        }
        // The pair of pieces, each piece pair's proofs and the steps were charged.
        assert!(budget.used() > 1);
    }
}

#[test]
fn a_march_that_meets_no_seed_at_the_rim_is_stalled_there() {
    // Two NURBS planes crossing in the line x = 0.3, z = 0, given only the seed at its
    // y = -1 end: the march crosses the square and reaches the rim at y = 1 with no seed
    // there, so it has left what the survey proved.
    let flat = |o: [f64; 3], z: [f64; 3], x: [f64; 3]| {
        let e = Extent2d::from_bounds(-1.0, -1.0, 1.0, 1.0).unwrap();
        let n = Plane::new(frame(o, z, x)).to_nurbs(&e).unwrap();
        (Surface::Nurbs(Arc::new(n)), e)
    };
    let (a, ae) = flat([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let (b, be) = flat([0.3, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let mut budget = Budget::unlimited();
    let survey =
        super::Survey::new(&a, &ae, &b, &be, acc(), res(), &mut budget, &Stop::new()).unwrap();
    let plans = survey.plans(&mut budget, &Stop::new()).unwrap();
    let mut seeds: Vec<super::Seed> = plans.iter().flat_map(|p| p.seeds.clone()).collect();
    seeds.sort_by(|p, q| p.point.y.total_cmp(&q.point.y));
    assert_eq!(seeds.len(), 2, "{plans:?}");
    seeds.truncate(1);
    let tol = survey.candidates().tolerance();
    let e = super::march::march(
        [&a, &b],
        [&ae, &be],
        &seeds,
        &[],
        tol,
        1e-9,
        false,
        &mut budget,
        &Stop::new(),
    )
    .err()
    .unwrap();
    let SsiError::Stalled { at, step } = e else {
        panic!("{e:?}")
    };
    assert!((at - Point3::new(0.3, 1.0, 0.0)).length() <= 1e-9, "{at:?}");
    assert!(step.mm() > 0.0);
    // With both seeds, one branch from rim to rim.
    let t = intersect(
        &a,
        &ae,
        &b,
        &be,
        acc(),
        res(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap();
    assert_eq!(t.branches.len(), 1, "{t:?}");
}

#[test]
fn the_budget_is_charged_before_the_stop_is_polled() {
    let pair = tpipe();
    let mut none = Budget::new(0);
    assert_eq!(
        run(&pair, &mut none, &Stop::new()),
        Err(SsiError::Budget(Exhausted { limit: 0 }))
    );
    assert_eq!(none.used(), 1);
    let stop = Stop::new();
    stop.set();
    // Out of budget and stopped: the budget answers, as the global solver orders them.
    assert_eq!(
        run(&pair, &mut Budget::new(0), &stop),
        Err(SsiError::Budget(Exhausted { limit: 0 }))
    );
    let mut one = Budget::new(1);
    assert_eq!(run(&pair, &mut one, &stop), Err(SsiError::Stopped));
    assert_eq!(one.used(), 1);
}

#[test]
fn closed_forms_answer_first_and_spend_nothing() {
    // A plane through a sphere: a circle, in closed form, even with no budget and the flag
    // set.
    let s = Sphere::new(
        frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        mm(3.0),
        Sense::Forward,
    )
    .unwrap();
    let se = Extent2d::from_bounds(0.0, 0.0, 1.0, 1.0).unwrap();
    let p = Plane::new(frame([0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]));
    let pe = Extent2d::from_bounds(-5.0, -5.0, 5.0, 5.0).unwrap();
    let (sa, sb) = (Surface::Sphere(s), Surface::Plane(p));
    let stop = Stop::new();
    stop.set();
    let mut none = Budget::new(0);
    let answer = crate::intersect(&sa, &se, &sb, &pe, acc(), res(), &mut none, &stop).unwrap();
    assert!(matches!(answer, Answer::Closed(_)), "{answer:?}");
    assert_eq!(none.used(), 0);
    // A pair with no closed form goes on to the general method, which charges and marches.
    let [(a, ae), (b, be)] = as_nurbs(tpipe());
    let mut budget = Budget::unlimited();
    let answer = crate::intersect(&a, &ae, &b, &be, acc(), res(), &mut budget, &Stop::new());
    let Ok(Answer::Traced(t)) = answer else {
        panic!("{answer:?}")
    };
    assert_eq!(
        t,
        run(&as_nurbs(tpipe()), &mut Budget::unlimited(), &Stop::new()).unwrap()
    );
    assert!(budget.used() > 1);
}

#[test]
fn the_answer_is_the_same_bits_every_run_and_under_any_budget_that_suffices() {
    for pair in marched_pairs() {
        let bits = |budget: &mut Budget| {
            let t = run(&pair, budget, &Stop::new()).unwrap();
            points(&t)
                .iter()
                .flat_map(|p| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()])
                .collect::<Vec<_>>()
        };
        let mut used = Budget::unlimited();
        let first = bits(&mut used);
        assert_eq!(bits(&mut Budget::unlimited()), first);
        assert_eq!(bits(&mut Budget::new(used.used())), first);
    }
}

#[test]
fn swapping_the_operands_gives_the_same_points() {
    // NURBS against NURBS is marched in one order of the operands: the same points either
    // way, each branch reversed.
    for pair in marched_pairs() {
        let [p, q] = pair;
        let fwd = run(
            &[p.clone(), q.clone()],
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap();
        let rev = run(&[q, p], &mut Budget::unlimited(), &Stop::new()).unwrap();
        assert_eq!(fwd.branches.len(), rev.branches.len());
        for (x, y) in fwd.branches.iter().zip(&rev.branches) {
            let mut back = y.curve.points().to_vec();
            back.reverse();
            assert_eq!(x.curve.points(), &back[..]);
        }
    }
}

#[test]
fn every_cause_has_a_kind_a_name_and_a_line() {
    let at = Point3::new(1.0, 2.0, 3.0);
    let cases = [
        (
            SsiError::Unresolved { at, size: mm(0.5) },
            ErrorKind::Unsupported,
            "ssi_unresolved",
            "a region of size 0.5 at (1, 2, 3) is neither proved apart nor loop-free",
        ),
        (
            SsiError::Stalled { at, step: mm(1e-9) },
            ErrorKind::Unsupported,
            "ssi_stalled",
            "the march stalled at (1, 2, 3) with a step of 0.000000001",
        ),
        (
            SsiError::Singular {
                at,
                kind: SingularKind::DegenerateForm,
            },
            ErrorKind::Unsupported,
            "ssi_singular",
            "a contact whose relative curvature form is singular at (1, 2, 3)",
        ),
        (
            SsiError::Singular {
                at,
                kind: SingularKind::HighOrderTangency,
            },
            ErrorKind::Unsupported,
            "ssi_singular",
            "a tangent curve of contact order above 2 at (1, 2, 3)",
        ),
        (
            SsiError::Uncertified { at },
            ErrorKind::Unsupported,
            "ssi_uncertified",
            "a span through (1, 2, 3) could not be certified",
        ),
        (
            SsiError::Untraced { at },
            ErrorKind::Unsupported,
            "ssi_untraced",
            "a tangential contact at (1, 2, 3) is not traced",
        ),
        (
            SsiError::Procedural { operand: 1 },
            ErrorKind::Unsupported,
            "ssi_procedural",
            "operand 1 is a procedural surface, not yet intersected in general",
        ),
        (
            SsiError::Budget(Exhausted { limit: 7 }),
            ErrorKind::Exhausted,
            "budget_exhausted",
            "the work budget of 7 units ran out",
        ),
        (
            SsiError::Stopped,
            ErrorKind::Cancelled,
            "stopped",
            "the caller stopped the operation",
        ),
    ];
    for (e, kind, name, line) in cases {
        assert_eq!(
            (e.kind(), e.name(), e.to_string().as_str()),
            (kind, name, line)
        );
        let named = matches!(
            e,
            SsiError::Procedural { .. } | SsiError::Budget(_) | SsiError::Stopped
        );
        assert_eq!(e.at(), if named { None } else { Some(at) });
    }
    assert_eq!(SingularKind::DegenerateForm.name(), "degenerate_form");
    assert_eq!(
        SingularKind::HighOrderTangency.name(),
        "high_order_tangency"
    );
}

#[test]
fn budget_and_stop_causes_convert_with_the_question_mark() {
    fn charge(budget: &mut Budget, stop: &Stop) -> Result<(), SsiError> {
        budget.charge(1)?;
        stop.check()?;
        Ok(())
    }
    assert_eq!(
        charge(&mut Budget::new(0), &Stop::new()),
        Err(SsiError::Budget(Exhausted { limit: 0 }))
    );
    let stop = Stop::new();
    stop.set();
    assert_eq!(charge(&mut Budget::new(1), &stop), Err(SsiError::Stopped));
    assert_eq!(SsiError::from(Stopped), SsiError::Stopped);
    assert_eq!(charge(&mut Budget::new(1), &Stop::new()), Ok(()));
}

#[test]
fn a_certificate_becomes_records_with_the_callers_keys() {
    let reads = Reads::new(&[1, 2, 3]).unwrap();
    let c = Certificate {
        to_intersection: mm(1e-7),
        on_surface: [OnSurface::Exact, OnSurface::Within(mm(2e-7))],
    };
    let curve = c.curve_cert(reads);
    assert_eq!(
        (curve.claim, curve.bound, curve.method, curve.reads),
        (
            Claim::CurveToIntersection,
            mm(1e-7),
            Method::Krawczyk,
            reads
        )
    );
    // An exact pcurve carries no record; a fitted one its enclosure.
    assert_eq!(c.on_surface[0].cert(reads), None);
    let p = c.on_surface[1].cert(reads).unwrap();
    assert_eq!(
        (p.claim, p.bound, p.method),
        (Claim::PcurveOnSurface, mm(2e-7), Method::Interval)
    );
}

#[test]
fn an_empty_trace_is_the_proof_of_no_meeting() {
    assert!(Traced::default().is_empty());
}

mod apart;
mod field;

/// A procedural operand is refused at once by name, either side, with nothing spent.
#[test]
fn a_procedural_operand_is_refused_before_any_work() {
    use scx_curve::{Curve, Line};
    use scx_num::extent1d::Extent1d;
    use scx_surface::{Procedural, Revolution};
    let g =
        Curve::Line(Line::through(Point3::new(1.0, 0.0, 0.0), Point3::new(2.0, 0.5, 1.0)).unwrap());
    let r = Revolution::new(
        &g,
        Extent1d::new(0.0, 1.0).unwrap(),
        Point3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    )
    .unwrap();
    let p = Surface::Procedural(Arc::new(Procedural::new(r).unwrap()));
    let pe = Extent2d::from_bounds(0.0, 0.0, 1.0, 1.0).unwrap();
    let plane = Surface::Plane(Plane::new(frame(
        [0.0, 0.0, 0.5],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
    )));
    let qe = Extent2d::from_bounds(-5.0, -5.0, 5.0, 5.0).unwrap();
    for (pair, operand) in [
        ([(p.clone(), pe), (plane.clone(), qe)], 0),
        ([(plane, qe), (p, pe)], 1),
    ] {
        let mut budget = Budget::new(1_000);
        let e = run(&pair, &mut budget, &Stop::new());
        assert_eq!(e, Err(SsiError::Procedural { operand }));
        assert_eq!(budget.used(), 0);
    }
}
