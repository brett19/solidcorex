// Written for solidcorex. Apache-2.0.
//! The local solvers' side of the facade (report 53 section 12): evaluators that record
//! every call and fail, on request, with an error of a chosen class at a chosen call, the
//! same on both sides; and the one-for-one map from `scx_algo::error::SolveError` onto the
//! pin's `SolidError`, through which the difftests compare the solvers' errors.
//!
//! The evaluators' error is the pin's own `SolidError` on both sides (wrapped in [`XErr`]
//! here, so that it can carry `EvalError`), so the injected errors are identical and only
//! the solvers' handling of them is compared.

use crate::harness::{Bits, Out};
use scx_algo::error::{EvalError, SolveError, SolveFailure};
use scx_num::error::NumFailure;
use scx_num::NumError;
use solidcore::error::geom::GeomFailure;
use solidcore::error::Failure;
use solidcore::SolidError;

impl Bits for String {
    fn bits(&self, o: &mut Out) {
        o.tag(self);
    }
}

/// An evaluator error class the pin's solvers tell apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Ends the solve (`ends_solve`).
    Unported,
    /// Ends the solve.
    Cancelled,
    /// Ends the solve.
    OutOfFuel,
    /// Passes through a failed call unchanged, but a solve goes on past it.
    PreparationRequired,
    /// Never ignored, but wrapped as a failed call.
    FaceTrimsUnavailable,
    /// An ordinary failed evaluation (upstream `SM_ERR`).
    SmErr,
    /// An ordinary failed evaluation of another status.
    InvalidInput,
}

/// Every class, in a fixed order.
pub const CLASSES: [Class; 7] = [
    Class::Unported,
    Class::Cancelled,
    Class::OutOfFuel,
    Class::PreparationRequired,
    Class::FaceTrimsUnavailable,
    Class::SmErr,
    Class::InvalidInput,
];

/// The pin's error of class `c`.
pub fn pin_error(c: Class) -> SolidError {
    match c {
        Class::Unported => SolidError::Unported {
            upstream: "SmTest::Evaluate",
            unit: "test.eval",
        },
        Class::Cancelled => SolidError::Cancelled {
            operation: "test::eval",
        },
        Class::OutOfFuel => SolidError::OutOfFuel {
            operation: "test::eval",
            spent: 7,
            limit: 5,
        },
        Class::PreparationRequired => SolidError::PreparationRequired {
            operation: "test::eval",
        },
        Class::FaceTrimsUnavailable => SolidError::FaceTrimsUnavailable {
            operation: "test::eval",
            entity: solidcore::error::EntityRef::Face(solidcore::topology::FaceId::from_index(3)),
        },
        Class::SmErr => SolidError::Upstream {
            operation: "test::eval",
            code: 1001,
            entity: None,
        },
        Class::InvalidInput => SolidError::InvalidInput {
            operation: "test::eval",
            reason: "the test evaluator refuses this parameter",
        },
    }
}

/// The scx side's evaluator error: the pin's error, classified as the pin's solvers
/// classify it (`local_solve1d::eval_failed`, `SolidError::geom_call`, `ignored`).
#[derive(Debug, Clone)]
pub struct XErr(pub SolidError);

impl core::fmt::Display for XErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

impl EvalError for XErr {
    fn ends_solve(&self) -> bool {
        matches!(
            self.0,
            SolidError::Unported { .. } | SolidError::Cancelled { .. } | SolidError::OutOfFuel { .. }
        )
    }
    fn passes_through(&self) -> bool {
        self.ends_solve() || matches!(self.0, SolidError::PreparationRequired { .. })
    }
    fn never_ignored(&self) -> bool {
        self.0.is_unported_or_cancelled() || matches!(self.0, SolidError::PreparationRequired { .. })
    }
    fn defect(operation: &'static str, why: &'static str) -> Self {
        XErr(Failure::new(operation, GeomFailure::Defect { why }).into_error())
    }
}

