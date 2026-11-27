// Written for solidcorex. Apache-2.0.
//! The exact rationals against IEEE arithmetic (correct rounding of every operation, the
//! subnormals and the overflow included) and against double-double (two independent
//! exact sums and products).

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::needless_range_loop,
    clippy::unwrap_used,
    clippy::float_cmp
)]

use scx_num::hash::Rng;
use scx_testkit::Rat;
use scx_testkit::dd::Dd;

fn r(x: f64) -> Rat {
    Rat::from_f64(x).unwrap()
}

/// A double with a random sign, mantissa and an exponent in `[lo, hi]`.
fn double(rng: &mut Rng, lo: i32, hi: i32) -> f64 {
    let e = lo + (rng.next_u64() % (hi - lo + 1) as u64) as i32;
    let m = 1.0 + rng.next_f64();
    let x = m * scx_testkit::dd::scale(1.0, e);
    if rng.next_u64() & 1 == 0 { x } else { -x }
}

fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a == 0.0 && b == 0.0)
}

#[test]
fn every_operation_rounds_as_ieee_does() {
    let mut rng = Rng::new(1);
    // Normal ranges, then pairs whose results are subnormal or overflow.
    let ranges = [(-30, 30), (-600, 600), (-1074, -1000), (900, 1023)];
    for (lo, hi) in ranges {
        for _ in 0..3000 {
            let (a, b) = (double(&mut rng, lo, hi), double(&mut rng, lo, hi));
            let (ra, rb) = (r(a), r(b));
            assert!(same((&ra + &rb).to_f64(), a + b), "{a:e} + {b:e}");
            assert!(same((&ra - &rb).to_f64(), a - b), "{a:e} - {b:e}");
            assert!(same((&ra * &rb).to_f64(), a * b), "{a:e} * {b:e}");
            assert!(
                same(ra.checked_div(&rb).unwrap().to_f64(), a / b),
                "{a:e} / {b:e}"
            );
            assert_eq!(ra.cmp(&rb), a.total_cmp(&b), "{a:e} <=> {b:e}");
            assert!(same(ra.to_f64(), a));
        }
    }
    // Subnormal operands.
    for _ in 0..2000 {
        let a = f64::from_bits(rng.next_u64() >> 12);
        let b = double(&mut rng, -60, 60);
        assert!(same((&r(a) * &r(b)).to_f64(), a * b), "{a:e} * {b:e}");
        assert!(same((&r(a) + &r(b * 1e-300)).to_f64(), a + b * 1e-300));
    }
}

#[test]
fn rational_and_double_double_sums_and_products_agree() {
    let mut rng = Rng::new(2);
    for _ in 0..5000 {
        let (a, b) = (double(&mut rng, -40, 40), double(&mut rng, -40, 40));
        let sum = Dd::from(a) + Dd::from(b);
        assert_eq!(&r(sum.hi) + &r(sum.lo), &r(a) + &r(b));
        let prod = Dd::prod(a, b);
        assert_eq!(&r(prod.hi) + &r(prod.lo), &r(a) * &r(b));
        // Three-term: double-double is within 2^-104 relative, the rational exact.
        let c = double(&mut rng, -40, 40);
        let d3 = Dd::prod(a, b) * Dd::from(c);
        let exact = &(&r(a) * &r(b)) * &r(c);
        let err = &(&r(d3.hi) + &r(d3.lo)) - &exact;
        let bound = exact.abs().scale(-100);
        assert!(err.abs() <= bound, "{a:e} {b:e} {c:e}");
    }
}

/// `dd::scale` and `dd::pow2` against the exact `x 2^k`, rounded once: the subnormal
/// results (where repeated scaling would round twice), the largest binade, overflow, and
/// `k` far below -1022.
#[test]
fn scaling_rounds_once() {
    use scx_testkit::dd::{pow2, scale};
    let mut xs = vec![
        1.0,
        1.5,
        2.0 - f64::EPSILON,
        f64::MAX,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
        f64::from_bits(3),
        7.0e-20,
        1e-300,
        1e300,
    ];
    let mut rng = Rng::new(3);
    for _ in 0..30 {
        xs.push(double(&mut rng, -1074, 1023));
    }
    for &x in &xs {
        for k in (-2200..=2200)
            .step_by(3)
            .chain([-1023, -1022, -1074, -1075, 1023, 1024])
        {
            let want = r(x).scale(i64::from(k)).to_f64();
            let got = scale(x, k);
            assert!(
                same(got, want),
                "scale({x:e}, {k}) = {got:e}, exact {want:e}"
            );
        }
    }
    for k in -1100..=1100 {
        assert!(
            same(pow2(k), r(1.0).scale(i64::from(k)).to_f64()),
            "pow2({k})"
        );
    }
    // 7e-20 2^-1002: two roundings gave 1.63e-321 here.
    assert_eq!(scale(7.0e-20, -1002), r(7.0e-20).scale(-1002).to_f64());
}
