// Written for solidcorex. Apache-2.0.
//! Certified boxes of a surface over a uv box (research 63 §6.9, BR4): [`Surface::box_over`].
//!
//! Each analytic kind is evaluated in intervals over parts of the box: cut at its arc breaks
//! (each arc is its own rational formula, so a part never crosses one) and each arc in
//! quarters, the parts' hull taken. A NURBS surface is cut at its knot lines, each part's
//! polynomial piece enclosed ([`NurbsSurface::partials_over_piece`]) and, where the box lies
//! in the domain, met with the control points that act on it (the convex-hull property).
//! Never a sampled box: every point of the surface over the box lies in the result, for any
//! box, inside the domain or past it where the kind continues.

use core::ops::{Add, Div, Mul, Sub};
use scx_curve::arcs::Arcs;
use scx_num::extent2d::Extent2d;
use scx_num::extent3d::Extent3d;
use scx_num::interval::Ivl;

use scx_num::ring::Ring;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{Approach, KnotVector};
use scx_nurbs_surface::{About, EnclosureCause, NurbsSurface, NurbsSurfaceError, ParamAxis};

use crate::surface::Surface;
use crate::{cone, sphere, torus};

/// The parts each arc is cut into. An interval form's excess shrinks with the part's width
/// (first order); on a full turn of a tilted unit cylinder a quarter arc whole gives sides
/// 1.6 to 1.9 times the exact ones, in quarters 1.1 to 1.24 for about four times the cost
/// (63 §6.9 has the table, and why not Taylor models).
const PER_ARC: usize = 4;

/// The parts a cone's height is cut into: `v` appears twice in its formula (the radius
/// and the height), so the interval form widens with the height's width.
const CONE_HEIGHT: usize = 2;

/// The most knot-span patches a NURBS box encloses one by one; past it the control points
/// alone bound it when they may (a fine net), else every patch is still enclosed.
const MAX_PATCHES: usize = 64;

/// The bisections a patch whose weights the form cannot prove positive is given.
const BISECT: u32 = 6;

impl Surface {
    /// A box holding `S(u, v)` for every `(u, v)` in `uv`, certified: interval arithmetic
    /// on the kind's own formulas, rounded outward, never samples. Past the domain it holds
    /// the kind's continuation (a NURBS surface's end pieces, a sphere's end arcs).
    ///
    /// `None` when the box cannot be enclosed: a value not finite, or a periodic range of
    /// over a thousand turns.
    #[must_use]
    pub fn box_over(&self, uv: Extent2d) -> Option<Extent3d> {
        let u = (uv.min().x, uv.max().x);
        let v = (uv.min().y, uv.max().y);
        match self {
            Self::Plane(s) => analytic(u, v, (None, 1), (None, 1), |a, b, n| s.eval(a, b, n)),
            Self::Cylinder(s) => analytic(u, v, (Some(s.arcs()), PER_ARC), (None, 1), |a, b, n| {
                s.eval(a, b, n)
            }),
            Self::Cone(s) => analytic(
                u,
                v,
                (Some(cone::U), PER_ARC),
                (None, CONE_HEIGHT),
                |a, b, n| s.eval(a, b, n),
            ),
            Self::Sphere(s) => analytic(
                u,
                v,
                (Some(sphere::U), PER_ARC),
                (Some(sphere::V), PER_ARC),
                |a, b, n| s.eval(a, b, n),
            ),
            Self::Torus(s) => analytic(
                u,
                v,
                (Some(torus::TURN), PER_ARC),
                (Some(torus::TURN), PER_ARC),
                |a, b, n| s.eval(a, b, n),
            ),
            Self::Nurbs(n) => nurbs(n, uv),
        }
    }

    /// An interval holding `dir . S(u, v)` for every `(u, v)` in `uv`, certified as
    /// [`Surface::box_over`] is (outward interval arithmetic, never samples): the range of
    /// the surface along a direction, for proofs that two pieces lie on opposite sides of a
    /// plane across `dir`. `dir` is any vector, taken as given (the range scales with its
    /// length).
    ///
    /// Each part of the box (cut at the arc breaks of an analytic kind, whole between them,
    /// and at the knot lines of a NURBS surface) is the mean-value form `dir . S(m) + (dir . S_u) [-hu, hu] + (dir . S_v) [-hv, hv]`
    /// about the part's middle `m`, with the partials enclosed over the part (an analytic
    /// kind's by the same formulas in a ring of values with their two partials, a NURBS
    /// surface's by [`NurbsSurface::partials_over_piece`]), met with the natural range of
    /// the part. A piece seen along its normal then has a width second order in its size.
    ///
    /// `None` when the box cannot be enclosed (as [`Surface::box_over`]).
    #[must_use]
    pub fn range_along(&self, dir: Vec3, uv: Extent2d) -> Option<Ivl> {
        self.along(dir, uv).map(|a| a.range)
    }

