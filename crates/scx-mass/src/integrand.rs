// Written for solidcorex. Apache-2.0.
//! What a face integral integrates, and how its result moves to another reference.
//!
//! At a surface point `S(u, v)`, with `p = S - r` about the face's reference `r` and the
//! natural normal `n = S_u x S_v` (its length the area element):
//!
//! - the area terms: `|n|` and `p |n|`;
//! - the flux monomials `M[a, b, c] = ∫ x^a y^b z^c n_z dA` for `a + b + c <= 3`, `p = (x, y, z)`.
//!
//! The divergence theorem with `F = (0, 0, x^a y^b z^(c+1) / (c + 1))` gives each volume
//! moment of degree up to 2 as one monomial with `c >= 1` (`∫ x^a y^b z^c dV =
//! M[a, b, c + 1] / (c + 1)`). The monomials with `c = 0` are kept so that a face's result
//! moves exactly to another reference: a monomial about `r` expands binomially into the
//! monomials of no higher degree about `r'` (the shift). A request takes only the
//! components it reads ([`Set`]).

use scx_num::vector3d::Vec3;

use crate::kronrod::MAX_COMPONENTS;
use crate::rounding::gamma;

/// The roundings of a linear shift's term `t a`: `t`'s own (a difference of references),
/// the product, and the sum's.
const SHIFT_LINEAR: u32 = 2;
/// The roundings of a binomial shift's terms, of their absolute sum: `t`'s own and its
/// powers (up to 9 products over the three axes), the coefficient's product with the old
/// value, and the sum of up to 20 terms, rounded up.
const SHIFT_ROUNDINGS: u32 = 32;

/// The components a request integrates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Set {
    /// `|n|`.
    Area,
    /// `n_z` and `z n_z`: the volume, and what moves it.
    Volume,
    /// `|n|`, `p |n|`, then the 20 monomials in [`MONOMIALS`] order.
    Full,
}

/// The monomials `(a, b, c)` of degree up to 3, by degree then lexicographically from the
/// highest power of `x`.
pub(crate) const MONOMIALS: [[u8; 3]; 20] = [
    [0, 0, 0],
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
    [2, 0, 0],
    [1, 1, 0],
    [1, 0, 1],
    [0, 2, 0],
    [0, 1, 1],
    [0, 0, 2],
    [3, 0, 0],
    [2, 1, 0],
    [2, 0, 1],
    [1, 2, 0],
    [1, 1, 1],
    [1, 0, 2],
    [0, 3, 0],
    [0, 2, 1],
    [0, 1, 2],
    [0, 0, 3],
];

/// The first monomial's place among [`Set::Full`]'s components.
pub(crate) const FIRST_MONOMIAL: usize = 4;

impl Set {
    /// How many components.
    pub(crate) const fn len(self) -> usize {
        match self {
            Self::Area => 1,
            Self::Volume => 2,
            Self::Full => FIRST_MONOMIAL + MONOMIALS.len(),
        }
    }

    /// The first component that is a volume term: the closure of a gap adds to these alone,
    /// the area terms having no counterpart there.
    pub(crate) const fn volume_start(self) -> usize {
        match self {
            Self::Area => 1,
            Self::Volume => 0,
            Self::Full => FIRST_MONOMIAL,
        }
    }

    /// Each component's length dimension past the area's: the power of a length its size
    /// grows by.
    pub(crate) fn powers(self) -> [u32; MAX_COMPONENTS] {
        let mut out = [0; MAX_COMPONENTS];
        match self {
            Self::Area => {}
            Self::Volume => {
                if let Some(p) = out.get_mut(1) {
                    *p = 1;
                }
            }
            Self::Full => {
                for p in out.iter_mut().take(FIRST_MONOMIAL).skip(1) {
                    *p = 1;
                }
                for (p, m) in out.iter_mut().skip(FIRST_MONOMIAL).zip(MONOMIALS) {
                    *p = u32::from(m[0] + m[1] + m[2]);
                }
            }
        }
        out
    }

