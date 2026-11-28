// Written for solidcorex. Apache-2.0.
//! [`recognize`] and [`recognize_written`] (research 74 §5.3, SU7): the pin's near-zero and
//! near-right cones (kernel cases 4005, 4006) rebuilt as truths, the exact rational NURBS
//! form of each kind, non-rational approximations within and beyond the accuracy, near
//! misses, the written sizes the kinds refuse, a generic NURBS rejected, and the deviation
//! bounded below by samples of the true distance.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic
)]

use std::f64::consts::FRAC_PI_2;

use scx_num::budget::{Budget, Stop};
use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Accuracy, Length};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{Approach, KnotVector};
use scx_nurbs_surface::{Net, NurbsSurface};
use scx_surface::{
    Cone, Cylinder, Plane, Recognized, Sphere, Surface, SurfaceError, Torus, Written, recognize,
    recognize_written,
};

const ABOVE: [Approach; 2] = [Approach::Above, Approach::Above];

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn acc(x: f64) -> Accuracy {
    Accuracy::new(mm(x)).unwrap()
}

fn frame(o: [f64; 3], z: [f64; 3], x: [f64; 3]) -> Frame {
    Frame::new(
        Point3::new(o[0], o[1], o[2]),
        Vec3::new(z[0], z[1], z[2]),
        Vec3::new(x[0], x[1], x[2]),
    )
    .unwrap()
}

fn tilted() -> Frame {
    frame([1.5, -2.0, 0.7], [0.3, -0.4, 0.86], [1.0, 0.2, 0.0])
}

fn ext(u: (f64, f64), v: (f64, f64)) -> Extent2d {
    Extent2d::from_bounds(u.0, v.0, u.1, v.1).unwrap()
}

fn rec(s: &Surface, over: &Extent2d, a: f64) -> Option<Recognized> {
    recognize(s, over, acc(a), &mut Budget::unlimited(), &Stop::new()).unwrap()
}

/// Points of `s` over `over`, a `n x n` grid with the edges.
fn grid(s: &Surface, over: &Extent2d, n: usize) -> Vec<(Point2, Point3)> {
    let (u, v) = (over.x_interval(), over.y_interval());
    let mut out = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let uv = Point2::new(
                u.min() + u.length() * i as f64 / (n - 1) as f64,
                v.min() + v.length() * j as f64 / (n - 1) as f64,
            );
            out.push((uv, s.point(uv)));
        }
    }
    out
}

/// The distance of `p` from the analytic surface `s`, in closed form.
fn dist(s: &Surface, p: Point3) -> f64 {
    let local = |f: &Frame| {
        let w = p - f.origin();
        let z = w.dot(*f.z());
        ((w - *f.z() * z).length(), z, w)
    };
    match s {
        Surface::Plane(k) => (p - k.frame().origin()).dot(*k.frame().z()).abs(),
        Surface::Sphere(k) => ((p - k.frame().origin()).length() - k.radius().mm()).abs(),
        Surface::Cylinder(k) => (local(k.frame()).0 - k.radius().mm()).abs(),
        Surface::Cone(k) => {
            let (rho, z, _) = local(k.frame());
            let t = fmath::tan(k.half_angle());
            (rho - k.radius().mm() - t * z).abs() * fmath::cos(k.half_angle())
        }
        Surface::Torus(k) => {
            let (rho, z, _) = local(k.frame());
            let big = k.major().mm();
            (((rho - big) * (rho - big) + z * z).sqrt() - k.minor().mm()).abs()
        }
        _ => panic!("not analytic: {s:?}"),
    }
}

/// Asserts `r` is within its deviation of `given` (sampled), that the deviation is at most
/// `a`, and that the normals agree at the middle of `over`.
fn check(given: &Surface, over: &Extent2d, r: &Recognized, a: f64) -> f64 {
    let dev = r.deviation.mm();
    assert!(dev <= a, "deviation {dev} above {a}");
    let mut seen = 0.0_f64;
    for (_, p) in grid(given, over, 33) {
        seen = seen.max(dist(&r.surface, p));
    }
    assert!(
        seen <= dev * (1.0 + 1e-9) + 1e-12,
        "sampled {seen} above {dev}"
    );
    let mid = Point2::new(over.x_interval().mid(), over.y_interval().mid());
    let n0 = given.normal(mid, ABOVE).unwrap();
    let p = given.point(mid);
    let (_, uv) = foot_normal(&r.surface, p);
    assert!(uv.dot(*n0) > 0.0, "normals oppose: {:?}", r.surface);
    seen
}

