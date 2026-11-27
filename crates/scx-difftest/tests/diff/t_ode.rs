// Written for solidcorex. Apache-2.0.
//! `scx-algo`: `ode.rs` (the pin's `mass/integrator.rs`, its ODE core).
//!
//! Every run compares the whole integration: the values and step counts, or the error
//! (`odes::Fp`: class, operation, typed reason, text, status and cause), and the
//! integrand's log (each point evaluated, in order, with the values it was given and
//! whether that call failed). The pin's private helpers (the Cash–Karp stages, the error
//! estimates and their rounding bound, the scale floors) are compared through every step
//! that reads them.
//!
//! The `*_clean` tests run every configuration with an integrand that never fails; the
//! `*_fail` tests take fewer, count the clean run's calls, and repeat it failing at each
//! call in turn (the step start, each stage, the retries), once and from there on, with
//! an error of each class (`odes::OCLASSES`). The `usize` argument splits the
//! configurations into 13 shares.
//!
//! The `truth_*` tests hold scx alone to closed-form integrals, to the accuracy's
//! promise, and the weights to their exact rational values.

use crate::diff;
use crate::odes::{Cfg, OClass, Rec, FUNCS, OCLASSES, QUADRATURES};
use scx_num::math::EFF_ZERO_SQ;

/// Calls swept per configuration.
const SWEEP: usize = 40;

const INTERVALS: [(f64, f64); 8] = [
    (0.0, 1.0),
    (1.0, 0.0),
    (0.0, 2.0),
    (-0.1, 0.3),
    (0.3, -0.1),
    (-1.0, 1.0),
    (0.0, 4.0),
    (2.0, 2.0),
];

fn base<const N: usize>() -> Cfg<N> {
    Cfg {
        accuracy: 1e-6,
        initial_step: 0.1,
        minimum_step: 0.0,
        scale: None,
        quadrature: false,
        rounding: None,
        rate: None,
        max_steps: 10_000,
        tiny: EFF_ZERO_SQ,
        x1: 0.0,
        x2: 1.0,
    }
}

/// The integrations of `N` components: each system over each interval, under each
/// variation of the controls, then the edge controls on two systems.
fn cfgs<const N: usize>(full: bool) -> Vec<(usize, Cfg<N>)> {
    let mut out = Vec::new();
    let b = base::<N>();
    let variations: Vec<Cfg<N>> = {
        let mut v = vec![
            b,
            Cfg {
                accuracy: 1e-10,
                ..b
            },
            Cfg {
                accuracy: 1e-3,
                initial_step: 1.0,
                ..b
            },
            Cfg {
                quadrature: true,
                ..b
            },
            Cfg {
                rate: Some(0.2),
                ..b
            },
            Cfg {
                rounding: Some([1e-16; N]),
                quadrature: true,
                ..b
            },
            Cfg {
                rounding: Some([1e-9; N]),
                accuracy: 1e-12,
                rate: Some(1.0),
                quadrature: true,
                ..b
            },
            Cfg {
                scale: Some([1.0; N]),
                initial_step: -0.1,
                ..b
            },
        ];
        if full {
            v.extend([
                Cfg {
                    minimum_step: 0.05,
                    accuracy: 1e-10,
                    ..b
                },
                Cfg { max_steps: 3, ..b },
                Cfg {
                    tiny: 1.0,
                    accuracy: 1e-8,
                    ..b
                },
                Cfg {
                    scale: Some([f64::INFINITY; N]),
                    ..b
                },
                Cfg {
                    accuracy: f64::INFINITY,
                    ..b
                },
                Cfg {
                    accuracy: 1e-300,
                    ..b
                },
            ]);
        }
        v
    };
    for func in 0..FUNCS {
        let ivls: &[(f64, f64)] = if full { &INTERVALS } else { &INTERVALS[..4] };
        for &(x1, x2) in ivls {
            for c in &variations {
                out.push((func, Cfg { x1, x2, ..*c }));
            }
        }
    }
    if full {
        // the controls refused, the early return's quirks, and each check's order
        for func in [0, 4] {
            for c in [
                Cfg { accuracy: 0.0, ..b },
                Cfg {
                    accuracy: -1e-6,
                    ..b
                },
                Cfg {
                    accuracy: f64::NAN,
                    ..b
                },
                Cfg {
                    initial_step: 1e-12,
                    accuracy: f64::NAN,
                    ..b
                },
                Cfg {
                    initial_step: f64::NAN,
                    ..b
                },
                Cfg {
                    minimum_step: f64::INFINITY,
                    ..b
                },
                Cfg {
                    x2: f64::INFINITY,
                    ..b
                },
                Cfg {
                    x1: 1.0,
                    x2: 1.0 + 1e-14,
                    ..b
                },
                Cfg {
                    scale: Some([f64::NAN; N]),
                    ..b
                },
                Cfg {
                    rounding: Some([-1.0; N]),
                    ..b
                },
                Cfg {
                    rounding: Some([f64::INFINITY; N]),
                    ..b
                },
                Cfg {
                    rate: Some(-1.0),
                    ..b
                },
                Cfg {
                    rate: Some(f64::NAN),
                    ..b
                },
                Cfg {
                    rate: Some(f64::INFINITY),
                    ..b
                },
                Cfg {
                    max_steps: 1_000_001,
                    ..b
                },
                Cfg { max_steps: 0, ..b },
                Cfg {
                    tiny: f64::NAN,
                    x2: 0.0,
                    ..b
                },
                Cfg {
                    tiny: f64::INFINITY,
                    ..b
                },
                Cfg {
                    rounding: Some([1e-3; N]),
                    accuracy: 1e-9,
                    ..b
                },
            ] {
                out.push((func, c));
            }
        }
    }
    out
}