    /// The components at `p` (about the reference) with normal `n`, into `out`.
    pub(crate) fn eval(self, p: Vec3, n: Vec3, out: &mut [f64]) {
        match self {
            Self::Area => {
                if let Some(o) = out.first_mut() {
                    *o = n.length();
                }
            }
            Self::Volume => {
                if let [a, b, ..] = out {
                    *a = n.z;
                    *b = p.z * n.z;
                }
            }
            Self::Full => {
                let len = n.length();
                if let [a, x, y, z, rest @ ..] = out {
                    *a = len;
                    *x = p.x * len;
                    *y = p.y * len;
                    *z = p.z * len;
                    let px = [1.0, p.x, p.x * p.x, p.x * p.x * p.x];
                    let py = [1.0, p.y, p.y * p.y, p.y * p.y * p.y];
                    let pz = [1.0, p.z, p.z * p.z, p.z * p.z * p.z];
                    for (o, m) in rest.iter_mut().zip(MONOMIALS) {
                        let g = |a: &[f64; 4], i: u8| a.get(usize::from(i)).copied().unwrap_or(0.0);
                        *o = g(&px, m[0]) * g(&py, m[1]) * g(&pz, m[2]) * n.z;
                    }
                }
            }
        }
    }

    /// The rounding of the components at `p` with normal `n`, into `error`, and their
    /// change along `du`, into `along`, each to first order with every term taken
    /// absolutely (at `|p|`, `|n|`): `error` is how far they move when `p` moves by up to
    /// `dp` and `n` by up to `dn` per axis, plus `rounds` times their absolute size (their
    /// own evaluation's rounding); `along` is how far they move per unit step of the point
    /// along `du`, the normal held. One pass, the powers taken once.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn rounding(
        self,
        (p, n): (Vec3, Vec3),
        (dp, dn): (Vec3, Vec3),
        du: Vec3,
        rounds: f64,
        error: &mut [f64],
        along: &mut [f64],
    ) {
        let ab = |v: Vec3| Vec3::new(v.x.abs(), v.y.abs(), v.z.abs());
        let (p, n, dp, dn, du) = (ab(p), ab(n), ab(dp), ab(dn), ab(du));
        match self {
            Self::Area => {
                if let Some(o) = error.first_mut() {
                    *o = dn.length() + rounds * n.length();
                }
                if let Some(o) = along.first_mut() {
                    *o = 0.0;
                }
            }
            Self::Volume => {
                if let ([a, b, ..], [la, lb, ..]) = (error, along) {
                    *a = dn.z + rounds * n.z;
                    *b = dp.z * n.z + p.z * dn.z + rounds * p.z * n.z;
                    *la = 0.0;
                    *lb = du.z * n.z;
                }
            }
            Self::Full => {
                let (len, dlen) = (n.length(), dn.length());
                if let ([a, x, y, z, rest @ ..], [la, lx, ly, lz, lrest @ ..]) = (error, along) {
                    *a = dlen + rounds * len;
                    *x = dp.x * len + p.x * dlen + rounds * p.x * len;
                    *y = dp.y * len + p.y * dlen + rounds * p.y * len;
                    *z = dp.z * len + p.z * dlen + rounds * p.z * len;
                    *la = 0.0;
                    *lx = du.x * len;
                    *ly = du.y * len;
                    *lz = du.z * len;
                    // `x^k` and its derivative `k x^(k-1)` per axis.
                    let pw = |t: f64| [1.0, t, t * t, t * t * t];
                    let dw = |t: f64| [0.0, 1.0, 2.0 * t, 3.0 * t * t];
                    let (px, py, pz) = (pw(p.x), pw(p.y), pw(p.z));
                    let (qx, qy, qz) = (dw(p.x), dw(p.y), dw(p.z));
                    let g = |a: &[f64; 4], i: u8| a.get(usize::from(i)).copied().unwrap_or(0.0);
                    for ((o, l), m) in rest.iter_mut().zip(lrest.iter_mut()).zip(MONOMIALS) {
                        let (mx, my, mz) = (g(&px, m[0]), g(&py, m[1]), g(&pz, m[2]));
                        let (gx, gy, gz) = (
                            g(&qx, m[0]) * my * mz,
                            mx * g(&qy, m[1]) * mz,
                            mx * my * g(&qz, m[2]),
                        );
                        let mono = mx * my * mz;
                        let d = gx * dp.x + gy * dp.y + gz * dp.z;
                        *o = d * n.z + mono * dn.z + rounds * mono * n.z;
                        *l = (gx * du.x + gy * du.y + gz * du.z) * n.z;
                    }
                }
            }
        }
    }
}

