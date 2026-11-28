// Written for solidcorex. Apache-2.0.
//! A branch's spans, proved (research 73 §4.8, row SM8): each span's residual on each
//! surface by an enclosure of the composition, and its distance to the intersection by a
//! parametric Krawczyk test; a span either test refuses is split at its middle, below the
//! resolution [`SsiError::Uncertified`] (the march's module docs, "The answer").

use scx_num::jet::SurfaceJet;
use scx_nurbs_surface::ParamAxis;

use super::{
    Approach, Arc, Certificate, Corr, Fix, Hermite, Ivl, Length, March, OnSurface, Point2, Point3,
    SsiError, Vec2, Vec3, Vert, coord, frame, gamma, set_coord, solve4, span_length, up,
};
use scx_nurbs_surface::{About, NurbsSurface};

/// A span's residual on each surface may be at most this many coincidences.
const RESIDUAL: f64 = 1.0;

/// A span's distance to the intersection may be at most this many coincidences.
const DISTANCE: f64 = 4.0;

/// From this many splits, a span keeps the looser bound it has: the Krawczyk test's, or
/// its distance to a node (module docs).
const NODE_DEPTH: u32 = 6;

/// No span is split more often than this (the resolution stops it first on any sane
/// curve).
const MAX_DEPTH: u32 = 60;

/// One surface's NURBS form, for the enclosures.
pub(super) fn nurbs(
    s: &scx_surface::Surface,
    e: &scx_num::extent2d::Extent2d,
) -> Option<Arc<NurbsSurface<3>>> {
    match s {
        scx_surface::Surface::Nurbs(n) => Some(Arc::clone(n)),
        _ => s.to_nurbs(e).ok().map(Arc::new),
    }
}

/// The partials of `n` to order `N` over the box `uv`: inside the domain, over it with the
/// knot lines crossed at least `continuity` times differentiable
/// ([`NurbsSurface::partials_over`]); past the domain, the one piece the box's part inside
/// lies in, continued (the piece a parameter outside the domain evaluates); else `None`.
fn jets<const N: usize>(
    n: &NurbsSurface<3>,
    uv: [Ivl; 2],
    continuity: usize,
) -> Option<SurfaceJet<[Ivl; 3], N>> {
    let d = n.domain();
    let [u, v] = uv;
    let inside = u.lo >= d.x_min() && u.hi <= d.x_max() && v.lo >= d.y_min() && v.hi <= d.y_max();
    if inside {
        return n.partials_over::<N>(uv, continuity, About::Middle).ok();
    }
    let piece = |axis: ParamAxis, t: Ivl, lo: f64, hi: f64| {
        let kv = n.knot_vector(axis);
        let a = kv.span(t.lo.clamp(lo, hi), Approach::Above).ok()?;
        let b = kv.span(t.hi.clamp(lo, hi), Approach::Below).ok()?;
        (a == b).then_some(a)
    };
    let su = piece(ParamAxis::U, u, d.x_min(), d.x_max())?;
    let sv = piece(ParamAxis::V, v, d.y_min(), d.y_max())?;
    n.partials_over_piece::<N>([su, sv], uv, About::Middle).ok()
}

fn iv(x: f64) -> Ivl {
    Ivl::from(x)
}

fn iv3(p: Vec3) -> Vec3<Ivl> {
    p.map(iv)
}

fn part<const N: usize>(j: &SurfaceJet<[Ivl; 3], N>, i: usize, k: usize) -> Option<Vec3<Ivl>> {
    j.get(i, k).map(Vec3::from)
}

/// The upper bound of `|v|`.
fn mag3(v: Vec3<Ivl>) -> f64 {
    v.norm().hi
}

/// The hull of the points.
fn hull2(ps: &[Vec2<Ivl>]) -> Option<Vec2<Ivl>> {
    let (f, rest) = ps.split_first()?;
    Some(
        rest.iter()
            .fold(*f, |h, p| Vec2::new(h.x.hull(p.x), h.y.hull(p.y))),
    )
}

