//! The certified bound on the trim-to-edge gap `max_t |Q(c(t)) - C(t)|`, where `Q = S + d N`
//! is the offset at distance `d` of the base `S` (`d = 0`: the base itself), `c` the UV trim
//! and `C` the edge, both on the same parameter `t`.
//!
//! Write `g(t) = Q(c(t)) - C(t)`. The pieces are cut at both curves' knots, so on each piece
//! the trim and the edge are single cubics. Within one patch of the surface `g` is smooth.
//! Patches meet C^1 (a [`crate::surface::HermiteNet`]'s lines), and a piece whose trim
//! enclosure meets a line is treated as crossing it: there `g` with `d = 0` is only C^1 (its
//! `g''` jumps), and `g` with `d != 0` only Lipschitz (the normal is C^0 across the line, so
//! `g'` jumps). Four enclosures of `max |g|` over a piece `[a, b]`, `h = b - a`, are compared:
//!
//! - **second order**: `max(|g(a)|, |g(b)|) + h^2/8 sup|g''|` (the chord of `g` is a convex
//!   combination of the end values, and each coordinate leaves it by at most
//!   `(t - a)(b - t)/2 sup|g_i''|`); valid for `g` C^1 with bounded `g''`, so across a line
//!   only with `d = 0`; with `d != 0` a piece across a line takes the first-order form;
//! - **centred second order**: the same, with `sup|g''|` taken as `|g''(m)| + h/2 sup|g'''|`
//!   at the middle `m` where that is smaller (third-order jets over the piece); valid only
//!   where `g''` is continuous, so only on a piece within one patch. Across a line it takes
//!   the plain `sup|g''|` (`d = 0`) or the first-order form (`d != 0`);
//! - **first order (Lipschitz)**: `|g(m)| + h/2 sup|g'|` at the middle `m`;
//! - **direct**: the interval extension of `|g|` over the piece.
//!
//! Every enclosure is evaluated with outward-rounded intervals through jets (second order,
//! third for the centred form), so `sup|g''|` comes from the chain rule through
//! `N = S_u x S_v / |S_u x S_v|` with no separately derived bound. A piece where
//! `|S_u x S_v|` is not proved positive encloses the normal as the whole line and is
//! bisected. Where the trim's enclosure leaves the domain, a coordinate proved monotone on
//! the piece is narrowed to the range of its end values (so a trim that starts or ends on the
//! boundary resolves); otherwise the piece's bound is infinite and it is bisected.
//!
//! **Caps.** When the depth or piece cap is hit first, [`Problem::certify`] returns the
//! largest piece bound it has, with `resolved = false`. That value is still a proved upper
//! bound (every piece's bound is, and the pieces cover the range), but it did not meet the
//! stop: under `Stop::Within(tol)` it is above `tol`, so the answer to "is the gap at most
//! `tol`" is no (refuse); under `Stop::Tight` it may be far above the true gap. A caller may
//! record it as a proved, loose bound, flagged as such, but must not report it as the tight
//! value. It is infinite only where some piece could not be enclosed at all (a pole, or a
//! trim leaving the domain).

use core::cmp::Ordering;
use std::collections::BinaryHeap;
use std::time::Instant;

use scx_num::interval::Ivl;
use scx_num::ring::TayScalar;

use crate::counted::{ops, Ci};
use crate::curve::Hermite;
use crate::jet::{add3, cross3, dot3, mul3, set_third, sub3, Jet, V3};
use crate::surface::{Frame, Surface};

/// How a piece is enclosed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// End values plus `h^2/8 sup|g''|`.
    Second,
    /// As [`Method::Second`], with `sup|g''|` in centred form: `|g''(m)| + h/2 sup|g'''|`
    /// at the middle `m` (the smaller of that and the plain enclosure, coordinate by
    /// coordinate).
    Centred,
    /// Middle value plus `h/2 sup|g'|`.
    Lipschitz,
    /// The interval extension of `|g|`.
    Direct,
}

