// Written for solidcorex. Apache-2.0.
//! The volume maps against their closed forms: the pin's map truths, round trips through the
//! inverse, boxes holding sampled images, and each image rule's surface holding the mapped
//! points with the pushed-forward normal.

use std::f64::consts::{FRAC_PI_2, PI};

use scx_num::budget::{Budget, Stop};
use scx_num::vector2d::Point2;
use scx_nurbs::Approach;

use super::*;

const ABOVE: [Approach; 2] = [Approach::Above; 2];

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn world() -> Frame {
    Frame::new(
        p(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap()
}

/// The pin's frame: centre (1, 2, 3), axis z, mid direction x.
fn pin_frame() -> Frame {
    Frame::new(
        p(1.0, 2.0, 3.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap()
}

/// A frame leaning off every world axis, for the image rules.
fn skew() -> Frame {
    Frame::new(
        p(1.0, -2.0, 0.5),
        Vec3::new(1.0, 1.0, 2.0),
        Vec3::new(1.0, -1.0, 0.0),
    )
    .unwrap()
}

fn bend(frame: Frame, d: f64) -> VolumeMap {
    VolumeMap::Bend(Bend::new(frame, mm(d)).unwrap())
}

fn unbend(frame: Frame, d: f64) -> VolumeMap {
    VolumeMap::Unbend(Bend::new(frame, mm(d)).unwrap())
}

fn twist(frame: Frame, pitch: f64) -> VolumeMap {
    VolumeMap::Twist(Twist::new(frame, mm(pitch)).unwrap())
}

fn close(a: Point3, b: Point3, tol: f64) -> bool {
    a.distance(b) <= tol * (1.0 + b.to_vec().length())
}

#[track_caller]
fn assert_close(a: Point3, b: Point3, tol: f64) {
    assert!(close(a, b, tol), "{a:?} vs {b:?}");
}

#[test]
fn pin_simple_truths() {
    let b = bend(world(), 2.0);
    let q = b.map(p(3.0, PI, 1.0));
    assert_close(q, p(0.0, 3.0, 1.0), 1e-12);
    assert_close(b.inverse(q).unwrap(), p(3.0, PI, 1.0), 1e-12);
    assert_close(
        twist(world(), 4.0).map(p(1.0, 0.0, 1.0)),
        p(0.0, 1.0, 1.0),
        1e-12,
    );
    assert_close(
        unbend(world(), 2.0).map(p(0.0, 3.0, 1.0)),
        p(3.0, PI, 1.0),
        1e-12,
    );
}

#[test]
fn pin_frame_truths() {
    let f = pin_frame();
    assert_close(
        bend(f, 2.0).map(p(3.0, 2.0 + PI, 5.0)),
        p(1.0, 4.0, 5.0),
        1e-12,
    );
    assert_close(twist(f, 8.0).map(p(2.0, 2.0, 5.0)), p(1.0, 3.0, 5.0), 1e-12);
    assert_close(
        unbend(f, 2.0).map(p(1.0, 3.0, 5.0)),
        p(2.0, 2.0 + PI, 5.0),
        1e-12,
    );
}

#[test]
fn pin_seam_takes_plus_pi() {
    // On the cut, the bend has two preimages, v = +-pi d; +0 takes +pi d.
    let q = bend(world(), 2.0).inverse(p(-3.0, 0.0, 0.5)).unwrap();
    assert_close(q, p(3.0, 2.0 * PI, 0.5), 1e-12);
}

#[test]
fn jacobian_matches_differences() {
    let at = p(0.7, -0.4, 0.9);
    let h = 1e-6;
    let maps = [
        bend(world(), 1.5),
        twist(world(), 3.0),
        unbend(world(), 1.5),
        bend(skew(), 1.5),
        VolumeMap::Chain(Arc::from([bend(skew(), 2.0), twist(pin_frame(), -3.0)])),
    ];
    for m in &maps {
        let jac = m.jacobian(at);
        for (k, col) in jac.iter().enumerate() {
            let mut e = [0.0; 3];
            e[k] = h;
            let step = Vec3::new(e[0], e[1], e[2]);
            let fd = (m.map(at + step) - m.map(at - step)) * (0.5 / h);
            assert!(
                (fd - *col).length() <= 1e-6,
                "{m:?} column {k}: {fd:?} vs {col:?}"
            );
        }
    }
}

/// Points in the bend's domain of `frame`, by local coordinates.
fn domain_points(frame: &Frame, d: f64) -> Vec<Point3> {
    let mut out = Vec::new();
    for &u in &[0.01, 0.5, 1.0, 3.0, 40.0] {
        for &t in &[-0.999, -0.5, 0.0, 0.3, 0.999] {
            for &w in &[-2.0, 0.0, 1.5] {
                out.push(to_world(frame, [u, t * PI * d, w]));
            }
        }
    }
    out
}

#[test]
fn map_then_inverse_is_identity() {
    for frame in [world(), pin_frame(), skew()] {
        for d in [0.5, 2.0, 30.0] {
            let pts = domain_points(&frame, d);
            let b = bend(frame, d);
            let ub = unbend(frame, d);
            for &x in &pts {
                assert_close(b.inverse(b.map(x)).unwrap(), x, 1e-13);
                let y = b.map(x);
                assert_close(ub.map(y), x, 1e-13);
                assert_close(ub.inverse(ub.map(y)).unwrap(), y, 1e-13);
            }
        }
        for pitch in [4.0, -0.7, 100.0] {
            let t = twist(frame, pitch);
            for &x in &domain_points(&frame, 1.0) {
                assert_close(t.inverse(t.map(x)).unwrap(), x, 1e-13);
                assert_close(t.inverted().map(t.map(x)), x, 1e-13);
            }
        }
    }
    let chain = VolumeMap::Chain(Arc::from([
        bend(skew(), 2.0),
        twist(skew(), 5.0),
        twist(pin_frame(), -3.0),
    ]));
    for &x in &domain_points(&skew(), 2.0) {
        assert_close(chain.inverse(chain.map(x)).unwrap(), x, 1e-12);
        assert_close(chain.inverted().map(chain.map(x)), x, 1e-12);
    }
}

#[test]
fn inverse_refuses_without_preimage() {
    let b = bend(pin_frame(), 2.0);
    assert_eq!(b.inverse(p(1.0, 2.0, 7.0)), Err(MapError::OnAxis));
    assert_eq!(b.inverse(p(f64::NAN, 2.0, 7.0)), Err(MapError::NotFinite));
    let ub = unbend(pin_frame(), 2.0);
    assert_eq!(ub.inverse(p(0.5, 2.0, 3.0)), Err(MapError::OutsideImage));
    assert_eq!(
        ub.inverse(p(2.0, 2.0 + 2.1 * PI, 3.0)),
        Err(MapError::OutsideImage)
    );
    assert!(ub.inverse(p(2.0, 2.0 + 2.0 * PI, 3.0)).is_ok());
}

#[test]
fn builders_refuse_bad_sizes() {
    for d in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            Bend::new(world(), mm(d)),
            Err(MapError::BadNeutral { .. })
        ));
    }
    for t in [0.0, f64::NAN, f64::NEG_INFINITY] {
        assert!(matches!(
            Twist::new(world(), mm(t)),
            Err(MapError::BadPitch { .. })
        ));
    }
}

fn ext(lo: [f64; 3], hi: [f64; 3]) -> Extent3d {
    Extent3d::from_bounds(lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]).unwrap()
}

