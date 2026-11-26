// Written for solidcorex. Apache-2.0.
//! Differential tests: every public scx-num and scx-algo function (interval modules in `t_interval`) against its counterpart in the
//! pinned solidcore, on the same inputs. scx's result must be the same to the bit when run
//! twice, and must agree with solidcore's within `harness::Tol` (same shape, floats close).
//! Each test prints `difftest <name>: <n> inputs, <e> exact, <w> within tolerance,
//! <m> mismatches, <d> nondeterministic`.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::needless_range_loop,
    clippy::unwrap_used
)]

mod facade;
mod harness;
mod solvers;
mod t_algo;
mod t_extent;
mod t_hash;
mod t_interval;
mod t_kraw;
mod t_math;
mod t_rest;
mod t_rng;
mod t_solve;
mod t_tol;
mod t_vector;

/// The harness itself: a different function, a nondeterministic one and a change in shape
/// must each fail; a rounding error of drift must pass; the fingerprint stays bit-exact.
mod controls {
    use crate::harness::{run, shot, Tol};
    use std::sync::atomic::{AtomicU64, Ordering};

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
    #[should_panic(expected = "not deterministic")]
    fn a_nondeterministic_function_is_caught() {
        static N: AtomicU64 = AtomicU64::new(0);
        run::<(f64,), _, _>(
            "control_nondet",
            |(a,)| a + N.fetch_add(1, Ordering::Relaxed) as f64,
            |(a,)| a,
        );
    }

    #[test]
    fn one_rounding_of_drift_agrees_and_a_real_change_does_not() {
        let t = Tol::DEFAULT;
        assert!(t.agree(&shot(|| 1.0f64), &shot(|| 1.0f64 + f64::EPSILON)));
        assert!(t.agree(&shot(|| 0.0f64), &shot(|| -0.0f64)));
        assert!(t.agree(&shot(|| f64::NAN), &shot(|| -f64::NAN)));
        assert!(!t.agree(&shot(|| 1.0f64), &shot(|| 1.0f64 + 1e-9)));
        assert!(!t.agree(&shot(|| f64::INFINITY), &shot(|| f64::MAX)));
        assert!(!t.agree(&shot(|| Some(1.0f64)), &shot(|| -> Option<f64> { None })));
        assert!(!t.agree(&shot(|| 1usize), &shot(|| 2usize)));
        assert!(!t.agree(&shot(|| 1.0f64), &shot(|| -> f64 { panic!("x") })));
    }

    #[test]
    fn the_fingerprint_is_bit_exact() {
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
