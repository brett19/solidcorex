// Written for solidcorex. Apache-2.0.
//! The NURBS arm of the gaps (research 77 FT4): [`super::curve_surface_gap`] and
//! [`super::pcurve_edge_gap`] on a [`Surface::Nurbs`](crate::Surface::Nurbs), through the
//! same branch and bound as the analytic kinds ([`super::search`]).
//!
//! - **The surface over a uv box.** The box is cut at the knot lines strictly inside it, in
//!   each direction; each part is enclosed on its own polynomial piece by
//!   [`NurbsSurface::partials_over_piece`] (NS4, about the piece's middle), and the parts'
//!   enclosures are hulled, so every partial holds at every point of the box from either
//!   side of a line it crosses. A part past the domain (a pcurve on the boundary, enclosed a
//!   few ulps outside it) takes the end piece continued, the surface's own extension there.
//!   A crossed line where the degree minus the knot's multiplicity is below one leaves the
//!   surface only C0 across it: the piece then gets no chord bound (below).
//! - **The chain rule.** For `(u(t), v(t))` with jets `(u', u'')`, `(v', v'')` over a piece,
//!   `d/dt S = S_u u' + S_v v'` and `d²/dt² S = S_uu u'² + 2 S_uv u' v' + S_vv v'² + S_u u'' +
//!   S_v v''`, each term from the enclosed partials. `|S(u(t), v(t)) - C(t)|` over the piece is
//!   then bounded as the analytic pcurves' ([`super::vector_bound`]: the chord bound on `D`
//!   and on `|D|²`, and the direct enclosure). Across a C0 line, where the second derivative
//!   does not exist, the bound is the Lipschitz one: `|D|` rises from either end at most at
//!   `sup |D'|`, so it is at most `min((|D(a)| + |D(b)| + h sup |D'|) / 2, min(|D(a)|,
//!   |D(b)|) + h sup |D'|)`; it shrinks linearly with the piece, not quadratically.
//! - **A pcurve** is `(u(t), v(t))` itself, sharing the edge's parameter: the gap is
//!   `sup_t |S(p(t)) - C(t)|` as on the analytic kinds.
//! - **A curve against the surface** is `sup_t min_uv |C(t) - S(uv)|`. A point's distance is
//!   [`nurbs_invert`]'s certified nearest: its foot is within `eps = GAP_REL (diagonal +
//!   first incumbent)` of the true nearest, and the first incumbent is a patch corner, inside
//!   the net's box, so `eps <= GAP_REL (1.5 diagonal + |q - centre|)` with `diagonal` and
//!   `centre` the net box's. The bracket at `t` is `|C(t) - S(foot)|` enclosed above; below,
//!   that less `2 eps`, twice the query's distance from the enclosed `C(t)`, and how far the
//!   search's own distance (in doubles, at its rounded point) can sit below the true one. A
//!   piece is bounded through a correspondence `uv(t)`, since `min_uv |C - S| <=
//!   |C(t) - S(uv(t))|` for any: the feet of its ends joined linearly (the chain rule over
//!   the box the feet span, `u'' = v'' = 0`), and either foot held (`S` a constant, the chord
//!   bound on `C` alone); the smallest. Held feet keep a piece bounded where the linear one
//!   is not: the feet on either side of a seam, or across a C0 line.
//!
//! The gap of a curve from a NURBS surface is from the whole surface over its domain (the
//! face's trim is not this crate's). One budget unit per point evaluated, as the analytic
//! kinds, and the nearest point search's own units at each point.

use scx_curve::{Curve, Curve2};
use scx_num::budget::{Budget, Stop};
use scx_num::interval::{Ivl, round_down, round_up};
use scx_num::jet::SurfaceJet;
use scx_num::vector2d::Point2;
use scx_nurbs::{Approach, KnotVector};
use scx_nurbs_surface::{About, NurbsSurface, ParamAxis};

use super::{Bracket, Field, Jet2, Short, ivl3, jets3, norm, sq, sub3, vector_bound};
use crate::error::SurfaceError;
use crate::nurbs_invert::{self, GAP_REL};

