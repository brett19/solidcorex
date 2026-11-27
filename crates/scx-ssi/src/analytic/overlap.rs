// Written for solidcorex. Apache-2.0.
//! The overlap of two coincident patches (research 63 SS4): the pieces of
//! [`Intersection::Coincident`](super::Intersection::Coincident).
//!
//! Wherever the closed forms find two surfaces coincident, the map from one surface's uv to
//! the other's is affine, so the overlap of two rectangles of uv is exact as polygons:
//!
//! - two planes: the second's rectangle, carried into the first's uv (a rotation and a
//!   shift), clipped by the first's rectangle: a convex polygon of up to eight vertices;
//! - two surfaces of revolution on one axis: each coordinate maps alone (an angle about the
//!   axis turns and may reverse, a height shifts and may reverse, a latitude or a tube angle
//!   may reverse), so the overlap is a product of intervals: a rectangle, or up to two per
//!   angle where an arc wraps past the other's start (four at most).
//!
//! The exact overlap comes first; where there is none, the clip is taken again with the
//! tolerance as slack: patches that touch within it give a piece of no width, and patches
//! apart by more than it are empty. Every vertex is clamped into both extents, so it lies
//! inside both, and the two of a pair are one point to rounding (a touch: within the
//! tolerance).

use core::f64::consts::{FRAC_PI_2, TAU};

use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::vector2d::Point2;
use scx_num::vector3d::Point3;
use scx_surface::{Plane, Surface};

use super::Overlap;

/// The overlap of two coincident planes' rectangles, `None` when they are apart by more
/// than `tol` (module docs).
pub(super) fn planes(
    a: &Plane,
    ae: &Extent2d,
    b: &Plane,
    be: &Extent2d,
    tol: f64,
) -> Option<Overlap> {
    let uv_on = |k: &Plane, q: Point3| {
        let f = k.frame();
        let d = q - f.origin();
        Point2::new(d.dot(*f.x()), d.dot(*f.y()))
    };
    let start: Vec<Point2> = corners(be).iter().map(|&p| uv_on(a, b.point(p))).collect();
    // The exact overlap first; failing it, a touch within the tolerance.
    let poly = [0.0, tol]
        .into_iter()
        .map(|slack| clip(&start, ae, slack))
        .find(|p| !p.is_empty())?;
    let on_a: Vec<Point2> = dedup(poly.iter().map(|&p| clamp(p, ae)).collect());
    let on_b = on_a
        .iter()
        .map(|&p| clamp(uv_on(b, a.point(p)), be))
        .collect();
    Some(Overlap { on_a, on_b })
}

/// The convex polygon `poly` cut to the rectangle `e` grown by `slack`.
fn clip(poly: &[Point2], e: &Extent2d, slack: f64) -> Vec<Point2> {
    let (lo, hi) = (e.min(), e.max());
    let mut poly = poly.to_vec();
    for (on_y, bound, sign) in [
        (false, lo.x - slack, -1.0),
        (false, hi.x + slack, 1.0),
        (true, lo.y - slack, -1.0),
        (true, hi.y + slack, 1.0),
    ] {
        let side = |p: Point2| sign * (if on_y { p.y } else { p.x } - bound);
        poly = half(&poly, side);
        if poly.is_empty() {
            break;
        }
    }
    poly
}

/// A rectangle's corners in turn (counter-clockwise).
fn corners(e: &Extent2d) -> [Point2; 4] {
    let (lo, hi) = (e.min(), e.max());
    [lo, Point2::new(hi.x, lo.y), hi, Point2::new(lo.x, hi.y)]
}

/// The convex polygon `poly` cut to `side(p) <= 0` (Sutherland and Hodgman's step).
fn half(poly: &[Point2], side: impl Fn(Point2) -> f64) -> Vec<Point2> {
    let mut out = Vec::with_capacity(poly.len() + 1);
    for (&p, &q) in poly.iter().zip(poly.iter().cycle().skip(1)) {
        let (fp, fq) = (side(p), side(q));
        if fp <= 0.0 {
            out.push(p);
        }
        if (fp < 0.0 && fq > 0.0) || (fp > 0.0 && fq < 0.0) {
            let t = fp / (fp - fq);
            out.push(p + (q - p) * t);
        }
    }
    out
}

fn clamp(p: Point2, e: &Extent2d) -> Point2 {
    let (lo, hi) = (e.min(), e.max());
    Point2::new(p.x.clamp(lo.x, hi.x), p.y.clamp(lo.y, hi.y))
}