/// `diff!` on a run whose step sizes take `powf`: where the platform's last bit moves the
/// pin's steps, the truth is the pin's own integrator with `fmath::powf`
/// (`pin_truths::ode_powf`).
macro_rules! ode_diff {
    ($name:ident, |$($a:ident : $t:ty),*| $body:expr) => {
        mod $name {
            #[allow(unused_imports)]
            use super::*;
            pub fn defect() -> crate::harness::TruthDefect<($($t,)*)> {
                crate::pin_truths::ode_powf(|args: &($($t,)*)| {
                    let ($($a,)*) = *args;
                    #[allow(unused_imports)]
                    use crate::odes::pinf as m;
                    crate::harness::want($body)
                })
            }
        }
        diff!($name, truth_defect = $name::defect, |$($a: $t),*| $body);
    };
}

/// One integration of `N` components under `m`: (outcome, log).
macro_rules! run_ode {
    ($n:literal, $func:expr, $c:expr, $k:expr, $persist:expr, $class:expr) => {{
        let mut rec = Rec::new($func, $k, $persist, $class);
        let r = m::od::integrate::<$n>(&$c, &mut rec);
        (r, rec.log)
    }};
}

macro_rules! ode_tests {
    ($clean:ident, $fail:ident, $n:literal) => {
        ode_diff!($clean, |share: usize| {
            let mut out = Vec::new();
            for (i, (func, c)) in cfgs::<$n>(true).into_iter().enumerate() {
                if i % 13 == share {
                    out.push(run_ode!($n, func, c, usize::MAX, false, OClass::SmErr));
                }
            }
            out
        });

        ode_diff!($fail, |share: usize| {
            let mut out = Vec::new();
            for (i, (func, c)) in cfgs::<$n>(false).into_iter().enumerate() {
                // the failing systems and the step-control ones fail by themselves
                if i % 13 != share || func >= 13 || i % 3 != 0 {
                    continue;
                }
                let clean = run_ode!($n, func, c, usize::MAX, false, OClass::SmErr);
                let calls = clean.1.len().min(SWEEP);
                out.push(clean);
                for k in 0..calls {
                    for persist in [false, true] {
                        for class in OCLASSES {
                            out.push(run_ode!($n, func, c, k, persist, class));
                        }
                    }
                }
            }
            out
        });
    };
}

ode_tests!(ode1_clean, ode1_fail, 1);
ode_tests!(ode2_clean, ode2_fail, 2);

// More components than the integrator takes are refused before anything is read.
diff!(ode_too_many_components, || {
    let mut rec = Rec::clean(0);
    let r = m::od::integrate::<4097>(&base::<4097>(), &mut rec);
    (r.map(|(_, counts)| counts), rec.log)
});

