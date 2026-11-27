// Written for solidcorex. Apache-2.0.
//! [`AnyCurve`] and [`CurveOnSurface`] (63 §4.9): a circle in uv on a plane is the 3-D
//! circle to high order; the surface's partials are taken from the side the curve moves to
//! at a crease, crossed backward or touched; `eval_many` is `point`; the free arm is the
//! curve's own calls. The chain rule itself is checked against exact rationals in the
//! module's unit tests.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic
)]

use scx_curve::{Circle, Circle2, Curve, Curve2, Line, Line2};
use scx_num::frame::{Frame, Frame2, Sense};
use scx_num::hash::Rng;
use scx_num::units::Length;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{Approach, KnotVector};
use scx_nurbs_surface::{Net, NurbsSurface};
use scx_surface::{AnyCurve, CurveOnSurface, Plane, Sphere, Surface};

const SIDES: [Approach; 2] = [Approach::Below, Approach::Above];

fn unit(rng: &mut Rng) -> f64 {
    rng.next_signed_unit()
}

fn frame(rng: &mut Rng) -> Frame {
    let mut v = || Vec3::new(unit(rng), unit(rng), unit(rng));
    let (o, z, x) = (v(), v(), v());
    Frame::new(Point3::new(o.x * 10.0, o.y * 10.0, o.z * 10.0), z, x).unwrap()
}

/// Two flat bilinear patches meeting at a right angle along `u = 0.5`: `(2u, v, 0)` below
/// it, `(1, v, 2u - 1)` above.
fn crease() -> Surface {
    let u = KnotVector::new(1, vec![0.0, 0.0, 0.5, 1.0, 1.0]).unwrap();
    let v = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
    let pts = vec![
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
    ];
    Surface::from(NurbsSurface::new(u, v, Net::new(3, 2, pts).unwrap(), None).unwrap())
}

fn close(a: Vec3, b: Vec3, tol: f64) -> bool {
    (a - b).length() <= tol
}

/// A uv circle on a plane is the 3-D circle in the plane's frame, to order 8.
#[test]
fn a_circle_on_a_plane_is_the_circle() {
    let mut rng = Rng::new(0x5c8);
    for _ in 0..40 {
        let f = frame(&mut rng);
        let (c, a) = (
            Point2::new(unit(&mut rng) * 5.0, unit(&mut rng) * 5.0),
            Vec2::new(unit(&mut rng), unit(&mut rng)),
        );
        let r = Length::from_mm(0.5 + rng.next_f64() * 5.0);
        let sense = if rng.next_f64() < 0.5 {
            Sense::Forward
        } else {
            Sense::Reversed
        };
        let uv = Circle2::new(Frame2::new(c, a).unwrap(), r, sense).unwrap();
        let (x, y) = (*f.x(), *f.y());
        let o = f.origin() + x * c.x + y * c.y;
        let x3 = x * a.x + y * a.y;
        let circle = Circle::new(Frame::new(o, *f.z(), x3).unwrap(), r, sense).unwrap();
        let on = AnyCurve::from(CurveOnSurface::new(
            Curve2::from(uv),
            Surface::from(Plane::new(f)),
        ));
        let scale = 10.0 + r.mm();
        for _ in 0..10 {
            let t = unit(&mut rng) * 7.0;
            for side in SIDES {
                let got = on.jet::<8>(t, side);
                let want = circle.jet::<8>(t, side);
                assert!(close(
                    got.point - Point3::ORIGIN,
                    want.point - Point3::ORIGIN,
                    1e-13 * scale
                ));
                for (g, w) in got.d.iter().zip(&want.d) {
                    assert!(close(*g, *w, 1e-13 * scale), "{g:?} vs {w:?}");
                }
            }
        }
    }
}

/// Crossing the crease with `u` falling: moving up in `t` reaches the piece below the knot
/// line, so the first derivative is the one-sided difference on that side, from each side.
/// The pin's `CrvOnSurf::evaluate` takes `side` for `u` too, the other piece.
#[test]
fn crossing_a_crease_backward_takes_the_piece_it_moves_into() {
    let s = crease();
    // At t = 0 on the crease, moving to lower u as t rises.
    let uv = Line2::new(Point2::new(0.5, 0.4), Vec2::new(-1.0, 0.5)).unwrap();
    let c = CurveOnSurface::new(Curve2::from(uv), s.clone());
    let h = 1.0 / 64.0;
    let above = c.jet::<1>(0.0, Approach::Above).d[0];
    let below = c.jet::<1>(0.0, Approach::Below).d[0];
    let fwd = (c.point(h) - c.point(0.0)) * (1.0 / h);
    let back = (c.point(0.0) - c.point(-h)) * (1.0 / h);
    assert!(close(above, Vec3::new(-2.0, 0.5, 0.0), 1e-15), "{above:?}");
    assert!(close(below, Vec3::new(0.0, 0.5, -2.0), 1e-15), "{below:?}");
    assert!(close(above, fwd, 1e-14) && close(below, back, 1e-14));
    // The pin's sides: `u` from `side` gives the piece the curve leaves.
    let pin = s.jet::<1>(Point2::new(0.5, 0.4), [Approach::Above; 2]);
    let pin = pin.get(1, 0).unwrap() * -1.0 + pin.get(0, 1).unwrap() * 0.5;
    assert!(!close(pin, above, 0.1));
}

