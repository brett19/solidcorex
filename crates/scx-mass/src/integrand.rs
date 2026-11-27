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
                *z += t.z * *n;
                *ez += t.z.abs() * *en;
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
                    *x += d * a;
                    *ex += d.abs() * ea;
                }
            }
            let old: Vec<f64> = v.iter().skip(FIRST_MONOMIAL).copied().collect();
            let old_err: Vec<f64> = e.iter().skip(FIRST_MONOMIAL).copied().collect();
            let tt = [t.x, t.y, t.z];
            for (j, m) in MONOMIALS.iter().enumerate() {
                let (mut sum, mut sum_err) = (0.0, 0.0);
                for (i, q) in MONOMIALS.iter().enumerate() {
                    if q.iter().zip(m).any(|(qa, ma)| qa > ma) {
                        continue;
                    }
                    let mut c = 1.0;
                    for ((&ma, &qa), &d) in m.iter().zip(q).zip(&tt) {
                        c *= binomial(ma, qa) * power(d, ma - qa);
                    }
                    sum += c * old.get(i).copied().unwrap_or(0.0);
                    sum_err += c.abs() * old_err.get(i).copied().unwrap_or(0.0);
                }
                if let (Some(x), Some(ex)) =
                    (v.get_mut(FIRST_MONOMIAL + j), e.get_mut(FIRST_MONOMIAL + j))
                {
                    *x = sum;
                    *ex = sum_err;
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