/// One direction of a uv box cut at the knot lines strictly inside it and inside the
/// domain: each part with the span whose polynomial holds the surface there (an end span,
/// continued, past the domain), and whether every line crossed keeps the surface C1. A
/// point on a knot line inside the domain takes both sides' spans.
fn parts(kv: &KnotVector, t: Ivl) -> Option<(Vec<(Ivl, usize)>, bool)> {
    if !(t.lo.is_finite() && t.hi.is_finite() && t.lo <= t.hi) {
        return None;
    }
    let (dlo, dhi) = (kv.domain().min(), kv.domain().max());
    let p = kv.degree();
    let mut ends = vec![t.lo];
    let mut c1 = true;
    for &k in kv.knots() {
        if k > t.lo && k < t.hi && k > dlo && k < dhi && ends.last() != Some(&k) {
            ends.push(k);
            c1 &= p > kv.multiplicity(k);
        }
    }
    ends.push(t.hi);
    if t.lo == t.hi {
        let below = kv.span(t.lo, Approach::Below).ok()?;
        let above = kv.span(t.lo, Approach::Above).ok()?;
        let mut out = vec![(t, below)];
        if above != below {
            out.push((t, above));
        }
        return Some((out, true));
    }
    let out = ends
        .iter()
        .zip(ends.iter().skip(1))
        .map(|(&a, &b)| {
            let span = kv.span(a + (b - a) / 2.0, Approach::Above).ok()?;
            Some((Ivl::new(a, b), span))
        })
        .collect::<Option<Vec<_>>>()?;
    Some((out, c1))
}

/// The partials of `n` to order `N` over the box `uv`, hulled over its pieces, and whether
/// the surface is C1 across every knot line the box crosses; `None` where a piece cannot
/// be enclosed (the weights not proved positive over it).
fn partials<const N: usize>(
    n: &NurbsSurface<3>,
    uv: [Ivl; 2],
) -> Option<(SurfaceJet<[Ivl; 3], N>, bool)> {
    let (us, cu) = parts(n.knot_vector(ParamAxis::U), uv[0])?;
    let (vs, cv) = parts(n.knot_vector(ParamAxis::V), uv[1])?;
    let mut out: Option<SurfaceJet<[Ivl; 3], N>> = None;
    for &(ui, su) in &us {
        for &(vi, sv) in &vs {
            let part = n
                .partials_over_piece::<N>([su, sv], [ui, vi], About::Middle)
                .ok()?;
            out = Some(match out {
                None => part,
                Some(mut h) => {
                    for ((i, j), v) in part.iter() {
                        if let Some(e) = h.get_mut(i, j) {
                            *e = [e[0].hull(v[0]), e[1].hull(v[1]), e[2].hull(v[2])];
                        }
                    }
                    h
                }
            });
        }
    }
    Some((out?, cu && cv))
}

/// `S` enclosed at the box `uv` (a point, or a point's rounding).
fn point_over(n: &NurbsSurface<3>, uv: [Ivl; 2]) -> Option<[Ivl; 3]> {
    partials::<0>(n, uv).map(|(j, _)| j.point())
}

/// `S(u(t), v(t))` and its first two derivatives in `t` over a piece, by the chain rule
/// from `S`'s partials over the box `(u, v)` lies in.
fn chain(s: &SurfaceJet<[Ivl; 3], 2>, u: Jet2, v: Jet2) -> Option<[Jet2; 3]> {
    let (s00, s10, s01) = (s.get(0, 0)?, s.get(1, 0)?, s.get(0, 1)?);
    let (s20, s11, s02) = (s.get(2, 0)?, s.get(1, 1)?, s.get(0, 2)?);
    let (uu, uv, vv) = (sq(u.d1), u.d1 * v.d1, sq(v.d1));
    let two = Ivl::from(2.0);
    let one = |e: [Ivl; 6]| {
        let [f, a, b, aa, ab, bb] = e;
        Jet2::new(
            f,
            a * u.d1 + b * v.d1,
            aa * uu + two * ab * uv + bb * vv + a * u.d2 + b * v.d2,
        )
    };
    let ([f0, f1, f2], [a0, a1, a2], [b0, b1, b2]) = (s00, s10, s01);
    let ([c0, c1, c2], [d0, d1, d2], [e0, e1, e2]) = (s20, s11, s02);
    Some([
        one([f0, a0, b0, c0, d0, e0]),
        one([f1, a1, b1, c1, d1, e1]),
        one([f2, a2, b2, c2, d2, e2]),
    ])
}