fn hull3(ps: &[Vec3<Ivl>]) -> Option<Vec3<Ivl>> {
    let (f, rest) = ps.split_first()?;
    Some(rest.iter().fold(*f, |h, p| h.hull(*p)))
}

/// A pcurve's span: the Bézier points of the cubic Hermite from `a` to `b` over a parameter
/// of length `delta`.
fn bez2(a: Point2, va: Vec2, b: Point2, vb: Vec2, delta: f64) -> [Point2; 4] {
    let h = delta / 3.0;
    [a, a + va * h, b - vb * h, b]
}

fn eval2(b: &[Point2; 4], s: f64) -> Point2 {
    let r = 1.0 - s;
    let w = [r * r * r, 3.0 * r * r * s, 3.0 * r * s * s, s * s * s];
    let x = b.iter().zip(w).fold(0.0, |acc, (p, w)| acc + p.x * w);
    let y = b.iter().zip(w).fold(0.0, |acc, (p, w)| acc + p.y * w);
    Point2::new(x, y)
}

/// The box of a pcurve's span (its Bézier points' hull).
fn box2(b: &[Point2; 4]) -> [Ivl; 2] {
    let f = |g: fn(&Point2) -> f64| {
        b.iter()
            .map(g)
            .fold(Ivl::new(f64::INFINITY, f64::NEG_INFINITY), |h, x| {
                Ivl::new(h.lo.min(x), h.hi.max(x))
            })
    };
    [f(|p| p.x), f(|p| p.y)]
}

/// A span's proved bounds.
#[derive(Clone, Copy, Debug)]
struct Proved {
    on: [f64; 2],
    dist: f64,
}

/// What a span's test found.
enum Verdict {
    Proved(Proved),
    /// Cut at the knot line `at` of surface `k` across `axis`, which the span crosses.
    Knot {
        k: usize,
        axis: usize,
        at: f64,
    },
    /// Split at the middle; `conceded`, the looser bound to keep should the split fail.
    Split {
        conceded: Option<Proved>,
    },
}

/// A span of the branch being proved.
#[derive(Clone, Copy)]
struct Span {
    depth: u32,
    /// Knot lines are cut at (cleared when a cut failed: the span is split instead).
    knots: bool,
    proved: Option<Proved>,
}