/// When the bisection stops.
#[derive(Clone, Copy, Debug)]
pub enum Stop {
    /// When the bound is within a thousandth of the largest proved value (as solidcore's
    /// tight bound).
    Tight,
    /// As soon as the bound is at most the tolerance (as solidcore's `_within` bound).
    Within(f64),
}

/// A bound and what it cost.
#[derive(Clone, Copy, Debug)]
pub struct Outcome {
    /// The proved upper bound. Infinite where some piece could not be enclosed at all; finite
    /// but looser than the stop asked when a cap was hit (`resolved = false`).
    pub bound: f64,
    /// The largest value proved to be attained (a lower bound on the maximum).
    pub lower: f64,
    /// Pieces at the end.
    pub pieces: usize,
    /// Interval operations.
    pub ops: u64,
    /// Wall time in seconds.
    pub secs: f64,
    /// The stop condition was met. `false`: a cap was hit first, and `bound` is the proved
    /// bound reached, not one that meets the stop (see the module documentation).
    pub resolved: bool,
}

/// The pieces and depth caps (solidcore's `MAX_PIECES`, `MAX_DEPTH`).
pub const MAX_PIECES: usize = 1 << 15;
/// See [`MAX_PIECES`].
pub const MAX_DEPTH: u32 = 30;
/// solidcore's `CLOSE_RELATIVE`.
pub const CLOSE_RELATIVE: f64 = 1.0e-3;
/// solidcore's `CLOSE_ABSOLUTE`.
pub const CLOSE_ABSOLUTE: f64 = 1.0e-13;

/// A trim, an edge and the surface (or its offset) they should agree on.
#[derive(Clone, Debug)]
pub struct Problem<S> {
    /// The base surface.
    pub surf: S,
    /// The offset distance (0: the base itself).
    pub d: f64,
    /// The UV trim.
    pub trim: Hermite<2>,
    /// The edge.
    pub edge: Hermite<3>,
}

/// `S + d N` from a frame.
pub fn offset_point<T: TayScalar>(f: &Frame<T>, d: f64) -> V3<T> {
    if d == 0.0 {
        return f[0];
    }
    let n = cross3(f[1], f[2]);
    let len = dot3(n, n).sqrt();
    let k = Jet::cst(d) / len;
    add3(f[0], mul3(k, n))
}

fn hull_jet(a: Jet<Ci>, b: Jet<Ci>) -> Jet<Ci> {
    Jet { v: a.v.hull(b.v), d: a.d.hull(b.d), dd: a.dd.hull(b.dd), ddd: a.ddd.hull(b.ddd) }
}

fn clip(j: Jet<Ci>, lo: f64, hi: f64) -> Option<Jet<Ci>> {
    let (l, h) = (j.v.0.lo.max(lo), j.v.0.hi.min(hi));
    (l <= h).then_some(Jet { v: Ci::span(l, h), ..j })
}

/// The frame of `surf` over every parameter the jets' values hold: each patch the box meets
/// is evaluated on its part of the box, and the results are hulled. Also says whether more
/// than one patch was evaluated (the box meets a patch line, across which the surface is only
/// C^1). `None` where the box leaves the surface's domain (the trim is not on the surface
/// there).
pub fn frame_ci<S: Surface>(surf: &S, u: Jet<Ci>, v: Jet<Ci>) -> Option<(Frame<Ci>, bool)> {
    let [u0, u1, v0, v1] = surf.domain();
    let (ui, vi) = (u.v.0, v.v.0);
    if !(ui.is_finite() && vi.is_finite() && ui.lo >= u0 && ui.hi <= u1 && vi.lo >= v0 && vi.hi <= v1) {
        return None;
    }
    let mut out: Option<Frame<Ci>> = None;
    let mut count = 0;
    for (p, b) in surf.patches([ui.lo, ui.hi], [vi.lo, vi.hi]) {
        let (Some(pu), Some(pv)) = (clip(u, b[0], b[1]), clip(v, b[2], b[3])) else {
            continue;
        };
        let f = surf.eval(p, pu, pv);
        count += 1;
        out = Some(match out {
            None => f,
            Some(o) => {
                let mut h = o;
                for r in 0..3 {
                    for k in 0..3 {
                        h[r][k] = hull_jet(o[r][k], f[r][k]);
                    }
                }
                h
            }
        });
    }
    out.map(|f| (f, count > 1))
}

