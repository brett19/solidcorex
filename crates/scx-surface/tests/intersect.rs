// Written for solidcorex. Apache-2.0.
//! `intersect_curve` against testkit T2 (`curve_surface`, exact roots in double-double) and
//! the exact checks of 63 §4.2 (SU6):
//! - every point lies on the curve at its `t`, within `tol` of the surface (a crossing
//!   within a few ulps of the model), and its uv's surface point is its foot;
//! - every root T2 finds inside the extents is reported: a simple root as a crossing at it,
//!   or inside a tangent where roots closer than `tol` tells apart merge; no crossing is
//!   reported where T2 has no root;
//! - near-tangent curves built at a surface point, offset by fractions and multiples of
//!   `tol`: one tangent within `tol`, two crossings or none past it, as T2 has them;
//! - curves built on each surface overlap it, and T2 calls them on the surface;
//! - the cone's far nappe, `se` clipping, NURBS, the budget and the stop.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic
)]

use core::f64::consts::{PI, TAU};

use scx_curve::{Circle, Curve, Ellipse, Hyperbola, Line, Parabola};
use scx_num::budget::{Budget, Stop};
use scx_num::domain::ParamDomain;
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::hash::Rng;
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_surface::{
    Cone, Contact, CurveSurfaceHit, Cylinder, OnSurface, Plane, Sphere, Surface, SurfaceError,
    Torus, curve_on_surface, intersect_curve,
};
use scx_testkit::analytic;
use scx_testkit::curve_surface::{self as t2, CurveKind, Meeting, SurfaceKind};
use scx_testkit::dd::Dd;
use scx_testkit::sample;

/// The model's size: frames within 10 of the origin, radii up to 20.
const SIZE: f64 = 20.0;
/// A crossing's point is a root of the equation to its rounding: a few ulps of the model.
const ROOT: f64 = 1e-11 * SIZE;
/// The far end of an unbounded parameter.
const FAR: f64 = 1e6;

fn len(x: f64) -> Length {
    Length::from_mm(x)
}

fn arr(v: Vec3) -> [f64; 3] {
    [v.x, v.y, v.z]
}

fn frame(rng: &mut Rng) -> Frame {
    let f = sample::frame(rng, 10.0);
    let v = |a: [f64; 3]| Vec3::new(a[0], a[1], a[2]);
    Frame::new(
        Point3::new(f.origin[0], f.origin[1], f.origin[2]),
        v(f.z),
        v(f.x),
    )
    .unwrap()
}

fn sense(rng: &mut Rng) -> Sense {
    if rng.next_f64() < 0.5 {
        Sense::Forward
    } else {
        Sense::Reversed
    }
}

fn between(rng: &mut Rng, lo: f64, hi: f64) -> f64 {
    lo + (hi - lo) * rng.next_f64()
}

fn unit(rng: &mut Rng) -> Vec3 {
    let d = sample::direction(rng);
    Vec3::new(d[0], d[1], d[2])
}

/// The testkit frame: `x`, `y` turned over by `y_sign`, `z` by `z_sign`.
fn kit(f: &Frame, y_sign: f64, z_sign: f64) -> analytic::Frame {
    analytic::Frame {
        origin: f.origin().to_array(),
        x: arr(*f.x()),
        y: arr(*f.y() * y_sign),
        z: arr(*f.z() * z_sign),
    }
}

// ---------------------------------------------------------------- the inputs

/// A surface and its T2 twin.
#[derive(Clone)]
struct Surf {
    s: Surface,
    kit: SurfaceKind,
}

fn plane(f: Frame) -> Surf {
    Surf {
        s: Plane::new(f).into(),
        kit: SurfaceKind::Plane(analytic::Plane {
            frame: kit(&f, 1.0, 1.0),
        }),
    }
}

fn cylinder(f: Frame, r: f64, s: Sense) -> Surf {
    Surf {
        s: Cylinder::new(f, len(r), s).unwrap().into(),
        kit: SurfaceKind::Cylinder(analytic::Cylinder {
            frame: kit(&f, 1.0, s.sign()),
            radius: r,
        }),
    }
}

/// A cone; T2's half-angle is positive, so a negative one turns its `z` over.
fn cone(f: Frame, r: f64, angle: f64, s: Sense) -> Surf {
    Surf {
        s: Cone::new(f, len(r), angle, s).unwrap().into(),
        kit: SurfaceKind::Cone(analytic::Cone {
            frame: kit(&f, 1.0, s.sign() * angle.signum()),
            radius: r,
            semi_angle: angle.abs(),
        }),
    }
}

fn sphere(f: Frame, r: f64, s: Sense) -> Surf {
    Surf {
        s: Sphere::new(f, len(r), s).unwrap().into(),
        kit: SurfaceKind::Sphere(analytic::Sphere {
            frame: kit(&f, 1.0, s.sign()),
            radius: r,
        }),
    }
}

fn torus(f: Frame, big: f64, small: f64, s: Sense) -> Surf {
    Surf {
        s: Torus::new(f, len(big), len(small), s).unwrap().into(),
        kit: SurfaceKind::Torus(analytic::Torus {
            frame: kit(&f, 1.0, s.sign()),
            major: big,
            minor: small,
        }),
    }
}

