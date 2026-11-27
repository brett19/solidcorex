// Moved from solidcore `solidcore/crates/solidcore/src/geom/local_solve1d.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/inc/SmLocalSolve1d.h`,
// `source/SMLib/src/SmLocalSolve1d.cpp`.
// Changes: the evaluator's error is generic (`EvalFunction::Error`, classified by
// `EvalError`; owner decision 2026-11-26, report 53 section 12), so `eval_failed`,
// `EvalFunction::evaluate` (default), `IterQueue1d::try_new` (a `NumError`),
// `LocalSolve1d::solve_by_brent`, `LocalSolve1d::solve_by_bisection`,
// `LocalSolve1d::eval_into` (the evaluator's error unchanged),
// `LocalSolve1d::solve_oscillating` and `LocalSolve1d::solve_it` return `SolveError` and
// test the class where the pin matched `SolidError` variants. The tests use a test
// evaluator error (`TestErr`, with `fmt`, `ends_solve`, `passes_through`, `never_ignored`,
// `defect`), so `tests::failed_first_evaluation_without_interval_has_no_value`,
// `tests::failed_interior_samples_cannot_create_a_bracket`,
// `tests::successful_sample_after_a_failure_can_still_bracket_a_root`, `tests::errors` and
// `tests::wr19_failed_first_evaluation_and_valid_start` match it. Paths otherwise. Results and every error's class, text and cause
// unchanged. `UNDEF_DOUBLE` is a private constant here (`scx_num::math` no longer has
// one). The solver is an options value and a pure call (`docs/01-architecture.md` §10.1
// item 6): `LocalSolve1d` holds only the options (`Domain1d` replaces `use_interval`,
// `interval` and `is_periodic`); `LocalSolve1d::new` and `LocalSolve1d::with_interval`
// are `const`; `LocalSolve1d::solve_it` takes `&self` and starts from
// `Solve1dState::START` every call; the new `LocalSolve1d::solve_traced` returns the state
// (`Solve1dState`, the former public members; `found_accuracy` is an `Option`); the
// iteration is the private `Run::solve`, with `Run::solve_oscillating`, `Run::eval_into`
// and `Run::reason_for` moved from `LocalSolve1d`; `max_boundary_hits` is the constant
// `MAX_BOUNDARY_HITS` and the bound hits start at 0; `UNDEF_ULONG`,
// `LocalSolve1d::found_accuracy`, `LocalSolve1d::termination_reason`,
// `LocalSolve1d::lower_bound_hits`, `LocalSolve1d::upper_bound_hits`,
// `LocalSolve1d::interval`, `LocalSolve1d::set_solution_interval`,
// `LocalSolve1d::set_solver_algorithm`, `LocalSolve1d::set_desired_accuracy` and
// `LocalSolve1d::desired_accuracy` are removed (the fields are public);
// `IterationValue::UNSET` is new. The tests `tests::newton_sqrt2`,
// `tests::issue_481_failed_guess_is_no_sign_change`,
// `tests::failed_guess_does_not_supply_a_sign_change`,
// `tests::failed_interior_samples_cannot_create_a_bracket`,
// `tests::successful_sample_after_a_failure_can_still_bracket_a_root`,
// `tests::wr19_unbounded_newton_converges_positive_and_negative_roots` and every test that
// made its solver `mut` read the state from `solve_traced` or a test `Run::fresh`.
//! One-dimensional local root finding (damped Newton iteration with oscillation
//! detection, regula-falsi bisection fallback) and Brent-like minimisation /
//! maximisation, driven by a caller-supplied function evaluator.
//!
//! Upstream: `source/SMLib/inc/SmLocalSolve1d.h`, `source/SMLib/src/SmLocalSolve1d.cpp`.
//!
//! | Rust                                         | Upstream                                       |
//! |----------------------------------------------|------------------------------------------------|
//! | [`SolverAlgorithm`]                          | `SmLocalSolverAlgorithmType`                   |
//! | [`TerminationReason`] (re-export)            | `SmTerminationReasonType` (defined in `local_solve_nd`) |
//! | [`EvalFunction`], [`FunctionValue`]          | `SmEvalFunctionObject`                         |
//! | [`EvalFunction::evaluate`] (default)         | `SmEvalFunctionObject::Evaluate`               |
//! | [`IterationValue`]                           | `SmIterationValue`                             |
//! | [`IterQueue1d`], [`IterQueue1d::new`], [`IterQueue1d::try_new`] | `SmIterQueue1d`, `SmIterQueue1d(ULONG)` |
//! | [`IterQueue1d::add`]                         | `SmIterQueue1d::Add`                           |
//! | [`IterQueue1d::is_oscillating`]              | `SmIterQueue1d::IsOscillating`                 |
//! | [`LocalSolve1d`] (the options), [`Domain1d`] | `SmLocalSolve1d`'s settings                    |
//! | [`LocalSolve1d::new`] | `SmLocalSolve1d(SmEvalFunctionObject&)` |
//! | [`LocalSolve1d::with_interval`] | `SmLocalSolve1d(SmEvalFunctionObject&, const SmExtent1d&, SmBoolean)` |
//! | [`Solve1dState`]                             | `SmLocalSolve1d`'s members after a solve       |
//! | [`Solve1dState`]'s fields                    | `GetFoundAccuracy`, `GetTerminationReason`, `GetLowerBoundHits`, `GetUpperBoundHits` |
//! | [`LocalSolve1d::domain`] etc. (public fields) | `SetSolutionInterval`, `SetSolverAlgorithm`, `SetDesiredAccuracy` |
//! | [`LocalSolve1d::solve_it`], [`LocalSolve1d::solve_traced`], [`SolveOutcome`] | `SolveIt` |
//! | [`LocalSolve1d::solve_by_bisection`]         | `SolveByBisection`                             |
//! | [`LocalSolve1d::solve_by_brent`]             | `SolveByBrent`                                 |
//! | `solve_oscillating` (private)                | `SolveOscillating`                             |
//!
//! Deviations:
//! - The solver is a value of options and each solve a pure call
//!   (`docs/01-architecture.md` §10.1 item 6): the evaluator is passed to each call,
//!   every [`LocalSolve1d::solve_it`] starts from the same state, and the state it
//!   leaves (upstream's members) is returned by [`LocalSolve1d::solve_traced`]. Upstream
//!   carries the members from one `SolveIt` to the next, but `SolveIt` resets every one
//!   it reads before reading it except the iterates and bracket parameters, which it
//!   writes first on every path but the one in [`UNDEF_DOUBLE`]'s note. `SetSolutionInterval`
//!   on a solver built without one changes nothing upstream; [`Domain1d::Line`] has no
//!   interval to change.
//! - [`EvalFunction::evaluate`] keeps upstream's out-parameter shape: the solver
//!   hands it the destination values and keeps whatever the evaluator wrote even
//!   when it fails, because upstream reads such values afterwards (e.g. the
//!   interior search of `SolveOscillating` continues after a failed evaluation).
//! - Out-params become return values: [`SolveOutcome`] for `SolveIt` (the found
//!   parameter is `None` where upstream leaves `rdFoundT` unwritten), `(t, f)` for
//!   `SolveByBisection`/`SolveByBrent`. On failure nothing is returned (upstream
//!   callers do not read the out-params after an error); the solver state is still
//!   updated as upstream.
//! - Status codes: upstream returns either the evaluator's status or `SM_ERR`
//!   (1001, [`SolveError`]) exactly as in C++. An evaluator error that
//!   [ends the solve](EvalError::ends_solve) is always propagated unchanged,
//!   also where upstream ignores or retries a failed evaluation (so callers that
//!   reach unported code report "deferred" instead of a different result).
//! - `SolveByBisection` recursion is a loop (same arithmetic and order).
//! - [`IterQueue1d::try_new`] (caller-chosen sizes) returns `InvalidInput` when the
//!   slots cannot be allocated or would take more than
//!   [`scx_num::limits::MAX_ALLOC_BYTES`] (upstream's `SetSize` fails softly and `Add`
//!   then writes out of bounds; upstream's only queue has 8 slots).
//! - `SE(...)`/`SER` error-log side effects and `SM_DEBUG_CODE` tracing are not
//!   ported.
//! - A failed evaluation has no value (WR-253, tangency-labs/solidcore#481 and #2574):
//!   `eval_into` leaves the iteration value as it was, `SolveOscillating` takes a sign
//!   change only from an interior guess that evaluated, and `SolveIt` without an interval
//!   returns a failed first evaluation's status instead of iterating from it. Upstream
//!   copies whatever the evaluator wrote before failing, so an evaluator that zeroes its
//!   outputs first hands the iteration a zero of `F`.
//! - Without an interval, `SolveIt` takes unclamped Newton steps (WR-19): upstream
//!   clamps each step against the default inside-out `[1e20, -1e20]` interval, replacing
//!   every step with `+-1.584e19` against the Newton direction.