/// A uv circle touching the crease from below: `u' = 0` there and `u'' < 0`, so the curve is
/// below the knot line on both sides and both jets take that piece; the curve stays in
/// `z = 0`.
#[test]
fn touching_a_crease_takes_the_piece_it_stays_in() {
    let s = crease();
    let uv = Circle2::new(
        Frame2::new(Point2::new(0.3, 0.5), Vec2::new(1.0, 0.0)).unwrap(),
        Length::from_mm(0.2),
        Sense::Forward,
    )
    .unwrap();
    let c = CurveOnSurface::new(Curve2::from(uv), s);
    for side in SIDES {
        let j = c.jet::<3>(0.0, side);
        assert!(close(
            j.point - Point3::ORIGIN,
            Vec3::new(1.0, 0.5, 0.0),
            1e-15
        ));
        // (2u, v, 0) with u = 0.3 + 0.2 cos t, v = 0.5 + 0.2 sin t.
        assert!(
            close(j.d[0], Vec3::new(0.0, 0.2, 0.0), 1e-15),
            "{:?}",
            j.d[0]
        );
        assert!(
            close(j.d[1], Vec3::new(-0.4, 0.0, 0.0), 1e-15),
            "{:?}",
            j.d[1]
        );
        assert!(
            close(j.d[2], Vec3::new(0.0, -0.2, 0.0), 1e-15),
            "{:?}",
            j.d[2]
        );
    }
}

fn eval_many_is_point(c: &AnyCurve) {
    let mut rng = Rng::new(3);
    for n in [0, 1, 63, 64, 65, 129, 200] {
        let ts: Vec<f64> = (0..n).map(|_| unit(&mut rng) * 3.0).collect();
        let mut out = vec![Point3::ORIGIN; n];
        c.eval_many(&ts, &mut out);
        for (t, p) in ts.iter().zip(&out) {
            assert_eq!(p.to_array(), c.point(*t).to_array());
        }
    }
}

#[test]
fn eval_many_is_point_bit_for_bit() {
    let mut rng = Rng::new(9);
    let sphere = Sphere::new(frame(&mut rng), Length::from_mm(3.0), Sense::Forward).unwrap();
    let uv = Circle2::new(
        Frame2::new(Point2::new(1.0, 0.2), Vec2::new(1.0, 2.0)).unwrap(),
        Length::from_mm(0.7),
        Sense::Reversed,
    )
    .unwrap();
    eval_many_is_point(&CurveOnSurface::new(Curve2::from(uv), Surface::from(sphere)).into());
    let line = Line2::through(Point2::new(0.1, 0.9), Point2::new(0.8, 0.2)).unwrap();
    eval_many_is_point(&CurveOnSurface::new(Curve2::from(line), crease()).into());
    let free = Line::through(Point3::ORIGIN, Point3::new(1.0, 2.0, 3.0)).unwrap();
    eval_many_is_point(&Curve::from(free).into());
}

#[test]
#[should_panic(expected = "differ in length")]
fn eval_many_refuses_lengths_that_differ() {
    let line = Line2::through(Point2::new(0.1, 0.9), Point2::new(0.8, 0.2)).unwrap();
    let c = CurveOnSurface::new(Curve2::from(line), crease());
    c.eval_many(&[0.0, 1.0], &mut [Point3::ORIGIN]);
}

/// The free arm is the curve's own calls, bit for bit.
#[test]
fn the_free_arm_is_the_curve() {
    let mut rng = Rng::new(11);
    let circle =
        Curve::from(Circle::new(frame(&mut rng), Length::from_mm(2.0), Sense::Reversed).unwrap());
    let any = AnyCurve::from(circle.clone());
    assert_eq!(any.domain(), circle.domain());
    for _ in 0..20 {
        let t = unit(&mut rng) * 4.0;
        assert_eq!(any.point(t).to_array(), circle.point(t).to_array());
        for side in SIDES {
            assert_eq!(any.jet::<4>(t, side), circle.jet::<4>(t, side));
        }
    }
}

/// The on-surface arm's domain is its uv curve's; its point is the surface's at the uv
/// point.
#[test]
fn the_on_surface_arm_is_the_surface_at_the_uv_point() {
    let line = Line2::through(Point2::new(0.1, 0.9), Point2::new(0.8, 0.2)).unwrap();
    let s = crease();
    let c = CurveOnSurface::new(Curve2::from(line), s.clone());
    assert_eq!(c.uv(), &Curve2::from(line));
    assert_eq!(**c.surface(), s);
    let any = AnyCurve::from(c.clone());
    assert_eq!(any.domain(), Curve2::from(line).domain());
    for t in [-0.5, 0.0, 0.3, 1.0, 2.0] {
        assert_eq!(any.point(t), s.point(line.point(t)));
        assert_eq!(
            any.jet::<2>(t, Approach::Above),
            c.jet::<2>(t, Approach::Above)
        );
    }
}