/// Every point of a grid over `b`, its corners included, maps into `m.bound(b)`.
#[track_caller]
fn assert_holds(m: &VolumeMap, b: &Extent3d) -> Extent3d {
    let out = m.bound(b).unwrap();
    let n = 6_u32;
    for i in 0..=n {
        for j in 0..=n {
            for k in 0..=n {
                let t = |a: f64, z: f64, s: u32| a + (z - a) * f64::from(s) / f64::from(n);
                let x = p(
                    t(b.x_min(), b.x_max(), i),
                    t(b.y_min(), b.y_max(), j),
                    t(b.z_min(), b.z_max(), k),
                );
                let q = m.map(x);
                let inside = q.x >= out.x_min()
                    && q.x <= out.x_max()
                    && q.y >= out.y_min()
                    && q.y <= out.y_max()
                    && q.z >= out.z_min()
                    && q.z <= out.z_max();
                assert!(inside, "{m:?}: {x:?} maps to {q:?} outside {out:?}");
            }
        }
    }
    out
}

#[test]
fn pin_bend_box() {
    let m = bend(world(), 1.0);
    let out = assert_holds(&m, &ext([1.0, 0.0, 0.0], [2.0, FRAC_PI_2, 1.0]));
    let tol = 1e-12;
    assert!(out.x_min() >= -tol && out.x_max() <= 2.0 + tol, "{out:?}");
    assert!(out.y_min() >= -tol && out.y_max() <= 2.0 + tol, "{out:?}");
    assert!(out.z_min() >= -tol && out.z_max() <= 1.0 + tol, "{out:?}");
}