// One step from arbitrary values, derivatives, positions and trial steps, at three
// accuracies, on a quadrature, a system and the failing systems: the step's validation,
// retries and refusals.
diff!(ode_step, |y: f64, d: f64, x: f64, h: f64| {
    let mut out = Vec::new();
    for func in [4, 9, 11, 13] {
        for accuracy in [1e-6, 1e-12, 0.0] {
            let mut rec = Rec::clean(func);
            let r = m::od::step::<2>([y, 0.5], [d, -d], x, h, accuracy, [1.0, y], &mut rec);
            out.push((r, rec.log));
        }
    }
    out
});

// A step, failing at each call: the six stages.
diff!(ode_step_fail, |share: usize| {
    let mut out = Vec::new();
    for (i, (func, h)) in [(4, 0.1), (9, 0.5), (11, 2.0), (7, 1.0)]
        .into_iter()
        .enumerate()
    {
        if i % 13 != share % 4 {
            continue;
        }
        for k in 0..SWEEP {
            for persist in [false, true] {
                for class in OCLASSES {
                    let mut rec = Rec::new(func, k, persist, class);
                    let r = m::od::step::<1>([0.0], [1.0], 0.0, h, 1e-9, [1.0], &mut rec);
                    out.push((r, rec.log));
                }
            }
        }
    }
    out
});

/// A scalar integration: system, interval, accuracy, rounding bound and rate.
type QCfg = (usize, f64, f64, f64, Option<f64>, Option<f64>);

/// The scalar integrations: each quadrature system over each interval, under each set
/// of controls (accuracy, rounding bound, rate).
fn qcfgs(full: bool) -> Vec<QCfg> {
    let mut out = Vec::new();
    let controls: &[(f64, Option<f64>, Option<f64>)] = if full {
        &[
            (1e-6, None, None),
            (1e-10, None, None),
            (1e-3, None, None),
            (1e-6, Some(1e-16), None),
            (1e-6, None, Some(0.5)),
            (1e-9, Some(1e-15), Some(2.0)),
            (1e-12, Some(1e-9), None),
            (0.0, None, None),
            (1e-300, None, None),
            (f64::NAN, None, None),
            (1e-6, Some(-1.0), None),
            (1e-6, None, Some(-1.0)),
        ]
    } else {
        &[(1e-6, None, None), (1e-9, Some(1e-15), Some(2.0))]
    };
    let ivls: &[(f64, f64)] = &[
        (0.0, 1.0),
        (1.0, 0.0),
        (0.0, 2.0),
        (-0.1, 0.3),
        (-1.0, 1.0),
        (0.0, 4.0),
        (0.0, scx_num::math::PI),
        (2.0, 2.0),
        (2.0, 2.0 + 5e-13),
        (2.0, 2.0 + 1.5e-12),
        (2.0, 2.0 + 1e-6),
    ];
    for &func in &QUADRATURES {
        for &(a, b) in ivls {
            for &(acc, rounding, rate) in controls {
                out.push((func, a, b, acc, rounding, rate));
            }
        }
    }
    out
}

macro_rules! run_quad {
    ($q:expr, $k:expr, $persist:expr, $class:expr) => {{
        let (func, a, b, acc, rounding, rate) = $q;
        let mut rec = Rec::new(func, $k, $persist, $class);
        let r = m::od::quadrature(a, b, acc, rounding, rate, &mut rec);
        (r, rec.log)
    }};
}

ode_diff!(quad_clean, |share: usize| {
    let mut out = Vec::new();
    for (i, q) in qcfgs(true).into_iter().enumerate() {
        if i % 13 == share {
            out.push(run_quad!(q, usize::MAX, false, OClass::SmErr));
        }
    }
    out
});

ode_diff!(quad_fail, |share: usize| {
    let mut out = Vec::new();
    for (i, q) in qcfgs(false).into_iter().enumerate() {
        if i % 13 != share || q.0 >= 13 {
            continue;
        }
        let clean = run_quad!(q, usize::MAX, false, OClass::SmErr);
        let calls = clean.1.len().min(SWEEP);
        out.push(clean);
        for k in 0..calls {
            for persist in [false, true] {
                for class in OCLASSES {
                    out.push(run_quad!(q, k, persist, class));
                }
            }
        }
    }
    out
});

