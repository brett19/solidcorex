// Moved from solidcore `solidcore/crates/solidcore/src/geom/local_solve1d.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/inc/SmLocalSolve1d.h`,
// `source/SMLib/src/SmLocalSolve1d.cpp`.
// Changes: the evaluator's error is generic (classified by `EvalError`; owner decision
// 2026-11-26, report 53 section 12), so `LocalSolve1d::solve_by_brent`,
// `LocalSolve1d::solve_by_bisection`, `LocalSolve1d::solve` and `IterQueue1d::try_new` (a
// `NumError`) return typed errors and test the class where the pin matched `SolidError`
// variants. The solver is an options value and a pure call over a closure (04 rows 89 and
// 90, research 57 M6): the `EvalFunction` trait (and its failing default `evaluate`),
// `FunctionValue` (and its `found_answer`), the `signal_errors` flag, `SolveOutcome`,
// `eval_failed`, `LocalSolve1d::eval_into`, `LocalSolve1d::with_interval`,
// `LocalSolve1d::set_solution_interval`, `LocalSolve1d::set_solver_algorithm`,
// `LocalSolve1d::set_desired_accuracy`, the getters (`LocalSolve1d::found_accuracy`,
// `termination_reason`, `lower_bound_hits`, `upper_bound_hits`, `interval`,
// `desired_accuracy`), `UNDEF_DOUBLE`, `UNDEF_ULONG`, `SolverAlgorithm::Bisection`,
// `SolverAlgorithm::code`, `SolverAlgorithm::from_code` and `SolverAlgorithm::is_brent`
// are removed; `LocalSolve1d::solve_it` is `LocalSolve1d::solve` (the acceptable accuracy
// is the option `LocalSolve1d::acceptable_accuracy`); `LocalSolve1d` holds only the options
// (`Domain1d` replaces `use_interval`, `interval` and `is_periodic`; the convergence test
// `Converged1d` replaces the evaluator's found flag); the solver's members after a solve are
// `Solve1dState` (brackets as `Option<Sample>`, the iterates as `Option<IterationValue>`,
// `found_accuracy` an `Option`), returned by the test-and-`trace`-feature
// `LocalSolve1d::solve_traced`; the iteration is the private `Run::solve`, with
// `Run::solve_oscillating` and `Run::reason_for`; `max_boundary_hits` is the constant
// `MAX_BOUNDARY_HITS` and the bound hits start at 0; `SolverAlgorithm::Brent` carries a
// `Goal`; `solve_by_brent` takes a `Bracket3` and a `Goal` and returns a `Sample`;
// `solve_by_bisection` takes a derivative-free closure and two `Sample`s; `IterQueue1d`
// keeps the steps only, its size is a `NonZeroUsize` and `IterQueue1d::is_oscillating`
// takes no period (only 2 ever worked). `Goal`, `Sample`, `Bracket3` and `Converged1d` are
// new. The tests use a test error (`TestErr`, with `fmt`, `ends_solve`, `passes_through`
// and `never_ignored`) and closures; `tests::errors` loses its failing-default and code
// checks and `tests::queue_oscillation` its size-0 and period-3 checks (the types no longer
// allow them); `tests::convergence_test_decides_found` is new; every test that read the
// state reads `solve_traced`'s or a test `Run::at`'s. Results and every error's class, text
// and cause unchanged, but for `solve_by_brent`'s returned `F` when maximising and the
// lower bracket at `F <= -SM_BIG_DOUBLE` (see the deviations).
//! One-dimensional local root finding (damped Newton iteration with oscillation
//! detection, regula-falsi bisection fallback) and Brent-like minimisation /
//! maximisation, over a caller-supplied function.
//!
//! Upstream: `source/SMLib/inc/SmLocalSolve1d.h`, `source/SMLib/src/SmLocalSolve1d.cpp`.
//!
//! | Rust                                         | Upstream                                       |
//! |----------------------------------------------|------------------------------------------------|
//! | [`SolverAlgorithm`], [`Goal`]                | `SmLocalSolverAlgorithmType`                   |
//! | [`TerminationReason`] (re-export)            | `SmTerminationReasonType` (defined in `local_solve_nd`) |
//! | the closure passed to [`LocalSolve1d::solve`] | `SmEvalFunctionObject::Evaluate`              |
//! | [`Converged1d`]                              | `Evaluate`'s `rbFoundAnswer`                   |
//! | [`IterationValue`]                           | `SmIterationValue`                             |
//! | [`IterQueue1d`], [`IterQueue1d::new`], [`IterQueue1d::try_new`] | `SmIterQueue1d`, `SmIterQueue1d(ULONG)` |
//! | [`IterQueue1d::add`]                         | `SmIterQueue1d::Add`                           |
//! | [`IterQueue1d::is_oscillating`]              | `SmIterQueue1d::IsOscillating`                 |
//! | [`LocalSolve1d`] (the options), [`Domain1d`], [`LocalSolve1d::new`] | `SmLocalSolve1d`'s settings, constructors and setters |
//! | [`Solve1dState`]                             | `SmLocalSolve1d`'s members after a solve, `GetFoundAccuracy`, `GetTerminationReason`, `GetLowerBoundHits`, `GetUpperBoundHits` |
//! | [`LocalSolve1d::solve`], [`Outcome`]         | `SolveIt`                                      |
//! | [`LocalSolve1d::solve_by_bisection`], [`Sample`] | `SolveByBisection`                         |
//! | [`LocalSolve1d::solve_by_brent`], [`Bracket3`] | `SolveByBrent`                               |
//! | `solve_oscillating` (private)                | `SolveOscillating`                             |
//!
//! Deviations:
//! - The solver is a value of options and each solve a pure call: the function is a
//!   closure `FnMut(f64) -> Result<(f64, f64), E>` returning `(F, F')` (derivative-free,
//!   `FnMut(f64) -> Result<f64, E>`, for [`LocalSolve1d::solve_by_bisection`]), every
//!   [`LocalSolve1d::solve`] starts from the same state, and whether a point solves `F`
//!   beyond the residual tests is the options' [`Converged1d`], not the evaluator's.
//!   Upstream carries the members from one `SolveIt` to the next, but `SolveIt` resets
//!   every one it reads before reading it, but for the lower bracket below.
//!   `SetSolutionInterval` on a solver built without one changes nothing upstream;
//!   [`Domain1d::Line`] has no interval to change. `SM_SA_BISECTION` is not kept: upstream
//!   reads the algorithm only to tell the Brent modes from the rest, so it solves as
//!   `SM_SA_NEWTON`.
//! - Out-params become return values: [`Outcome`] for `SolveIt` ([`Outcome::Best`]
//!   where upstream's `rbFoundSolution` is false), [`Sample`] for
//!   `SolveByBisection`/`SolveByBrent`. On failure nothing is returned (upstream callers
//!   do not read the out-params after an error).
//! - `SolveByBrent` returns `F` as evaluated, as its documentation says; upstream's code
//!   returns the negated `F` when maximising (no upstream caller reads it).
//! - The lower bracket starts unset where upstream starts its `F` at `-SM_BIG_DOUBLE`
//!   with an undefined parameter: a first negative `F <= -SM_BIG_DOUBLE` sets it here,
//!   where upstream flags a bracket at the undefined parameter `SM_UNDEF_DOUBLE`.
//! - Status codes: upstream returns either the evaluator's status or `SM_ERR`
//!   (1001, [`SolveError`]) exactly as in C++. An evaluator error that
//!   [ends the solve](EvalError::ends_solve) is always propagated unchanged,
//!   also where upstream ignores or retries a failed evaluation (so callers that
//!   reach unported code report "deferred" instead of a different result).
//! - `SolveByBisection` recursion is a loop (same arithmetic and order).
//! - [`IterQueue1d::try_new`] (caller-chosen sizes) returns `InvalidInput` when the
//!   slots cannot be allocated or would take more than
//!   [`scx_num::limits::MAX_ALLOC_BYTES`] (upstream's `SetSize` fails softly and `Add`
//!   then writes out of bounds; upstream's only queue has 8 slots). A queue keeps only
//!   the steps, the one value `IsOscillating` reads.
//! - `SE(...)`/`SER` error-log side effects and `SM_DEBUG_CODE` tracing are not
//!   ported.
//! - A failed evaluation has no value (WR-253, tangency-labs/solidcore#481 and #2574):
//!   the iteration keeps the last evaluated values, `SolveOscillating` takes a sign
//!   change only from an interior guess that evaluated, and `SolveIt` without an interval
//!   returns a failed first evaluation's error instead of iterating from it. Upstream
//!   copies whatever the evaluator wrote before failing, so an evaluator that zeroes its
//!   outputs first hands the iteration a zero of `F`.
//! - Without an interval, `SolveIt` takes unclamped Newton steps (WR-19): upstream
//!   clamps each step against the default inside-out `[1e20, -1e20]` interval, replacing
//!   every step with `+-1.584e19` against the Newton direction.