use crate::error::{ignored, EvalError, SolveError, SolveFailure};
pub use crate::local_solve_nd::TerminationReason;
use scx_num::extent1d::Extent1d;
use scx_num::limits::try_filled;
use scx_num::math::{BIG_DOUBLE, EFF_ZERO, EFF_ZERO_SQ};

/// Upstream `SM_UNDEF_DOUBLE`, the solver state's value before it is first written. The
/// solver reads it in one case (a bracket flagged by `F <= -SM_BIG_DOUBLE` before its
/// parameter is recorded), so it stays here, private, until that defect is fixed on
/// purpose (`docs/01-architecture.md` §10.1 item 10).
const UNDEF_DOUBLE: f64 = 1.23456e-61;
use scx_num::tol::in_tol;
use scx_num::NumError;

/// `ZEPS` of `SolveByBrent`.
const ZEPS: f64 = 1.0e-12;

/// Upstream returns `SM_ERR` after a failed evaluation: the solver's failure keeps the
/// evaluator's error as its cause ([`SolveError::CallFailed`]); an error that
/// [passes through](EvalError::passes_through) is returned unchanged.
fn eval_failed<E: EvalError>(e: E, operation: &'static str) -> SolveError<E> {
    SolveError::call(operation)(e)
}

/// `SMSIGN(a, b)`: `|a|` with the sign of `b` (`b >= 0` counts as positive).
fn sign(a: f64, b: f64) -> f64 {
    if b >= 0.0 {
        a.abs()
    } else {
        -a.abs()
    }
}

/// Algorithm used by the solver (`SmLocalSolverAlgorithmType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SolverAlgorithm {
    /// `SM_SA_NEWTON` (default).
    #[default]
    Newton,
    /// `SM_SA_BISECTION`.
    Bisection,
    /// `SM_SA_BRENT_MAXIMIZE`.
    BrentMaximize,
    /// `SM_SA_BRENT_MINIMIZE`.
    BrentMinimize,
}

impl SolverAlgorithm {
    /// Upstream enum value (`SM_SA_NEWTON` = 0 .. `SM_SA_BRENT_MINIMIZE` = 3).
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::Newton => 0,
            Self::Bisection => 1,
            Self::BrentMaximize => 2,
            Self::BrentMinimize => 3,
        }
    }

    /// One of the Brent min/max modes.
    #[must_use]
    pub const fn is_brent(self) -> bool {
        matches!(self, Self::BrentMaximize | Self::BrentMinimize)
    }

    /// The algorithm with upstream enum value `code`.
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Newton),
            1 => Some(Self::Bisection),
            2 => Some(Self::BrentMaximize),
            3 => Some(Self::BrentMinimize),
            _ => None,
        }
    }
}

/// Values an evaluator writes for one parameter (the out-params of
/// `SmEvalFunctionObject::Evaluate`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FunctionValue {
    /// `F(t)`.
    pub f: f64,
    /// `dF/dt (t)`.
    pub f_prime: f64,
    /// True when the evaluator judges `F(t)` to be zero within its own tolerance.
    pub found_answer: bool,
}

/// A function `F(t)` with derivative, solved by [`LocalSolve1d`]
/// (`SmEvalFunctionObject`).
pub trait EvalFunction {
    /// The evaluator's error; the solver acts on its [class](EvalError).
    type Error: EvalError;

    /// Evaluates `F` and `F'` at `t` into `out` (`Evaluate`). `signal_errors` is
    /// false where the solver anticipates failures. On failure the solver keeps
    /// whatever was written to `out`, as upstream does.
    ///
    /// The default (upstream base class) fails with `SM_ERR`.
    fn evaluate(
        &mut self,
        t: f64,
        out: &mut FunctionValue,
        signal_errors: bool,
    ) -> Result<(), Self::Error> {
        let _ = (t, out, signal_errors);
        Err(Self::Error::defect(
            "geom::local_solve1d::EvalFunction::evaluate",
            "the solver called an EvalFunction that does not implement evaluate; every function the kernel solves implements it",
        ))
    }
}

/// One iteration's values (`SmIterationValue`); all `UNDEF_DOUBLE` initially.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IterationValue {
    /// Parameter `t`.
    pub t: f64,
    /// `F(t)`.
    pub f: f64,
    /// `F'(t)`.
    pub f_prime: f64,
    /// The step that led to this `t`.
    pub prev_step: f64,
}

impl IterationValue {
    /// Every value upstream's `SM_UNDEF_DOUBLE` ([`UNDEF_DOUBLE`]): not yet written.
    pub const UNSET: Self = Self {
        t: UNDEF_DOUBLE,
        f: UNDEF_DOUBLE,
        f_prime: UNDEF_DOUBLE,
        prev_step: UNDEF_DOUBLE,
    };
}

impl Default for IterationValue {
    fn default() -> Self {
        Self::UNSET
    }
}

/// Fixed-size ring of the most recent iteration values (`SmIterQueue1d`): once
/// full, each addition overwrites the oldest slot.
#[derive(Debug, Clone, PartialEq)]
pub struct IterQueue1d {
    array: Vec<IterationValue>,
    size: u64,
    count: u64,
    next_idx: u64,
    full: bool,
}

impl IterQueue1d {
    /// An empty queue of `size` slots (`SmIterQueue1d(ULONG)`).
    ///
    /// For small sizes known at the call site. Like any `Vec`, it aborts when the
    /// slots cannot be allocated; use [`IterQueue1d::try_new`] for sizes that come
    /// from caller input.
    #[must_use]
    pub fn new(size: u64) -> Self {
        Self {
            array: vec![IterationValue::default(); size as usize],
            size,
            count: 0,
            next_idx: 0,
            full: false,
        }
    }

    /// An empty queue of `size` slots (`SmIterQueue1d(ULONG)`) for sizes that come
    /// from caller input.
    ///
    /// # Errors
    ///
    /// [`NumError::InvalidInput`] when `size` slots cannot be allocated or would take
    /// more than [`scx_num::limits::MAX_ALLOC_BYTES`] (upstream's `SetSize` then fails
    /// softly and `Add` writes past the empty array).
    pub fn try_new(size: u64) -> Result<Self, NumError> {
        let too_large = || NumError::InvalidInput {
            operation: "geom::local_solve1d::IterQueue1d::try_new",
            reason: "queue size too large to allocate",
        };
        let len = usize::try_from(size).map_err(|_| too_large())?;
        let array = try_filled(len, IterationValue::default()).ok_or_else(too_large)?;
        Ok(Self {
            array,
            size,
            count: 0,
            next_idx: 0,
            full: false,
        })
    }

    /// Stores `v` in the next slot (`Add`). A zero-size queue (undefined behaviour
    /// upstream) ignores the value.
    pub fn add(&mut self, v: &IterationValue) {
        let Some(slot) = self.array.get_mut(self.next_idx as usize) else {
            return;
        };
        *slot = *v;
        self.next_idx += 1;
        if self.next_idx >= self.size {
            self.next_idx = 0;
            self.full = true;
        }
        if self.count < self.size {
            self.count += 1;
        }
    }

    /// Whether the stored steps alternate (period `step`, only 2 is supported) with
    /// at least three cycles: the alternating sums of `prev_step` (by slot index)
    /// nearly cancel (`IsOscillating`; upstream default `step` is 2).
    #[must_use]
    pub fn is_oscillating(&self, step: u64) -> bool {
        if step != 2 {
            return false;
        }
        if self.count < 3 * step {
            return false;
        }
        let top = if self.full { self.size } else { self.next_idx };
        let mut sum1 = 0.0;
        let mut sum2 = 0.0;
        for (i, v) in self.array.iter().enumerate().take(top as usize) {
            if (i as u64) % step == 0 {
                sum1 += v.prev_step;
            } else {
                sum2 += v.prev_step;
            }
        }
        let sum_all = (sum1 + sum2).abs();
        let sum_abs = sum1.abs() + sum2.abs();
        sum_all < sum_abs / 100.0
    }
}

/// Result of [`LocalSolve1d::solve_it`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolveOutcome {
    /// `rbFoundSolution`: a solution within the acceptable accuracy was found.
    pub found: bool,
    /// `rdFoundT`: the solution, or the best parameter reached when none was found.
    pub t: f64,
}

/// Where a solve looks for `t`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Domain1d {
    /// The whole real line: Newton steps are not limited or clamped (WR-19).
    #[default]
    Line,
    /// `interval`: the guess is clamped into it, each Newton step is limited to 8% of its
    /// length, and a step that leaves it is clamped at the end it crosses.
    Bounded(Extent1d),
    /// `interval` as one period: as [`Domain1d::Bounded`], but a step that leaves it
    /// wraps.
    Periodic(Extent1d),
}

