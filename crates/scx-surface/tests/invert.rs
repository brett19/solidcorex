// Written for solidcorex. Apache-2.0.
//! The analytic kinds' nearest points against the testkit's references (63 §4.2, X8):
//! - the cylinder's and sphere's feet against `analytic`'s double-double closest points, in
//!   distance, point and STEP angles;
//! - the cone's and torus's against T3's certified stationary points (`project`): the
//!   nearest of every one, so the global nearest, not a local one;
//! - every kind against a dense uv sample of itself: no sample nearer than the foot;
//! - the sets of feet (axis, centre, apex, tube circle) on exactly placed queries;
//! - `invert`'s wrap and its inside or outside against a domain.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic
)]

use core::f64::consts::PI;

use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::hash::Rng;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Closest, Cone, Cylinder, Foot, Inversion, Plane, Sphere, Torus};
use scx_testkit::analytic;
use scx_testkit::dd::Dd;
use scx_testkit::project::{Locus, Nappes};
use scx_testkit::sample;

const SEEDS: u64 = 12;
const QUERIES: usize = 25;
/// Relative to the model's size (about 20): a few ulps of the coordinates.
const TOL: f64 = 1e-13;

fn frame(rng: &mut Rng) -> Frame {
    let f = sample::frame(rng, 10.0);
    let v = |a: [f64; 3]| Vec3::new(a[0], a[1], a[2]);
    let o = f.origin;
    Frame::new(Point3::new(o[0], o[1], o[2]), v(f.z), v(f.x)).unwrap_or(Frame::WORLD)
}

fn sense(rng: &mut Rng) -> Sense {
    if rng.next_f64() < 0.5 {
        Sense::Forward
    } else {
        Sense::Reversed
    }
}

/// The testkit frame of a kind: its x, y and effective z, `z` turned over by `flip`.
fn tk(frame: &Frame, sense: Sense, flip: f64) -> analytic::Frame {
    let a = |v: Vec3| [v.x, v.y, v.z];
    let o = frame.origin();
    analytic::Frame {
        origin: [o.x, o.y, o.z],
        x: a(*frame.x()),
        y: a(*frame.y()),
        z: a(*frame.z() * (sense.sign() * flip)),
    }
}

fn arr(p: Point3) -> [f64; 3] {
    [p.x, p.y, p.z]
}

