// Written for solidcorex. Apache-2.0.
//! The sphere pairs and the coaxial pairs (research 63 SS3) against the exact sections
//! (testkit T5, `scx_testkit::ssi`): every answer's circles and points lie on both surfaces
//! and inside both extents, every truth point inside both extents lies on an answer's piece
//! (so an empty answer is checked, not trusted), a crossing circle runs along `n_a x n_b`,
//! both orders give one answer, and a coincident answer is the truth's.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic
)]

mod common;

use core::f64::consts::{FRAC_PI_2, PI, TAU};

use common::*;
use scx_num::frame::{Frame, Sense};
use scx_num::hash::Rng;
use scx_num::units::{Accuracy, Length, Resolution};
use scx_num::vector3d::{Point3, Vec3};
use scx_ssi::analytic::{self, Contact, Intersection};
use scx_surface::Closed;
use scx_testkit::curve_surface as cs;
use scx_testkit::dd::Dd;
use scx_testkit::ssi::{self, Section};

fn solve(a: &Side, b: &Side) -> Closed<Intersection> {
    let acc = Accuracy::new(Length::from_mm(TOL)).unwrap();
    let res = Resolution::new(Length::from_mm(TOL)).unwrap();
    analytic::intersect(
        &a.surface,
        &a.extent,
        &b.surface,
        &b.extent,
        [acc, acc],
        res,
    )
}

/// What the truth owes: its pieces' sample points inside both extents.
fn owed(a: &Side, b: &Side, truth: &[ssi::Piece]) -> Vec<Point3> {
    let mut out = Vec::new();
    for piece in truth {
        let pts: Vec<Point3> = match piece.curve {
            ssi::Curve::Point(q) => vec![tpoint(q)],
            _ => (0..720)
                .map(|i| tpoint(piece.curve.point(Dd::new(TAU * f64::from(i) / 720.0))))
                .collect(),
        };
        out.extend(
            pts.into_iter()
                .filter(|&q| inside(a, q, -MARGIN) && inside(b, q, -MARGIN)),
        );
    }
    out
}

/// The distance from `q` to a truth circle (a tangent answer may stand for two truth
/// circles within the tolerance of each other).
fn near_circle(c: &ssi::Curve, q: Point3) -> f64 {
    let ssi::Curve::Circle { centre, a, b } = *c else {
        return f64::INFINITY;
    };
    let (o, a, b) = (tpoint(centre), tvec(a), tvec(b));
    let n = *a.cross(b).normalized().unwrap();
    let d = q - o;
    let up = d.dot(n);
    scx_num::fmath::hypot(up, (d - n * up).length() - a.length())
}

/// What a case's answer is, beyond the truth's checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expect {
    /// Checked against the truth only.
    Truth,
    /// Coincident surfaces whose extents are apart: `Empty`.
    Apart,
    /// Coincident surfaces (two nappes of one double cone, as the truth has cones) whose
    /// extents meet only in pieces: on both surfaces and inside both extents only.
    Nappes,
}