/// A layer-0 error as the pin reports it (the variants the solvers can reach; any other
/// comes out as a `Defect` and so mismatches).
pub fn num_pin(e: NumError) -> SolidError {
    match e {
        NumError::InvalidInput { operation, reason } => {
            SolidError::InvalidInput { operation, reason }
        }
        NumError::Failed {
            operation,
            failure,
            cause,
        } => {
            let cause = cause.map(|c| num_pin(*c));
            let f = match failure {
                NumFailure::Singular { measure } => GeomFailure::Singular { measure },
                NumFailure::IndexOutOfRange { index, len } => {
                    GeomFailure::IndexOutOfRange { index, len }
                }
                NumFailure::NotSquare { rows, cols } => GeomFailure::NotSquare { rows, cols },
                NumFailure::CallFailed { .. } => GeomFailure::CallFailed {
                    class: cause
                        .as_ref()
                        .map_or(solidcore::error::ErrorKind::Failed, SolidError::kind),
                },
                _ => GeomFailure::Defect {
                    why: "a layer-0 failure the solver facade does not map",
                },
            };
            let mut fl = Failure::new(operation, f);
            if let Some(c) = cause {
                fl = fl.caused_by(c);
            }
            fl.into_error()
        }
    }
}

/// The solver's error as the pin reports it, one for one (report 53 section 12).
pub fn solve_pin(e: SolveError<XErr>) -> SolidError {
    match e {
        SolveError::InvalidInput { operation, reason } => {
            SolidError::InvalidInput { operation, reason }
        }
        SolveError::Failed { operation, failure } => Failure::new(
            operation,
            match failure {
                SolveFailure::BrentDidNotConverge {
                    iterations,
                    at,
                    bracket,
                } => GeomFailure::BrentDidNotConverge {
                    iterations,
                    at,
                    bracket,
                },
                SolveFailure::ZeroDifferenceStep { parameter, at } => {
                    GeomFailure::ZeroDifferenceStep { parameter, at }
                }
            },
        )
        .into_error(),
        SolveError::Num(n) => num_pin(n),
        SolveError::CallFailed { operation, cause } => Failure::new(
            operation,
            GeomFailure::CallFailed {
                class: cause.0.kind(),
            },
        )
        .caused_by(cause.0)
        .into_error(),
        SolveError::Eval(e) => e.0,
    }
}

/// `F(t)`, `F'(t)` and the evaluator's own found flag of test function `func`.
pub fn f1(func: usize, t: f64) -> (f64, f64, bool) {
    match func {
        0 => (t * t - 2.0, 2.0 * t, false),
        1 => ((t * t - 2.0) * t + 2.0, 3.0 * t * t - 2.0, false),
        2 => ((t - 0.75) * (t - 0.75) - 0.01, 2.0 * (t - 0.75), false),
        3 => (t * t + 1.0, 2.0 * t, false),
        4 => ((t - 0.3) * (t - 0.3), 2.0 * (t - 0.3), false),
        5 => (1.0 - (t - 0.3) * (t - 0.3), -2.0 * (t - 0.3), false),
        6 => (t - 3.0, 1.0, false),
        7 => {
            let d = 1.0 + t.abs();
            (t / d - 0.5, 1.0 / (d * d), false)
        }
        8 => (t * t - 2.0, 2.0 * t, (t * t - 2.0).abs() < 1e-3),
        // a reported slope that sends Newton from t to -t: a two-cycle of equal-sign
        // values around the roots +-0.5
        9 => {
            let f = t * t - 0.25;
            (f, if t == 0.0 { 1.0 } else { f / (2.0 * t) }, false)
        }
        // a slope that overshoots below 0.5 and points the wrong way above: diverges
        // with the root bracketed
        10 => (t - 0.3, if t < 0.5 { 0.25 } else { -1.0 }, false),
        // a constant with a huge slope: the Newton step vanishes
        11 => (1.0, 1e30, false),
        12 => (2e-4, 1e30, false),
        _ => (f64::NAN, f64::NAN, false),
    }
}

