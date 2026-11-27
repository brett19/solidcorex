// Written for solidcorex. Apache-2.0.
//! The analytic kinds against the testkit's references (research 59 T4, 63 §4.4):
//! - points and the uv map against `quadric::Implicit` and `quadric::Layout` in
//!   double-double, and on the implicit quadric by exact residual;
//! - jets against the exact rational NURBS of the same layout (the tensor product of the
//!   quarter-arc circles, evaluated in rationals);
//! - normals against the jet's `S_u x S_v`, `None` exactly at the poles.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic
)]

use core::f64::consts::{FRAC_PI_2, PI};

use scx_num::domain::ParamDomain;
use scx_num::error::ErrorKind;
use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::hash::Rng;
use scx_num::interval::Ivl;
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_nurbs_surface::{NurbsSurface, ParamAxis};
use scx_surface::{Cone, Cylinder, Plane, Sphere, Surface as AnySurface, SurfaceError, Torus};
use scx_testkit::analytic;
use scx_testkit::dd::{self, Dd};
use scx_testkit::nurbs::{Side, Surface};
use scx_testkit::quadric::{Implicit, Layout, Order, Param};
use scx_testkit::rat::Rat;
use scx_testkit::sample;

const SAMPLES: usize = 40;

fn frame(rng: &mut Rng) -> Frame {
    let f = sample::frame(rng, 10.0);
    let v = |a: [f64; 3]| Vec3::new(a[0], a[1], a[2]);
    let o = f.origin;
    Frame::new(Point3::new(o[0], o[1], o[2]), v(f.z), v(f.x)).unwrap_or(Frame::WORLD)
}

/// The testkit frame of a kind: its x, y and effective z.
fn tk(frame: &Frame, sense: Sense) -> analytic::Frame {
    let a = |v: Vec3| [v.x, v.y, v.z];
    let o = frame.origin();
    analytic::Frame {
        origin: [o.x, o.y, o.z],
        x: a(*frame.x()),
        y: a(*frame.y()),
        z: a(*frame.z() * sense.sign()),
    }
}

fn same() -> Param {
    Param::Linear {
        nurb: (0.0, 1.0),
        step: (0.0, 1.0),
    }
}

fn turn(period: f64) -> Param {
    Param::arcs((0.0, period), Dd::new(0.0), dd::consts().pi * 2.0)
}

fn layout(u: Param, v: Param) -> Layout {
    Layout {
        u,
        v,
        order: Order::Same,
    }
}

fn dist(p: Point3, q: [Dd; 3]) -> f64 {
    let d = [
        (Dd::new(p.x) - q[0]).to_f64(),
        (Dd::new(p.y) - q[1]).to_f64(),
        (Dd::new(p.z) - q[2]).to_f64(),
    ];
    fmath::hypot(fmath::hypot(d[0], d[1]), d[2])
}

fn on(i: &Implicit, p: Point3) -> f64 {
    let r = |x: f64| Rat::from_f64(x).unwrap_or_else(Rat::zero);
    i.distance(&[r(p.x), r(p.y), r(p.z)])
        .unwrap_or(f64::INFINITY)
}

fn sense(rng: &mut Rng) -> Sense {
    if rng.next_f64() < 0.5 {
        Sense::Forward
    } else {
        Sense::Reversed
    }
}

/// A parameter in `[lo, hi]`, a break one time in four.
fn param(rng: &mut Rng, lo: f64, hi: f64, span: f64) -> f64 {
    if rng.next_f64() < 0.25 {
        let n = ((hi - lo) / span).round();
        lo + span * (rng.next_f64() * n).floor()
    } else {
        lo + (hi - lo) * rng.next_f64()
    }
}

/// The analytic kinds as one value for the shared checks.
enum Kind {
    Plane(Plane),
    Cylinder(Cylinder),
    Cone(Cone),
    Sphere(Sphere),
    Torus(Torus),
}

struct Case {
    kind: Kind,
    implicit: Implicit,
    layout: Layout,
    /// The `u` and `v` ranges sampled, and their spans.
    u: (f64, f64, f64),
    v: (f64, f64, f64),
    /// The tensor-product NURBS of the layout over the sampled range.
    nurbs: Surface<3>,
    scale: f64,
}