/// One random surface of each kind.
fn surfaces(rng: &mut Rng) -> Vec<Surf> {
    let mut out = Vec::new();
    let f = frame(rng);
    out.push(plane(f));
    let (f, s) = (frame(rng), sense(rng));
    out.push(cylinder(f, between(rng, 0.5, 20.0), s));
    let (f, s) = (frame(rng), sense(rng));
    let angle = between(rng, 0.1, 1.3) * if rng.next_f64() < 0.5 { 1.0 } else { -1.0 };
    let r = if rng.next_f64() < 0.2 {
        0.0
    } else {
        between(rng, 0.5, 10.0)
    };
    out.push(cone(f, r, angle, s));
    let (f, s) = (frame(rng), sense(rng));
    out.push(sphere(f, between(rng, 0.5, 20.0), s));
    let (f, s) = (frame(rng), sense(rng));
    let big = between(rng, 2.0, 15.0);
    out.push(torus(f, big, big * between(rng, 0.2, 0.9), s));
    out
}

/// A curve, its T2 twin and its extent.
#[derive(Clone)]
struct Path {
    c: Curve,
    kit: CurveKind,
    ce: Extent1d,
}

fn line(origin: Point3, direction: Vec3, half: f64) -> Path {
    Path {
        c: Line::new(origin, direction).unwrap().into(),
        kit: CurveKind::Line(analytic::Line {
            origin: origin.to_array(),
            direction: arr(direction),
        }),
        ce: Extent1d::new(-half, half).unwrap(),
    }
}

fn circle(f: Frame, r: f64, s: Sense, ce: Extent1d) -> Path {
    Path {
        c: Circle::new(f, len(r), s).unwrap().into(),
        kit: CurveKind::Circle(analytic::Circle {
            frame: kit(&f, s.sign(), 1.0),
            radius: r,
        }),
        ce,
    }
}

fn ellipse(f: Frame, a: f64, b: f64, s: Sense, ce: Extent1d) -> Path {
    Path {
        c: Ellipse::new(f, len(a), len(b), s).unwrap().into(),
        kit: CurveKind::Ellipse(analytic::Ellipse {
            frame: kit(&f, s.sign(), 1.0),
            semi_x: a,
            semi_y: b,
        }),
        ce,
    }
}

fn parabola(f: Frame, focal: f64, s: Sense, ce: Extent1d) -> Path {
    Path {
        c: Parabola::new(f, len(focal), s).unwrap().into(),
        kit: CurveKind::Parabola(analytic::Parabola {
            frame: kit(&f, s.sign(), 1.0),
            focal,
        }),
        ce,
    }
}

fn hyperbola(f: Frame, a: f64, b: f64, s: Sense, ce: Extent1d) -> Path {
    Path {
        c: Hyperbola::new(f, len(a), len(b), s).unwrap().into(),
        kit: CurveKind::Hyperbola(analytic::Hyperbola {
            frame: kit(&f, s.sign(), 1.0),
            semi_x: a,
            semi_y: b,
        }),
        ce,
    }
}

/// Whether `t` parameterises the curve as a line does: not an angle.
fn open(k: &CurveKind) -> bool {
    matches!(
        k,
        CurveKind::Line(_) | CurveKind::Parabola(_) | CurveKind::Hyperbola(_)
    )
}

fn turn() -> Extent1d {
    Extent1d::new(0.0, TAU).unwrap()
}

/// Random curves about `o`: lines, circles and ellipses over a turn and over an arc,
/// parabolas and hyperbolas over a stretch about the vertex and one off it.
fn curves(rng: &mut Rng, o: Point3) -> Vec<Path> {
    let mut out = Vec::new();
    for _ in 0..3 {
        let p = o + unit(rng) * between(rng, 0.0, 15.0);
        out.push(line(p, unit(rng) * between(rng, 0.5, 2.0), 60.0));
    }
    for i in 0..4 {
        let c = o + unit(rng) * between(rng, 0.0, 12.0);
        let f = Frame::new(c, unit(rng), unit(rng)).unwrap();
        let ce = if i % 2 == 0 {
            turn()
        } else {
            let lo = between(rng, -7.0, 7.0);
            Extent1d::new(lo, lo + between(rng, 0.5, 6.0)).unwrap()
        };
        let a = between(rng, 0.5, 15.0);
        let s = sense(rng);
        if i < 2 {
            out.push(circle(f, a, s, ce));
        } else {
            out.push(ellipse(f, a, a * between(rng, 0.2, 1.0), s, ce));
        }
    }
    for i in 0..4 {
        let c = o + unit(rng) * between(rng, 0.0, 12.0);
        let f = Frame::new(c, unit(rng), unit(rng)).unwrap();
        let s = sense(rng);
        let a = between(rng, 0.5, 10.0);
        if i < 2 {
            let lo = if i == 0 {
                -3.0
            } else {
                between(rng, -3.0, 2.0)
            };
            let ce = Extent1d::new(lo, lo + between(rng, 0.5, 5.0)).unwrap();
            out.push(parabola(f, a, s, ce));
        } else {
            let lo = if i == 2 {
                -2.0
            } else {
                between(rng, -2.5, 1.5)
            };
            let ce = Extent1d::new(lo, lo + between(rng, 0.5, 4.0)).unwrap();
            out.push(hyperbola(f, a, a * between(rng, 0.2, 2.0), s, ce));
        }
    }
    out
}