/// The recognised surface's normal at the foot of `p`, by inversion.
fn foot_normal(s: &Surface, p: Point3) -> (Point2, Vec3) {
    let inv = s
        .invert(p, None, None, &mut Budget::unlimited(), &Stop::new())
        .unwrap();
    let uv = inv.foot().at;
    (uv, *s.normal(uv, ABOVE).unwrap())
}

/// A NURBS surface and its domain.
fn nurbs(n: NurbsSurface<3>) -> (Surface, Extent2d) {
    let d = n.domain();
    (Surface::Nurbs(std::sync::Arc::new(n)), d)
}

// ---------------------------------------------------------------- the pin's cases

/// Kernel case 4005: a cone of semi-angle `8e-12`, radius 5, over a height of 10, is the
/// cylinder of radius `5 + 4e-11` within `4e-11`.
#[test]
fn near_zero_cone_is_a_cylinder() {
    let f = frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let cone = Surface::Cone(Cone::new(f, mm(5.0), 8e-12, Sense::Forward).unwrap());
    let over = ext((0.0, 1.0), (0.0, 10.0));
    let r = rec(&cone, &over, 1e-6).unwrap();
    let Surface::Cylinder(c) = &r.surface else {
        panic!("{:?}", r.surface)
    };
    let k = fmath::tan(8e-12_f64);
    assert!((c.radius().mm() - (5.0 + 5.0 * k)).abs() <= 1e-14);
    assert!(r.deviation.mm() >= 5.0 * k * (1.0 - 1e-6) && r.deviation.mm() <= 4.1e-11);
    check(&cone, &over, &r, 1e-6);
}

