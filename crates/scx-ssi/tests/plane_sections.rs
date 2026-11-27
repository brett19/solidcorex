// Written for solidcorex. Apache-2.0.
//! The plane sections (research 63 SS5) against the exact sections (testkit T5,
//! `scx_testkit::ssi`): every answer's curves and points lie on both surfaces and inside
//! both extents, their kinds are the truth's, every truth point inside both extents lies on
//! an answer's piece (so an empty answer is checked, not trusted), a crossing curve runs
//! along `n_s x n_plane`, and `General` comes only where the truth has no closed form. A
//! cone's parabola, which the exact truth never reaches, is checked against the cone's
//! generators met with the plane, and so is every cone answer. Each answer also comes
//! through `intersect` both ways (63 SS2): the surface first the same, the plane first the
//! same pieces run the other way.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic
)]

mod common;

use common::*;
use core::f64::consts::{FRAC_PI_2, PI, TAU};

use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::hash::Rng;
use scx_num::units::{Accuracy, Length, Resolution};
use scx_num::vector3d::{Point3, Vec3};
use scx_ssi::analytic::{self, Contact, Intersection};
use scx_surface::{Closed, Surface};
use scx_testkit::curve_surface as cs;
use scx_testkit::dd::Dd;
use scx_testkit::ssi::{self, Section};

/// What the truth owes: its pieces' sample points inside both extents.
fn owed(s: &Side, pl: &Side, truth: &[ssi::Piece]) -> Vec<Point3> {
    let mut out = Vec::new();
    for piece in truth {
        let pts: Vec<Point3> = match piece.curve {
            ssi::Curve::Point(q) => vec![tpoint(q)],
            ssi::Curve::Line { origin, direction } => {
                let (o, d) = (tpoint(origin), tvec(direction));
                let t0 = -(o - pl_origin(pl)).dot(d) / d.dot(d);
                let span = 200.0 / d.length();
                (0..=1000)
                    .map(|i| o + d * (t0 - span + 2.0 * span * f64::from(i) / 1000.0))
                    .collect()
            }
            // A branch runs both ways from its vertex.
            ssi::Curve::Hyperbola { .. } => (0..=4000)
                .map(|i| {
                    tpoint(
                        piece
                            .curve
                            .point(Dd::new(-10.0 + 20.0 * f64::from(i) / 4000.0)),
                    )
                })
                .collect(),
            _ => (0..720)
                .map(|i| tpoint(piece.curve.point(Dd::new(TAU * f64::from(i) / 720.0))))
                .collect(),
        };
        out.extend(
            pts.into_iter()
                .filter(|&q| inside(s, q, -MARGIN) && inside(pl, q, -MARGIN)),
        );
    }
    out
}

/// What a cone's section owes, from the cone alone: each of `n` generators met with the
/// plane (one division each, so to a few units of the last place), where that lies inside
/// both extents. Empty for another kind.
fn generators_owed(s: &Side, pl: &Side, n: u32) -> Vec<Point3> {
    let Surface::Cone(k) = &s.surface else {
        return Vec::new();
    };
    let (o, x, y, z) = axes_of(&s.surface).unwrap();
    let kk = fmath::tan(k.half_angle());
    let apex = o + z * (-k.radius().mm() / kk);
    let nv = n_of(pl);
    let delta = nv.dot(pl_origin(pl) - apex);
    (0..n)
        .filter_map(|i| {
            let (sn, cs) = fmath::sin_cos(TAU * f64::from(i) / f64::from(n));
            let dir = (x * cs + y * sn) * kk + z;
            let lambda = delta / nv.dot(dir);
            (lambda.is_finite() && lambda * kk >= 0.0).then(|| apex + dir * lambda)
        })
        .filter(|&q| inside(s, q, -MARGIN) && inside(pl, q, -MARGIN))
        .collect()
}

fn n_of(pl: &Side) -> Vec3 {
    match &pl.surface {
        Surface::Plane(k) => *k.frame().z(),
        _ => unreachable!(),
    }
}

fn pl_origin(pl: &Side) -> Point3 {
    match &pl.surface {
        Surface::Plane(k) => k.frame().origin(),
        _ => unreachable!(),
    }
}

/// What an expected answer is, beyond the truth's checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expect {
    /// Checked against the truth only.
    Truth,
    /// `General`.
    General,
    /// Torus meridians: the truth has no closed form; on both surfaces and inside only.
    Meridians(usize),
    /// A cone section the truth has no closed form for (a parabola): `General`, or pieces
    /// checked against the generators.
    Generators,
}