#[test]
fn pin_wr59_boxes() {
    let m = bend(world(), 1.0);
    let b = ext([0.5, -2.0, -1.0], [1.5, 2.0, 1.0]);
    let out = assert_holds(&m, &b);
    for x in [p(1.5, 2.0, 0.0), p(1.5, FRAC_PI_2, 0.0)] {
        let q = m.map(x);
        assert!(q.y <= out.y_max() && q.x >= out.x_min(), "{q:?} {out:?}");
    }
    assert_holds(&bend(world(), 2.5), &ext([0.8, 0.5, -0.5], [2.2, 2.5, 0.5]));
    assert_holds(&bend(world(), 1.5), &ext([0.6, -0.9, 0.0], [1.8, 4.5, 2.0]));
    // Full periods: the angle covers the whole turn.
    assert_holds(&bend(world(), 1.0), &ext([0.5, -PI, 0.0], [1.0, PI, 1.0]));
    assert_holds(
        &bend(world(), 1.0),
        &ext([0.5, 0.0, 0.0], [1.0, 2.0 * PI, 1.0]),
    );
}

#[test]
fn pin_twist_full_turn_box() {
    let m = twist(world(), 4.0);
    let out = assert_holds(&m, &ext([0.0, 0.0, 0.0], [1.0, 1.0, 5.0]));
    let reach = 2.0_f64.sqrt() * (1.0 + 1e-12);
    assert!(out.x_min() >= -reach && out.x_max() <= reach, "{out:?}");
    assert!(out.y_min() >= -reach && out.y_max() <= reach, "{out:?}");
}

#[test]
fn boxes_hold_sampled_images() {
    let maps = [
        bend(skew(), 2.0),
        unbend(skew(), 2.0),
        unbend(world(), 1.0),
        twist(skew(), -3.0),
        VolumeMap::Chain(Arc::from([bend(skew(), 2.0), twist(pin_frame(), 5.0)])),
    ];
    let boxes = [
        ext([0.2, -1.0, 0.0], [1.0, 1.5, 0.5]),
        ext([-3.0, -0.1, -1.0], [-1.0, 0.1, 1.0]),
        ext([1.0, 2.0, 3.0], [1.01, 2.01, 3.01]),
        ext([-0.5, -0.5, -0.5], [0.5, 0.5, 0.5]),
    ];
    for m in &maps {
        for b in &boxes {
            assert_holds(m, b);
        }
    }
}

#[test]
fn interval_angle_is_tight_off_the_cut() {
    let a = <Ivl as Polar>::atan2(Ivl::new(1.0, 2.0), Ivl::new(1.0, 2.0));
    assert!(a.lo <= fmath::atan2(1.0, 2.0) && a.hi >= fmath::atan2(2.0, 1.0));
    assert!(a.hi - a.lo < 0.65, "{a:?}");
    let cut = <Ivl as Polar>::atan2(Ivl::new(-0.1, 0.1), Ivl::new(-2.0, -1.0));
    assert!(cut.lo <= -PI && cut.hi >= PI);
}

/// The source's normal pushed forward by the map at `x`: `J^-T n`, up to a positive factor.
fn pushed_normal(m: &VolumeMap, x: Point3, n: Vec3) -> Vec3 {
    let [a, b, c] = m.jacobian(x);
    let det = a.dot(b.cross(c));
    (b.cross(c) * n.x + c.cross(a) * n.y + a.cross(b) * n.z) * det.signum()
}

