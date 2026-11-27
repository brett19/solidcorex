// Written for solidcorex. Apache-2.0.
//! scx-num's cases against the pin (targets `num` and `num_ic`). Each case is a group
//! `num/<case>` holding `scx` and `pin` over the same seeded inputs, and a group
//! `num/<case>/edge` over its edge inputs (NaN, infinities, degenerate values).
//!
//! The cases: hull folds of values and points (`extent1_fold`, `extent3_fold`), union folds
//! of intervals and boxes (`extent1_union`, `extent3_union`), per element: `normalize`,
//! `transformed_box`, `compose`, `inverse`, `point` and `vector` (a transform applied),
//! `distance`, `vec_mix` (cross, dot, scale, add and subtract in one expression) and
//! `contains` (a point in a box, with tolerance).

use crate::case::{Case, both, same};
use crate::{Rng, edges, seeded};

use scx_num::extent1d::Extent1d as XE1;
use scx_num::extent3d::Extent3d as XE3;
use scx_num::math::Affine;
use scx_num::vector3d::{Point3 as XP3, Vec3 as XV3};
use solidcore::geom::extent1d::Extent1d as PE1;
use solidcore::geom::extent3d::Extent3d as PE3;
use solidcore::geom::vector3d::Vec3 as PV3;
use solidcore::math::Transform;

type M = [[f64; 4]; 4];

const NAN: f64 = f64::NAN;
const INF: f64 = f64::INFINITY;
const MAX: f64 = f64::MAX;

#[allow(clippy::unnecessary_wraps)] // the shape `both` takes
fn xp(a: &[f64; 3]) -> Option<XP3> {
    Some(XP3::new(a[0], a[1], a[2]))
}
#[allow(clippy::unnecessary_wraps)] // the shape `both` takes
fn xv(a: &[f64; 3]) -> Option<XV3> {
    Some(XV3::new(a[0], a[1], a[2]))
}
#[allow(clippy::unnecessary_wraps)] // the shape `both` takes
fn pv(a: &[f64; 3]) -> Option<PV3> {
    Some(PV3::new(a[0], a[1], a[2]))
}
/// Corners `lo`, `hi` as a box: `None` where the scx box cannot hold them.
fn xb(a: &[[f64; 3]; 2]) -> Option<XE3> {
    XE3::from_corners(xp(&a[0])?, xp(&a[1])?)
}
fn pb(a: &[[f64; 3]; 2]) -> Option<PE3> {
    Some(PE3::from_points(pv(&a[0])?, pv(&a[1])?))
}
fn xi(a: &[f64; 2]) -> Option<XE1> {
    XE1::new(a[0], a[1])
}
#[allow(clippy::unnecessary_wraps)] // the shape `both` takes
fn pi(a: &[f64; 2]) -> Option<PE1> {
    Some(PE1::from_min_max(a[0], a[1]))
}
/// The matrix made affine (column 3 `(0, 0, 0, 1)`), as the difftest compares them.
fn affine(mut m: M) -> M {
    for (i, row) in m.iter_mut().enumerate() {
        row[3] = if i == 3 { 1.0 } else { 0.0 };
    }
    m
}
fn xm(m: &M) -> Option<Affine> {
    Affine::try_from(affine(*m)).ok()
}
#[allow(clippy::unnecessary_wraps)] // the shape `both` takes
fn pm(m: &M) -> Option<Transform> {
    Some(Transform { m: affine(*m) })
}

fn point(r: &mut Rng) -> [f64; 3] {
    r.array(-100.0, 100.0)
}
fn interval(r: &mut Rng) -> [f64; 2] {
    let (a, b) = (r.range(-100.0, 100.0), r.range(-100.0, 100.0));
    [a.min(b), a.max(b)]
}
fn boxed(r: &mut Rng) -> [[f64; 3]; 2] {
    let [x, y, z] = [interval(r), interval(r), interval(r)];
    [[x[0], y[0], z[0]], [x[1], y[1], z[1]]]
}
/// A random linear part in `[-2, 2]` and translation in `[-10, 10]`.
fn matrix(r: &mut Rng) -> M {
    let mut m: M = [[0.0; 4]; 4];
    for row in &mut m[..3] {
        *row = r.array(-2.0, 2.0);
    }
    m[3] = r.array(-10.0, 10.0);
    m
}

/// Edge coordinates for points and vectors.
const EDGE_POINTS: [[f64; 3]; 8] = [
    [0.0, 0.0, 0.0],
    [-0.0, -0.0, -0.0],
    [NAN, 1.0, 2.0],
    [INF, 0.0, -INF],
    [1e-300, 0.0, 1e-310],
    [1e300, -1e300, 1e300],
    [5e-324, -5e-324, 0.0],
    [1.0, NAN, NAN],
];