impl March<'_, '_> {
    /// The branch's vertices `vs`, refined until every span is proved, and the certificate
    /// (in the march's operand order). `closed`: the last vertex is the first.
    pub(super) fn certify(
        &mut self,
        mut vs: Vec<Vert>,
        closed: bool,
    ) -> Result<(Vec<Vert>, Certificate), SsiError> {
        let nodes: Vec<(Point3, f64)> =
            vs.iter().filter_map(|v| v.node.map(|e| (v.p, e))).collect();
        let mut spans = vec![
            Span {
                depth: 0,
                knots: true,
                proved: None,
            };
            vs.len().saturating_sub(1)
        ];
        loop {
            for i in self.limit(&mut vs, closed) {
                for j in [i.wrapping_sub(1), i] {
                    if let Some(s) = spans.get_mut(j) {
                        s.proved = None;
                    }
                }
            }
            let mut out_v = Vec::with_capacity(vs.len());
            let mut out_s = Vec::with_capacity(spans.len());
            let mut cut = false;
            for (i, s) in spans.iter().enumerate() {
                let (Some(&a), Some(&b)) = (vs.get(i), vs.get(i + 1)) else {
                    continue;
                };
                out_v.push(a);
                if s.proved.is_some() {
                    out_s.push(*s);
                    continue;
                }
                self.budget.charge(1)?;
                self.stop.check()?;
                let at = Hermite::new(a.p, a.t, b.p, b.t).point(0.5);
                let deeper = Span {
                    depth: s.depth + 1,
                    proved: None,
                    ..*s
                };
                match self.span(&a, &b, s.depth, s.knots, &nodes) {
                    Verdict::Proved(p) => out_s.push(Span {
                        proved: Some(p),
                        ..*s
                    }),
                    Verdict::Knot { k, axis, at } => {
                        cut = true;
                        if let Some(m) = self.knot_vert(&a, &b, k, axis, at)? {
                            out_v.push(m);
                            out_s.extend([Span { proved: None, ..*s }; 2]);
                        } else {
                            out_s.push(Span { knots: false, ..*s });
                        }
                    }
                    Verdict::Split { conceded } => {
                        let short = (b.p - a.p).length() <= self.res || s.depth >= MAX_DEPTH;
                        let m = if short { None } else { self.mid_vert(&a, &b)? };
                        match (m, conceded) {
                            (Some(m), _) => {
                                cut = true;
                                out_v.push(m);
                                out_s.extend([deeper; 2]);
                            }
                            (None, Some(p)) => out_s.push(Span {
                                proved: Some(p),
                                ..*s
                            }),
                            (None, None) => return Err(SsiError::Uncertified { at }),
                        }
                    }
                }
            }
            if let Some(&l) = vs.last() {
                out_v.push(l);
            }
            vs = out_v;
            spans = out_s;
            if !cut {
                break;
            }
        }
        let reach = vs.iter().fold(0.0_f64, |r, v| r.max(v.p.max_dimension()));
        let round = up(gamma(64) * reach);
        let (on, dist) = spans.iter().filter_map(|s| s.proved).fold(
            ([0.0_f64; 2], 0.0_f64),
            |([a, b], d), p| {
                let [pa, pb] = p.on;
                ([a.max(pa), b.max(pb)], d.max(p.dist))
            },
        );
        let [oa, ob] = on;
        Ok((
            vs,
            Certificate {
                to_intersection: Length::from_mm(up(dist + round)),
                on_surface: [
                    OnSurface::Within(Length::from_mm(up(oa + round))),
                    OnSurface::Within(Length::from_mm(up(ob + round))),
                ],
            },
        ))
    }

    /// Fritsch-Carlson's limit on the pcurves' tangents (73 §24): on each surface and axis,
    /// where a span's Bézier points leave the extent, each end's velocity component is cut
    /// so the span is monotone between its ends (`alpha, beta >= 0`, `alpha^2 + beta^2 <=
    /// 9`, `alpha = m0 delta / dx`), the least cut of a vertex's spans kept. The vertices
    /// changed, by index.
    fn limit(&self, vs: &mut [Vert], closed: bool) -> Vec<usize> {
        let mut changed = Vec::new();
        for k in 0..2 {
            for axis in 0..2 {
                let mut f = vec![1.0_f64; vs.len()];
                for (i, w) in vs.windows(2).enumerate() {
                    let [a, b] = w else { continue };
                    let delta = span_length(a.p, a.t, b.p, b.t);
                    let x0 = coord(&a.uv, k, axis);
                    let x1 = coord(&b.uv, k, axis);
                    let (lo, hi) = self.cell(k, axis, x0, x1);
                    let comp = |v: &Vert| {
                        let [va, vb] = v.vel;
                        let w = if k == 0 { va } else { vb };
                        if axis == 0 { w.x } else { w.y }
                    };
                    let (m0, m1) = (comp(a) * delta, comp(b) * delta);
                    let (b1, b2) = (x0 + m0 / 3.0, x1 - m1 / 3.0);
                    if [b1, b2].iter().all(|&x| x >= lo && x <= hi) {
                        continue;
                    }
                    let dx = x1 - x0;
                    let (fa, fb) = if dx == 0.0 {
                        (0.0, 0.0)
                    } else {
                        let (al, be) = (m0 / dx, m1 / dx);
                        let fa = if al < 0.0 { 0.0 } else { 1.0 };
                        let fb = if be < 0.0 { 0.0 } else { 1.0 };
                        let s = (al * fa) * (al * fa) + (be * fb) * (be * fb);
                        if s > 9.0 {
                            let g = 3.0 / s.sqrt();
                            (fa * g, fb * g)
                        } else {
                            (fa, fb)
                        }
                    };
                    if let Some(x) = f.get_mut(i) {
                        *x = x.min(fa);
                    }
                    if let Some(x) = f.get_mut(i + 1) {
                        *x = x.min(fb);
                    }
                }
                if closed && let (Some(&a), Some(&b)) = (f.first(), f.last()) {
                    let m = a.min(b);
                    if let Some(x) = f.first_mut() {
                        *x = m;
                    }
                    if let Some(x) = f.last_mut() {
                        *x = m;
                    }
                }
                for (i, (v, g)) in vs.iter_mut().zip(f).enumerate() {
                    if g < 1.0 {
                        let [va, vb] = &mut v.vel;
                        let w = if k == 0 { va } else { vb };
                        if axis == 0 {
                            w.x *= g;
                        } else {
                            w.y *= g;
                        }
                        changed.push(i);
                    }
                }
            }
        }
        changed
    }