/// The binomial coefficient `C(n, k)` for `n <= 3`.
fn binomial(n: u8, k: u8) -> f64 {
    match (n, k) {
        (_, 0) => 1.0,
        (n, k) if k == n => 1.0,
        (2, 1) => 2.0,
        (3, 1 | 2) => 3.0,
        _ => 0.0,
    }
}

/// `t^k` for `k <= 3`.
fn power(t: f64, k: u8) -> f64 {
    (0..k).fold(1.0, |a, _| a * t)
}

/// A face's components about its reference moved by `t` (the face's reference less the new
/// one): values exactly by the binomial expansion, errors by the same with absolute
/// coefficients. `value` and `error` are [`Set`]-ordered for `set`.
pub(crate) fn shift(set: Set, t: Vec3, value: &mut [f64], error: &mut [f64]) {
    match set {
        Set::Area => {}
        Set::Volume => {
            if let ([n, z, ..], [en, ez, ..]) = (value, error) {
                let moved = t.z * *n;
                *z += moved;
                *ez += t.z.abs() * *en + gamma(SHIFT_LINEAR) * moved.abs() + gamma(1) * z.abs();
            }
        }
        Set::Full => {
            let (Some(v), Some(e)) = (value.get_mut(..), error.get_mut(..)) else {
                return;
            };
            let (a, ea) = (
                v.first().copied().unwrap_or(0.0),
                e.first().copied().unwrap_or(0.0),
            );
            for (i, d) in [t.x, t.y, t.z].into_iter().enumerate() {
                if let (Some(x), Some(ex)) = (v.get_mut(1 + i), e.get_mut(1 + i)) {
                    let moved = d * a;
                    *x += moved;
                    *ex += d.abs() * ea + gamma(SHIFT_LINEAR) * moved.abs() + gamma(1) * x.abs();
                }
            }
            let old: Vec<f64> = v.iter().skip(FIRST_MONOMIAL).copied().collect();
            let old_err: Vec<f64> = e.iter().skip(FIRST_MONOMIAL).copied().collect();
            let tt = [t.x, t.y, t.z];
            for (j, m) in MONOMIALS.iter().enumerate() {
                let (mut sum, mut sum_err, mut sum_abs) = (0.0, 0.0, 0.0);
                for (i, q) in MONOMIALS.iter().enumerate() {
                    if q.iter().zip(m).any(|(qa, ma)| qa > ma) {
                        continue;
                    }
                    let mut c = 1.0;
                    for ((&ma, &qa), &d) in m.iter().zip(q).zip(&tt) {
                        c *= binomial(ma, qa) * power(d, ma - qa);
                    }
                    let term = c * old.get(i).copied().unwrap_or(0.0);
                    sum += term;
                    sum_abs += term.abs();
                    sum_err += c.abs() * old_err.get(i).copied().unwrap_or(0.0);
                }
                if let (Some(x), Some(ex)) =
                    (v.get_mut(FIRST_MONOMIAL + j), e.get_mut(FIRST_MONOMIAL + j))
                {
                    *x = sum;
                    *ex = sum_err + gamma(SHIFT_ROUNDINGS) * sum_abs;
                }
            }
        }
    }
}

/// The place of monomial `m` among [`Set::Full`]'s components.
pub(crate) fn monomial(m: [u8; 3]) -> usize {
    FIRST_MONOMIAL + MONOMIALS.iter().position(|q| *q == m).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Moving the components by `t` equals evaluating them about the moved reference.
    #[test]
    fn the_shift_is_the_expansion() {
        let p = Vec3::new(0.3, -1.25, 2.5);
        let n = Vec3::new(0.5, 0.75, -1.5);
        let t = Vec3::new(1.5, -0.5, 4.0);
        for set in [Set::Volume, Set::Full] {
            let mut at = [0.0; MAX_COMPONENTS];
            let mut moved = [0.0; MAX_COMPONENTS];
            set.eval(p, n, &mut at[..set.len()]);
            set.eval(p + t, n, &mut moved[..set.len()]);
            let mut err = [0.0; MAX_COMPONENTS];
            shift(set, t, &mut at[..set.len()], &mut err[..set.len()]);
            for (a, b) in at.iter().zip(&moved) {
                assert!((a - b).abs() <= 1e-13 * b.abs().max(1.0), "{a} against {b}");
            }
        }
    }
}