impl Domain1d {
    /// The interval, if the domain has one.
    #[must_use]
    pub const fn interval(self) -> Option<Extent1d> {
        match self {
            Self::Line => None,
            Self::Bounded(e) | Self::Periodic(e) => Some(e),
        }
    }
}

/// Consecutive clamps at one end of the interval after which a solve stops
/// [out of bounds](TerminationReason::OutOfBounds) (upstream sets `m_lMaxBoundaryHits`
/// to 2 in every `SolveIt`).
const MAX_BOUNDARY_HITS: u64 = 2;

/// The options of a one-dimensional local solve (`SmLocalSolve1d`'s settings). A value:
/// every solve ([`LocalSolve1d::solve_it`], [`LocalSolve1d::solve_by_brent`],
/// [`LocalSolve1d::solve_by_bisection`]) reads it and changes nothing, so one value may
/// serve any number of solves, in any order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalSolve1d {
    /// Where `t` is looked for (default: the whole line).
    pub domain: Domain1d,
    /// The algorithm (default: Newton).
    pub algorithm: SolverAlgorithm,
    /// `|F|` below which iteration stops as converged (default `SM_EFF_ZERO`).
    pub desired_accuracy: f64,
    /// The iteration limit (default 100).
    pub max_iter: u64,
}

impl Default for LocalSolve1d {
    fn default() -> Self {
        Self::new()
    }
}

/// What one [`LocalSolve1d::solve_traced`] left behind: its brackets, last iterates,
/// counters and why it stopped. Every solve starts from the same state; none reads a
/// previous one's.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // upstream's SmBoolean members
pub struct Solve1dState {
    /// A zero crossing's lower (`F < 0`) side is known.
    pub have_lower_bracket: bool,
    /// `F` at the lower bracket.
    pub lower_bracket_f: f64,
    /// Parameter of the lower bracket.
    pub lower_bracket_t: f64,
    /// A zero crossing's upper (`F > 0`) side is known.
    pub have_upper_bracket: bool,
    /// `F` at the upper bracket.
    pub upper_bracket_f: f64,
    /// Parameter of the upper bracket.
    pub upper_bracket_t: f64,
    /// Values two iterations ago (`m_v2Prev`).
    pub prev2: IterationValue,
    /// Values one iteration ago (`m_v1Prev`).
    pub prev1: IterationValue,
    /// Current values (`m_vCurr`): the final `t` and `F`.
    pub curr: IterationValue,
    /// Iteration counter (`m_lIter`).
    pub iter: u64,
    /// `F` at the returned parameter (`m_dFoundAccuracy`), where the solve recorded one.
    pub found_accuracy: Option<f64>,
    /// Consecutive clamps at the interval minimum.
    pub lower_bound_hits: u64,
    /// Consecutive clamps at the interval maximum.
    pub upper_bound_hits: u64,
    /// Why the solve stopped.
    pub termination_reason: TerminationReason,
}

impl Solve1dState {
    /// The state every solve starts from (upstream's resets at the top of `SolveIt`).
    /// The bracket parameters and iterates hold upstream's `SM_UNDEF_DOUBLE` until
    /// written; see [`UNDEF_DOUBLE`].
    const START: Self = Self {
        have_lower_bracket: false,
        lower_bracket_f: -BIG_DOUBLE,
        lower_bracket_t: UNDEF_DOUBLE,
        have_upper_bracket: false,
        upper_bracket_f: BIG_DOUBLE,
        upper_bracket_t: UNDEF_DOUBLE,
        prev2: IterationValue::UNSET,
        prev1: IterationValue::UNSET,
        curr: IterationValue {
            prev_step: 0.0,
            ..IterationValue::UNSET
        },
        iter: 0,
        found_accuracy: None,
        lower_bound_hits: 0,
        upper_bound_hits: 0,
        termination_reason: TerminationReason::UnableToConverge,
    };
}