    /// The range a span from `x0` to `x1` on surface `k`'s `axis` may cover: the extent, cut
    /// at the nearest knot line on each side where the surface is less than three times
    /// differentiable (one between the ends is cut at by [`Self::straddle`] instead).
    fn cell(&self, k: usize, axis: usize, x0: f64, x1: f64) -> (f64, f64) {
        let sd = self.side(k);
        let (mut lo, mut hi) = (sd.lo(axis), sd.hi(axis));
        let (l, h) = (x0.min(x1), x0.max(x1));
        if let Some(n) = sd.n.as_deref() {
            let kv = n.knot_vector(if axis == 0 {
                ParamAxis::U
            } else {
                ParamAxis::V
            });
            for (at, mult) in kv.breakpoints() {
                if kv.degree().saturating_sub(mult) >= 3 {
                    continue;
                }
                if at <= l {
                    lo = lo.max(at);
                } else if at >= h {
                    hi = hi.min(at);
                }
            }
        }
        (lo, hi)
    }

    /// The test of the span from `a` to `b` (module docs).
    fn span(
        &self,
        a: &Vert,
        b: &Vert,
        depth: u32,
        knots: bool,
        nodes: &[(Point3, f64)],
    ) -> Verdict {
        if knots && let Some((k, axis, at)) = self.straddle(a, b) {
            return Verdict::Knot { k, axis, at };
        }
        let c = Hermite::new(a.p, a.t, b.p, b.t);
        let delta = span_length(a.p, a.t, b.p, b.t);
        let pc = [0, 1].map(|k| {
            let (Some(&ua), Some(&ub)) = (a.uv.get(k), b.uv.get(k)) else {
                return [Point2::new(0.0, 0.0); 4];
            };
            let (Some(&va), Some(&vb)) = (a.vel.get(k), b.vel.get(k)) else {
                return [ua; 4];
            };
            bez2(ua, va, ub, vb, delta)
        });
        let [pa, pb] = &pc;
        let on = match (
            self.residual(0, a, b, pa, &c, delta),
            self.residual(1, a, b, pb, &c, delta),
        ) {
            (Some(x), Some(y)) if x <= RESIDUAL * self.tol && y <= RESIDUAL * self.tol => [x, y],
            _ => return Verdict::Split { conceded: None },
        };
        // The bound by the nearest node, and whether the span lies no nearer it than its
        // own length (so no split shrinks that bound by half).
        let (direct, stuck) = nodes
            .iter()
            .fold((f64::INFINITY, false), |(d, st), &(p, e)| {
                let (near, far) = c.b.iter().fold((f64::INFINITY, 0.0_f64), |(n, f), q| {
                    let r = (*q - p).length();
                    (n.min(r), f.max(r))
                });
                let b = up(e + far);
                if b < d {
                    (b, far <= 2.0 * near)
                } else {
                    (d, st)
                }
            });
        let reach = c.b.iter().fold(0.0_f64, |r, q| r.max(q.max_dimension()));
        let near = (256.0 * self.tol).max(2.0 * gamma(64).sqrt() * reach);
        let cap = near.max((self.tol * reach).sqrt());
        let k = if a.node.is_some() || b.node.is_some() {
            None
        } else {
            self.krawczyk(&c, &pc, on, a.t + b.t)
        };
        let dist = k.unwrap_or(f64::INFINITY).min(direct);
        if dist <= DISTANCE * self.tol {
            return Verdict::Proved(Proved { on, dist });
        }
        // Proved, but looser: by the test where the residual's ends dominate it (the
        // vertices' own coincidence over the conditioning by a near node, which no split
        // shrinks), or by the distance to a node.
        let conceded = (k.is_some() || direct <= cap).then_some(Proved { on, dist });
        if depth >= NODE_DEPTH
            && (k.is_some() || direct <= near || stuck)
            && let Some(p) = conceded
        {
            return Verdict::Proved(p);
        }
        Verdict::Split { conceded }
    }