/// The frame of `surf` at one point.
pub fn frame_f64<S: Surface>(surf: &S, u: Jet<f64>, v: Jet<f64>) -> Frame<f64> {
    let p = surf.patches([u.v, u.v], [v.v, v.v]);
    let patch = p.first().map_or(0, |x| x.0);
    surf.eval(patch, u, v)
}

fn norm_v(g: &V3<Ci>) -> Ivl {
    let s = g[0].v * g[0].v + g[1].v * g[1].v + g[2].v * g[2].v;
    Ivl::new(s.0.lo.max(0.0), s.0.hi).sqrt()
}

fn norm_mags(x: [Ci; 3]) -> Ivl {
    let m = x.map(|c| Ci(Ivl::from(c.0.mag())));
    let s = m[0] * m[0] + m[1] * m[1] + m[2] * m[2];
    s.0.sqrt()
}

#[derive(Clone, Copy, Debug)]
struct Piece {
    a: f64,
    b: f64,
    /// `|g|` at the ends (second order only).
    na: Ivl,
    nb: Ivl,
    /// `|g|` at the middle, where the enclosure computed it.
    mid: Option<Ivl>,
    upper: f64,
    depth: u32,
}

impl PartialEq for Piece {
    fn eq(&self, o: &Self) -> bool {
        self.upper.total_cmp(&o.upper) == Ordering::Equal
    }
}
impl Eq for Piece {}
impl PartialOrd for Piece {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Piece {
    fn cmp(&self, o: &Self) -> Ordering {
        self.upper.total_cmp(&o.upper)
    }
}

impl<S: Surface> Problem<S> {
    /// Trim and edge fitted to the UV curve `gamma` (point and slope) over `[t0, t1]`:
    /// the trim to `gamma` at `n_trim` spans, the edge to `Q(gamma(t))` at `n_edge`.
    pub fn fitted(
        surf: S,
        d: f64,
        gamma: impl Fn(f64) -> ([f64; 2], [f64; 2]) + Copy,
        [t0, t1]: [f64; 2],
        n_trim: usize,
        n_edge: usize,
    ) -> Self {
        let trim = Hermite::fit(gamma, t0, t1, n_trim);
        let on = |t: f64| {
            let (p, m) = gamma(t);
            let f = frame_f64(&surf, Jet { v: p[0], d: m[0], dd: 0.0, ddd: 0.0 }, Jet { v: p[1], d: m[1], dd: 0.0, ddd: 0.0 });
            let q = offset_point(&f, d);
            ([q[0].v, q[1].v, q[2].v], [q[0].d, q[1].d, q[2].d])
        };
        let edge = Hermite::fit(on, t0, t1, n_edge);
        Self { surf, d, trim, edge }
    }

    /// The parameter range.
    pub fn range(&self) -> [f64; 2] {
        [self.trim.t[0], self.trim.t[self.trim.t.len() - 1]]
    }

    /// Both curves' knots, merged.
    pub fn cuts(&self) -> Vec<f64> {
        let mut c: Vec<f64> = self.trim.t.iter().chain(&self.edge.t).copied().collect();
        c.sort_by(f64::total_cmp);
        c.dedup();
        c
    }

    /// `g` over `t` (spans `kc`, `ke`), enclosed, and whether the trim's enclosure there meets
    /// a patch line of the surface.
    fn g_ci(&self, t: Jet<Ci>, kc: usize, ke: usize) -> Option<(V3<Ci>, bool)> {
        let mut uv = self.trim.eval(kc, t);
        self.confine(&mut uv, t.v.0, kc);
        let (f, across) = frame_ci(&self.surf, uv[0], uv[1])?;
        let q = offset_point(&f, self.d);
        let e = self.edge.eval(ke, t);
        Some((sub3(q, e), across))
    }