    /// [`Surface::range_along`] with the enclosures of the two partials of `dir . S` over
    /// the same box: where either excludes 0, `dir . S` has no critical point in `uv`, so
    /// over a region its extremes are on the region's boundary. A partial is the hull over
    /// the parts of the box (a NURBS surface's knot lines among them, so across a knot
    /// where it is only continuous the partial holds both sides' values).
    ///
    /// `None` when the box cannot be enclosed (as [`Surface::box_over`]).
    #[must_use]
    pub fn along(&self, dir: Vec3, uv: Extent2d) -> Option<Along> {
        let u = (uv.min().x, uv.max().x);
        let v = (uv.min().y, uv.max().y);
        let d = [Ivl::from(dir.x), Ivl::from(dir.y), Ivl::from(dir.z)];
        // The kind's own formula twice, in the ring of values with partials and in intervals.
        macro_rules! along {
            ($s:expr, $cu:expr, $cv:expr) => {{
                let s = $s;
                along_parts(
                    u,
                    v,
                    $cu,
                    $cv,
                    d,
                    |a, b, n| s.eval(a, b, n),
                    |a, b, n| s.eval(a, b, n),
                )
            }};
        }
        match self {
            Self::Plane(s) => along!(s, (None, 1), (None, 1)),
            Self::Cylinder(s) => along!(s, (Some(s.arcs()), 1), (None, 1)),
            Self::Cone(s) => along!(s, (Some(cone::U), 1), (None, 1)),
            Self::Sphere(s) => along!(s, (Some(sphere::U), 1), (Some(sphere::V), 1)),
            Self::Torus(s) => along!(s, (Some(torus::TURN), 1), (Some(torus::TURN), 1)),
            Self::Nurbs(n) => nurbs_along(n, d, uv),
        }
    }
}

/// The range of `dir . S` over a box and the ranges of its two partials there (see
/// [`Surface::along`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Along {
    /// Holds `dir . S(u, v)` over the box.
    pub range: Ivl,
    /// Holds `dir . S_u` over the box.
    pub du: Ivl,
    /// Holds `dir . S_v` over the box.
    pub dv: Ivl,
}

impl Along {
    /// The hull of two.
    fn hull(self, b: Self) -> Self {
        Self {
            range: self.range.hull(b.range),
            du: self.du.hull(b.du),
            dv: self.dv.hull(b.dv),
        }
    }
}

/// A value with its two partials, each an enclosure over the same box: the ring the kinds'
/// formulas are evaluated in to enclose their partials (forward-mode differentiation in
/// interval arithmetic). A quotient by a value that holds 0 is the whole line.
#[derive(Clone, Copy)]
struct Grad {
    f: Ivl,
    u: Ivl,
    v: Ivl,
}

impl Add for Grad {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self {
            f: self.f + b.f,
            u: self.u + b.u,
            v: self.v + b.v,
        }
    }
}

impl Sub for Grad {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self {
            f: self.f - b.f,
            u: self.u - b.u,
            v: self.v - b.v,
        }
    }
}

impl Mul for Grad {
    type Output = Self;
    fn mul(self, b: Self) -> Self {
        Self {
            f: self.f * b.f,
            u: self.u * b.f + self.f * b.u,
            v: self.v * b.f + self.f * b.v,
        }
    }
}

impl Div for Grad {
    type Output = Self;
    fn div(self, b: Self) -> Self {
        if !(b.f.lo > 0.0 || b.f.hi < 0.0) {
            let w = Ivl::whole();
            return Self { f: w, u: w, v: w };
        }
        // q = a / b, and (a / b)' = (a' - q b') / b.
        let q = self.f / b.f;
        Self {
            f: q,
            u: (self.u - q * b.u) / b.f,
            v: (self.v - q * b.v) / b.f,
        }
    }
}

