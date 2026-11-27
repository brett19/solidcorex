// Written for solidcorex. Apache-2.0. Mirrors, for the local solvers, how solidcore at
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8 reports their failures (`error/geom.rs`
// `GeomFailure::{BrentDidNotConverge, ZeroDifferenceStep, CallFailed}`,
// `SolidError::geom_call`, `error.rs` `ignored`), one for one (report 53, section 12), and
// for the ODE integrator its `error/mass.rs` `MassFailure::{StepControl, Integrand}` and
// `SolidError::nurbs_routine` (report 53, section 13).
//! Errors of the local solvers and the ODE integrator, generic over the evaluator's error.
//!
//! A solver cannot name the kernel's error type (it sits above this layer), yet it acts on
//! the class of what an evaluator returns, exactly as solidcore does:
//!
//! | what the solver does | solidcore | here |
//! |---|---|---|
//! | stops at once, even where it would retry | `Unported`, `Cancelled`, `OutOfFuel` | [`EvalError::ends_solve`] |
//! | returns the error unchanged where it would wrap a failed call | those and `PreparationRequired` (`geom_call`) | [`EvalError::passes_through`] |
//! | never skips the failed sample | those, `FaceTrimsUnavailable` and an uncertified blend (`ignored`) | [`EvalError::never_ignored`] |
//!
//! Whether the N-dimensional solver's Jacobian system is "bad" is read from the matrix
//! solve's error (a layer-0 [`NumError::Failed`], upstream `SM_ERR`), never from the
//! evaluator's, so it is not part of [`EvalError`].
//!
//! [`SolveError`] maps one for one onto solidcore's errors: `InvalidInput` and `Failed`
//! (the solver's own failures) and [`SolveError::Num`] as they are there;
//! [`SolveError::CallFailed`] is solidcore's `Failed` with `GeomFailure::CallFailed`, whose
//! class is the cause's; [`SolveError::Eval`] is the evaluator's error returned unchanged.

use core::fmt;

use scx_num::error::ErrorKind;
use scx_num::NumError;

/// What a solver needs to know about an evaluator's error.
pub trait EvalError: fmt::Debug + fmt::Display {
    /// The solve stops here, even where it would otherwise retry or skip the sample
    /// (solidcore: unported, cancelled, out of fuel).
    fn ends_solve(&self) -> bool;

    /// Returned unchanged where the solver would report a failed call with this as its
    /// cause: everything [`EvalError::ends_solve`] holds for, and an evaluation that needs
    /// a prepared body (solidcore `SolidError::geom_call`).
    fn passes_through(&self) -> bool;

    /// Never skipped as a failed sample: everything [`EvalError::passes_through`] holds
    /// for, missing face trims and a blend certificate that ran out of work (solidcore
    /// `ignored`).
    fn never_ignored(&self) -> bool;
}

/// A local solver's own failure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SolveFailure {
    /// The Brent minimisation reached its iteration limit.
    BrentDidNotConverge {
        /// The iterations run.
        iterations: u64,
        /// The last estimate.
        at: f64,
        /// The bracket left, `[min, max]`.
        bracket: [f64; 2],
    },
    /// A finite-difference step of a parameter collapsed to zero.
    ZeroDifferenceStep {
        /// The parameter's index.
        parameter: usize,
        /// The parameter's value.
        at: f64,
    },
    /// The ODE integrator's step control cannot meet the accuracy (solidcore
    /// `MassFailure::StepControl`), for the reason given.
    StepControl(StepControlFailure),
    /// The integrand evaluated to a value that is not a finite number (solidcore
    /// `MassFailure::Integrand`).
    IntegrandNotFinite,
}

/// Why the ODE integrator's step control gave up (`crate::ode`): solidcore reports every
/// one of these but [`StepControlFailure::Overflow`] as `MassFailure::StepControl`, without
/// the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepControlFailure {
    /// A step's error estimate is not above the accuracy by more than its own rounding
    /// (`accuracy <= estimate * EFF_ZERO`), or the accuracy is not positive.
    Unachievable,
    /// The proposed next step is no longer than the minimum step.
    BelowMinimumStep,
    /// The step budget ran out before the end of the interval.
    StepsExhausted,
    /// A step was retried, shrinking, the most times allowed.
    RetriesExhausted,
    /// A retried step shrank to nothing (`x + h` is `x`) or is not finite.
    StepUnderflow,
    /// The errors owed by steps accepted through the rounding floor exceed the accuracy
    /// relative to the integral.
    ErrorOwed,
    /// A step's error scale is not a number: the step grew past the largest float with a
    /// zero derivative, or the values overflowed (solidcore: a failed routine caused by
    /// its own refusal of the scale, `invalid error scale`).
    Overflow,
}