/// Kernel case 4006: a cone of semi-angle `pi/2 - 7e-12` at height 3, radius 4 to 10, is the
/// plane at height `3 + 2.1e-11` facing `-z`, its normal's side.
#[test]
fn near_right_cone_is_a_plane() {
    let f = frame([0.0, 0.0, 3.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let alpha = 1.570_796_326_787_896_6;
    let cone = Cone::new(f, mm(4.0), alpha, Sense::Forward).unwrap();
    let k = fmath::tan(alpha);
    let over = ext((0.0, 1.0), (0.0, 6.0 / k));
    let s = Surface::Cone(cone);
    let r = rec(&s, &over, 1e-6).unwrap();
    let Surface::Plane(p) = &r.surface else {
        panic!("{:?}", r.surface)
    };
    assert!((p.frame().origin().z - 3.0).abs() <= 1e-10);
    assert!((p.frame().z().z + 1.0).abs() <= 1e-15);
    assert!(r.deviation.mm() <= 2.2e-11);
    check(&s, &over, &r, 1e-6);
}

/// The cone's closed forms switch exactly at their bounds: a cylinder just above the radius
/// half-range, the cone itself just below; and a plane needs the height half-range.
#[test]
fn cone_thresholds() {
    let s = Surface::Cone(Cone::new(tilted(), mm(5.0), 1e-4, Sense::Reversed).unwrap());
    let over = ext((0.05, 0.3), (-2.0, 8.0));
    let half = fmath::tan(1e-4_f64) * 5.0;
    let r = rec(&s, &over, half * (1.0 + 1e-6)).unwrap();
    assert!(matches!(r.surface, Surface::Cylinder(_)), "{:?}", r.surface);
    check(&s, &over, &r, half * (1.0 + 1e-6));
    let r = rec(&s, &over, half * (1.0 - 1e-6)).unwrap();
    assert_eq!(r.surface, s);
    assert_eq!(r.deviation, Length::ZERO);
    // A short band of a steep cone is a plane once the accuracy covers half its height.
    let steep = Surface::Cone(Cone::new(tilted(), mm(5.0), 1.5, Sense::Forward).unwrap());
    let band = ext((0.0, 0.2), (0.0, 1e-3));
    let r = rec(&steep, &band, 5.0001e-4).unwrap();
    assert!(matches!(r.surface, Surface::Plane(_)), "{:?}", r.surface);
    check(&steep, &band, &r, 5.0001e-4);
    let r = rec(&steep, &band, 4.9999e-4).unwrap();
    assert!(matches!(r.surface, Surface::Cone(_)), "{:?}", r.surface);
}

/// The other analytic kinds are themselves.
#[test]
fn analytic_kinds_are_themselves() {
    let s = Surface::Sphere(Sphere::new(tilted(), mm(3.0), Sense::Forward).unwrap());
    let over = ext((0.0, 0.3), (0.3, 0.6));
    let r = rec(&s, &over, 1e-6).unwrap();
    assert_eq!(r.surface, s);
    assert_eq!(r.deviation, Length::ZERO);
}

// ---------------------------------------------------------------- written sizes

#[test]
fn written_sizes() {
    let f = tilted();
    let r = recognize_written(
        &Written::Cone {
            frame: f,
            radius: mm(2.0),
            half_angle: 0.0,
            sense: Sense::Reversed,
        },
        acc(1e-6),
    )
    .unwrap();
    assert_eq!(
        r.surface,
        Surface::Cylinder(Cylinder::new(f, mm(2.0), Sense::Reversed).unwrap())
    );
    assert_eq!(r.deviation, Length::ZERO);
    let torus = |major: f64, minor: f64| Written::Torus {
        frame: f,
        major: mm(major),
        minor: mm(minor),
        sense: Sense::Forward,
    };
    let sphere = Surface::Sphere(Sphere::new(f, mm(2.0), Sense::Forward).unwrap());
    let r = recognize_written(&torus(0.0, 2.0), acc(1e-6)).unwrap();
    assert_eq!((r.surface, r.deviation), (sphere.clone(), Length::ZERO));
    // A spindle torus is the sphere of its minor radius within its major radius.
    let r = recognize_written(&torus(1e-7, 2.0), acc(1e-6)).unwrap();
    assert_eq!(r.surface, sphere);
    assert!(r.deviation.mm() >= 1e-7 && r.deviation.mm() <= 1e-7 * (1.0 + 1e-15));
    assert!(matches!(
        recognize_written(&torus(0.5, 2.0), acc(1e-6)),
        Err(SurfaceError::SelfIntersecting { .. })
    ));
    let r = recognize_written(&torus(5.0, 2.0), acc(1e-6)).unwrap();
    assert!(matches!(r.surface, Surface::Torus(_)));
    let r = recognize_written(
        &Written::Cone {
            frame: f,
            radius: mm(2.0),
            half_angle: 0.3,
            sense: Sense::Forward,
        },
        acc(1e-6),
    )
    .unwrap();
    assert!(matches!(r.surface, Surface::Cone(_)));
}

// ---------------------------------------------------------------- exact NURBS forms

fn exact_cases() -> Vec<(&'static str, Surface, Extent2d)> {
    let f = tilted();
    vec![
        (
            "plane",
            Surface::Plane(Plane::new(f)),
            ext((-3.0, 4.0), (-1.0, 2.0)),
        ),
        (
            "sphere",
            Surface::Sphere(Sphere::new(f, mm(3.0), Sense::Forward).unwrap()),
            ext((0.05, 0.3), (0.3, 0.7)),
        ),
        (
            "cylinder",
            Surface::Cylinder(Cylinder::new(f, mm(2.5), Sense::Reversed).unwrap()),
            ext((0.2, 3.5), (-1.0, 4.0)),
        ),
        (
            "cone",
            Surface::Cone(Cone::new(f, mm(2.0), 0.4, Sense::Forward).unwrap()),
            ext((0.0, 0.4), (-1.0, 3.0)),
        ),
        (
            "torus",
            Surface::Torus(Torus::new(f, mm(5.0), mm(1.5), Sense::Forward).unwrap()),
            ext((0.1, 0.3), (-0.1, 0.3)),
        ),
    ]
}

fn same_kind(a: &Surface, b: &Surface) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// Each kind's exact rational form is that kind, its sizes to `1e-9`.
#[test]
fn exact_rational_forms() {
    for (name, s, over) in exact_cases() {
        let (n, over_n) = nurbs(s.to_nurbs(&over).unwrap());
        let r = rec(&n, &over_n, 1e-6).unwrap_or_else(|| panic!("{name}: None"));
        assert!(same_kind(&r.surface, &s), "{name}: {:?}", r.surface);
        check(&n, &over_n, &r, 1e-6);
        let size = |s: &Surface| match s {
            Surface::Sphere(k) => vec![k.radius().mm()],
            Surface::Cylinder(k) => vec![k.radius().mm()],
            Surface::Cone(k) => vec![k.half_angle()],
            Surface::Torus(k) => vec![k.major().mm(), k.minor().mm()],
            _ => vec![],
        };
        for (a, b) in size(&r.surface).iter().zip(size(&s)) {
            assert!((a - b).abs() <= 1e-9, "{name}: {a} vs {b}");
        }
        // The same twice, to the bit.
        let again = rec(&n, &over_n, 1e-6).unwrap();
        assert_eq!(again, r, "{name}");
        assert_eq!(again.deviation.mm().to_bits(), r.deviation.mm().to_bits());
    }
}

/// A cone of a small angle as NURBS: a cylinder where the accuracy covers its radius range,
/// a cone where it does not.
#[test]
fn small_cone_nurbs() {
    let s = Surface::Cone(Cone::new(tilted(), mm(5.0), 1e-5, Sense::Forward).unwrap());
    let (n, over) = nurbs(s.to_nurbs(&ext((0.0, 0.3), (0.0, 10.0))).unwrap());
    let r = rec(&n, &over, 1e-3).unwrap();
    assert!(matches!(r.surface, Surface::Cylinder(_)), "{:?}", r.surface);
    check(&n, &over, &r, 1e-3);
    let r = rec(&n, &over, 1e-7).unwrap();
    assert!(matches!(r.surface, Surface::Cone(_)), "{:?}", r.surface);
    check(&n, &over, &r, 1e-7);
}

/// A rational cylinder with one control point moved: recognised within an accuracy above the
/// move, refuted below it.
#[test]
fn bumped_cylinder() {
    let s = Surface::Cylinder(Cylinder::new(tilted(), mm(2.5), Sense::Forward).unwrap());
    let base = s.to_nurbs(&ext((0.2, 1.5), (-1.0, 4.0))).unwrap();
    for (bump, a, found) in [(1e-9, 1e-6, true), (1e-3, 1e-6, false)] {
        let mut pts = base.points().points().to_vec();
        let mid = pts.len() / 2;
        pts[mid][0] += bump;
        let rows = base.points().nrows();
        let cols = base.points().ncols();
        let n = NurbsSurface::new(
            base.knot_vector(scx_nurbs_surface::ParamAxis::U).clone(),
            base.knot_vector(scx_nurbs_surface::ParamAxis::V).clone(),
            Net::new(rows, cols, pts).unwrap(),
            base.weights().cloned(),
        )
        .unwrap();
        let (n, over) = nurbs(n);
        let r = rec(&n, &over, a);
        assert_eq!(r.is_some(), found, "bump {bump}: {r:?}");
        if let Some(r) = r {
            assert!(matches!(r.surface, Surface::Cylinder(_)));
            check(&n, &over, &r, a);
        }
    }
}

// ---------------------------------------------------------------- non-rational forms

/// The cubic Bezier arc of a quarter turn from `a`: control points `(x, y)`.
fn arc(a: f64) -> [(f64, f64); 4] {
    let k = 0.552_284_749_830_793_4;
    let (c, s) = (fmath::cos(a), fmath::sin(a));
    let rot = |x: f64, y: f64| (c * x - s * y, s * x + c * y);
    [rot(1.0, 0.0), rot(1.0, k), rot(k, 1.0), rot(0.0, 1.0)]
}

fn bezier(points: Vec<[f64; 3]>, du: usize, dv: usize) -> (Surface, Extent2d) {
    let kv = |d: usize| {
        let mut k = vec![0.0; d + 1];
        k.extend(vec![1.0; d + 1]);
        KnotVector::new(d, k).unwrap()
    };
    let f = tilted();
    let (o, x, y, z) = (f.origin(), *f.x(), *f.y(), *f.z());
    let world = points
        .into_iter()
        .map(|p| {
            let w = o + x * p[0] + y * p[1] + z * p[2];
            [w.x, w.y, w.z]
        })
        .collect();
    nurbs(
        NurbsSurface::new(
            kv(du),
            kv(dv),
            Net::new(du + 1, dv + 1, world).unwrap(),
            None,
        )
        .unwrap(),
    )
}

/// Non-rational quarter-turn approximations of a cylinder, a sphere zone and a torus (radial
/// error about `2.7e-4` of the radius): the kind within an accuracy above the error, nothing
/// within one below it.
#[test]
fn polynomial_approximations() {
    let d = arc(0.3);
    let (r, big) = (2.0, 6.0);
    let cyl: Vec<[f64; 3]> = d
        .iter()
        .flat_map(|&(x, y)| [[r * x, r * y, -1.0], [r * x, r * y, 3.0]])
        .collect();
    let m = arc(-FRAC_PI_2 / 2.0);
    let sph: Vec<[f64; 3]> = d
        .iter()
        .flat_map(|&(x, y)| m.map(|(c, s)| [r * c * x, r * c * y, r * s]))
        .collect();
    let tor: Vec<[f64; 3]> = d
        .iter()
        .flat_map(|&(x, y)| arc(0.2).map(|(c, s)| [(big + r * c) * x, (big + r * c) * y, r * s]))
        .collect();
    for (name, s, kind) in [
        ("cylinder", bezier(cyl, 3, 1), "Cylinder"),
        ("sphere", bezier(sph, 3, 3), "Sphere"),
        ("torus", bezier(tor, 3, 3), "Torus"),
    ] {
        let (s, over) = s;
        let r = rec(&s, &over, 0.01).unwrap_or_else(|| panic!("{name}: None"));
        assert!(
            format!("{:?}", r.surface).starts_with(kind),
            "{name}: {:?}",
            r.surface
        );
        let seen = check(&s, &over, &r, 0.01);
        assert!(seen > 1e-4, "{name}: {seen}");
        assert!(rec(&s, &over, 1e-5).is_none(), "{name}");
        // Near the error: whatever is proved holds at every sample, and twice the first
        // fit's sampled distance is proved.
        for f in [0.25, 0.4, 0.5, 0.6, 0.75, 1.0, 1.5, 2.0] {
            let a = f * seen;
            let r = rec(&s, &over, a);
            println!(
                "{name}: {f} x {seen:.3e}: {:?}",
                r.as_ref().map(|r| r.deviation.mm())
            );
            match r {
                Some(r) => {
                    check(&s, &over, &r, a);
                }
                None => assert!(f < 2.0, "{name}: 2x None"),
            }
        }
    }
}

// ---------------------------------------------------------------- rejects and errors

/// A bicubic patch of no kind.
fn generic() -> (Surface, Extent2d) {
    let mut pts = Vec::new();
    for i in 0..4 {
        for j in 0..4 {
            let (x, y) = (i as f64, j as f64);
            let z = 0.3 * fmath::sin(x * y) + 0.2 * (x - 1.5) * (x - 1.5) - 0.15 * y * y * y / 9.0
                + 0.05 * x * y * y;
            pts.push([x, y, z]);
        }
    }
    bezier(pts, 3, 3)
}

#[test]
fn generic_nurbs_rejected() {
    let (s, over) = generic();
    for a in [1e-6, 1e-3, 1e-2] {
        assert!(rec(&s, &over, a).is_none(), "{a}");
    }
}

#[test]
fn budget_stop_and_extent() {
    let s = exact_cases()[2].1.clone();
    let (n, over) = nurbs(s.to_nurbs(&ext((0.2, 3.5), (-1.0, 4.0))).unwrap());
    assert!(matches!(
        recognize(&n, &over, acc(1e-6), &mut Budget::new(3), &Stop::new()),
        Err(SurfaceError::Exhausted(_))
    ));
    let stop = Stop::new();
    stop.set();
    assert!(matches!(
        recognize(&n, &over, acc(1e-6), &mut Budget::unlimited(), &stop),
        Err(SurfaceError::Stopped)
    ));
    let outside = ext((-10.0, 100.0), (-1.0, 4.0));
    assert!(
        recognize(
            &n,
            &outside,
            acc(1e-6),
            &mut Budget::unlimited(),
            &Stop::new()
        )
        .is_err()
    );
}