impl Kind {
    fn point(&self, uv: Point2) -> Point3 {
        match self {
            Kind::Plane(s) => s.point(uv),
            Kind::Cylinder(s) => s.point(uv),
            Kind::Cone(s) => s.point(uv),
            Kind::Sphere(s) => s.point(uv),
            Kind::Torus(s) => s.point(uv),
        }
    }
    fn eval_ivl(&self, uv: Point2) -> Point3<Ivl> {
        let (u, v) = (Ivl::from(uv.x), Ivl::from(uv.y));
        match self {
            Kind::Plane(s) => s.eval(u, v, uv),
            Kind::Cylinder(s) => s.eval(u, v, uv),
            Kind::Cone(s) => s.eval(u, v, uv),
            Kind::Sphere(s) => s.eval(u, v, uv),
            Kind::Torus(s) => s.eval(u, v, uv),
        }
    }
    fn eval_f64(&self, uv: Point2) -> Point3 {
        match self {
            Kind::Plane(s) => s.eval(uv.x, uv.y, uv),
            Kind::Cylinder(s) => s.eval(uv.x, uv.y, uv),
            Kind::Cone(s) => s.eval(uv.x, uv.y, uv),
            Kind::Sphere(s) => s.eval(uv.x, uv.y, uv),
            Kind::Torus(s) => s.eval(uv.x, uv.y, uv),
        }
    }
    fn jet(&self, uv: Point2, side: [Approach; 2]) -> Vec<((usize, usize), Vec3)> {
        macro_rules! j {
            ($s:expr) => {{
                let j = $s.jet::<3>(uv, side);
                let mut out = Vec::new();
                for i in 0..=3 {
                    for k in 0..=3 - i {
                        out.push(((i, k), j.get(i, k).unwrap_or_default()));
                    }
                }
                out
            }};
        }
        match self {
            Kind::Plane(s) => j!(s),
            Kind::Cylinder(s) => j!(s),
            Kind::Cone(s) => j!(s),
            Kind::Sphere(s) => j!(s),
            Kind::Torus(s) => j!(s),
        }
    }
    fn normal(&self, uv: Point2) -> Option<Vec3> {
        match self {
            Kind::Plane(s) => Some(*s.normal(uv)),
            Kind::Cylinder(s) => Some(*s.normal(uv)),
            Kind::Cone(s) => s.normal(uv).map(|n| *n),
            Kind::Sphere(s) => Some(*s.normal(uv)),
            Kind::Torus(s) => Some(*s.normal(uv)),
        }
    }
    fn to_step(&self, uv: Point2) -> Point2 {
        match self {
            Kind::Plane(s) => s.to_step(uv),
            Kind::Cylinder(s) => s.to_step(uv),
            Kind::Cone(s) => s.to_step(uv),
            Kind::Sphere(s) => s.to_step(uv),
            Kind::Torus(s) => s.to_step(uv),
        }
    }
    fn to_nurbs(&self, over: &Extent2d) -> Result<NurbsSurface<3>, SurfaceError> {
        match self {
            Kind::Plane(s) => s.to_nurbs(over),
            Kind::Cylinder(s) => s.to_nurbs(over),
            Kind::Cone(s) => s.to_nurbs(over),
            Kind::Sphere(s) => s.to_nurbs(over),
            Kind::Torus(s) => s.to_nurbs(over),
        }
    }
    fn any(&self) -> AnySurface {
        match self {
            Kind::Plane(s) => AnySurface::Plane(*s),
            Kind::Cylinder(s) => AnySurface::Cylinder(*s),
            Kind::Cone(s) => AnySurface::Cone(*s),
            Kind::Sphere(s) => AnySurface::Sphere(*s),
            Kind::Torus(s) => AnySurface::Torus(*s),
        }
    }
    fn uv_of_step(&self, st: Point2) -> Point2 {
        match self {
            Kind::Plane(s) => s.from_step(st),
            Kind::Cylinder(s) => s.from_step(st),
            Kind::Cone(s) => s.from_step(st),
            Kind::Sphere(s) => s.from_step(st),
            Kind::Torus(s) => s.from_step(st),
        }
    }
}

// ---------------------------------------------------------------- the exact NURBS

/// A planar rational curve: degree, knots, control points `(a, b)`, weights.
struct Curve2 {
    degree: usize,
    knots: Vec<f64>,
    points: Vec<(f64, f64)>,
    weights: Vec<f64>,
}

