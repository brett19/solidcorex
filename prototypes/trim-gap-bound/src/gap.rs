//! The certified bound on the trim-to-edge gap `max_t |Q(c(t)) - C(t)|`, where `Q = S + d N`
//! is the offset at distance `d` of the base `S` (`d = 0`: the base itself), `c` the UV trim
//! and `C` the edge, both on the same parameter `t`.
//!
//! Write `g(t) = Q(c(t)) - C(t)`. The pieces are cut at both curves' knots, so on each piece
//! the trim and the edge are single cubics and `g` is C^1 with a bounded second derivative
//! (the offset of a smooth patch; across a C^1 net's patch lines the second derivative jumps
//! but stays bounded). Four enclosures of `max |g|` over a piece `[a, b]`, `h = b - a`, are
//! compared:
//!
//! - **second order**: `max(|g(a)|, |g(b)|) + h^2/8 sup|g''|` (the chord of `g` is a convex
//!   combination of the end values, and each coordinate leaves it by at most
//!   `(t - a)(b - t)/2 sup|g_i''|`);
//! - **centred second order**: the same, with `sup|g''|` taken as `|g''(m)| + h/2 sup|g'''|`
//!   at the middle `m` where that is smaller (third-order jets over the piece);
//! - **first order (Lipschitz)**: `|g(m)| + h/2 sup|g'|` at the middle `m`;
//! - **direct**: the interval extension of `|g|` over the piece.
//!
//! Every enclosure is evaluated with outward-rounded intervals through jets (second order,
//! third for the centred form), so `sup|g''|` comes from the chain rule through
//! `N = S_u x S_v / |S_u x S_v|` with no separately derived bound. A piece where
//! `|S_u x S_v|` is not proved positive encloses the normal as the whole line and is
//! bisected; a piece that cannot be resolved by the depth or piece cap leaves the bound
//! infinite (refused).

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
    /// The proved upper bound (infinite where refused).
    pub bound: f64,
    /// The largest value proved to be attained (a lower bound on the maximum).
    pub lower: f64,
    /// Pieces at the end.
    pub pieces: usize,
    /// Interval operations.
    pub ops: u64,
    /// Wall time in seconds.
    pub secs: f64,
    /// The stop condition was met (otherwise a cap was hit).
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
/// is evaluated on its part of the box, and the results are hulled. `None` where the box
/// leaves the surface's domain (the trim is not on the surface there).
pub fn frame_ci<S: Surface>(surf: &S, u: Jet<Ci>, v: Jet<Ci>) -> Option<Frame<Ci>> {
    let [u0, u1, v0, v1] = surf.domain();
    let (ui, vi) = (u.v.0, v.v.0);
    if !(ui.is_finite() && vi.is_finite() && ui.lo >= u0 && ui.hi <= u1 && vi.lo >= v0 && vi.hi <= v1) {
        return None;
    }
    let mut out: Option<Frame<Ci>> = None;
    for (p, b) in surf.patches([ui.lo, ui.hi], [vi.lo, vi.hi]) {
        let (Some(pu), Some(pv)) = (clip(u, b[0], b[1]), clip(v, b[2], b[3])) else {
            continue;
        };
        let f = surf.eval(p, pu, pv);
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
    out
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

    /// `g` over `t` (spans `kc`, `ke`), enclosed.
    fn g_ci(&self, t: Jet<Ci>, kc: usize, ke: usize) -> Option<V3<Ci>> {
        let uv = self.trim.eval(kc, t);
        let f = frame_ci(&self.surf, uv[0], uv[1])?;
        let q = offset_point(&f, self.d);
        let e = self.edge.eval(ke, t);
        Some(sub3(q, e))
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
        self.g_ci(Jet::var(Ci::point(t)), kc, ke).map_or(Ivl::whole(), |g| norm_v(&g))
    }

    /// Upper bound of `|g|` on `[a, b]` by `method`, a value proved attained, and `|g|` at
    /// the middle where it was computed.
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
            let (Some(gm), Some(g)) = (gm, g) else {
                return (f64::INFINITY, 0.0, None);
            };
            let nm = norm_v(&gm);
            let half = h / Ci::point(2.0);
            let m: [Ci; 3] = core::array::from_fn(|i| {
                let centred = Ci(Ivl::from(gm[i].dd.0.mag())) + half * Ci(Ivl::from(g[i].ddd.0.mag()));
                Ci(Ivl::from(centred.0.hi.min(g[i].dd.0.mag())))
            });
            let ends = Ci(Ivl::from(na.hi.max(nb.hi)));
            let up = ends + h * h * Ci(norm_mags(m)) / Ci::point(8.0);
            return (finite_or_inf(up.0.hi), na.lo.max(nb.lo).max(nm.lo), Some(nm));
        }
        let Some(g) = self.g_ci(t, kc, ke) else {
            return (f64::INFINITY, 0.0, None);
        };
        match method {
            Method::Second => {
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

fn finite_or_inf(x: f64) -> f64 {
    if x.is_nan() {
        f64::INFINITY
    } else {
        x
    }
}