/// `m`'s image of `source` is exact, holds the map of every sample with the pushed-forward
/// normal, and keeps uv when it says so. Returns the image.
#[track_caller]
fn assert_image(m: &VolumeMap, source: &Surface, uvs: &[Point2]) -> (Surface, Uv) {
    let near = source.point(uvs[0]);
    let Image::Exact { surface, uv } = m.image(source, near) else {
        panic!("{m:?}: no exact image of {source:?}");
    };
    for &st in uvs {
        let x = source.point(st);
        let q = m.map(x);
        let foot = surface
            .invert(q, None, None, &mut Budget::unlimited(), &Stop::new())
            .unwrap();
        let foot = foot.foot();
        assert!(
            foot.distance <= 1e-11 * (1.0 + q.to_vec().length()),
            "{m:?}: {q:?} is {} off {surface:?}",
            foot.distance
        );
        let n = *source.normal(st, ABOVE).unwrap();
        let want = pushed_normal(m, x, n).normalized().unwrap();
        let got = surface.normal(foot.at, ABOVE).unwrap();
        assert!(
            want.dot(*got) > 1.0 - 1e-9,
            "{m:?} at {st:?}: normal {got:?} vs {want:?}"
        );
        if uv == Uv::Same {
            assert_close(surface.point(st), q, 1e-12);
        }
    }
    (surface, uv)
}

fn grid(us: &[f64], vs: &[f64]) -> Vec<Point2> {
    us.iter()
        .flat_map(|&u| vs.iter().map(move |&v| Point2::new(u, v)))
        .collect()
}

/// A frame given in `m`'s local coordinates.
fn local_frame(m: &Frame, o: [f64; 3], z: [f64; 3], x: [f64; 3]) -> Frame {
    frame_at(m, o, z, x).unwrap()
}

const X: [f64; 3] = [1.0, 0.0, 0.0];
const Y: [f64; 3] = [0.0, 1.0, 0.0];
const Z: [f64; 3] = [0.0, 0.0, 1.0];

fn neg(a: [f64; 3]) -> [f64; 3] {
    [-a[0], -a[1], -a[2]]
}

fn plane(m: &Frame, o: [f64; 3], z: [f64; 3], x: [f64; 3]) -> Surface {
    Surface::Plane(Plane::new(local_frame(m, o, z, x)))
}

fn senses() -> [Sense; 2] {
    [Sense::Forward, Sense::Reversed]
}

#[test]
fn bend_images_by_rule() {
    let f = skew();
    let m = bend(f, 2.0);
    let flat = grid(&[0.0, 0.7, 2.0], &[-2.0, 0.0, 1.5]);
    // Square to the axis: the same plane.
    let (s, uv) = assert_image(&m, &plane(&f, [1.0, 0.3, 0.5], Z, X), &flat);
    assert!(matches!(s, Surface::Plane(_)) && uv == Uv::Moved);
    // u = 1.5, either facing: a cylinder about the axis.
    for z in [X, neg(X)] {
        let (s, _) = assert_image(&m, &plane(&f, [1.5, 0.0, 0.0], z, Y), &flat);
        assert!(matches!(s, Surface::Cylinder(c) if (c.radius().mm() - 1.5).abs() < 1e-12));
    }
    // v = 1: the plane through the axis, same uv.
    for z in [Y, neg(Y)] {
        let (s, uv) = assert_image(&m, &plane(&f, [1.0, 1.0, 0.0], z, X), &flat);
        assert!(matches!(s, Surface::Plane(_)) && uv == Uv::Same);
    }
    // Oblique to the axis in the (u, w) half-plane: a cone with its apex on the axis.
    let slanted = grid(&[-2.0, 0.0, 1.5], &[-1.0, 0.0, 1.0]);
    for z in [
        [0.6, 0.0, 0.8],
        [-0.6, 0.0, 0.8],
        [0.6, 0.0, -0.8],
        [-0.8, 0.0, -0.6],
    ] {
        let (s, _) = assert_image(&m, &plane(&f, [2.0, 0.0, 0.0], z, Y), &slanted);
        assert!(matches!(s, Surface::Cone(_)), "{s:?}");
    }
    // A cylinder across the bend: a torus off the axis, a sphere on it.
    let arcs = grid(&[0.85, 0.95, 0.05, 0.15], &[-2.0, 0.0, 2.0]);
    for sense in senses() {
        let c = Cylinder::new(local_frame(&f, [3.0, 0.0, 0.5], Y, X), mm(1.0), sense).unwrap();
        let (s, _) = assert_image(&m, &Surface::Cylinder(c), &arcs);
        assert!(matches!(s, Surface::Torus(t) if t.sense() == sense));
        let c = Cylinder::new(local_frame(&f, [0.0, 0.0, 0.5], neg(Y), X), mm(1.0), sense).unwrap();
        let (s, _) = assert_image(&m, &Surface::Cylinder(c), &arcs);
        assert!(matches!(s, Surface::Sphere(_)));
    }
}

