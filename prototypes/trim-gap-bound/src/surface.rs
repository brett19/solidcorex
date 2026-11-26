//! The surfaces of the study: a bicubic Bézier patch (the one-span NURBS case), a quarter
//! torus and a sphere octant (analytic cases; the sphere has a pole), and a net of bicubic
//! Hermite patches (the stand-in for a procedural blend's B-spline net).
//!
//! Each is evaluated with its first partials, generic over the scalar, at jets `u(t)` and
//! `v(t)`: the result's jets carry the derivatives of `S(c(t))`, `S_u(c(t))` and
//! `S_v(c(t))` along the trim's parameter.

use scx_num::ring::{Ring, TayScalar};

use crate::jet::{Jet, V3};

/// `S`, `S_u` and `S_v` at one parameter.
pub type Frame<T> = [V3<T>; 3];

/// A parametric surface made of patches, each a smooth (polynomial or analytic) map.
pub trait Surface {
    /// The whole parameter box `[u0, u1, v0, v1]`.
    fn domain(&self) -> [f64; 4];
    /// The patches meeting the box `u x v`, with each patch's own box.
    fn patches(&self, u: [f64; 2], v: [f64; 2]) -> Vec<(usize, [f64; 4])>;
    /// `S`, `S_u`, `S_v` of patch `patch` at `(u, v)`.
    fn eval<T: TayScalar>(&self, patch: usize, u: Jet<T>, v: Jet<T>) -> Frame<T>;
}

/// Bernstein cubic basis and its derivative.
fn bernstein<T: Ring>(u: Jet<T>) -> ([Jet<T>; 4], [Jet<T>; 4]) {
    let w = Jet::cst(1.0) - u;
    let (uu, ww, uw) = (u * u, w * w, u * w);
    (
        [ww * w, (u * ww).scale(3.0), (uu * w).scale(3.0), uu * u],
        [ww.scale(-3.0), ww.scale(3.0) - uw.scale(6.0), uw.scale(6.0) - uu.scale(3.0), uu.scale(3.0)],
    )
}

/// A bicubic Bézier patch on `[0, 1]^2`: a single-span, non-rational bicubic NURBS.
#[derive(Clone, Debug)]
pub struct Bezier {
    /// Control points, `p[i][j]` for `u` index `i` and `v` index `j`.
    pub p: [[[f64; 3]; 4]; 4],
}

impl Surface for Bezier {
    fn domain(&self) -> [f64; 4] {
        [0.0, 1.0, 0.0, 1.0]
    }
    fn patches(&self, _u: [f64; 2], _v: [f64; 2]) -> Vec<(usize, [f64; 4])> {
        vec![(0, self.domain())]
    }
    fn eval<T: TayScalar>(&self, _patch: usize, u: Jet<T>, v: Jet<T>) -> Frame<T> {
        let (bu, dbu) = bernstein(u);
        let (bv, dbv) = bernstein(v);
        let z = Jet::cst(0.0);
        let mut f = [[z; 3]; 3];
        for (i, row) in self.p.iter().enumerate() {
            for (j, pt) in row.iter().enumerate() {
                let (s, su, sv) = (bu[i] * bv[j], dbu[i] * bv[j], bu[i] * dbv[j]);
                for k in 0..3 {
                    let c = Jet::cst(pt[k]);
                    f[0][k] = f[0][k] + s * c;
                    f[1][k] = f[1][k] + su * c;
                    f[2][k] = f[2][k] + sv * c;
                }
            }
        }
        f
    }
}

/// A quarter torus `u, v in [0, pi/2]`, major radius `big`, minor `small`; with `big = 0`
/// a sphere octant whose pole (`v = pi/2`) has `S_u = 0`.
///
/// The interval sine and cosine of `scx-num` are proved on the first quadrant only, so the
/// domain is that quadrant.
#[derive(Clone, Debug)]
pub struct Torus {
    /// Distance from the axis to the tube's centre.
    pub big: f64,
    /// The tube's radius.
    pub small: f64,
}

impl Surface for Torus {
    fn domain(&self) -> [f64; 4] {
        let q = core::f64::consts::FRAC_PI_2;
        [0.0, q, 0.0, q]
    }
    fn patches(&self, _u: [f64; 2], _v: [f64; 2]) -> Vec<(usize, [f64; 4])> {
        vec![(0, self.domain())]
    }
    fn eval<T: TayScalar>(&self, _patch: usize, u: Jet<T>, v: Jet<T>) -> Frame<T> {
        let (su, cu) = u.sin_cos();
        let (sv, cv) = v.sin_cos();
        let rho = Jet::cst(self.big) + cv.scale(self.small);
        let z = Jet::cst(0.0);
        let rsv = sv.scale(self.small);
        [
            [rho * cu, rho * su, rsv],
            [z - rho * su, rho * cu, z],
            [z - rsv * cu, z - rsv * su, cv.scale(self.small)],
        ]
    }
}

/// A net of bicubic Hermite patches over a grid: position, both first partials and the
/// twist at every node. With those data exact, the net is C^1 by construction (both
/// patches at a shared line see the same node data), whatever rounding its evaluation has.
#[derive(Clone, Debug)]
pub struct HermiteNet {
    /// The `u` grid lines.
    pub ku: Vec<f64>,
    /// The `v` grid lines.
    pub kv: Vec<f64>,
    /// `[P, P_u, P_v, P_uv]` at node `(i, j)`, index `i * kv.len() + j`.
    pub node: Vec<[[f64; 3]; 4]>,
}