    /// Narrows the enclosure of the trim's value over `t` (span `kc`) where it leaves the
    /// domain, by proof: where a coordinate's derivative is proved of one sign on the piece,
    /// the coordinate is monotone there, so its values lie between its values at the piece's
    /// ends. At a knot that end value is the trim's own data, exactly; elsewhere it is a point
    /// evaluation. The enclosure is intersected with that range. This lets a trim that
    /// starts or ends on the domain's boundary resolve; the interval evaluation alone
    /// overshoots the boundary by its overestimation at every depth. A coordinate that is
    /// not proved monotone is left as it is (and `frame_ci` refuses the piece if it leaves).
    fn confine(&self, uv: &mut [Jet<Ci>; 2], t: Ivl, kc: usize) {
        let dom = self.surf.domain();
        for (i, x) in uv.iter_mut().enumerate() {
            let (lo, hi) = (dom[2 * i], dom[2 * i + 1]);
            let e = x.v.0;
            if (e.lo >= lo && e.hi <= hi) || !(x.d.0.lo > 0.0 || x.d.0.hi < 0.0) {
                continue;
            }
            let at = |s: f64| -> Ivl {
                if s == self.trim.t[kc] {
                    Ivl::from(self.trim.p[kc][i])
                } else if s == self.trim.t[kc + 1] {
                    Ivl::from(self.trim.p[kc + 1][i])
                } else {
                    self.trim.eval(kc, Jet::of(Ci::point(s)))[i].v.0
                }
            };
            let r = at(t.lo).hull(at(t.hi));
            let (l, h) = (e.lo.max(r.lo), e.hi.min(r.hi));
            if l <= h {
                x.v = Ci(Ivl::new(l, h));
            }
        }
    }

    /// `g(t)` in floating point, for sampling.
    pub fn g_f64(&self, t: f64) -> [f64; 3] {
        let uv = self.trim.eval(self.trim.span_at(t), Jet::var(t));
        let f = frame_f64(&self.surf, uv[0], uv[1]);
        let q = offset_point(&f, self.d);
        let e = self.edge.eval(self.edge.span_at(t), Jet::var(t));
        [q[0].v - e[0].v, q[1].v - e[1].v, q[2].v - e[2].v]
    }

    /// `|g(t)|`.
    pub fn gap_f64(&self, t: f64) -> f64 {
        let g = self.g_f64(t);
        (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]).sqrt()
    }

    /// `|g(t)|` enclosed (the whole line where not enclosed).
    fn norm_at(&self, t: f64) -> Ivl {
        let (kc, ke) = (self.trim.span(t, t), self.edge.span(t, t));
        self.g_ci(Jet::var(Ci::point(t)), kc, ke).map_or(Ivl::whole(), |(g, _)| norm_v(&g))
    }

