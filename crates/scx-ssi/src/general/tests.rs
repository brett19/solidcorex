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

/// The pairs the general method refuses until row SM7: NURBS against NURBS.
fn refused_pairs() -> [[(Surface, Extent2d); 2]; 2] {
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

/// The point an `Untraced` refusal names.
fn untraced(e: SsiError) -> Point3 {
    let SsiError::Untraced { at } = e else {
        panic!("{e:?}");
    };
    at
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
fn a_general_pair_is_refused_by_name_where_it_lies() {
    for (which, pair) in refused_pairs().into_iter().enumerate() {
        let mut budget = Budget::unlimited();
        let e = run(&pair, &mut budget, &Stop::new()).unwrap_err();
        assert_eq!(
            (e.kind(), e.name()),
            (ErrorKind::Unsupported, "ssi_untraced")
        );
        // A proved point of the curve (a seed), which no marcher has traced yet.
        let named = e.at();
        let at = untraced(e);
        assert_eq!(named, Some(at));
        assert!(off(which, at) <= 1e-9, "{at:?}");
        // The pair of pieces, then each piece pair's proofs, were charged.
        assert!(budget.used() > 1);
    }
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
    // A pair with no closed form goes on to the general method, which charges and refuses.
    let [(a, ae), (b, be)] = as_nurbs(tpipe());
    let mut budget = Budget::unlimited();
    let e =
        crate::intersect(&a, &ae, &b, &be, acc(), res(), &mut budget, &Stop::new()).unwrap_err();
    assert_eq!(
        e,
        run(&as_nurbs(tpipe()), &mut Budget::unlimited(), &Stop::new()).unwrap_err()
    );
    assert!(budget.used() > 1);
}

#[test]
fn the_answer_is_the_same_bits_every_run_and_under_any_budget_that_suffices() {
    for pair in refused_pairs() {
        let bits = |budget: &mut Budget| {
            let at = untraced(run(&pair, budget, &Stop::new()).unwrap_err());
            [at.x.to_bits(), at.y.to_bits(), at.z.to_bits()]
        };
        let first = bits(&mut Budget::unlimited());
        assert_eq!(bits(&mut Budget::unlimited()), first);
        assert_eq!(bits(&mut Budget::new(1_000_000)), first);
    }
}

#[test]
fn swapping_the_operands_names_the_same_place() {
    // The least seed either way: the same root, solved with the unknowns in the other order,
    // so the same point to its enclosure's width.
    for pair in refused_pairs() {
        let [p, q] = pair;
        let fwd = untraced(
            run(
                &[p.clone(), q.clone()],
                &mut Budget::unlimited(),
                &Stop::new(),
            )
            .unwrap_err(),
        );
        let rev = untraced(run(&[q, p], &mut Budget::unlimited(), &Stop::new()).unwrap_err());
        assert!((fwd - rev).length() <= 1e-12, "{fwd:?} {rev:?}");
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
            "a curve through (1, 2, 3) needs the marcher, not built",
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
        let named = matches!(e, SsiError::Budget(_) | SsiError::Stopped);
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