/// `back`, the plane-first answer, is `got` seen from the plane: the same kinds of piece in
/// the same order, each between the same ends run the other way (a tangent curve either
/// way, as `n_a x n_b` vanishes along it; a whole turn by its kind only), the same touch
/// points, and a coincidence's pieces as many (each judged against the sides).
fn reversed(name: &str, got: &Closed<Intersection>, back: &Closed<Intersection>) {
    let near = |p: Point3, q: Point3| p.distance(q) <= 1e-9 * (1.0 + p.to_vec().length());
    match (got.as_solved(), back.as_solved()) {
        (None, None) => {}
        (Some(Intersection::Empty), Some(Intersection::Empty)) => {}
        (Some(Intersection::Coincident(p)), Some(Intersection::Coincident(bp))) => {
            assert_eq!(p.len(), bp.len(), "{name}: coincident pieces {p:?} {bp:?}")
        }
        (
            Some(Intersection::Pieces { curves, points }),
            Some(Intersection::Pieces {
                curves: bc,
                points: bp,
            }),
        ) => {
            assert_eq!(curves.len(), bc.len(), "{name}: pieces {got:?} {back:?}");
            for (a, b) in curves.iter().zip(bc) {
                assert_eq!(
                    core::mem::discriminant(&a.curve),
                    core::mem::discriminant(&b.curve),
                    "{name}: kinds {a:?} {b:?}"
                );
                assert_eq!(a.contact, b.contact, "{name}: contact");
                let ends = |c: &analytic::IntersectionCurve| {
                    (c.curve.point(c.span.min()), c.curve.point(c.span.max()))
                };
                let ((a0, a1), (b0, b1)) = (ends(a), ends(b));
                let (la, lb) = (a.span.max() - a.span.min(), b.span.max() - b.span.min());
                let whole = la >= TAU - 1e-12 && lb >= TAU - 1e-12;
                assert!(
                    whole
                        || (near(a0, b1) && near(a1, b0))
                        || (a.contact == Contact::Tangent && near(a0, b0) && near(a1, b1)),
                    "{name}: ends {a0:?} {a1:?} against {b1:?} {b0:?}"
                );
            }
            assert_eq!(points.len(), bp.len(), "{name}: points");
            for (p, q) in points.iter().zip(bp) {
                assert!(near(p.point, q.point), "{name}: point {p:?} {q:?}");
            }
        }
        _ => panic!("{name}: {got:?} against plane-first {back:?}"),
    }
}