use crate::error::{ignored, EvalError, SolveError, SolveFailure};
pub use crate::local_solve_nd::{Outcome, Residual, TerminationReason};
use core::num::NonZeroUsize;
use scx_num::error::Limit;
use scx_num::extent1d::Extent1d;
use scx_num::limits::try_filled;
use scx_num::math::{EFF_ZERO, EFF_ZERO_SQ};
use scx_num::tol::in_tol;
use scx_num::NumError;

/// `ZEPS` of `SolveByBrent`.
const ZEPS: f64 = 1.0e-12;

/// `SMSIGN(a, b)`: `|a|` with the sign of `b` (`b >= 0` counts as positive).
fn sign(a: f64, b: f64) -> f64 {
    if b >= 0.0 {
        a.abs()
    } else {
        -a.abs()
    }
}

/// Which extremum a Brent search looks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Goal {
    /// The smallest `F` (`SM_SA_BRENT_MINIMIZE`).
    Minimize,
    /// The largest `F` (`SM_SA_BRENT_MAXIMIZE`).
    Maximize,
}

/// Algorithm used by the solver (`SmLocalSolverAlgorithmType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SolverAlgorithm {
    /// A root of `F` (`SM_SA_NEWTON`, the default; also upstream's `SM_SA_BISECTION`).
    #[default]
    Newton,
    /// A root of `F`, finishing with a Brent search for the extremum `Goal` once one is
    /// bracketed (`SM_SA_BRENT_MAXIMIZE`, `SM_SA_BRENT_MINIMIZE`).
    Brent(Goal),
}

impl SolverAlgorithm {
    /// The Brent goal, if any.
    #[must_use]
    pub const fn goal(self) -> Option<Goal> {
        match self {
            Self::Newton => None,
            Self::Brent(g) => Some(g),
        }
    }
}

/// A parameter and `F` there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// The parameter.
    pub t: f64,
    /// `F(t)`.
    pub f: f64,
}

/// A Brent bracket: the search starts at `b`, between the ends `a` and `c` (either
/// order); `F(b)` should beat `F(a)` and `F(c)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bracket3 {
    /// One end.
    pub a: f64,
    /// The start.
    pub b: f64,
    /// The other end.
    pub c: f64,
}

/// A function's own convergence test, beside the solver's residual tests.
///
/// A closure `Fn(f64, f64) -> bool` is one, deciding from `t` and `F(t)`; [`Residual`]
/// declares nothing solved.
pub trait Converged1d {
    /// Whether `t`, with `F(t) = f`, solves `F` whatever the residual says (upstream:
    /// `Evaluate`'s `rbFoundAnswer`).
    fn converged(&self, t: f64, f: f64) -> bool;
}

impl Converged1d for Residual {
    fn converged(&self, _: f64, _: f64) -> bool {
        false
    }
}

impl<F: Fn(f64, f64) -> bool> Converged1d for F {
    fn converged(&self, t: f64, f: f64) -> bool {
        self(t, f)
    }
}

/// One iteration's values (`SmIterationValue`).
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

/// Fixed-size ring of the most recent iteration steps (`SmIterQueue1d`): once full,
/// each addition overwrites the oldest slot.
#[derive(Debug, Clone, PartialEq)]
pub struct IterQueue1d {
    steps: Vec<f64>,
    count: usize,
    next_idx: usize,
    full: bool,
}

impl IterQueue1d {
    /// An empty queue of `size` slots (`SmIterQueue1d(ULONG)`).
    ///
    /// For small sizes known at the call site. Like any `Vec`, it aborts when the
    /// slots cannot be allocated; use [`IterQueue1d::try_new`] for sizes that come
    /// from caller input.
    #[must_use]
    pub fn new(size: NonZeroUsize) -> Self {
        Self::with_slots(vec![0.0; size.get()])
    }

    /// An empty queue of `size` slots (`SmIterQueue1d(ULONG)`) for sizes that come
    /// from caller input.
    ///
    /// # Errors
    ///
    /// [`NumError::Exhausted`] when `size` slots cannot be allocated or would take
    /// more than [`scx_num::limits::MAX_ALLOC_BYTES`] (upstream's `SetSize` then fails
    /// softly and `Add` writes past the empty array).
    pub fn try_new(size: NonZeroUsize) -> Result<Self, NumError> {
        let count = size.get();
        let steps = try_filled(count, 0.0).ok_or(NumError::Exhausted {
            operation: "geom::local_solve1d::IterQueue1d::try_new",
            limit: Limit::Allocation { count },
        })?;
        Ok(Self::with_slots(steps))
    }

    const fn with_slots(steps: Vec<f64>) -> Self {
        Self {
            steps,
            count: 0,
            next_idx: 0,
            full: false,
        }
    }

    /// Stores `prev_step`, the step that led to an iterate, in the next slot (`Add`).
    pub fn add(&mut self, prev_step: f64) {
        let size = self.steps.len();
        if let Some(slot) = self.steps.get_mut(self.next_idx) {
            *slot = prev_step;
        }
        self.next_idx += 1;
        if self.next_idx >= size {
            self.next_idx = 0;
            self.full = true;
        }
        if self.count < size {
            self.count += 1;
        }
    }