/// `poly` without a vertex equal to the one before it (round the turn).
fn dedup(mut poly: Vec<Point2>) -> Vec<Point2> {
    poly.dedup();
    while poly.len() > 1 && poly.first() == poly.last() {
        poly.pop();
    }
    poly
}

/// One coordinate of a coincidence of two surfaces of revolution: the second's value at
/// the first's `x` is `s (x - shift)`, give or take turns when `turns`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Map1 {
    pub(super) s: f64,
    pub(super) shift: f64,
    pub(super) turns: bool,
}

/// A piece of one coordinate: the first's interval, and the second's values at its ends.
#[derive(Clone, Copy, Debug)]
pub(super) struct Span1 {
    pub(super) a: [f64; 2],
    pub(super) b: [f64; 2],
}

/// The pieces where the intervals `a` (the first's) and `b` (the second's) overlap under
/// `m`: an exact overlap's pieces; else, where they are within `touch` of each other, a
/// piece of no width at the gap; else none when they are more than `apart` apart, and
/// `None` (undecided) between.
pub(super) fn spans(
    a: [f64; 2],
    b: [f64; 2],
    m: Map1,
    touch: f64,
    apart: f64,
) -> Option<Vec<Span1>> {
    let (c0, c1) = {
        let (x, y) = (m.shift + m.s * b[0], m.shift + m.s * b[1]);
        (x.min(y), x.max(y))
    };
    let (k0, k1) = if m.turns {
        (
            ((a[0] - c1) / TAU).floor() - 1.0,
            ((a[1] - c0) / TAU).ceil() + 1.0,
        )
    } else {
        (0.0, 0.0)
    };
    let at = |x: f64, off: f64| (m.s * (x - off - m.shift)).clamp(b[0], b[1]);
    let piece = |lo: f64, hi: f64, off: f64| Span1 {
        a: [lo, hi],
        b: [at(lo, off), at(hi, off)],
    };
    let mut out = Vec::new();
    let mut nearest: Option<(f64, f64, f64)> = None;
    let mut k = k0;
    while k <= k1 {
        let off = k * TAU;
        let (lo, hi) = (a[0].max(c0 + off), a[1].min(c1 + off));
        if lo < hi {
            out.push(piece(lo, hi, off));
        } else if nearest.is_none_or(|(g, _, _)| lo - hi < g) {
            nearest = Some((lo - hi, 0.5 * (lo + hi), off));
        }
        k += 1.0;
    }
    if !out.is_empty() {
        return Some(out);
    }
    let (gap, mid, off) = nearest?;
    if gap <= touch {
        let x = mid.clamp(a[0], a[1]);
        return Some(vec![piece(x, x, off)]);
    }
    (gap > apart).then(Vec::new)
}

/// The angle past which two arcs at distance at least `rho_min` from the axis are apart by
/// more than `slack`: `asin(slack / rho_min)` (every gap `g` keeps them `rho_min sin g`
/// apart, `rho_min` past a right angle), unbounded when `rho_min <= slack`.
pub(super) fn apart_angle(rho_min: f64, slack: f64) -> f64 {
    if rho_min > slack {
        fmath::asin(slack / rho_min).min(FRAC_PI_2)
    } else {
        f64::INFINITY
    }
}

/// The rectangles of `u` pieces times `v` pieces, in STEP's coordinates, as overlaps in each
/// surface's D13 uv.
pub(super) fn rectangles(
    a: &Surface,
    b: &Surface,
    us: &[Span1],
    vs: &[Span1],
) -> Option<Vec<Overlap>> {
    let mut out = Vec::with_capacity(us.len() * vs.len());
    for u in us {
        for v in vs {
            let corners = |s: &Surface, [u0, u1]: [f64; 2], [v0, v1]: [f64; 2]| {
                [(u0, v0), (u1, v0), (u1, v1), (u0, v1)]
                    .into_iter()
                    .map(|(x, y)| from_step(s, Point2::new(x, y)))
                    .collect::<Option<Vec<_>>>()
            };
            let on_a = corners(a, u.a, v.a)?;
            let on_b = corners(b, u.b, v.b)?;
            out.push(Overlap { on_a, on_b });
        }
    }
    Some(out)
}

/// D13's uv at STEP's `(u, v)` on a surface of revolution.
fn from_step(s: &Surface, p: Point2) -> Option<Point2> {
    Some(match s {
        Surface::Cylinder(k) => k.from_step(p),
        Surface::Cone(k) => k.from_step(p),
        Surface::Sphere(k) => k.from_step(p),
        Surface::Torus(k) => k.from_step(p),
        _ => return None,
    })
}