/// Checks the section of `s` by `pl` and returns it.
fn check(name: &str, s: &Side, pl: &Side, expect: Expect) -> Closed<Intersection> {
    let Surface::Plane(plane) = &pl.surface else {
        unreachable!()
    };
    let got = analytic::section_by_plane(
        &s.surface,
        &s.extent,
        plane,
        &pl.extent,
        Resolution::new(Length::from_mm(TOL)).unwrap(),
    );
    // Through `intersect` both ways, the same answer (the extents swap for coincident).
    let acc = Accuracy::new(Length::from_mm(TOL)).unwrap();
    let res = Resolution::new(Length::from_mm(TOL)).unwrap();
    let via = analytic::intersect(
        &s.surface,
        &s.extent,
        &pl.surface,
        &pl.extent,
        [acc, acc],
        res,
    );
    assert_eq!(via, got, "{name}: intersect differs from section_by_plane");
    let back = analytic::intersect(
        &pl.surface,
        &pl.extent,
        &s.surface,
        &s.extent,
        [acc, acc],
        res,
    );
    reversed(name, &got, &back);
    let truth = ssi::intersect(&s.kind, &pl.kind);
    // A cylinder or cone wholly off the plane is empty even where its section has no
    // closed form: the truth's samples check it below.
    let off = matches!(got.as_solved(), Some(Intersection::Empty))
        && matches!(s.surface, Surface::Cylinder(_) | Surface::Cone(_));
    if expect == Expect::Generators && got.is_general() {
        return got;
    }
    if expect == Expect::General && !off {
        if let (Some(Intersection::Empty), Surface::Torus(k)) = (got.as_solved(), &s.surface) {
            // Past the torus's extent along the normal (the core circle's `major s` and the
            // tube's `minor`), where the truth has no closed form.
            let n = n_of(pl);
            let d = n.dot(pl_origin(pl) - k.frame().origin());
            let s = n.cross(*k.frame().z()).length();
            assert!(
                d.abs() > s * k.major().mm() + k.minor().mm(),
                "{name}: torus empty inside the bound"
            );
            return got;
        }
        assert!(got.is_general(), "{name}: expected General, got {got:?}");
        match &truth {
            Err(ssi::Error::NoClosedForm | ssi::Error::Undecided) => {}
            Ok(Section::Curves(t)) => assert!(
                t.iter().any(|q| truth_class(&q.curve) == Class::Other),
                "{name}: General where the truth is closed: {t:?}"
            ),
            other => panic!("{name}: General against truth {other:?}"),
        }
        return got;
    }
    let Some(answer) = got.as_solved() else {
        panic!("{name}: General, truth {truth:?}");
    };
    let n_p = *plane.frame().z();
    let (curves, points) = match answer {
        Intersection::Coincident(p) => {
            assert_eq!(truth, Ok(Section::Coincident), "{name}");
            judge_overlap(name, s, pl, p);
            if let Some(Intersection::Coincident(bp)) = back.as_solved() {
                judge_overlap(&format!("{name} (plane first)"), pl, s, bp);
            }
            return got;
        }
        Intersection::Empty => (&[][..], &[][..]),
        Intersection::Pieces { curves, points } => (&curves[..], &points[..]),
        _ => unreachable!(),
    };
    let tpieces: Vec<ssi::Piece> = match (&truth, expect) {
        (_, Expect::Meridians(n)) => {
            assert_eq!(curves.len(), n, "{name}: meridians {answer:?}");
            Vec::new()
        }
        (_, Expect::Generators) => Vec::new(),
        (Ok(Section::Curves(t)), _) => t.clone(),
        (Ok(Section::Empty), _) => Vec::new(),
        (other, _) => panic!("{name}: truth {other:?}"),
    };
    // On both surfaces: a crossing to rounding, a touch within the tolerance (63 SS4).
    let on = |q: Point3, contact: Contact| {
        let (a, b) = (
            cs::distance(&s.kind, arr(q)).unwrap(),
            cs::distance(&pl.kind, arr(q)).unwrap(),
        );
        let tol = match contact {
            Contact::Transversal => ON * (1.0 + q.to_vec().length()),
            Contact::Tangent => touch_tol(q),
        };
        assert!(
            a <= tol && b <= tol,
            "{name}: {q:?} off the surfaces by {a:e}, {b:e}"
        );
        assert!(
            inside(s, q, 1e-7) && inside(pl, q, 1e-7),
            "{name}: {q:?} outside an extent"
        );
    };
    for c in curves {
        let (t0, t1) = (c.span.min(), c.span.max());
        for i in 0..=32 {
            on(
                c.curve.point(t0 + (t1 - t0) * f64::from(i) / 32.0),
                c.contact,
            );
        }
        // A touch may stand for crossings within the tolerance (the owed points judge it).
        if expect == Expect::Truth && c.contact == Contact::Transversal {
            let k = class(&c.curve);
            assert!(
                tpieces
                    .iter()
                    .any(|q| truth_class(&q.curve) == k && contact(q.contact) == c.contact),
                "{name}: no truth piece of kind {k:?} contact {:?}: {tpieces:?}",
                c.contact
            );
        }
        if c.contact == Contact::Transversal {
            let tm = 0.5 * (t0 + t1);
            let q = c.curve.point(tm);
            let h = 1e-6 * (1.0 + (t1 - t0));
            let tangent = c.curve.point(tm + h) - c.curve.point(tm - h);
            if let Some(n_s) = normal_at(s, q) {
                let want = n_s.cross(n_p);
                if want.length() > 1e-6 {
                    assert!(tangent.dot(want) > 0.0, "{name}: runs against n_s x n_p");
                }
            }
        }
    }
    let tiny = |t: &ssi::Piece| match t.curve {
        ssi::Curve::Point(_) => true,
        ssi::Curve::Circle { a, b, .. } | ssi::Curve::Ellipse { a, b, .. } => {
            tvec(a).length().max(tvec(b).length()) <= TOL
        }
        _ => false,
    };
    for q in points {
        on(q.point, q.contact);
        if expect == Expect::Truth && q.contact == Contact::Transversal {
            assert!(
                tpieces
                    .iter()
                    .any(|t| contact(t.contact) == q.contact || tiny(t)),
                "{name}: point {q:?} against {tpieces:?}"
            );
        }
    }
    for q in owed(s, pl, &tpieces)
        .into_iter()
        .chain(generators_owed(s, pl, 2000))
    {
        let near_curve = curves
            .iter()
            .map(|c| distance_to(&c.curve, c.span.min(), c.span.max(), q))
            .fold(f64::INFINITY, f64::min);
        let near_point = points
            .iter()
            .map(|x| x.point.distance(q))
            .fold(f64::INFINITY, f64::min);
        assert!(
            near_curve.min(near_point) <= 1e-7 * (1.0 + q.to_vec().length())
                || absorbed(s, pl, curves, points, q),
            "{name}: truth point {q:?} missed by {}; got {got:?}; truth {tpieces:?}; extent {:?} step {:?}",
            near_curve.min(near_point),
            s.extent,
            s.step
        );
    }
    judge_depth(name, s, pl, curves);
    got
}

