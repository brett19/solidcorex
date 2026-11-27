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

/// Two-product: `(p, e)` with `p = fl(a * b)` and `a * b = p + e` exactly.
///
/// `e` is one `mul_add` where that is a fused multiply-add instruction: with the `fma`
/// target feature, on aarch64, and on x86-64, where the `fma` Rust calls picks the
/// instruction at run time when the processor has it (one indirect call; Dekker's product
/// is shorter in throughput but longer in latency, and the Horner loops that use `two_prod`
/// are latency bound). Elsewhere (wasm32), where `mul_add` is a software `fma` (about 15 ns
/// against 2), it is Dekker's product of Veltkamp halves ([`two_prod_split`]) whenever that
/// is exact, and the `mul_add` otherwise. Both give the exact `a * b - p` (one number) with a
/// zero as `+0`, so the bits do not depend on the path.
#[inline]
#[must_use]
pub fn two_prod(a: f64, b: f64) -> (f64, f64) {
    #[cfg(any(
        target_feature = "fma",
        target_arch = "aarch64",
        target_arch = "x86_64"
    ))]
    {
        two_prod_fma(a, b)
    }
    #[cfg(not(any(
        target_feature = "fma",
        target_arch = "aarch64",
        target_arch = "x86_64"
    )))]
    {
        if split_is_exact(a, b) {
            two_prod_split(a, b)
        } else {
            two_prod_fma(a, b)
        }
    }
}

/// [`two_prod`] by one fused multiply-add.
#[inline]
#[must_use]
#[expect(
    clippy::disallowed_methods,
    reason = "eft is the one home of mul_add (01 §8.3)"
)]
fn two_prod_fma(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    let e = a.mul_add(b, -p);
    (p, e)
}

/// Whether [`two_prod_split`] is exact for `a` and `b`, from their exponent fields `ea`
/// and `eb` (biased by 1023): both normal and below `2^996` (`1 <= e <= 2018`: the
/// split's `(2^27 + 1) x` does not overflow), and `ea + eb` in `[1078, 3065]`, so
/// `|a b| >= 2^-968` (every partial product is a multiple of `ulp(a) ulp(b) >= 2^-1074`,
/// so none rounds) and `|a b| < 2^1021` (none overflows). Zero, subnormal, huge, infinite
/// and NaN operands are not, and go to the `mul_add`.
#[inline]
#[must_use]
#[cfg_attr(
    any(
        target_feature = "fma",
        target_arch = "aarch64",
        target_arch = "x86_64"
    ),
    allow(dead_code)
)]
const fn split_is_exact(a: f64, b: f64) -> bool {
    let ea = (a.to_bits() >> 52) & 0x7ff;
    let eb = (b.to_bits() >> 52) & 0x7ff;
    ea.wrapping_sub(1) < 2018 && eb.wrapping_sub(1) < 2018 && (ea + eb).wrapping_sub(1078) <= 1987
}

/// Veltkamp's split: `(h, l)` with `a = h + l` exactly, each half of at most 26 bits.
#[inline]
fn split(a: f64) -> (f64, f64) {
    let c = 134_217_729.0 * a; // 2^27 + 1
    let h = c - (c - a);
    (h, a - h)
}

/// Dekker's two-product (Ogita, Rump and Oishi's `TwoProduct`): exact where
/// [`split_is_exact`] holds; a zero error is `+0`, as the `mul_add` gives.
#[inline]
#[must_use]
#[cfg_attr(
    any(
        target_feature = "fma",
        target_arch = "aarch64",
        target_arch = "x86_64"
    ),
    allow(dead_code)
)]
fn two_prod_split(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    let e = al * bl - (((p - ah * bh) - al * bh) - ah * bl);
    // `-0 + 0 = +0`; a nonzero `e` is unchanged
    (p, e + 0.0)
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

    /// The split path against the `mul_add` bit for bit, where the split is taken, and
    /// that it is taken across the range (so the fast path is the common one).
    fn same_bits(a: f64, b: f64) {
        let f = two_prod_fma(a, b);
        assert_eq!(two_prod(a, b).0.to_bits(), f.0.to_bits(), "{a:e} {b:e}");
        assert_eq!(two_prod(a, b).1.to_bits(), f.1.to_bits(), "{a:e} {b:e}");
        if split_is_exact(a, b) {
            let s = two_prod_split(a, b);
            assert_eq!(
                (s.0.to_bits(), s.1.to_bits()),
                (f.0.to_bits(), f.1.to_bits()),
                "{a:e} {b:e}: {s:?} against {f:?}"
            );
        }
    }

    #[test]
    fn split_path_matches_the_fma() {
        let mut r = crate::hash::Rng::new(0x5EED_D3C3);
        let mut taken = 0;
        for i in 0..200_000 {
            // random bits: every exponent, sign and mantissa, NaN and infinity included
            let (a, b) = if i % 4 == 0 {
                (f64::from_bits(r.next_u64()), f64::from_bits(r.next_u64()))
            } else if i % 4 == 1 {
                // full mantissas at the ends of the split's range: products near `2^-968`
                // and `2^1021`, factors near `2^995` and near the smallest normal
                let ea = [-1022, -990, -500, 26, 500, 990, 995][(r.next_u64() % 7) as usize];
                let eb = if r.next_u64().is_multiple_of(2) {
                    -968 - ea
                } else {
                    1021 - ea
                };
                let eb = (eb + (r.next_u64() % 9) as i32 - 4).clamp(-1022, 1023);
                (
                    (1.0 + r.next_f64()) * p2(ea),
                    -(1.0 + r.next_f64()) * p2(eb),
                )
            } else {
                // ordinary sizes, where the split is taken
                let e = |r: &mut crate::hash::Rng| (r.next_u64() % 120) as i32 - 60;
                let (ea, eb) = (e(&mut r), e(&mut r));
                (r.next_signed_unit() * p2(ea), r.next_signed_unit() * p2(eb))
            };
            same_bits(a, b);
            taken += usize::from(split_is_exact(a, b));
        }
        assert!(taken > 90_000, "the split was taken {taken} times");
    }

    #[test]
    fn split_path_edges() {
        let ones = [
            1.0,
            1.0 + p2(-52),
            2.0 - p2(-52),
            1.5,
            1.0 + p2(-26),
            1.0 + p2(-27),
        ];
        let mut ks = vec![
            -1074, -1073, -1060, -1023, -1022, -1021, -1000, -970, -969, -968,
        ];
        ks.extend([-967, -600, -500, -27, -26, 0, 26, 27, 500, 600]);
        ks.extend([968, 969, 994, 995, 996, 997, 1000, 1021, 1022, 1023]);
        let at = |m: f64, k: i32| {
            // `m 2^k`, through two steps so the subnormal and top ends are reachable
            m * p2(k / 2) * p2(k - k / 2)
        };
        let mut xs = Vec::new();
        for &k in &ks {
            for &m in &ones {
                let x = at(m, k);
                xs.extend([x, -x, x.next_up(), x.next_down()]);
            }
        }
        xs.extend([
            0.0,
            -0.0,
            f64::MAX,
            f64::MIN_POSITIVE,
            5e-324,
            f64::INFINITY,
        ]);
        xs.extend([f64::NEG_INFINITY, f64::NAN, 0.1, 1.0 / 3.0, 134_217_729.0]);
        for &a in &xs {
            for &b in &xs {
                same_bits(a, b);
            }
        }
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