fn dist(p: Point3, q: [Dd; 3]) -> f64 {
    let d = [
        (Dd::new(p.x) - q[0]).to_f64(),
        (Dd::new(p.y) - q[1]).to_f64(),
        (Dd::new(p.z) - q[2]).to_f64(),
    ];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// A query about the frame's origin, within `reach` of it; one in four near the axis.
fn query(rng: &mut Rng, f: &Frame, reach: f64) -> Point3 {
    let mut c = || reach * rng.next_signed_unit();
    let (x, y, z) = (c(), c(), c());
    let (x, y) = if rng.next_f64() < 0.25 {
        (x * 1e-3, y * 1e-3)
    } else {
        (x, y)
    };
    f.origin() + *f.x() * x + *f.y() * y + *f.z() * z
}

fn one(c: Closest) -> Foot<Point2> {
    match c {
        Closest::One(f) => f,
        other => panic!("a set of feet off every axis: {other:?}"),
    }
}

/// `a - b` reduced to `(-pi, pi]`.
fn angle_gap(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(2.0 * PI);
    if d > PI { d - 2.0 * PI } else { d }
}

/// No point of a `n x n` grid over `[u0, u1] x [v0, v1]` is nearer than `foot`.
fn no_sample_nearer(
    point: impl Fn(Point2) -> Point3,
    p: Point3,
    foot: &Foot<Point2>,
    (u0, u1): (f64, f64),
    (v0, v1): (f64, f64),
) {
    let n = 120;
    for i in 0..=n {
        for j in 0..=n {
            let uv = Point2::new(
                u0 + (u1 - u0) * f64::from(i) / f64::from(n),
                v0 + (v1 - v0) * f64::from(j) / f64::from(n),
            );
            let d = (point(uv) - p).length();
            assert!(
                d >= foot.distance - TOL * 20.0,
                "sample {uv:?} at {d} beats the foot {foot:?}"
            );
        }
    }
}

fn check_foot(point: impl Fn(Point2) -> Point3, p: Point3, f: &Foot<Point2>) {
    assert_eq!(
        f.point,
        point(f.at),
        "the foot's point is the kind's point(at)"
    );
    assert_eq!(f.distance, (p - f.point).length());
}

#[test]
fn plane_feet_are_projections() {
    let mut rng = Rng::new(0x51_04_01);
    for _ in 0..SEEDS {
        let f = frame(&mut rng);
        let s = Plane::new(f);
        for _ in 0..QUERIES {
            let p = query(&mut rng, &f, 8.0);
            let foot = one(s.closest(p));
            check_foot(|uv| s.point(uv), p, &foot);
            let h = (p - f.origin()).dot(*f.z());
            assert!((foot.distance - h.abs()).abs() <= TOL * 20.0);
            assert!((foot.point + *f.z() * h - p).length() <= TOL * 20.0);
            no_sample_nearer(|uv| s.point(uv), p, &foot, (-20.0, 20.0), (-20.0, 20.0));
        }
    }
}

#[test]
fn cylinder_and_sphere_feet_match_the_references() {
    let mut rng = Rng::new(0x51_04_02);
    for _ in 0..SEEDS {
        let (f, sn) = (frame(&mut rng), sense(&mut rng));
        let r = 0.5 + 4.0 * rng.next_f64();
        let cyl = Cylinder::new(f, Length::from_mm(r), sn).unwrap();
        let cyl_t = analytic::Cylinder {
            frame: tk(&f, sn, 1.0),
            radius: r,
        };
        let sph = Sphere::new(f, Length::from_mm(r), sn).unwrap();
        let sph_t = analytic::Sphere {
            frame: tk(&f, sn, 1.0),
            radius: r,
        };
        for _ in 0..QUERIES {
            let p = query(&mut rng, &f, 8.0);

            let foot = one(cyl.closest(p));
            check_foot(|uv| cyl.point(uv), p, &foot);
            let rho = (p - f.origin()).cross(*f.z()).length();
            // As for the sphere below: the reference's Newton can miss its step by the axis.
            match cyl_t.closest(arr(p)) {
                Some(t) => {
                    assert!((foot.distance - t.distance.to_f64()).abs() <= TOL * 20.0);
                    assert!(dist(foot.point, t.point) <= TOL * 20.0);
                    let st = cyl.to_step(foot.at);
                    let a_tol = 1e-13 * 20.0 / rho.min(1.0);
                    assert!(angle_gap(st.x, t.param.0.to_f64()).abs() <= a_tol);
                    assert!((st.y - t.param.1.to_f64()).abs() <= TOL * 20.0);
                }
                None => {
                    assert!(rho < 0.02, "{p:?}");
                    assert!((foot.distance - (r - rho)).abs() <= TOL * 20.0);
                }
            }
            let period = 2.0 * PI * r;
            no_sample_nearer(|uv| cyl.point(uv), p, &foot, (0.0, period), (-20.0, 20.0));

            let foot = one(sph.closest(p));
            check_foot(|uv| sph.point(uv), p, &foot);
            let rho = (p - f.origin()).cross(*f.z()).length();
            // The reference's Newton can miss its 1e-31 step by a pole, where `u` is poorly
            // conditioned: then only the sample and the radial direction speak.
            match sph_t.closest(arr(p)) {
                Some(t) => {
                    assert!((foot.distance - t.distance.to_f64()).abs() <= TOL * 20.0);
                    assert!(dist(foot.point, t.point) <= TOL * 20.0);
                    let st = sph.to_step(foot.at);
                    // A near-axis query's angle is only as good as its radial offset.
                    let a_tol = 1e-13 * 20.0 / rho.min(1.0);
                    assert!(angle_gap(st.x, t.param.0.to_f64()).abs() <= a_tol);
                    assert!((st.y - t.param.1.to_f64()).abs() <= 1e-12);
                }
                None => {
                    let c = f.origin();
                    assert!(
                        rho < 0.02,
                        "{p:?} rho {rho} r {r} centre {}",
                        (p - c).length()
                    );
                    let radial = (p - c).length();
                    assert!((foot.distance - (radial - r).abs()).abs() <= TOL * 20.0);
                }
            }
            no_sample_nearer(|uv| sph.point(uv), p, &foot, (0.0, 1.0), (0.0, 1.0));
        }
    }
}

#[test]
fn cone_feet_are_the_certified_nearest() {
    let mut rng = Rng::new(0x51_04_03);
    for _ in 0..SEEDS * 2 {
        let (f, sn) = (frame(&mut rng), sense(&mut rng));
        let r = 0.5 + 4.0 * rng.next_f64();
        let a = (0.1 + 1.3 * rng.next_f64()) * if rng.next_f64() < 0.5 { 1.0 } else { -1.0 };
        let cone = Cone::new(f, Length::from_mm(r), a, sn).unwrap();
        // T3 takes a positive semi-angle: a negative one is the same nappe about `-z`.
        let t = analytic::Cone {
            frame: tk(&f, sn, a.signum()),
            radius: r,
            semi_angle: a.abs(),
        };
        let k = fmath::tan(a);
        let apex = -r / k;
        let reach = 8.0 + apex.abs();
        for _ in 0..QUERIES {
            let p = query(&mut rng, &f, reach);
            let foot = match cone.closest(p) {
                Closest::One(foot) => foot,
                Closest::AlongU { v, distance } => {
                    // Past the apex: the apex itself.
                    assert_eq!(v, apex);
                    let at = Point2::new(0.0, v);
                    assert_eq!(distance, (cone.point(at) - p).length());
                    Foot {
                        at,
                        point: cone.point(at),
                        distance,
                    }
                }
                other => panic!("{other:?}"),
            };
            check_foot(|uv| cone.point(uv), p, &foot);
            let proj = t.stationary(arr(p), Nappes::One).unwrap();
            let near = proj.nearest_locus().unwrap();
            let d = near.distance().to_dd().to_f64();
            assert!(
                (foot.distance - d).abs() <= TOL * reach,
                "{p:?}: {} against {d}",
                foot.distance
            );
            if let Locus::Point(s) = near {
                assert!(dist(foot.point, s.point) <= 1e-12 * reach);
            }
            let (lo, hi) = if k > 0.0 {
                (apex, apex + 3.0 * reach)
            } else {
                (apex - 3.0 * reach, apex)
            };
            no_sample_nearer(|uv| cone.point(uv), p, &foot, (0.0, 1.0), (lo, hi));
        }
    }
}

#[test]
fn torus_feet_are_the_certified_nearest() {
    let mut rng = Rng::new(0x51_04_04);
    for _ in 0..SEEDS * 2 {
        let (f, sn) = (frame(&mut rng), sense(&mut rng));
        let big = 2.0 + 3.0 * rng.next_f64();
        let small = (0.1 + 0.8 * rng.next_f64()) * big;
        let tor = Torus::new(f, Length::from_mm(big), Length::from_mm(small), sn).unwrap();
        let t = analytic::Torus {
            frame: tk(&f, sn, 1.0),
            major: big,
            minor: small,
        };
        for _ in 0..QUERIES {
            let p = query(&mut rng, &f, 8.0);
            let foot = one(tor.closest(p));
            check_foot(|uv| tor.point(uv), p, &foot);
            let proj = t.stationary(arr(p)).unwrap();
            let near = proj.nearest_locus().unwrap();
            let d = near.distance().to_dd().to_f64();
            assert!((foot.distance - d).abs() <= TOL * 20.0, "{p:?}");
            let s = near.point().unwrap();
            assert!(dist(foot.point, s.point) <= 1e-12 * 20.0);
            let st = tor.to_step(foot.at);
            assert!(angle_gap(st.y, s.param.1.mid.to_f64()).abs() <= 1e-11);
            no_sample_nearer(|uv| tor.point(uv), p, &foot, (0.0, 1.0), (0.0, 1.0));
        }
    }
}

/// A frame at `o` with the world's axes, so a query's local coordinates are exact.
fn at(o: Point3) -> Frame {
    Frame::new(o, Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0)).unwrap()
}

