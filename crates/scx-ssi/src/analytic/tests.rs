// Written for solidcorex. Apache-2.0.
//! Unit tests: the pair dispatch and the clipping's spans. The sections themselves are
//! checked against the exact sections in `tests/plane_sections.rs`.

#![allow(clippy::unwrap_used, clippy::panic, clippy::float_cmp)]

use core::f64::consts::{PI, TAU};

use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Accuracy, Length, Resolution};
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Closed, Cylinder, Plane, Sphere, Surface, pcurve_of};

use super::{Intersection, intersect};

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

fn acc() -> [Accuracy; 2] {
    let a = Accuracy::new(Length::from_mm(1e-9)).unwrap();
    [a, a]
}

fn res() -> Resolution {
    Resolution::new(Length::from_mm(1e-9)).unwrap()
}

fn square(h: f64) -> Extent2d {
    Extent2d::from_bounds(-h, -h, h, h).unwrap()
}

fn cylinder() -> (Surface, Extent2d) {
    let c = Cylinder::new(
        frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        Length::from_mm(1.0),
        Sense::Forward,
    )
    .unwrap();
    (
        Surface::Cylinder(c),
        Extent2d::from_bounds(0.0, -2.0, TAU, 2.0).unwrap(),
    )
}

fn tilted() -> (Surface, Extent2d) {
    let p = Plane::new(frame([0.0; 3], [0.3, 0.0, 1.0], [1.0, 0.0, -0.3]));
    (Surface::Plane(p), square(10.0))
}

fn curves(r: &Closed<Intersection>) -> &[super::IntersectionCurve] {
    match r.as_solved() {
        Some(Intersection::Pieces { curves, .. }) => curves,
        other => panic!("{other:?}"),
    }
}

/// The plane second or first: the same curve, run the other way (along `n_a x n_b`).
#[test]
fn order_reverses_direction() {
    let ((c, ce), (p, pe)) = (cylinder(), tilted());
    let ab = intersect(&c, &ce, &p, &pe, acc(), res());
    let ba = intersect(&p, &pe, &c, &ce, acc(), res());
    let (x, y) = (curves(&ab), curves(&ba));
    assert_eq!((x.len(), y.len()), (1, 1));
    let (x, y) = (&x[0], &y[0]);
    let mid = |c: &super::IntersectionCurve| 0.5 * (c.span.min() + c.span.max());
    let (tx, ty) = (mid(x), mid(y));
    let h = 1e-6;
    let dx = x.curve.point(tx + h) - x.curve.point(tx - h);
    let dy = y.curve.point(ty + h) - y.curve.point(ty - h);
    // Both whole ellipses; their tangents at the same point oppose.
    let q = x.curve.point(tx);
    let mut best = (f64::INFINITY, 0.0);
    for i in 0..4096 {
        let t = TAU * f64::from(i) / 4096.0;
        let d = y.curve.point(t).distance(q);
        if d < best.0 {
            best = (d, t);
        }
    }
    let dy_at = y.curve.point(best.1 + h) - y.curve.point(best.1 - h);
    assert!(dx.dot(dy_at) < 0.0, "{dx:?} {dy:?} {dy_at:?}");
}

/// The overlap's pieces as sets of vertex pairs, `(on_a, on_b)`, rounded to `1e-12`.
fn pairs(got: &Closed<Intersection>) -> Vec<Vec<[i64; 4]>> {
    let Closed::Solved(Intersection::Coincident(pieces)) = got else {
        panic!("not coincident: {got:?}");
    };
    let r = |x: f64| (x * 1e12).round() as i64;
    pieces
        .iter()
        .map(|o| {
            assert_eq!(o.on_a.len(), o.on_b.len());
            let mut v: Vec<[i64; 4]> = o
                .on_a
                .iter()
                .zip(&o.on_b)
                .map(|(a, b)| [r(a.x), r(a.y), r(b.x), r(b.y)])
                .collect();
            v.sort_unstable();
            v
        })
        .collect()
}

fn flip(p: &[Vec<[i64; 4]>]) -> Vec<Vec<[i64; 4]>> {
    p.iter()
        .map(|v| {
            let mut w: Vec<[i64; 4]> = v.iter().map(|&[a, b, c, d]| [c, d, a, b]).collect();
            w.sort_unstable();
            w
        })
        .collect()
}

/// Two planes over the same place, either way round: coincident, the overlap the smaller
/// square's in each plane's uv, its pieces swapped with the order asked.
#[test]
fn coincident_overlap_follows_order() {
    let a = Surface::Plane(Plane::new(frame(
        [0.0; 3],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
    )));
    // u along world y, v along world x less a half.
    let b = Surface::Plane(Plane::new(frame(
        [0.5, 0.0, 0.0],
        [0.0, 0.0, -1.0],
        [0.0, 1.0, 0.0],
    )));
    let (ae, be) = (square(1.0), square(2.0));
    let fwd = pairs(&intersect(&a, &ae, &b, &be, acc(), res()));
    let r = |x: f64| (x * 1e12).round() as i64;
    let mut want: Vec<[i64; 4]> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .iter()
        .map(|&(x, y): &(f64, f64)| [r(x), r(y), r(y), r(x - 0.5)])
        .collect();
    want.sort_unstable();
    assert_eq!(fwd, vec![want]);
    assert_eq!(
        pairs(&intersect(&b, &be, &a, &ae, acc(), res())),
        flip(&fwd)
    );
}