fn counts(got: &Closed<Intersection>) -> (usize, usize) {
    match got.as_solved() {
        Some(Intersection::Pieces { curves, points }) => (curves.len(), points.len()),
        _ => (0, 0),
    }
}

fn kinds(got: &Closed<Intersection>) -> Vec<(Class, Contact)> {
    match got.as_solved() {
        Some(Intersection::Pieces { curves, .. }) => curves
            .iter()
            .map(|c| (class(&c.curve), c.contact))
            .collect(),
        _ => Vec::new(),
    }
}

fn big_plane(o: Point3, n: Vec3, x: Vec3) -> Side {
    plane_side(frame(o, n, x), [-50.0, 50.0], [-50.0, 50.0])
}

// ---------------------------------------------------------------- planes

#[test]
fn plane_plane() {
    let a = plane_side(world(), [-2.0, 3.0], [-1.0, 4.0]);
    let tilted = big_plane(p(0.5, 0.0, 0.0), v(1.0, 0.0, 0.3), v(0.0, 1.0, 0.0));
    let got = check("oblique", &a, &tilted, Expect::Truth);
    assert_eq!(kinds(&got), vec![(Class::Line, Contact::Transversal)]);
    let above = big_plane(p(0.0, 0.0, 1.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
    assert_eq!(
        check("parallel", &a, &above, Expect::Truth).solved(),
        Some(Intersection::Empty)
    );
    let same = plane_side(
        frame(p(1.0, 1.0, 0.0), v(0.0, 0.0, -1.0), v(0.0, 1.0, 0.0)),
        [-1.0, 1.0],
        [-1.0, 1.0],
    );
    assert!(matches!(
        check("coincident", &a, &same, Expect::Truth).solved(),
        Some(Intersection::Coincident(_))
    ));
    let far = plane_side(
        frame(p(40.0, 0.0, 0.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
        [-1.0, 1.0],
        [-1.0, 1.0],
    );
    // Coincident planes whose extents miss: the truth (complete planes) says coincident;
    // the extents make it empty.
    let got = analytic::section_by_plane(
        &far.surface,
        &far.extent,
        match &a.surface {
            Surface::Plane(k) => k,
            _ => unreachable!(),
        },
        &a.extent,
        Resolution::new(Length::from_mm(TOL)).unwrap(),
    );
    assert_eq!(got.solved(), Some(Intersection::Empty));
    judge_overlap("far", &far, &a, &[]);
    // Turned a little and moved: a polygon, judged by samples of each plane's extent.
    for (n, turn) in [0.3_f64, 1.0, 2.5, -0.7].into_iter().enumerate() {
        let (sn, cs) = fmath::sin_cos(turn);
        let b = plane_side(
            frame(
                p(2.0, 2.5, 0.0),
                v(0.0, 0.0, if n % 2 == 0 { 1.0 } else { -1.0 }),
                v(cs, sn, 0.0),
            ),
            [-1.5, 2.0],
            [-1.0, 1.25],
        );
        let got = check(&format!("turned {n}"), &a, &b, Expect::Truth);
        assert!(matches!(got.solved(), Some(Intersection::Coincident(pc)) if pc.len() == 1));
    }
    // Edge to edge: a piece of no width.
    let next = plane_side(
        frame(p(4.0, 0.0, 0.0), v(0.0, 0.0, -1.0), v(-1.0, 0.0, 0.0)),
        [-1.0, 1.0],
        [-1.0, 1.0],
    );
    let got = check("edge to edge", &a, &next, Expect::Truth);
    let Some(Intersection::Coincident(pc)) = got.as_solved() else {
        panic!("edge to edge: {got:?}")
    };
    assert!(
        pc.len() == 1 && pc[0].on_a.iter().all(|q| q.x == 3.0),
        "{pc:?}"
    );
    // A plane crossing the plane outside the extent: empty, checked.
    let off = big_plane(p(10.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0));
    assert_eq!(
        check("off", &a, &off, Expect::Truth).solved(),
        Some(Intersection::Empty)
    );
}

// ---------------------------------------------------------------- cylinders

#[test]
fn cylinder_sections() {
    for sense in [Sense::Forward, Sense::Reversed] {
        let c = cylinder(world(), 2.0, sense, TURN, [-3.0, 3.0]);
        let across = big_plane(p(0.0, 0.0, 1.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
        assert_eq!(
            kinds(&check("circle", &c, &across, Expect::Truth)),
            vec![(Class::Circle, Contact::Transversal)]
        );
        let oblique = big_plane(p(0.0, 0.0, 0.5), v(0.3, 0.2, 1.0), v(1.0, 0.0, -0.3));
        assert_eq!(
            kinds(&check("ellipse", &c, &oblique, Expect::Truth)),
            vec![(Class::Ellipse, Contact::Transversal)]
        );
        let steep = big_plane(p(0.0, 0.0, 0.0), v(1.0, 0.0, 0.6), v(0.0, 1.0, 0.0));
        let got = check("clipped ellipse", &c, &steep, Expect::Truth);
        assert_eq!(counts(&got), (2, 0));
        let along = big_plane(p(1.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0));
        assert_eq!(
            kinds(&check("two lines", &c, &along, Expect::Truth)),
            vec![(Class::Line, Contact::Transversal); 2]
        );
        let touching = big_plane(p(2.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0));
        assert_eq!(
            kinds(&check("tangent line", &c, &touching, Expect::Truth)),
            vec![(Class::Line, Contact::Tangent)]
        );
        let apart = big_plane(p(2.5, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0));
        assert_eq!(
            check("apart", &c, &apart, Expect::Truth).solved(),
            Some(Intersection::Empty)
        );
        let above = big_plane(p(0.0, 0.0, 4.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
        assert_eq!(
            check("above", &c, &above, Expect::Truth).solved(),
            Some(Intersection::Empty)
        );
    }
    // Half a cylinder: the circle's arc, the lines on one side only.
    let half = cylinder(world(), 2.0, Sense::Forward, [0.0, PI], [-3.0, 3.0]);
    let across = big_plane(p(0.0, 0.0, 1.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
    assert_eq!(
        counts(&check("half circle", &half, &across, Expect::Truth)),
        (1, 0)
    );
    let along = big_plane(p(0.0, 1.0, 0.0), v(0.0, 1.0, 0.0), v(1.0, 0.0, 0.0));
    assert_eq!(
        counts(&check("half lines", &half, &along, Expect::Truth)),
        (2, 0)
    );
    let below = big_plane(p(0.0, -1.0, 0.0), v(0.0, 1.0, 0.0), v(1.0, 0.0, 0.0));
    assert_eq!(
        check("half empty", &half, &below, Expect::Truth).solved(),
        Some(Intersection::Empty)
    );
    // A small plane extent clips the circle to an arc.
    let small = plane_side(
        frame(p(2.0, 0.0, 1.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
        [-0.5, 0.5],
        [-0.5, 0.5],
    );
    let full = cylinder(world(), 2.0, Sense::Forward, TURN, [-3.0, 3.0]);
    assert_eq!(counts(&check("arc", &full, &small, Expect::Truth)), (1, 0));
}

// ---------------------------------------------------------------- cones

#[test]
fn cone_sections() {
    for sense in [Sense::Forward, Sense::Reversed] {
        let c = cone(world(), 1.0, 0.4, sense, TURN, [-1.5, 3.0]);
        let across = big_plane(p(0.0, 0.0, 1.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
        assert_eq!(
            kinds(&check("circle", &c, &across, Expect::Truth)),
            vec![(Class::Circle, Contact::Transversal)]
        );
        let oblique = big_plane(p(0.0, 0.0, 1.0), v(0.2, 0.1, 1.0), v(1.0, 0.0, -0.2));
        assert_eq!(
            kinds(&check("ellipse", &c, &oblique, Expect::Truth)),
            vec![(Class::Ellipse, Contact::Transversal)]
        );
        // The apex's height along the axis, and in the world (the axis is z times the sense).
        let apex_z = -1.0 / fmath::tan(0.4);
        let apex_w = apex_z * sense.sign();
        let low = cone(world(), 1.0, 0.4, sense, TURN, [apex_z, 3.0]);
        let shallow = big_plane(p(0.0, 0.0, apex_w), v(0.1, 0.0, 1.0), v(1.0, 0.0, -0.1));
        let got = check("apex", &low, &shallow, Expect::Truth);
        assert_eq!(counts(&got), (0, 1));
        let steep = big_plane(p(0.0, 0.0, apex_w), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0));
        assert_eq!(
            kinds(&check("generators", &low, &steep, Expect::Truth)),
            vec![(Class::Line, Contact::Transversal); 2]
        );
        // Along the axis off it: the branch on the nappe, its vertex below the extent's
        // height, so in two pieces.
        let hyperbola = big_plane(p(0.3, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0));
        assert_eq!(
            kinds(&check("hyperbola", &c, &hyperbola, Expect::Truth)),
            vec![(Class::Hyperbola, Contact::Transversal); 2]
        );
        let tilted = big_plane(p(0.5, 0.2, 1.0), v(1.0, 0.1, 0.15), v(0.0, 1.0, 0.0));
        assert_eq!(
            kinds(&check("hyperbola tilted", &c, &tilted, Expect::Truth)),
            vec![(Class::Hyperbola, Contact::Transversal)]
        );
        // Parallel to a generator (square to it turned into the plane): a parabola, one
        // piece inside it, and outside it (the other nappe) nothing.
        for (h, pieces) in [(-1.5, 1), (1.5, 0)] {
            let (sn, cs) = fmath::sin_cos(0.4);
            let normal = v(cs, 0.0, -sn * sense.sign());
            let on = p(1.0 + h * cs, 0.0, -h * sn * sense.sign());
            let par = big_plane(on, normal, v(0.0, 1.0, 0.0));
            let got = check("parabola", &c, &par, Expect::Generators);
            assert_eq!(
                kinds(&got),
                vec![(Class::Parabola, Contact::Transversal); pieces],
                "parabola at {h}"
            );
        }
        // Square to the axis past the apex: only the other nappe, outside the domain.
        let beyond = big_plane(
            p(0.0, 0.0, (apex_z - 1.0) * sense.sign()),
            v(0.0, 0.0, 1.0),
            v(1.0, 0.0, 0.0),
        );
        let wide = cone(world(), 1.0, 0.4, sense, TURN, [apex_z - 2.0, 3.0]);
        assert_eq!(analytic_empty(&wide, &beyond), Some(Intersection::Empty));
    }
}

/// The answer without the truth's completeness (the truth's cone has both nappes; the
/// kind has one).
fn analytic_empty(s: &Side, pl: &Side) -> Option<Intersection> {
    let Surface::Plane(plane) = &pl.surface else {
        unreachable!()
    };
    analytic::section_by_plane(
        &s.surface,
        &s.extent,
        plane,
        &pl.extent,
        Resolution::new(Length::from_mm(TOL)).unwrap(),
    )
    .solved()
}

// ---------------------------------------------------------------- spheres

#[test]
fn sphere_sections() {
    for sense in [Sense::Forward, Sense::Reversed] {
        // Axis-aligned: a rounded frame is exactly a slight ellipsoid to the truth.
        let s = sphere(
            frame(p(1.0, -2.0, 0.5), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
            3.0,
            sense,
            TURN,
            [-FRAC_PI_2, FRAC_PI_2],
        );
        let cut = big_plane(p(1.0, -2.0, 1.5), v(0.2, 0.0, 1.0), v(1.0, 0.0, -0.2));
        assert_eq!(
            kinds(&check("circle", &s, &cut, Expect::Truth)),
            vec![(Class::Circle, Contact::Transversal)]
        );
        let top = big_plane(p(1.0, -2.0, 3.5), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
        assert_eq!(counts(&check("touch", &s, &top, Expect::Truth)), (0, 1));
        let miss = big_plane(p(1.0, -2.0, 3.6), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
        assert_eq!(
            check("miss", &s, &miss, Expect::Truth).solved(),
            Some(Intersection::Empty)
        );
    }
    // A cap: latitudes clip a meridian plane's circle.
    let cap = sphere(world(), 2.0, Sense::Forward, TURN, [0.5, FRAC_PI_2]);
    let meridian = big_plane(p(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0));
    assert_eq!(
        counts(&check("cap meridian", &cap, &meridian, Expect::Truth)),
        (1, 0)
    );
    let band = sphere(world(), 2.0, Sense::Forward, [0.5, 4.0], [-0.5, 0.7]);
    let oblique = big_plane(p(0.0, 0.0, 0.3), v(0.4, -0.3, 1.0), v(1.0, 0.0, -0.4));
    check("band", &band, &oblique, Expect::Truth);
}

// ---------------------------------------------------------------- tori

/// The depth rule (63 SS4): a plane within the tolerance of a surface's top, inside or
/// out, touches it once at the deepest point; past the tolerance it crosses (inside) or
/// misses (outside). Each answer is checked against the truth, whose crossings a touch
/// stands for when the run between them stays within the tolerance.
#[test]
fn depth_rule() {
    // (depth inside the surface, the answer's kinds)
    let cases = |touch: Vec<(Class, Contact)>, cross: Vec<(Class, Contact)>| {
        [
            (0.5 * TOL, touch.clone()),
            (0.0, touch.clone()),
            (-0.5 * TOL, touch),
            (4.0 * TOL, cross),
            (-4.0 * TOL, Vec::new()),
        ]
    };
    let s = sphere(world(), 2.0, Sense::Forward, TURN, [-FRAC_PI_2, FRAC_PI_2]);
    for (n, (d, want)) in cases(vec![], vec![(Class::Circle, Contact::Transversal)])
        .into_iter()
        .enumerate()
    {
        let pl = big_plane(p(0.0, 0.0, 2.0 - d), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
        let got = check(&format!("sphere {n}"), &s, &pl, Expect::Truth);
        assert_eq!(kinds(&got), want, "sphere {n}: {got:?}");
        assert_eq!(counts(&got).1, usize::from(n < 3), "sphere {n}: {got:?}");
    }
    let c = cylinder(world(), 1.0, Sense::Forward, TURN, [-2.0, 2.0]);
    let lines = vec![
        (Class::Line, Contact::Transversal),
        (Class::Line, Contact::Transversal),
    ];
    for (n, (d, want)) in cases(vec![(Class::Line, Contact::Tangent)], lines)
        .into_iter()
        .enumerate()
    {
        let pl = big_plane(p(1.0 - d, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0));
        let got = check(&format!("cylinder {n}"), &c, &pl, Expect::Truth);
        assert_eq!(kinds(&got), want, "cylinder {n}: {got:?}");
    }
    let t = torus(world(), 2.0, 0.5, Sense::Forward, TURN, TURN);
    let rings = vec![
        (Class::Circle, Contact::Transversal),
        (Class::Circle, Contact::Transversal),
    ];
    for (n, (d, want)) in cases(vec![(Class::Circle, Contact::Tangent)], rings)
        .into_iter()
        .enumerate()
    {
        let pl = big_plane(p(0.0, 0.0, 0.5 - d), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
        let got = check(&format!("torus {n}"), &t, &pl, Expect::Truth);
        assert_eq!(kinds(&got), want, "torus {n}: {got:?}");
    }
    // A cone's tangent plane along a generator, turned about the apex across it: within
    // the tolerance over the extent, one touch; turned further, two generators.
    let k = cone(world(), 1.0, 0.5, Sense::Forward, TURN, [-1.0, 1.0]);
    let (sa, ca) = fmath::sin_cos(0.5);
    let apex = p(0.0, 0.0, -1.0 / fmath::tan(0.5));
    let along = v(sa, 0.0, ca);
    for (n, (tilt, want)) in [
        (1e-12, vec![(Class::Line, Contact::Tangent)]),
        (
            1e-3,
            vec![
                (Class::Line, Contact::Transversal),
                (Class::Line, Contact::Transversal),
            ],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        // The tangent plane's normal, turned about the generator's square in the plane.
        let nrm = v(ca, 0.0, -sa) * fmath::cos(tilt) + along * fmath::sin(tilt);
        let pl = big_plane(apex, nrm, v(0.0, 1.0, 0.0));
        // The apex is not exact in doubles, so the truth meets a slight hyperbola: the
        // pieces are checked against the generators instead.
        let got = check(&format!("cone {n}"), &k, &pl, Expect::Generators);
        assert_eq!(kinds(&got), want, "cone {n}: {got:?}");
    }
}

#[test]
fn torus_sections() {
    for sense in [Sense::Forward, Sense::Reversed] {
        let t = torus(world(), 3.0, 1.0, sense, TURN, TURN);
        let across = big_plane(p(0.0, 0.0, 0.5), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
        assert_eq!(
            kinds(&check("two circles", &t, &across, Expect::Truth)),
            vec![(Class::Circle, Contact::Transversal); 2]
        );
        let top = big_plane(p(0.0, 0.0, 1.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
        assert_eq!(
            kinds(&check("top circle", &t, &top, Expect::Truth)),
            vec![(Class::Circle, Contact::Tangent)]
        );
        let miss = big_plane(p(0.0, 0.0, 1.2), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
        assert_eq!(
            check("miss", &t, &miss, Expect::Truth).solved(),
            Some(Intersection::Empty)
        );
        let meridian = big_plane(p(0.0, 0.0, 0.0), v(1.0, 1.0, 0.0), v(0.0, 0.0, 1.0));
        check("meridians", &t, &meridian, Expect::Meridians(2));
        let oblique = big_plane(p(0.0, 0.0, 0.2), v(0.3, 0.0, 1.0), v(1.0, 0.0, -0.3));
        check("spiric", &t, &oblique, Expect::General);
    }
    // The outer half of the tube: each meridian loses its inner half; across the axis the
    // inner circle goes.
    let outer = torus(
        world(),
        3.0,
        1.0,
        Sense::Forward,
        TURN,
        [-FRAC_PI_2, FRAC_PI_2],
    );
    let meridian = big_plane(p(0.0, 0.0, 0.0), v(0.0, 1.0, 0.0), v(1.0, 0.0, 0.0));
    let got = check("outer meridians", &outer, &meridian, Expect::Meridians(2));
    if let Some(Intersection::Pieces { curves, .. }) = got.as_solved() {
        for c in curves {
            assert!(
                (c.span.max() - c.span.min() - PI).abs() < 1e-9,
                "{:?}",
                c.span
            );
        }
    }
    let across = big_plane(p(0.0, 0.0, 0.5), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
    assert_eq!(
        counts(&check("outer circle", &outer, &across, Expect::Truth)),
        (1, 0)
    );
}

// ---------------------------------------------------------------- random

fn unit(rng: &mut Rng) -> Vec3 {
    loop {
        let q = v(
            rng.next_signed_unit(),
            rng.next_signed_unit(),
            rng.next_signed_unit(),
        );
        if q.length() > 0.2 && q.length() <= 1.0 {
            return *q.normalized().unwrap();
        }
    }
}

fn random_frame(rng: &mut Rng, spread: f64) -> Frame {
    let o = p(
        spread * rng.next_signed_unit(),
        spread * rng.next_signed_unit(),
        spread * rng.next_signed_unit(),
    );
    loop {
        let (z, x) = (unit(rng), unit(rng));
        if z.cross(x).length() > 0.3 {
            return frame(o, z, x);
        }
    }
}

fn random_plane_near(rng: &mut Rng, at: Point3, reach: f64) -> Side {
    let q = at + unit(rng) * (reach * rng.next_f64());
    let n = unit(rng);
    let mut x = unit(rng);
    while n.cross(x).length() < 0.3 {
        x = unit(rng);
    }
    plane_side(
        frame(q, n, x),
        [-30.0 * rng.next_f64() - 1.0, 30.0 * rng.next_f64() + 1.0],
        [-30.0 * rng.next_f64() - 1.0, 30.0 * rng.next_f64() + 1.0],
    )
}

fn random_angles(rng: &mut Rng) -> [f64; 2] {
    if rng.next_f64() < 0.5 {
        TURN
    } else {
        let a = TAU * rng.next_f64();
        [a, a + 0.3 + 5.5 * rng.next_f64()]
    }
}

fn sense_of(rng: &mut Rng) -> Sense {
    if rng.next_f64() < 0.5 {
        Sense::Forward
    } else {
        Sense::Reversed
    }
}

/// Random kinds and planes: each answer checked against the truth; `General` only where
/// the truth is a hyperbola or parabola, or has no closed form.
#[test]
fn random_sections() {
    let mut rng = Rng::new(0x5ec7_1011);
    let mut tally = [0usize; 3];
    let mut branches = 0;
    for i in 0..400 {
        let f = random_frame(&mut rng, 5.0);
        let sense = sense_of(&mut rng);
        let u = random_angles(&mut rng);
        let side = match i % 5 {
            0 => plane_side(f, [-3.0, 4.0], [-2.0, 2.5]),
            1 => cylinder(
                f,
                0.5 + 3.0 * rng.next_f64(),
                sense,
                u,
                [-4.0, 2.0 + 3.0 * rng.next_f64()],
            ),
            2 => {
                let a = 0.1 + 1.2 * rng.next_f64();
                let r = 0.2 + 2.0 * rng.next_f64();
                let apex = -r / fmath::tan(a);
                cone(
                    f,
                    r,
                    a,
                    sense,
                    u,
                    [apex.max(-3.0), 1.0 + 3.0 * rng.next_f64()],
                )
            }
            3 => {
                let a = -FRAC_PI_2 + 1.5 * rng.next_f64();
                sphere(
                    f,
                    0.5 + 3.0 * rng.next_f64(),
                    sense,
                    u,
                    [a, (a + 0.5 + 2.0 * rng.next_f64()).min(FRAC_PI_2)],
                )
            }
            _ => {
                let small = 0.3 + 1.0 * rng.next_f64();
                torus(f, small + 0.5 + 2.0 * rng.next_f64(), small, sense, u, TURN)
            }
        };
        let pl = random_plane_near(&mut rng, f.origin(), 3.0);
        let truth = ssi::intersect(&side.kind, &pl.kind);
        let expect = match (&truth, &side.surface) {
            (Err(_), Surface::Cone(_)) => Expect::Generators,
            (Err(_), _) => Expect::General,
            _ => Expect::Truth,
        };
        let name = format!("random {i}");
        if matches!(side.surface, Surface::Cone(_)) && expect == Expect::Truth {
            // Owed points on the other nappe are outside the kind's domain ("inside" checks
            // `r + k v >= 0`), so the full check applies.
        }
        let got = check(&name, &side, &pl, expect);
        branches += kinds(&got)
            .iter()
            .filter(|k| k.0 == Class::Hyperbola)
            .count();
        tally[match got.as_solved() {
            None => 0,
            Some(Intersection::Empty) => 1,
            _ => 2,
        }] += 1;
    }
    // The mix exercises every outcome.
    assert!(tally.iter().all(|&n| n > 10), "{tally:?}");
    assert!(branches > 15, "{branches} hyperbola pieces");
}