impl LocalSolve1d {
    /// Newton on the whole real line (`SmLocalSolve1d(rFunctionEvaluator)`).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            domain: Domain1d::Line,
            algorithm: SolverAlgorithm::Newton,
            desired_accuracy: EFF_ZERO,
            max_iter: 100,
        }
    }

    /// Newton restricted to `interval`, wrapping instead of clamping when `is_periodic`
    /// (`SmLocalSolve1d(rFunctionEvaluator, crInterval, bIsPeriodic)`).
    #[must_use]
    pub const fn with_interval(interval: Extent1d, is_periodic: bool) -> Self {
        Self {
            domain: if is_periodic {
                Domain1d::Periodic(interval)
            } else {
                Domain1d::Bounded(interval)
            },
            ..Self::new()
        }
    }

    /// Finds a root of `F` near `guess_t` by damped Newton iteration, falling back
    /// to oscillation handling, bisection on a bracket, or Brent min/max for the
    /// `Brent*` algorithms (`SolveIt`). `found` is set when `|F| < desired` or
    /// within `acceptable_accuracy`. Succeeds even when no solution is found; fails
    /// only when an evaluation fails (`SM_ERR` or the evaluator's status). The
    /// state it leaves is [`LocalSolve1d::solve_traced`]'s.
    ///
    /// Without an interval ([`Self::new`]) the Newton steps are not clamped; upstream
    /// still clamps each against the default inside-out `[1e20, -1e20]` interval (WR-19),
    /// replacing every step with `±1.584e19` opposite to Newton.
    ///
    /// # Errors
    ///
    /// An evaluation's failure, as [`SolveError`].
    pub fn solve_it<E: EvalFunction + ?Sized>(
        &self,
        eval: &mut E,
        guess_t: f64,
        acceptable_accuracy: f64,
    ) -> Result<SolveOutcome, SolveError<E::Error>> {
        self.solve_traced(eval, guess_t, acceptable_accuracy).0
    }

    /// [`LocalSolve1d::solve_it`], with the state the solve left: brackets, iterates,
    /// counters, the found accuracy and the termination reason (upstream's members after
    /// `SolveIt`).
    pub fn solve_traced<E: EvalFunction + ?Sized>(
        &self,
        eval: &mut E,
        guess_t: f64,
        acceptable_accuracy: f64,
    ) -> (Result<SolveOutcome, SolveError<E::Error>>, Solve1dState) {
        let mut run = Run {
            o: *self,
            acceptable_accuracy,
            st: Solve1dState::START,
        };
        let r = run.solve(eval, guess_t);
        (r, run.st)
    }

    /// Minimises (or, with `maximize`, maximises) `F` on the bracket `ax`..`cx`
    /// starting from `bx`, by secant steps on `F'` or bisection (`SolveByBrent`).
    /// `F(bx)` should be better than `F(ax)` and `F(cx)`. Returns `(t, F(t))`
    /// (`F` as evaluated, not negated). Fails with `SM_ERR` when an evaluation
    /// fails or the iteration limit is reached.
    pub fn solve_by_brent<E: EvalFunction + ?Sized>(
        &self,
        eval: &mut E,
        ax: f64,
        bx: f64,
        cx: f64,
        tol: f64,
        maximize: bool,
    ) -> Result<(f64, f64), SolveError<E::Error>> {
        const OP: &str = "geom::local_solve1d::LocalSolve1d::solve_by_brent";
        let mut bracket_min = if ax < cx { ax } else { cx };
        let mut bracket_max = if ax > cx { ax } else { cx };

        let mut u: f64;
        let mut fu: f64;
        let mut du: f64;
        let mut x = bx;
        let mut v = bx;
        let mut fv = 0.0;
        let mut dv = 0.0;
        let mut w = bx;
        let mut fw = 0.0;
        let mut dw = 0.0;

        let mut delta_x = 0.0;
        let mut e: f64 = 0.0;

        let mut val = FunctionValue::default();
        eval.evaluate(x, &mut val, true)
            .map_err(|err| eval_failed(err, OP))?;
        let mut fx = val.f;
        let mut dx = val.f_prime;
        if maximize {
            fx = -fx;
            dx = -dx;
        }

        let mut iter = 1;
        while iter <= self.max_iter {
            let tol1 = tol * x.abs() + ZEPS;
            let tol2 = 2.0 * tol1;

            // Calculate delta_x by either secant or bisection.
            let mut secant_ok = false;
            if e.abs() > tol1 {
                // Trial secant steps on the derivatives, between x and w and x and v.
                let mut d1 = 2.0 * (bracket_max - bracket_min);
                let mut d2 = d1;
                if dw != dx && w != x {
                    d1 = (w - x) * dx / (dx - dw);
                }
                if dv != dx && v != x {
                    d2 = (v - x) * dx / (dx - dv);
                }
                let u1 = x + d1;
                let u2 = x + d2;

                // ok if: between the bracket ends && slope has opposite sign.
                let ok1 = (bracket_min - u1) * (u1 - bracket_max) > 0.0 && dx * d1 <= 0.0;
                let ok2 = (bracket_min - u2) * (u2 - bracket_max) > 0.0 && dx * d2 <= 0.0;

                let olde = e;
                e = delta_x;

                if ok1 || ok2 {
                    let step = if ok1 && ok2 {
                        if d1.abs() < d2.abs() {
                            d1
                        } else {
                            d2
                        }
                    } else if ok1 {
                        d1
                    } else {
                        d2
                    };

                    // Accept the secant step only if it is less than half the one
                    // before last.
                    if step.abs() <= (0.5 * olde).abs() {
                        secant_ok = true;
                        delta_x = step;
                        // Too close to the bracket ends?
                        let u = x + delta_x;
                        if u - bracket_min < tol2 || bracket_max - u < tol2 {
                            delta_x = sign(tol1, delta_x);
                        }
                    }
                }
            }
            if !secant_ok {
                // Bisect towards the downhill bracket end.
                e = if dx >= 0.0 {
                    bracket_min - x
                } else {
                    bracket_max - x
                };
                delta_x = 0.5 * e;
            }

            // Apply delta_x.
            if delta_x.abs() >= tol1 {
                u = x + delta_x;
                if u == x {
                    // Step is below machine precision.
                    return Ok((x, fx));
                }
                eval.evaluate(u, &mut val, true)
                    .map_err(|err| eval_failed(err, OP))?;
                fu = val.f;
                du = val.f_prime;
                if maximize {
                    fu = -fu;
                    du = -du;
                }
            } else {
                u = x + sign(tol1, delta_x);
                // Step must stay within the bracket.
                if u <= bracket_min {
                    u = (x + bracket_min) / 2.0;
                }
                if u >= bracket_max {
                    u = (x + bracket_max) / 2.0;
                }
                eval.evaluate(u, &mut val, true)
                    .map_err(|err| eval_failed(err, OP))?;
                fu = val.f;
                du = val.f_prime;
                if maximize {
                    fu = -fu;
                    du = -du;
                }
                // Within machine precision, or down in the noise.
                if u == x || fu >= fx {
                    return Ok((x, fx));
                }
            }

            // Update values.
            if fu <= fx {
                if u >= x {
                    bracket_min = x;
                } else {
                    bracket_max = x;
                }
                (v, fv, dv) = (w, fw, dw);
                (w, fw, dw) = (x, fx, dx);
                (x, fx, dx) = (u, fu, du);
            } else {
                if u < x {
                    bracket_min = u;
                } else {
                    bracket_max = u;
                }
                if fu <= fw || w == x {
                    // w == x means w, fw, dw have not been set yet.
                    (v, fv, dv) = (w, fw, dw);
                    (w, fw, dw) = (u, fu, du);
                } else if fu < fv || v == x || v == w {
                    (v, fv, dv) = (u, fu, du);
                }
            }
            iter += 1;
        }

        // Should have returned within the loop.
        Err(SolveError::Failed {
            operation: OP,
            failure: SolveFailure::BrentDidNotConverge {
                iterations: self.max_iter,
                at: x,
                bracket: [bracket_min, bracket_max],
            },
        })
    }

    /// Regula-falsi bisection between `lower_t` (`F < 0`) and `upper_t` (`F > 0`)
    /// for at most `max_division_level` subdivisions; the fraction is clamped to
    /// `[0.15, 0.85]` (`SolveByBisection`). Returns `(t, F(t))`. Fails with
    /// `SM_ERR` when an evaluation fails.
    #[allow(clippy::manual_clamp)] // keep upstream's comparisons
    pub fn solve_by_bisection<E: EvalFunction + ?Sized>(
        &self,
        eval: &mut E,
        max_division_level: u64,
        mut lower_t: f64,
        mut upper_t: f64,
        mut lower_f: f64,
        mut upper_f: f64,
    ) -> Result<(f64, f64), SolveError<E::Error>> {
        const OP: &str = "geom::local_solve1d::LocalSolve1d::solve_by_bisection";
        let mut level = max_division_level;
        loop {
            if level == 0 {
                // Return the better value (low is negative, high is positive).
                return Ok(if lower_f + upper_f >= 0.0 {
                    (lower_t, lower_f)
                } else {
                    (upper_t, upper_f)
                });
            }

            let mut frac = -lower_f / (upper_f - lower_f);
            // (upstream's two comparisons, not f64::clamp)
            if frac < 0.15 {
                frac = 0.15;
            }
            if frac > 0.85 {
                frac = 0.85;
            }
            let mid_t = lower_t + frac * (upper_t - lower_t);

            let scale = 1.0 * mid_t.abs();
            if (mid_t - upper_t).abs() < EFF_ZERO_SQ * scale
                || (mid_t - lower_t).abs() < EFF_ZERO_SQ * scale
            {
                return Ok(if lower_f.abs() < upper_f.abs() {
                    (lower_t, lower_f)
                } else if upper_f.abs() < lower_f.abs() {
                    (upper_t, upper_f)
                } else {
                    // They can be identical: use the midpoint.
                    ((lower_t + upper_t) / 2.0, (lower_f + upper_f) / 2.0)
                });
            }

            let mut val = FunctionValue::default();
            eval.evaluate(mid_t, &mut val, true)
                .map_err(|err| eval_failed(err, OP))?;
            let mid_f = val.f;
            if val.found_answer || mid_f == 0.0 {
                return Ok((mid_t, mid_f));
            }

            // Continue with whichever interval brackets zero.
            if mid_f > 0.0 {
                upper_t = mid_t;
                upper_f = mid_f;
            } else {
                lower_t = mid_t;
                lower_f = mid_f;
            }
            level -= 1;
        }
    }
}

/// One solve in progress: the options, the accuracy the call accepts, and the state.
struct Run {
    o: LocalSolve1d,
    acceptable_accuracy: f64,
    st: Solve1dState,
}

impl Run {
    /// Evaluates at `value.t` into `value.f` / `value.f_prime` and `found`. A failed
    /// evaluation has no value, so `value` and `found` keep what they held.
    ///
    /// Upstream copies the evaluator's outputs before returning its error, and an
    /// evaluator that zeroes them first (`SmCurvePropertyEFO::Evaluate`) therefore hands
    /// the iteration a zero of `F` where its objective cannot be evaluated at all
    /// (WR-253, #2574).
    fn eval_into<E: EvalFunction + ?Sized>(
        eval: &mut E,
        value: &mut IterationValue,
        found: &mut bool,
        signal_errors: bool,
    ) -> Result<(), E::Error> {
        let mut val = FunctionValue {
            f: value.f,
            f_prime: value.f_prime,
            found_answer: *found,
        };
        eval.evaluate(value.t, &mut val, signal_errors)?;
        value.f = val.f;
        value.f_prime = val.f_prime;
        *found = val.found_answer;
        Ok(())
    }