    /// Upper bound of `|g|` on `[a, b]` by `method`, a value proved attained, and `|g|` at
    /// the middle where it was computed.
    ///
    /// On a piece whose trim enclosure meets a patch line (the surface only C^1 there):
    /// - with `d = 0`, `g` is C^1 with a bounded, jumping `g''`, so the plain second-order
    ///   form holds but the centred one does not (`|g''(m)|` is one side's value); `Centred`
    ///   takes the plain `sup|g''|` there (review-55, P1);
    /// - with `d != 0`, the normal is only C^0 across the line, so `g'` jumps and no
    ///   second-order form holds; `Second` and `Centred` take the first-order form
    ///   `|g(m)| + h/2 sup|g'|` there, which needs only `g` Lipschitz (review-55, P2).
    fn enclose(&self, a: f64, b: f64, na: Ivl, nb: Ivl, method: Method) -> (f64, f64, Option<Ivl>) {
        let (kc, ke) = (self.trim.span(a, b), self.edge.span(a, b));
        let t = Jet::var(Ci::span(a, b));
        let h = Ci::point(b) - Ci::point(a);
        if method == Method::Centred {
            let mid = 0.5 * (a + b);
            let gm = self.g_ci(Jet::var(Ci::point(mid)), kc, ke);
            set_third(true);
            let g = self.g_ci(t, kc, ke);
            set_third(false);
            let (Some((gm, _)), Some((g, across))) = (gm, g) else {
                return (f64::INFINITY, 0.0, None);
            };
            let nm = norm_v(&gm);
            if across && self.d != 0.0 {
                return first_order(&g, nm, na, nb, h);
            }
            let half = h / Ci::point(2.0);
            let m: [Ci; 3] = core::array::from_fn(|i| {
                let plain = g[i].dd.0.mag();
                if across {
                    return Ci(Ivl::from(plain));
                }
                let centred = Ci(Ivl::from(gm[i].dd.0.mag())) + half * Ci(Ivl::from(g[i].ddd.0.mag()));
                Ci(Ivl::from(centred.0.hi.min(plain)))
            });
            let ends = Ci(Ivl::from(na.hi.max(nb.hi)));
            let up = ends + h * h * Ci(norm_mags(m)) / Ci::point(8.0);
            return (finite_or_inf(up.0.hi), na.lo.max(nb.lo).max(nm.lo), Some(nm));
        }
        let Some((g, across)) = self.g_ci(t, kc, ke) else {
            return (f64::INFINITY, 0.0, None);
        };
        match method {
            Method::Second => {
                if across && self.d != 0.0 {
                    return first_order(&g, self.norm_at(0.5 * (a + b)), na, nb, h);
                }
                let m = norm_mags([g[0].dd, g[1].dd, g[2].dd]);
                let ends = Ci(Ivl::from(na.hi.max(nb.hi)));
                let up = ends + h * h * Ci(m) / Ci::point(8.0);
                (finite_or_inf(up.0.hi), na.lo.max(nb.lo), None)
            }
            Method::Lipschitz => {
                let mid = 0.5 * (a + b);
                let nm = self.norm_at(mid);
                let m = norm_mags([g[0].d, g[1].d, g[2].d]);
                let up = Ci(Ivl::from(nm.hi)) + h * Ci(m) / Ci::point(2.0);
                (finite_or_inf(up.0.hi), nm.lo, None)
            }
            Method::Direct => {
                let mid = 0.5 * (a + b);
                let nm = self.norm_at(mid);
                (finite_or_inf(norm_v(&g).hi), nm.lo, None)
            }
            Method::Centred => (f64::INFINITY, 0.0, None),
        }
    }

    /// The bound `method` gives on the one piece `[a, b]` (inside one span of the trim and
    /// one of the edge), as the bisection would take it for that piece: an upper bound of
    /// `max |g|` over `[a, b]`.
    pub fn piece_bound(&self, a: f64, b: f64, method: Method) -> f64 {
        let second = matches!(method, Method::Second | Method::Centred);
        let (na, nb) = if second { (self.norm_at(a), self.norm_at(b)) } else { (Ivl::from(0.0), Ivl::from(0.0)) };
        self.enclose(a, b, na, nb, method).0
    }

    /// `max |g|` over `[a, b]` by sampling `n` equal steps (a reference for one piece).
    pub fn sampled_on(&self, a: f64, b: f64, n: usize) -> f64 {
        (0..=n).map(|i| self.gap_f64(a + (b - a) * i as f64 / n as f64)).fold(0.0, f64::max)
    }

