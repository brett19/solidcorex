// Written for solidcorex. Apache-2.0.
//! Error-free transformations: a sum or product of two doubles as the rounded result and
//! its exact rounding error, `a ∘ b = s + e` exactly.
//!
//! This is the one module where `f64::mul_add` is allowed (`docs/01-architecture.md`
//! §8.3, `docs/07-conventions.md` §7 rule 4): Rust's `mul_add` is a single correctly
//! rounded fused multiply-add on every target (a library `fma` where the hardware has
//! none), so [`two_prod`] gives the same bits everywhere. Rust never fuses `a * b + c` on
//! its own, so plain arithmetic elsewhere is unfused.
//!
//! Depends on nothing else in the crate, so it can move to a crate of its own.
//!
//! Each result is exact when no intermediate overflows and, for [`two_prod`], when the
//! error term does not underflow (`|a * b| > 2^-969` is enough).

/// Knuth's two-sum: `(s, e)` with `s = fl(a + b)` and `a + b = s + e` exactly, for any
/// finite `a` and `b` whose sum does not overflow.
#[inline]
#[must_use]
pub fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    let e = (a - (s - bb)) + (b - bb);
    (s, e)
}

/// Dekker's fast two-sum: as [`two_sum`] with three operations instead of six, valid when
/// `a` is zero or the exponent of `a` is at least that of `b` (`|a| >= |b|` is enough).
#[inline]
#[must_use]
pub fn fast_two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let e = b - (s - a);
    (s, e)
}

/// Two-product: `(p, e)` with `p = fl(a * b)` and `a * b = p + e` exactly, from one fused
/// multiply-add.
#[inline]
#[must_use]
#[expect(
    clippy::disallowed_methods,
    reason = "eft is the one home of mul_add (01 §8.3)"
)]
pub fn two_prod(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    let e = a.mul_add(b, -p);
    (p, e)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `2^k` for a normal power of two.
    fn p2(k: i32) -> f64 {
        f64::from_bits(((1023 + k) as u64) << 52)
    }

    /// Exact rational check through integers: every value here is a small dyadic.
    fn exact_sum(a: f64, b: f64, s: f64, e: f64) -> bool {
        // Scale to integers: all test values are multiples of 2^-80 below 2^60.
        let k = p2(80);
        let to = |x: f64| (x * k) as i128;
        to(a) + to(b) == to(s) + to(e)
    }

    #[test]
    fn two_sum_is_exact() {
        let cases = [
            (1.0, p2(-60)),
            (p2(-60), 1.0),
            (1.0 + p2(-52), -1.0),
            (3.0, p2(-53)),
            (-7.5, p2(-55) * 3.0),
        ];
        for (a, b) in cases {
            let (s, e) = two_sum(a, b);
            assert_eq!(s, a + b);
            assert!(exact_sum(a, b, s, e), "{a} {b}");
            if a.abs() >= b.abs() {
                assert_eq!(fast_two_sum(a, b), (s, e));
            }
        }
        assert_eq!(fast_two_sum(0.0, p2(-70)), (p2(-70), 0.0));
    }

    #[test]
    fn two_prod_is_exact() {
        // (1 + 2^-30)^2 = 1 + 2^-29 + 2^-60: the last term is the error.
        let a = 1.0 + p2(-30);
        let (p, e) = two_prod(a, a);
        assert_eq!(p, 1.0 + p2(-29));
        assert_eq!(e, p2(-60));
        // (2^27 + 1)(2^27 - 1) = 2^54 - 1: rounds to 2^54, error -1.
        let (p, e) = two_prod(p2(27) + 1.0, p2(27) - 1.0);
        assert_eq!((p, e), (p2(54), -1.0));
        let (p, e) = two_prod(0.1, 0.1);
        assert_eq!(p, 0.1 * 0.1);
        assert!(e != 0.0 && e.abs() <= p * p2(-53));
    }
}