    /// A knot line of either surface strictly between the span's ends where the surface is
    /// less than three times differentiable: the one nearest the middle.
    fn straddle(&self, a: &Vert, b: &Vert) -> Option<(usize, usize, f64)> {
        let mut best: Option<(f64, (usize, usize, f64))> = None;
        for k in 0..2 {
            let Some(n) = self.side(k).n.as_deref() else {
                continue;
            };
            let d = n.domain();
            for (axis, pa) in [(0, ParamAxis::U), (1, ParamAxis::V)] {
                let kv = n.knot_vector(pa);
                let (lo, hi) = if axis == 0 {
                    (d.x_min(), d.x_max())
                } else {
                    (d.y_min(), d.y_max())
                };
                let (x0, x1) = (coord(&a.uv, k, axis), coord(&b.uv, k, axis));
                let (l, h) = (x0.min(x1), x0.max(x1));
                if h <= l {
                    continue;
                }
                let mid = 0.5 * (l + h);
                for (at, mult) in kv.breakpoints() {
                    if at <= l || at >= h || at <= lo || at >= hi {
                        continue;
                    }
                    if kv.degree().saturating_sub(mult) >= 3 {
                        continue;
                    }
                    let off = (at - mid).abs() / (h - l);
                    if best.is_none_or(|(o, _)| off < o) {
                        best = Some((off, (k, axis, at)));
                    }
                }
            }
        }
        best.map(|(_, x)| x)
    }

    /// The vertex where the span from `a` to `b` crosses the knot line `at` of surface `k`
    /// across `axis`; `None` when the corrector finds none between them.
    fn knot_vert(
        &mut self,
        a: &Vert,
        b: &Vert,
        k: usize,
        axis: usize,
        at: f64,
    ) -> Result<Option<Vert>, SsiError> {
        let (x0, x1) = (coord(&a.uv, k, axis), coord(&b.uv, k, axis));
        let f = ((at - x0) / (x1 - x0)).clamp(0.0, 1.0);
        let mut uv = Self::guess(a, b, f);
        set_coord(&mut uv, k, axis, at);
        self.solved(a, b, uv, Fix::Edge { k, axis, at })
    }

    /// The vertex the corrector finds on the plane across the span's middle; `None` when
    /// it finds none between `a` and `b`.
    fn mid_vert(&mut self, a: &Vert, b: &Vert) -> Result<Option<Vert>, SsiError> {
        let c = Hermite::new(a.p, a.t, b.p, b.t);
        let x0 = c.point(0.5);
        let Some(n) = c.derivs(0.5).0.normalized().map(Vec3::from) else {
            return Ok(None);
        };
        let uv = Self::guess(a, b, 0.5);
        self.solved(a, b, uv, Fix::Plane { x0, n })
    }

    /// Both pcurves' points at `s` of the span.
    fn guess(a: &Vert, b: &Vert, s: f64) -> [Point2; 2] {
        let delta = span_length(a.p, a.t, b.p, b.t);
        let [ua, ub] = a.uv;
        let [wa, wb] = b.uv;
        let [va, vb] = a.vel;
        let [xa, xb] = b.vel;
        [
            eval2(&bez2(ua, va, wa, xa, delta), s),
            eval2(&bez2(ub, vb, wb, xb, delta), s),
        ]
    }