    /// The certified bound of `max |g|` over the whole parameter range.
    pub fn certify(&self, method: Method, stop: Stop) -> Outcome {
        let start = Instant::now();
        let ops0 = ops();
        let cuts = self.cuts();
        let second = matches!(method, Method::Second | Method::Centred);
        let ends: Vec<Ivl> =
            if second { cuts.iter().map(|&t| self.norm_at(t)).collect() } else { vec![Ivl::from(0.0); cuts.len()] };
        let mut heap = BinaryHeap::new();
        let mut lower = 0.0_f64;
        for w in 0..cuts.len() - 1 {
            let (a, b) = (cuts[w], cuts[w + 1]);
            let (na, nb) = (ends[w], ends[w + 1]);
            let (upper, lo, mid) = self.enclose(a, b, na, nb, method);
            lower = lower.max(lo);
            heap.push(Piece { a, b, na, nb, mid, upper, depth: 0 });
        }
        let resolved;
        loop {
            let Some(top) = heap.peek().copied() else {
                resolved = true;
                break;
            };
            let done = match stop {
                Stop::Tight => top.upper <= lower * (1.0 + CLOSE_RELATIVE) + CLOSE_ABSOLUTE,
                Stop::Within(tol) => top.upper <= tol,
            };
            if done {
                resolved = true;
                break;
            }
            if top.depth >= MAX_DEPTH || heap.len() >= MAX_PIECES {
                resolved = false;
                break;
            }
            heap.pop();
            let m = 0.5 * (top.a + top.b);
            let nm = match (second, top.mid) {
                (true, Some(x)) => x,
                (true, None) => self.norm_at(m),
                (false, _) => Ivl::from(0.0),
            };
            for (a, b, na, nb) in [(top.a, m, top.na, nm), (m, top.b, nm, top.nb)] {
                let (upper, lo, mid) = self.enclose(a, b, na, nb, method);
                lower = lower.max(lo);
                heap.push(Piece { a, b, na, nb, mid, upper, depth: top.depth + 1 });
            }
        }
        let bound = heap.peek().map_or(0.0, |p| p.upper);
        Outcome { bound, lower, pieces: heap.len(), ops: ops() - ops0, secs: start.elapsed().as_secs_f64(), resolved }
    }

    /// `max |g|` by dense sampling (`n` equal steps), each of the 32 largest local maxima
    /// refined by golden-section search; and where it is.
    pub fn sampled_max(&self, n: usize) -> (f64, f64) {
        let [t0, t1] = self.range();
        let at = |i: usize| t0 + (t1 - t0) * i as f64 / n as f64;
        let vals: Vec<f64> = (0..=n).map(|i| self.gap_f64(at(i))).collect();
        let mut peaks: Vec<usize> =
            (0..=n).filter(|&i| (i == 0 || vals[i] >= vals[i - 1]) && (i == n || vals[i] >= vals[i + 1])).collect();
        peaks.sort_by(|&x, &y| vals[y].total_cmp(&vals[x]));
        peaks.truncate(32);
        let mut best = (0.0, t0);
        for &i in &peaks {
            let (mut lo, mut hi) = (at(i.saturating_sub(1)), at((i + 1).min(n)));
            let r = 0.5 * (5.0_f64.sqrt() - 1.0);
            for _ in 0..60 {
                let (x1, x2) = (hi - r * (hi - lo), lo + r * (hi - lo));
                if self.gap_f64(x1) > self.gap_f64(x2) {
                    hi = x2;
                } else {
                    lo = x1;
                }
            }
            for x in [at(i), 0.5 * (lo + hi)] {
                let v = self.gap_f64(x);
                if v > best.0 {
                    best = (v, x);
                }
            }
        }
        best
    }
}

/// The first-order form on a piece of width `h`: `|g(m)| + h/2 sup|g'|`, with `nm = |g(m)|`
/// enclosed and `g`'s jets over the piece. Valid for any Lipschitz `g`, `g'` jumps included.
fn first_order(g: &V3<Ci>, nm: Ivl, na: Ivl, nb: Ivl, h: Ci) -> (f64, f64, Option<Ivl>) {
    let m = norm_mags([g[0].d, g[1].d, g[2].d]);
    let up = Ci(Ivl::from(nm.hi)) + h * Ci(m) / Ci::point(2.0);
    (finite_or_inf(up.0.hi), na.lo.max(nb.lo).max(nm.lo), Some(nm))
}

fn finite_or_inf(x: f64) -> f64 {
    if x.is_nan() {
        f64::INFINITY
    } else {
        x
    }
}