    /// Whether the stored steps alternate with period 2 over at least three cycles: the
    /// alternating sums of the steps (by slot index) nearly cancel (`IsOscillating` with
    /// its default, and only working, period 2).
    #[must_use]
    pub fn is_oscillating(&self) -> bool {
        const STEP: usize = 2;
        if self.count < 3 * STEP {
            return false;
        }
        let top = if self.full {
            self.steps.len()
        } else {
            self.next_idx
        };
        let mut sum1 = 0.0;
        let mut sum2 = 0.0;
        for (i, &v) in self.steps.iter().enumerate().take(top) {
            if i % STEP == 0 {
                sum1 += v;
            } else {
                sum2 += v;
            }
        }
        let sum_all = (sum1 + sum2).abs();
        let sum_abs = sum1.abs() + sum2.abs();
        sum_all < sum_abs / 100.0
    }
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

/// The iteration's queue length (upstream's `SmIterQueue1d queue(8)`).
const QUEUE_SLOTS: NonZeroUsize = NonZeroUsize::MIN.saturating_add(7);

/// The options of a one-dimensional local solve (`SmLocalSolve1d`'s settings). A value:
/// every solve ([`LocalSolve1d::solve`], [`LocalSolve1d::solve_by_brent`],
/// [`LocalSolve1d::solve_by_bisection`]) reads it and changes nothing, so one value may
/// serve any number of solves, in any order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalSolve1d<C = Residual> {
    /// Where `t` is looked for (default: the whole line).
    pub domain: Domain1d,
    /// The algorithm (default: Newton).
    pub algorithm: SolverAlgorithm,
    /// `|F|` below which iteration stops as converged (default `SM_EFF_ZERO`).
    pub desired_accuracy: f64,
    /// `|F|` accepted as a solution when the desired accuracy is not reached (default
    /// `SM_EFF_ZERO`; upstream's `SolveIt` argument).
    pub acceptable_accuracy: f64,
    /// The iteration limit (default 100).
    pub max_iter: u64,
    /// The function's own convergence test (default [`Residual`]).
    pub converged: C,
}

impl Default for LocalSolve1d {
    fn default() -> Self {
        Self::new()
    }
}

/// What one `LocalSolve1d::solve_traced` (tests and the `trace` feature) left behind:
/// its brackets, last iterates, counters and why it stopped (upstream's members after
/// `SolveIt`). Every solve starts from the same state; none reads a previous one's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Solve1dState {
    /// The best known sample with `F < 0` (the lower side of a zero crossing).
    pub lower: Option<Sample>,
    /// The best known sample with `F > 0` (the upper side of a zero crossing).
    pub upper: Option<Sample>,
    /// Values two iterations ago (`m_v2Prev`).
    pub prev2: Option<IterationValue>,
    /// Values one iteration ago (`m_v1Prev`).
    pub prev1: Option<IterationValue>,
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
    /// The state a solve starts iterating from, at its first evaluated value
    /// (upstream's resets at the top of `SolveIt`).
    const fn at(curr: IterationValue) -> Self {
        Self {
            lower: None,
            upper: None,
            prev2: None,
            prev1: None,
            curr,
            iter: 0,
            found_accuracy: None,
            lower_bound_hits: 0,
            upper_bound_hits: 0,
            termination_reason: TerminationReason::UnableToConverge,
        }
    }
}

impl LocalSolve1d {
    /// Newton on the whole real line (`SmLocalSolve1d(rFunctionEvaluator)`), desired and
    /// acceptable accuracy `SM_EFF_ZERO`, 100 iterations, the [`Residual`] test.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            domain: Domain1d::Line,
            algorithm: SolverAlgorithm::Newton,
            desired_accuracy: EFF_ZERO,
            acceptable_accuracy: EFF_ZERO,
            max_iter: 100,
            converged: Residual,
        }
    }
}

impl<C> LocalSolve1d<C> {
    /// These options with the convergence test `converged`.
    #[must_use]
    pub fn with_convergence<D: Converged1d>(self, converged: D) -> LocalSolve1d<D> {
        LocalSolve1d {
            domain: self.domain,
            algorithm: self.algorithm,
            desired_accuracy: self.desired_accuracy,
            acceptable_accuracy: self.acceptable_accuracy,
            max_iter: self.max_iter,
            converged,
        }
    }