    /// The corrector's vertex from `uv` with `fix`, oriented as the span, if strictly
    /// between `a` and `b` along its chord.
    fn solved(
        &mut self,
        a: &Vert,
        b: &Vert,
        uv: [Point2; 2],
        fix: Fix,
    ) -> Result<Option<Vert>, SsiError> {
        self.budget.charge(1)?;
        self.stop.check()?;
        let Corr::Done(st) = self.correct(uv, fix) else {
            return Ok(None);
        };
        let Some(fr) = frame(&st) else {
            return Ok(None);
        };
        let chord = b.p - a.p;
        let along = (st.p - a.p).dot(chord);
        if !(along > 0.0 && along < chord.dot(chord)) {
            return Ok(None);
        }
        let sign = if fr.t.dot(a.t + b.t) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        Ok(Some(Self::vert(&st, &fr, sign)))
    }

    /// The residual `|S_k(p(t)) - c(t)|` of the span on surface `k`, bounded (module
    /// docs): the Hermite interpolant of its ends' values and derivatives plus the
    /// interpolation error, of order four where the surface is three times differentiable
    /// over the span's box, else of order two.
    fn residual(
        &self,
        k: usize,
        a: &Vert,
        b: &Vert,
        pc: &[Point2; 4],
        c: &Hermite,
        delta: f64,
    ) -> Option<f64> {
        let n = self.side(k).n.as_deref()?;
        let end = |v: &Vert| -> Option<(f64, f64)> {
            let uv = *v.uv.get(k)?;
            let vel = *v.vel.get(k)?;
            let j = jets::<1>(n, [iv(uv.x), iv(uv.y)], 0)?;
            let s = part(&j, 0, 0)?;
            let e = s - iv3(v.p - Point3::new(0.0, 0.0, 0.0));
            let d = part(&j, 1, 0)? * iv(vel.x) + part(&j, 0, 1)? * iv(vel.y) - iv3(v.t);
            Some((mag3(e), mag3(d)))
        };
        let (e0, d0) = end(a)?;
        let (e1, d1) = end(b)?;
        let ends = up(e0.max(e1) + up(4.0 / 27.0 * delta * (d0 + d1)));
        let bx = box2(pc);
        let dl = iv(delta);
        let p = pc.map(|q| Vec2::new(iv(q.x), iv(q.y)));
        let [b0, b1, b2, b3] = p;
        let d1v = hull2(&[b1 - b0, b2 - b1, b3 - b2].map(|d| d * (iv(3.0) / dl)))?;
        let d2v =
            hull2(&[b2 - b1 - (b1 - b0), b3 - b2 - (b2 - b1)].map(|d| d * (iv(6.0) / (dl * dl))))?;
        let three = b3 - b0 - (b2 - b1) * iv(3.0);
        let d3v = three * (iv(6.0) / (dl * dl * dl));
        let (pa, pb) = (d1v.x, d1v.y);
        let (pcx, pcy) = (d2v.x, d2v.y);
        if let Some(j) = jets::<4>(n, bx, 3) {
            let s = |i, m| part(&j, i, m);
            let (g, h) = (d3v.x, d3v.y);
            let sq = |x: Ivl| x * x;
            let d4 = s(4, 0)? * (sq(pa) * sq(pa))
                + s(3, 1)? * (iv(4.0) * sq(pa) * pa * pb)
                + s(2, 2)? * (iv(6.0) * sq(pa) * sq(pb))
                + s(1, 3)? * (iv(4.0) * pa * sq(pb) * pb)
                + s(0, 4)? * (sq(pb) * sq(pb));
            let d3 = s(3, 0)? * (pcx * sq(pa))
                + s(2, 1)? * (iv(2.0) * pcx * pa * pb + pcy * sq(pa))
                + s(1, 2)? * (pcx * sq(pb) + iv(2.0) * pcy * pa * pb)
                + s(0, 3)? * (pcy * sq(pb));
            let d2a = s(2, 0)? * sq(pcx) + s(1, 1)? * (iv(2.0) * pcx * pcy) + s(0, 2)? * sq(pcy);
            let d2b = s(2, 0)? * (g * pa) + s(1, 1)? * (g * pb + h * pa) + s(0, 2)? * (h * pb);
            let e4 = d4 + d3 * iv(6.0) + d2a * iv(3.0) + d2b * iv(4.0);
            let m4 = mag3(e4);
            if !m4.is_finite() {
                return None;
            }
            let d2 = delta * delta;
            return Some(up(ends + up(d2 * d2 / 384.0 * m4)));
        }
        let j = jets::<2>(n, bx, 2)?;
        let s = |i, m| part(&j, i, m);
        let sq = |x: Ivl| x * x;
        let cb = c.b.map(|q| iv3(q - Point3::new(0.0, 0.0, 0.0)));
        let [c0, c1, c2, c3] = cb;
        let cdd =
            hull3(&[c2 - c1 - (c1 - c0), c3 - c2 - (c2 - c1)].map(|d| d * (iv(6.0) / (dl * dl))))?;
        let e2 = s(2, 0)? * sq(pa)
            + s(1, 1)? * (iv(2.0) * pa * pb)
            + s(0, 2)? * sq(pb)
            + s(1, 0)? * pcx
            + s(0, 1)? * pcy
            - cdd;
        let m2 = mag3(e2);
        let h2 = up((6.0 * (e0 + e1) + 4.0 * delta * (d0 + d1)) / (delta * delta));
        if !(m2.is_finite() && h2.is_finite()) {
            return None;
        }
        Some(up(ends + up(delta * delta / 8.0 * up(m2 + h2))))
    }