/// The unit circle in four quarter arcs from angle 0 over `[0, period]`.
fn circle(period: f64) -> Curve2 {
    let q = period / 4.0;
    let w = core::f64::consts::FRAC_1_SQRT_2;
    let k = |i: f64| q * i;
    Curve2 {
        degree: 2,
        knots: vec![
            0.0,
            0.0,
            0.0,
            k(1.0),
            k(1.0),
            k(2.0),
            k(2.0),
            k(3.0),
            k(3.0),
            period,
            period,
            period,
        ],
        points: vec![
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
        weights: vec![1.0, w, 1.0, w, 1.0, w, 1.0, w, 1.0],
    }
}

/// `(cos, sin)` of the latitude from `-pi/2` to `pi/2` over `[0, 1]`.
fn half_circle() -> Curve2 {
    let w = core::f64::consts::FRAC_1_SQRT_2;
    Curve2 {
        degree: 2,
        knots: vec![0.0, 0.0, 0.0, 0.5, 0.5, 1.0, 1.0, 1.0],
        points: vec![(0.0, -1.0), (1.0, -1.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
        weights: vec![1.0, w, 1.0, w, 1.0],
    }
}

/// The line `(f, g)` from `(f0, g0)` at `lo` to `(f1, g1)` at `hi`.
fn segment(lo: f64, hi: f64, a: (f64, f64), b: (f64, f64)) -> Curve2 {
    Curve2 {
        degree: 1,
        knots: vec![lo, lo, hi, hi],
        points: vec![a, b],
        weights: vec![1.0, 1.0],
    }
}

/// `(f, g)` scaled and shifted: `(f0 + s a, s b)`.
fn profile(c: Curve2, f0: f64, s: f64) -> Curve2 {
    Curve2 {
        points: c.points.iter().map(|&(a, b)| (f0 + s * a, s * b)).collect(),
        ..c
    }
}

/// `O + f(v) D(u) + g(v) Z` as a tensor-product NURBS: `D` from `around`, `(f, g)` from
/// `prof`, net `O + f_j D_i + g_j Z` with weights `w_i w_j`.
fn revolve(t: &analytic::Frame, around: &Curve2, prof: &Curve2) -> Surface<3> {
    let mut pts = Vec::new();
    let mut ws = Vec::new();
    for (&(c, s), &wu) in around.points.iter().zip(&around.weights) {
        for (&(f, g), &wv) in prof.points.iter().zip(&prof.weights) {
            pts.push(core::array::from_fn(|k| {
                t.origin[k] + f * (c * t.x[k] + s * t.y[k]) + g * t.z[k]
            }));
            ws.push(wu * wv);
        }
    }
    Surface::new(
        (around.degree, prof.degree),
        &around.knots,
        &prof.knots,
        &pts,
        Some(&ws),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// The plane `O + u X + v Y` over `[lo, hi]^2`.
fn plane_nurbs(t: &analytic::Frame, lo: f64, hi: f64) -> Surface<3> {
    let mut pts = Vec::new();
    for u in [lo, hi] {
        for v in [lo, hi] {
            pts.push(core::array::from_fn(|k| {
                t.origin[k] + u * t.x[k] + v * t.y[k]
            }));
        }
    }
    Surface::new((1, 1), &[lo, lo, hi, hi], &[lo, lo, hi, hi], &pts, None)
        .unwrap_or_else(|e| panic!("{e}"))
}

// ---------------------------------------------------------------- the cases

fn cases(seed: u64) -> Vec<Case> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::new();
    for _ in 0..SAMPLES / 5 {
        let f = frame(&mut rng);
        let t0 = tk(&f, Sense::Forward);
        out.push(Case {
            kind: Kind::Plane(Plane::new(f)),
            implicit: Implicit::Plane { frame: t0 },
            layout: layout(same(), same()),
            u: (-5.0, 5.0, 10.0),
            v: (-5.0, 5.0, 10.0),
            nurbs: plane_nurbs(&t0, -5.0, 5.0),
            scale: 20.0,
        });

        let (f, s) = (frame(&mut rng), sense(&mut rng));
        let t = tk(&f, s);
        let r = 0.5 + 4.0 * rng.next_f64();
        let period = 2.0 * PI * r;
        out.push(Case {
            kind: Kind::Cylinder(
                Cylinder::new(f, Length::from_mm(r), s).unwrap_or_else(|e| panic!("{e}")),
            ),
            implicit: Implicit::Cylinder {
                frame: t,
                radius: r,
            },
            layout: layout(turn(period), same()),
            u: (0.0, period, period / 4.0),
            v: (-3.0, 4.0, 7.0),
            nurbs: revolve(
                &t,
                &circle(period),
                &segment(-3.0, 4.0, (r, -3.0), (r, 4.0)),
            ),
            scale: 20.0,
        });

        let (f, s) = (frame(&mut rng), sense(&mut rng));
        let t = tk(&f, s);
        let r = 0.5 + 4.0 * rng.next_f64();
        let a = (0.1 + 1.3 * rng.next_f64()) * if rng.next_f64() < 0.5 { 1.0 } else { -1.0 };
        let cone = Cone::new(f, Length::from_mm(r), a, s).unwrap_or_else(|e| panic!("{e}"));
        let k = fmath::tan(a);
        // From near the apex to past the base circle, on the domain's nappe.
        let apex = -r / k;
        let (lo, hi) = if k > 0.0 {
            (apex + 0.01, apex + 2.0 * r / k.abs() + 1.0)
        } else {
            (apex - 2.0 * r / k.abs() - 1.0, apex - 0.01)
        };
        out.push(Case {
            kind: Kind::Cone(cone),
            implicit: Implicit::Cone {
                frame: t,
                radius: r,
                slope: k.into(),
            },
            layout: layout(turn(1.0), same()),
            u: (0.0, 1.0, 0.25),
            v: (lo, hi, hi - lo),
            nurbs: revolve(
                &t,
                &circle(1.0),
                &segment(lo, hi, (r + k * lo, lo), (r + k * hi, hi)),
            ),
            scale: 20.0 + hi.abs() + lo.abs(),
        });

        let (f, s) = (frame(&mut rng), sense(&mut rng));
        let t = tk(&f, s);
        let r = 0.5 + 4.0 * rng.next_f64();
        out.push(Case {
            kind: Kind::Sphere(
                Sphere::new(f, Length::from_mm(r), s).unwrap_or_else(|e| panic!("{e}")),
            ),
            implicit: Implicit::Sphere {
                frame: t,
                radius: r,
            },
            layout: layout(
                turn(1.0),
                Param::arcs((0.0, 1.0), -dd::consts().pio2, dd::consts().pi),
            ),
            u: (0.0, 1.0, 0.25),
            v: (0.0, 1.0, 0.5),
            nurbs: revolve(&t, &circle(1.0), &profile(half_circle(), 0.0, r)),
            scale: 20.0,
        });

        let (f, s) = (frame(&mut rng), sense(&mut rng));
        let t = tk(&f, s);
        let big = 2.0 + 3.0 * rng.next_f64();
        let small = (0.1 + 0.8 * rng.next_f64()) * big;
        out.push(Case {
            kind: Kind::Torus(
                Torus::new(f, Length::from_mm(big), Length::from_mm(small), s)
                    .unwrap_or_else(|e| panic!("{e}")),
            ),
            implicit: Implicit::Torus {
                frame: t,
                major: big,
                minor: small,
            },
            layout: layout(turn(1.0), turn(1.0)),
            u: (0.0, 1.0, 0.25),
            v: (0.0, 1.0, 0.25),
            nurbs: revolve(&t, &circle(1.0), &profile(circle(1.0), big, small)),
            scale: 20.0,
        });
    }
    out
}

fn uvs(rng: &mut Rng, c: &Case, n: usize) -> Vec<Point2> {
    (0..n)
        .map(|_| {
            Point2::new(
                param(rng, c.u.0, c.u.1, c.u.2),
                param(rng, c.v.0, c.v.1, c.v.2),
            )
        })
        .collect()
}

#[test]
fn points_follow_the_layout_and_lie_on_the_quadric() {
    let mut rng = Rng::new(11);
    let mut worst = 0.0f64;
    for c in cases(1) {
        for uv in uvs(&mut rng, &c, 30) {
            let p = c.kind.point(uv);
            let truth = c.implicit.point_at(&c.layout, (uv.x, uv.y));
            let d = dist(p, truth) / c.scale;
            worst = worst.max(d);
            assert!(d < 4e-16, "{uv:?}: {d:e}");
            let off = on(&c.implicit, p) / c.scale;
            assert!(off < 4e-16, "{uv:?}: off by {off:e}");
            assert_eq!(c.kind.eval_f64(uv), p, "the f64 ring is point");
            let e = c.kind.eval_ivl(uv);
            for (iv, x) in [(e.x, p.x), (e.y, p.y), (e.z, p.z)] {
                assert!(iv.lo <= x && x <= iv.hi, "{iv:?} misses {x}");
            }
        }
    }
    eprintln!("worst point error / scale: {worst:e}");
}

#[test]
fn the_uv_map_is_the_layout_and_inverts() {
    let mut rng = Rng::new(12);
    for c in cases(2) {
        for uv in uvs(&mut rng, &c, 30) {
            let st = c.kind.to_step(uv);
            let (a, b) = c.layout.to_step((uv.x.into(), uv.y.into()));
            // Angles in radians, continuous: compare modulo a turn.
            let near = |x: f64, y: Dd| {
                let d = (Dd::new(x) - y).to_f64();
                let turns = (d / (2.0 * PI)).round();
                (d - turns * 2.0 * PI).abs()
            };
            assert!(
                near(st.x, a) < 1e-14,
                "{uv:?}: u {} vs {}",
                st.x,
                a.to_f64()
            );
            assert!(near(st.y, b) < 1e-14 * (1.0 + st.y.abs()), "{uv:?}: v");
            let back = c.kind.uv_of_step(st);
            let tol = 1e-14 * (1.0 + uv.x.abs() + uv.y.abs());
            assert!(
                (back.x - uv.x).abs() < tol && (back.y - uv.y).abs() < tol,
                "{uv:?} {back:?}"
            );
            let (tu, tv) = c.layout.to_nurb((Dd::new(st.x), Dd::new(st.y)));
            assert!((tu.to_f64() - uv.x).abs() < tol && (tv.to_f64() - uv.y).abs() < tol);
        }
    }
}

fn side_of(a: Approach) -> Side {
    match a {
        Approach::Below => Side::Below,
        Approach::Above => Side::Above,
    }
}

#[test]
fn jets_match_the_exact_nurbs() {
    let mut rng = Rng::new(13);
    let mut worst = 0.0f64;
    for c in cases(3) {
        let (ulo, uhi) = (c.u.0, c.u.1);
        let (vlo, vhi) = (c.v.0, c.v.1);
        for uv in uvs(&mut rng, &c, 3) {
            for side in [
                [Approach::Above, Approach::Above],
                [Approach::Below, Approach::Below],
                [Approach::Above, Approach::Below],
            ] {
                // The NURBS takes its end span at the domain's ends; so do the kinds, except
                // across the periodic seam, which the NURBS does not have.
                let su = if uv.x == ulo {
                    Approach::Above
                } else if uv.x == uhi {
                    Approach::Below
                } else {
                    side[0]
                };
                let sv = if uv.y == vlo {
                    Approach::Above
                } else if uv.y == vhi {
                    Approach::Below
                } else {
                    side[1]
                };
                let truth = c
                    .nurbs
                    .derivs((uv.x, uv.y), (side_of(su), side_of(sv)), 3)
                    .unwrap_or_else(|e| panic!("{e}"));
                for ((i, j), d) in c.kind.jet(uv, [su, sv]) {
                    let t = truth.get(i, j).map_or_else(Default::default, |e| e.rounded);
                    let scale = c.scale
                        * fmath::powi(1.0 / c.u.2.min(1.0), i as i32)
                        * fmath::powi(4.0 / c.v.2.min(1.0), j as i32);
                    let err = fmath::hypot(fmath::hypot(d.x - t[0], d.y - t[1]), d.z - t[2]);
                    worst = worst.max(err / scale);
                    assert!(
                        err < 1e-13 * scale,
                        "{uv:?} {side:?} ({i},{j}): {d:?} vs {t:?}"
                    );
                }
            }
        }
    }
    eprintln!("worst jet error / scale: {worst:e}");
}

#[test]
fn normals_are_the_jets_cross_and_none_at_poles() {
    let mut rng = Rng::new(14);
    for c in cases(4) {
        for uv in uvs(&mut rng, &c, 30) {
            let j = c.kind.jet(uv, [Approach::Above; 2]);
            let get = |k: (usize, usize)| {
                j.iter()
                    .find(|(x, _)| *x == k)
                    .map_or_else(Vec3::default, |(_, v)| *v)
            };
            let cross = get((1, 0)).cross(get((0, 1)));
            match c.kind.normal(uv) {
                Some(n) if matches!(c.kind, Kind::Sphere(_)) && (uv.y == 0.0 || uv.y == 1.0) => {
                    // The limit at a pole: along the radius, outward for Forward.
                    let Kind::Sphere(s) = &c.kind else {
                        unreachable!()
                    };
                    let out = c.kind.point(uv) - s.frame().origin();
                    let d = (n - out / out.length() * s.sense().sign()).length();
                    assert!(d < 1e-12, "pole {uv:?}: {n:?}");
                }
                Some(n) => {
                    let len = cross.length();
                    assert!(len > 0.0, "{uv:?}");
                    let d = (n - cross / len).length();
                    assert!(d < 1e-12, "{uv:?}: {n:?} vs {cross:?}");
                }
                None => {
                    let apex = matches!(&c.kind, Kind::Cone(k) if uv.y == k.pole().v);
                    assert!(apex, "no normal at {uv:?}");
                }
            }
        }
    }
}

#[test]
fn outward_for_forward_inward_for_reversed() {
    let s = Sphere::new(Frame::WORLD, Length::from_mm(2.0), Sense::Forward)
        .unwrap_or_else(|e| panic!("{e}"));
    let r = Sphere::new(Frame::WORLD, Length::from_mm(2.0), Sense::Reversed)
        .unwrap_or_else(|e| panic!("{e}"));
    let uv = Point2::new(0.1, 0.3);
    let out = |p: Point3, n: Vec3| p.to_vec().dot(n);
    assert!(out(s.point(uv), *s.normal(uv)) > 0.0);
    assert!(out(r.point(uv), *r.normal(uv)) < 0.0);
    // The mirror image in z = 0: same uv, mirrored point.
    let (p, q) = (s.point(uv), r.point(uv));
    assert_eq!((p.x, p.y, p.z), (q.x, q.y, -q.z));
}

#[test]
fn poles_and_apex() {
    let s = Sphere::new(Frame::WORLD, Length::from_mm(3.0), Sense::Forward)
        .unwrap_or_else(|e| panic!("{e}"));
    for pole in s.poles() {
        let n = *s.normal(Point2::new(0.37, pole.v));
        let out = (pole.point - Point3::default()) / 3.0;
        assert!((n - out).length() < 1e-15, "{n:?}");
        let p = s.point(Point2::new(0.37, pole.v));
        assert!(
            (p - pole.point).length() < 1e-15 * 3.0,
            "{p:?} {:?}",
            pole.point
        );
    }
    let c = Cone::new(Frame::WORLD, Length::from_mm(2.0), 0.5, Sense::Forward)
        .unwrap_or_else(|e| panic!("{e}"));
    let apex = c.pole();
    assert!(c.normal(Point2::new(0.2, apex.v)).is_none());
    assert!((c.point(Point2::new(0.2, apex.v)) - apex.point).length() < 1e-15);
    assert_eq!(Some(c.domain().v), ParamDomain::at_least(apex.v));
    let n = Cone::new(Frame::WORLD, Length::from_mm(2.0), -0.5, Sense::Forward)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(Some(n.domain().v), ParamDomain::at_most(n.pole().v));
}

#[test]
fn domains() {
    let f = Frame::WORLD;
    let cyl =
        Cylinder::new(f, Length::from_mm(2.0), Sense::Forward).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(cyl.domain().u.period(), Some(4.0 * PI));
    assert_eq!(cyl.domain().v, ParamDomain::Unbounded);
    let t = Torus::new(
        f,
        Length::from_mm(2.0),
        Length::from_mm(1.0),
        Sense::Forward,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(t.domain().u.period(), Some(1.0));
    assert_eq!(t.domain().v.period(), Some(1.0));
    let p = Plane::new(f);
    assert_eq!(p.domain().u, ParamDomain::Unbounded);
    // Periodic: one turn on is the same point.
    let uv = Point2::new(0.3, 0.6);
    let d = (t.point(uv) - t.point(Point2::new(1.3, -0.4))).length();
    assert!(d < 1e-14, "{d:e}");
}

#[test]
fn construction_is_refused_with_kinds() {
    let f = Frame::WORLD;
    let len = Length::from_mm;
    let kind = |e: SurfaceError| (e.kind(), e.name());
    assert_eq!(
        Cylinder::new(f, len(0.0), Sense::Forward)
            .map_err(kind)
            .err(),
        Some((ErrorKind::Refused, "size_degenerate"))
    );
    assert_eq!(
        Sphere::new(f, len(f64::NAN), Sense::Forward)
            .map_err(kind)
            .err(),
        Some((ErrorKind::Refused, "size_not_finite"))
    );
    assert_eq!(
        Cone::new(f, len(-1.0), 0.3, Sense::Forward)
            .map_err(kind)
            .err(),
        Some((ErrorKind::Refused, "size_negative"))
    );
    for a in [0.0, FRAC_PI_2, -FRAC_PI_2, 2.0] {
        assert_eq!(
            Cone::new(f, len(1.0), a, Sense::Forward)
                .map_err(kind)
                .err(),
            Some((ErrorKind::Refused, "size_degenerate")),
            "{a}"
        );
    }
    assert!(Cone::new(f, len(0.0), 0.3, Sense::Forward).is_ok());
    assert_eq!(
        Torus::new(f, len(1.0), len(1.0), Sense::Forward)
            .map_err(kind)
            .err(),
        Some((ErrorKind::Unsupported, "torus_self_intersecting"))
    );
}

#[test]
fn eval_many_is_point() {
    let s = Torus::new(
        Frame::WORLD,
        Length::from_mm(3.0),
        Length::from_mm(1.0),
        Sense::Forward,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let uv: Vec<Point2> = (0..17)
        .map(|i| Point2::new(f64::from(i) / 16.0, 0.7 - f64::from(i) / 9.0))
        .collect();
    let mut out = vec![Point3::default(); uv.len()];
    s.eval_many(&uv, &mut out);
    assert_eq!(out, uv.iter().map(|&p| s.point(p)).collect::<Vec<_>>());
}

// ---------------------------------------------------------------- to_nurbs (SU3)

fn ext(u: (f64, f64), v: (f64, f64)) -> Extent2d {
    Extent2d::from_bounds(u.0, v.0, u.1, v.1).unwrap_or_else(|| panic!("{u:?} {v:?}"))
}

/// The stored surface itself, each double taken exactly: `w P` and `w` where rational.
fn exact(n: &NurbsSurface<3>) -> Surface<3> {
    let deg = (n.degree(ParamAxis::U), n.degree(ParamAxis::V));
    let (ku, kv) = (n.knots(ParamAxis::U), n.knots(ParamAxis::V));
    match (n.weighted_points(), n.weights()) {
        (Some(wp), Some(w)) => Surface::from_homogeneous(deg, ku, kv, wp.points(), w.points()),
        _ => Surface::new(deg, ku, kv, n.points().points(), None),
    }
    .unwrap_or_else(|e| panic!("{e}"))
}

fn at(n: &NurbsSurface<3>, uv: Point2) -> Point3 {
    let p = n
        .point([uv.x, uv.y], [Approach::Above; 2])
        .unwrap_or_else(|e| panic!("{e}"));
    Point3::new(p[0], p[1], p[2])
}

/// The extents each case converts: its whole range, a random piece, one crossing the seam
/// of each turn, and one from break to break.
fn extents(rng: &mut Rng, c: &Case) -> Vec<Extent2d> {
    let (u, v) = ((c.u.0, c.u.1), (c.v.0, c.v.1));
    let piece = |rng: &mut Rng, (lo, hi): (f64, f64)| {
        let a = lo + (hi - lo) * 0.45 * rng.next_f64();
        (a, hi - (hi - lo) * 0.45 * rng.next_f64())
    };
    let seam = |(lo, hi): (f64, f64)| (lo + 0.6 * (hi - lo), lo + 1.4 * (hi - lo));
    let brk = |(lo, hi, s): (f64, f64, f64)| {
        if hi - lo > 2.0 * s {
            (lo + s, hi - s)
        } else {
            (lo, hi)
        }
    };
    let mut out = vec![
        ext(u, v),
        ext(piece(rng, u), piece(rng, v)),
        ext(brk(c.u), brk(c.v)),
    ];
    match c.kind {
        Kind::Cylinder(_) | Kind::Cone(_) | Kind::Sphere(_) => out.push(ext(seam(u), v)),
        Kind::Torus(_) => out.push(ext(seam(u), seam(v))),
        Kind::Plane(_) => {}
    }
    out
}

/// A parameter in the extent, its ends and breaks one time in four.
fn inside(rng: &mut Rng, lo: f64, hi: f64, span: f64) -> f64 {
    let r = rng.next_f64();
    if r < 0.1 {
        lo
    } else if r < 0.2 {
        hi
    } else if r < 0.3 {
        let k = (lo / span).ceil() + f64::floor(rng.next_f64() * ((hi - lo) / span));
        (k * span).clamp(lo, hi)
    } else {
        lo + (hi - lo) * rng.next_f64()
    }
}

fn wrap(t: f64, (lo, hi): (f64, f64), periodic: bool) -> f64 {
    if periodic {
        lo + (t - lo).rem_euclid(hi - lo)
    } else {
        t
    }
}

#[test]
fn to_nurbs_is_on_the_quadric_and_keeps_uv() {
    let mut rng = Rng::new(23);
    let (mut worst_on, mut worst_uv) = (0.0f64, 0.0f64);
    // The exact check is slow in a debug build: the first two of each kind take it.
    for (i, c) in cases(3).into_iter().enumerate() {
        let (pu, pv) = match c.kind {
            Kind::Plane(_) => (false, false),
            Kind::Torus(_) => (true, true),
            Kind::Sphere(_) => (true, false),
            _ => (true, false),
        };
        for e in extents(&mut rng, &c) {
            let n = c
                .kind
                .to_nurbs(&e)
                .unwrap_or_else(|err| panic!("{e:?}: {err}"));
            assert_eq!(n.domain(), e, "the domain is the extent");
            let verdict = (i < 10).then(|| {
                c.implicit
                    .check(&exact(&n))
                    .unwrap_or_else(|err| panic!("{err}"))
            });
            if let Some(off) = verdict.and_then(|v| v.worst) {
                let d = off.distance / c.scale;
                worst_on = worst_on.max(d);
                assert!(d < 4e-16, "{e:?}: off at {:?} by {d:e}", off.at);
            }
            let (u, v) = (e.x_interval(), e.y_interval());
            for _ in 0..20 {
                let uv = Point2::new(
                    inside(&mut rng, u.min(), u.max(), c.u.2),
                    inside(&mut rng, v.min(), v.max(), c.v.2),
                );
                let p = at(&n, uv);
                let d = dist(p, c.kind.point(uv).to_array().map(Dd::new)) / c.scale;
                let w = Point2::new(
                    wrap(uv.x, (c.u.0, c.u.1), pu),
                    wrap(uv.y, (c.v.0, c.v.1), pv),
                );
                let t = dist(p, c.implicit.point_at(&c.layout, (w.x, w.y))) / c.scale;
                worst_uv = worst_uv.max(d).max(t);
                assert!(d < 8e-16 && t < 8e-16, "{e:?} {uv:?}: {d:e} {t:e}");
            }
        }
    }
    eprintln!("to_nurbs: worst off {worst_on:e}, worst uv {worst_uv:e}");
}

#[test]
fn to_nurbs_structure() {
    let f = Frame::WORLD;
    let t = Torus::new(
        f,
        Length::from_mm(3.0),
        Length::from_mm(1.0),
        Sense::Forward,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let n = t
        .to_nurbs(&ext((0.0, 1.0), (0.0, 1.0)))
        .unwrap_or_else(|e| panic!("{e}"));
    let full = [
        0.0, 0.0, 0.0, 0.25, 0.25, 0.5, 0.5, 0.75, 0.75, 1.0, 1.0, 1.0,
    ];
    assert_eq!(n.knots(ParamAxis::U), full);
    assert_eq!(n.knots(ParamAxis::V), full);
    assert_eq!((n.degree(ParamAxis::U), n.degree(ParamAxis::V)), (2, 2));
    let w = n.weights().unwrap_or_else(|| panic!("rational"));
    assert!(w.points().iter().all(|&x| x > 0.0));
    // A quarter arc's middle weight is cos(pi/4) of its ends'.
    let r = w.get(1, 0).copied().unwrap_or(0.0) / w.get(0, 0).copied().unwrap_or(1.0);
    assert!((r - core::f64::consts::FRAC_1_SQRT_2).abs() < 1e-15, "{r}");
    // Off the breaks, one span per crossed arc: [0.1, 0.6] crosses 0.25 and 0.5.
    let n = t
        .to_nurbs(&ext((0.1, 0.6), (0.3, 0.4)))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        n.knots(ParamAxis::U),
        [0.1, 0.1, 0.1, 0.25, 0.25, 0.5, 0.5, 0.6, 0.6, 0.6]
    );
    assert_eq!(n.knots(ParamAxis::V), [0.3, 0.3, 0.3, 0.4, 0.4, 0.4]);
    // The sphere's poles are its control rows' ends, exactly.
    let s = Sphere::new(f, Length::from_mm(2.0), Sense::Forward).unwrap_or_else(|e| panic!("{e}"));
    let n = s
        .to_nurbs(&ext((0.0, 1.0), (0.0, 1.0)))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        n.knots(ParamAxis::V),
        [0.0, 0.0, 0.0, 0.5, 0.5, 1.0, 1.0, 1.0]
    );
    let net = n.points();
    for i in 0..net.nrows() {
        assert_eq!(net.get(i, 0).copied(), Some([0.0, 0.0, -2.0]));
        assert_eq!(net.get(i, net.ncols() - 1).copied(), Some([0.0, 0.0, 2.0]));
    }
    // A plane in an integer frame over integers is exactly on, and polynomial.
    let p = Plane::new(
        Frame::new(
            Point3::new(1.0, -2.0, 3.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap_or(Frame::WORLD),
    );
    let n = p
        .to_nurbs(&ext((-3.0, 5.0), (2.0, 7.0)))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(!n.is_rational());
    let implicit = Implicit::Plane {
        frame: tk(p.frame(), Sense::Forward),
    };
    assert!(
        implicit
            .check(&exact(&n))
            .unwrap_or_else(|e| panic!("{e}"))
            .on()
    );
}

#[test]
fn to_nurbs_is_refused_with_kinds() {
    let f = Frame::WORLD;
    let len = Length::from_mm;
    let kind = |e: SurfaceError| (e.kind(), e.name());
    let empty = Some((ErrorKind::Refused, "extent_empty"));
    let outside = Some((ErrorKind::Refused, "extent_outside_domain"));
    let cyl = Cylinder::new(f, len(1.0), Sense::Forward).unwrap_or_else(|e| panic!("{e}"));
    let period = 2.0 * PI;
    let go = |s: &dyn Fn(&Extent2d) -> Result<NurbsSurface<3>, SurfaceError>, e| {
        s(&e).map(|_| ()).map_err(kind).err()
    };
    let c = |e: &Extent2d| cyl.to_nurbs(e);
    assert_eq!(go(&c, ext((0.0, 0.0), (0.0, 1.0))), empty);
    assert_eq!(go(&c, ext((0.0, 1.0), (2.0, 2.0))), empty);
    assert_eq!(go(&c, ext((0.0, period * 1.01), (0.0, 1.0))), outside);
    assert_eq!(
        go(&c, ext((-0.3 * period, 0.7 * period), (0.0, 1.0))),
        None,
        "one turn anywhere"
    );
    let sph = Sphere::new(f, len(1.0), Sense::Forward).unwrap_or_else(|e| panic!("{e}"));
    let s = |e: &Extent2d| sph.to_nurbs(e);
    assert_eq!(go(&s, ext((0.0, 1.0), (-0.1, 0.5))), outside);
    assert_eq!(go(&s, ext((0.0, 1.0), (0.5, 1.1))), outside);
    for a in [0.4, -0.4] {
        let cone = Cone::new(f, len(1.0), a, Sense::Forward).unwrap_or_else(|e| panic!("{e}"));
        let apex = -1.0 / fmath::tan(a);
        let k = |e: &Extent2d| cone.to_nurbs(e);
        let (near, far) = if a > 0.0 {
            (apex, apex + 3.0)
        } else {
            (apex - 3.0, apex)
        };
        assert_eq!(
            go(&k, ext((0.0, 1.0), (near, far))),
            None,
            "{a}: to the apex"
        );
        assert_eq!(
            go(&k, ext((0.0, 1.0), (near - 0.5, far + 0.5))),
            outside,
            "{a}"
        );
    }
    let tor = Torus::new(f, len(3.0), len(1.0), Sense::Forward).unwrap_or_else(|e| panic!("{e}"));
    let t = |e: &Extent2d| tor.to_nurbs(e);
    assert_eq!(go(&t, ext((0.0, 1.0), (0.2, 1.3))), outside);
    let nurbs = AnySurface::Torus(tor)
        .to_nurbs(&ext((0.0, 1.0), (0.0, 1.0)))
        .unwrap_or_else(|e| panic!("{e}"));
    let any = AnySurface::Nurbs(std::sync::Arc::new(nurbs));
    let a = |e: &Extent2d| any.to_nurbs(e);
    assert_eq!(go(&a, ext((0.2, 0.2), (0.0, 1.0))), empty);
    assert_eq!(go(&a, ext((-0.1, 0.5), (0.0, 1.0))), outside);
    assert_eq!(go(&a, ext((0.0, 1.0), (0.5, 1.5))), outside);
}

#[test]
fn surface_to_nurbs_is_the_kinds_and_restricts_nurbs() {
    let mut rng = Rng::new(29);
    for c in cases(5) {
        let e = ext((c.u.0, c.u.1), (c.v.0, c.v.1));
        let n = c.kind.to_nurbs(&e).unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(c.kind.any().to_nurbs(&e).ok().as_ref(), Some(&n));
        let any = AnySurface::Nurbs(std::sync::Arc::new(n.clone()));
        assert_eq!(
            any.to_nurbs(&e).ok().as_ref(),
            Some(&n),
            "the whole domain is itself"
        );
        for sub in extents(&mut rng, &c).into_iter().skip(1).take(2) {
            let r = any
                .to_nurbs(&sub)
                .unwrap_or_else(|err| panic!("{sub:?}: {err}"));
            assert_eq!(r.domain(), sub);
            let (u, v) = (sub.x_interval(), sub.y_interval());
            for _ in 0..10 {
                let uv = Point2::new(
                    inside(&mut rng, u.min(), u.max(), c.u.2),
                    inside(&mut rng, v.min(), v.max(), c.v.2),
                );
                let d = (at(&r, uv) - at(&n, uv)).length() / c.scale;
                assert!(d < 1e-15, "{sub:?} {uv:?}: {d:e}");
            }
        }
    }
}

/// The rational quadratic `sum w_i B_i(s) P_i / sum w_i B_i(s)` at `s`, exactly.
fn bezier(points: &[[Rat; 3]; 3], weights: &[Rat; 3], s: &Rat) -> [Rat; 3] {
    let b = scx_testkit::ssi::bernstein();
    let mut num = [Rat::zero(), Rat::zero(), Rat::zero()];
    let mut den = Rat::zero();
    for i in 0..3 {
        let c = &b[i].eval(s) * &weights[i];
        den = &den + &c;
        for j in 0..3 {
            num[j] = &num[j] + &(&c * &points[i][j]);
        }
    }
    num.map(|x| x.checked_div(&den).unwrap())
}

/// WR-680 (63 §4.14): a plane's oblique section of a cylinder, an ellipse, has the exact
/// pcurve `pcurve_of` gives. The truth is the testkit's exact rational section on the
/// pin's NURBS cylinder, span by span: at each rational `s` its point (on the plane and the
/// cylinder exactly) and its uv `(u(s), H(s) / W(s))`, `u` in the pin's quarter-arc layout
/// (period `4 pi`, here `2 pi r`). The ellipse's parameter at the point is read off its
/// frame; the pcurve there is the truth's uv, `u` modulo the period.
#[test]
fn oblique_cylinder_sections_have_the_rational_uv() {
    use scx_curve::{Curve, Curve2, Ellipse};
    use scx_num::extent1d::Extent1d;
    use scx_surface::pcurve_of;
    use scx_testkit::ssi::cylinder_section;

    let mut rng = Rng::new(0x9c_0680);
    let mut n = 0;
    for _ in 0..24 {
        let f = frame(&mut rng);
        let cs = sense(&mut rng);
        let r = 0.5 + 4.0 * rng.next_f64();
        let cyl = Cylinder::new(f, Length::from_mm(r), cs).unwrap();
        let tf = tk(&f, cs);
        let z = *f.z() * cs.sign();
        // A plane through a point near the axis, its normal tilted off the axis.
        let pf = {
            let g = sample::frame(&mut rng, 10.0);
            let normal = Vec3::new(g.z[0], g.z[1], g.z[2]);
            let normal = Vec3::from((z * (0.3 + rng.next_f64()) + normal).normalized().unwrap());
            let origin = f.origin() + z * (5.0 * rng.next_signed_unit());
            let x = z.cross(normal).normalized().unwrap();
            Frame::new(origin, normal, Vec3::from(x)).unwrap()
        };
        let a = |v: Vec3| [v.x, v.y, v.z];
        let po = pf.origin();
        let tplane = analytic::Plane {
            frame: analytic::Frame {
                origin: [po.x, po.y, po.z],
                x: a(*pf.x()),
                y: a(*pf.y()),
                z: [0.0; 3],
            },
        };
        // The ellipse: centre where the plane meets the axis, semi-axes `r` across the
        // axis (along the plane's x) and `r / cos tilt` up the plane (its y).
        let nz = pf.z().dot(z);
        let centre = f.origin() + z * ((po - f.origin()).dot(*pf.z()) / nz);
        let es = sense(&mut rng);
        let fe = Frame::new(centre, *pf.z(), *pf.x()).unwrap();
        let e = Ellipse::new(fe, Length::from_mm(r), Length::from_mm(r / nz.abs()), es).unwrap();
        let curve = Curve::Ellipse(e);
        let size = 10.0 + r / nz.abs();
        let lo = 6.0 * rng.next_signed_unit();
        let over = Extent1d::new(lo, lo + 7.0).unwrap();
        let surface = AnySurface::Cylinder(cyl);
        let pc = pcurve_of(
            &curve,
            &surface,
            over,
            Resolution::new(Length::from_mm(2e-9 * size)).unwrap(),
        )
        .unwrap();
        assert!(matches!(pc, Curve2::AngleWave(_)), "{pc:?}");
        let period = 2.0 * PI * r;
        let scale = period / (4.0 * PI);
        let spans = cylinder_section(
            &analytic::Cylinder {
                frame: tf,
                radius: r,
            },
            &tplane,
        )
        .unwrap();
        for span in &spans {
            for j in 0..8 {
                let s = Rat::from_f64(if j == 0 { 0.0 } else { rng.next_f64() }).unwrap();
                let q = bezier(&span.points, &span.weights, &s).map(|x| x.to_f64());
                let q = Point3::new(q[0], q[1], q[2]);
                let u = span.u.eval(&s).to_f64() * scale;
                let v = span
                    .v_num
                    .eval(&s)
                    .checked_div(&span.weight.eval(&s))
                    .unwrap()
                    .to_f64();
                // The ellipse's parameter of `q`, moved into `over`.
                let d = q - centre;
                let (cx, sy) = (d.dot(*fe.x()) / r, d.dot(*fe.y()) * nz.abs() / r);
                let t0 = fmath::atan2(sy * es.sign(), cx);
                let t = t0 + ((lo - t0) / (2.0 * PI)).ceil() * 2.0 * PI;
                assert!((curve.point(t) - q).length() < 1e-12 * size);
                let got = pc.point(t);
                let du = (got.x - u) / period;
                assert!(
                    (du - du.round()).abs() * period < 1e-12 * size,
                    "u {} vs {u} at {t}",
                    got.x
                );
                assert!((got.y - v).abs() < 1e-12 * size, "v {} vs {v}", got.y);
                n += 1;
            }
        }
    }
    assert_eq!(n, 24 * 4 * 8);
}