    /// Minimises (or, for [`Goal::Maximize`], maximises) `F` on the bracket from
    /// `bracket.a` to `bracket.c` starting from `bracket.b`, by secant steps on `F'` or
    /// bisection (`SolveByBrent`). `f` returns `(F, F')`. Returns the sample reached, with
    /// `F` as evaluated (not negated).
    ///
    /// # Errors
    ///
    /// A failed evaluation ([`SolveError::call`]), and
    /// [`SolveFailure::BrentDidNotConverge`] when the iteration limit is reached.
    pub fn solve_by_brent<E, F>(
        &self,
        mut f: F,
        bracket: Bracket3,
        tol: f64,
        goal: Goal,
    ) -> Result<Sample, SolveError<E>>
    where
        E: EvalError,
        F: FnMut(f64) -> Result<(f64, f64), E>,
    {
        const OP: &str = "geom::local_solve1d::LocalSolve1d::solve_by_brent";
        // the search minimises: a maximum is the minimum of -F
        let mut eval = |t: f64| -> Result<(f64, f64), SolveError<E>> {
            let (v, d) = f(t).map_err(SolveError::call(OP))?;
            Ok(match goal {
                Goal::Minimize => (v, d),
                Goal::Maximize => (-v, -d),
            })
        };
        let sample = |t: f64, fx: f64| Sample {
            t,
            f: match goal {
                Goal::Minimize => fx,
                Goal::Maximize => -fx,
            },
        };
        let Bracket3 {
            a: ax,
            b: bx,
            c: cx,
        } = bracket;
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

        let (mut fx, mut dx) = eval(x)?;

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
                    return Ok(sample(x, fx));
                }
                (fu, du) = eval(u)?;
            } else {
                u = x + sign(tol1, delta_x);
                // Step must stay within the bracket.
                if u <= bracket_min {
                    u = (x + bracket_min) / 2.0;
                }
                if u >= bracket_max {
                    u = (x + bracket_max) / 2.0;
                }
                (fu, du) = eval(u)?;
                // Within machine precision, or down in the noise.
                if u == x || fu >= fx {
                    return Ok(sample(x, fx));
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
}

impl<C: Converged1d> LocalSolve1d<C> {
    /// Regula-falsi bisection between `lower` (`F < 0`) and `upper` (`F > 0`) for at
    /// most `max_division_level` subdivisions; the fraction is clamped to `[0.15, 0.85]`
    /// (`SolveByBisection`). `f` returns `F`. A midpoint that [converges](Converged1d) or
    /// has `F = 0` ends the search.
    ///
    /// # Errors
    ///
    /// A failed evaluation ([`SolveError::call`]).
    #[allow(clippy::manual_clamp)] // keep upstream's comparisons
    pub fn solve_by_bisection<E, F>(
        &self,
        mut f: F,
        lower: Sample,
        upper: Sample,
        max_division_level: u64,
    ) -> Result<Sample, SolveError<E>>
    where
        E: EvalError,
        F: FnMut(f64) -> Result<f64, E>,
    {
        const OP: &str = "geom::local_solve1d::LocalSolve1d::solve_by_bisection";
        let Sample {
            t: mut lower_t,
            f: mut lower_f,
        } = lower;
        let Sample {
            t: mut upper_t,
            f: mut upper_f,
        } = upper;
        let mut level = max_division_level;
        loop {
            if level == 0 {
                // Return the better value (low is negative, high is positive).
                return Ok(if lower_f + upper_f >= 0.0 {
                    Sample {
                        t: lower_t,
                        f: lower_f,
                    }
                } else {
                    Sample {
                        t: upper_t,
                        f: upper_f,
                    }
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
                    Sample {
                        t: lower_t,
                        f: lower_f,
                    }
                } else if upper_f.abs() < lower_f.abs() {
                    Sample {
                        t: upper_t,
                        f: upper_f,
                    }
                } else {
                    // They can be identical: use the midpoint.
                    Sample {
                        t: (lower_t + upper_t) / 2.0,
                        f: (lower_f + upper_f) / 2.0,
                    }
                });
            }

            let mid_f = f(mid_t).map_err(SolveError::call(OP))?;
            if self.converged.converged(mid_t, mid_f) || mid_f == 0.0 {
                return Ok(Sample { t: mid_t, f: mid_f });
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

    /// Finds a root of `F` near `guess` by damped Newton iteration, falling back to
    /// oscillation handling, bisection on a bracket, or a Brent search for the
    /// [`SolverAlgorithm::Brent`] goals (`SolveIt`). `f` returns `(F, F')`. A root is a
    /// point that [converges](Converged1d), has `|F|` below the desired accuracy, or
    /// (where the iteration gives up) below the acceptable one; [`Outcome::Best`]
    /// otherwise.
    ///
    /// Without an interval ([`Domain1d::Line`]) the Newton steps are not clamped;
    /// upstream still clamps each against the default inside-out `[1e20, -1e20]`
    /// interval (WR-19), replacing every step with `±1.584e19` opposite to Newton.
    ///
    /// # Errors
    ///
    /// An evaluation's failure, as [`SolveError`].
    pub fn solve<E, F>(&self, mut f: F, guess: f64) -> Result<Outcome<f64>, SolveError<E>>
    where
        E: EvalError,
        F: FnMut(f64) -> Result<(f64, f64), E>,
    {
        self.run(&mut f, guess).map(|(outcome, _)| outcome)
    }

    /// [`LocalSolve1d::solve`], with the state the solve left: brackets, iterates,
    /// counters, the found accuracy and the termination reason (upstream's members after
    /// `SolveIt`). For tests and the difftest (the `trace` feature).
    ///
    /// # Errors
    ///
    /// As [`LocalSolve1d::solve`].
    #[cfg(any(test, feature = "trace"))]
    pub fn solve_traced<E, F>(
        &self,
        mut f: F,
        guess: f64,
    ) -> Result<(Outcome<f64>, Solve1dState), SolveError<E>>
    where
        E: EvalError,
        F: FnMut(f64) -> Result<(f64, f64), E>,
    {
        self.run(&mut f, guess)
    }

    /// The first evaluation, then [`Run::solve`].
    fn run<E, F>(
        &self,
        f: &mut F,
        guess: f64,
    ) -> Result<(Outcome<f64>, Solve1dState), SolveError<E>>
    where
        E: EvalError,
        F: FnMut(f64) -> Result<(f64, f64), E>,
    {
        const OP: &str = "geom::local_solve1d::LocalSolve1d::solve_it";

        // The initial guess must be within the problem interval (clamped if not).
        let mut guess_t = guess;
        if let Some(ivl) = self.domain.interval() {
            if !ivl.contains_value(guess_t, 0.0) {
                // (upstream only logs SM_ERR_INVALID_INPUT beyond EFF_ZERO_SQRT and goes on)
                guess_t = ivl.clamp_value(guess_t);
            }
        }

        // Evaluate at the guess.
        let (t, (v, d)) = match f(guess_t) {
            Ok(value) => (guess_t, value),
            Err(e) if e.ends_solve() => return Err(SolveError::Eval(e)),
            Err(e) => {
                // Bad start point: within an interval, tweak it by 1/1000 of the interval.
                let Some(ivl) = self.domain.interval() else {
                    // No other start point: the failed evaluation left no function value
                    // to iterate from, so its error is returned. Upstream iterates from
                    // it (WR-253; the fork's tangency-labs/smlib@d2757e1).
                    return Err(SolveError::Eval(e));
                };
                let size = ivl.length();
                let mut t = guess_t + size / 1000.0;
                if !ivl.contains_value(t, 0.0) {
                    t = guess_t - size / 1000.0;
                }
                (t, f(t).map_err(SolveError::call(OP))?)
            }
        };

        let mut run = Run {
            o: self,
            st: Solve1dState::at(IterationValue {
                t,
                f: v,
                f_prime: d,
                prev_step: 0.0,
            }),
        };
        let outcome = run.solve(f, guess_t)?;
        Ok((outcome, run.st))
    }
}

/// One solve in progress: the options and the state.
struct Run<'o, C> {
    o: &'o LocalSolve1d<C>,
    st: Solve1dState,
}

impl<C: Converged1d> Run<'_, C> {
    /// Resolves an oscillating Newton iteration between `prev1` and `curr`: look
    /// for a sign change in between and bisect, else Brent (min/max modes) or the
    /// best interior value (`SolveOscillating`). Leaves the result in `curr`.
    fn solve_oscillating<E, F>(
        &mut self,
        f: &mut F,
        guess_t: f64,
        prev1: IterationValue,
    ) -> Result<Outcome<f64>, SolveError<E>>
    where
        E: EvalError,
        F: FnMut(f64) -> Result<(f64, f64), E>,
    {
        const GUESSES: [f64; 8] = [0.25, 0.75, 0.13, 0.38, 0.82, 0.87, 0.01, 0.99];

        let mid_t = 0.5 * (prev1.t + self.st.curr.t);
        let (mid_f, _) = f(mid_t).map_err(SolveError::Eval)?;
        let mut best_interior = Sample { t: mid_t, f: mid_f };

        // Not bracketed: look for a sign change between prev1 and curr.
        if self.st.lower.is_none() || self.st.upper.is_none() {
            let curr = self.st.curr;

            let mut sign_change = None;
            for g in GUESSES {
                let mid_t = prev1.t + g * (curr.t - prev1.t);
                let Some((mid_f, _)) = ignored(f(mid_t).map_err(SolveError::Eval))? else {
                    continue; // just keep trying
                };
                if mid_f.abs() < best_interior.f.abs() {
                    best_interior = Sample { t: mid_t, f: mid_f };
                }
                if mid_f * curr.f <= 0.0 {
                    sign_change = Some(Sample { t: mid_t, f: mid_f });
                    break; // opposite sign
                }
            }

            // Only a guess that evaluated can show a sign change; upstream tests the
            // last guess's `F` whether its evaluation succeeded or not (WR-253).
            if let Some(mid) = sign_change {
                // Curr and prev have the same sign, opposite to mid: pick the end on
                // the side of the given guess.
                let pick_curr = if guess_t < mid.t {
                    curr.t < prev1.t
                } else {
                    curr.t > prev1.t
                };
                let end = if pick_curr {
                    Sample {
                        t: curr.t,
                        f: curr.f,
                    }
                } else {
                    Sample {
                        t: prev1.t,
                        f: prev1.f,
                    }
                };
                let (mut new_lo, mut new_hi) = if mid.f > 0.0 { (end, mid) } else { (mid, end) };

                // Prefer an existing bracket when it is closer.
                if let Some(lo) = self.st.lower {
                    if (lo.t - new_hi.t).abs() < (new_lo.t - new_hi.t).abs() {
                        new_lo = lo;
                    }
                }
                if let Some(hi) = self.st.upper {
                    if (hi.t - new_lo.t).abs() < (new_hi.t - new_lo.t).abs() {
                        new_hi = hi;
                    }
                }
                self.st.lower = Some(new_lo);
                self.st.upper = Some(new_hi);
            }
        }

        // Bracketed: bisect (deeper, we might not be close). Success even if F is
        // too big: at a discontinuity the caller wants the best solution.
        if let (Some(lo), Some(hi)) = (self.st.lower, self.st.upper) {
            let r = self
                .o
                .solve_by_bisection(|t| f(t).map(|(v, _)| v), lo, hi, 40);
            if let Some(s) = ignored(r)? {
                self.st.curr.t = s.t;
                self.st.curr.f = s.f;
                self.st.found_accuracy = Some(s.f);
                self.st.termination_reason = if s.f.abs() < self.o.desired_accuracy {
                    TerminationReason::FoundAnswerConverged
                } else {
                    TerminationReason::FoundAnswerClose
                };
                return Ok(Outcome::Root(s.t));
            }
        }

        // No bracket: oscillating between two values of the same sign.
        let goal = self.o.algorithm.goal();
        if let Some(goal) = goal {
            let bracket = Bracket3 {
                a: prev1.t,
                b: best_interior.t,
                c: self.st.curr.t,
            };
            let s = self
                .o
                .solve_by_brent(&mut *f, bracket, self.o.desired_accuracy, goal)?;
            self.st.curr.t = s.t;
        } else if best_interior.f.abs() < self.st.curr.f.abs() {
            self.st.curr.t = best_interior.t;
        } else {
            // just try the midpoint
            self.st.curr.t = 0.5 * (prev1.t + self.st.curr.t);
        }

        // The caller expects the solution in curr.
        let (v, d) = f(self.st.curr.t).map_err(SolveError::Eval)?;
        self.st.curr.f = v;
        self.st.curr.f_prime = d;

        let c = self.st.curr;
        // for min/max the tolerance does not matter
        let found = goal.is_some()
            || self.o.converged.converged(c.t, c.f)
            || c.f.abs() <= self.o.acceptable_accuracy;
        Ok(if found {
            Outcome::Root(c.t)
        } else {
            Outcome::Best(c.t)
        })
    }

    /// [`LocalSolve1d::solve`]'s iteration from the first evaluated value in `curr`.
    #[allow(clippy::too_many_lines)] // upstream's one loop
    fn solve<E, F>(&mut self, f: &mut F, guess_t: f64) -> Result<Outcome<f64>, SolveError<E>>
    where
        E: EvalError,
        F: FnMut(f64) -> Result<(f64, f64), E>,
    {
        const OP: &str = "geom::local_solve1d::LocalSolve1d::solve_it";

        // Serendipity: the guess is a solution.
        if self.o.converged.converged(self.st.curr.t, self.st.curr.f) {
            self.st.found_accuracy = Some(self.st.curr.f);
            self.st.termination_reason = TerminationReason::FoundAnswerConverged;
            return Ok(Outcome::Root(self.st.curr.t));
        }

        let mut cut_factor = 1.0;
        let mut minimum = self.st.curr.f.abs();
        let mut min_count: u64 = 0;
        let mut queue = IterQueue1d::new(QUEUE_SLOTS);

        self.st.iter = 0;
        while self.st.iter < self.o.max_iter {
            // Brackets of zero crossings (used for bisection after Newton).
            let here = Sample {
                t: self.st.curr.t,
                f: self.st.curr.f,
            };
            if here.f < 0.0 && self.st.lower.is_none_or(|b| here.f > b.f) {
                self.st.lower = Some(here);
            }
            if here.f > 0.0 && self.st.upper.is_none_or(|b| here.f < b.f) {
                self.st.upper = Some(here);
            }

            // Converged.
            if self.st.curr.f.abs() < self.o.desired_accuracy {
                self.st.found_accuracy = Some(self.st.curr.f);
                self.st.termination_reason = TerminationReason::FoundAnswerConverged;
                return Ok(Outcome::Root(self.st.curr.t));
            }

            // (an oscillating queue holds six steps, so prev1 is set)
            if let (true, Some(prev1)) = (queue.is_oscillating(), self.st.prev1) {
                return self.solve_oscillating(f, guess_t, prev1);
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
            self.st.curr.prev_step = match (self.st.iter > 0, self.st.prev1) {
                (true, Some(p)) => self.st.curr.t - p.t,
                _ => 0.0,
            };
            let prev2 = match (self.st.iter > 1, self.st.prev1) {
                (true, Some(p)) => p,
                _ => self.st.curr,
            };
            let prev1 = self.st.curr;
            self.st.prev2 = Some(prev2);
            self.st.prev1 = Some(prev1);
            queue.add(prev1.prev_step);

            // Evaluate into curr.
            let (v, d) = f(new_t).map_err(SolveError::call(OP))?;
            self.st.curr = IterationValue {
                t: new_t,
                f: v,
                f_prime: d,
                ..self.st.curr
            };
            let curr = self.st.curr;

            if self.o.converged.converged(curr.t, curr.f) || curr.f.abs() < self.o.desired_accuracy
            {
                self.st.found_accuracy = Some(curr.f);
                self.st.termination_reason = TerminationReason::FoundAnswerConverged;
                return Ok(Outcome::Root(curr.t));
            }

            // No decrease for 4 iterations: stuck, try bisection.
            if curr.f.abs() >= minimum {
                min_count += 1;
                if min_count >= 4 && curr.f.abs() < self.o.acceptable_accuracy {
                    break;
                }
            } else {
                min_count = 0;
                minimum = curr.f.abs();
            }

            // Brent min/max once the extremum is roughly bracketed: curr beats both
            // previous values.
            if let (Some(goal), true) = (self.o.algorithm.goal(), self.st.iter > 1) {
                let beats = |a: f64, b: f64| match goal {
                    Goal::Maximize => a > b,
                    Goal::Minimize => a < b,
                };
                if beats(curr.f, prev1.f) && beats(curr.f, prev2.f) {
                    let ext = Extent1d::spanning(prev1.t, prev2.t);
                    if ext.contains_value(curr.t, 0.0) {
                        let bracket = Bracket3 {
                            a: prev1.t,
                            b: curr.t,
                            c: prev2.t,
                        };
                        let s = self.o.solve_by_brent(
                            &mut *f,
                            bracket,
                            self.o.desired_accuracy,
                            goal,
                        )?;
                        return Ok(Outcome::Root(s.t));
                    }
                }
            }

            // Getting larger on the same side of the answer.
            if curr.f.abs() >= prev1.f.abs() && curr.f * prev1.f > 0.0 {
                if curr.f != prev1.f
                    && curr.t != prev1.t
                    && self.st.lower.is_some()
                    && self.st.upper.is_some()
                {
                    break; // bracketed: try bisection
                }

                // The step went to zero: exit with or without a solution.
                if delta_t.abs() < EFF_ZERO {
                    let f_mag = curr.f.abs();
                    self.st.found_accuracy = Some(curr.f);
                    self.st.termination_reason = self.reason_for(f_mag);
                    return Ok(if f_mag < self.o.acceptable_accuracy {
                        Outcome::Root(curr.t)
                    } else {
                        Outcome::Best(curr.t)
                    });
                }
            }
            self.st.iter += 1;
        }

        // Not converged (iteration limit, out of bounds, or diverging): try
        // bisection on a bracket.
        if let (Some(lo), Some(hi)) = (self.st.lower, self.st.upper) {
            let r = self
                .o
                .solve_by_bisection(|t| f(t).map(|(v, _)| v), lo, hi, 20);
            if let Some(s) = ignored(r)? {
                self.st.curr.t = s.t;
                self.st.curr.f = s.f;
            }
        }

        let c = self.st.curr;
        self.st.found_accuracy = Some(c.f);
        if c.f.abs() < self.o.acceptable_accuracy {
            self.st.termination_reason = self.reason_for(c.f.abs());
            return Ok(Outcome::Root(c.t));
        }

        // Best answer anyway, so that min/max works without convergence.
        Ok(Outcome::Best(c.t))
    }

    /// Termination reason for a final `|F|` of `f_mag`.
    fn reason_for(&self, f_mag: f64) -> TerminationReason {
        if f_mag < self.o.desired_accuracy {
            TerminationReason::FoundAnswerConverged
        } else if f_mag < self.o.acceptable_accuracy {
            TerminationReason::FoundAnswerClose
        } else {
            TerminationReason::UnableToConverge
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl<'o, C> Run<'o, C> {
        /// A run that no solve has started, with the iterates `prev1` and `curr` set
        /// by hand: what `solve_oscillating` sees in the tests below.
        fn at(o: &'o LocalSolve1d<C>, prev1: Sample, curr: Sample) -> Self {
            let value = |s: Sample| IterationValue {
                t: s.t,
                f: s.f,
                f_prime: 0.0,
                prev_step: 0.0,
            };
            let mut st = Solve1dState::at(value(curr));
            st.prev1 = Some(value(prev1));
            Self { o, st }
        }
    }

    /// The test evaluators' error: the classes the pin's tests use.
    #[derive(Debug, Clone, PartialEq)]
    enum TestErr {
        /// An upstream `SM_ERR` status (pin `SolidError::sm_err`).
        Sm(&'static str),
        /// Pin `SolidError::Unported`.
        Unported,
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
    }

    type Fd = Result<(f64, f64), TestErr>;

    /// `F(t) = t^2 - 2`, never failing.
    #[allow(clippy::unnecessary_wraps)] // the solver's closure type
    fn sq(t: f64) -> Fd {
        Ok((t * t - 2.0, 2.0 * t))
    }

    /// `F(t) = t^3 - 2t + 2`: Newton from 0 cycles 0, 1, 0, 1, ...
    #[allow(clippy::unnecessary_wraps)] // the solver's closure type
    fn cycle(t: f64) -> Fd {
        Ok(((t * t - 2.0) * t + 2.0, 3.0 * t * t - 2.0))
    }

    /// The two values `cycle` settles between from 0 within [-3, 3] (Newton's
    /// steps are capped at 0.08 of the interval) and the last interior guess of
    /// `SolveOscillating` between them, 0.99 of the way.
    const CYCLE_PREV: f64 = 0.475_200_000_000_989_94;
    const CYCLE_CURR: f64 = 0.950_400_000_001_979_9;
    const LAST_GUESS: f64 = CYCLE_PREV + 0.99 * (CYCLE_CURR - CYCLE_PREV);

    /// `cycle`, except that the evaluation at `LAST_GUESS` fails.
    fn cycle_failing_at_last_guess(t: f64) -> Fd {
        if t.to_bits() == LAST_GUESS.to_bits() {
            return Err(TestErr::Sm("test: no value here"));
        }
        cycle(t)
    }

    /// `sq`, except that the evaluation at 1 fails.
    fn sq_failing_at_one(t: f64) -> Fd {
        if t.to_bits() == 1f64.to_bits() {
            return Err(TestErr::Sm("test: no value here"));
        }
        sq(t)
    }

    fn unported(_: f64) -> Fd {
        Err(TestErr::Unported)
    }

    /// Newton on the whole line, accepting `|F| < 1e-9`.
    const fn line() -> LocalSolve1d {
        LocalSolve1d {
            acceptable_accuracy: 1e-9,
            ..LocalSolve1d::new()
        }
    }

    /// Newton on `[lo, hi]`, accepting `|F| < 1e-9`.
    fn bounded(lo: f64, hi: f64) -> LocalSolve1d {
        LocalSolve1d {
            domain: Domain1d::Bounded(Extent1d::spanning(lo, hi)),
            ..line()
        }
    }

    /// WR-253 (the fork's tangency-labs/smlib@d2757e1, tangency-labs/solidcore#2574): a
    /// failed first evaluation without an interval returns the evaluator's error, as the
    /// interval case does when its tweaked start fails too. Upstream iterates from it
    /// instead, and its zeroed `F` reads as converged, reporting `t = 1` as a root of
    /// `t^2 - 2`. A good start is unaffected.
    #[test]
    fn failed_first_evaluation_without_interval_has_no_value() {
        let s = line();
        let e = s.solve(sq_failing_at_one, 1.0).unwrap_err();
        assert!(
            matches!(e, SolveError::Eval(TestErr::Sm("test: no value here"))),
            "{e:?}"
        );

        // A start whose evaluation succeeds solves as on the clean function.
        let r = s.solve(sq_failing_at_one, 1.5).unwrap();
        let plain = s.solve(sq, 1.5).unwrap();
        assert!(r.is_root());
        assert_eq!(r.value().to_bits(), plain.value().to_bits());
    }

    #[test]
    fn newton_sqrt2() {
        let (r, st) = bounded(0.0, 3.0).solve_traced(sq, 1.0).unwrap();
        let Outcome::Root(t) = r else {
            panic!("{r:?}");
        };
        assert!((t - 2f64.sqrt()).abs() < 1e-10);
        assert_eq!(
            st.termination_reason,
            TerminationReason::FoundAnswerConverged
        );
        assert_eq!(st.found_accuracy, Some(st.curr.f));
    }

    #[test]
    fn oscillation_is_resolved() {
        let r = bounded(-3.0, 3.0).solve(cycle, 0.0).unwrap();
        assert!(r.value().is_finite());
    }

    /// tangency-labs/solidcore#481 (WR-253): the failed last guess is no sign change, and
    /// the solve finds nothing, as on the clean function. `F = t^3 - 2t + 2` has no root
    /// between the two values Newton cycles on. Upstream reads the `F` the failed
    /// evaluation left behind, brackets that guess with `F = 0` and reports it converged
    /// there, although `F` is about 0.95 at that parameter.
    #[test]
    fn issue_481_failed_guess_is_no_sign_change() {
        let s = bounded(-3.0, 3.0);
        let (clean, clean_st) = s.solve_traced(cycle, 0.0).unwrap();
        assert!(!clean.is_root());
        assert_eq!(
            clean_st.termination_reason,
            TerminationReason::UnableToConverge
        );

        let (r, st) = s.solve_traced(cycle_failing_at_last_guess, 0.0).unwrap();
        assert!(!r.is_root(), "{r:?}");
        assert_ne!(r.value().to_bits(), LAST_GUESS.to_bits());
        assert!(st
            .lower
            .is_none_or(|b| b.t.to_bits() != LAST_GUESS.to_bits()));
        assert_eq!(st.termination_reason, TerminationReason::UnableToConverge);

        // F really is about 0.95 there.
        assert!(cycle(LAST_GUESS).unwrap().0 > 0.9);
    }

    /// A failed final sample must not replace the valid, same-sign samples with
    /// a fabricated zero (WR-253 / #481).
    #[test]
    fn failed_guess_does_not_supply_a_sign_change() {
        let solver = bounded(-3.0, 3.0);
        let (clean, clean_st) = solver.solve_traced(cycle, 0.0).unwrap();
        assert!(!clean.is_root());
        let (result, st) = solver
            .solve_traced(cycle_failing_at_last_guess, 0.0)
            .unwrap();
        assert!(!result.is_root());
        assert_eq!(result.value().to_bits(), clean.value().to_bits());
        assert_eq!(st.curr.f, clean_st.curr.f);
        assert_eq!(st.termination_reason, TerminationReason::UnableToConverge);
        assert_eq!(st.lower.is_some(), clean_st.lower.is_some());
        assert_eq!(st.upper.is_some(), clean_st.upper.is_some());
    }

    #[test]
    fn failed_interior_samples_cannot_create_a_bracket() {
        fn failed_samples(t: f64) -> Fd {
            if [0.25, 0.75, 0.13, 0.38, 0.82, 0.87, 0.01, 0.99].contains(&t) {
                return Err(TestErr::Sm("test: failed interior sample"));
            }
            Ok((t * t + 1.0, 2.0 * t))
        }
        let o = LocalSolve1d {
            acceptable_accuracy: 0.0,
            ..LocalSolve1d::new()
        };
        let mut run = Run::at(&o, Sample { t: 0.0, f: 1.0 }, Sample { t: 1.0, f: 2.0 });
        let prev1 = run.st.prev1.unwrap();
        let result = run
            .solve_oscillating(&mut failed_samples, 0.9, prev1)
            .unwrap();
        assert!(!result.is_root());
        let t = result.value();
        assert_eq!(run.st.curr.f, t * t + 1.0);
        assert!(run.st.lower.is_none());
        assert!(run.st.upper.is_none());
    }

    #[test]
    fn successful_sample_after_a_failure_can_still_bracket_a_root() {
        fn failed_quarter(t: f64) -> Fd {
            if t == 0.25 {
                return Err(TestErr::Sm("test: failed quarter sample"));
            }
            Ok(((t - 0.75) * (t - 0.75) - 0.01, 2.0 * (t - 0.75)))
        }
        let o = LocalSolve1d {
            desired_accuracy: 1e-10,
            acceptable_accuracy: 0.0,
            ..LocalSolve1d::new()
        };
        let mut run = Run::at(
            &o,
            Sample { t: 0.0, f: 0.5525 },
            Sample { t: 1.0, f: 0.0525 },
        );
        let prev1 = run.st.prev1.unwrap();
        let result = run
            .solve_oscillating(&mut failed_quarter, 0.9, prev1)
            .unwrap();
        let Outcome::Root(t) = result else {
            panic!("{result:?}");
        };
        assert!((t - 0.85).abs() < 1e-8);
        assert!(run.st.curr.f.abs() < 1e-10);
    }

    #[test]
    fn queue_oscillation() {
        let eight = NonZeroUsize::new(8).unwrap();
        let mut q = IterQueue1d::new(eight);
        for i in 0..6 {
            assert!(!q.is_oscillating());
            q.add(if i % 2 == 0 { 1.0 } else { -1.0 });
        }
        assert!(q.is_oscillating());
        assert_eq!(
            IterQueue1d::try_new(eight).unwrap(),
            IterQueue1d::new(eight)
        );
        assert_eq!(QUEUE_SLOTS, eight);
        // fuzz regression: a caller-chosen size that cannot be allocated fails softly
        assert!(matches!(
            IterQueue1d::try_new(NonZeroUsize::MAX),
            Err(NumError::Exhausted {
                limit: Limit::Allocation { count: usize::MAX },
                ..
            })
        ));
        assert!(IterQueue1d::try_new(NonZeroUsize::new(1 << 60).unwrap()).is_err());
    }

    #[test]
    fn bisection_and_brent() {
        let s = LocalSolve1d::new();
        let f = |t: f64| sq(t).map(|(v, _)| v);
        let lo = Sample { t: 1.0, f: -1.0 };
        let hi = Sample { t: 2.0, f: 2.0 };
        let r = s.solve_by_bisection(f, lo, hi, 40).unwrap();
        assert!((r.t - 2f64.sqrt()).abs() < 1e-9 && r.f.abs() < 1e-8);
        let bracket = Bracket3 {
            a: -1.0,
            b: 0.3,
            c: 2.0,
        };
        let m = s
            .solve_by_brent(sq, bracket, 1e-12, Goal::Minimize)
            .unwrap();
        assert!(m.t.abs() < 1e-6);
        assert_eq!(m.f, sq(m.t).unwrap().0);
        assert_eq!(s.solve_by_bisection(f, lo, hi, 0).unwrap(), lo);

        // a maximum is reported with F as evaluated, not negated
        let cap = |t: f64| Ok::<_, TestErr>((1.0 - t * t, -2.0 * t));
        let m = s
            .solve_by_brent(cap, bracket, 1e-12, Goal::Maximize)
            .unwrap();
        assert!(m.t.abs() < 1e-6);
        assert!(m.f > 0.0 && m.f == cap(m.t).unwrap().0);
    }

    #[test]
    fn errors() {
        assert!(matches!(
            bounded(0.0, 1.0).solve(unported, 0.5),
            Err(SolveError::Eval(TestErr::Unported))
        ));
        assert_eq!(SolverAlgorithm::default().goal(), None);
        assert_eq!(
            SolverAlgorithm::Brent(Goal::Maximize).goal(),
            Some(Goal::Maximize)
        );
    }

    /// The options' convergence test decides "found" beside the residual: a test that
    /// accepts any `t` past 1 ends the solve at the first iterate there, and one that
    /// accepts nothing leaves the residual to decide.
    #[test]
    fn convergence_test_decides_found() {
        let s = bounded(0.0, 3.0);
        let past_one = s.with_convergence(|t: f64, _: f64| t > 1.0);
        let (r, st) = past_one.solve_traced(sq, 0.5).unwrap();
        let Outcome::Root(t) = r else {
            panic!("{r:?}");
        };
        assert!(t > 1.0 && (t - 2f64.sqrt()).abs() > 1e-3, "{t}");
        assert_eq!(
            st.termination_reason,
            TerminationReason::FoundAnswerConverged
        );

        // the guess itself
        let r = past_one.solve(sq, 2.5).unwrap();
        assert_eq!(r, Outcome::Root(2.5));

        let never = s.with_convergence(|_: f64, _: f64| false);
        assert_eq!(never.solve(sq, 1.0).unwrap(), s.solve(sq, 1.0).unwrap());
    }

    /// WR-19 (#2150): solving without an interval skips the interval-length clamp and
    /// converges cleanly on linear, nonlinear positive/negative, and large-scale roots.
    #[test]
    fn wr19_unbounded_newton_converges_positive_and_negative_roots() {
        /// Solves `func` from `guess` on the whole line, recording each evaluated `t`.
        fn recorded(
            func: impl Fn(f64) -> (f64, f64),
            guess: f64,
        ) -> (Outcome<f64>, Solve1dState, Vec<f64>) {
            let mut ts = Vec::new();
            let (r, st) = line()
                .solve_traced(
                    |t| {
                        ts.push(t);
                        Ok::<_, TestErr>(func(t))
                    },
                    guess,
                )
                .unwrap();
            (r, st, ts)
        }

        // Linear `f(t) = t - 3.0` from `guess_t = 0.0` converges in 1 Newton step.
        let (r, st, ts) = recorded(|t| (t - 3.0, 1.0), 0.0);
        assert!(r.is_root());
        assert!((r.value() - 3.0).abs() < 1e-12);
        assert_eq!(ts, vec![0.0, 3.0]);
        assert_eq!(
            st.termination_reason,
            TerminationReason::FoundAnswerConverged
        );

        // Nonlinear positive root `f(t) = t^3 - 27.0` from `guess_t = 10.0`.
        let (r, _, ts) = recorded(|t| (t * t * t - 27.0, 3.0 * t * t), 10.0);
        assert!(r.is_root());
        assert!((r.value() - 3.0).abs() < 1e-12);
        assert!(ts.len() <= 10);

        // Nonlinear negative root `f(t) = t^3 + 125.0` from `guess_t = -10.0`
        let (r, _, ts) = recorded(|t| (t * t * t + 125.0, 3.0 * t * t), -10.0);
        assert!(r.is_root());
        assert!((r.value() + 5.0).abs() < 1e-12);
        assert!(ts.len() <= 10);

        // Large-scale root `f(t) = t - 1.0e6` from `guess_t = 0.0` without step clamping.
        let (r, _, ts) = recorded(|t| (t - 1.0e6, 1.0), 0.0);
        assert!(r.is_root());
        assert!((r.value() - 1.0e6).abs() < 1e-12);
        assert_eq!(ts, vec![0.0, 1.0e6]);
    }

    /// WR-19 (#2150): with an interval, Newton steps exceeding
    /// `0.08 * interval.length() + EFF_ZERO` are still clamped by `0.99`, matching
    /// upstream bit-for-bit.
    #[test]
    fn wr19_bounded_interval_still_clamps_newton_step() {
        let s = bounded(0.0, 10.0);
        let expected_clamped_step = (10.0 * 0.08 + EFF_ZERO) * 0.99;
        let recorded = |func: fn(f64) -> f64, guess: f64| {
            let mut ts = Vec::new();
            let r = s
                .solve(
                    |t| {
                        ts.push(t);
                        Ok::<_, TestErr>((func(t), 1.0))
                    },
                    guess,
                )
                .unwrap();
            (r, ts)
        };

        // Positive step (`delta_t = +9.0 > 0.8` from `guess_t = 0.0`): first step `0.792`.
        let (r, ts) = recorded(|t| t - 9.0, 0.0);
        assert_eq!(ts[1], expected_clamped_step);
        assert!((ts[1] - 0.792).abs() < 1e-11);
        assert!(r.is_root());
        assert!((r.value() - 9.0).abs() < 1e-12);

        // Negative step (`delta_t = -9.0 < -0.8` from `guess_t = 10.0`): first step `9.208`.
        let (r, ts) = recorded(|t| t - 1.0, 10.0);
        assert_eq!(ts[1], 10.0 - expected_clamped_step);
        assert!((ts[1] - 9.208).abs() < 1e-11);
        assert!(r.is_root());
        assert!((r.value() - 1.0).abs() < 1e-12);
    }

    /// WR-253's "a failed evaluation has no value" (#2574) and WR-19's unbounded
    /// steps without an interval.
    #[test]
    fn wr19_failed_first_evaluation_and_valid_start() {
        // Failed first evaluation without an interval returns `Err` (WR-253), and a
        // valid start now converges to `sqrt(2)` instead of stepping `-1.584e19` (WR-19).
        let err = line().solve(sq_failing_at_one, 1.0).unwrap_err();
        assert!(
            matches!(err, SolveError::Eval(TestErr::Sm("test: no value here"))),
            "{err:?}"
        );
        let ok = line().solve(sq_failing_at_one, 1.5).unwrap();
        assert!(ok.is_root());
        assert!((ok.value() - 2.0_f64.sqrt()).abs() < 1e-12);

        // With an interval, WR-253's rule holds too.
        let r = bounded(-3.0, 3.0)
            .solve(cycle_failing_at_last_guess, 0.0)
            .unwrap();
        assert!(!r.is_root());
        assert_ne!(r.value().to_bits(), LAST_GUESS.to_bits());
    }
}