impl Ring for Grad {
    fn lift(d: f64) -> Self {
        Self {
            f: Ivl::from(d),
            u: Ivl::from(0.0),
            v: Ivl::from(0.0),
        }
    }
}

/// The hull over the parts of `u x v` of each part's mean-value range along `d` (see
/// [`Surface::range_along`]): `eval_g` is the kind's formula in [`Grad`], `eval_i` in
/// intervals.
fn along_parts(
    u: (f64, f64),
    v: (f64, f64),
    cut_u: (Option<Arcs>, usize),
    cut_v: (Option<Arcs>, usize),
    d: [Ivl; 3],
    eval_g: impl Fn(Grad, Grad, Point2) -> Point3<Grad>,
    eval_i: impl Fn(Ivl, Ivl, Point2) -> Point3<Ivl>,
) -> Option<Along> {
    let us = parts(u, cut_u)?;
    let vs = parts(v, cut_v)?;
    let mut out: Option<Along> = None;
    for &(u0, u1) in &us {
        for &(v0, v1) in &vs {
            let (ui, vi) = (Ivl::new(u0, u1), Ivl::new(v0, v1));
            let near = Point2::new(u0, v0);
            let (mu, mv) = (ui.mid(), vi.mid());
            let one = Ivl::from(1.0);
            let zero = Ivl::from(0.0);
            let g = eval_g(
                Grad {
                    f: ui,
                    u: one,
                    v: zero,
                },
                Grad {
                    f: vi,
                    u: zero,
                    v: one,
                },
                near,
            );
            // The kind's formula in intervals over the part: tight where it is monotone per arc.
            let natural = dot_ivl(d, eval_i(ui, vi, near));
            let su = dot_ivl(d, Point3::new(g.x.u, g.y.u, g.z.u));
            let sv = dot_ivl(d, Point3::new(g.x.v, g.y.v, g.z.v));
            let m = dot_ivl(d, eval_i(Ivl::from(mu), Ivl::from(mv), near));
            let (hu, hv) = (ivl_half(ui, mu), ivl_half(vi, mv));
            let mean = m + su * Ivl::new(-hu, hu) + sv * Ivl::new(-hv, hv);
            let r = meet(mean, natural);
            if !r.is_finite() {
                return None;
            }
            let a = Along {
                range: r,
                du: su,
                dv: sv,
            };
            out = Some(out.map_or(a, |o| o.hull(a)));
        }
    }
    out
}

/// `d . p` in interval arithmetic.
fn dot_ivl(d: [Ivl; 3], p: Point3<Ivl>) -> Ivl {
    d[0] * p.x + d[1] * p.y + d[2] * p.z
}

/// The range along `d` of a NURBS surface over `uv`: the hull over its knot-span parts of
/// each part's mean-value range (see [`Surface::range_along`]).
fn nurbs_along(n: &NurbsSurface<3>, d: [Ivl; 3], uv: Extent2d) -> Option<Along> {
    let ku = n.knot_vector(ParamAxis::U);
    let kv = n.knot_vector(ParamAxis::V);
    let us = knot_parts(ku, uv.min().x, uv.max().x)?;
    let vs = knot_parts(kv, uv.min().y, uv.max().y)?;
    let mut out: Option<Along> = None;
    for &(u0, u1, su) in &us {
        for &(v0, v1, sv) in &vs {
            let r = patch_along(n, d, [su, sv], [Ivl::new(u0, u1), Ivl::new(v0, v1)], BISECT)?;
            out = Some(out.map_or(r, |o| o.hull(r)));
        }
    }
    out
}

