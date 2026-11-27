// Written for solidcorex. Apache-2.0.
//! Rounding in the stated error (research 72 §14.31, MA6).
//!
//! An integral whose terms cancel (a loop's pieces far larger than the area they enclose, a
//! face far from its body's reference) loses what the terms' rounding was, which the
//! Kronrod-Gauss difference does not see: where the rules are exact (a plane, a polynomial
//! integrand) that difference is zero whatever the rounding. So every value carries, beside
//! its estimate, a first-order bound of its rounding, in units of [`U`]:
//!
//! - **Sums** are compensated ([`Sum`], the cascaded `TwoSum` of Ogita, Rump and Oishi's
//!   `Sum2`), with its proved bound `u |s| + gamma(n-1)^2 sum |x|`: the cancellation in a sum
//!   costs nothing past the result's own rounding.
//! - **Each term** is bounded by the absolute values of what it was made from, per
//!   operation (`gamma(k)` times the operands' absolute sizes): a rule's value, a sample's
//!   product, a shift's binomial terms. The integrands report the rounding of the points,
//!   normals and pcurves they read (see `face`).
//!
//! It is a forward error bound to first order in `u`, with the evaluations of curves and
//! surfaces modelled as accurate to a few roundings of their results' size (see `face`).

/// The unit roundoff of `f64`, `2^-53`.
pub(crate) const U: f64 = f64::EPSILON / 2.0;

/// `gamma(n) = n u / (1 - n u)`: the bound on `n` roundings compounded.
pub(crate) const fn gamma(n: u32) -> f64 {
    let nu = n as f64 * U;
    nu / (1.0 - nu)
}

/// The larger of `a` and `b`, for the running peaks of samples: a plain comparison, which
/// compiles to one instruction where `f64::max`'s handling of NaN does not. A NaN `a` is
/// dropped; the samples' NaN are caught by the sums they also go into.
pub(crate) fn larger(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

/// A compensated sum (Ogita, Rump and Oishi, "Accurate sum and dot product", SIAM J. Sci.
/// Comput. 26(6), 2005, `Sum2`): each addition's exact error by `TwoSum`, the errors summed
/// apart and added at the end. Its result is within `u |s| + gamma(n - 1)^2 sum |x|` of the
/// exact sum (their Proposition 4.5), which [`Sum::bound`] states.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Sum {
    total: f64,
    carry: f64,
    abs: f64,
    count: u32,
}

impl Sum {
    /// Adds `x`.
    pub(crate) fn add(&mut self, x: f64) {
        let s = self.total + x;
        let z = s - self.total;
        let e = (self.total - (s - z)) + (x - z);
        self.total = s;
        self.carry += e;
        self.abs += x.abs();
        self.count = self.count.saturating_add(1);
    }

    /// The sum.
    pub(crate) fn value(&self) -> f64 {
        self.total + self.carry
    }

    /// The bound on the sum's rounding.
    pub(crate) fn bound(&self) -> f64 {
        let g = gamma(self.count.saturating_sub(1));
        gamma(1) * self.value().abs() + g * g * self.abs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sum that cancels from 1e16 to 1 is exact, and its bound covers it.
    #[test]
    fn a_cancelling_sum_is_exact() {
        let mut s = Sum::default();
        for x in [1e16, 1.0, -1e16, 1e-3, 3.0, -1e-3] {
            s.add(x);
        }
        assert_eq!(s.value(), 4.0);
        // `u |4| + gamma(5)^2 (2e16 + 4)`: a few times `eps` of the result.
        assert!(
            s.bound() <= 4.0 * f64::EPSILON * 4.0 + 1e-14,
            "{}",
            s.bound()
        );
        let mut plain = 0.0;
        for x in [1e16, 1.0, -1e16] {
            plain += x;
        }
        assert_eq!(plain, 0.0, "the plain sum loses the 1");
    }

    #[test]
    fn gamma_is_n_roundings() {
        assert_eq!(gamma(0), 0.0);
        assert!((gamma(2) - 2.0 * U).abs() <= 1e-30);
    }
}
