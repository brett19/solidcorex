//! The certified same-parameter distance `eps = max |B(u, v) - S(u, v)|` between a net `B`
//! and the definition `S` it approximates, over a parameter box: what a procedural blend
//! carries as its net's bound. With it, for any trim `c` and edge `C` on one parameter,
//! `|S(c(t)) - C(t)| <= |B(c(t)) - C(t)| + eps` (the triangle inequality at the same
//! `(u, v)`), so the trim gap on the definition is the gap on the net plus `eps`.
//!
//! Each box (inside one patch of the net) is enclosed by the bilinear interpolant of
//! `f = B - S` at its corners plus `hu^2/8 sup|f_uu| + hv^2/8 sup|f_vv|` (the tensor-product
//! interpolation error), the second partials in centred form (`|f_uu(c)| + hu/2 sup|f_uuu|
//! + hv/2 sup|f_uuv|` at the centre `c`, and the same for `f_vv`) from jets over jets, and
//! bisected until within a hundredth of the largest corner value.

use std::collections::BinaryHeap;
use std::time::Instant;

use scx_num::interval::Ivl;
use scx_num::ring::TayScalar;

use crate::counted::{ops, Ci};
use crate::gap::frame_f64;
use crate::jet::{set_third, Jet};
use crate::surface::Surface;

/// The bound and its cost.
#[derive(Clone, Copy, Debug)]
pub struct EpsOutcome {
    /// The proved bound.
    pub bound: f64,
    /// The largest corner value proved attained.
    pub lower: f64,
    /// Boxes at the end.
    pub boxes: usize,
    /// Interval operations.
    pub ops: u64,
    /// Wall time in seconds.
    pub secs: f64,
    /// The stop condition was met (otherwise the box cap was hit).
    pub resolved: bool,
}