/// Edge boxes: points, the whole `f64` range, reaching to one end of it, huge and denormal
/// (an extent's ends are finite).
const EDGE_BOXES: [[[f64; 3]; 2]; 6] = [
    [[1.0, 2.0, 3.0], [1.0, 2.0, 3.0]],
    [[-MAX, -MAX, -MAX], [MAX, MAX, MAX]],
    [[0.0, -MAX, 0.0], [1.0, 0.0, MAX]],
    [[-1e300, -1e300, -1e300], [1e300, 1e300, 1e300]],
    [[0.0, 0.0, 0.0], [5e-324, 5e-324, 5e-324]],
    [[-0.0, 0.0, -0.0], [0.0, -0.0, 0.0]],
];

/// Edge matrices: zero, rank one, identity, NaN, infinite and huge linear parts.
fn edge_matrices() -> Vec<M> {
    let id: M = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let mut out = vec![[[0.0; 4]; 4], id];
    let mut rank1 = [[1.0; 4]; 4];
    rank1[3] = [0.0; 4];
    out.push(rank1);
    for (i, v) in [NAN, INF, 1e300, 1e-300].into_iter().enumerate() {
        let mut m = id;
        m[i % 3][(i + 1) % 3] = v;
        m[i % 3][i % 3] = v;
        out.push(m);
    }
    out
}

fn folds(out: &mut Vec<Case>) {
    let vals = seeded(1, |r| r.range(-100.0, 100.0));
    let vals_e = edges(2, &[NAN, INF, -INF, -0.0, 0.0, 5e-324, 1e300, 1.0]);
    for (name, v) in [
        ("num/extent1_fold", &vals),
        ("num/extent1_fold/edge", &vals_e),
    ] {
        out.push(Case::fold(
            name,
            same(v),
            |v| XE1::hull(v.iter().copied()),
            |v| {
                let mut e = PE1::new();
                for &x in v {
                    e.add_value(x);
                }
                e
            },
        ));
    }
    let pts = seeded(3, point);
    let pts_e = edges(4, &EDGE_POINTS);
    for (name, raw) in [
        ("num/extent3_fold", &pts),
        ("num/extent3_fold/edge", &pts_e),
    ] {
        let (x, p) = both(raw, xp, pv);
        out.push(Case::fold(
            name,
            (x, p),
            |v| XE3::hull(v.iter().copied()),
            |v| {
                let mut e = PE3::new();
                for &q in v {
                    e.add_point(q);
                }
                e
            },
        ));
    }
    let ivls = seeded(5, interval);
    let ivls_e = edges(
        6,
        &[
            [1.0, 1.0],
            [-MAX, MAX],
            [0.0, MAX],
            [-1e300, 1e300],
            [-0.0, 0.0],
        ],
    );
    for (name, raw) in [
        ("num/extent1_union", &ivls),
        ("num/extent1_union/edge", &ivls_e),
    ] {
        let (x, p) = both(raw, xi, pi);
        out.push(Case::fold(
            name,
            (x, p),
            |v| v.iter().copied().reduce(XE1::union),
            |v| v.iter().fold(PE1::new(), |e, b| e.union(b)),
        ));
    }
    let boxes = seeded(7, boxed);
    let boxes_e = edges(8, &EDGE_BOXES);
    for (name, raw) in [
        ("num/extent3_union", &boxes),
        ("num/extent3_union/edge", &boxes_e),
    ] {
        let (x, p) = both(raw, xb, pb);
        out.push(Case::fold(
            name,
            (x, p),
            |v| v.iter().copied().reduce(XE3::union),
            |v| v.iter().fold(PE3::new(), |e, b| e.union(b)),
        ));
    }
}