    /// Resolves an oscillating Newton iteration between `prev1` and `curr`: look
    /// for a sign change in between and bisect, else Brent (min/max modes) or the
    /// best interior value (`SolveOscillating`). Leaves the result in `curr`.
    fn solve_oscillating<E: EvalFunction + ?Sized>(
        &mut self,
        eval: &mut E,
        guess_t: f64,
    ) -> Result<SolveOutcome, SolveError<E::Error>> {
        const GUESSES: [f64; 8] = [0.25, 0.75, 0.13, 0.38, 0.82, 0.87, 0.01, 0.99];
        let mut found_answer = false;

        let mut temp = IterationValue {
            t: 0.5 * (self.st.prev1.t + self.st.curr.t),
            ..IterationValue::default()
        };
        Self::eval_into(eval, &mut temp, &mut found_answer, true).map_err(SolveError::Eval)?;

        let mut best_interior_t = temp.t;
        let mut best_interior_f = temp.f;

        // Not bracketed: look for a sign change between prev1 and curr.
        if !self.st.have_lower_bracket || !self.st.have_upper_bracket {
            let curr_t = self.st.curr.t;
            let prev_t = self.st.prev1.t;
            let curr_f = self.st.curr.f;

            let mut evaluated_sign_change = false;
            for g in GUESSES {
                let mid_t = prev_t + g * (curr_t - prev_t);
                temp.t = mid_t;
                let r = Self::eval_into(eval, &mut temp, &mut found_answer, true)
                    .map_err(SolveError::Eval);
                if ignored(r)?.is_none() {
                    continue; // just keep trying
                }
                if temp.f.abs() < best_interior_f.abs() {
                    best_interior_f = temp.f;
                    best_interior_t = mid_t;
                }
                if temp.f * curr_f <= 0.0 {
                    evaluated_sign_change = true;
                    break; // opposite sign
                }
            }

            // Only a guess that evaluated can show a sign change; upstream tests the
            // last guess's `F` whether its evaluation succeeded or not (WR-253).
            if evaluated_sign_change {
                // Curr and prev have the same sign, opposite to mid: pick the end on
                // the side of the given guess.
                let pick_curr = if guess_t < temp.t {
                    curr_t < prev_t
                } else {
                    curr_t > prev_t
                };
                let end = if pick_curr {
                    (curr_t, curr_f)
                } else {
                    (prev_t, self.st.prev1.f)
                };
                let ((mut new_lo_t, mut new_lo_f), (mut new_hi_t, mut new_hi_f)) = if temp.f > 0.0 {
                    (end, (temp.t, temp.f))
                } else {
                    ((temp.t, temp.f), end)
                };

                // Prefer an existing bracket when it is closer.
                if self.st.have_lower_bracket
                    && (self.st.lower_bracket_t - new_hi_t).abs() < (new_lo_t - new_hi_t).abs()
                {
                    new_lo_t = self.st.lower_bracket_t;
                    new_lo_f = self.st.lower_bracket_f;
                }
                if self.st.have_upper_bracket
                    && (self.st.upper_bracket_t - new_lo_t).abs() < (new_hi_t - new_lo_t).abs()
                {
                    new_hi_t = self.st.upper_bracket_t;
                    new_hi_f = self.st.upper_bracket_f;
                }

                self.st.have_lower_bracket = true;
                self.st.lower_bracket_t = new_lo_t;
                self.st.lower_bracket_f = new_lo_f;
                self.st.have_upper_bracket = true;
                self.st.upper_bracket_t = new_hi_t;
                self.st.upper_bracket_f = new_hi_f;
            }
        }

        // Bracketed: bisect (deeper, we might not be close). Success even if F is
        // too big: at a discontinuity the caller wants the best solution.
        if self.st.have_lower_bracket && self.st.have_upper_bracket {
            let r = self.o.solve_by_bisection(
                eval,
                40,
                self.st.lower_bracket_t,
                self.st.upper_bracket_t,
                self.st.lower_bracket_f,
                self.st.upper_bracket_f,
            );
            if let Some((t, f)) = ignored(r)? {
                self.st.curr.t = t;
                self.st.curr.f = f;
                self.st.found_accuracy = Some(self.st.curr.f);
                self.st.termination_reason = if self.st.curr.f.abs() < self.o.desired_accuracy {
                    TerminationReason::FoundAnswerConverged
                } else {
                    TerminationReason::FoundAnswerClose
                };
                return Ok(SolveOutcome {
                    found: true,
                    t: self.st.curr.t,
                });
            }
        }

        // No bracket: oscillating between two values of the same sign.
        if self.o.algorithm.is_brent() {
            let maximize = self.o.algorithm == SolverAlgorithm::BrentMaximize;
            let (t, _) = self.o.solve_by_brent(
                eval,
                self.st.prev1.t,
                best_interior_t,
                self.st.curr.t,
                self.o.desired_accuracy,
                maximize,
            )?;
            self.st.curr.t = t;
        } else if best_interior_f.abs() < self.st.curr.f.abs() {
            self.st.curr.t = best_interior_t;
        } else {
            // just try the midpoint
            self.st.curr.t = 0.5 * (self.st.prev1.t + self.st.curr.t);
        }

        // The caller expects the solution in curr.
        Self::eval_into(eval, &mut self.st.curr, &mut found_answer, true)
            .map_err(SolveError::Eval)?;

        let found = if self.o.algorithm.is_brent() {
            true // for min/max the tolerance does not matter
        } else {
            found_answer || self.st.curr.f.abs() <= self.acceptable_accuracy
        };
        Ok(SolveOutcome {
            found,
            t: self.st.curr.t,
        })
    }