/// The surface's whole domain, the unbounded ends at [`FAR`].
fn full(s: &Surface) -> Extent2d {
    let side = |d: ParamDomain| match d {
        ParamDomain::Periodic { period, base, .. } => (base, base + period),
        ParamDomain::Bounded(e) => (e.min(), e.max()),
        ParamDomain::AtLeast { from, .. } => (from, FAR),
        ParamDomain::AtMost { to, .. } => (-FAR, to),
        _ => (-FAR, FAR),
    };
    let d = s.domain();
    let (u, v) = (side(d.u), side(d.v));
    Extent2d::from_bounds(u.0, v.0, u.1, v.1).unwrap()
}

/// The resolution whose zone is `mm` millimetres.
fn zone(mm: f64) -> Resolution {
    Resolution::new(len(mm)).unwrap()
}

fn run(p: &Path, s: &Surf, se: &Extent2d, tol: f64) -> Result<Vec<CurveSurfaceHit>, SurfaceError> {
    intersect_curve(
        &p.c,
        &p.ce,
        &s.s,
        se,
        zone(tol),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
}

// ---------------------------------------------------------------- the checks

fn dd3(v: [Dd; 3]) -> Point3 {
    Point3::new(v[0].to_f64(), v[1].to_f64(), v[2].to_f64())
}

/// T2's roots inside the extents: the point and multiplicity.
fn truth(p: &Path, s: &Surf) -> Option<Vec<(Point3, u32, f64)>> {
    match t2::intersect(&p.kit, &s.kit).unwrap() {
        Meeting::OnSurface => None,
        Meeting::Points(hits) => Some(
            hits.into_iter()
                .filter(|h| h.pin_uv.is_some())
                .filter(|h| {
                    let t = h.t.to_f64();
                    let (lo, hi) = (p.ce.min(), p.ce.max());
                    match p.kit {
                        ref k if open(k) => (lo..=hi).contains(&t),
                        _ => {
                            hi - lo >= TAU || {
                                let t = t + TAU * ((lo - t) / TAU).ceil();
                                t <= hi + 1e-12
                            }
                        }
                    }
                })
                .map(|h| (dd3(h.point), h.multiplicity, h.t.to_f64()))
                .collect(),
        ),
    }
}

#[derive(Default, Debug)]
struct Stats {
    pairs: usize,
    roots: usize,
    crossings: usize,
    tangents: usize,
    overlaps: usize,
    worst_crossing: f64,
    worst_by_kind: [f64; 5],
    /// Crossings by a parabola or a hyperbola.
    open_crossings: usize,
}

fn kind(s: &Surface) -> usize {
    match s {
        Surface::Plane(_) => 0,
        Surface::Cylinder(_) => 1,
        Surface::Cone(_) => 2,
        Surface::Sphere(_) => 3,
        _ => 4,
    }
}

/// Whether the curve stays within `tol` of the surface from `from` to `to` (the shorter
/// way round a conic), on 32 samples.
fn joined(p: &Path, s: &Surf, tol: f64, from: f64, to: f64) -> bool {
    let to = match p.kit {
        ref k if open(k) => to,
        _ => from + (to - from + PI).rem_euclid(TAU) - PI,
    };
    (0..=32).all(|i| {
        let t = from + (to - from) * f64::from(i) / 32.0;
        let d = t2::distance(&s.kit, p.c.point(t).to_array()).unwrap();
        d <= tol * 1.01 + ROOT
    })
}

/// The exact checks on one answer; panics with the case on a failure.
fn check(p: &Path, s: &Surf, se: &Extent2d, tol: f64, hits: &[CurveSurfaceHit], stats: &mut Stats) {
    stats.pairs += 1;
    let Some(roots) = truth(p, s) else {
        assert!(
            matches!(hits, [CurveSurfaceHit::Overlap { .. }]),
            "T2 has the curve on the surface: {hits:?}"
        );
        stats.overlaps += 1;
        return;
    };
    stats.roots += roots.len();
    // A tangent stands for the roots it is joined to by a stretch of curve within `tol`
    // of the surface (roots closer than `tol` tells apart).
    let joined = |from: f64, to: f64| joined(p, s, tol, from, to);
    let mut points = Vec::new();
    for h in hits {
        let CurveSurfaceHit::Point {
            t,
            uv,
            point,
            contact,
        } = *h
        else {
            panic!("an overlap where T2 has points {roots:?}: {hits:?}");
        };
        assert_eq!(point, p.c.point(t), "the point is the curve's at t");
        assert!(se.contains_point(uv, 0.0), "{uv:?} outside {se:?}");
        let d = t2::distance(&s.kit, point.to_array()).unwrap();
        let foot = s.s.point(uv);
        assert!(
            (foot - point).length() <= d * 1.01 + ROOT,
            "the uv's point {foot:?} is not the foot of {point:?} ({d})"
        );
        match contact {
            Contact::Crossing => {
                stats.crossings += 1;
                if !matches!(p.kit, CurveKind::Line(_)) && open(&p.kit) {
                    stats.open_crossings += 1;
                }
                let near = roots
                    .iter()
                    .map(|(q, ..)| (*q - point).length())
                    .fold(f64::INFINITY, f64::min);
                stats.worst_crossing = stats.worst_crossing.max(near);
                let k = kind(&s.s);
                stats.worst_by_kind[k] = stats.worst_by_kind[k].max(near);
                assert!(
                    near <= ROOT,
                    "a crossing {near} from T2's roots {roots:?}: {hits:?}"
                );
            }
            Contact::Tangent => {
                stats.tangents += 1;
                assert!(
                    d <= tol * (1.0 + 1e-9) + ROOT,
                    "a tangent {d} from the surface"
                );
            }
        }
        points.push((point, contact, t));
    }
    for (q, m, tq) in &roots {
        let found = points.iter().any(|(pt, c, t)| {
            let gap = (*pt - *q).length();
            match c {
                Contact::Crossing => *m == 1 && gap <= ROOT,
                Contact::Tangent => joined(*t, *tq),
            }
        });
        assert!(found, "T2's root {q:?} ({m}) missed: {hits:?}");
    }
}

// ---------------------------------------------------------------- the tests

#[test]
fn random_pairs_match_t2() {
    let mut stats = Stats::default();
    for seed in 0..40 {
        let mut rng = Rng::new(0x5_06_00 + seed);
        for s in surfaces(&mut rng) {
            let se = full(&s.s);
            let o = Point3::new(s.kit_origin()[0], s.kit_origin()[1], s.kit_origin()[2]);
            for p in curves(&mut rng, o) {
                for tol in [1e-6, 1e-3] {
                    let hits = run(&p, &s, &se, tol).unwrap();
                    check(&p, &s, &se, tol, &hits, &mut stats);
                }
            }
        }
    }
    eprintln!("{stats:?}");
    assert!(
        stats.crossings > 1000 && stats.open_crossings > 300,
        "{stats:?}"
    );
}

impl Surf {
    fn kit_origin(&self) -> [f64; 3] {
        match &self.kit {
            SurfaceKind::Plane(k) => k.frame.origin,
            SurfaceKind::Cylinder(k) => k.frame.origin,
            SurfaceKind::Cone(k) => k.frame.origin,
            SurfaceKind::Sphere(k) => k.frame.origin,
            SurfaceKind::Torus(k) => k.frame.origin,
        }
    }
}

/// A uv well inside the surface's domain, off a cone's apex and a sphere's poles.
fn interior_uv(rng: &mut Rng, s: &Surface) -> Point2 {
    let u = rng.next_f64();
    match s {
        Surface::Plane(_) => Point2::new(between(rng, -5.0, 5.0), between(rng, -5.0, 5.0)),
        Surface::Cylinder(c) => Point2::new(u * TAU * c.radius().mm(), between(rng, -5.0, 5.0)),
        Surface::Cone(c) => {
            let k = fmath::tan(c.half_angle());
            let rho = between(rng, 0.5, 5.0);
            Point2::new(u, (rho - c.radius().mm()) / k)
        }
        Surface::Sphere(_) => Point2::new(u, between(rng, 0.15, 0.85)),
        Surface::Torus(_) => Point2::new(u, rng.next_f64()),
        _ => unreachable!(),
    }
}

#[test]
fn near_tangents_classify_at_the_tolerance() {
    let mut stats = Stats::default();
    let (mut touches, mut misses, mut pairs) = (0, 0, 0);
    for seed in 0..4 {
        let mut rng = Rng::new(0x5_06_01 + seed);
        for s in surfaces(&mut rng) {
            let se = full(&s.s);
            for tol in [1e-6, 1e-3] {
                let uv = interior_uv(&mut rng, &s.s);
                let at = s.s.point(uv);
                let n = s.s.normal(uv, [Approach::Below; 2]).unwrap().as_vec();
                let t1 = n.cross(unit(&mut rng)).normalized().unwrap().as_vec();
                for delta in [
                    0.0,
                    1e-9,
                    -1e-9,
                    0.5 * tol,
                    -0.5 * tol,
                    2.0 * tol,
                    -2.0 * tol,
                ] {
                    let q = at + n * delta;
                    let mut paths = Vec::new();
                    if !matches!(s.s, Surface::Plane(_)) {
                        paths.push(line(q, t1, 30.0));
                    }
                    // A conic through `q` along `t1`, curving away from `n` or toward it.
                    for side in [1.0, -1.0] {
                        let a = between(&mut rng, 0.5, 15.0);
                        let x = n * -side;
                        let f = Frame::new(q - x * a, x.cross(t1), x).unwrap();
                        paths.push(circle(f, a, Sense::Forward, turn()));
                        let b = a * between(&mut rng, 0.3, 0.9);
                        paths.push(ellipse(f, a, b, Sense::Forward, turn()));
                    }
                    for p in paths {
                        let hits = run(&p, &s, &se, tol).unwrap();
                        check(&p, &s, &se, tol, &hits, &mut stats);
                        let near: Vec<_> = hits
                            .iter()
                            .filter_map(|h| match h {
                                CurveSurfaceHit::Point {
                                    t, point, contact, ..
                                } if if delta.abs() < tol {
                                    joined(&p, &s, tol, *t, 0.0)
                                } else {
                                    (*point - at).length() < 0.2
                                } =>
                                {
                                    Some(*contact)
                                }
                                _ => None,
                            })
                            .collect();
                        let kit = truth(&p, &s)
                            .map(|r| r.iter().filter(|(q, ..)| (*q - at).length() < 0.2).count());
                        pairs += 1;
                        if delta.abs() < tol {
                            // Within the tolerance: one touch, unless the curve osculates
                            // so closely that it stays within `tol` across the window.
                            if !hits
                                .iter()
                                .any(|h| matches!(h, CurveSurfaceHit::Overlap { .. }))
                            {
                                assert_eq!(near, [Contact::Tangent], "delta {delta}: {hits:?}");
                                touches += 1;
                            }
                        } else if kit == Some(0) {
                            assert!(near.is_empty(), "delta {delta} misses: {hits:?}");
                            misses += 1;
                        }
                    }
                }
            }
        }
    }
    eprintln!("{stats:?} touches {touches} misses {misses} of {pairs}");
    assert!(touches > 500 && misses > 150, "{touches} {misses}");
}

/// Curves lying on each kind about a random frame: a plane's line and circle, a cylinder's
/// ruling and parallel, a cone's ruling and parallel, a sphere's parallel and meridian, a
/// torus's parallel and tube circle; and (new in 63 §4.14) a cylinder's oblique section.
fn on_surface_cases(rng: &mut Rng) -> Vec<(Surf, Path)> {
    let f = frame(rng);
    let (o, x, y, z) = (f.origin(), *f.x(), *f.y(), *f.z());
    let s = sense(rng);
    let zs = z * s.sign();
    let phi = between(rng, 0.0, TAU);
    let dir = x * fmath::cos(phi) + y * fmath::sin(phi);
    let along = |c: Point3, r: f64, axis: Vec3, x: Vec3| {
        circle(
            Frame::new(c, axis, x).unwrap(),
            r,
            sense(&mut Rng::new(7)),
            turn(),
        )
    };
    let mut cases: Vec<(Surf, Path)> = Vec::new();
    cases.push((plane(f), line(o + x * 2.0 - y, dir, 40.0)));
    cases.push((plane(f), along(o + x * 3.0, 4.0, z, x)));
    let r = between(rng, 1.0, 10.0);
    cases.push((cylinder(f, r, s), line(o + dir * r, z, 40.0)));
    cases.push((cylinder(f, r, s), along(o + z * 3.0, r, z, dir)));
    let angle = between(rng, 0.2, 1.2);
    let k = fmath::tan(angle);
    let v = between(rng, 0.5, 3.0);
    cases.push((
        cone(f, r, angle, s),
        line(o + dir * (r + k * v) + zs * v, dir * k + zs, 2.0),
    ));
    cases.push((cone(f, r, angle, s), along(o + zs * v, r + k * v, z, dir)));
    let lat = between(rng, -1.2, 1.2);
    cases.push((
        sphere(f, r, s),
        along(o + z * (r * fmath::sin(lat)), r * fmath::cos(lat), z, dir),
    ));
    cases.push((sphere(f, r, s), along(o, r, dir.cross(z), dir)));
    let small = r * between(rng, 0.2, 0.8);
    let big = r;
    cases.push((
        torus(f, big, small, s),
        along(
            o + z * (small * fmath::sin(lat)),
            big + small * fmath::cos(lat),
            z,
            dir,
        ),
    ));
    cases.push((
        torus(f, big, small, s),
        along(o + dir * big, small, dir.cross(z), dir),
    ));
    let tilt = between(rng, 0.1, 1.2);
    let normal = dir.cross(z) * fmath::sin(tilt) + z * fmath::cos(tilt);
    let fe = Frame::new(o + z * v, normal, dir).unwrap();
    cases.push((
        cylinder(f, r, s),
        ellipse(fe, r, r / fmath::cos(tilt), sense(rng), turn()),
    ));
    // The open conics: in a plane, and a cone's sections by a plane along a generator (a
    // parabola, its vertex `c0 / 2` inside the generator's line) and along the axis (the
    // hyperbola's branch on the nappe, its centre the apex's height).
    let ext = |a: f64| Extent1d::new(-a, a).unwrap();
    let side = dir.cross(z);
    let fp = Frame::new(o + x * 0.5 - y, z, dir).unwrap();
    cases.push((
        plane(f),
        parabola(fp, between(rng, 0.2, 2.0), sense(rng), ext(3.0)),
    ));
    let (a, b) = (between(rng, 0.5, 3.0), between(rng, 0.5, 3.0));
    cases.push((plane(f), hyperbola(fp, a, b, sense(rng), ext(2.0))));
    let apex = o + zs * (-r / k);
    let g = (dir * k + zs) * (1.0 / fmath::hypot(k, 1.0));
    let c0 = r * between(rng, 0.2, 1.0);
    let hv = c0 / (2.0 * k);
    let vertex = apex + dir * (-0.5 * c0) + zs * hv;
    let focal = k * c0 / (2.0 * fmath::hypot(k, 1.0));
    cases.push((
        cone(f, r, angle, s),
        parabola(
            Frame::new(vertex, g.cross(side), g).unwrap(),
            focal,
            sense(rng),
            ext(between(rng, 1.0, 4.0)),
        ),
    ));
    let c = r * between(rng, 0.2, 1.0);
    cases.push((
        cone(f, r, angle, s),
        hyperbola(
            Frame::new(apex + dir * c, zs.cross(side), zs).unwrap(),
            c / k,
            c,
            sense(rng),
            ext(between(rng, 0.5, 2.0)),
        ),
    ));
    cases
}

#[test]
fn curves_on_the_surface_overlap_it() {
    let mut rng = Rng::new(0x5_06_02);
    let mut count = 0;
    for _ in 0..20 {
        for (surf, p) in on_surface_cases(&mut rng) {
            let se = full(&surf.s);
            let hits = run(&p, &surf, &se, 1e-6).unwrap();
            assert!(
                matches!(hits[..], [CurveSurfaceHit::Overlap { t }] if t == p.ce),
                "{:?} on {:?}: {hits:?}",
                p.c,
                surf.s
            );
            count += 1;
        }
    }
    assert_eq!(count, 300);
}

/// [`curve_on_surface`] and the work it charged.
fn stated(p: &Path, s: &Surf, se: &Extent2d, tol: f64) -> (OnSurface, u64) {
    let mut budget = Budget::unlimited();
    let on = curve_on_surface(&p.c, &p.ce, &s.s, se, zone(tol), &mut budget, &Stop::new());
    (on.unwrap(), budget.used())
}

/// The answer [`curve_on_surface`] stands for, as [`intersect_curve`] gives it.
fn read(on: OnSurface) -> Vec<CurveSurfaceHit> {
    match on {
        OnSurface::Proved(t) | OnSurface::Isolated(t) => t
            .into_iter()
            .map(|t| CurveSurfaceHit::Overlap { t })
            .collect(),
        OnSurface::Leaves(hits) => hits,
    }
}

/// BO21: a curve stated to lie on a surface is answered as `intersect_curve` answers it, to
/// the bit, on curves lying on each kind and on random pairs that do not; on a plane the
/// closed-form bound proves it for one unit against the roots' two a piece.
#[test]
fn a_stated_curve_is_answered_as_intersect_curve_answers_it() {
    let mut rng = Rng::new(0x5_06_21);
    let mut proved = 0;
    for _ in 0..20 {
        for (surf, p) in on_surface_cases(&mut rng) {
            let se = full(&surf.s);
            for tol in [1e-6, 1e-3] {
                let (on, work) = stated(&p, &surf, &se, tol);
                let mut plain = Budget::unlimited();
                let want = intersect_curve(
                    &p.c,
                    &p.ce,
                    &surf.s,
                    &se,
                    zone(tol),
                    &mut plain,
                    &Stop::new(),
                );
                if matches!(surf.s, Surface::Plane(_)) {
                    assert!(matches!(on, OnSurface::Proved(_)), "{:?}: {on:?}", p.c);
                    assert!(
                        work == 1 && work < plain.used(),
                        "{work} against {}",
                        plain.used()
                    );
                    proved += 1;
                } else {
                    assert!(matches!(on, OnSurface::Isolated(_)), "{:?}: {on:?}", p.c);
                    assert_eq!(work, plain.used());
                }
                assert_eq!(read(on), want.unwrap());
            }
        }
    }
    assert_eq!(proved, 160);
    for seed in 0..10 {
        let mut rng = Rng::new(0x5_06_00 + seed);
        for s in surfaces(&mut rng) {
            let se = full(&s.s);
            let o = Point3::new(s.kit_origin()[0], s.kit_origin()[1], s.kit_origin()[2]);
            for p in curves(&mut rng, o) {
                for tol in [1e-6, 1e-3] {
                    let (on, _) = stated(&p, &s, &se, tol);
                    assert_eq!(read(on), run(&p, &s, &se, tol).unwrap());
                }
            }
        }
    }
}

/// A line just off a plane: within the tolerance the bound proves it on; past it the
/// curve leaves, as `intersect_curve` finds, and nothing is proved.
#[test]
fn a_stated_line_off_a_plane_is_proved_only_within_the_tolerance() {
    let f = Frame::new(
        Point3::new(1.0, 2.0, 3.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    let surf = plane(f);
    let se = full(&surf.s);
    let tol = 1e-6;
    for (lift, proved) in [
        (0.0, true),
        (0.5, true),
        (0.99, true),
        (1.5, false),
        (3.0, false),
    ] {
        // tilted: one end on the plane, the other `lift` tolerances above it
        let p = line(
            Point3::new(1.0, 2.0, 3.0 + 0.5 * lift * tol),
            Vec3::new(1.0, 0.0, lift * tol / 20.0),
            10.0,
        );
        let (on, _) = stated(&p, &surf, &se, tol);
        assert_eq!(
            matches!(on, OnSurface::Proved(_)),
            proved,
            "lift {lift}: {on:?}"
        );
        if lift > 1.0 {
            assert!(matches!(on, OnSurface::Leaves(_)), "lift {lift}: {on:?}");
        }
        assert_eq!(read(on), run(&p, &surf, &se, tol).unwrap(), "lift {lift}");
    }
}

/// Whether the overlap pieces cover `t` (on a closed curve, `t` or `t` a turn on).
fn covered(pieces: &[Extent1d], t: f64, closed: bool) -> bool {
    pieces
        .iter()
        .any(|e| e.contains_value(t, 0.0) || (closed && e.contains_value(t + TAU, 0.0)))
}

/// How far `t` is from the nearest end of a piece, turns apart on a closed curve.
fn from_ends(pieces: &[Extent1d], t: f64, closed: bool) -> f64 {
    pieces
        .iter()
        .flat_map(|e| [e.min(), e.max()])
        .map(|end| {
            let d = t - end;
            if closed {
                (d - TAU * (d / TAU).round()).abs()
            } else {
                d.abs()
            }
        })
        .fold(f64::INFINITY, f64::min)
}

/// SU6's follow-up (63 §4.14): an overlap is clipped by `se`. Against an independent
/// oracle, the surface's own inversion placed against `se`, at 200 parameters across the
/// curve: a parameter is in a piece exactly when its foot is inside, except within `1e-6`
/// of a piece's end. Each `se` is a random part of the domain, its `u` sometimes a whole
/// period or more.
#[test]
fn overlaps_are_clipped_by_se() {
    let mut rng = Rng::new(0x5_06_04);
    let (mut several, mut none, mut whole, mut checked) = (0, 0, 0, 0);
    for _ in 0..30 {
        for (surf, p) in on_surface_cases(&mut rng) {
            let full = full(&surf.s);
            let part = |rng: &mut Rng, lo: f64, hi: f64| {
                let (lo, hi) = (lo.max(-30.0), hi.min(30.0));
                let w = (hi - lo) * between(rng, 0.2, 1.0);
                let a = lo + (hi - lo - w) * rng.next_f64();
                (a, a + w)
            };
            let (u, v) = match surf.s.domain().u {
                ParamDomain::Periodic { period, base, .. } => {
                    let a = base + period * between(&mut rng, -1.0, 1.0);
                    let w = period * between(&mut rng, 0.1, 1.2);
                    ((a, a + w), part(&mut rng, full.min().y, full.max().y))
                }
                _ => (
                    part(&mut rng, full.min().x, full.max().x),
                    part(&mut rng, full.min().y, full.max().y),
                ),
            };
            let se = Extent2d::from_bounds(u.0, v.0, u.1, v.1).unwrap();
            let hits = run(&p, &surf, &se, 1e-6).unwrap();
            let pieces: Vec<Extent1d> = hits
                .iter()
                .map(|h| match h {
                    CurveSurfaceHit::Overlap { t } => *t,
                    other => panic!("{other:?}"),
                })
                .collect();
            assert!(
                pieces.windows(2).all(|w| w[0].max() < w[1].min()),
                "{pieces:?}"
            );
            match pieces.len() {
                0 => none += 1,
                1 if pieces[0] == p.ce => whole += 1,
                1 => {}
                _ => several += 1,
            }
            let closed = !open(&p.kit) && p.ce.length() >= TAU;
            let (lo, hi) = (
                p.ce.min(),
                if closed { p.ce.min() + TAU } else { p.ce.max() },
            );
            for i in 0..200 {
                let t = lo + (hi - lo) * (f64::from(i) + 0.5) / 200.0;
                if from_ends(&pieces, t, closed) < 1e-6 {
                    continue;
                }
                let inv = surf
                    .s
                    .invert(
                        p.c.point(t),
                        None,
                        Some(&se),
                        &mut Budget::unlimited(),
                        &Stop::new(),
                    )
                    .unwrap();
                let inside = matches!(inv, scx_num::invert::Inversion::Inside(_));
                assert_eq!(
                    covered(&pieces, t, closed),
                    inside,
                    "{:?} on {:?} in {se:?} at {t}: {pieces:?}",
                    p.c,
                    surf.s
                );
                checked += 1;
            }
        }
    }
    assert!(
        several > 10 && none > 20 && whole > 5,
        "{several} {none} {whole}"
    );
    assert!(checked > 50_000, "{checked}");
}

/// The exact pieces of a unit circle in a plane box: `x >= -1/2` keeps the arc about
/// `t = 0`, across the turn's start; `|x|, |y| <= 4/5` keeps four arcs.
#[test]
fn a_circle_in_a_box_is_its_arcs() {
    let s = plane(Frame::WORLD);
    let p = circle(Frame::WORLD, 1.0, Sense::Forward, turn());
    let se = Extent2d::from_bounds(-0.5, -2.0, 2.0, 2.0).unwrap();
    let hits = run(&p, &s, &se, 1e-9).unwrap();
    let third = TAU / 3.0;
    assert!(
        matches!(hits[..], [CurveSurfaceHit::Overlap { t }]
            if (t.min() - 2.0 * third).abs() < 1e-14 && (t.max() - 4.0 * third).abs() < 1e-14),
        "{hits:?}"
    );
    let se = Extent2d::from_bounds(-0.8, -0.8, 0.8, 0.8).unwrap();
    let hits = run(&p, &s, &se, 1e-9).unwrap();
    let (a, b) = (fmath::acos(0.8), fmath::asin(0.8));
    assert_eq!(hits.len(), 4, "{hits:?}");
    for (k, h) in hits.iter().enumerate() {
        let CurveSurfaceHit::Overlap { t } = h else {
            panic!("{h:?}");
        };
        let q = PI / 2.0 * k as f64;
        assert!((t.min() - (q + a)).abs() < 1e-14, "{k} {t:?}");
        assert!((t.max() - (q + b)).abs() < 1e-14, "{k} {t:?}");
    }
    // The box past the circle: nothing; around it: the whole turn.
    let se = Extent2d::from_bounds(2.0, 2.0, 3.0, 3.0).unwrap();
    assert!(run(&p, &s, &se, 1e-9).unwrap().is_empty());
    let se = Extent2d::from_bounds(-2.0, -2.0, 2.0, 2.0).unwrap();
    assert!(
        matches!(run(&p, &s, &se, 1e-9).unwrap()[..], [CurveSurfaceHit::Overlap { t }] if t == p.ce)
    );
}

/// A cylinder's ruling clipped in `v`, and a parallel in `u` over half the turn, the
/// window a period up.
#[test]
fn rulings_and_parallels_clip_exactly() {
    let r = 2.0;
    let s = cylinder(Frame::WORLD, r, Sense::Forward);
    let p = line(Point3::new(r, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 40.0);
    let period = TAU * r;
    let se = Extent2d::from_bounds(0.0, -1.0, period, 2.5).unwrap();
    let hits = run(&p, &s, &se, 1e-9).unwrap();
    assert!(
        matches!(hits[..], [CurveSurfaceHit::Overlap { t }] if t.min() == -1.0 && t.max() == 2.5),
        "{hits:?}"
    );
    let p = circle(Frame::WORLD, r, Sense::Forward, turn());
    let se = Extent2d::from_bounds(period, -1.0, 1.5 * period, 1.0).unwrap();
    let hits = run(&p, &s, &se, 1e-9).unwrap();
    assert!(
        matches!(hits[..], [CurveSurfaceHit::Overlap { t }]
            if t.min().abs() < 1e-14 && (t.max() - PI).abs() < 1e-14),
        "{hits:?}"
    );
}

#[test]
fn nurbs_budget_and_stop_are_typed() {
    let s = sphere(Frame::WORLD, 1.0, Sense::Forward);
    let p = circle(Frame::WORLD, 0.5, Sense::Forward, turn());
    let se = full(&s.s);
    let nurbs = Surface::from(s.s.to_nurbs(&se).unwrap());
    // An analytic curve on a NURBS surface is searched: the circle is inside the sphere.
    let hits = intersect_curve(
        &p.c,
        &p.ce,
        &nurbs,
        &se,
        zone(1e-6),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap();
    assert!(hits.is_empty(), "{hits:?}");
    // A NURBS curve is named, never an empty answer.
    let k = scx_nurbs::KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
    let n = scx_nurbs::NurbsCurve::new(k, vec![[-2.0, 0.0, 0.0], [2.0, 0.0, 0.0]], None).unwrap();
    let e = intersect_curve(
        &Curve::Nurbs(std::sync::Arc::new(n)),
        &Extent1d::new(0.0, 1.0).unwrap(),
        &nurbs,
        &se,
        zone(1e-6),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap_err();
    assert_eq!(e, SurfaceError::NurbsIntersection);
    assert_eq!(e.name(), "nurbs_intersection_unbuilt");
    let mut budget = Budget::new(1);
    let e = intersect_curve(
        &p.c,
        &p.ce,
        &s.s,
        &se,
        zone(1e-6),
        &mut budget,
        &Stop::new(),
    )
    .unwrap_err();
    assert!(matches!(e, SurfaceError::Exhausted(_)), "{e:?}");
    let stop = Stop::new();
    stop.set();
    let e = intersect_curve(
        &p.c,
        &p.ce,
        &s.s,
        &se,
        zone(1e-6),
        &mut Budget::unlimited(),
        &stop,
    )
    .unwrap_err();
    assert_eq!(e, SurfaceError::Stopped);
    // A circle against the sphere is two pieces: four units.
    let mut budget = Budget::new(4);
    intersect_curve(
        &p.c,
        &p.ce,
        &s.s,
        &se,
        zone(1e-6),
        &mut budget,
        &Stop::new(),
    )
    .unwrap();
    assert_eq!(budget.used(), 4);
}

#[test]
fn open_conics_overlap_their_plane_and_touch_at_the_vertex() {
    let mut rng = Rng::new(0x5_06_77);
    let mut touched = 0;
    for i in 0..20 {
        let f = frame(&mut rng);
        let s = sense(&mut rng);
        let a = between(&mut rng, 0.5, 10.0);
        let ce = Extent1d::new(-between(&mut rng, 0.5, 3.0), between(&mut rng, 0.5, 3.0)).unwrap();
        let p = if i % 2 == 0 {
            parabola(f, a, s, ce)
        } else {
            hyperbola(f, a, a * between(&mut rng, 0.2, 2.0), s, ce)
        };
        // In its own plane: an overlap over the whole extent.
        let own = plane(f);
        let hits = run(&p, &own, &full(&own.s), 1e-6).unwrap();
        assert_eq!(hits, vec![CurveSurfaceHit::Overlap { t: ce }]);
        // The plane through the vertex across the axis: one tangent at t = 0.
        let vertex = p.c.point(0.0);
        let across = Frame::new(vertex, *f.x(), *f.y()).unwrap();
        let touch = plane(across);
        let hits = run(&p, &touch, &full(&touch.s), 1e-6).unwrap();
        let [CurveSurfaceHit::Point { t, contact, .. }] = hits[..] else {
            panic!("{hits:?}");
        };
        assert_eq!(contact, Contact::Tangent);
        assert!(t.abs() <= 1e-3, "{t}");
        touched += 1;
        // Moved past the vertex by more than tol along x: two crossings, at T2's roots.
        let past = plane(Frame::new(vertex + *f.x() * 0.01, *f.x(), *f.y()).unwrap());
        let hits = run(&p, &past, &full(&past.s), 1e-6).unwrap();
        let mut stats = Stats::default();
        check(&p, &past, &full(&past.s), 1e-6, &hits, &mut stats);
        assert_eq!(stats.crossings, 2, "{hits:?}");
    }
    assert_eq!(touched, 20);
}
