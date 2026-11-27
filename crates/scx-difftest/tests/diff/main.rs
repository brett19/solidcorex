// Written for solidcorex. Apache-2.0.
//! Differential tests: every public scx-num and scx-algo function (interval modules in `t_interval`, scx-p21 in `t_p21`) against its counterpart in the
//! pinned solidcore, on the same inputs. scx's result must be the same to the bit when run
//! twice, and must agree with solidcore's within `harness::Tol` (same shape, floats close).
//! Each test prints `difftest <name>: <n> inputs, <e> exact, <w> within tolerance,
//! <p> pin defects, <m> mismatches, <d> nondeterministic`; a pin defect is an input where
//! the pin is known wrong and scx passed a check against the true value instead
//! (`pin_defects.rs`).

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::needless_range_loop,
    clippy::unwrap_used
)]

mod facade;
mod harness;
mod pin_defects;
mod pin_extent;
mod pin_truths;
mod solvers;
mod t_algo;
mod t_extent;
mod t_hash;
mod t_interval;
mod t_kraw;
mod t_math;
mod t_nurbs;
mod t_nurbs_surface;
mod t_p21;
mod t_rest;
mod t_rng;
mod t_solve;
mod t_tol;
mod t_vector;

/// The harness itself: a different function, a nondeterministic one and a change in shape
/// must each fail; a rounding error of drift must pass; the fingerprint stays bit-exact; a
/// pin defect excuses only inputs whose check passes, and one that applies to none fails.
mod controls {
    use crate::harness::{run, run_defect, shot, Applies, PinDefect, Tol};
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    #[should_panic(expected = "inputs differ")]
    fn a_different_function_is_caught() {
        run::<(f64, f64), _, _>(
            "control",
            |(a, b)| a.max(b),
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

    /// A "pin" that is wrong for negative inputs, excused where scx returns `|a|`.
    fn wrong_below_zero(check: fn(&(f64,), &f64) -> Result<(), String>) -> PinDefect<(f64,), f64> {
        PinDefect {
            reason: "control",
            applies: Applies::Inputs(|&(a,)| a < 0.0),
            check,
        }
    }

    #[test]
    fn a_pin_defect_excuses_the_inputs_it_checks() {
        run_defect::<(f64,), _, _>(
            "control_defect",
            &wrong_below_zero(|&(a,), &x| {
                if x == a.abs() {
                    Ok(())
                } else {
                    Err("not |a|".to_owned())
                }
            }),
            |(a,)| a.abs(),
            |(a,)| a,
        );
    }

    #[test]
    #[should_panic(expected = "inputs differ")]
    fn a_failed_pin_defect_check_is_caught() {
        run_defect::<(f64,), _, _>(
            "control_defect_check",
            &wrong_below_zero(|_, _| Err("never true".to_owned())),
            |(a,)| a.abs(),
            |(a,)| a,
        );
    }

    #[test]
    #[should_panic(expected = "applies to no input")]
    fn a_stale_pin_defect_is_caught() {
        run_defect::<(f64,), _, _>(
            "control_defect_stale",
            &PinDefect {
                reason: "control",
                applies: Applies::Inputs(|_| false),
                check: |_, _| Ok(()),
            },
            |(a,)| a,
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