/// `|D|` over a piece of length `h` with `|D|` at its ends and `D'` enclosed over it, for a
/// `D` only continuous with a bounded derivative: the Lipschitz bound (module docs).
fn lipschitz(ends: (Ivl, Ivl), d1: [Ivl; 3], h: f64) -> f64 {
    let (a, b) = (ends.0.mag(), ends.1.mag());
    let m = round_up(norm(d1).mag() * h);
    let mean = round_up(round_up(round_up(a + b) + m) / 2.0);
    let near = round_up(a.min(b) + m);
    let x = mean.min(near);
    if x.is_nan() { f64::INFINITY } else { x }
}

/// `|D|` over a piece from its jets: the chord bounds where the surface is C1 over the
/// piece's box, else the Lipschitz bound and the direct enclosure.
fn piece_bound(d: [Jet2; 3], ends: (Ivl, Ivl), h: f64, c1: bool) -> f64 {
    let b = if c1 {
        vector_bound(d, ends, h)
    } else {
        let direct = norm([d[0].f, d[1].f, d[2].f]).mag();
        lipschitz(ends, [d[0].d1, d[1].d1, d[2].d1], h).min(direct)
    };
    if b.is_nan() { f64::INFINITY } else { b }
}

fn minus(a: [Jet2; 3], b: [Jet2; 3]) -> [Jet2; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// A bracket from an enclosure of a distance, its upper end infinite where not finite.
fn bracket(lower: f64, upper: f64) -> Bracket {
    Bracket {
        lower: if lower.is_nan() { 0.0 } else { lower },
        upper: if upper.is_nan() { f64::INFINITY } else { upper },
    }
}

// ---------------------------------------------------------------------------------------
// A pcurve on a NURBS surface.

/// `|S(p(t)) - C(t)|` on a NURBS surface.
pub(super) struct PcurveOnNurbs<'a> {
    pub(super) n: &'a NurbsSurface<3>,
    pub(super) pcurve: &'a Curve2,
    pub(super) curve: &'a Curve,
}

impl Field for PcurveOnNurbs<'_> {
    /// The gap at the point.
    type Mark = Ivl;

    fn at(&self, t: f64, budget: &mut Budget, _: &Stop) -> Result<(Bracket, Ivl), Short> {
        budget.charge(1).map_err(Short::Exhausted)?;
        let ti = Ivl::from(t);
        let p = self.pcurve.point_over(ti);
        let g = point_over(self.n, [p.x, p.y]).map_or(Ivl::whole(), |s| {
            norm(sub3(s, ivl3(self.curve.point_over(ti))))
        });
        Ok((bracket(g.lo.max(0.0), g.hi), g))
    }

    fn over(&self, (a, ga): (f64, Ivl), (b, gb): (f64, Ivl)) -> f64 {
        let i = Ivl::new(a, b);
        let p = self.pcurve.jet_over::<Ivl, 2>(i, Approach::Above);
        let Some((s, c1)) = partials::<2>(self.n, [p.point.x, p.point.y]) else {
            return f64::INFINITY;
        };
        let u = Jet2::new(p.point.x, p.d[0].x, p.d[1].x);
        let v = Jet2::new(p.point.y, p.d[0].y, p.d[1].y);
        let Some(sj) = chain(&s, u, v) else {
            return f64::INFINITY;
        };
        let c = self.curve.jet_over::<Ivl, 2>(i, Approach::Above);
        piece_bound(
            minus(sj, jets3(c.point, c.d[0], c.d[1])),
            (ga, gb),
            b - a,
            c1,
        )
    }
}

// ---------------------------------------------------------------------------------------
// A curve against a NURBS surface.

/// A point's nearest on the surface: the foot's `uv`, `S` there and `C(t)` enclosed, and
/// `|C(t) - S(foot)|` enclosed.
#[derive(Clone, Copy)]
pub(super) struct Foot {
    uv: Point2,
    s: [Ivl; 3],
    c: [Ivl; 3],
    g: Ivl,
}

/// `sup_t min_uv |C(t) - S(uv)|` on a NURBS surface.
pub(super) struct CurveToNurbs<'a> {
    curve: &'a Curve,
    n: &'a NurbsSurface<3>,
    /// The net box's diagonal and centre, for the nearest point search's gap.
    diagonal: f64,
    centre: [f64; 3],
}

impl<'a> CurveToNurbs<'a> {
    pub(super) fn new(curve: &'a Curve, n: &'a NurbsSurface<3>) -> Self {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in n.points().points() {
            for ((l, h), &x) in lo.iter_mut().zip(hi.iter_mut()).zip(p) {
                *l = l.min(x);
                *h = h.max(x);
            }
        }
        let ([l0, l1, l2], [h0, h1, h2]) = (lo, hi);
        let side = |l: f64, h: f64| Ivl::from(h) - Ivl::from(l);
        let d = norm([side(l0, h0), side(l1, h1), side(l2, h2)]);
        let mid = |l: f64, h: f64| l + (h - l) / 2.0;
        Self {
            curve,
            n,
            diagonal: d.hi,
            centre: [mid(l0, h0), mid(l1, h1), mid(l2, h2)],
        }
    }
}