#[test]
fn bend_without_rule() {
    let f = skew();
    let m = bend(f, 2.0);
    let near = to_world(&f, [1.0, 0.0, 0.0]);
    let no = |s: &Surface| match m.image(s, near) {
        Image::NotExact(why) => why,
        Image::Exact { surface, .. } => panic!("{surface:?}"),
    };
    assert_eq!(no(&plane(&f, [-1.0, 0.0, 0.0], X, Y)), NotExact::MeetsAxis);
    assert_eq!(
        no(&plane(&f, [1.0, 7.0, 0.0], Y, X)),
        NotExact::OutsideDomain
    );
    assert_eq!(
        no(&plane(&f, [1.0, 0.0, 0.0], [0.6, 0.8, 0.0], Z)),
        NotExact::NoRule
    );
    let c = Cylinder::new(
        local_frame(&f, [0.5, 0.0, 0.0], Y, X),
        mm(1.0),
        Sense::Forward,
    );
    assert_eq!(no(&Surface::Cylinder(c.unwrap())), NotExact::MeetsAxis);
    let c = Cylinder::new(
        local_frame(&f, [3.0, 0.0, 0.0], Z, X),
        mm(1.0),
        Sense::Forward,
    );
    assert_eq!(no(&Surface::Cylinder(c.unwrap())), NotExact::NoRule);
    let sp = Sphere::new(
        local_frame(&f, [3.0, 0.0, 0.0], Z, X),
        mm(1.0),
        Sense::Forward,
    );
    assert_eq!(no(&Surface::Sphere(sp.unwrap())), NotExact::NoRule);
}

#[test]
fn unbend_images_by_rule() {
    let f = skew();
    let m = unbend(f, 2.0);
    let flat = grid(&[0.5, 1.0, 2.0], &[-1.5, 0.0, 1.0]);
    let (s, _) = assert_image(&m, &plane(&f, [1.0, 0.3, 0.5], Z, X), &flat);
    assert!(matches!(s, Surface::Plane(_)));
    let around = grid(&[0.05, 0.3, 0.45, 0.6, 0.9], &[-1.0, 0.0, 2.0]);
    for sense in senses() {
        let c = Cylinder::new(local_frame(&f, [0.0, 0.0, 1.0], Z, X), mm(1.5), sense).unwrap();
        let (s, _) = assert_image(&m, &Surface::Cylinder(c), &around);
        assert!(matches!(s, Surface::Plane(_)));
    }
    // A plane holding the axis: each half by `near`, same uv.
    let (sn, cs) = fmath::sin_cos(0.7);
    let half = plane(&f, [0.0, 0.0, 0.3], [-sn, cs, 0.0], [cs, sn, 0.0]);
    let (_, uv) = assert_image(&m, &half, &grid(&[0.5, 1.0, 3.0], &[-1.0, 0.0, 2.0]));
    assert_eq!(uv, Uv::Same);
    assert_image(&m, &half, &grid(&[-0.5, -1.0, -3.0], &[-1.0, 0.0, 2.0]));
    let on_cut = plane(&f, [0.0, 0.0, 0.0], Y, X);
    assert_eq!(
        m.image(&on_cut, to_world(&f, [-2.0, 0.0, 0.0])),
        Image::NotExact(NotExact::OnCut)
    );
    // A coaxial cone: each nappe by `near`.
    for sense in senses() {
        for z in [Z, neg(Z)] {
            let c = Cone::new(local_frame(&f, [0.0, 0.0, 0.5], z, X), mm(1.0), 0.4, sense).unwrap();
            let s = Surface::Cone(c);
            let (s1, _) = assert_image(&m, &s, &grid(&[0.1, 0.4, 0.8], &[-1.0, 0.0, 1.0]));
            let (s2, _) = assert_image(&m, &s, &grid(&[0.1, 0.4, 0.8], &[-5.0, -4.0, -3.0]));
            assert!(matches!(s1, Surface::Plane(_)) && matches!(s2, Surface::Plane(_)));
            assert_ne!(s1, s2);
        }
    }
    for sense in senses() {
        let t = Torus::new(
            local_frame(&f, [0.0, 0.0, 0.5], neg(Z), X),
            mm(3.0),
            mm(1.0),
            sense,
        )
        .unwrap();
        let (s, _) = assert_image(
            &m,
            &Surface::Torus(t),
            &grid(&[0.1, 0.3, 0.7], &[0.1, 0.4, 0.8]),
        );
        assert!(matches!(s, Surface::Cylinder(c) if c.sense() == sense));
        let sp = Sphere::new(local_frame(&f, [0.0, 0.0, 0.5], X, Y), mm(2.0), sense).unwrap();
        let (s, _) = assert_image(
            &m,
            &Surface::Sphere(sp),
            &grid(&[0.1, 0.3, 0.7], &[0.2, 0.5, 0.8]),
        );
        assert!(matches!(s, Surface::Cylinder(_)));
    }
}

