// Written for solidcorex. Apache-2.0.
//! The field path (73 §4.5, row SM6a): an analytic side's implicit form over the other
//! side's Bernstein net, traced in that side's uv.
//!
//! - the field's coefficients are within their bound of the exact rational computation
//!   (`scx_testkit::rat`), for every kind's form over plane and cylinder nets;
//! - the traced branches of SM0 truths (`scx_testkit::ssi_truth`) lie on both surfaces
//!   within their certificates, and every truth sample lies near a traced chord;
//! - a coincident patch (all-zero field) is told apart from a coplanar neighbour beside it
//!   and from a parallel plane just beyond the coincidence, and agrees with the closed forms;
//! - the answer is the same bits every run, and any budget that suffices gives it.

#![allow(clippy::unwrap_used, clippy::panic, clippy::float_cmp)]

use core::f64::consts::{FRAC_PI_2, TAU};
use std::sync::Arc;

use scx_num::budget::{Budget, Exhausted, Stop};
use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Accuracy, Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_surface::{Closed, Cone, Cylinder, Plane, Sphere, Surface, Torus};
use scx_testkit::rat::Rat;
use scx_testkit::ssi_truth::{
    Surf, Truth, cylinders_parallel, cylinders_perpendicular, sphere_sphere, torus_plane_parallel,
};

use super::{nurbs_sphere_and_plane, tpipe};
use crate::SsiError;
use crate::analytic::{self, Intersection, Overlap};
use crate::general::implicit::Implicit;
use crate::general::net::Net;
use crate::general::{NodeKind, OnSurface, Traced, intersect};

/// The accuracy the traces run at: coarse, so a debug run stays quick.
const ACC: f64 = 1e-6;

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn acc() -> [Accuracy; 2] {
    let a = Accuracy::new(mm(ACC)).unwrap();
    [a, a]
}

fn res() -> Resolution {
    Resolution::new(mm(1e-9)).unwrap()
}

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

/// A frame turned off every axis: `z` along `(1, 2, 2) / 3`, `x` along `(2, 1, -2) / 3`.
fn tilted(o: [f64; 3]) -> Frame {
    let t = 1.0 / 3.0;
    frame(o, [t, 2.0 * t, 2.0 * t], [2.0 * t, t, -2.0 * t])
}

fn rect(u0: f64, v0: f64, u1: f64, v1: f64) -> Extent2d {
    Extent2d::from_bounds(u0, v0, u1, v1).unwrap()
}

fn nurbs(s: &Surface, e: Extent2d) -> (Surface, Extent2d) {
    (Surface::Nurbs(Arc::new(s.to_nurbs(&e).unwrap())), e)
}

fn trace(pair: &[(Surface, Extent2d); 2], budget: &mut Budget) -> Result<Traced, SsiError> {
    let [(a, ae), (b, be)] = pair;
    intersect(a, ae, b, be, acc(), res(), budget, &Stop::new())
}

fn p3(p: [f64; 3]) -> Point3 {
    let [x, y, z] = p;
    Point3::new(x, y, z)
}

fn dist(a: Point3, b: Point3) -> f64 {
    (a - b).length()
}

