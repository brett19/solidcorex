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

use scx_curve::arcs::Arcs;
use scx_num::extent2d::Extent2d;
use scx_num::extent3d::Extent3d;
use scx_num::interval::Ivl;
use scx_num::vector2d::Point2;
use scx_num::vector3d::Point3;
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
    let us = parts(u, cut_u)?;
    let vs = parts(v, cut_v)?;
    let mut out: Option<Extent3d> = None;
    for &(u0, u1) in &us {
        for &(v0, v1) in &vs {
            let b = ivl_box(eval(
                Ivl::new(u0, u1),
                Ivl::new(v0, v1),
                Point2::new(u0, v0),
            ))?;
            out = Some(out.map_or(b, |o| o.union(b)));
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
        // The torus's own box is within a few percent of the NURBS one's.
        let a = holds(&Surface::from(t), uv(0.0, 0.0, 1.0, 1.0));
        assert!(
            whole.size().x <= 1.05 * a.size().x,
            "{whole:?} against {a:?}"
        );
    }
}