    /// [`LocalSolve1d::solve_it`]'s iteration.
    fn solve<E: EvalFunction + ?Sized>(
        &mut self,
        eval: &mut E,
        guess_t: f64,
    ) -> Result<SolveOutcome, SolveError<E::Error>> {
        const OP: &str = "geom::local_solve1d::LocalSolve1d::solve_it";

        // The initial guess must be within the problem interval (clamped if not).
        let mut guess_t = guess_t;
        if let Some(ivl) = self.o.domain.interval() {
            if !ivl.contains_value(guess_t, 0.0) {
                // (upstream only logs SM_ERR_INVALID_INPUT beyond EFF_ZERO_SQRT and goes on)
                guess_t = ivl.clamp_value(guess_t);
            }
        }

        // Evaluate at the guess.
        self.st.curr.t = guess_t;
        let mut found_answer = false;
        let r = Self::eval_into(eval, &mut self.st.curr, &mut found_answer, false);
        if let Err(e) = r {
            if e.ends_solve() {
                return Err(SolveError::Eval(e));
            }
            // Bad start point: within an interval, tweak it by 1/1000 of the interval.
            if let Some(ivl) = self.o.domain.interval() {
                let size = ivl.length();
                self.st.curr.t = guess_t + size / 1000.0;
                if !ivl.contains_value(self.st.curr.t, 0.0) {
                    self.st.curr.t = guess_t - size / 1000.0;
                }
                Self::eval_into(eval, &mut self.st.curr, &mut found_answer, true)
                    .map_err(|err| eval_failed(err, OP))?;
            } else {
                // No other start point: the failed evaluation left `curr` with no
                // function value to iterate from, so its status is returned. Upstream
                // iterates from it (WR-253; the fork's tangency-labs/smlib@d2757e1).
                return Err(SolveError::Eval(e));
            }
        }

        // Serendipity: the guess is a solution.
        if found_answer {
            self.st.found_accuracy = Some(self.st.curr.f);
            self.st.termination_reason = TerminationReason::FoundAnswerConverged;
            return Ok(SolveOutcome {
                found: true,
                t: self.st.curr.t,
            });
        }

        let mut cut_factor = 1.0;
        let mut minimum = self.st.curr.f.abs();
        let mut min_count: u64 = 0;
        let mut queue = IterQueue1d::new(8);

        self.st.iter = 0;
        while self.st.iter < self.o.max_iter {
            // Brackets of zero crossings (used for bisection after Newton).
            if self.st.curr.f < 0.0 {
                self.st.have_lower_bracket = true;
                if self.st.curr.f > self.st.lower_bracket_f {
                    self.st.lower_bracket_t = self.st.curr.t;
                    self.st.lower_bracket_f = self.st.curr.f;
                }
            }
            if self.st.curr.f > 0.0 {
                self.st.have_upper_bracket = true;
                if self.st.curr.f < self.st.upper_bracket_f {
                    self.st.upper_bracket_t = self.st.curr.t;
                    self.st.upper_bracket_f = self.st.curr.f;
                }
            }

            // Converged.
            if self.st.curr.f.abs() < self.o.desired_accuracy {
                self.st.found_accuracy = Some(self.st.curr.f);
                self.st.termination_reason = TerminationReason::FoundAnswerConverged;
                return Ok(SolveOutcome {
                    found: true,
                    t: self.st.curr.t,
                });
            }

            if queue.is_oscillating(2) {
                return self.solve_oscillating(eval, guess_t);
            }

            // f'(t) near zero: use +/-1 to nudge in the right direction.
            if in_tol(self.st.curr.f_prime, EFF_ZERO) {
                self.st.curr.f_prime = if self.st.curr.f_prime < 0.0 {
                    -1.0
                } else {
                    1.0
                };
            }

            // Newton step, never huge (even 0.10 can be too big).
            let mut delta_t = -self.st.curr.f / self.st.curr.f_prime;
            if let Some(ivl) = self.o.domain.interval() {
                let mut step_limit = ivl.length() * 0.08 + EFF_ZERO;
                if delta_t.abs() > step_limit {
                    step_limit *= 0.99; // just to avoid exact half
                    if delta_t < 0.0 {
                        step_limit = -step_limit;
                    }
                    delta_t = step_limit;
                }
            }

            // Before giving up, cut the step size and allow 20 more tries.
            if self.st.iter > 20 && self.st.iter >= self.o.max_iter - 1 {
                cut_factor *= 0.5;
                if cut_factor > 0.0001 {
                    self.st.iter -= 20;
                }
            }

            delta_t *= cut_factor;
            let mut new_t = self.st.curr.t + delta_t;

            // The next guess leaves the problem domain.
            let left = self
                .o
                .domain
                .interval()
                .filter(|ivl| !ivl.contains_value(new_t, 0.0));
            if let Some(ivl) = left {
                if matches!(self.o.domain, Domain1d::Periodic(_)) {
                    new_t = ivl.periodic_wrap(new_t);
                } else {
                    // Clamp and count boundary hits; reset the other side's count.
                    if new_t < ivl.min() {
                        new_t = ivl.min();
                        self.st.lower_bound_hits += 1;
                        self.st.upper_bound_hits = 0;
                    } else if new_t > ivl.max() {
                        new_t = ivl.max();
                        self.st.upper_bound_hits += 1;
                        self.st.lower_bound_hits = 0;
                    }
                    if self.st.lower_bound_hits > MAX_BOUNDARY_HITS
                        || self.st.upper_bound_hits > MAX_BOUNDARY_HITS
                    {
                        // (upstream also sets rdFoundT = new_t, overwritten below)
                        self.st.termination_reason = TerminationReason::OutOfBounds;
                        break;
                    }
                }
            } else {
                self.st.lower_bound_hits = 0;
                self.st.upper_bound_hits = 0;
            }

            // Save the last two iterations.
            self.st.curr.prev_step = if self.st.iter > 0 {
                self.st.curr.t - self.st.prev1.t
            } else {
                0.0
            };
            self.st.prev2 = if self.st.iter > 1 {
                self.st.prev1
            } else {
                self.st.curr
            };
            self.st.prev1 = self.st.curr;
            queue.add(&self.st.prev1);

            // Evaluate into curr.
            self.st.curr.t = new_t;
            Self::eval_into(eval, &mut self.st.curr, &mut found_answer, true)
                .map_err(|err| eval_failed(err, OP))?;

            if found_answer || self.st.curr.f.abs() < self.o.desired_accuracy {
                self.st.found_accuracy = Some(self.st.curr.f);
                self.st.termination_reason = TerminationReason::FoundAnswerConverged;
                return Ok(SolveOutcome {
                    found: true,
                    t: self.st.curr.t,
                });
            }

            // No decrease for 4 iterations: stuck, try bisection.
            if self.st.curr.f.abs() >= minimum {
                min_count += 1;
                if min_count >= 4 && self.st.curr.f.abs() < self.acceptable_accuracy {
                    break;
                }
            } else {
                min_count = 0;
                minimum = self.st.curr.f.abs();
            }

            // Brent min/max once the extremum is roughly bracketed: curr beats both
            // previous values.
            if self.o.algorithm.is_brent() && self.st.iter > 1 {
                let maximize = self.o.algorithm == SolverAlgorithm::BrentMaximize;
                let beats = |a: f64, b: f64| if maximize { a > b } else { a < b };
                if beats(self.st.curr.f, self.st.prev1.f) && beats(self.st.curr.f, self.st.prev2.f)
                {
                    let mut ext = Extent1d::from_value(self.st.prev1.t);
                    ext.add_value(self.st.prev2.t);
                    if ext.contains_value(self.st.curr.t, 0.0) {
                        let (t, _) = self.o.solve_by_brent(
                            eval,
                            self.st.prev1.t,
                            self.st.curr.t,
                            self.st.prev2.t,
                            self.o.desired_accuracy,
                            maximize,
                        )?;
                        return Ok(SolveOutcome { found: true, t });
                    }
                }
            }

            // Getting larger on the same side of the answer.
            if self.st.curr.f.abs() >= self.st.prev1.f.abs()
                && self.st.curr.f * self.st.prev1.f > 0.0
            {
                if self.st.curr.f != self.st.prev1.f
                    && self.st.curr.t != self.st.prev1.t
                    && self.st.have_lower_bracket
                    && self.st.have_upper_bracket
                {
                    break; // bracketed: try bisection
                }

                // The step went to zero: exit with or without a solution.
                if delta_t.abs() < EFF_ZERO {
                    let f_mag = self.st.curr.f.abs();
                    self.st.found_accuracy = Some(self.st.curr.f);
                    self.st.termination_reason = self.reason_for(f_mag);
                    return Ok(SolveOutcome {
                        found: f_mag < self.acceptable_accuracy,
                        t: self.st.curr.t,
                    });
                }
            }
            self.st.iter += 1;
        }

        // Not converged (iteration limit, out of bounds, or diverging): try
        // bisection on a bracket.
        if self.st.have_lower_bracket && self.st.have_upper_bracket {
            let r = self.o.solve_by_bisection(
                eval,
                20,
                self.st.lower_bracket_t,
                self.st.upper_bracket_t,
                self.st.lower_bracket_f,
                self.st.upper_bracket_f,
            );
            if let Some((t, f)) = ignored(r)? {
                self.st.curr.t = t;
                self.st.curr.f = f;
            }
        }

        if self.st.curr.f.abs() < self.acceptable_accuracy {
            self.st.found_accuracy = Some(self.st.curr.f);
            let f_mag = self.st.curr.f.abs();
            self.st.termination_reason = self.reason_for(f_mag);
            return Ok(SolveOutcome {
                found: true,
                t: self.st.curr.t,
            });
        }

        // Best answer anyway, so that min/max works without convergence.
        self.st.found_accuracy = Some(self.st.curr.f);
        Ok(SolveOutcome {
            found: false,
            t: self.st.curr.t,
        })
    }