/// Number of 1-D test functions.
pub const F1: usize = 14;

/// The N-D test equation sets (two unknowns): values, row-major Jacobian, and whether the
/// evaluator computes the Jacobian by differences, with which intervals.
pub fn fn_n(func: usize, x: &[f64]) -> ([f64; 2], [f64; 4]) {
    let (a, b) = (x.first().copied().unwrap_or(0.0), x.get(1).copied().unwrap_or(0.0));
    match func {
        0 | 1 | 6 | 7 | 8 => (
            [
                4.0 * a + b + 0.5 * a * a * a - 1.0,
                a + 3.0 * b + 0.25 * b * b * b - 2.0,
            ],
            [4.0 + 1.5 * a * a, 1.0, 1.0, 3.0 + 0.75 * b * b],
        ),
        2 => ([a * a - 1.0, b - 4.0], [2.0 * a, 0.0, 0.0, 1.0]),
        3 => ([10.0 * (b - a * a), 1.0 - a], [-20.0 * a, 10.0, -1.0, 0.0]),
        4 => ([a + b - 1.0, -2.0], [1.0, 1.0, 0.0, 0.0]),
        // a root of multiplicity three: the steps shrink by a third
        10 => ([a * a * a, b * b * b], [3.0 * a * a, 0.0, 0.0, 3.0 * b * b]),
        // no root, and a Jacobian that makes every step vanish
        11 => ([1.0 + 0.0 * a, b], [1e10, 0.0, 0.0, 1.0]),
        _ => ([a * a + 1.0, b], [2.0 * a, 0.0, 0.0, 1.0]),
    }
}

/// Number of N-D test equation sets.
pub const FN: usize = 12;

/// `Some(intervals)` when equation set `func` computes its Jacobian by differences.
pub fn fd(func: usize) -> Option<Option<[[f64; 2]; 2]>> {
    match func {
        1 => Some(None),
        6 => Some(Some([[-2.0, 2.0], [-2.0, 2.0]])),
        7 => Some(Some([[0.5, 0.5], [-2.0, 2.0]])),
        _ => None,
    }
}

/// Equation set `func`'s own found flag.
pub fn found_n(func: usize, f: &[f64; 2]) -> bool {
    func == 8 && f[0].abs() + f[1].abs() < 1e-6
}

/// Equation set 9 says it wants the intervals but keeps the default `set_intervals`.
pub fn wants_intervals(func: usize) -> bool {
    func == 9
}