/// The mean-value range along `d` of the piece on `span` over `uv`, bisected where the
/// form cannot prove its weights positive, `depth` times at most.
fn patch_along(
    n: &NurbsSurface<3>,
    d: [Ivl; 3],
    span: [usize; 2],
    uv: [Ivl; 2],
    depth: u32,
) -> Option<Along> {
    let dot = |a: [Ivl; 3]| d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
    let [u, v] = uv;
    let (mu, mv) = (u.mid(), v.mid());
    let whole = n.partials_over_piece::<1>(span, uv, About::Middle);
    let mid = n.partials_over_piece::<0>(span, [Ivl::from(mu), Ivl::from(mv)], About::Middle);
    match (whole, mid) {
        (Ok(j), Ok(m)) => {
            let (Some(su), Some(sv)) = (j.get(1, 0), j.get(0, 1)) else {
                return None;
            };
            // |x - m| over the part, each end rounded outward.
            let hu = Ivl::new(-ivl_half(u, mu), ivl_half(u, mu));
            let hv = Ivl::new(-ivl_half(v, mv), ivl_half(v, mv));
            let mean = dot(m.point()) + dot(su) * hu + dot(sv) * hv;
            let boxed = dot(j.point());
            let (du, dv) = (dot(su), dot(sv));
            Some(Along {
                range: meet(mean, boxed),
                du,
                dv,
            })
        }
        (Err(NurbsSurfaceError::Enclosure(EnclosureCause::WeightsNotPositive { .. })), _)
        | (_, Err(NurbsSurfaceError::Enclosure(EnclosureCause::WeightsNotPositive { .. })))
            if depth > 0 =>
        {
            let halves = if u.hi - u.lo >= v.hi - v.lo {
                [[Ivl::new(u.lo, mu), v], [Ivl::new(mu, u.hi), v]]
            } else {
                [[u, Ivl::new(v.lo, mv)], [u, Ivl::new(mv, v.hi)]]
            };
            let a = patch_along(n, d, span, halves[0], depth - 1)?;
            let b = patch_along(n, d, span, halves[1], depth - 1)?;
            Some(a.hull(b))
        }
        _ => None,
    }
}

/// The distance from `m` to the far end of `t`, rounded up: `|x - m| <= this` over `t`.
fn ivl_half(t: Ivl, m: f64) -> f64 {
    let a = Ivl::from(t.hi) - Ivl::from(m);
    let b = Ivl::from(m) - Ivl::from(t.lo);
    a.hi.max(b.hi)
}

/// The meet of two enclosures of the same value (so they intersect); were rounding to
/// part them, the first stands.
fn meet(a: Ivl, b: Ivl) -> Ivl {
    let (lo, hi) = (a.lo.max(b.lo), a.hi.min(b.hi));
    if lo <= hi { Ivl::new(lo, hi) } else { a }
}

/// The hull of `eval` over the parts of `u × v`, each evaluated with the formula of the arc
/// that holds its start (its own arc: a part ends at the next break).
fn analytic(
    u: (f64, f64),
    v: (f64, f64),
    cut_u: (Option<Arcs>, usize),
    cut_v: (Option<Arcs>, usize),
    eval: impl Fn(Ivl, Ivl, Point2) -> Point3<Ivl>,
) -> Option<Extent3d> {
    analytic_with(u, v, cut_u, cut_v, eval, ivl_box, Extent3d::union)
}

/// [`analytic`] with the finish of each part (`fin`) and the join of the parts' results.
fn analytic_with<R>(
    u: (f64, f64),
    v: (f64, f64),
    cut_u: (Option<Arcs>, usize),
    cut_v: (Option<Arcs>, usize),
    eval: impl Fn(Ivl, Ivl, Point2) -> Point3<Ivl>,
    fin: impl Fn(Point3<Ivl>) -> Option<R>,
    join: impl Fn(R, R) -> R,
) -> Option<R> {
    let us = parts(u, cut_u)?;
    let vs = parts(v, cut_v)?;
    let mut out: Option<R> = None;
    for &(u0, u1) in &us {
        for &(v0, v1) in &vs {
            let b = fin(eval(
                Ivl::new(u0, u1),
                Ivl::new(v0, v1),
                Point2::new(u0, v0),
            ))?;
            out = Some(match out {
                None => b,
                Some(o) => join(o, b),
            });
        }
    }
    out
}

/// `[lo, hi]` cut at the breaks of `arcs` strictly inside it, and each piece in `per` equal
/// parts, consecutive parts sharing their ends bit for bit. A range over more than two
/// turns gives way to one whole turn (the surface over it is the same set). `None` when
/// the range is not finite or spans over a thousand turns.
fn parts((lo, hi): (f64, f64), (arcs, per): (Option<Arcs>, usize)) -> Option<Vec<(f64, f64)>> {
    let mut cuts = vec![lo];
    if let Some(a) = arcs {
        let breaks = a.breaks(lo, hi)?;
        if breaks.len() > 8 {
            // Breaks 0..=4 bound one whole turn.
            cuts = breaks.get(..5)?.to_vec();
        } else {
            cuts.extend(breaks);
            cuts.push(hi);
        }
    } else {
        cuts.push(hi);
    }
    let mut out = Vec::with_capacity((cuts.len() - 1) * per);
    for (&a, &b) in cuts.iter().zip(cuts.iter().skip(1)) {
        let n = if b > a { per } else { 1 };
        let mut prev = a;
        for i in 1..=n {
            let x = if i == n {
                b
            } else {
                (a + (b - a) * (i as f64 / n as f64)).clamp(prev, b)
            };
            out.push((prev, x));
            prev = x;
        }
    }
    Some(out)
}