    /// Termination reason for a final `|F|` of `f_mag`.
    fn reason_for(&self, f_mag: f64) -> TerminationReason {
        if f_mag < self.o.desired_accuracy {
            TerminationReason::FoundAnswerConverged
        } else if f_mag < self.acceptable_accuracy {
            TerminationReason::FoundAnswerClose
        } else {
            TerminationReason::UnableToConverge
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Run {
        /// A run that no solve has started: what `solve_oscillating` sees when a test
        /// sets the iterates by hand. Nothing is accepted short of a found answer.
        fn fresh(o: LocalSolve1d) -> Self {
            Self {
                o,
                acceptable_accuracy: 0.0,
                st: Solve1dState::START,
            }
        }
    }

    /// The test evaluators' error: the classes the pin's tests use.
    #[derive(Debug, Clone, PartialEq)]
    enum TestErr {
        /// An upstream `SM_ERR` status (pin `SolidError::sm_err`).
        Sm(&'static str),
        /// Pin `SolidError::Unported`.
        Unported,
        /// Pin `SolidError::geom(operation, GeomFailure::Defect { why })`.
        Defect(&'static str, &'static str),
    }

    impl core::fmt::Display for TestErr {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "{self:?}")
        }
    }

    impl EvalError for TestErr {
        fn ends_solve(&self) -> bool {
            matches!(self, Self::Unported)
        }
        fn passes_through(&self) -> bool {
            self.ends_solve()
        }
        fn never_ignored(&self) -> bool {
            self.ends_solve()
        }
        fn defect(operation: &'static str, why: &'static str) -> Self {
            Self::Defect(operation, why)
        }
    }

    /// `F(t) = t^2 - 2`, never failing.
    struct Sq;
    impl EvalFunction for Sq {
        type Error = TestErr;
        fn evaluate(&mut self, t: f64, out: &mut FunctionValue, _: bool) -> Result<(), TestErr> {
            out.f = t * t - 2.0;
            out.f_prime = 2.0 * t;
            out.found_answer = false;
            Ok(())
        }
    }

    /// `F(t) = t^3 - 2t + 2`: Newton from 0 cycles 0, 1, 0, 1, ...
    struct Cycle;
    impl EvalFunction for Cycle {
        type Error = TestErr;
        fn evaluate(&mut self, t: f64, out: &mut FunctionValue, _: bool) -> Result<(), TestErr> {
            out.f = (t * t - 2.0) * t + 2.0;
            out.f_prime = 3.0 * t * t - 2.0;
            out.found_answer = false;
            Ok(())
        }
    }

    /// The two values `Cycle` settles between from 0 within [-3, 3] (Newton's
    /// steps are capped at 0.08 of the interval) and the last interior guess of
    /// `SolveOscillating` between them, 0.99 of the way.
    const CYCLE_PREV: f64 = 0.475_200_000_000_989_94;
    const CYCLE_CURR: f64 = 0.950_400_000_001_979_9;
    const LAST_GUESS: f64 = CYCLE_PREV + 0.99 * (CYCLE_CURR - CYCLE_PREV);

    /// `Cycle`, except that the evaluation at `LAST_GUESS` fails after zeroing its
    /// outputs, as `SmCurvePropertyEFO::Evaluate` does (`SmCurve.cpp:14918`).
    struct CycleFailingAtLastGuess;
    impl EvalFunction for CycleFailingAtLastGuess {
        type Error = TestErr;
        fn evaluate(&mut self, t: f64, out: &mut FunctionValue, s: bool) -> Result<(), TestErr> {
            if t.to_bits() == LAST_GUESS.to_bits() {
                *out = FunctionValue::default();
                return Err(TestErr::Sm("test: no value here"));
            }
            Cycle.evaluate(t, out, s)
        }
    }

    /// `Sq`, except that the evaluation at 1 fails after zeroing its outputs, as
    /// `SmCurvePropertyEFO::Evaluate` does.
    struct SqFailingAtOne;
    impl EvalFunction for SqFailingAtOne {
        type Error = TestErr;
        fn evaluate(&mut self, t: f64, out: &mut FunctionValue, s: bool) -> Result<(), TestErr> {
            if t.to_bits() == 1f64.to_bits() {
                *out = FunctionValue::default();
                return Err(TestErr::Sm("test: no value here"));
            }
            Sq.evaluate(t, out, s)
        }
    }

    /// WR-253 (the fork's tangency-labs/smlib@d2757e1, tangency-labs/solidcore#2574): a
    /// failed first evaluation without an interval returns the evaluator's status, as the
    /// interval case does when its tweaked start fails too. Upstream iterates from it
    /// instead, and its zeroed `F` reads as converged, reporting `t = 1` as a root of
    /// `t^2 - 2`. A good start is unaffected.
    #[test]
    fn failed_first_evaluation_without_interval_has_no_value() {
        let s = LocalSolve1d::new();
        let e = s.solve_it(&mut SqFailingAtOne, 1.0, 1e-9).unwrap_err();
        assert!(
            matches!(e, SolveError::Eval(TestErr::Sm("test: no value here"))),
            "{e:?}"
        );

        // A start whose evaluation succeeds solves as without the flag.
        let r = s.solve_it(&mut SqFailingAtOne, 1.5, 1e-9).unwrap();
        let plain = LocalSolve1d::new().solve_it(&mut Sq, 1.5, 1e-9).unwrap();
        assert_eq!((r.found, r.t.to_bits()), (plain.found, plain.t.to_bits()));
    }

    struct Unp;
    impl EvalFunction for Unp {
        type Error = TestErr;
        fn evaluate(&mut self, _: f64, _: &mut FunctionValue, _: bool) -> Result<(), TestErr> {
            Err(TestErr::Unported)
        }
    }

    struct Base;
    impl EvalFunction for Base {
        type Error = TestErr;
    }

    #[test]
    fn newton_sqrt2() {
        let s = LocalSolve1d::with_interval(Extent1d::from_min_max(0.0, 3.0), false);
        let (r, st) = s.solve_traced(&mut Sq, 1.0, 1e-9);
        let r = r.unwrap();
        assert!(r.found);
        assert!((r.t - 2f64.sqrt()).abs() < 1e-10);
        assert_eq!(
            st.termination_reason,
            TerminationReason::FoundAnswerConverged
        );
    }

    #[test]
    fn oscillation_is_resolved() {
        let s = LocalSolve1d::with_interval(Extent1d::from_min_max(-3.0, 3.0), false);
        let r = s.solve_it(&mut Cycle, 0.0, 1e-9).unwrap();
        assert!(r.t.is_finite());
    }

    /// tangency-labs/solidcore#481 (WR-253): the failed last guess is no sign change, and
    /// the solve finds nothing, as on the clean function. `F = t^3 - 2t + 2` has no root
    /// between the two values Newton cycles on. Upstream reads the `F` the failed
    /// evaluation left behind, brackets that guess with `F = 0` and reports it converged
    /// there, although `F` is about 0.95 at that parameter.
    #[test]
    fn issue_481_failed_guess_is_no_sign_change() {
        let s = LocalSolve1d::with_interval(Extent1d::from_min_max(-3.0, 3.0), false);
        let (clean, clean_st) = s.solve_traced(&mut Cycle, 0.0, 1e-9);
        assert!(!clean.unwrap().found);
        assert_eq!(
            clean_st.termination_reason,
            TerminationReason::UnableToConverge
        );

        let (r, st) = s.solve_traced(&mut CycleFailingAtLastGuess, 0.0, 1e-9);
        let r = r.unwrap();
        assert!(!r.found, "{r:?}");
        assert_ne!(r.t.to_bits(), LAST_GUESS.to_bits());
        assert_ne!(st.lower_bracket_t.to_bits(), LAST_GUESS.to_bits());
        assert_eq!(st.termination_reason, TerminationReason::UnableToConverge);

        // F really is about 0.95 there.
        let mut out = FunctionValue::default();
        Cycle.evaluate(LAST_GUESS, &mut out, true).unwrap();
        assert!(out.f > 0.9);
    }

    /// A failed final sample must not replace the valid, same-sign samples with
    /// a fabricated zero (WR-253 / #481).
    #[test]
    fn failed_guess_does_not_supply_a_sign_change() {
        let interval = Extent1d::from_min_max(-3.0, 3.0);
        let solver = LocalSolve1d::with_interval(interval, false);
        let (clean, clean_st) = solver.solve_traced(&mut Cycle, 0.0, 1e-9);
        let clean = clean.unwrap();
        assert!(!clean.found);
        let (result, st) = solver.solve_traced(&mut CycleFailingAtLastGuess, 0.0, 1e-9);
        let result = result.unwrap();
        assert!(!result.found);
        assert_eq!(result.t.to_bits(), clean.t.to_bits());
        assert_eq!(st.curr.f, clean_st.curr.f);
        assert_eq!(st.termination_reason, TerminationReason::UnableToConverge);
        assert_eq!(st.have_lower_bracket, clean_st.have_lower_bracket);
        assert_eq!(st.have_upper_bracket, clean_st.have_upper_bracket);
    }

    #[test]
    fn failed_interior_samples_cannot_create_a_bracket() {
        struct FailedSamples(f64);
        impl EvalFunction for FailedSamples {
            type Error = TestErr;
            fn evaluate(
                &mut self,
                t: f64,
                out: &mut FunctionValue,
                _: bool,
            ) -> Result<(), TestErr> {
                if [0.25, 0.75, 0.13, 0.38, 0.82, 0.87, 0.01, 0.99].contains(&t) {
                    out.f = self.0;
                    return Err(TestErr::Sm("test: failed interior sample"));
                }
                out.f = t * t + 1.0;
                out.f_prime = 2.0 * t;
                out.found_answer = false;
                Ok(())
            }
        }
        for failed_value in [0.0, -1.0] {
            let mut solver = Run::fresh(LocalSolve1d::new());
            solver.st.prev1 = IterationValue {
                t: 0.0,
                f: 1.0,
                ..IterationValue::default()
            };
            solver.st.curr = IterationValue {
                t: 1.0,
                f: 2.0,
                ..IterationValue::default()
            };
            let result = solver
                .solve_oscillating(&mut FailedSamples(failed_value), 0.9)
                .unwrap();
            assert!(!result.found);
            assert_eq!(solver.st.curr.f, result.t * result.t + 1.0);
            assert!(!solver.st.have_lower_bracket);
            assert!(!solver.st.have_upper_bracket);
        }
    }

    #[test]
    fn successful_sample_after_a_failure_can_still_bracket_a_root() {
        struct FailedQuarter;
        impl EvalFunction for FailedQuarter {
            type Error = TestErr;
            fn evaluate(
                &mut self,
                t: f64,
                out: &mut FunctionValue,
                _: bool,
            ) -> Result<(), TestErr> {
                if t == 0.25 {
                    *out = FunctionValue::default();
                    return Err(TestErr::Sm("test: failed quarter sample"));
                }
                out.f = (t - 0.75) * (t - 0.75) - 0.01;
                out.f_prime = 2.0 * (t - 0.75);
                out.found_answer = false;
                Ok(())
            }
        }
        let mut solver = Run::fresh(LocalSolve1d {
            desired_accuracy: 1e-10,
            ..LocalSolve1d::new()
        });
        solver.st.prev1 = IterationValue {
            t: 0.0,
            f: 0.5525,
            ..IterationValue::default()
        };
        solver.st.curr = IterationValue {
            t: 1.0,
            f: 0.0525,
            ..IterationValue::default()
        };
        let result = solver.solve_oscillating(&mut FailedQuarter, 0.9).unwrap();
        assert!(result.found);
        assert!((result.t - 0.85).abs() < 1e-8);
        assert!(solver.st.curr.f.abs() < 1e-10);
    }

    #[test]
    fn queue_oscillation() {
        let mut q = IterQueue1d::new(8);
        for i in 0..6 {
            let step = if i % 2 == 0 { 1.0 } else { -1.0 };
            assert!(!q.is_oscillating(2));
            q.add(&IterationValue {
                prev_step: step,
                ..IterationValue::default()
            });
        }
        assert!(q.is_oscillating(2));
        assert!(!q.is_oscillating(3));
        let mut z = IterQueue1d::new(0);
        z.add(&IterationValue::default());
        assert!(!z.is_oscillating(2));
        assert_eq!(IterQueue1d::try_new(8).unwrap(), IterQueue1d::new(8));
        // fuzz regression: a caller-chosen size that cannot be allocated fails softly
        assert!(matches!(
            IterQueue1d::try_new(u64::MAX),
            Err(NumError::InvalidInput { .. })
        ));
        assert!(IterQueue1d::try_new(1 << 60).is_err());
    }

    #[test]
    fn bisection_and_brent() {
        let s = LocalSolve1d::new();
        let (t, f) = s
            .solve_by_bisection(&mut Sq, 40, 1.0, 2.0, -1.0, 2.0)
            .unwrap();
        assert!((t - 2f64.sqrt()).abs() < 1e-9 && f.abs() < 1e-8);
        let (t, _) = s
            .solve_by_brent(&mut Sq, -1.0, 0.3, 2.0, 1e-12, false)
            .unwrap();
        assert!(t.abs() < 1e-6);
        assert_eq!(
            s.solve_by_bisection(&mut Sq, 0, 1.0, 2.0, -1.0, 2.0)
                .unwrap(),
            (1.0, -1.0)
        );
    }

    #[test]
    fn errors() {
        let s = LocalSolve1d::with_interval(Extent1d::from_min_max(0.0, 1.0), false);
        assert!(matches!(
            s.solve_it(&mut Unp, 0.5, 1e-9),
            Err(SolveError::Eval(TestErr::Unported))
        ));
        assert!(matches!(
            s.solve_it(&mut Base, 0.5, 1e-9),
            Err(SolveError::CallFailed {
                cause: TestErr::Defect("geom::local_solve1d::EvalFunction::evaluate", _),
                ..
            })
        ));
        assert_eq!(
            SolverAlgorithm::from_code(2),
            Some(SolverAlgorithm::BrentMaximize)
        );
        assert_eq!(SolverAlgorithm::BrentMinimize.code(), 3);
    }

    /// Evaluates `(t) -> (f, f_prime)` and records each evaluated `t`.
    struct RecordingEval<F> {
        func: F,
        evaluated_ts: Vec<f64>,
    }

    impl<F: FnMut(f64) -> (f64, f64)> RecordingEval<F> {
        fn new(func: F) -> Self {
            Self {
                func,
                evaluated_ts: Vec::new(),
            }
        }
    }

    impl<F: FnMut(f64) -> (f64, f64)> EvalFunction for RecordingEval<F> {
        type Error = TestErr;

        fn evaluate(&mut self, t: f64, out: &mut FunctionValue, _: bool) -> Result<(), TestErr> {
            self.evaluated_ts.push(t);
            let (f, f_prime) = (self.func)(t);
            out.f = f;
            out.f_prime = f_prime;
            out.found_answer = false;
            Ok(())
        }
    }

    /// WR-19 (#2150): solving without an interval skips the interval-length clamp and converges cleanly on
    /// linear, nonlinear positive/negative, and large-scale roots.
    #[test]
    fn wr19_unbounded_newton_converges_positive_and_negative_roots() {
        // Linear `f(t) = t - 3.0` from `guess_t = 0.0` converges in 1 Newton step.
        let s_lin = LocalSolve1d::new();
        let mut eval_lin = RecordingEval::new(|t| (t - 3.0, 1.0));
        let (out_lin, st_lin) = s_lin.solve_traced(&mut eval_lin, 0.0, 1e-9);
        let out_lin = out_lin.unwrap();
        assert!(out_lin.found);
        assert!((out_lin.t - 3.0).abs() < 1e-12);
        assert_eq!(eval_lin.evaluated_ts, vec![0.0, 3.0]);
        assert_eq!(
            st_lin.termination_reason,
            TerminationReason::FoundAnswerConverged
        );

        // Nonlinear positive root `f(t) = t^3 - 27.0` from `guess_t = 10.0`.
        let s_pos = LocalSolve1d::new();
        let mut eval_pos = RecordingEval::new(|t| (t * t * t - 27.0, 3.0 * t * t));
        let out_pos = s_pos.solve_it(&mut eval_pos, 10.0, 1e-9).unwrap();
        assert!(out_pos.found);
        assert!((out_pos.t - 3.0).abs() < 1e-12);
        assert!(eval_pos.evaluated_ts.len() <= 10);

        // Nonlinear negative root `f(t) = t^3 + 125.0` from `guess_t = -10.0`
        let s_neg = LocalSolve1d::new();
        let mut eval_neg = RecordingEval::new(|t| (t * t * t + 125.0, 3.0 * t * t));
        let out_neg = s_neg.solve_it(&mut eval_neg, -10.0, 1e-9).unwrap();
        assert!(out_neg.found);
        assert!((out_neg.t + 5.0).abs() < 1e-12);
        assert!(eval_neg.evaluated_ts.len() <= 10);

        // Large-scale root `f(t) = t - 1.0e6` from `guess_t = 0.0` without step clamping.
        let s_large = LocalSolve1d::new();
        let mut eval_large = RecordingEval::new(|t| (t - 1.0e6, 1.0));
        let out_large = s_large.solve_it(&mut eval_large, 0.0, 1e-9).unwrap();
        assert!(out_large.found);
        assert!((out_large.t - 1.0e6).abs() < 1e-12);
        assert_eq!(eval_large.evaluated_ts, vec![0.0, 1.0e6]);
    }

    /// WR-19 (#2150): when `use_interval` is `true`, `with_interval` still
    /// clamps Newton steps exceeding `0.08 * interval.length() + EFF_ZERO` by `0.99`,
    /// matching upstream bit-for-bit.
    #[test]
    fn wr19_bounded_interval_still_clamps_newton_step() {
        let interval = Extent1d::from_min_max(0.0, 10.0);
        let expected_clamped_step = (interval.length() * 0.08 + EFF_ZERO) * 0.99;

        // Positive step (`delta_t = +9.0 > 0.8` from `guess_t = 0.0`): first step `0.792`.
        let s_on = LocalSolve1d::with_interval(interval, false);
        let mut eval_on = RecordingEval::new(|t| (t - 9.0, 1.0));
        let out_on = s_on.solve_it(&mut eval_on, 0.0, 1e-9).unwrap();
        assert_eq!(eval_on.evaluated_ts[1], expected_clamped_step);
        assert!((eval_on.evaluated_ts[1] - 0.792).abs() < 1e-11);
        assert!(out_on.found);
        assert!((out_on.t - 9.0).abs() < 1e-12);

        // Negative step (`delta_t = -9.0 < -0.8` from `guess_t = 10.0`): first step `9.208`.
        let s_on_neg = LocalSolve1d::with_interval(interval, false);
        let mut eval_on_neg = RecordingEval::new(|t| (t - 1.0, 1.0));
        let out_on_neg = s_on_neg.solve_it(&mut eval_on_neg, 10.0, 1e-9).unwrap();
        assert_eq!(eval_on_neg.evaluated_ts[1], 10.0 - expected_clamped_step);
        assert!((eval_on_neg.evaluated_ts[1] - 9.208).abs() < 1e-11);
        assert!(out_on_neg.found);
        assert!((out_on_neg.t - 1.0).abs() < 1e-12);
    }

    /// WR-253's "a failed evaluation has no value" (#2574) and WR-19's unbounded
    /// steps without an interval.
    #[test]
    fn wr19_failed_first_evaluation_and_valid_start() {
        let s = LocalSolve1d::new();

        // Failed first evaluation without an interval returns `Err` (WR-253), and a
        // valid start now converges to `sqrt(2)` instead of stepping `-1.584e19` (WR-19).
        let err = s.solve_it(&mut SqFailingAtOne, 1.0, 1e-9).unwrap_err();
        assert!(
            matches!(err, SolveError::Eval(TestErr::Sm("test: no value here"))),
            "{err:?}"
        );
        let ok = s.solve_it(&mut SqFailingAtOne, 1.5, 1e-9).unwrap();
        assert!(ok.found);
        assert!((ok.t - 2.0_f64.sqrt()).abs() < 1e-12);

        // With an interval, WR-253's rule holds too.
        let s_ivl = LocalSolve1d::with_interval(Extent1d::from_min_max(-3.0, 3.0), false);
        let r_ivl = s_ivl
            .solve_it(&mut CycleFailingAtLastGuess, 0.0, 1e-9)
            .unwrap();
        assert!(!r_ivl.found);
        assert_ne!(r_ivl.t.to_bits(), LAST_GUESS.to_bits());
    }
}