fn point_segment(p: Point3, a: Point3, b: Point3) -> f64 {
    let ab = b - a;
    let l2 = ab.dot(ab);
    let t = if l2 > 0.0 {
        ((p - a).dot(ab) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    dist(p, a + ab * t)
}

// --- the Bernstein field against exact rationals ----------------------------------------

fn rat(x: f64) -> Rat {
    Rat::from_f64(x).unwrap()
}

/// `sum c_i B_i(t)` exactly (de Casteljau in rationals).
fn casteljau(c: &[Rat], t: &Rat) -> Rat {
    let one_minus = &Rat::one() - t;
    let mut c = c.to_vec();
    for k in (1..c.len()).rev() {
        for i in 0..k {
            c[i] = &(&c[i] * &one_minus) + &(&c[i + 1] * t);
        }
    }
    c.swap_remove(0)
}

/// A tensor polynomial's value at `(s, t)`, exactly: its coefficients `c[i (dv + 1) + j]`
/// read as exact rationals.
fn exact_value(f: &scx_algo::bernstein::Bernstein2, s: &Rat, t: &Rat) -> Rat {
    let (_, dv) = f.degrees();
    let rows: Vec<Rat> = f
        .coefficients()
        .chunks(dv + 1)
        .map(|row| casteljau(&row.iter().map(|&x| rat(x)).collect::<Vec<_>>(), t))
        .collect();
    casteljau(&rows, s)
}

/// The kinds' shapes, as the reference reads them.
#[derive(Clone, Copy)]
enum Shape {
    Plane,
    Cylinder(f64),
    Cone(f64, f64),
    Sphere(f64),
    Torus(f64, f64),
}

/// `W^k f0(M (H - O W) / W)` at `(s, t)` over `net`, in exact rationals from the stored
/// doubles: the frame's axes (z times the sense), its origin and the shape's constants.
fn exact_field(net: &Net, f: &Frame, sign: f64, shape: Shape, s: &Rat, t: &Rat) -> Rat {
    let w = exact_value(&net.w, s, t);
    let o = f.origin();
    let d: Vec<Rat> = net
        .h
        .iter()
        .zip([o.x, o.y, o.z])
        .map(|(h, o)| &exact_value(h, s, t) - &(&rat(o) * &w))
        .collect();
    let (x, y, z) = (*f.x(), *f.y(), *f.z() * sign);
    let local = |m: Vec3| &(&(&rat(m.x) * &d[0]) + &(&rat(m.y) * &d[1])) + &(&rat(m.z) * &d[2]);
    let (lx, ly, lz) = (local(x), local(y), local(z));
    let sq = |a: &Rat| a * a;
    let radial = &sq(&lx) + &sq(&ly);
    let w2 = sq(&w);
    match shape {
        Shape::Plane => lz,
        Shape::Cylinder(r) => &radial - &(&sq(&rat(r)) * &w2),
        Shape::Cone(r, k) => &radial - &sq(&(&(&rat(r) * &w) + &(&rat(k) * &lz))),
        Shape::Sphere(r) => &(&radial + &sq(&lz)) - &(&sq(&rat(r)) * &w2),
        Shape::Torus(big, small) => {
            let c = &sq(&rat(big)) - &sq(&rat(small));
            let s = &(&radial + &sq(&lz)) + &(&c * &w2);
            let four = &rat(4.0) * &sq(&rat(big));
            &sq(&s) - &(&(&four * &radial) * &w2)
        }
    }
}

/// Every kind's form, in a tilted frame and both senses, over a tilted plane's net and a
/// cylinder's quarter-arc net: at 25 dyadic `(s, t)` the computed field is within its
/// bound of the exact one, and the bound is not vacuous.
#[test]
fn the_field_is_within_its_bound_of_the_exact_rationals() {
    let o = [0.25, -0.5, 0.75];
    let mut forms = Vec::new();
    for sense in [Sense::Forward, Sense::Reversed] {
        let sign = sense.sign();
        let f = tilted(o);
        forms.push((Surface::Plane(Plane::new(f)), f, 1.0, Shape::Plane));
        let c = Cylinder::new(f, mm(1.25), sense).unwrap();
        forms.push((Surface::Cylinder(c), f, sign, Shape::Cylinder(1.25)));
        let k = Cone::new(f, mm(0.75), 0.3, sense).unwrap();
        let slope = fmath::tan(0.3);
        forms.push((Surface::Cone(k), f, sign, Shape::Cone(0.75, slope)));
        let s = Sphere::new(f, mm(2.5), sense).unwrap();
        forms.push((Surface::Sphere(s), f, sign, Shape::Sphere(2.5)));
        let t = Torus::new(f, mm(3.0), mm(1.25), sense).unwrap();
        forms.push((Surface::Torus(t), f, sign, Shape::Torus(3.0, 1.25)));
    }
    let nets = [
        (
            Surface::Plane(Plane::new(frame(
                [0.5, 0.25, -1.0],
                [0.6, 0.0, 0.8],
                [0.0, 1.0, 0.0],
            ))),
            rect(-1.0, 0.5, 2.0, 3.0),
        ),
        (
            Surface::Cylinder(
                Cylinder::new(
                    frame([1.0, 0.0, 0.5], [0.0, 0.6, 0.8], [1.0, 0.0, 0.0]),
                    mm(2.0),
                    Sense::Forward,
                )
                .unwrap(),
            ),
            rect(0.25, -1.0, 2.0 * FRAC_PI_2 - 0.5, 2.0),
        ),
    ];
    let ts = [0.0, 0.25, 0.375, 0.8125, 1.0];
    for (ns, ne) in &nets {
        let net = Net::of(ns, ne).unwrap();
        for (fs, f, sign, shape) in &forms {
            let form = Implicit::of(fs).unwrap();
            let g = form.field(&net).unwrap();
            let size = g.f.max_abs_coefficient();
            assert!(g.err <= 1e-12 * size.max(1.0), "{} {size}", g.err);
            let bound = rat(g.err);
            for s in ts {
                for t in ts {
                    let (sr, tr) = (rat(s), rat(t));
                    let want = exact_field(&net, f, *sign, *shape, &sr, &tr);
                    let got = exact_value(&g.f, &sr, &tr);
                    let off = (&got - &want).abs();
                    assert!(off <= bound, "{} > {} at {s} {t}", off.to_f64(), g.err);
                }
            }
        }
    }
}

// --- the SM0 truths ---------------------------------------------------------------------

/// A case: the pair, the two surfaces' distances, and points on the true curves.
struct Case {
    name: &'static str,
    pair: [(Surface, Extent2d); 2],
    surfs: [Surf; 2],
    samples: Vec<Point3>,
}

fn truth_case(
    name: &'static str,
    pair: [(Surface, Extent2d); 2],
    t: &Truth,
    keep: impl Fn(Point3) -> bool,
    to_pair: impl Fn([f64; 3]) -> [f64; 3],
) -> Case {
    Case {
        name,
        pair,
        surfs: [t.a, t.b],
        samples: t
            .curves
            .iter()
            .flat_map(|c| c.samples(400))
            .map(|p| p3(to_pair(p)))
            .filter(|&p| keep(p))
            .collect(),
    }
}

fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    let z = |o: [f64; 3]| frame(o, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);

    // An analytic sphere of radius 3 against a NURBS plane at z = 1: the sphere's quadric.
    let sphere = Surface::Sphere(Sphere::new(z([0.0; 3]), mm(3.0), Sense::Forward).unwrap());
    let plane = Surface::Plane(Plane::new(z([0.0, 0.0, 1.0])));
    let r = 8.0_f64.sqrt();
    out.push(Case {
        name: "sphere vs nurbs plane",
        pair: [
            (sphere, rect(0.0, 0.0, 1.0, 1.0)),
            nurbs(&plane, rect(-5.0, -5.0, 5.0, 5.0)),
        ],
        surfs: [
            Surf::Sphere {
                centre: [0.0; 3],
                radius: 3.0,
            },
            Surf::Plane {
                point: [0.0, 0.0, 1.0],
                normal: [0.0, 0.0, 1.0],
            },
        ],
        samples: (0..400)
            .map(|i| {
                let (s, c) = fmath::sin_cos(TAU * f64::from(i) / 400.0);
                Point3::new(r * c, r * s, 1.0)
            })
            .collect(),
    });

    // A NURBS sphere against an analytic plane: the plane's form.
    let nsp = nurbs_sphere_and_plane();
    out.push(Case {
        name: "nurbs sphere vs plane",
        pair: nsp,
        surfs: [
            Surf::Sphere {
                centre: [0.0; 3],
                radius: 3.0,
            },
            Surf::Plane {
                point: [0.0, 0.0, 1.0],
                normal: [0.0, 0.0, 1.0],
            },
        ],
        samples: (0..400)
            .map(|i| {
                let (s, c) = fmath::sin_cos(TAU * f64::from(i) / 400.0);
                Point3::new(r * c, r * s, 1.0)
            })
            .collect(),
    });

    // The pin's tpipe: both analytic, no closed form. The truth's first cylinder (radius
    // 1.5, axis z) is the branch along y; its second (radius 2, axis x) the pipe along z:
    // truth (x, y, z) is tpipe (y, z, x), the surfaces given in tpipe's place. The branch
    // runs over y >= 0 only.
    let t = cylinders_perpendicular(1.5, 2.0, 0.0).unwrap();
    let mut case = truth_case("tpipe", tpipe(), &t, |p| p.y >= 0.0, |[x, y, z]| [y, z, x]);
    case.surfs = [
        Surf::Cylinder {
            point: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            radius: 2.0,
        },
        Surf::Cylinder {
            point: [0.0; 3],
            axis: [0.0, 1.0, 0.0],
            radius: 1.5,
        },
    ];
    out.push(case);

    // An analytic sphere against a NURBS sphere: one circle.
    let (c1, c2) = ([0.0; 3], [1.5, 0.5, 0.25]);
    let t = sphere_sphere(c1, 2.0, c2, 1.75).unwrap();
    let s1 = Surface::Sphere(Sphere::new(z(c1), mm(2.0), Sense::Forward).unwrap());
    let s2 = Surface::Sphere(Sphere::new(z(c2), mm(1.75), Sense::Forward).unwrap());
    out.push(truth_case(
        "sphere vs nurbs sphere",
        [
            (s1, rect(0.0, 0.0, 1.0, 1.0)),
            nurbs(&s2, rect(0.0, 0.0, 1.0, 1.0)),
        ],
        &t,
        |_| true,
        |p| p,
    ));

    // Parallel cylinders e = 1.2 apart, one a NURBS image: two lines.
    let t = cylinders_parallel(1.0, 1.0, 1.2, 2.0).unwrap();
    let c1 = Surface::Cylinder(Cylinder::new(z([0.0; 3]), mm(1.0), Sense::Forward).unwrap());
    let c2 = Surface::Cylinder(Cylinder::new(z([1.2, 0.0, 0.0]), mm(1.0), Sense::Forward).unwrap());
    out.push(truth_case(
        "cylinder vs nurbs cylinder",
        [
            (c1, rect(0.0, -2.0, TAU, 2.0)),
            nurbs(&c2, rect(0.0, -2.0, TAU, 2.0)),
        ],
        &t,
        |_| true,
        |p| p,
    ));

    // A torus against a NURBS plane parallel to its axis: two spiric ovals, the quartic form.
    let t = torus_plane_parallel(3.0, 1.0, 1.0).unwrap();
    let torus = Surface::Torus(Torus::new(z([0.0; 3]), mm(3.0), mm(1.0), Sense::Forward).unwrap());
    let plane = Surface::Plane(Plane::new(frame(
        [0.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0],
    )));
    out.push(truth_case(
        "torus vs nurbs plane",
        [
            (torus, rect(0.0, 0.0, 1.0, 1.0)),
            nurbs(&plane, rect(-5.0, -2.0, 5.0, 2.0)),
        ],
        &t,
        |_| true,
        |p| p,
    ));
    out
}

/// Every branch's vertices lie on both surfaces within its certificate, its pcurves map
/// onto it within theirs, and every truth sample lies within the accuracy of a chord.
#[test]
fn the_field_path_traces_the_sm0_truths() {
    for case in cases() {
        let t = trace(&case.pair, &mut Budget::unlimited())
            .unwrap_or_else(|e| panic!("{}: {e:?}", case.name));
        assert!(!t.branches.is_empty(), "{}", case.name);
        assert!(
            t.touches.is_empty() && t.coincident.is_empty(),
            "{}",
            case.name
        );
        assert!(
            t.nodes
                .iter()
                .all(|n| matches!(n.kind, NodeKind::Seam | NodeKind::Pole)),
            "{}",
            case.name
        );
        let mut chords = Vec::new();
        for b in &t.branches {
            let bound = b.certificate.to_intersection.mm();
            assert!(bound <= ACC, "{}: {bound}", case.name);
            let c = &b.curve;
            assert_eq!(c.degree(), 1, "{}", case.name);
            let pts: Vec<Point3> = c.points().iter().map(|&p| p3(p)).collect();
            for &p in &pts {
                let r = case.surfs[0]
                    .distance([p.x, p.y, p.z])
                    .abs()
                    .max(case.surfs[1].distance([p.x, p.y, p.z]).abs());
                assert!(r <= bound + 1e-12, "{}: {r} > {bound}", case.name);
            }
            // Degree 1: the i-th vertex is at the knot i + 1.
            let knots = c.knots();
            for (i, &p) in pts.iter().enumerate() {
                let k = knots[i + 1];
                let q = p3(c.point(k, Approach::Above).unwrap());
                assert!(dist(p, q) <= 1e-12, "{}", case.name);
                for (side, pc) in b.pcurves.iter().enumerate() {
                    let on = case.pair[side].0.point(pc.point(k));
                    let within = match b.certificate.on_surface[side] {
                        OnSurface::Exact => 1e-12,
                        OnSurface::Within(l) => l.mm(),
                    };
                    assert!(
                        dist(on, p) <= within + 1e-12,
                        "{} side {side}: {} > {within}",
                        case.name,
                        dist(on, p)
                    );
                }
            }
            chords.push(pts);
        }
        let far = case
            .samples
            .iter()
            .map(|&s| {
                chords
                    .iter()
                    .flat_map(|pl| pl.windows(2).map(|w| point_segment(s, w[0], w[1])))
                    .fold(f64::INFINITY, f64::min)
            })
            .fold(0.0, f64::max);
        assert!(
            far <= 2.0 * ACC,
            "{}: a truth sample {far} from the trace",
            case.name
        );
    }
}

// --- coincidence ------------------------------------------------------------------------

fn flat(o: [f64; 3]) -> Surface {
    Surface::Plane(Plane::new(frame(o, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0])))
}