// Arbitrary intervals (the pool's edge values among them) on two systems.
ode_diff!(quad_intervals, |a: f64, b: f64| {
    let mut out = Vec::new();
    for func in [2, 4] {
        out.push(run_quad!(
            (func, a, b, 1e-6, None, None),
            usize::MAX,
            false,
            OClass::SmErr
        ));
    }
    out
});

/// The truths: scx alone, against closed forms and exact rationals.
mod truths {
    use crate::odes::{x as od, OErr, Rec};
    use scx_algo::error::{EvalError, SolveError, SolveFailure, StepControlFailure};
    use scx_algo::ode::{
        self, OdeIntegrator, Quadrature, StepEstimate, DIVIDED_DIFFERENCE_5,
        EMBEDDED_ERROR_WEIGHTS, FIFTH_ORDER_ERROR_ON_S5, OP, OP_INTEGRAND, OP_STEP_CONTROL,
        QUADRATURE_ERROR_WEIGHTS,
    };
    use scx_num::fmath;
    use scx_num::math::PI;

    type Integrand = fn(f64) -> f64;
    /// A named integral: its name, interval, integrand and value.
    type Case = (&'static str, f64, f64, Integrand, f64);

    /// `∫_a^b g` by the scalar integrator, at `accuracy`.
    fn quad(a: f64, b: f64, accuracy: f64, g: impl Fn(f64) -> f64) -> f64 {
        Quadrature::new(accuracy)
            .integrate(a, b, |t| Ok::<_, OErr>(g(t)))
            .map_err(|e| format!("{a}..{b} at {accuracy:e}: {e:?}"))
            .unwrap()
    }

    /// `|got - want| <= k accuracy max(1, |want|)`.
    fn close(got: f64, want: f64, accuracy: f64, k: f64, what: &str) {
        let err = (got - want).abs();
        assert!(
            err <= k * accuracy * want.abs().max(1.0),
            "{what}: {got} vs {want} (error {err:e})"
        );
    }

    #[test]
    fn truth_closed_form_quadratures() {
        let e4 = fmath::exp(4.0);
        let cases: [Case; 11] = [
            ("1", 0.0, 1.0, |_| 1.0, 1.0),
            ("3t^2", 0.0, 2.0, |t| 3.0 * t * t, 8.0),
            ("3t^2 reversed", 2.0, 0.0, |t| 3.0 * t * t, -8.0),
            ("t^4", 0.0, 1.0, |t| t * t * t * t, 0.2),
            (
                "t^7 - t^3",
                -1.0,
                2.0,
                |t| fmath::powi(t, 7) - t * t * t,
                255.0 / 8.0 - 15.0 / 4.0,
            ),
            ("e^t", 0.0, 4.0, fmath::exp, e4 - 1.0),
            ("sin", 0.0, PI, fmath::sin, 2.0),
            ("1/(1+t^2)", -1.0, 1.0, |t| 1.0 / (1.0 + t * t), PI / 2.0),
            (
                "Runge",
                -1.0,
                1.0,
                |t| 1.0 / (1.0 + 25.0 * t * t),
                0.4 * fmath::atan(5.0),
            ),
            (
                "2 + sin 20t",
                0.0,
                PI,
                |t| 2.0 + fmath::sin(20.0 * t),
                2.0 * PI,
            ),
            ("e^-t", 0.0, 3.0, |t| fmath::exp(-t), 1.0 - fmath::exp(-3.0)),
        ];
        for accuracy in [1e-6, 1e-9, 1e-12] {
            for (what, a, b, g, want) in cases {
                // (the starts where the integrand vanishes: `truth_a_vanishing_start_gives_up`)
                let gives_up = match what {
                    "t^7 - t^3" => true,
                    "t^4" | "sin" => accuracy < 1e-6,
                    _ => false,
                };
                if gives_up {
                    unachievable(
                        &Quadrature::new(accuracy).integrate(a, b, |t| Ok::<_, OErr>(g(t))),
                        what,
                    );
                } else {
                    close(quad(a, b, accuracy, g), want, accuracy, 10.0, what);
                }
            }
        }
    }

    /// `r` is the step control's refusal of the accuracy.
    fn unachievable<T: std::fmt::Debug>(r: &Result<T, SolveError<OErr>>, what: &str) {
        assert!(
            matches!(
                r,
                Err(SolveError::Failed {
                    failure: SolveFailure::StepControl(StepControlFailure::Unachievable),
                    ..
                })
            ),
            "{what}: {r:?}"
        );
    }

    /// Inherited from the pin (and kept, for a ruling; report 53 section 13): where the
    /// values and the integrand both vanish at the start, the error scale is its floor
    /// `EFF_ZERO`, so the guard `accuracy <= (estimate / scale) EFF_ZERO`, meant for an
    /// accuracy below the values' rounding, reads `accuracy <= estimate`: the integration
    /// gives up on the first trial step whose error estimate reaches the accuracy instead
    /// of shrinking it. A start just past the zero integrates.
    #[test]
    fn truth_a_vanishing_start_gives_up() {
        let g = |t: f64| fmath::powi(t, 7) - t * t * t;
        let big = |t: f64| fmath::powi(t, 8) / 8.0 - fmath::powi(t, 4) / 4.0;
        for accuracy in [1e-6, 1e-9] {
            // g(-1) = 0
            unachievable(
                &Quadrature::new(accuracy).integrate(-1.0, 2.0, |t| Ok::<_, OErr>(g(t))),
                "from -1",
            );
            let want = big(2.0) - big(-0.999);
            close(
                quad(-0.999, 2.0, accuracy, g),
                want,
                accuracy,
                10.0,
                "from -0.999",
            );
        }
        unachievable(
            &Quadrature::new(1e-9).integrate(0.0, PI, |t| Ok::<_, OErr>(fmath::sin(t))),
            "sin from 0",
        );
        let want = 1.0 + fmath::cos(0.01);
        close(
            quad(0.01, PI, 1e-9, fmath::sin),
            want,
            1e-9,
            10.0,
            "sin from 0.01",
        );
    }

    #[test]
    fn truth_closed_form_systems() {
        let ode = |accuracy| OdeIntegrator::<2>::new(accuracy, 0.1, 0.0);
        // (y1 and y1' vanish at the start of both: `truth_a_vanishing_start_gives_up`; the
        // tighter accuracies give up)
        for accuracy in [1e-6, 1e-9, 1e-11] {
            // y0' = y0 + 1, y1' = -y0: y0 = e^x - 1, y1 = x + 1 - e^x
            let x2 = 2.0;
            let r = ode(accuracy).integrate(0.0, x2, |_, y: &[f64; 2]| {
                Ok::<_, OErr>([y[0] + 1.0, -y[0]])
            });
            if accuracy < 1e-6 {
                unachievable(&r, "e^x - 1");
            } else {
                let s = r.unwrap();
                let e = fmath::exp(x2);
                close(s.values[0], e - 1.0, accuracy, 20.0, "e^x - 1");
                close(s.values[1], x2 + 1.0 - e, accuracy, 20.0, "x + 1 - e^x");
            }
            // y0' = y1 + 1, y1' = -y0: y0 = sin x, y1 = cos x - 1, over two turns
            let x2 = 4.0 * PI;
            let r = ode(accuracy).integrate(0.0, x2, |_, y: &[f64; 2]| {
                Ok::<_, OErr>([y[1] + 1.0, -y[0]])
            });
            if accuracy < 1e-9 {
                unachievable(&r, "sin x");
            } else {
                let s = r.unwrap();
                close(s.values[0], fmath::sin(x2), accuracy, 200.0, "sin x");
                close(
                    s.values[1],
                    fmath::cos(x2) - 1.0,
                    accuracy,
                    200.0,
                    "cos x - 1",
                );
            }
        }
    }

    /// Stiff-ish: `y' = -50 (y - cos x)`, `y(0) = 0`, whose solution is
    /// `(2500 cos x + 50 sin x - 2500 e^(-50 x)) / 2501`.
    #[test]
    fn truth_stiff_relaxation() {
        for (x2, accuracy) in [(0.1, 1e-8), (1.0, 1e-8), (3.0, 1e-6), (3.0, 1e-10)] {
            let s = OdeIntegrator::<1>::new(accuracy, 0.1, 0.0)
                .integrate(0.0, x2, |x, y: &[f64; 1]| {
                    Ok::<_, OErr>([-50.0 * (y[0] - fmath::cos(x))])
                })
                .unwrap();
            let want = (2500.0 * fmath::cos(x2) + 50.0 * fmath::sin(x2)
                - 2500.0 * fmath::exp(-50.0 * x2))
                / 2501.0;
            close(s.values[0], want, accuracy, 50.0, "relaxation");
            // the step control limits the steps to the explicit scheme's stability
            assert!(s.counts.good + s.counts.reduced >= (x2 * 10.0) as u32);
        }
    }

    /// With a rate, a range cut into pieces is held to the accuracy of the whole
    /// (`OdeIntegrator::rate`): the pieces' errors add up to at most about the accuracy
    /// relative to the whole integral, where each piece alone is held to its own.
    #[test]
    fn truth_a_rate_holds_pieces_to_the_whole() {
        let cases: [(f64, f64, Integrand, f64); 3] = [
            (0.0, 4.0, fmath::exp, fmath::exp(4.0) - 1.0),
            (
                -1.0,
                1.0,
                |t| 1.0 / (1.0 + 25.0 * t * t),
                0.4 * fmath::atan(5.0),
            ),
            (0.0, PI, |t| 2.0 + fmath::sin(20.0 * t), 2.0 * PI),
        ];
        let accuracy = 1e-7;
        for (a, b, g, want) in cases {
            let rate = want / (b - a);
            for n in [1u32, 8, 64] {
                let mut sum = 0.0;
                for k in 0..n {
                    let lo = a + (b - a) * f64::from(k) / f64::from(n);
                    let hi = a + (b - a) * f64::from(k + 1) / f64::from(n);
                    let q = Quadrature {
                        rate: Some(rate),
                        ..Quadrature::new(accuracy)
                    };
                    sum += q.integrate(lo, hi, |t| Ok::<_, OErr>(g(t))).unwrap();
                }
                close(sum, want, accuracy, 4.0, "pieces");
            }
        }
    }

    /// `t^5 + a t^4` from 1/2 to 3/2, in one trial step: the embedded estimate reads only
    /// `g''''` and is blind to the `t^5` term's error; the quadrature estimate is not, and
    /// holds the result to the accuracy.
    #[test]
    fn truth_quadrature_estimate_sees_the_fifth_derivative() {
        for a in [-2.5, -1.0, 0.0, 3.0] {
            let g = move |t: f64| t * t * t * t * t + a * t * t * t * t;
            let big = |t: f64| fmath::powi(t, 6) / 6.0 + a * fmath::powi(t, 5) / 5.0;
            let ode = OdeIntegrator {
                estimate: StepEstimate::Quadrature,
                ..OdeIntegrator::<1>::new(1e-9, 1.0, 0.0)
            };
            let s = ode
                .integrate(0.5, 1.5, |t, _| Ok::<_, OErr>([g(t)]))
                .unwrap();
            close(
                s.values[0],
                big(1.5) - big(0.5),
                1e-9,
                10.0,
                "quadrature estimate",
            );
        }
    }

    /// The last node is the interval's end exactly, in both directions.
    #[test]
    fn truth_the_final_step_ends_at_the_interval_end() {
        for (a, b) in [(-0.1, 0.3), (0.3, -0.1), (0.0, 0.7), (1.0, 1.3)] {
            let mut rec = Rec::clean(0);
            od::quadrature(a, b, 1e-6, None, None, &mut rec).unwrap();
            let last = rec.log.iter().map(|l| l.0).filter(|&x| x == b).count();
            assert!(last > 0, "{a}..{b}: no node at the end");
            assert!(
                rec.log.iter().all(|l| (l.0 - a) * (l.0 - b) <= 0.0),
                "{a}..{b}: a node outside the interval"
            );
        }
    }

    /// The failures name their causes.
    #[test]
    fn truth_failures_name_their_causes() {
        let q = Quadrature::new(1e-6);
        let r = q.integrate(0.0, 1.0, |t| {
            Ok::<_, OErr>(if t > 0.5 { f64::NAN } else { 1.0 })
        });
        assert!(matches!(
            r,
            Err(SolveError::Failed {
                operation: OP_INTEGRAND,
                failure: SolveFailure::IntegrandNotFinite
            })
        ));
        let r =
            Quadrature::new(1e-300).integrate(0.0, 1.0, |t| Ok::<_, OErr>(fmath::sin(10.0 * t)));
        assert!(matches!(
            r,
            Err(SolveError::Failed {
                operation: OP_STEP_CONTROL,
                failure: SolveFailure::StepControl(_)
            })
        ));
        for accuracy in [0.0, -1.0, -1e-6] {
            let r = Quadrature::new(accuracy).integrate(0.0, 1.0, |_| Ok::<_, OErr>(1.0));
            assert_eq!(
                r.err().map(|e| matches!(
                    e,
                    SolveError::Failed {
                        failure: SolveFailure::StepControl(StepControlFailure::Unachievable),
                        ..
                    }
                )),
                Some(true),
                "accuracy {accuracy}"
            );
        }
        let r = Quadrature::new(f64::NAN).integrate(0.0, 1.0, |_| Ok::<_, OErr>(1.0));
        assert!(matches!(
            r,
            Err(SolveError::InvalidInput { operation: OP, .. })
        ));
        // a step budget too small, a minimum step too large
        let ode = OdeIntegrator {
            max_steps: 3,
            ..OdeIntegrator::<1>::new(1e-9, 0.01, 0.0)
        };
        let r = ode.integrate(0.0, 1.0, |t, _| Ok::<_, OErr>([fmath::exp(t)]));
        assert!(matches!(
            r,
            Err(SolveError::Failed {
                failure: SolveFailure::StepControl(StepControlFailure::StepsExhausted),
                ..
            })
        ));
        let ode = OdeIntegrator::<1>::new(1e-12, 0.1, 0.05);
        let r = ode.integrate(0.0, 4.0, |t, _| Ok::<_, OErr>([fmath::exp(t)]));
        assert!(matches!(
            r,
            Err(SolveError::Failed {
                failure: SolveFailure::StepControl(StepControlFailure::BelowMinimumStep),
                ..
            })
        ));
        // rounding above what the accuracy allows: the owed error is refused
        let q = Quadrature {
            rounding: Some(1e-3),
            ..Quadrature::new(1e-9)
        };
        let r = q.integrate(0.0, 1.0, Ok::<_, OErr>);
        assert!(matches!(
            r,
            Err(SolveError::Failed {
                failure: SolveFailure::StepControl(StepControlFailure::ErrorOwed),
                ..
            })
        ));
        // the integrand's own error: unchanged at the start, wrapped inside a step, and a
        // nested integration's failure passed through unchanged
        let fail = || {
            OErr(solidcore::SolidError::Upstream {
                operation: "test",
                code: 1,
                entity: None,
            })
        };
        let r = q.integrate(0.0, 1.0, |_| Err::<f64, _>(fail()));
        assert!(matches!(r, Err(SolveError::Eval(_))));
        let r =
            Quadrature::new(1e-6).integrate(
                0.0,
                1.0,
                |t| {
                    if t > 0.0 {
                        Err(fail())
                    } else {
                        Ok(1.0)
                    }
                },
            );
        assert!(matches!(
            r,
            Err(SolveError::CallFailed { operation: OP, .. })
        ));
        let nested = OErr(crate::odes::pin_error(crate::odes::OClass::StepControl));
        assert!(nested.passes_through());
    }

    /// Exact rationals `n / d`, reduced (`d > 0`).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Q(i128, i128);
    fn gcd(a: i128, b: i128) -> i128 {
        if b == 0 {
            a.abs()
        } else {
            gcd(b, a % b)
        }
    }
    impl Q {
        fn new(n: i128, d: i128) -> Self {
            let g = gcd(n, d).max(1) * d.signum();
            Q(n / g, d / g)
        }
        fn add(self, o: Q) -> Q {
            Q::new(self.0 * o.1 + o.0 * self.1, self.1 * o.1)
        }
        fn sub(self, o: Q) -> Q {
            self.add(Q(-o.0, o.1))
        }
        fn mul(self, o: Q) -> Q {
            Q::new(self.0 * o.0, self.1 * o.1)
        }
        fn pow(self, n: u32) -> Q {
            (0..n).fold(Q(1, 1), |p, _| p.mul(self))
        }
        /// Within `ulps` units in the last place of `v`.
        fn near(self, v: f64, ulps: f64) -> bool {
            let q = self.0 as f64 / self.1 as f64;
            (q - v).abs() <= ulps * f64::EPSILON * q.abs().max(f64::MIN_POSITIVE)
        }
    }

    /// The Cash–Karp nodes, in sample order.
    const NODES: [Q; 6] = [Q(0, 1), Q(1, 5), Q(3, 10), Q(3, 5), Q(1, 1), Q(7, 8)];

    #[test]
    fn truth_weights_are_their_exact_rationals() {
        // the fifth divided difference: 1 / prod_{j != i} (c_i - c_j)
        for (i, ci) in NODES.iter().enumerate() {
            let prod = NODES
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .fold(Q(1, 1), |p, (_, cj)| p.mul(ci.sub(*cj)));
            let w = Q::new(prod.1, prod.0);
            assert!(w.near(DIVIDED_DIFFERENCE_5[i], 0.5), "D5[{i}]: {w:?}");
        }
        // exact on s^p: zero for p <= 4, one for p = 5
        let d5: Vec<Q> = NODES
            .iter()
            .enumerate()
            .map(|(i, ci)| {
                let prod = NODES
                    .iter()
                    .enumerate()
                    .filter(|&(j, _)| j != i)
                    .fold(Q(1, 1), |p, (_, cj)| p.mul(ci.sub(*cj)));
                Q::new(prod.1, prod.0)
            })
            .collect();
        for p in 0..=5 {
            let s = d5
                .iter()
                .zip(NODES)
                .fold(Q(0, 1), |s, (w, c)| s.add(w.mul(c.pow(p))));
            assert_eq!(s, Q(i128::from(p == 5), 1), "s^{p}");
        }
        // the fifth-order weights' error on s^5
        let b = [
            Q(37, 378),
            Q(0, 1),
            Q(250, 621),
            Q(125, 594),
            Q(0, 1),
            Q(512, 1771),
        ];
        let err = b
            .iter()
            .zip(NODES)
            .fold(Q(0, 1), |s, (w, c)| s.add(w.mul(c.pow(5))))
            .sub(Q(1, 6));
        assert_eq!(err, Q(-1, 960));
        assert!(err.near(FIFTH_ORDER_ERROR_ON_S5, 0.5));
        // the fifth-order weights integrate s^p exactly for p <= 4 (so the embedded
        // estimate, their difference from the fourth-order ones, vanishes on cubics)
        let b4 = [
            Q(2825, 27648),
            Q(0, 1),
            Q(18575, 48384),
            Q(13525, 55296),
            Q(277, 14336),
            Q(1, 4),
        ];
        for p in 0..=4u32 {
            let s = b
                .iter()
                .zip(NODES)
                .fold(Q(0, 1), |s, (w, c)| s.add(w.mul(c.pow(p))));
            assert_eq!(s, Q(1, i128::from(p) + 1), "fifth order on s^{p}");
        }
        for p in 0..=3u32 {
            let s = b4
                .iter()
                .zip(NODES)
                .fold(Q(0, 1), |s, (w, c)| s.add(w.mul(c.pow(p))));
            assert_eq!(s, Q(1, i128::from(p) + 1), "fourth order on s^{p}");
        }
        for (j, (hi, lo)) in b.iter().zip(b4).enumerate() {
            // (each weight is the difference of two rounded quotients: a few ulps)
            let w = hi.sub(lo);
            let tol = 4.0
                * (hi.0 as f64 / hi.1 as f64)
                    .abs()
                    .max(lo.0 as f64 / lo.1 as f64)
                / (w.0 as f64 / w.1 as f64).abs().max(f64::MIN_POSITIVE);
            assert!(
                w.near(EMBEDDED_ERROR_WEIGHTS[j], tol.max(0.5)),
                "E[{j}]: {w:?}"
            );
            let q = d5[j].mul(Q(-1, 960));
            assert!(q.near(QUADRATURE_ERROR_WEIGHTS[j], 2.0), "Q[{j}]: {q:?}");
        }
        let _ = ode::MAX_COMPONENTS;
    }
}