/// A square and the same square turned an eighth of a turn: an octagon, each vertex on
/// both squares' edges.
#[test]
fn coincident_planes_turned_give_an_octagon() {
    let a = Surface::Plane(Plane::new(frame(
        [0.0; 3],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
    )));
    let h = core::f64::consts::FRAC_1_SQRT_2;
    let b = Surface::Plane(Plane::new(frame([0.0; 3], [0.0, 0.0, 1.0], [h, h, 0.0])));
    let e = square(1.0);
    let Closed::Solved(Intersection::Coincident(p)) = intersect(&a, &e, &b, &e, acc(), res())
    else {
        panic!()
    };
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].on_a.len(), 8, "{p:?}");
    for (q, w) in p[0].on_a.iter().zip(&p[0].on_b) {
        let edge = |x: f64, y: f64| (x.abs().max(y.abs()) - 1.0).abs() <= 1e-12;
        assert!(edge(q.x, q.y) && edge(w.x, w.y), "{q:?} {w:?}");
        assert!((fmath::hypot(q.x, q.y) - fmath::hypot(w.x, w.y)).abs() <= 1e-12);
    }
}

/// Coincident planes whose squares share an edge (within the tolerance): a piece of no
/// width along it; a little further apart: empty.
#[test]
fn coincident_planes_touching_and_apart() {
    let a = Surface::Plane(Plane::new(frame(
        [0.0; 3],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
    )));
    let at = |x: f64| {
        Surface::Plane(Plane::new(frame(
            [x, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
        )))
    };
    let e = square(1.0);
    let Closed::Solved(Intersection::Coincident(p)) =
        intersect(&a, &e, &at(2.0 + 5e-10), &e, acc(), res())
    else {
        panic!()
    };
    assert_eq!(p.len(), 1);
    assert!(p[0].on_a.iter().all(|q| q.x == 1.0), "{p:?}");
    assert!(p[0].on_b.iter().all(|q| q.x == -1.0), "{p:?}");
    assert_eq!(
        intersect(&a, &e, &at(2.0 + 1e-6), &e, acc(), res()),
        Closed::Solved(Intersection::Empty)
    );
}

/// Curved pairs off one axis are not solved here yet: a sphere whose centre is off the
/// cylinder's axis.
#[test]
fn other_pairs_are_general() {
    let (c, ce) = cylinder();
    let s = Surface::Sphere(
        Sphere::new(
            frame([0.5, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            Length::from_mm(2.0),
            Sense::Forward,
        )
        .unwrap(),
    );
    let se = Extent2d::from_bounds(0.0, 0.0, 1.0, 1.0).unwrap();
    assert!(intersect(&c, &ce, &s, &se, acc(), res()).is_general());
}

/// A circle clipped across the turn's start merges into one span that passes `2 pi`.
#[test]
fn span_merges_across_turn_start() {
    let (c, _) = cylinder();
    // A plane square to the axis over x in [0.5, 2]: the arc about the x axis, where the
    // turn starts.
    let ce = Extent2d::from_bounds(0.0, -2.0, TAU, 2.0).unwrap();
    let p = Surface::Plane(Plane::new(frame(
        [0.0; 3],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
    )));
    let pe = Extent2d::from_bounds(0.5, -2.0, 2.0, 2.0).unwrap();
    let r = intersect(&c, &ce, &p, &pe, acc(), res());
    let x = curves(&r);
    assert_eq!(x.len(), 1, "{x:?}");
    let span = x[0].span;
    // The arc x >= 0.5 is |angle| <= pi / 3, one span from 5 pi / 3 to 7 pi / 3 or its
    // reverse.
    assert!(
        (span.max() - span.min() - 2.0 * PI / 3.0).abs() < 1e-9,
        "{span:?}"
    );
    assert!(span.min() >= 0.0 && span.min() < TAU);
    assert!(x[0].curve.point(0.5 * (span.min() + span.max())).x > 0.99);
}

/// The uv seam: a piece's curve over its span has its pcurve from `pcurve_of` on both
/// surfaces (a square cut of a cylinder is a parallel).
#[test]
fn pieces_take_their_pcurves() {
    let (c, ce) = cylinder();
    let p = Surface::Plane(Plane::new(frame(
        [0.0, 0.0, 0.5],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
    )));
    let pe = square(2.0);
    let r = intersect(&c, &ce, &p, &pe, acc(), res());
    let x = curves(&r);
    assert_eq!(x.len(), 1, "{x:?}");
    let tol = Resolution::new(Length::from_mm(2e-9)).unwrap();
    for s in [&c, &p] {
        assert!(pcurve_of(&x[0].curve, s, x[0].span, tol).is_ok());
    }
}