#[test]
fn the_sets_of_feet() {
    let o = Point3::new(1.5, -2.25, 3.0);
    let up = |h: f64| o + Vec3::new(0.0, 0.0, h);

    let cyl = Cylinder::new(at(o), Length::from_mm(2.0), Sense::Forward).unwrap();
    assert_eq!(
        cyl.closest(up(1.25)),
        Closest::AlongU {
            v: 1.25,
            distance: 2.0
        }
    );

    let sph = Sphere::new(at(o), Length::from_mm(2.0), Sense::Forward).unwrap();
    assert_eq!(sph.closest(o), Closest::Every { distance: 2.0 });
    assert_eq!(
        sph.closest(up(5.0)),
        Closest::AlongU {
            v: 1.0,
            distance: 3.0
        }
    );
    assert_eq!(
        sph.closest(up(-0.5)),
        Closest::AlongU {
            v: 0.0,
            distance: 1.5
        }
    );
    // Reversed, the poles swap ends of `v`.
    let rev = Sphere::new(at(o), Length::from_mm(2.0), Sense::Reversed).unwrap();
    assert_eq!(
        rev.closest(up(5.0)),
        Closest::AlongU {
            v: 0.0,
            distance: 3.0
        }
    );

    // A 45 degree cone of radius 2 at v = 0: the apex at v = -2.
    let cone = Cone::new(at(o), Length::from_mm(2.0), PI / 4.0, Sense::Forward).unwrap();
    let k = fmath::tan(PI / 4.0);
    let apex = -2.0 / k;
    // Inside the opening the feet are a circle at v = (h - 2k) / (k^2 + 1).
    match cone.closest(up(3.0)) {
        Closest::AlongU { v, distance } => {
            assert!((v - 0.5).abs() < 1e-15);
            assert!((distance - 2.5 * 2f64.sqrt()).abs() < 1e-14);
        }
        other => panic!("{other:?}"),
    }
    // Below the apex, on the axis and off it within the opposite opening: the apex.
    for p in [up(-3.0), o + Vec3::new(0.1, 0.2, -6.0)] {
        match cone.closest(p) {
            Closest::AlongU { v, distance } => {
                assert_eq!(v, apex);
                assert_eq!(distance, (cone.point(Point2::new(0.0, v)) - p).length());
            }
            other => panic!("{other:?}"),
        }
    }

    let tor = Torus::new(
        at(o),
        Length::from_mm(3.0),
        Length::from_mm(1.0),
        Sense::Forward,
    )
    .unwrap();
    // On the axis: the circle seen from the axis at angle atan2(h, -R) about each centre.
    match tor.closest(up(4.0)) {
        Closest::AlongU { v, distance } => {
            assert!((tor.to_step(Point2::new(0.0, v)).y - fmath::atan2(4.0, -3.0)).abs() < 1e-14);
            assert!((distance - 4.0).abs() < 1e-14);
        }
        other => panic!("{other:?}"),
    }
    match tor.closest(o) {
        Closest::AlongU { v, distance } => {
            assert_eq!(v, 0.5);
            assert_eq!(distance, 2.0);
        }
        other => panic!("{other:?}"),
    }
    // On the tube's centre circle: its meridian circle.
    match tor.closest(o + Vec3::new(0.0, 3.0, 0.0)) {
        Closest::AlongV { u, distance } => {
            assert_eq!(u, 0.25);
            assert!((distance - 1.0).abs() < 1e-15);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn invert_places_the_foot_against_the_domain() {
    let o = Point3::new(1.5, -2.25, 3.0);
    let r = 2.0;
    let cyl = Cylinder::new(at(o), Length::from_mm(r), Sense::Forward).unwrap();
    let period = 2.0 * PI * r;
    // A query at angle pi/2 (u a quarter period), height 1.
    let p = o + Vec3::new(0.0, 5.0, 1.0);
    let u0 = period / 4.0;
    let near = one(cyl.closest(p));
    assert!((near.at.x - u0).abs() < 1e-14);
    assert_eq!(near.at.y, 1.0);

    // Every u, and the default domain: inside, unmoved.
    assert_eq!(cyl.invert(p, None), Inversion::Inside(near));
    // A domain two periods on: the foot wraps into it.
    let d = Extent2d::from_bounds(2.0 * period, -5.0, 2.5 * period, 5.0).unwrap();
    let Inversion::Inside(f) = cyl.invert(p, Some(&d)) else {
        panic!()
    };
    assert!((f.at.x - (2.0 * period + u0)).abs() < 1e-12);
    assert!((f.distance - near.distance).abs() < 1e-14);
    assert_eq!(f.point, cyl.point(f.at));
    // A domain short of the foot's u, or its v: outside, the foot still the nearest.
    for d in [
        Extent2d::from_bounds(0.3 * period, -5.0, 0.9 * period, 5.0).unwrap(),
        Extent2d::from_bounds(0.0, 2.0, period, 5.0).unwrap(),
    ] {
        let Inversion::Outside(f) = cyl.invert(p, Some(&d)) else {
            panic!("{d:?}")
        };
        assert!((f.distance - near.distance).abs() < 1e-14);
        assert!(f.at.x >= d.min().x && f.at.x < d.min().x + period);
    }
    // A domain of a whole period holds every u.
    let d = Extent2d::from_bounds(0.5 * period, -5.0, 1.5 * period, 5.0).unwrap();
    assert!(cyl.invert(p, Some(&d)).is_inside());

    // On the axis, the circle's foot at the domain's start in u.
    let d = Extent2d::from_bounds(0.3 * period, -5.0, 0.6 * period, 5.0).unwrap();
    let Inversion::Inside(f) = cyl.invert(o, Some(&d)) else {
        panic!()
    };
    assert_eq!(f.at, Point2::new(0.3 * period, 0.0));
    assert_eq!(f.distance, r);

    // The sphere's centre: the domain's start.
    let sph = Sphere::new(at(o), Length::from_mm(r), Sense::Forward).unwrap();
    let d = Extent2d::from_bounds(0.25, 0.2, 0.5, 0.7).unwrap();
    let Inversion::Inside(f) = sph.invert(o, Some(&d)) else {
        panic!()
    };
    assert_eq!(f.at, Point2::new(0.25, 0.2));

    // The plane's projection outside its domain stays the projection.
    let pl = Plane::new(at(o));
    let p = o + Vec3::new(4.0, 1.0, -2.0);
    let d = Extent2d::from_bounds(-1.0, -1.0, 1.0, 1.0).unwrap();
    let Inversion::Outside(f) = pl.invert(p, Some(&d)) else {
        panic!()
    };
    assert_eq!(f.at, Point2::new(4.0, 1.0));
    assert_eq!(f.distance, 2.0);

    // A non-finite query is never inside.
    let nan = Point3::new(f64::NAN, 0.0, 0.0);
    assert!(!pl.invert(nan, None).is_inside());
    assert!(!cyl.invert(nan, None).is_inside());
    assert!(!sph.invert(nan, None).is_inside());
    let tor = Torus::new(
        at(o),
        Length::from_mm(3.0),
        Length::from_mm(1.0),
        Sense::Forward,
    )
    .unwrap();
    assert!(!tor.invert(nan, None).is_inside());
    let cone = Cone::new(at(o), Length::from_mm(2.0), 0.5, Sense::Forward).unwrap();
    assert!(!cone.invert(nan, None).is_inside());
}