/// Checks `a` against `b` one way.
fn check_one(name: &str, a: &Side, b: &Side, expect: Expect) -> Closed<Intersection> {
    let got = solve(a, b);
    let truth = ssi::intersect(&a.kind, &b.kind);
    let Some(answer) = got.as_solved() else {
        panic!("{name}: General, truth {truth:?}");
    };
    let (curves, points) = match answer {
        Intersection::Coincident(pieces) => {
            assert_eq!(truth, Ok(Section::Coincident), "{name}");
            assert_eq!(expect, Expect::Truth, "{name}: coincident, expected apart");
            judge_overlap(name, a, b, pieces);
            return got;
        }
        Intersection::Empty => (&[][..], &[][..]),
        Intersection::Pieces { curves, points } => (&curves[..], &points[..]),
        _ => unreachable!(),
    };
    let tpieces: Vec<ssi::Piece> = match (&truth, expect) {
        (Ok(Section::Coincident), Expect::Apart) => {
            assert!(curves.is_empty() && points.is_empty(), "{name}: {answer:?}");
            // No piece: no sample of the first's extent well inside the second's.
            judge_overlap(name, a, b, &[]);
            return got;
        }
        (Ok(Section::Coincident), Expect::Nappes) => Vec::new(),
        (Ok(Section::Curves(t)), Expect::Truth) => t.clone(),
        (Ok(Section::Empty), Expect::Truth) => Vec::new(),
        (other, _) => panic!("{name}: truth {other:?}, got {answer:?}"),
    };
    // On both surfaces: a crossing to rounding, a touch within the tolerance (63 SS4).
    let on = |q: Point3, contact: Contact| {
        let (da, db) = (
            cs::distance(&a.kind, arr(q)).unwrap(),
            cs::distance(&b.kind, arr(q)).unwrap(),
        );
        let tol = match contact {
            Contact::Transversal => ON * (1.0 + q.to_vec().length()),
            Contact::Tangent => touch_tol(q),
        };
        assert!(
            da <= tol && db <= tol,
            "{name}: {q:?} off the surfaces by {da:e}, {db:e}"
        );
        assert!(
            inside(a, q, 1e-7) && inside(b, q, 1e-7),
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
        assert_eq!(class(&c.curve), Class::Circle, "{name}: {c:?}");
        // A touch may stand for crossings within the tolerance (the owed points judge it).
        assert!(
            expect == Expect::Nappes
                || c.contact == Contact::Tangent
                || tpieces
                    .iter()
                    .any(|q| truth_class(&q.curve) == Class::Circle
                        && (contact(q.contact) == c.contact
                            || c.contact == Contact::Tangent
                                && near_circle(&q.curve, c.curve.point(t0)) <= 1e-7)),
            "{name}: no truth circle of contact {:?}: {tpieces:?}",
            c.contact
        );
        if c.contact == Contact::Transversal {
            let tm = 0.5 * (t0 + t1);
            let q = c.curve.point(tm);
            let h = 1e-6 * (1.0 + (t1 - t0));
            let tangent = c.curve.point(tm + h) - c.curve.point(tm - h);
            if let (Some(na), Some(nb)) = (normal_at(a, q), normal_at(b, q)) {
                let want = na.cross(nb);
                if want.length() > 1e-6 {
                    assert!(tangent.dot(want) > 0.0, "{name}: runs against n_a x n_b");
                }
            }
        }
    }
    for q in points {
        on(q.point, q.contact);
        assert_eq!(q.contact, Contact::Tangent, "{name}: {q:?}");
    }
    for q in owed(a, b, &tpieces) {
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
                || absorbed(a, b, curves, points, q),
            "{name}: truth point {q:?} missed by {}; got {got:?}; truth {tpieces:?}",
            near_curve.min(near_point),
        );
    }
    judge_depth(name, a, b, curves);
    got
}

/// The shape of an answer: its kind and its pieces' counts.
fn shape(got: &Closed<Intersection>) -> (u8, usize, usize) {
    match got.as_solved() {
        None => (0, 0, 0),
        Some(Intersection::Empty) => (1, 0, 0),
        Some(Intersection::Coincident(p)) => (2, p.len(), 0),
        Some(Intersection::Pieces { curves, points }) => (3, curves.len(), points.len()),
        Some(_) => unreachable!(),
    }
}

/// Checks the pair both ways (one answer, the pieces the same) and returns `a` against `b`.
fn check(name: &str, a: &Side, b: &Side, expect: Expect) -> Closed<Intersection> {
    let got = check_one(name, a, b, expect);
    let back = check_one(&format!("{name} (swapped)"), b, a, expect);
    assert_eq!(
        shape(&got),
        shape(&back),
        "{name}: {got:?} against {back:?}"
    );
    got
}

fn counts(got: &Closed<Intersection>) -> (usize, usize) {
    let (_, c, p) = shape(got);
    (c, p)
}

fn is_empty(got: &Closed<Intersection>) -> bool {
    matches!(got.as_solved(), Some(Intersection::Empty))
}

fn is_coincident(got: &Closed<Intersection>) -> bool {
    matches!(got.as_solved(), Some(Intersection::Coincident(_)))
}

fn at(o: Point3, z: Vec3, x: Vec3) -> Frame {
    frame(o, z, x)
}

const ALL: [f64; 2] = [-FRAC_PI_2, FRAC_PI_2];

// ---------------------------------------------------------------- spheres