/// The evaluators, the same source on both sides (`$assoc` adds the scx side's error type).
macro_rules! evaluators {
    ($($assoc:tt)*) => {
        use self::local_solve1d::{EvalFunction, FunctionValue};
        use self::local_solve_nd::{compute_jacobian, EvalNFunctions};
        use self::matrix::Matrix;
        use self::extent1d::Extent1d;
        use crate::solvers::{f1, fd, fn_n, found_n, wants_intervals, Class};

        /// A 1-D evaluator that logs `(t, signal_errors, failed)` per call and fails at call
        /// `fail_at` (and every later call when `persist`) with class `class`.
        pub struct Rec1 {
            pub func: usize,
            pub fail_at: usize,
            pub persist: bool,
            pub class: Class,
            pub log: Vec<(f64, bool, bool)>,
        }

        impl Rec1 {
            pub fn new(func: usize, fail_at: usize, persist: bool, class: Class) -> Self {
                Self { func, fail_at, persist, class, log: Vec::new() }
            }
        }

        impl EvalFunction for Rec1 {
            $($assoc)*
            fn evaluate(&mut self, t: f64, out: &mut FunctionValue, s: bool) -> Result<(), EvErr> {
                let k = self.log.len();
                let fail = k == self.fail_at || (self.persist && k > self.fail_at);
                self.log.push((t, s, fail));
                if fail {
                    // as `SmCurvePropertyEFO::Evaluate`: zeroed outputs, then the error
                    *out = FunctionValue::default();
                    return Err(err(self.class));
                }
                let (f, f_prime, found) = f1(self.func, t);
                out.f = f;
                out.f_prime = f_prime;
                out.found_answer = found;
                Ok(())
            }
        }

        /// A 1-D evaluator with the default (failing) `evaluate`.
        pub struct Base1;
        impl EvalFunction for Base1 {
            $($assoc)*
        }

        /// An N-D equation set that logs `(x, jacobian asked, failed)` per call and fails as
        /// [`Rec1`] does.
        pub struct RecN {
            pub func: usize,
            pub fail_at: usize,
            pub persist: bool,
            pub class: Class,
            pub log: Vec<(Vec<f64>, bool, bool)>,
        }

        impl RecN {
            pub fn new(func: usize, fail_at: usize, persist: bool, class: Class) -> Self {
                Self { func, fail_at, persist, class, log: Vec::new() }
            }
        }

        impl EvalNFunctions for RecN {
            $($assoc)*
            fn has_intervals(&self) -> bool {
                wants_intervals(self.func)
            }
            fn evaluate(
                &mut self,
                x: &[f64],
                f: &mut [f64],
                jacobian: Option<&mut Matrix>,
            ) -> Result<bool, EvErr> {
                let k = self.log.len();
                let fail = k == self.fail_at || (self.persist && k > self.fail_at);
                self.log.push((x.to_vec(), jacobian.is_some(), fail));
                if fail {
                    f.fill(0.0);
                    return Err(err(self.class));
                }
                let (vals, jac) = fn_n(self.func, x);
                for (d, v) in f.iter_mut().zip(vals) {
                    *d = v;
                }
                if let Some(jm) = jacobian {
                    if let Some(ivls) = fd(self.func) {
                        let mut xx = x.to_vec();
                        let ff = f.to_vec();
                        let e = ivls.map(|a| a.map(|[lo, hi]| Extent1d::from_min_max(lo, hi)));
                        compute_jacobian(self, &mut xx, &ff, jm, e.as_ref().map(|a| &a[..]))
                            .map_err(solve_err)?;
                    } else {
                        for i in 0..2 {
                            for j in 0..2 {
                                jm.set_at(i, j, jac[i * 2 + j]).map_err(num_err)?;
                            }
                        }
                    }
                }
                Ok(found_n(self.func, &vals))
            }
        }
    };
}

/// The scx side.
pub mod x {
    pub use scx_algo::{local_solve1d, local_solve_nd};
    pub use scx_num::{extent1d, matrix};
    pub type EvErr = super::XErr;
    pub fn err(c: Class) -> EvErr {
        super::XErr(super::pin_error(c))
    }
    fn num_err(e: scx_num::NumError) -> EvErr {
        super::XErr(super::num_pin(e))
    }
    fn solve_err(e: scx_algo::error::SolveError<EvErr>) -> EvErr {
        super::XErr(super::solve_pin(e))
    }
    /// A solver error as `(Display, the pin's error)`.
    pub fn pin(e: scx_algo::error::SolveError<EvErr>) -> (String, solidcore::SolidError) {
        (e.to_string(), super::solve_pin(e))
    }
    evaluators!(type Error = EvErr;);
}

/// The pin's side.
pub mod s {
    pub use solidcore::geom::{extent1d, local_solve1d, local_solve_nd, matrix};
    pub type EvErr = solidcore::SolidError;
    pub fn err(c: Class) -> EvErr {
        super::pin_error(c)
    }
    fn num_err(e: EvErr) -> EvErr {
        e
    }
    fn solve_err(e: EvErr) -> EvErr {
        e
    }
    /// A solver error as `(Display, the error)`.
    pub fn pin(e: EvErr) -> (String, solidcore::SolidError) {
        (e.to_string(), e)
    }
    evaluators!();
}