/// Cubic Hermite basis on `[0, 1]`: value at the start, slope at the start, value at the
/// end, slope at the end; and the four derivatives.
pub fn hermite<T: Ring>(s: Jet<T>) -> ([Jet<T>; 4], [Jet<T>; 4]) {
    let one = Jet::cst(1.0);
    let ss = s * s;
    let sm1 = s - one;
    (
        [ss * (s.scale(2.0) - Jet::cst(3.0)) + one, s * sm1 * sm1, ss * (Jet::cst(3.0) - s.scale(2.0)), ss * sm1],
        [(s * sm1).scale(6.0), (s.scale(3.0) - one) * sm1, (s * sm1).scale(-6.0), s * (s.scale(3.0) - Jet::cst(2.0))],
    )
}

impl HermiteNet {
    /// The net interpolating `def` (position, partials, twist) at an `nu x nv` grid over
    /// its domain.
    pub fn of<S: Surface>(def: &S, nu: usize, nv: usize) -> Self {
        let [u0, u1, v0, v1] = def.domain();
        let ku: Vec<f64> = (0..=nu).map(|i| u0 + (u1 - u0) * i as f64 / nu as f64).collect();
        let kv: Vec<f64> = (0..=nv).map(|j| v0 + (v1 - v0) * j as f64 / nv as f64).collect();
        let mut node = Vec::new();
        for &u in &ku {
            for &v in &kv {
                let f = def.eval::<f64>(0, Jet::of(u), Jet::var(v));
                let p = [f[0][0].v, f[0][1].v, f[0][2].v];
                let pu = [f[1][0].v, f[1][1].v, f[1][2].v];
                let pv = [f[2][0].v, f[2][1].v, f[2][2].v];
                let puv = [f[1][0].d, f[1][1].d, f[1][2].d];
                node.push([p, pu, pv, puv]);
            }
        }
        Self { ku, kv, node }
    }

    fn nv(&self) -> usize {
        self.kv.len() - 1
    }
}

fn spans(k: &[f64], lo: f64, hi: f64) -> Vec<usize> {
    (0..k.len() - 1).filter(|&i| k[i] <= hi && lo <= k[i + 1]).collect()
}

impl Surface for HermiteNet {
    fn domain(&self) -> [f64; 4] {
        [self.ku[0], self.ku[self.ku.len() - 1], self.kv[0], self.kv[self.kv.len() - 1]]
    }
    fn patches(&self, u: [f64; 2], v: [f64; 2]) -> Vec<(usize, [f64; 4])> {
        let mut out = Vec::new();
        for i in spans(&self.ku, u[0], u[1]) {
            for j in spans(&self.kv, v[0], v[1]) {
                out.push((i * self.nv() + j, [self.ku[i], self.ku[i + 1], self.kv[j], self.kv[j + 1]]));
            }
        }
        out
    }
    fn eval<T: TayScalar>(&self, patch: usize, u: Jet<T>, v: Jet<T>) -> Frame<T> {
        let (i, j) = (patch / self.nv(), patch % self.nv());
        let lift = |x: f64| T::lift(x);
        let hu = lift(self.ku[i + 1]) - lift(self.ku[i]);
        let hv = lift(self.kv[j + 1]) - lift(self.kv[j]);
        let s = (u - Jet::cst(self.ku[i])) / Jet::of(hu);
        let t = (v - Jet::cst(self.kv[j])) / Jet::of(hv);
        let (bs, dbs) = hermite(s);
        let (bt, dbt) = hermite(t);
        let (jhu, jhv) = (Jet::of(hu), Jet::of(hv));
        let z = Jet::cst(0.0);
        let mut f = [[z; 3]; 3];
        let row = self.kv.len();
        for a in 0..2 {
            for b in 0..2 {
                let nd = &self.node[(i + a) * row + j + b];
                // value basis index 2a, slope basis index 2a + 1
                let (va, sa, dva, dsa) = (bs[2 * a], bs[2 * a + 1], dbs[2 * a], dbs[2 * a + 1]);
                let (vb, sb, dvb, dsb) = (bt[2 * b], bt[2 * b + 1], dbt[2 * b], dbt[2 * b + 1]);
                // S = sum va vb P + hu sa vb Pu + hv va sb Pv + hu hv sa sb Puv, and its
                // derivatives in s and t (divided by hu and hv below)
                let w = [va * vb, jhu * sa * vb, jhv * va * sb, jhu * jhv * sa * sb];
                let wu = [dva * vb, jhu * dsa * vb, jhv * dva * sb, jhu * jhv * dsa * sb];
                let wv = [va * dvb, jhu * sa * dvb, jhv * va * dsb, jhu * jhv * sa * dsb];
                for (q, data) in nd.iter().enumerate() {
                    for k in 0..3 {
                        let c = Jet::cst(data[k]);
                        f[0][k] = f[0][k] + w[q] * c;
                        f[1][k] = f[1][k] + wu[q] * c;
                        f[2][k] = f[2][k] + wv[q] * c;
                    }
                }
            }
        }
        for k in 0..3 {
            f[1][k] = f[1][k] / jhu;
            f[2][k] = f[2][k] / jhv;
        }
        f
    }
}
