// Written for solidcorex. Apache-2.0.
//! Differential tests: every public scx-num function against its counterpart in the
//! pinned solidcore, on the same inputs, compared bit for bit (`f64::to_bits`).
//! Each test prints `difftest <name>: <n> inputs, <m> mismatches`.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::needless_range_loop,
    clippy::unwrap_used
)]

mod facade;
mod harness;
mod t_extent;
mod t_math;
mod t_rest;
mod t_tol;
mod t_vector;

/// The harness itself: a difference in one bit, in a sign of zero, in a NaN's payload or
/// in panicking must each fail the comparison.
mod controls {
    use crate::harness::{run, shot};

    #[test]
    #[should_panic(expected = "inputs differ")]
    fn a_different_function_is_caught() {
        run::<(f64, f64), _, _>(
            "control",
            |(a, b)| scx_num::math::max2(a, b),
            |(a, b)| solidcore::math::min2(a, b),
        );
    }

    #[test]
    fn signed_zero_nan_payload_and_panic_are_told_apart() {
        assert_ne!(shot(|| 0.0f64), shot(|| -0.0f64));
        assert_ne!(shot(|| f64::NAN), shot(|| -f64::NAN));
        assert_ne!(shot(|| 1.0f64), shot(|| 1.0f64 + f64::EPSILON));
        assert_ne!(shot(|| 1.0f64), shot(|| -> f64 { panic!("x") }));
        assert_eq!(
            shot(|| (1.0f64, Some(2.0f64))),
            shot(|| (1.0f64, Some(2.0f64)))
        );
    }
}