#[test]
fn sphere_sphere() {
    let a = sphere(world(), 2.0, Sense::Forward, TURN, ALL);
    let off = |o: Point3, r: f64| {
        sphere(
            at(o, v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
            r,
            Sense::Forward,
            TURN,
            ALL,
        )
    };
    // Crossing: one circle, along every line of centres.
    for (i, o) in [
        p(3.0, 0.0, 0.0),
        p(0.0, -2.5, 0.0),
        p(1.0, 1.5, -2.0),
        p(0.0, 0.0, 1.0),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            counts(&check(
                &format!("cross {i}"),
                &a,
                &off(o, 2.0),
                Expect::Truth
            )),
            (1, 0)
        );
    }
    // Touching outside and inside: a point.
    assert_eq!(
        counts(&check(
            "touch out",
            &a,
            &off(p(0.0, 3.0, 0.0), 1.0),
            Expect::Truth
        )),
        (0, 1)
    );
    assert_eq!(
        counts(&check(
            "touch in",
            &a,
            &off(p(0.0, 0.0, 1.0), 1.0),
            Expect::Truth
        )),
        (0, 1)
    );
    // Apart, nested, concentric.
    assert!(is_empty(&check(
        "apart",
        &a,
        &off(p(5.0, 0.0, 0.0), 1.0),
        Expect::Truth
    )));
    assert!(is_empty(&check(
        "nested",
        &a,
        &off(p(0.5, 0.0, 0.0), 1.0),
        Expect::Truth
    )));
    assert!(is_empty(&check(
        "concentric",
        &a,
        &off(p(0.0, 0.0, 0.0), 1.0),
        Expect::Truth
    )));
    assert!(is_coincident(&check(
        "same",
        &a,
        &sphere(
            at(p(0.0, 0.0, 0.0), v(0.0, 0.0, -1.0), v(0.0, 1.0, 0.0)),
            2.0,
            Sense::Reversed,
            TURN,
            ALL
        ),
        Expect::Truth
    )));
    // Clipped: a cap of the first misses the circle; half a turn keeps half of it.
    let cap = sphere(world(), 2.0, Sense::Forward, TURN, [1.0, FRAC_PI_2]);
    assert!(is_empty(&check(
        "cap",
        &cap,
        &off(p(3.0, 0.0, 0.0), 2.0),
        Expect::Truth
    )));
    let half = sphere(world(), 2.0, Sense::Forward, [0.0, PI], ALL);
    assert_eq!(
        counts(&check(
            "half",
            &half,
            &off(p(0.0, 0.0, 1.0), 2.0),
            Expect::Truth
        )),
        (1, 0)
    );
}

// ---------------------------------------------------------------- coaxial