/// The box cap.
pub const MAX_BOXES: usize = 1 << 16;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Cell {
    upper: f64,
    patch: usize,
    b: [f64; 4],
}
impl Eq for Cell {}
impl PartialOrd for Cell {
    fn partial_cmp(&self, o: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Cell {
    fn cmp(&self, o: &Self) -> core::cmp::Ordering {
        self.upper.total_cmp(&o.upper)
    }
}

type J2 = Jet<Jet<Ci>>;

/// `B - S` on patch `patch` of the net (the definition has one patch).
fn diff<N: Surface, D: Surface, T: TayScalar>(net: &N, def: &D, patch: usize, u: Jet<T>, v: Jet<T>) -> [Jet<T>; 3] {
    let (fb, fs) = (net.eval(patch, u, v), def.eval(0, u, v));
    [fb[0][0] - fs[0][0], fb[0][1] - fs[0][1], fb[0][2] - fs[0][2]]
}

fn mag(c: Ci) -> f64 {
    c.0.mag()
}

fn norm3(m: [f64; 3]) -> Ivl {
    let m = m.map(|x| Ci(Ivl::from(x)));
    (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).0.sqrt()
}

/// The nested jet of a parameter box: outer derivative along `u`, inner along `v`.
fn box_jets(u: Ci, v: Ci) -> (J2, J2) {
    let (z, one) = (Jet::cst(0.0), Jet::cst(1.0));
    (Jet { v: Jet::of(u), d: one, dd: z, ddd: z }, Jet { v: Jet::var(v), d: z, dd: z, ddd: z })
}

/// `(upper, lower, u term, v term)` on one box.
fn enclose<N: Surface, D: Surface>(net: &N, def: &D, patch: usize, b: [f64; 4]) -> (f64, f64, f64, f64) {
    let mut corner = Ivl::from(0.0);
    for (u, v) in [(b[0], b[2]), (b[1], b[2]), (b[0], b[3]), (b[1], b[3])] {
        let f = diff(net, def, patch, Jet::of(Ci::point(u)), Jet::of(Ci::point(v)));
        let s = f[0].v * f[0].v + f[1].v * f[1].v + f[2].v * f[2].v;
        let n = Ivl::new(s.0.lo.max(0.0), s.0.hi).sqrt();
        corner = Ivl::new(corner.lo.max(n.lo), corner.hi.max(n.hi));
    }
    let (cu, cv) = (0.5 * (b[0] + b[1]), 0.5 * (b[2] + b[3]));
    let (ju, jv) = box_jets(Ci::point(cu), Ci::point(cv));
    let fc = diff(net, def, patch, ju, jv);
    set_third(true);
    let (ju, jv) = box_jets(Ci::span(b[0], b[1]), Ci::span(b[2], b[3]));
    let fb = diff(net, def, patch, ju, jv);
    set_third(false);
    let (hu, hv) = (Ci::point(b[1]) - Ci::point(b[0]), Ci::point(b[3]) - Ci::point(b[2]));
    let (hu2, hv2) = (hu / Ci::point(2.0), hv / Ci::point(2.0));
    // f_uu = outer.dd.v; f_uuu = outer.ddd.v; f_uuv = outer.dd.d;
    // f_vv = outer.v.dd; f_vvv = outer.v.ddd; f_vvu = outer.d.dd
    let fuu: [f64; 3] = core::array::from_fn(|k| {
        let centred = Ci(Ivl::from(mag(fc[k].dd.v)))
            + hu2 * Ci(Ivl::from(mag(fb[k].ddd.v)))
            + hv2 * Ci(Ivl::from(mag(fb[k].dd.d)));
        centred.0.hi.min(mag(fb[k].dd.v))
    });
    let fvv: [f64; 3] = core::array::from_fn(|k| {
        let centred = Ci(Ivl::from(mag(fc[k].v.dd)))
            + hv2 * Ci(Ivl::from(mag(fb[k].v.ddd)))
            + hu2 * Ci(Ivl::from(mag(fb[k].d.dd)));
        centred.0.hi.min(mag(fb[k].v.dd))
    });
    let eight = Ci::point(8.0);
    let tu = hu * hu * Ci(norm3(fuu)) / eight;
    let tv = hv * hv * Ci(norm3(fvv)) / eight;
    let up = Ci(Ivl::from(corner.hi)) + tu + tv;
    let up = if up.0.hi.is_nan() { f64::INFINITY } else { up.0.hi };
    (up, corner.lo, tu.0.hi, tv.0.hi)
}

/// The certified `max |B - S|` over the net's patches inside `area` (`[u0, u1, v0, v1]`,
/// inside the definition's domain).
pub fn net_gap_bound<N: Surface, D: Surface>(net: &N, def: &D, area: [f64; 4]) -> EpsOutcome {
    let start = Instant::now();
    let ops0 = ops();
    let mut heap = BinaryHeap::new();
    let mut lower = 0.0_f64;
    for (patch, p) in net.patches([area[0], area[1]], [area[2], area[3]]) {
        let b = [p[0].max(area[0]), p[1].min(area[1]), p[2].max(area[2]), p[3].min(area[3])];
        if b[0] >= b[1] || b[2] >= b[3] {
            continue;
        }
        let (upper, lo, _, _) = enclose(net, def, patch, b);
        lower = lower.max(lo);
        heap.push(Cell { upper, patch, b });
    }
    let mut resolved = true;
    while let Some(top) = heap.peek().copied() {
        if top.upper <= lower * 1.01 + 1.0e-15 {
            break;
        }
        if heap.len() >= MAX_BOXES {
            resolved = false;
            break;
        }
        heap.pop();
        let b = top.b;
        let (_, _, tu, tv) = enclose(net, def, top.patch, b);
        let halves = if tu >= tv {
            let m = 0.5 * (b[0] + b[1]);
            [[b[0], m, b[2], b[3]], [m, b[1], b[2], b[3]]]
        } else {
            let m = 0.5 * (b[2] + b[3]);
            [[b[0], b[1], b[2], m], [b[0], b[1], m, b[3]]]
        };
        for h in halves {
            let (upper, lo, _, _) = enclose(net, def, top.patch, h);
            lower = lower.max(lo);
            heap.push(Cell { upper, patch: top.patch, b: h });
        }
    }
    let bound = heap.peek().map_or(0.0, |c| c.upper);
    EpsOutcome { bound, lower, boxes: heap.len(), ops: ops() - ops0, secs: start.elapsed().as_secs_f64(), resolved }
}

/// `max |B - S|` sampled on an `n x n` grid of `area`.
pub fn net_gap_sampled<N: Surface, D: Surface>(net: &N, def: &D, area: [f64; 4], n: usize) -> f64 {
    let mut best = 0.0_f64;
    for i in 0..=n {
        for j in 0..=n {
            let u = area[0] + (area[1] - area[0]) * i as f64 / n as f64;
            let v = area[2] + (area[3] - area[2]) * j as f64 / n as f64;
            let (fb, fs) = (frame_f64(net, Jet::of(u), Jet::of(v)), frame_f64(def, Jet::of(u), Jet::of(v)));
            let d: f64 = (0..3).map(|k| (fb[0][k].v - fs[0][k].v).powi(2)).sum::<f64>().sqrt();
            best = best.max(d);
        }
    }
    best
}