/// The box of an interval point, `None` when a bound is not finite.
fn ivl_box(p: Point3<Ivl>) -> Option<Extent3d> {
    Extent3d::from_bounds(p.x.lo, p.y.lo, p.z.lo, p.x.hi, p.y.hi, p.z.hi)
}

/// A NURBS surface's box: each knot-span patch of the box enclosed on its piece, met with
/// the control hull where it holds; the hull alone past [`MAX_PATCHES`].
fn nurbs(n: &NurbsSurface<3>, uv: Extent2d) -> Option<Extent3d> {
    let ku = n.knot_vector(ParamAxis::U);
    let kv = n.knot_vector(ParamAxis::V);
    let us = knot_parts(ku, uv.min().x, uv.max().x)?;
    let vs = knot_parts(kv, uv.min().y, uv.max().y)?;
    let hull = control_hull(n, uv);
    if us.len().saturating_mul(vs.len()) > MAX_PATCHES
        && let Some(h) = hull
    {
        return Some(h);
    }
    let mut out: Option<Extent3d> = None;
    for &(u0, u1, su) in &us {
        for &(v0, v1, sv) in &vs {
            let b = patch(n, [su, sv], [Ivl::new(u0, u1), Ivl::new(v0, v1)], BISECT)?;
            out = Some(out.map_or(b, |o| o.union(b)));
        }
    }
    let out = out?;
    // Both hold the surface, so they meet; were rounding to part them, the enclosure stands.
    Some(hull.and_then(|h| out.intersect(h)).unwrap_or(out))
}

/// `[lo, hi]` cut at the distinct knots strictly inside it, each part with the span whose
/// piece holds it (past an end, the end span: the surface continues it). `None` when the
/// range is not finite.
fn knot_parts(k: &KnotVector, lo: f64, hi: f64) -> Option<Vec<(f64, f64, usize)>> {
    if !(lo.is_finite() && hi.is_finite() && lo <= hi) {
        return None;
    }
    let knots = k.knots();
    let (a, b) = (
        knots.partition_point(|&x| x <= lo),
        knots.partition_point(|&x| x < hi),
    );
    // `a > b` for a range of zero width on a knot: none lie strictly inside.
    let inner = knots.get(a..b.max(a)).unwrap_or_default();
    let mut cuts = vec![lo];
    for &x in inner {
        if cuts.last() != Some(&x) {
            cuts.push(x);
        }
    }
    cuts.push(hi);
    cuts.iter()
        .zip(cuts.iter().skip(1))
        .map(|(&a, &b)| Some((a, b, k.span(a, Approach::Above).ok()?)))
        .collect()
}

/// The box of the piece on `span` over `uv`, bisected (the wider side first) where the
/// form cannot prove its weights positive, `depth` times at most.
fn patch(n: &NurbsSurface<3>, span: [usize; 2], uv: [Ivl; 2], depth: u32) -> Option<Extent3d> {
    match n.partials_over_piece::<0>(span, uv, About::Middle) {
        Ok(j) => {
            let p = j.point();
            Extent3d::from_bounds(p[0].lo, p[1].lo, p[2].lo, p[0].hi, p[1].hi, p[2].hi)
        }
        Err(NurbsSurfaceError::Enclosure(EnclosureCause::WeightsNotPositive { .. }))
            if depth > 0 =>
        {
            let [u, v] = uv;
            let halves = if u.hi - u.lo >= v.hi - v.lo {
                let m = u.mid();
                [[Ivl::new(u.lo, m), v], [Ivl::new(m, u.hi), v]]
            } else {
                let m = v.mid();
                [[u, Ivl::new(v.lo, m)], [u, Ivl::new(m, v.hi)]]
            };
            let a = patch(n, span, halves[0], depth - 1)?;
            let b = patch(n, span, halves[1], depth - 1)?;
            Some(a.union(b))
        }
        Err(_) => None,
    }
}