#[test]
fn cylinder_pairs() {
    let c = cylinder(world(), 1.0, Sense::Forward, TURN, [-2.0, 2.0]);
    // A sphere on the axis: two circles, a touch circle, or none.
    let s = |z: f64, r: f64| {
        sphere(
            at(p(0.0, 0.0, z), v(0.0, 0.0, -1.0), v(0.0, 1.0, 0.0)),
            r,
            Sense::Forward,
            TURN,
            ALL,
        )
    };
    assert_eq!(
        counts(&check("sphere two", &c, &s(0.5, 1.25), Expect::Truth)),
        (2, 0)
    );
    let got = check("sphere touch", &c, &s(0.5, 1.0), Expect::Truth);
    assert_eq!(counts(&got), (1, 0));
    let Some(Intersection::Pieces { curves, .. }) = got.as_solved() else {
        unreachable!()
    };
    assert_eq!(curves[0].contact, Contact::Tangent);
    assert!(is_empty(&check(
        "sphere inside",
        &c,
        &s(0.0, 0.5),
        Expect::Truth
    )));
    // A sphere past the cylinder's height: its circles are clipped away.
    assert_eq!(
        counts(&check("sphere high", &c, &s(1.5, 1.25), Expect::Truth)),
        (1, 0)
    );
    // Two cylinders: one surface, or nested.
    let same = cylinder(
        at(p(0.0, 0.0, 1.0), v(0.0, 0.0, -1.0), v(0.0, 1.0, 0.0)),
        1.0,
        Sense::Reversed,
        TURN,
        [-1.0, 4.0],
    );
    assert!(is_coincident(&check("same", &c, &same, Expect::Truth)));
    let above = cylinder(
        at(p(0.0, 0.0, 3.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
        1.0,
        Sense::Forward,
        TURN,
        [0.0, 1.0],
    );
    assert!(is_empty(&check("same above", &c, &above, Expect::Apart)));
    let turned = cylinder(world(), 1.0, Sense::Forward, [PI, 1.5 * PI], [-2.0, 2.0]);
    // A quarter about the world's half turn, from a turned frame's x.
    let quarter = cylinder(
        frame(p(0.0, 0.0, 0.0), v(0.0, 0.0, 1.0), v(0.0, 1.0, 0.0)),
        1.0,
        Sense::Forward,
        [0.25 * PI, 0.75 * PI],
        [-2.0, 2.0],
    );
    assert!(is_coincident(&check(
        "arcs meet",
        &turned,
        &quarter,
        Expect::Truth
    )));
    // A frame turning the other way: its `[0.1 pi, 0.4 pi]` is the world's
    // `[1.6 pi, 1.9 pi]`, past the first's end; its `[0.6 pi, 0.9 pi]` overlaps.
    let mirrored = |u: [f64; 2]| {
        cylinder(
            frame(p(0.0, 0.0, 0.0), v(0.0, 0.0, -1.0), v(1.0, 0.0, 0.0)),
            1.0,
            Sense::Forward,
            u,
            [-2.0, 2.0],
        )
    };
    assert!(is_coincident(&check(
        "mirrored arcs meet",
        &turned,
        &mirrored([0.6 * PI, 0.9 * PI]),
        Expect::Truth
    )));
    let first = mirrored([0.1 * PI, 0.4 * PI]);
    assert!(is_empty(&check(
        "arcs apart",
        &turned,
        &first,
        Expect::Apart
    )));
    let wide = cylinder(world(), 2.0, Sense::Forward, TURN, [-2.0, 2.0]);
    assert!(is_empty(&check("nested", &c, &wide, Expect::Truth)));
    // A cone on the axis: a circle where its radius is the cylinder's.
    let k = cone(
        at(p(0.0, 0.0, -1.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
        0.5,
        0.4,
        Sense::Forward,
        TURN,
        [-1.0, 4.0],
    );
    assert_eq!(counts(&check("cone", &c, &k, Expect::Truth)), (1, 0));
    let down = cone(
        at(p(0.0, 0.0, 1.0), v(0.0, 0.0, -1.0), v(0.0, 1.0, 0.0)),
        0.5,
        0.4,
        Sense::Forward,
        TURN,
        [-1.0, 4.0],
    );
    assert_eq!(
        counts(&check("cone down", &c, &down, Expect::Truth)),
        (1, 0)
    );
    // A torus about the axis: tangent circles where the tube meets the cylinder.
    let t = |big: f64| torus(world(), big, 0.5, Sense::Forward, TURN, TURN);
    assert_eq!(
        counts(&check("torus two", &c, &t(1.25), Expect::Truth)),
        (2, 0)
    );
    assert_eq!(
        counts(&check("torus touch", &c, &t(1.5), Expect::Truth)),
        (1, 0)
    );
    assert!(is_empty(&check("torus out", &c, &t(2.0), Expect::Truth)));
}

#[test]
fn cone_pairs() {
    let k = cone(world(), 1.0, 0.5, Sense::Forward, TURN, [-1.5, 3.0]);
    let (_, cn) = scx_num::fmath::sin_cos(0.5);
    // Two cones from one apex (exact: radius zero there), one up and one down: only the
    // apex, a point on the axis.
    let tip = |z: f64, up: f64, hv: [f64; 2]| {
        cone(
            at(p(0.0, 0.0, z), v(0.0, 0.0, up), v(1.0, 0.0, 0.0)),
            0.0,
            0.5,
            Sense::Forward,
            TURN,
            hv,
        )
    };
    let up = tip(0.0, 1.0, [0.0, 3.0]);
    assert_eq!(
        counts(&check(
            "apexes",
            &up,
            &tip(0.0, -1.0, [0.0, 3.0]),
            Expect::Nappes
        )),
        (0, 1)
    );
    // The lower moved up: the nappes cross in a circle.
    assert_eq!(
        counts(&check(
            "crossed",
            &up,
            &tip(2.0, -1.0, [0.0, 3.0]),
            Expect::Truth
        )),
        (1, 0)
    );
    // Moved down: they cross behind both apexes, nothing.
    assert!(is_empty(&check(
        "behind",
        &up,
        &tip(-1.0, -1.0, [0.0, 3.0]),
        Expect::Truth
    )));
    // Two from one apex at different angles: the apex.
    let narrow = cone(world(), 0.0, 0.25, Sense::Reversed, TURN, [0.0, 3.0]);
    assert_eq!(
        counts(&check("nested apexes", &up, &narrow, Expect::Truth)),
        (0, 1)
    );
    // Same cone, a turned frame reversed with its sense (z times the sense is the first's):
    // one surface.
    let same = cone(
        at(p(0.0, 0.0, 0.0), v(0.0, 0.0, -1.0), v(0.0, 1.0, 0.0)),
        1.0,
        0.5,
        Sense::Reversed,
        TURN,
        [0.0, 2.0],
    );
    assert!(is_coincident(&check("same", &k, &same, Expect::Truth)));
    // A sphere on the axis.
    let s = |z: f64, r: f64| {
        sphere(
            at(p(0.0, 0.0, z), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
            r,
            Sense::Forward,
            TURN,
            ALL,
        )
    };
    assert_eq!(
        counts(&check("sphere", &k, &s(0.0, 1.5), Expect::Truth)),
        (2, 0)
    );
    // Inscribed (the meridian line's distance from the centre): tangent along a circle.
    let got = check("sphere inscribed", &k, &s(0.0, cn), Expect::Truth);
    assert_eq!(counts(&got), (1, 0));
    assert!(is_empty(&check(
        "sphere inside",
        &k,
        &s(0.0, 0.5),
        Expect::Truth
    )));
    // A torus about the axis.
    let t = torus(
        at(p(0.0, 0.0, 1.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
        1.5,
        0.5,
        Sense::Forward,
        TURN,
        TURN,
    );
    assert_eq!(counts(&check("torus", &k, &t, Expect::Truth)), (2, 0));
}

#[test]
fn torus_pairs() {
    let t = torus(world(), 2.0, 0.5, Sense::Forward, TURN, TURN);
    let s = |z: f64, r: f64| {
        sphere(
            at(p(0.0, 0.0, z), v(0.0, 0.0, 1.0), v(0.0, -1.0, 0.0)),
            r,
            Sense::Forward,
            TURN,
            ALL,
        )
    };
    assert_eq!(
        counts(&check("sphere", &t, &s(0.0, 2.0), Expect::Truth)),
        (2, 0)
    );
    assert_eq!(
        counts(&check("sphere touch", &t, &s(0.0, 1.5), Expect::Truth)),
        (1, 0)
    );
    assert!(is_empty(&check(
        "sphere in hole",
        &t,
        &s(0.0, 1.0),
        Expect::Truth
    )));
    assert!(is_empty(&check(
        "sphere above",
        &t,
        &s(5.0, 1.0),
        Expect::Truth
    )));
    let u = |z: f64, big: f64, small: f64| {
        torus(
            at(p(0.0, 0.0, z), v(0.0, 0.0, -1.0), v(0.0, 1.0, 0.0)),
            big,
            small,
            Sense::Forward,
            TURN,
            TURN,
        )
    };
    assert_eq!(
        counts(&check("stacked", &t, &u(0.5, 2.0, 0.5), Expect::Truth)),
        (2, 0)
    );
    assert_eq!(
        counts(&check(
            "stacked touch",
            &t,
            &u(1.0, 2.0, 0.5),
            Expect::Truth
        )),
        (1, 0)
    );
    assert!(is_empty(&check(
        "stacked apart",
        &t,
        &u(1.5, 2.0, 0.5),
        Expect::Truth
    )));
    assert!(is_coincident(&check(
        "same",
        &t,
        &u(0.0, 2.0, 0.5),
        Expect::Truth
    )));
    assert!(is_empty(&check(
        "inner tube",
        &t,
        &u(0.0, 2.0, 0.25),
        Expect::Truth
    )));
    // Half the tube (its outer side) against a sphere through the inner side.
    let outer = torus(
        world(),
        2.0,
        0.5,
        Sense::Forward,
        TURN,
        [-FRAC_PI_2, FRAC_PI_2],
    );
    assert!(is_empty(&check(
        "outer half",
        &outer,
        &s(0.0, 1.75),
        Expect::Truth
    )));
}

// ---------------------------------------------------------------- depth

/// Whether every piece of an answer touches (none crosses).
fn touches(got: &Closed<Intersection>) -> bool {
    match got.as_solved() {
        Some(Intersection::Pieces { curves, points }) => {
            curves.iter().all(|c| c.contact == Contact::Tangent)
                && points.iter().all(|q| q.contact == Contact::Tangent)
        }
        _ => true,
    }
}

/// The depth rule (63 SS4): meridians within the tolerance of each other between two
/// crossings touch once, at the deepest point; past it they cross twice (or miss).
#[test]
fn depth_rule() {
    let depths = [0.5 * TOL, 0.0, -0.5 * TOL, 4.0 * TOL, -4.0 * TOL];
    // (circles, points) for each depth: a touch, a touch, a touch, two crossings, none.
    let want = |touch: (usize, usize)| [touch, touch, touch, (2, 0), (0, 0)];
    let s = sphere(world(), 2.0, Sense::Forward, TURN, ALL);
    for (n, (&d, w)) in depths.iter().zip(want((1, 0))).enumerate() {
        // A cylinder inside the sphere by `d` at the equator.
        let c = cylinder(world(), 2.0 - d, Sense::Forward, TURN, [-1.0, 1.0]);
        let got = check(&format!("sphere cylinder {n}"), &s, &c, Expect::Truth);
        assert_eq!(counts(&got), w, "sphere cylinder {n}: {got:?}");
        assert_eq!(touches(&got), n != 3, "sphere cylinder {n}: {got:?}");
    }
    for (n, (&d, w)) in depths.iter().zip(want((0, 1))).enumerate() {
        // Two spheres overlapping by `d`: two crossings would be one circle; a touch a point.
        let b = sphere(
            at(p(0.0, 0.0, 3.0 - d), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
            1.0,
            Sense::Forward,
            TURN,
            ALL,
        );
        let got = check(&format!("spheres {n}"), &s, &b, Expect::Truth);
        let w = if n == 3 { (1, 0) } else { w };
        assert_eq!(counts(&got), w, "spheres {n}: {got:?}");
        assert_eq!(touches(&got), n != 3, "spheres {n}: {got:?}");
    }
    let t = torus(world(), 2.0, 0.5, Sense::Forward, TURN, TURN);
    for (n, (&d, w)) in depths.iter().zip(want((1, 0))).enumerate() {
        // A cylinder in the tube's outside by `d`.
        let c = cylinder(world(), 2.5 - d, Sense::Forward, TURN, [-1.0, 1.0]);
        let got = check(&format!("torus cylinder {n}"), &t, &c, Expect::Truth);
        assert_eq!(counts(&got), w, "torus cylinder {n}: {got:?}");
        assert_eq!(touches(&got), n != 3, "torus cylinder {n}: {got:?}");
    }
}

// ---------------------------------------------------------------- overlaps

fn pieces(got: &Closed<Intersection>) -> Vec<analytic::Overlap> {
    match got.as_solved() {
        Some(Intersection::Coincident(p)) => p.clone(),
        other => panic!("not coincident: {other:?}"),
    }
}

/// Whether a piece has no width (a touch within the tolerance).
fn thin(o: &analytic::Overlap) -> bool {
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for q in &o.on_a {
        lo = [lo[0].min(q.x), lo[1].min(q.y)];
        hi = [hi[0].max(q.x), hi[1].max(q.y)];
    }
    hi[0] - lo[0] <= 1e-12 || hi[1] - lo[1] <= 1e-12
}

/// Coincident surfaces of revolution (63 SS4): the overlap of their patches, judged by
/// samples of the first's extent; arcs that wrap past the other's start give two pieces;
/// patches that only touch give one piece of no width; a sphere's frame tilted against the
/// other's is `General`.
#[test]
fn overlaps() {
    let c = cylinder(
        world(),
        1.0,
        Sense::Forward,
        [0.25 * PI, 1.75 * PI],
        [-2.0, 2.0],
    );
    // Through the first's gap at `0`: `[1.5 pi, 2.5 pi]`, from a frame turned half a turn.
    let wrap = cylinder(
        frame(p(0.0, 0.0, 1.0), v(0.0, 0.0, 1.0), v(-1.0, 0.0, 0.0)),
        1.0,
        Sense::Reversed,
        [0.5 * PI, 1.5 * PI],
        [-1.0, 3.0],
    );
    let got = check("wrapped", &c, &wrap, Expect::Truth);
    assert_eq!(pieces(&got).len(), 2, "{got:?}");
    // Heights end to end, the axis reversed.
    let stacked = cylinder(
        frame(p(0.0, 0.0, 4.0), v(0.0, 0.0, -1.0), v(1.0, 0.0, 0.0)),
        1.0,
        Sense::Forward,
        TURN,
        [0.0, 2.0],
    );
    let got = check("stacked", &c, &stacked, Expect::Truth);
    let p0 = pieces(&got);
    assert!(!p0.is_empty() && p0.iter().all(thin), "{got:?}");
    // Arcs end to end.
    let after = cylinder(
        world(),
        1.0,
        Sense::Forward,
        [1.75 * PI, 1.95 * PI],
        [-1.0, 1.0],
    );
    let got = check("arcs end to end", &c, &after, Expect::Truth);
    let p1 = pieces(&got);
    assert!(p1.len() == 1 && thin(&p1[0]), "{got:?}");
    // A cone and the same cone from a turned frame (a moved origin's radius is not exact
    // in doubles).
    let k = cone(world(), 1.0, 0.5, Sense::Forward, TURN, [0.0, 2.0]);
    let other = cone(
        frame(p(0.0, 0.0, 0.0), v(0.0, 0.0, 1.0), v(0.0, 1.0, 0.0)),
        1.0,
        0.5,
        Sense::Forward,
        [0.5 * PI, 1.5 * PI],
        [-0.5, 3.0],
    );
    let got = check("cone part", &k, &other, Expect::Truth);
    assert_eq!(pieces(&got).len(), 1, "{got:?}");
    // Tori: tube arcs that overlap from a frame turned upside down.
    let t = torus(world(), 2.0, 0.5, Sense::Forward, TURN, [0.0, PI]);
    let flipped = torus(
        frame(p(0.0, 0.0, 0.0), v(0.0, 0.0, -1.0), v(1.0, 0.0, 0.0)),
        2.0,
        0.5,
        Sense::Forward,
        [0.0, PI],
        [0.5 * PI, 1.5 * PI],
    );
    let got = check("tube arcs", &t, &flipped, Expect::Truth);
    assert!(!pieces(&got).is_empty(), "{got:?}");
    // Spheres: caps that overlap, about one axis either way.
    let cap = sphere(world(), 2.0, Sense::Forward, [0.0, PI], [-0.5, 1.0]);
    let under = sphere(
        frame(p(0.0, 0.0, 0.0), v(0.0, 0.0, -1.0), v(0.0, 1.0, 0.0)),
        2.0,
        Sense::Reversed,
        TURN,
        [0.0, FRAC_PI_2],
    );
    // Two pieces: the second's seam (its frame's x, the world's y) cuts the first's arc.
    let got = check("caps", &cap, &under, Expect::Truth);
    assert_eq!(pieces(&got).len(), 2, "{got:?}");
    // The same sphere from a frame tilted off the first's axis: no rectangle in either's uv.
    let tilted = sphere(
        frame(p(0.0, 0.0, 0.0), v(0.0, 0.3, 1.0), v(1.0, 0.0, 0.0)),
        2.0,
        Sense::Forward,
        TURN,
        [-0.5, 0.5],
    );
    assert!(solve(&cap, &tilted).is_general());
}

// ---------------------------------------------------------------- near one axis

/// An axis moved by much less than the tolerance over the extent is the same axis; moved by
/// more, the pair is not on one axis here and is `General`.
#[test]
fn near_axes() {
    let c = cylinder(world(), 1.0, Sense::Forward, TURN, [-2.0, 2.0]);
    let tilted = |w: f64| {
        cylinder(
            frame(p(w, 0.0, 0.0), v(w, 0.0, 1.0), v(1.0, 0.0, 0.0)),
            1.0,
            Sense::Forward,
            TURN,
            [-1.0, 1.0],
        )
    };
    assert!(is_coincident(&solve(&c, &tilted(1e-12))));
    assert!(solve(&c, &tilted(1e-3)).is_general());
    let s = |x: f64| {
        sphere(
            frame(p(x, 0.0, 0.5), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)),
            1.25,
            Sense::Forward,
            TURN,
            ALL,
        )
    };
    assert_eq!(counts(&solve(&c, &s(1e-12))), (2, 0));
    assert!(solve(&c, &s(1e-3)).is_general());
    // Square axes are never one.
    let across = cylinder(
        frame(p(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0)),
        1.0,
        Sense::Forward,
        TURN,
        [-2.0, 2.0],
    );
    assert!(solve(&c, &across).is_general());
}

// ---------------------------------------------------------------- random

/// A frame with its z along a signed world axis and its x along another: exact for the
/// truth.
fn axis_frame(rng: &mut Rng, o: Point3) -> Frame {
    let axes = [v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0), v(0.0, 0.0, 1.0)];
    let i = (rng.next_u64() % 3) as usize;
    let j = (i + 1 + (rng.next_u64() % 2) as usize) % 3;
    let sz = if rng.next_f64() < 0.5 { 1.0 } else { -1.0 };
    let sx = if rng.next_f64() < 0.5 { 1.0 } else { -1.0 };
    frame(o, axes[i] * sz, axes[j] * sx)
}

/// A dyadic in `[-m, m]` on a grid of eighths: exact sums.
fn grid(rng: &mut Rng, m: f64) -> f64 {
    (rng.next_signed_unit() * m * 8.0).round() / 8.0
}

fn positive(rng: &mut Rng, lo: f64, hi: f64) -> f64 {
    ((lo + (hi - lo) * rng.next_f64()) * 8.0).round().max(1.0) / 8.0
}

fn sense(rng: &mut Rng) -> Sense {
    if rng.next_f64() < 0.5 {
        Sense::Forward
    } else {
        Sense::Reversed
    }
}

fn angles(rng: &mut Rng) -> [f64; 2] {
    if rng.next_f64() < 0.5 {
        TURN
    } else {
        let a = TAU * rng.next_f64();
        [a, a + 0.5 + 5.0 * rng.next_f64()]
    }
}

/// A random revolution kind with its frame's z along `z` (the same line for every kind of a
/// pair), its origin at `o`.
fn revolution(rng: &mut Rng, kind: u64, f: Frame) -> Side {
    // Sizes from short lists, so that pairs meet, touch and coincide often.
    let mut pick = |xs: &[f64]| xs[(rng.next_u64() % xs.len() as u64) as usize];
    let r = pick(&[0.5, 1.0, 1.5, 2.0]);
    let a = pick(&[0.25, 0.5, 0.75]);
    let (big, small) = (pick(&[1.5, 2.0]), pick(&[0.25, 0.5, 1.0]));
    let s = sense(rng);
    let h0 = grid(rng, 2.0);
    let heights = [h0, h0 + positive(rng, 0.5, 4.0)];
    match kind {
        0 => cylinder(f, r, s, angles(rng), heights),
        1 => cone(f, r, a, s, angles(rng), heights),
        2 => {
            let lat = if rng.next_f64() < 0.5 {
                ALL
            } else {
                let l = -FRAC_PI_2 + PI * rng.next_f64();
                [l, (l + 0.5 + 2.0 * rng.next_f64()).min(FRAC_PI_2)]
            };
            sphere(f, r, s, angles(rng), lat)
        }
        _ => torus(f, big, small, s, angles(rng), angles(rng)),
    }
}

/// Random coaxial pairs of every kind, each frame along one signed world axis, the second's
/// origin moved along it; and random sphere pairs anywhere.
#[test]
fn random_pairs() {
    let mut rng = Rng::new(0x55_3c0a);
    let (mut solved, mut skipped) = (0, 0);
    let mut seen = [0usize; 4];
    for n in 0..800 {
        let o = p(
            grid(&mut rng, 2.0),
            grid(&mut rng, 2.0),
            grid(&mut rng, 2.0),
        );
        let fa = axis_frame(&mut rng, o);
        let (ka, kb) = (rng.next_u64() % 4, rng.next_u64() % 4);
        let a = revolution(&mut rng, ka, fa);
        let ob = if ka == 2 && kb == 2 {
            o + v(
                grid(&mut rng, 2.0),
                grid(&mut rng, 2.0),
                grid(&mut rng, 2.0),
            )
        } else {
            // On the first's origin a quarter of the time, so that cones and tori coincide.
            let h = if rng.next_f64() < 0.25 {
                0.0
            } else {
                grid(&mut rng, 2.0)
            };
            o + *fa.z() * h
        };
        // The second's z along the first's axis, either way, its x anywhere square to it.
        let fb = loop {
            let f = axis_frame(&mut rng, ob);
            if f.z().cross(*fa.z()).length() == 0.0 {
                break f;
            }
        };
        let b = revolution(&mut rng, kb, fb);
        let truth = ssi::intersect(&a.kind, &b.kind);
        if matches!(truth, Err(ssi::Error::Undecided)) {
            skipped += 1;
            continue;
        }
        let name = format!("random {n} ({ka}, {kb})");
        let expect = match (&truth, solve(&a, &b).as_solved()) {
            (Ok(Section::Coincident), Some(Intersection::Empty)) => Expect::Apart,
            (Ok(Section::Coincident), Some(Intersection::Pieces { .. })) => Expect::Nappes,
            _ => Expect::Truth,
        };
        let got = check(&name, &a, &b, expect);
        let (k, _, _) = shape(&got);
        seen[usize::from(k)] += 1;

        solved += 1;
    }
    assert!(skipped * 20 < solved, "{skipped} undecided of {solved}");
    // Every answer shape is met.
    assert!(seen[1] > 50 && seen[3] > 50 && seen[2] > 0, "{seen:?}");
}