/// The closed forms' answer for the same two analytic planes.
fn closed(a: &Surface, ae: &Extent2d, b: &Surface, be: &Extent2d) -> Vec<Overlap> {
    match analytic::intersect(a, ae, b, be, acc(), res()) {
        Closed::Solved(Intersection::Coincident(o)) => o,
        Closed::Solved(Intersection::Empty) => Vec::new(),
        other => panic!("{other:?}"),
    }
}

/// The polygon's vertices, rounded to a picometre and sorted: equal as sets.
fn corners(p: &[Point2]) -> Vec<[i64; 2]> {
    #[allow(clippy::cast_possible_truncation)]
    let r = |x: f64| (x * 1e9).round() as i64;
    let mut v: Vec<[i64; 2]> = p.iter().map(|q| [r(q.x), r(q.y)]).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// A NURBS plane on an analytic one: overlapping, the overlap is coincident and no curve,
/// as the closed forms give it for two analytic planes; abutting along an edge, or beside
/// it with a gap, they agree with the closed forms too (a zero-width overlap or nothing).
#[test]
fn a_coincident_patch_is_told_from_a_coplanar_neighbour() {
    let a = flat([0.0; 3]);
    let ae = rect(-1.0, -1.0, 1.0, 1.0);
    for (name, be, area) in [
        ("overlapping", rect(0.0, 0.0, 2.0, 2.0), true),
        ("abutting", rect(1.0, -1.0, 2.0, 1.0), false),
        ("beside", rect(1.5, -1.0, 3.0, 1.0), false),
    ] {
        let want = closed(&a, &ae, &a, &be);
        let pair = [(a.clone(), ae), nurbs(&a, be)];
        let t = trace(&pair, &mut Budget::unlimited()).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert!(
            t.branches.is_empty() && t.touches.is_empty(),
            "{name}: {t:?}"
        );
        assert_eq!(t.coincident.len(), want.len(), "{name}: {t:?}");
        for (got, want) in t.coincident.iter().zip(&want) {
            assert_eq!(corners(&got.on_a), corners(&want.on_a), "{name}");
            assert_eq!(corners(&got.on_b), corners(&want.on_b), "{name}");
        }
        if area {
            assert_eq!(
                corners(&t.coincident[0].on_a),
                corners(&[
                    Point2::new(0.0, 0.0),
                    Point2::new(1.0, 0.0),
                    Point2::new(1.0, 1.0),
                    Point2::new(0.0, 1.0),
                ]),
                "{name}"
            );
        }
    }
}

/// A NURBS plane parallel to an analytic one: half the accuracy off, one surface (the
/// whole overlap coincident); three times it off, apart (nothing).
#[test]
fn a_parallel_plane_is_coincident_only_within_the_accuracy() {
    let a = flat([0.0; 3]);
    let ae = rect(-1.0, -1.0, 1.0, 1.0);
    let be = rect(-2.0, -2.0, 2.0, 2.0);
    let near = [(a.clone(), ae), nurbs(&flat([0.0, 0.0, 0.5 * ACC]), be)];
    let t = trace(&near, &mut Budget::unlimited()).unwrap();
    assert_eq!(t.coincident.len(), 1, "{t:?}");
    assert!(t.branches.is_empty());
    let far = [(a, ae), nurbs(&flat([0.0, 0.0, 3.0 * ACC]), be)];
    assert!(trace(&far, &mut Budget::unlimited()).unwrap().is_empty());
}

// --- the rim ----------------------------------------------------------------------------

/// A plane through the edge `u = 0` of a NURBS plane: the curve runs along the rim, where
/// the grid's signs cannot see it, so it is refused there and never dropped; the same plane
/// a little inside crosses the patch and is traced.
#[test]
fn a_curve_along_the_rim_is_refused_not_dropped() {
    let wall = |x: f64| {
        Surface::Plane(Plane::new(frame(
            [x, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        )))
    };
    let be = rect(0.0, 0.0, 2.0, 2.0);
    let we = rect(-1.0, -1.0, 3.0, 3.0);
    let on = [(wall(0.0), we), nurbs(&flat([0.0; 3]), be)];
    match trace(&on, &mut Budget::unlimited()) {
        Err(SsiError::Unresolved { .. }) => {}
        other => panic!("{other:?}"),
    }
    let inside = [(wall(1e-3), we), nurbs(&flat([0.0; 3]), be)];
    let t = trace(&inside, &mut Budget::unlimited()).unwrap();
    assert_eq!(t.branches.len(), 1, "{t:?}");
}

// --- determinism ------------------------------------------------------------------------

/// Two runs give the same bits; the budget the first used gives them again, and one less
/// is exhausted, not a different answer.
#[test]
fn the_field_answer_is_the_same_bits_under_any_budget_that_suffices() {
    let cases = cases();
    for case in cases
        .iter()
        .filter(|c| c.name.starts_with("sphere vs nurbs plane"))
    {
        let mut b = Budget::unlimited();
        let first = trace(&case.pair, &mut b).unwrap();
        let used = b.used();
        assert_eq!(trace(&case.pair, &mut Budget::unlimited()).unwrap(), first);
        assert_eq!(trace(&case.pair, &mut Budget::new(used)).unwrap(), first);
        let short = trace(&case.pair, &mut Budget::new(used - 1)).unwrap_err();
        assert!(
            matches!(short, SsiError::Budget(Exhausted { .. })),
            "{short:?}"
        );
    }
}

/// A plane through a NURBS torus's equators, which lie on knot lines: each circle is traced
/// once, by the piece on whichever side the grid's signs see it. Over the whole torus the
/// outer circle lies on the rim `v = 0` (closed, but not marked periodic in the NURBS
/// form), so the trace refuses there rather than answer one circle.
#[test]
fn a_curve_along_a_knot_line_is_traced_once() {
    let torus = Surface::Torus(
        Torus::new(
            frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            mm(3.0),
            mm(1.0),
            Sense::Forward,
        )
        .unwrap(),
    );
    let plane = (flat([0.0; 3]), rect(-5.0, -5.0, 5.0, 5.0));
    for (v0, v1) in [(0.25, 0.75), (-0.25, 0.25), (0.3, 0.7), (-0.2, 0.3)] {
        let pair = [plane.clone(), nurbs(&torus, rect(0.0, v0, 1.0, v1))];
        let t = trace(&pair, &mut Budget::unlimited()).unwrap();
        assert_eq!(t.branches.len(), 1, "v in [{v0}, {v1}]: {t:?}");
    }
    let whole = [plane, nurbs(&torus, rect(0.0, 0.0, 1.0, 1.0))];
    assert!(matches!(
        trace(&whole, &mut Budget::unlimited()),
        Err(SsiError::Unresolved { .. })
    ));
}