    /// The parametric Krawczyk test of the span (module docs): the distance from the span
    /// to a unique intersection point on the plane across it at each parameter, or `None`
    /// when the test fails.
    fn krawczyk(
        &self,
        c: &Hermite,
        pc: &[[Point2; 4]; 2],
        on: [f64; 2],
        along: Vec3,
    ) -> Option<f64> {
        let [b0, .., b3] = c.b;
        // Across the chord; across `along` (the ends' tangents) where the chord is too short
        // to have a direction.
        let nrm = Vec3::from((b3 - b0).normalized().or_else(|| along.normalized())?);
        let [na, nb] = [self.side(0).n.as_deref()?, self.side(1).n.as_deref()?];
        let [pa, pb] = pc;
        let (xa, xb) = (box2(pa), box2(pb));
        let fsum = up(on[0] + on[1]);
        let f = [fsum, fsum, fsum, up(0.5 * fsum)];
        // The mid Jacobian from the boxes' first partials.
        let ja = jets::<1>(na, xa, 0)?;
        let jb = jets::<1>(nb, xb, 0)?;
        let mid = |v: Vec3<Ivl>| v.map(Ivl::mid);
        let cols = [
            mid(part(&ja, 1, 0)?),
            mid(part(&ja, 0, 1)?),
            -mid(part(&jb, 1, 0)?),
            -mid(part(&jb, 0, 1)?),
        ];
        let row = |r: usize| {
            cols.map(|v| match r {
                0 => v.x,
                1 => v.y,
                _ => v.z,
            })
        };
        // The fourth row: n . (S_A + S_B) / 2, so the B columns with `+`.
        let j4 = cols.map(|v| 0.5 * nrm.dot(v));
        let j4 = {
            let [a0, a1, a2, a3] = j4;
            [a0, a1, -a2, -a3]
        };
        let jm = [row(0), row(1), row(2), j4];
        let m = inv4(&jm)?;
        let z: [f64; 4] = m.map(|r| up(r.iter().zip(f).map(|(x, y)| x.abs() * y).sum::<f64>()));
        let scale = |b: &[Ivl; 2]| b.iter().fold(0.0_f64, |s, x| s.max(x.mag())) + 1.0;
        let (sa, sb) = (scale(&xa), scale(&xb));
        let floor = [sa, sa, sb, sb].map(|s| 4.0 * f64::EPSILON * s);
        let r: [f64; 4] = core::array::from_fn(|i| {
            up(2.0 * z.get(i).copied().unwrap_or(0.0) + floor.get(i).copied().unwrap_or(0.0))
        });
        let widen = |b: [Ivl; 2], r0: f64, r1: f64| {
            let [u, v] = b;
            [
                Ivl::checked(u.lo - r0, u.hi + r0),
                Ivl::checked(v.lo - r1, v.hi + r1),
            ]
        };
        let [r0, r1, r2, r3] = r;
        let ja = jets::<1>(na, widen(xa, r0, r1), 0)?;
        let jb = jets::<1>(nb, widen(xb, r2, r3), 0)?;
        let (au, av) = (part(&ja, 1, 0)?, part(&ja, 0, 1)?);
        let (bu, bv) = (part(&jb, 1, 0)?, part(&jb, 0, 1)?);
        let neg = |v: Vec3<Ivl>| v.map(|x| Ivl::new(-x.hi, -x.lo));
        let jc = [au, av, neg(bu), neg(bv)];
        let n3 = iv3(nrm);
        let hj = |v: Vec3<Ivl>| iv(0.5) * n3.dot(v);
        let jrows: [[Ivl; 4]; 4] = [
            jc.map(|v| v.x),
            jc.map(|v| v.y),
            jc.map(|v| v.z),
            [hj(au), hj(av), hj(bu), hj(bv)],
        ];
        let mut kr = [0.0_f64; 4];
        for (i, (mi, ki)) in m.iter().zip(kr.iter_mut()).enumerate() {
            let mut s = z.get(i).copied().unwrap_or(f64::INFINITY);
            for (j, rj) in r.iter().enumerate() {
                let mut cij = iv(if i == j { 1.0 } else { 0.0 });
                for (l, ml) in mi.iter().enumerate() {
                    let jl = jrows.get(l).and_then(|row| row.get(j)).copied()?;
                    cij = cij - iv(*ml) * jl;
                }
                s = up(s + up(cij.mag() * rj));
            }
            // Not strictly inside, or NaN: no proof.
            if s.partial_cmp(&r.get(i).copied().unwrap_or(0.0)) != Some(std::cmp::Ordering::Less) {
                return None;
            }
            *ki = s;
        }
        let [k0, k1, k2, k3] = kr;
        let reach = |u: Vec3<Ivl>, v: Vec3<Ivl>, x: f64, y: f64| {
            let c = |a: Ivl, b: Ivl| up(a.mag() * x + b.mag() * y);
            let (cx, cy, cz) = (c(u.x, v.x), c(u.y, v.y), c(u.z, v.z));
            up((cx * cx + cy * cy + cz * cz).sqrt())
        };
        let da = up(on[0] + reach(au, av, k0, k1));
        let db = up(on[1] + reach(bu, bv, k2, k3));
        let d = da.min(db);
        d.is_finite().then_some(d)
    }
}

/// The inverse of the 4 x 4 matrix `m`, column by column ([`solve4`]).
fn inv4(m: &[[f64; 4]; 4]) -> Option<[[f64; 4]; 4]> {
    let mut out = [[0.0; 4]; 4];
    for c in 0..4 {
        let aug: [[f64; 5]; 4] = core::array::from_fn(|r| {
            let row = m.get(r).copied().unwrap_or([0.0; 4]);
            let [a, b, cc, d] = row;
            [a, b, cc, d, if r == c { 1.0 } else { 0.0 }]
        });
        let x = solve4(aug)?;
        for (r, xr) in x.iter().enumerate() {
            if let Some(o) = out.get_mut(r).and_then(|row| row.get_mut(c)) {
                *o = *xr;
            }
        }
    }
    Some(out)
}