impl StepControlFailure {
    /// The reason in words.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Unachievable => "the accuracy is below the rounding of the error estimate",
            Self::BelowMinimumStep => "the next step is below the minimum step",
            Self::StepsExhausted => "the step budget ran out",
            Self::RetriesExhausted => "a step was retried the most times allowed",
            Self::StepUnderflow => "a retried step shrank to nothing",
            Self::ErrorOwed => "the errors owed below the rounding floor exceed the accuracy",
            Self::Overflow => "the step or the values overflowed",
        }
    }
}

impl SolveFailure {
    /// The failure's class.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::BrentDidNotConverge { .. } => ErrorKind::StageFailed,
            Self::ZeroDifferenceStep { .. } | Self::StepControl(_) | Self::IntegrandNotFinite => {
                ErrorKind::Failed
            }
        }
    }
}

impl fmt::Display for SolveFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BrentDidNotConverge {
                iterations,
                at,
                bracket,
            } => write!(
                f,
                "{}: the Brent solve did not converge after {iterations} iterations (at {at}, bracket [{}, {}])",
                ErrorKind::StageFailed,
                bracket[0],
                bracket[1]
            ),
            Self::ZeroDifferenceStep { parameter, at } => write!(
                f,
                "the difference step of parameter {parameter} at {at} is zero"
            ),
            Self::StepControl(why) => write!(
                f,
                "the integrator's step control cannot meet the accuracy: {}",
                why.reason()
            ),
            Self::IntegrandNotFinite => f.write_str("the integrand is not a finite number"),
        }
    }
}

/// A local solver's error over the evaluator's error `E`.
#[derive(Debug, Clone, PartialEq)]
pub enum SolveError<E> {
    /// An argument was out of range or inconsistent (upstream `SM_ERR_INVALID_INPUT`).
    InvalidInput {
        /// Operation that rejected the input.
        operation: &'static str,
        /// What was wrong.
        reason: &'static str,
    },
    /// The solver's own failure (upstream `SM_ERR`).
    Failed {
        /// The solidcore path of the failing routine.
        operation: &'static str,
        /// What failed.
        failure: SolveFailure,
    },
    /// A layer-0 routine failed (a matrix the solver built or solved).
    Num(NumError),
    /// An evaluation failed and the solver reports it as its own failure, with the
    /// evaluator's error as cause (solidcore `GeomFailure::CallFailed`, class the cause's).
    CallFailed {
        /// The solidcore path of the solver routine.
        operation: &'static str,
        /// The evaluator's error.
        cause: E,
    },
    /// The evaluator's error, returned unchanged.
    Eval(E),
}

impl<E: EvalError> SolveError<E> {
    /// A failed evaluation inside `operation`: unchanged when it
    /// [passes through](EvalError::passes_through), else a [`SolveError::CallFailed`]
    /// (solidcore `local_solve1d::eval_failed`, `SolidError::geom_call`).
    pub fn call(operation: &'static str) -> impl Fn(E) -> Self {
        move |e| {
            if e.passes_through() {
                Self::Eval(e)
            } else {
                Self::CallFailed {
                    operation,
                    cause: e,
                }
            }
        }
    }

    /// Whether a solver may skip the failed step that returned this (solidcore `ignored`
    /// returns `None` for it).
    #[must_use]
    pub fn is_ignorable(&self) -> bool {
        match self {
            Self::Eval(e) => !e.never_ignored(),
            _ => true,
        }
    }
}

impl<E: fmt::Display> fmt::Display for SolveError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { operation, reason } => {
                write!(f, "{operation}: invalid input: {reason}")
            }
            Self::Failed { operation, failure } => write!(f, "{operation}: {failure}"),
            Self::Num(e) => e.fmt(f),
            Self::CallFailed { operation, cause } => {
                write!(
                    f,
                    "{operation}: a routine it calls failed; caused by {cause}"
                )
            }
            Self::Eval(e) => e.fmt(f),
        }
    }
}

impl<E: fmt::Debug + fmt::Display + 'static> std::error::Error for SolveError<E> {}

impl<E> From<NumError> for SolveError<E> {
    fn from(e: NumError) -> Self {
        Self::Num(e)
    }
}

/// The value of a step whose failure the solver may skip: `Some` on success, `None` on an
/// [ignorable](SolveError::is_ignorable) error, the error otherwise (solidcore `ignored`).
pub(crate) fn ignored<T, E: EvalError>(
    r: Result<T, SolveError<E>>,
) -> Result<Option<T>, SolveError<E>> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(e) if !e.is_ignorable() => Err(e),
        Err(_) => Ok(None),
    }
}