/// The box of the control points acting on `uv` (spans `span(lo) - p ..= span(hi)` each
/// way), which holds the surface there when every weight among them is positive (the
/// convex-hull property); `None` when `uv` leaves the domain (the continuation past it is
/// not in the hull) or a weight is not positive. Exact: a min and max of stored values.
fn control_hull(n: &NurbsSurface<3>, uv: Extent2d) -> Option<Extent3d> {
    let d = n.domain();
    let inside = d.min().x <= uv.min().x
        && uv.max().x <= d.max().x
        && d.min().y <= uv.min().y
        && uv.max().y <= d.max().y;
    if !inside {
        return None;
    }
    let range = |axis: ParamAxis, lo: f64, hi: f64| -> Option<(usize, usize)> {
        let k = n.knot_vector(axis);
        let a = k.span(lo, Approach::Above).ok()?;
        let b = k.span(hi, Approach::Below).ok()?;
        Some((a.checked_sub(k.degree())?, b.max(a)))
    };
    let (i0, i1) = range(ParamAxis::U, uv.min().x, uv.max().x)?;
    let (j0, j1) = range(ParamAxis::V, uv.min().y, uv.max().y)?;
    let pts = n.points();
    let w = n.weights();
    let mut corners = Vec::with_capacity((i1 - i0 + 1) * (j1 - j0 + 1));
    for i in i0..=i1 {
        for j in j0..=j1 {
            if let Some(w) = w
                && w.get(i, j)?.partial_cmp(&0.0) != Some(core::cmp::Ordering::Greater)
            {
                return None;
            }
            let p = pts.get(i, j)?;
            corners.push(Point3::new(p[0], p[1], p[2]));
        }
    }
    Extent3d::hull(corners)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cone, Cylinder, Plane, Sphere, Torus};
    use scx_num::frame::{Frame, Sense};
    use scx_num::units::Length;
    use scx_num::vector3d::Vec3;
    use std::sync::Arc;

    fn mm(x: f64) -> Length {
        Length::from_mm(x)
    }

    /// Every sampled point (with a relative slack for the f64 evaluation's own rounding)
    /// lies in the box.
    fn holds(s: &Surface, uv: Extent2d) -> Extent3d {
        let b = s
            .box_over(uv)
            .unwrap_or_else(|| panic!("no box for {uv:?}"));
        let n = 40;
        for i in 0..=n {
            for j in 0..=n {
                let t = |a: f64, b: f64, k: usize| a + (b - a) * (k as f64 / n as f64);
                let p = s.point(Point2::new(
                    t(uv.min().x, uv.max().x, i),
                    t(uv.min().y, uv.max().y, j),
                ));
                let slack = 1e-12 * (1.0 + p.x.abs().max(p.y.abs()).max(p.z.abs()));
                let grown = b.expanded(slack).unwrap_or(b);
                assert!(
                    grown.contains_point(p, 0.0),
                    "{p:?} outside {b:?} over {uv:?}"
                );
            }
        }
        b
    }

    fn frame() -> Frame {
        Frame::new(
            Point3::new(1.0, 2.0, -3.0),
            Vec3::new(0.3, -0.2, 0.9),
            Vec3::new(1.0, 0.1, 0.0),
        )
        .unwrap_or_else(|| panic!("frame"))
    }

    fn uv(a: f64, b: f64, c: f64, d: f64) -> Extent2d {
        Extent2d::from_bounds(a, b, c, d).unwrap_or_else(|| panic!("box"))
    }

    #[test]
    fn analytic_boxes_hold_the_surface() {
        let f = frame();
        let kinds: Vec<(Surface, Extent2d)> = vec![
            (Plane::new(f).into(), uv(-2.0, -1.0, 3.0, 0.5)),
            (
                Cylinder::new(f, mm(1.5), Sense::Forward)
                    .unwrap_or_else(|e| panic!("{e}"))
                    .into(),
                uv(0.3, -1.0, 7.9, 2.0),
            ),
            (
                Cone::new(f, mm(1.0), 0.4, Sense::Reversed)
                    .unwrap_or_else(|e| panic!("{e}"))
                    .into(),
                uv(-0.1, 0.0, 0.8, 2.0),
            ),
            (
                Sphere::new(f, mm(2.0), Sense::Forward)
                    .unwrap_or_else(|e| panic!("{e}"))
                    .into(),
                uv(0.1, 0.05, 0.6, 0.95),
            ),
            (
                Torus::new(f, mm(3.0), mm(1.0), Sense::Forward)
                    .unwrap_or_else(|e| panic!("{e}"))
                    .into(),
                uv(-0.2, 0.3, 0.45, 1.2),
            ),
        ];
        for (s, b) in &kinds {
            holds(s, *b);
            // A point box holds as well.
            holds(s, uv(b.min().x, b.min().y, b.min().x, b.min().y));
        }
    }

    #[test]
    fn a_full_turn_of_a_cylinder_is_tight() {
        let c: Surface = Cylinder::new(frame(), mm(1.0), Sense::Forward)
            .unwrap_or_else(|e| panic!("{e}"))
            .into();
        let period = 2.0 * core::f64::consts::PI;
        let b = holds(&c, uv(0.0, 0.0, period, 1.0));
        // The exact box of a unit circle's turn, one unit high along the axis, is the
        // circle's extent per side plus the height's share; quarters of quarter arcs keep the
        // interval form within 25% of each side of the exact one.
        let f = frame();
        let z = f.z();
        let exact = |zc: f64| 2.0 * (1.0 - zc * zc).max(0.0).sqrt() + zc.abs();
        let size = b.size();
        for (got, zc) in [(size.x, z.x), (size.y, z.y), (size.z, z.z)] {
            assert!(got <= 1.25 * exact(zc), "{got} against {}", exact(zc));
        }
        // Many turns give the box of one (up to the breaks' own rounding).
        let many = holds(&c, uv(-50.0 * period, 0.0, 50.0 * period, 1.0));
        assert!(many.size().x <= 1.0001 * size.x, "{many:?} against {b:?}");
        assert!(c.box_over(uv(0.0, 0.0, 1e4 * period, 1.0)).is_none());
    }

    #[test]
    fn nurbs_boxes_hold_the_surface() {
        let t =
            Torus::new(frame(), mm(3.0), mm(1.0), Sense::Forward).unwrap_or_else(|e| panic!("{e}"));
        let n: Surface = Arc::new(
            t.to_nurbs(&uv(0.0, 0.0, 1.0, 1.0))
                .unwrap_or_else(|e| panic!("{e}")),
        )
        .into();
        let whole = holds(&n, uv(0.0, 0.0, 1.0, 1.0));
        holds(&n, uv(0.1, 0.2, 0.3, 0.9));
        holds(&n, uv(0.25, 0.25, 0.25, 0.25));
        // Past the domain the end pieces continue; no control hull bounds them.
        holds(&n, uv(-0.2, 0.9, 0.1, 1.3));
        // The torus's own box is exact (tight cos and sin on the arcs); the NURBS one's is the
        // quarter arcs' control hull, up to 12 percent wider on this torus.
        let a = holds(&Surface::from(t), uv(0.0, 0.0, 1.0, 1.0));
        assert!(
            a.size().x <= whole.size().x && whole.size().x <= 1.12 * a.size().x,
            "{whole:?} against {a:?}"
        );
    }

    /// Every sampled `dir . S` (with the evaluation's own rounding as slack) lies in the
    /// range; a flat NURBS piece seen along its normal is far narrower than its box.
    #[test]
    fn ranges_along_a_direction_hold_the_surface() {
        let t =
            Torus::new(frame(), mm(3.0), mm(1.0), Sense::Forward).unwrap_or_else(|e| panic!("{e}"));
        let nurbs: Surface = Arc::new(
            t.to_nurbs(&uv(0.0, 0.0, 1.0, 1.0))
                .unwrap_or_else(|e| panic!("{e}")),
        )
        .into();
        let dirs = [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.3, -0.5, 0.8),
            Vec3::new(-2.0, 0.25, 0.1),
        ];
        let kinds = [
            (Surface::from(t), uv(0.2, 0.1, 5.0, 4.0)),
            (nurbs, uv(0.1, 0.2, 0.9, 0.7)),
            (
                Surface::from(
                    Sphere::new(frame(), mm(2.0), Sense::Forward).unwrap_or_else(|e| panic!("{e}")),
                ),
                uv(0.3, -0.4, 2.0, 0.9),
            ),
        ];
        for (s, e) in &kinds {
            for d in dirs {
                let r = s.range_along(d, *e).unwrap_or_else(|| panic!("no range"));
                let n = 30;
                for i in 0..=n {
                    for j in 0..=n {
                        let f = |a: f64, b: f64, k: usize| a + (b - a) * (k as f64 / n as f64);
                        let p = s.point(Point2::new(
                            f(e.min().x, e.max().x, i),
                            f(e.min().y, e.max().y, j),
                        ));
                        let x = d.x * p.x + d.y * p.y + d.z * p.z;
                        assert!(r.lo - 1e-12 <= x && x <= r.hi + 1e-12, "{x} outside {r:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_flat_piece_along_its_normal_has_second_order_width() {
        // A bilinear patch z = 0.01 u v over [0, 1]^2 as a NURBS surface.
        use scx_nurbs::KnotVector;
        use scx_nurbs_surface::Net;
        let k = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap_or_else(|e| panic!("{e}"));
        let pts = Net::new(
            2,
            2,
            vec![
                [0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.01],
            ],
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let n: Surface =
            Arc::new(NurbsSurface::new(k.clone(), k, pts, None).unwrap_or_else(|e| panic!("{e}")))
                .into();
        let z = Vec3::new(0.0, 0.0, 1.0);
        let r = n
            .range_along(z, uv(0.0, 0.0, 1.0, 1.0))
            .unwrap_or_else(|| panic!("range"));
        assert!(r.lo <= 0.0 && r.hi >= 0.01 && r.hi - r.lo < 0.02, "{r:?}");
    }

    /// A sphere's boxes are the exact extrema of the kind where they are closed form: an
    /// octant is `[0, r]` on each axis, the whole sphere `[-r, r]`, to rounding, and the
    /// range along an axis of the frame is its own extent.
    #[test]
    fn a_spheres_extrema_are_exact() {
        let r = 1.5;
        let axes = Frame::new(
            Point3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap_or_else(|| panic!("frame"));
        let s: Surface = Sphere::new(axes, mm(r), Sense::Forward)
            .unwrap_or_else(|e| panic!("{e}"))
            .into();
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-13;
        // The whole sphere.
        let b = holds(&s, uv(0.0, 0.0, 1.0, 1.0));
        for (lo, hi) in [
            (b.min().x, b.max().x),
            (b.min().y, b.max().y),
            (b.min().z, b.max().z),
        ] {
            assert!(close(lo, -r) && close(hi, r), "{b:?}");
        }
        // The octant over the first quarter turn and the northern half.
        let b = holds(&s, uv(0.0, 0.5, 0.25, 1.0));
        assert!(close(b.min().x, 0.0) && close(b.max().x, r), "{b:?}");
        assert!(close(b.min().y, 0.0) && close(b.max().y, r), "{b:?}");
        assert!(close(b.min().z, 0.0) && close(b.max().z, r), "{b:?}");
        // The range along z over the whole sphere, and along x over the octant.
        let z = s
            .range_along(Vec3::new(0.0, 0.0, 1.0), uv(0.0, 0.0, 1.0, 1.0))
            .unwrap_or_else(|| panic!("range"));
        assert!(close(z.lo, -r) && close(z.hi, r), "{z:?}");
        let x = s
            .range_along(Vec3::new(1.0, 0.0, 0.0), uv(0.0, 0.5, 0.25, 1.0))
            .unwrap_or_else(|| panic!("range"));
        assert!(close(x.lo, 0.0) && close(x.hi, r), "{x:?}");
    }

    /// A cylinder's and a torus's boxes along their own frame's axes are tight too: a quarter
    /// turn of a cylinder of radius `r` and height `h` is `[0, r] x [0, r] x [0, h]`.
    #[test]
    fn a_cylinder_quarter_is_exact() {
        let axes = Frame::new(
            Point3::new(1.0, -2.0, 3.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap_or_else(|| panic!("frame"));
        let c: Surface = Cylinder::new(axes, mm(2.0), Sense::Forward)
            .unwrap_or_else(|e| panic!("{e}"))
            .into();
        let period = 2.0 * core::f64::consts::PI * 2.0;
        let b = holds(&c, uv(0.0, 0.0, period / 4.0, 5.0));
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-12;
        assert!(close(b.min().x, 1.0) && close(b.max().x, 3.0), "{b:?}");
        assert!(close(b.min().y, -2.0) && close(b.max().y, 0.0), "{b:?}");
        assert!(close(b.min().z, 3.0) && close(b.max().z, 8.0), "{b:?}");
    }
}