fn vectors(out: &mut Vec<Case>) {
    let vs = seeded(11, point);
    let vs_e = edges(12, &EDGE_POINTS);
    for (name, raw) in [("num/normalize", &vs), ("num/normalize/edge", &vs_e)] {
        let (x, p) = both(raw, xv, pv);
        out.push(Case::map(name, (x, p), |v| v.normalized(), |v| v.unitize()));
    }
    let pairs = seeded(13, |r| [point(r), point(r)]);
    let pairs_e = edges(
        14,
        &EDGE_POINTS
            .iter()
            .zip(EDGE_POINTS.iter().rev())
            .map(|(a, b)| [*a, *b])
            .collect::<Vec<_>>(),
    );
    for (name, raw) in [("num/distance", &pairs), ("num/distance/edge", &pairs_e)] {
        let (x, p) = both(
            raw,
            |[a, b]| Some((xp(a)?, xp(b)?)),
            |[a, b]| Some((pv(a)?, pv(b)?)),
        );
        out.push(Case::map(
            name,
            (x, p),
            |(a, b)| a.distance(*b),
            |(a, b)| a.distance(*b),
        ));
    }
    let triples = seeded(15, |r| [point(r), point(r), point(r)]);
    let triples_e = edges(
        16,
        &EDGE_POINTS
            .iter()
            .zip(EDGE_POINTS.iter().rev())
            .zip(EDGE_POINTS.iter().skip(3))
            .map(|((a, b), d)| [*a, *b, *d])
            .collect::<Vec<_>>(),
    );
    for (name, raw) in [("num/vec_mix", &triples), ("num/vec_mix/edge", &triples_e)] {
        let (x, p) = both(
            raw,
            |[a, b, d]| Some((xv(a)?, xv(b)?, xv(d)?)),
            |[a, b, d]| Some((pv(a)?, pv(b)?, pv(d)?)),
        );
        out.push(Case::map(
            name,
            (x, p),
            |&(a, b, d)| a.cross(b) * d.dot(a) + (b - d) * 0.5,
            |&(a, b, d)| a.cross(b) * d.dot(a) + (b - d) * 0.5,
        ));
    }
    let inside = seeded(17, |r| (boxed(r), point(r)));
    let inside_e = edges(
        18,
        &EDGE_BOXES
            .iter()
            .zip(EDGE_POINTS.iter().cycle())
            .map(|(b, p)| (*b, *p))
            .collect::<Vec<_>>(),
    );
    for (name, raw) in [("num/contains", &inside), ("num/contains/edge", &inside_e)] {
        let (x, p) = both(
            raw,
            |(b, q)| Some((xb(b)?, xp(q)?)),
            |(b, q)| Some((pb(b)?, pv(q)?)),
        );
        out.push(Case::map(
            name,
            (x, p),
            |(b, q)| b.contains_point(*q, 1e-9),
            |(b, q)| b.contains_point(*q, 1e-9),
        ));
    }
}

fn transforms(out: &mut Vec<Case>) {
    let ms = seeded(21, matrix);
    let ms_e = edges(22, &edge_matrices());
    for (name, raw) in [("num/inverse", &ms), ("num/inverse/edge", &ms_e)] {
        let (x, p) = both(raw, xm, pm);
        out.push(Case::map(name, (x, p), Affine::inverse, Transform::inverse));
    }
    let pairs = seeded(23, |r| [matrix(r), matrix(r)]);
    let em = edge_matrices();
    let pairs_e = edges(
        24,
        &em.iter()
            .zip(em.iter().rev())
            .map(|(a, b)| [*a, *b])
            .collect::<Vec<_>>(),
    );
    for (name, raw) in [("num/compose", &pairs), ("num/compose/edge", &pairs_e)] {
        let (x, p) = both(
            raw,
            |[a, b]| Some((xm(a)?, xm(b)?)),
            |[a, b]| Some((pm(a)?, pm(b)?)),
        );
        // the pin's `a.concat(&b)` applies `a` first: scx's `b * a`
        out.push(Case::map(
            name,
            (x, p),
            |&(a, b)| b * a,
            |(a, b)| a.concat(b),
        ));
    }
    let with_point = seeded(25, |r| (matrix(r), point(r)));
    let with_point_e = edges(
        26,
        &em.iter()
            .zip(EDGE_POINTS.iter().cycle())
            .map(|(m, q)| (*m, *q))
            .collect::<Vec<_>>(),
    );
    for (case, raw) in [("point", &with_point), ("point/edge", &with_point_e)] {
        let (x, p) = both(
            raw,
            |(m, q)| Some((xm(m)?, xp(q)?)),
            |(m, q)| Some((pm(m)?, pv(q)?)),
        );
        out.push(Case::map(
            &format!("num/{case}"),
            (x, p),
            |(t, q)| t.apply(*q),
            |(t, q)| t.transform_point(*q),
        ));
        let (x, p) = both(
            raw,
            |(m, q)| Some((xm(m)?, xv(q)?)),
            |(m, q)| Some((pm(m)?, pv(q)?)),
        );
        out.push(Case::map(
            &format!("num/vector{}", case.strip_prefix("point").unwrap_or("")),
            (x, p),
            |(t, v)| t.apply_vec(*v),
            |(t, v)| t.transform_vector(*v),
        ));
    }
    let with_box = seeded(27, |r| (matrix(r), boxed(r)));
    let with_box_e = edges(
        28,
        &em.iter()
            .zip(EDGE_BOXES.iter().cycle())
            .map(|(m, b)| (*m, *b))
            .collect::<Vec<_>>(),
    );
    for (name, raw) in [
        ("num/transformed_box", &with_box),
        ("num/transformed_box/edge", &with_box_e),
    ] {
        let (x, p) = both(
            raw,
            |(m, b)| Some((xm(m)?, xb(b)?)),
            |(m, b)| Some((pm(m)?, pb(b)?)),
        );
        out.push(Case::map(
            name,
            (x, p),
            |(t, b)| t.transformed_box(*b),
            |(t, b)| t.transformed_box(b.min(), b.max()),
        ));
    }
}

/// Every case, in order.
#[must_use]
pub fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    folds(&mut out);
    vectors(&mut out);
    transforms(&mut out);
    out
}