/// The bound of a piece with `S` held at `s`, `|C - s|` at the ends `ends`.
fn held(cj: [Jet2; 3], s: [Ivl; 3], ends: (Ivl, Ivl), h: f64) -> f64 {
    let d = minus(cj, s.map(Jet2::lift_ivl));
    let b = vector_bound(d, ends, h);
    if b.is_nan() { f64::INFINITY } else { b }
}

/// A [`Short`] for the nearest point search's error.
fn short(e: SurfaceError) -> Short {
    match e {
        SurfaceError::Exhausted(x) => Short::Exhausted(x),
        SurfaceError::Stopped => Short::Stopped,
        SurfaceError::Defect { reason } => Short::Defect(reason),
        _ => Short::Defect("the nearest point search failed"),
    }
}

impl Field for CurveToNurbs<'_> {
    type Mark = Foot;

    fn at(&self, t: f64, budget: &mut Budget, stop: &Stop) -> Result<(Bracket, Foot), Short> {
        budget.charge(1).map_err(Short::Exhausted)?;
        let c = ivl3(self.curve.point_over(Ivl::from(t)));
        let q = self.curve.point(t);
        let qi = [Ivl::from(q.x), Ivl::from(q.y), Ivl::from(q.z)];
        let inv = nurbs_invert::invert(self.n, q, None, None, budget, stop).map_err(short)?;
        let foot = inv.foot();
        let uv = foot.at;
        let s = point_over(self.n, [Ivl::from(uv.x), Ivl::from(uv.y)]).unwrap_or([Ivl::whole(); 3]);
        let g = norm(sub3(c, s));
        // The query's distance from the true `C(t)`; the search's gap for this query; how
        // far the search's own distance (in doubles, at its rounded point) may sit below
        // the true one at its foot.
        let cq = norm(sub3(c, qi)).mag();
        let off = norm(sub3(qi, self.centre.map(Ivl::from))).mag();
        let eps = round_up(GAP_REL * round_up(round_up(1.5 * self.diagonal) + off));
        let fp = foot.point;
        let rounded = norm(sub3(s, [Ivl::from(fp.x), Ivl::from(fp.y), Ivl::from(fp.z)])).mag();
        let rounded = round_up(rounded + round_up(8.0 * f64::EPSILON * foot.distance));
        let pad = round_up(round_up(2.0 * round_up(cq + eps)) + rounded);
        let lower = round_down(g.lo - pad);
        Ok((bracket(lower, g.hi), Foot { uv, s, c, g }))
    }

    fn over(&self, (ta, fa): (f64, Foot), (tb, fb): (f64, Foot)) -> f64 {
        let h = tb - ta;
        let cj = self
            .curve
            .jet_over::<Ivl, 2>(Ivl::new(ta, tb), Approach::Above);
        let cj = jets3(cj.point, cj.d[0], cj.d[1]);
        let mut best = held(cj, fa.s, (fa.g, norm(sub3(fb.c, fa.s))), h).min(held(
            cj,
            fb.s,
            (norm(sub3(fa.c, fb.s)), fb.g),
            h,
        ));
        // The feet joined linearly: `uv(t) = uv_a + k (t - ta)`.
        let span = Ivl::from(tb) - Ivl::from(ta);
        let k = |a: f64, b: f64| (Ivl::from(b) - Ivl::from(a)) / span;
        let range = |a: f64, b: f64| Ivl::new(a.min(b), a.max(b));
        let (bu, bv) = (range(fa.uv.x, fb.uv.x), range(fa.uv.y, fb.uv.y));
        if let Some((s, c1)) = partials::<2>(self.n, [bu, bv]) {
            let zero = Ivl::from(0.0);
            let u = Jet2::new(bu, k(fa.uv.x, fb.uv.x), zero);
            let v = Jet2::new(bv, k(fa.uv.y, fb.uv.y), zero);
            if let Some(sj) = chain(&s, u, v) {
                best = best.min(piece_bound(minus(cj, sj), (fa.g, fb.g), h, c1));
            }
        }
        best
    }
}