#[test]
fn twist_images_by_rule() {
    let f = skew();
    let m = twist(f, -5.0);
    let flat = grid(&[-2.0, 0.0, 1.5], &[-1.0, 0.0, 2.0]);
    let (s, uv) = assert_image(&m, &plane(&f, [0.5, 0.3, 0.7], neg(Z), Y), &flat);
    assert!(matches!(s, Surface::Plane(_)) && uv == Uv::Same);
    let around = grid(&[0.05, 0.3, 0.6, 0.9], &[0.1, 0.4, 0.8]);
    let axis = |o| local_frame(&f, o, Z, X);
    let kinds = [
        Surface::Cylinder(Cylinder::new(axis([0.0, 0.0, 1.0]), mm(1.5), Sense::Reversed).unwrap()),
        Surface::Cone(Cone::new(axis([0.0, 0.0, 1.0]), mm(1.5), -0.3, Sense::Forward).unwrap()),
        Surface::Torus(
            Torus::new(axis([0.0, 0.0, 1.0]), mm(3.0), mm(1.0), Sense::Forward).unwrap(),
        ),
        Surface::Sphere(
            Sphere::new(
                local_frame(&f, [0.0, 0.0, 2.0], X, Y),
                mm(2.0),
                Sense::Reversed,
            )
            .unwrap(),
        ),
    ];
    for s in &kinds {
        let (img, uv) = assert_image(&m, s, &around);
        assert_eq!((&img, uv), (s, Uv::Moved));
    }
    let off = Cylinder::new(axis([0.5, 0.0, 0.0]), mm(1.0), Sense::Forward).unwrap();
    assert_eq!(
        m.image(&Surface::Cylinder(off), f.origin()),
        Image::NotExact(NotExact::NoRule)
    );
    assert_eq!(
        m.image(&plane(&f, [0.0; 3], Y, X), f.origin()),
        Image::NotExact(NotExact::NoRule)
    );
}

#[test]
fn chain_images_compose() {
    let f = skew();
    let chain = VolumeMap::Chain(Arc::from([bend(f, 2.0), twist(f, 5.0)]));
    let flat = grid(&[0.0, 0.7, 2.0], &[-2.0, 0.0, 1.5]);
    // u = 1.5 bends to a coaxial cylinder, which the twist keeps.
    let (s, uv) = assert_image(&chain, &plane(&f, [1.5, 0.0, 0.0], X, Y), &flat);
    assert!(matches!(s, Surface::Cylinder(_)) && uv == Uv::Moved);
    // v = 1 bends to a plane through the axis, which the twist has no rule for.
    let near = to_world(&f, [1.0, 1.0, 0.0]);
    assert_eq!(
        chain.image(&plane(&f, [1.0, 1.0, 0.0], Y, X), near),
        Image::NotExact(NotExact::NoRule)
    );
    // A twist then its inverse keeps uv through both turns.
    let there_and_back = VolumeMap::Chain(Arc::from([twist(f, 5.0), twist(f, -5.0)]));
    let (_, uv) = assert_image(&there_and_back, &plane(&f, [0.0, 0.0, 0.7], Z, X), &flat);
    assert_eq!(uv, Uv::Same);
}
