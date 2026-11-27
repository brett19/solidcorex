// Moved from solidcore `solidcore/crates/solidcore/src/geom/local_solve_nd.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/inc/SmLocalSolveNd.h`,
// `source/SMLib/src/SmLocalSolveNd.cpp` (enums `SmLocalSolverAlgorithmType` /
// `SmTerminationReasonType` from `source/SMLib/inc/SmLocalSolve1d.h`).
// Changes: the equation set's error is generic (classified by `EvalError`; owner decision
// 2026-11-26, report 53 section 12), so `jacobian` and `LocalSolveNd::solve` return
// `SolveError` and test the class where the pin matched `SolidError` variants;
// `is_bad_jacobian` reads a layer-0 `NumError::Failed` (status `SM_ERR`) where the pin read
// the status. The solver is an options value and a pure call over a closure (04 rows 89
// and 90, research 57 M6): the `EvalNFunctions` trait, its `solution_size`, `scaled_zero`,
// `is_converged`, `has_intervals` and `set_intervals` (and its failing default), the
// found flag the evaluator returned, `NdSolution`, `eval_outcome`, `set`, the borrowed
// `LocalSolveNd<'a, E>` with its constructor, setters and getters
// (`intervals`, `termination_reason`, `guess_unevaluated`, `current_iteration`,
// `evaluator`), `TerminationReason::{Undefined, StillIterating}` and
// `TerminationReason::code` are removed; `compute_jacobian` (out-parameters, `x`
// perturbed in place) is `jacobian`; `LocalSolveNd<N, C>` is new (const-`N` arrays, the
// domain as `[Domain1d; N]` in place of the `ExtentNd` and periodicity slice, the
// convergence test `C`, the boundary handler's limit in `BoundaryHandler::TotalHits`),
// as are `LocalSolveNd::solve`, `LocalSolveNd::clamp`, `Clamped`, `NdReport`, `Outcome`,
// `Residual`, `ConvergedNd` and `TerminationReason::GuessUnevaluated`. The tests use a
// test error (`TestErr`, with `fmt`, `ends_solve`, `passes_through`, `never_ignored` and
// one `from`) and closures; `tests::oversized_jacobian_fails_before_evaluation` and
// `tests::set_intervals_default_fails` are removed (a const `N` and the removed trait
// leave nothing to test), `tests::clamp_follows_the_domain` and
// `tests::residual_and_closure_convergence` are new, and every other test reads the
// report where it read the solver; `tests::tol_scaled_zero_of_equations` stays with the
// kernel's `tol::scaled_zero_of_equations` (not moved). Results and every error's class,
// text and cause unchanged, but for the guess's convergence test (see the deviations).
//! Multi-variable Newton-Raphson solver for square systems of equations near a good
//! guess, with step cutting, interval clamping and boundary-hit exit criteria, plus
//! the finite-difference Jacobian helper for equation sets without derivatives.
//!
//! Upstream: `source/SMLib/inc/SmLocalSolveNd.h`, `source/SMLib/src/SmLocalSolveNd.cpp`
//! (enums `SmLocalSolverAlgorithmType` / `SmTerminationReasonType` from
//! `source/SMLib/inc/SmLocalSolve1d.h`).
//!
//! | Rust                                        | Upstream                                         |
//! |---------------------------------------------|--------------------------------------------------|
//! | the closure passed to [`LocalSolveNd::solve`] | `SmEvalNFunctionsObject::Evaluate`             |
//! | [`ConvergedNd::solved`]                     | `Evaluate`'s `rbFoundAnswer`                     |
//! | [`ConvergedNd::guess_converged`]            | `IsConverged` (`m_dSolutionSize < m_dScaledZero`) |
//! | [`LocalSolveNd::domain`]                    | `SmLocalSolveNd`'s intervals and periodicities, `HasIntervals` / `SetIntervals` |
//! | [`jacobian`]                                | `SmEvalNFunctionsObject::ComputeJacobian`        |
//! | [`LocalSolveNd`] (the options), [`LocalSolveNd::new`] | `SmLocalSolveNd`, its constructor and setters |
//! | [`LocalSolveNd::solve`], [`NdReport`]       | `SolveIt`, `GetTerminationReason`, `m_lCurrentIteration` |
//! | [`LocalSolveNd::clamp`], [`Clamped`]        | `SmExtentNd::ClampVector`                        |
//! | [`BoundaryHandler`]                         | `SmBoundaryHandlerType`, `m_lMaxBoundaryHits`    |
//! | [`TerminationReason`]                       | `SmTerminationReasonType`                        |
//! | [`Outcome`]                                 | `SolveIt`'s `rbFoundSolution` and `rSolutionVector` |
//!
//! Deviations:
//! - The equation set is a closure `FnMut(&[f64; N]) -> Result<([f64; N], [[f64; N]; N]), E>`
//!   returning the values and the Jacobian (`jacobian[i][j] = dF_i/dx_j`); the solver's
//!   settings, the domain among them, are [`LocalSolveNd`]'s fields, and whether a point
//!   solves the system is the options' [`ConvergedNd`], not the evaluator's. `SolveIt`'s
//!   state after the call is the returned [`NdReport`].
//! - The guess's convergence test ([`ConvergedNd::guess_converged`]) reads the guess and
//!   its values. Upstream's `IsConverged` reads the equation set's members as the last
//!   evaluation left them, which for an equation set computing its Jacobian by
//!   differences (`ComputeJacobian`) is a perturbed point, not the guess.
//! - Any `Err` counts as a failed evaluation exactly where upstream tests
//!   `!= SM_SUCCESS`, except an error that [ends the solve](EvalError::ends_solve), which
//!   always propagates (the solver cannot reproduce upstream past an unported callee).
//! - Failed guesses are never solutions (WR-20, WR-253). An accepted clamped-and-scaled
//!   step carries its own equations and Jacobian, and its own convergence test. If no
//!   candidate evaluates, iteration stops at the last evaluated point; partial writes
//!   cannot become a zero residual. A best point whose reevaluation fails is not accepted.
//!   A guess that does not evaluate ends the solve with
//!   [`TerminationReason::GuessUnevaluated`] (upstream iterates from whatever the failed
//!   evaluation wrote).
//! - Only a linear solve failing with `SM_ERR` ends the iteration with
//!   [`TerminationReason::BadJacobian`] as upstream; its other errors (unreachable with
//!   an `N x N` Jacobian) are returned.
//! - [`jacobian`] takes the point by value and returns the matrix; it never changes the
//!   caller's point. It takes no current values (upstream's `crFn` is never read).
//! - `m_eLocalSolverType` (always `SM_SA_NEWTON`, never read), `m_pSolutionVector`
//!   (a pointer to the caller's output) and `m_dFoundAccuracy` (never written by
//!   `SolveIt`) are not kept. `SetAcceptableAccuracy` is
//!   [`LocalSolveNd::acceptable_accuracy`] (upstream's `SolveIt` overwrites it with its
//!   argument).
//! - The `SM_DEBUG_CODE` trace output is not ported.

use crate::error::{EvalError, SolveError, SolveFailure};
use crate::local_solve1d::Domain1d;
use scx_num::NumError;
use scx_num::math::{BIG_DOUBLE, EFF_ZERO, EFF_ZERO_SQ, EFF_ZERO_SQRT};
use scx_num::matrix::Matrix;

/// Whether the Jacobian system's solve failed with `SM_ERR` (the system is not solvable),
/// which ends the iteration: by its status, so also a typed failure that replaced the
/// `Upstream` (#13790). Every layer-0 `Failed` and `Call` has status `SM_ERR`; `InvalidInput` has
/// `SM_ERR_INVALID_INPUT`.
fn is_bad_jacobian(error: &NumError) -> bool {
    matches!(error, NumError::Failed { .. } | NumError::Call { .. })
}

/// How a solve treats next guesses that leave the domain's intervals
/// (`SmBoundaryHandlerType` with `m_lMaxBoundaryHits`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoundaryHandler {
    /// Stop when the parameters have stepped out of bounds (any combination, counting
    /// only large steps) more than `max` times (`SM_BH_TOTAL_BOUNDARY_HITS`).
    TotalHits {
        /// The hits allowed (upstream's default is 3).
        max: u64,
    },
    /// Stop when the next guess was out of every interval at once more than twice
    /// (`SM_BH_HITS_ALL_BOUNDARIES`, the default).
    #[default]
    HitsAllBoundaries,
    /// Never stop because of boundary hits (`SM_BH_IGNORE_BOUNDARY_HITS`).
    Ignore,
}

/// Why a finished local solve stopped (`SmTerminationReasonType`, shared with the 1-D
/// solver).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminationReason {
    /// Converged to the desired accuracy (`SM_TR_FOUND_ANSWER_CONVERGED`).
    FoundAnswerConverged,
    /// Converged to the acceptable accuracy only (`SM_TR_FOUND_ANSWER_CLOSE`).
    FoundAnswerClose,
    /// Stopped by the boundary handler (`SM_TR_OUT_OF_BOUNDS`).
    OutOfBounds,
    /// The Jacobian system could not be solved (`SM_TR_BAD_JACOBIAN`).
    BadJacobian,
    /// No convergence (`SM_TR_UNABLE_TO_CONVERGE`).
    UnableToConverge,
    /// The equations could not be evaluated at the guess itself (not upstream, WR-20):
    /// the outcome is the guess, unsolved; a caller may still check it by other means.
    GuessUnevaluated,
}

/// Where a solve ended: at a root, or at the best point it reached without one
/// (upstream's `rbFoundSolution` true or false).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome<T> {
    /// A solution within the accuracy the solve accepts.
    Root(T),
    /// No solution; the best point reached.
    Best(T),
}

impl<T> Outcome<T> {
    /// The point, root or not.
    pub fn into_value(self) -> T {
        match self {
            Self::Root(v) | Self::Best(v) => v,
        }
    }

    /// The point, root or not.
    pub const fn value(&self) -> &T {
        match self {
            Self::Root(v) | Self::Best(v) => v,
        }
    }

    /// The root, if the solve found one.
    pub fn root(self) -> Option<T> {
        match self {
            Self::Root(v) => Some(v),
            Self::Best(_) => None,
        }
    }

    /// Whether the solve found a root.
    pub const fn is_root(&self) -> bool {
        matches!(self, Self::Root(_))
    }
}

/// The convergence test that only the solver's own residual tests decide: no point is
/// declared solved beyond them, and the guess passes [`ConvergedNd::guess_converged`]
/// (upstream's defaults: `rbFoundAnswer` false, `m_dSolutionSize` 0 below
/// `m_dScaledZero` 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Residual;

/// A system's own convergence test, beside the solver's residual tests.
///
/// A closure `Fn(&[f64; N], &[f64; N]) -> bool` is one: it decides
/// [`ConvergedNd::solved`] from a point and its values, and accepts every guess.
pub trait ConvergedNd<const N: usize> {
    /// Whether `x`, with values `f`, solves the system whatever the residual says
    /// (upstream: `Evaluate`'s `rbFoundAnswer`).
    fn solved(&self, x: &[f64; N], f: &[f64; N]) -> bool;

    /// Whether a guess whose residual is below the desired accuracy is accepted as it is
    /// (upstream `IsConverged`). Every guess is, unless the system says otherwise.
    fn guess_converged(&self, x: &[f64; N], f: &[f64; N]) -> bool {
        let _ = (x, f);
        true
    }
}

impl<const N: usize> ConvergedNd<N> for Residual {
    fn solved(&self, _: &[f64; N], _: &[f64; N]) -> bool {
        false
    }
}

impl<const N: usize, F: Fn(&[f64; N], &[f64; N]) -> bool> ConvergedNd<N> for F {
    fn solved(&self, x: &[f64; N], f: &[f64; N]) -> bool {
        self(x, f)
    }
}

/// Finite-difference Jacobian of `f` at `x`, by central differences of step
/// `SM_EFF_ZERO_SQRT * (1 + |x_j|)` clamped into `domain`'s intervals
/// (`SmEvalNFunctionsObject::ComputeJacobian`): `jacobian[i][j] = dF_i/dx_j`, each
/// parameter perturbed in a copy of `x` in turn, low end first.
///
/// # Errors
///
/// A failed evaluation ([`SolveError::Eval`]), and [`SolveFailure::ZeroDifferenceStep`]
/// when a clamped step collapses below `SM_EFF_ZERO`.
pub fn jacobian<const N: usize, E, F>(
    f: &mut F,
    x: [f64; N],
    domain: &[Domain1d; N],
) -> Result<[[f64; N]; N], SolveError<E>>
where
    F: FnMut(&[f64; N]) -> Result<[f64; N], E>,
{
    const OP: &str = "geom::local_solve_nd::compute_jacobian";
    let mut jac = [[0.0; N]; N];
    for (jj, (&base, dom)) in x.iter().zip(domain).enumerate() {
        let delta = EFF_ZERO_SQRT * (1.0 + base.abs());
        let mut lo = base - delta;
        let mut hi = base + delta;
        if let Some(ivl) = dom.interval() {
            lo = ivl.clamp_value(lo);
            hi = ivl.clamp_value(hi);
        }

        // evaluate all equations at the low and the high end -- no Jacobian
        let f_lo = f(&replaced(x, jj, lo)).map_err(SolveError::Eval)?;
        let f_hi = f(&replaced(x, jj, hi)).map_err(SolveError::Eval)?;

        let delta = hi - lo; // use the actual difference, whatever happened
        if delta.abs() < EFF_ZERO {
            return Err(SolveError::Failed {
                operation: OP,
                failure: SolveFailure::ZeroDifferenceStep {
                    parameter: jj,
                    at: base,
                },
            });
        }

        // column jj by first order central differences
        for (row, (h, l)) in jac.iter_mut().zip(f_hi.iter().zip(&f_lo)) {
            put(row, jj, (h - l) / delta);
        }
    }
    Ok(jac)
}

/// `x` with entry `i` replaced by `v`.
fn replaced<const N: usize>(x: [f64; N], i: usize, v: f64) -> [f64; N] {
    let mut y = x;
    put(&mut y, i, v);
    y
}

/// Writes `v` at `i`, which every caller takes from an enumeration of an `N`-array.
fn put<const N: usize>(a: &mut [f64; N], i: usize, v: f64) {
    if let Some(slot) = a.get_mut(i) {
        *slot = v;
    }
}

fn sum_abs(v: &[f64]) -> f64 {
    let mut s = 0.0;
    for x in v {
        s += x.abs();
    }
    s
}

/// A point clamped into the domain ([`LocalSolveNd::clamp`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clamped<const N: usize> {
    /// The point: each bounded parameter clamped into its interval, each periodic one
    /// wrapped into its period, the rest as given.
    pub x: [f64; N],
    /// Whether clamping moved each parameter by more than `SM_EFF_ZERO` (wrapping is no
    /// hit).
    pub hit: [bool; N],
}

impl<const N: usize> Clamped<N> {
    /// Whether any parameter was hit.
    #[must_use]
    pub fn any(&self) -> bool {
        self.hit.iter().any(|&h| h)
    }
}

/// How a finished [`LocalSolveNd::solve`] ended.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NdReport<const N: usize> {
    /// The solution, or the best point when none was accepted.
    pub outcome: Outcome<[f64; N]>,
    /// Why the solve stopped.
    pub reason: TerminationReason,
    /// The iteration counter at the end (`m_lCurrentIteration`: the maximum when the
    /// iteration ran out, `0` when it stopped before the first).
    pub iterations: u64,
}

/// One evaluation outcome inside the solver: `Some(values)` on success, `None` on a
/// failed evaluation; an error that [ends the solve](EvalError::ends_solve) propagates.
fn evaluated<T, E: EvalError>(r: Result<T, E>) -> Result<Option<T>, SolveError<E>> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.ends_solve() => Err(SolveError::Eval(e)),
        Err(_) => Ok(None),
    }
}

/// An evaluated line-search candidate.
struct Candidate<const N: usize> {
    x: [f64; N],
    f: [f64; N],
    jac: [[f64; N]; N],
    sum: f64,
}

/// The options of a Newton-Raphson solve of `N` equations in `N` unknowns
/// (`SmLocalSolveNd`'s settings). A value: every [`LocalSolveNd::solve`] reads it and
/// changes nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalSolveNd<const N: usize, C = Residual> {
    /// Each parameter's domain (default: the whole line). A bounded parameter is clamped
    /// into its interval, a periodic one wrapped; an interval that is not bounded at
    /// both ends leaves its parameter free.
    pub domain: [Domain1d; N],
    /// The tight stopping tolerance on the sum of `|F_i|` (default `SM_EFF_ZERO / 10`).
    pub desired_accuracy: f64,
    /// The sum of `|F_i|` accepted when the desired accuracy cannot be reached (default
    /// `SM_EFF_ZERO`; upstream's `SolveIt` argument).
    pub acceptable_accuracy: f64,
    /// The iteration limit (default 100).
    pub max_iter: u64,
    /// The out-of-bounds exit rule (default [`BoundaryHandler::HitsAllBoundaries`]).
    pub boundary_handler: BoundaryHandler,
    /// Cut (and finally negate) the step when it makes the error worse (default off).
    pub step_cutting: bool,
    /// The system's own convergence test (default [`Residual`]).
    pub converged: C,
}

impl<const N: usize> Default for LocalSolveNd<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> LocalSolveNd<N> {
    /// The defaults (`SmLocalSolveNd::SmLocalSolveNd`): the whole line for every
    /// parameter, desired accuracy `SM_EFF_ZERO / 10`, acceptable accuracy `SM_EFF_ZERO`,
    /// 100 iterations, [`BoundaryHandler::HitsAllBoundaries`], no step cutting, the
    /// [`Residual`] test.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            domain: [Domain1d::Line; N],
            desired_accuracy: EFF_ZERO / 10.0,
            acceptable_accuracy: EFF_ZERO,
            max_iter: 100,
            boundary_handler: BoundaryHandler::HitsAllBoundaries,
            step_cutting: false,
            converged: Residual,
        }
    }
}

impl<const N: usize, C> LocalSolveNd<N, C> {
    /// These options with the convergence test `converged`.
    #[must_use]
    pub fn with_convergence<D: ConvergedNd<N>>(self, converged: D) -> LocalSolveNd<N, D> {
        LocalSolveNd {
            domain: self.domain,
            desired_accuracy: self.desired_accuracy,
            acceptable_accuracy: self.acceptable_accuracy,
            max_iter: self.max_iter,
            boundary_handler: self.boundary_handler,
            step_cutting: self.step_cutting,
            converged,
        }
    }

    /// `x` clamped into the domain (`SmExtentNd::ClampVector`): see [`Clamped`].
    #[must_use]
    pub fn clamp(&self, x: [f64; N]) -> Clamped<N> {
        let mut out = Clamped { x, hit: [false; N] };
        for ((v, hit), dom) in out.x.iter_mut().zip(&mut out.hit).zip(&self.domain) {
            match dom {
                Domain1d::Periodic(e) => *v = e.periodic_wrap(*v),
                Domain1d::Bounded(e) => {
                    let c = e.clamp_value(*v);
                    *hit = (c - *v).abs() > EFF_ZERO;
                    *v = c;
                }
                // the line: unchanged
                Domain1d::Line => {}
            }
        }
        out
    }

    /// [`LocalSolveNd::clamp`], adding each hit to `counts`.
    fn clamp_counting(&self, x: [f64; N], counts: &mut [u64; N]) -> Clamped<N> {
        let c = self.clamp(x);
        for (n, &hit) in counts.iter_mut().zip(&c.hit) {
            if hit {
                *n = n.wrapping_add(1);
            }
        }
        c
    }
}

impl<const N: usize, C: ConvergedNd<N>> LocalSolveNd<N, C> {
    /// Solves `f(x) = 0` starting at `guess` (`SmLocalSolveNd::SolveIt`). `f` returns the
    /// values and the Jacobian at `x` (`jacobian[i][j] = dF_i/dx_j`; [`jacobian`]
    /// computes one by differences).
    ///
    /// Not finding a solution is not an error: see [`NdReport`]. Failed evaluations
    /// supply no usable values. Each accepted step keeps the equations and Jacobian
    /// evaluated at that same point (WR-253).
    ///
    /// # Errors
    ///
    /// An evaluation's error that [ends the solve](EvalError::ends_solve), and
    /// [`SolveError::Num`] when the Jacobian's matrix cannot be built or its solve fails
    /// other than as a singular system.
    pub fn solve<E, F>(&self, mut f: F, guess: [f64; N]) -> Result<NdReport<N>, SolveError<E>>
    where
        E: EvalError,
        F: FnMut(&[f64; N]) -> Result<([f64; N], [[f64; N]; N]), E>,
    {
        let report = |outcome, reason, iterations| NdReport {
            outcome,
            reason,
            iterations,
        };
        let mut reason = TerminationReason::UnableToConverge;
        let mut iteration: u64 = 0;
        let mut x = guess;

        // first evaluation: equation values and gradients at the guess
        let Some((mut values, mut jac)) = evaluated(f(&x))? else {
            // An unevaluated guess is no solution (WR-20).
            return Ok(report(
                Outcome::Best(x),
                TerminationReason::GuessUnevaluated,
                iteration,
            ));
        };

        // exit case - the system declares the guess solved
        if self.converged.solved(&x, &values) {
            return Ok(report(
                Outcome::Root(x),
                TerminationReason::FoundAnswerConverged,
                iteration,
            ));
        }

        // accumulate the errors
        let mut sum_f = sum_abs(&values);

        // exit case - the initial guess was good enough
        if sum_f < self.desired_accuracy && self.converged.guess_converged(&x, &values) {
            return Ok(report(
                Outcome::Root(x),
                TerminationReason::FoundAnswerConverged,
                iteration,
            ));
        }

        // remember the best solution (upstream initialises it to SM_BIG_DOUBLE above)
        let mut best_sum_f = sum_f;
        let mut best_guess = guess;

        let mut boundary_hits: u64 = 0;
        let mut hit_all_bounds_count: u64 = 0;
        let mut clamped = false;
        let mut out_of_bounds_counts = [0u64; N];
        let mut matrix = Matrix::try_new(N, N)?;

        'iterations: while iteration < self.max_iter {
            // parameter steps from the Jacobian system J * deltas = -F
            for (i, row) in jac.iter().enumerate() {
                for (j, &v) in row.iter().enumerate() {
                    matrix.set_at(i, j, v)?;
                }
            }
            let rhs = values.map(|v| -v);
            let mut step = [0.0; N];
            match matrix.solve_linear_system(&rhs) {
                Ok(s) => {
                    // (the solution has the system's N entries)
                    for (d, v) in step.iter_mut().zip(s.x) {
                        *d = v;
                    }
                }
                Err(e) if is_bad_jacobian(&e) => {
                    // exit case - the Jacobian system is not solvable
                    reason = TerminationReason::BadJacobian;
                    break;
                }
                Err(e) => return Err(e.into()),
            }

            // line search: find a next guess that reduces the sum of |F|
            let mut step_scale = 1.0;
            let next = loop {
                // next guess and step size
                let mut step_size = 0.0;
                let mut next_guess = x;
                for (ng, &s) in next_guess.iter_mut().zip(&step) {
                    let this_step = step_scale * s;
                    step_size += this_step.abs();
                    *ng += this_step;
                }

                // exit condition - step size below the tight tolerance
                if step_size < self.desired_accuracy * self.desired_accuracy {
                    if sum_f < self.acceptable_accuracy {
                        // success - steps and function values are small
                        return Ok(report(
                            Outcome::Root(x),
                            TerminationReason::FoundAnswerClose,
                            iteration,
                        ));
                    }
                    // failure - function values are not small
                    let reason = if clamped {
                        TerminationReason::OutOfBounds
                    } else {
                        TerminationReason::UnableToConverge
                    };
                    return Ok(report(Outcome::Best(x), reason, iteration));
                }

                // clamp the next guess to the parameter bounds
                let first_x = self.clamp_counting(next_guess, &mut out_of_bounds_counts);
                clamped = first_x.any();

                // evaluate the equations at the next guess
                let first = evaluated(f(&first_x.x))?;
                if first.is_none() {
                    // exit case - the evaluation failed
                    reason = TerminationReason::UnableToConverge;
                    if !clamped {
                        break 'iterations;
                    }
                }
                // A candidate has a residual only if its evaluation succeeded.
                let mut candidate = first.map(|(f, jac)| Candidate {
                    x: first_x.x,
                    sum: sum_abs(&f),
                    f,
                    jac,
                });

                // if clamped, compare the unscaled and scaled locations
                if clamped {
                    // scale the step by the most severe clamping
                    let mut scale_factor = 1.0;
                    for ((&ng, &ngc), &cur) in next_guess.iter().zip(&first_x.x).zip(&x) {
                        let base_step = ng - cur;
                        if base_step.abs() > EFF_ZERO {
                            let this_scale = (ngc - cur) / base_step;
                            if this_scale < scale_factor {
                                scale_factor = this_scale;
                            }
                        }
                    }

                    // clamped noticeably (not completely truncated)?
                    if scale_factor < 1.0 - EFF_ZERO && scale_factor > 0.0 {
                        // the clamped-and-scaled location
                        let mut scaled = x;
                        for (ncs, &s) in scaled.iter_mut().zip(&step) {
                            let this_step = step_scale * s;
                            *ncs += scale_factor * this_step;
                        }
                        // fix round-off over the boundary by clamping again
                        let scaled = self.clamp_counting(scaled, &mut out_of_bounds_counts).x;

                        // The better evaluated candidate supplies all of its state.
                        if let Some((f2, jac2)) = evaluated(f(&scaled))? {
                            let scaled_sum = sum_abs(&f2);
                            if candidate.as_ref().is_none_or(|c| scaled_sum < c.sum) {
                                candidate = Some(Candidate {
                                    x: scaled,
                                    f: f2,
                                    jac: jac2,
                                    sum: scaled_sum,
                                });
                            }
                        }
                    }
                }

                // No evaluated candidate: keep the current point and its residual.
                let Some(c) = candidate else {
                    break 'iterations;
                };

                // small enough: call it good
                if self.converged.solved(&c.x, &c.f) || c.sum < self.desired_accuracy {
                    // the next guess is the solution
                    return Ok(report(
                        Outcome::Root(c.x),
                        TerminationReason::FoundAnswerConverged,
                        iteration,
                    ));
                }

                // the error grew: try smaller steps, then the opposite direction
                if c.sum >= sum_f
                    && self.step_cutting
                    && sum_f > self.acceptable_accuracy
                    && step_scale.abs() > 0.0001
                {
                    step_scale /= 10.0;
                    // failing miserably: perhaps a delta has the wrong sign
                    if step_scale > 0.0 && step_scale < 0.001 {
                        step_scale = -1.0;
                    }
                    continue;
                }
                break c;
            };
            let next_sum_f = next.sum;
            values = next.f;
            jac = next.jac;

            // stop when the error grows after 2 iterations but the last was good
            // enough (round-off noise after convergence)
            if iteration > 2 && next_sum_f >= sum_f && sum_f < self.acceptable_accuracy / 100.0 {
                reason = TerminationReason::FoundAnswerClose;
                break;
            }

            // stop when the error stops changing after 2 iterations and might not be
            // good enough
            let change_factor = 0.000001;
            if iteration > 2
                && (next_sum_f - sum_f).abs() < sum_f * change_factor
                && sum_f > self.acceptable_accuracy / 100.0
            {
                reason = TerminationReason::UnableToConverge;
                break;
            }

            // remember the best solution
            if next_sum_f < best_sum_f {
                best_sum_f = next_sum_f;
                best_guess = next.x;
            }

            // keep the last solution as best when no better next one was found and
            // the last one is good enough
            if next_sum_f > sum_f && sum_f < self.acceptable_accuracy && sum_f < best_sum_f {
                best_sum_f = sum_f;
                best_guess = x;
            }

            // set up the next iteration
            sum_f = next_sum_f;
            let mut step_size = 0.0;
            for (cur, &ng) in x.iter().zip(&next.x) {
                step_size += (cur - ng).abs();
            }
            x = next.x;

            // exit when the step size is near zero - fail
            if step_size < EFF_ZERO_SQ {
                reason = TerminationReason::UnableToConverge;
                break;
            }

            // out-of-bounds exit criteria
            if self.boundary_handler != BoundaryHandler::Ignore {
                let mut hits_all_boundaries = true;
                for ((count, dom), &s) in
                    out_of_bounds_counts.iter_mut().zip(&self.domain).zip(&step)
                {
                    if *count != 0 {
                        *count = 0;
                        // count a hit only when the step is large compared to the
                        // parameter's range (.001 of the span); only a bounded
                        // parameter is ever hit
                        if dom
                            .interval()
                            .is_some_and(|e| s.abs() > 1.0e-3 * e.length())
                        {
                            boundary_hits += 1;
                        }
                    } else {
                        hits_all_boundaries = false;
                        if self.boundary_handler == BoundaryHandler::HitsAllBoundaries {
                            break;
                        }
                    }
                }

                // count the times every parameter went out of bounds at once
                if hits_all_boundaries && N != 0 {
                    hit_all_bounds_count += 1;
                }

                // exit criteria - stepped out of bounds repeatedly
                let out = match self.boundary_handler {
                    BoundaryHandler::TotalHits { max } => boundary_hits > max,
                    BoundaryHandler::HitsAllBoundaries => hit_all_bounds_count > 2,
                    BoundaryHandler::Ignore => false,
                };
                if out {
                    reason = TerminationReason::OutOfBounds;
                    break;
                }
            }

            iteration += 1;
        }

        // use the best solution seen
        if sum_f > best_sum_f && best_sum_f.abs() < BIG_DOUBLE {
            sum_f = best_sum_f;
            x = best_guess;
            // A best guess that cannot be reevaluated is no solution (WR-253).
            if evaluated(f(&x))?.is_none() {
                return Ok(report(
                    Outcome::Best(x),
                    TerminationReason::UnableToConverge,
                    iteration,
                ));
            }
        }

        // converged
        if sum_f < self.desired_accuracy {
            return Ok(report(
                Outcome::Root(x),
                TerminationReason::FoundAnswerConverged,
                iteration,
            ));
        }

        // nearly converged
        if sum_f < self.acceptable_accuracy {
            return Ok(report(
                Outcome::Root(x),
                TerminationReason::FoundAnswerClose,
                iteration,
            ));
        }

        // failed to converge; the termination reason was already set
        Ok(report(Outcome::Best(x), reason, iteration))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scx_num::error::{Invalid, NumFailure};
    use scx_num::extent1d::Extent1d;

    /// The test equation sets' error: the classes the pin's tests use.
    #[derive(Debug, Clone, PartialEq)]
    enum TestErr {
        /// An upstream `SM_ERR` status (pin `SolidError::sm_err`, `SolidError::geom`).
        Sm(&'static str),
        /// Pin `SolidError::Unported`.
        Unported,
        /// A solver error returned by `jacobian` inside an evaluation.
        Solve(Box<SolveError<TestErr>>),
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

    impl From<SolveError<TestErr>> for TestErr {
        fn from(e: SolveError<TestErr>) -> Self {
            Self::Solve(Box::new(e))
        }
    }

    type Values<const N: usize> = Result<([f64; N], [[f64; N]; N]), TestErr>;

    /// `F_i = sum_j A_ij x_j + c_i x_i^3 - b_i`.
    #[derive(Clone, Copy)]
    struct Cubic<const N: usize> {
        a: [[f64; N]; N],
        c: [f64; N],
        b: [f64; N],
    }

    impl<const N: usize> Cubic<N> {
        fn values(&self, x: &[f64; N]) -> [f64; N] {
            let mut f = [0.0; N];
            for i in 0..N {
                let mut v = -self.b[i];
                for (a, xj) in self.a[i].iter().zip(x) {
                    v += a * xj;
                }
                v += self.c[i] * x[i] * x[i] * x[i];
                f[i] = v;
            }
            f
        }

        fn eval(&self, x: &[f64; N], fd: bool) -> Values<N> {
            let f = self.values(x);
            let jac = if fd {
                jacobian(
                    &mut |y: &[f64; N]| Ok(self.values(y)),
                    *x,
                    &[Domain1d::Line; N],
                )?
            } else {
                let mut jac = self.a;
                for i in 0..N {
                    jac[i][i] += 3.0 * self.c[i] * x[i] * x[i];
                }
                jac
            };
            Ok((f, jac))
        }
    }

    const CUBIC: Cubic<2> = Cubic {
        a: [[4.0, 1.0], [1.0, 3.0]],
        c: [0.5, 0.25],
        b: [1.0, 2.0],
    };

    fn bounds<const N: usize>(extents: [(f64, f64); N]) -> [Domain1d; N] {
        extents.map(|(lo, hi)| Domain1d::Bounded(Extent1d::spanning(lo, hi).unwrap()))
    }

    fn bounded<const N: usize>(extents: [(f64, f64); N]) -> LocalSolveNd<N> {
        LocalSolveNd {
            domain: bounds(extents),
            ..LocalSolveNd::new()
        }
    }

    fn accepting<const N: usize>(acceptable_accuracy: f64) -> LocalSolveNd<N> {
        LocalSolveNd {
            acceptable_accuracy,
            ..LocalSolveNd::new()
        }
    }

    #[test]
    fn converges_on_cubic_system() {
        for fd in [false, true] {
            let r = accepting(EFF_ZERO_SQRT)
                .solve(|x| CUBIC.eval(x, fd), [0.0, 0.0])
                .unwrap();
            assert_eq!(r.reason, TerminationReason::FoundAnswerConverged);
            let Outcome::Root(x) = r.outcome else {
                panic!("{r:?}");
            };
            let f = CUBIC.values(&x);
            assert!(f[0].abs() + f[1].abs() < 1e-9);
        }
    }

    #[test]
    fn singular_jacobian_is_bad_jacobian() {
        let e = Cubic {
            a: [[1.0, 1.0], [0.0, 0.0]],
            c: [0.0, 0.0],
            b: [1.0, 2.0],
        };
        let mut calls = 0;
        let r = accepting(EFF_ZERO_SQRT)
            .solve(
                |x| {
                    calls += 1;
                    e.eval(x, false)
                },
                [0.0, 0.0],
            )
            .unwrap();
        assert_eq!(r.outcome, Outcome::Best([0.0, 0.0]));
        assert_eq!(r.reason, TerminationReason::BadJacobian);
        assert_eq!(calls, 1);
    }

    #[test]
    fn exact_guess_returns_immediately() {
        let e = Cubic {
            a: [[1.0]],
            c: [0.0],
            b: [0.0],
        };
        let r = LocalSolveNd::new()
            .solve(|x| e.eval(x, false), [0.0])
            .unwrap();
        assert_eq!(r.outcome, Outcome::Root([0.0]));
        assert_eq!(r.reason, TerminationReason::FoundAnswerConverged);
        assert_eq!(r.iterations, 0);
    }

    /// An initial evaluation error cannot certify a zero residual.
    #[test]
    fn wr20_failing_first_evaluation_finds_no_solution_by_default() {
        let r = LocalSolveNd::new()
            .solve(
                |_: &[f64; 2]| -> Values<2> { Err(TestErr::Sm("test")) },
                [1.0, 2.0],
            )
            .unwrap();
        assert_eq!(
            r.outcome,
            Outcome::Best([1.0, 2.0]),
            "an unevaluated guess is not a solution"
        );
        assert_eq!(r.reason, TerminationReason::GuessUnevaluated);
        assert_eq!(r.iterations, 0);
    }

    /// `F = (x^2 - 1, y - 4)`.
    #[allow(clippy::unnecessary_wraps)] // the solver's closure type
    fn quadratic_and_linear(x: &[f64; 2]) -> Values<2> {
        Ok((
            [x[0] * x[0] - 1.0, x[1] - 4.0],
            [[2.0 * x[0], 0.0], [0.0, 1.0]],
        ))
    }

    /// A scaled candidate carries its own Jacobian into the next Newton step.
    #[test]
    fn scaled_step_uses_the_jacobian_at_its_own_point() {
        let s = LocalSolveNd {
            max_iter: 2,
            acceptable_accuracy: EFF_ZERO,
            ..bounded([(0.0, 1.5), (0.0, 1.0)])
        };
        let r = s.solve(quadratic_and_linear, [0.5, 0.0]).unwrap();
        // The first Newton point is (1.25, 4). Clamping gives (1.25, 1),
        // scaling gives (11/16, 1), whose residual is smaller. The next x is
        // Newton's (x + 1/x)/2 at 11/16, not at the rejected clamped point.
        let expected = (11.0 / 16.0 + 16.0 / 11.0) / 2.0;
        // y = 4 is outside the bounds.
        let Outcome::Best(x) = r.outcome else {
            panic!("{r:?}");
        };
        assert!((x[0] - expected).abs() < 1e-14, "{r:?}");
        assert_eq!(x[1], 1.0);
    }

    /// `F = (x - 3, y - 1/2)`, failing at `x = 1` with `y < 1/4` (`reject_scaled`) or
    /// `y >= 1/4`.
    fn bounded_linear(reject_scaled: bool) -> impl Fn(&[f64; 2]) -> Values<2> {
        move |x| {
            if x[0] == 1.0 && ((x[1] < 0.25) == reject_scaled) {
                return Err(TestErr::Sm("test::bounded_linear"));
            }
            Ok(([x[0] - 3.0, x[1] - 0.5], [[1.0, 0.0], [0.0, 1.0]]))
        }
    }

    /// A failed scaled evaluation cannot overwrite a successfully evaluated clamped candidate.
    #[test]
    fn failed_scaled_evaluation_cannot_replace_the_valid_clamped_step() {
        let s = bounded([(0.0, 1.0), (0.0, 1.0)]);
        let r = s.solve(bounded_linear(true), [0.0, 0.0]).unwrap();
        assert_eq!(r.outcome, Outcome::Best([1.0, 0.5]), "{r:?}");
    }

    /// A failed clamped evaluation preserves the last fully evaluated scaled candidate.
    #[test]
    fn failed_clamped_evaluation_keeps_the_last_valid_scaled_point() {
        let s = bounded([(0.0, 1.0), (0.0, 1.0)]);
        let r = s.solve(bounded_linear(false), [0.0, 0.0]).unwrap();
        let Outcome::Best(x) = r.outcome else {
            panic!("{r:?}");
        };
        assert_eq!(x[0], 1.0);
        assert!((x[1] - 1.0 / 6.0).abs() < 1e-14, "{r:?}");
    }

    /// A later evaluation error cannot turn stale residuals into a successful solution.
    #[test]
    fn a_failed_next_evaluation_cannot_create_a_solution() {
        let only_initial = |x: &[f64; 1]| -> Values<1> {
            if x[0] != 0.0 {
                return Err(TestErr::Sm("test::only_initial"));
            }
            Ok(([-2.0], [[1.0]]))
        };
        for s in [LocalSolveNd::new(), bounded([(0.0, 1.0)])] {
            let r = s.solve(only_initial, [0.0]).unwrap();
            assert_eq!(r.outcome, Outcome::Best([0.0]), "{s:?}: {r:?}");
            assert_eq!(r.reason, TerminationReason::UnableToConverge);
        }
    }

    /// The saved best point must still evaluate successfully before it can be returned.
    #[test]
    fn a_best_point_that_fails_reevaluation_is_not_a_solution() {
        let mut calls = 0;
        let s = LocalSolveNd {
            max_iter: 2,
            ..accepting(1.5)
        };
        // Newton cycles 0 -> 1 -> 0. The best point is 1, with residual 1,
        // but its mandatory reevaluation fails before it can be accepted.
        let r = s
            .solve(
                |x: &[f64; 1]| {
                    calls += 1;
                    if calls == 4 {
                        return Err(TestErr::Sm("test::best_reevaluation"));
                    }
                    Ok((
                        [x[0] * x[0] * x[0] - 2.0 * x[0] + 2.0],
                        [[3.0 * x[0] * x[0] - 2.0]],
                    ))
                },
                [0.0],
            )
            .unwrap();
        assert_eq!(r.outcome, Outcome::Best([1.0]), "{r:?}");
        assert_eq!(r.reason, TerminationReason::UnableToConverge);
    }

    #[test]
    fn unported_propagates() {
        assert!(matches!(
            LocalSolveNd::new().solve(
                |_: &[f64; 1]| -> Values<1> { Err(TestErr::Unported) },
                [1.0]
            ),
            Err(SolveError::Eval(TestErr::Unported))
        ));
    }

    /// A system's own test declares a point solved; the [`Residual`] test never does.
    #[test]
    fn residual_and_closure_convergence() {
        // F = x - 1 from 0: one Newton step lands on 1 exactly.
        let linear = |x: &[f64; 1]| -> Values<1> { Ok(([x[0] - 1.0], [[1.0]])) };
        let r = LocalSolveNd::new().solve(linear, [0.0]).unwrap();
        assert_eq!(r.outcome, Outcome::Root([1.0]));
        // A test accepting |F| <= 1 accepts the guess itself.
        let loose =
            LocalSolveNd::new().with_convergence(|_: &[f64; 1], f: &[f64; 1]| f[0].abs() <= 1.0);
        let r = loose.solve(linear, [0.0]).unwrap();
        assert_eq!(r.outcome, Outcome::Root([0.0]));
        assert_eq!(r.reason, TerminationReason::FoundAnswerConverged);
        assert_eq!(r.iterations, 0);
        assert!(!Residual.solved(&[0.0], &[0.0]));
        assert!(Residual.guess_converged(&[0.0], &[1.0]));
    }

    /// Bounded parameters clamp (a hit past `SM_EFF_ZERO`), periodic ones wrap (never a
    /// hit), the line and a value inside its interval leave the parameter as given.
    #[test]
    fn clamp_follows_the_domain() {
        let unit = Extent1d::spanning(0.0, 1.0).unwrap();
        let wide = Extent1d::spanning(-10.0, 10.0).unwrap();
        let s = LocalSolveNd {
            domain: [
                Domain1d::Line,
                Domain1d::Bounded(unit),
                Domain1d::Bounded(unit),
                Domain1d::Periodic(unit),
                Domain1d::Bounded(wide),
            ],
            ..LocalSolveNd::new()
        };
        let c = s.clamp([5.0, 1.5, 1.0 + EFF_ZERO / 2.0, 1.25, 7.0]);
        assert_eq!(c.x, [5.0, 1.0, 1.0, unit.periodic_wrap(1.25), 7.0]);
        assert_eq!(c.hit, [false, true, false, false, false]);
        assert!(c.any());
        assert!(!s.clamp([0.5; 5]).any());
    }

    #[test]
    fn jacobian_by_central_differences() {
        let x0 = [0.5, -1.0];
        let j = jacobian(
            &mut |x: &[f64; 2]| Ok::<_, TestErr>(CUBIC.values(x)),
            x0,
            &[Domain1d::Line; 2],
        )
        .unwrap();
        assert!((j[0][0] - (4.0 + 1.5 * 0.25)).abs() < 1e-6);
        assert!((j[1][1] - (3.0 + 0.75)).abs() < 1e-6);
        assert!((j[0][1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn jacobian_on_degenerate_interval_fails() {
        let domain = [
            Domain1d::Bounded(Extent1d::point(0.5).unwrap()),
            Domain1d::Bounded(Extent1d::spanning(-2.0, 2.0).unwrap()),
        ];
        let r = jacobian(
            &mut |x: &[f64; 2]| Ok::<_, TestErr>(CUBIC.values(x)),
            [0.5, -1.0],
            &domain,
        );
        assert_eq!(
            r.unwrap_err(),
            SolveError::Failed {
                operation: "geom::local_solve_nd::compute_jacobian",
                failure: SolveFailure::ZeroDifferenceStep {
                    parameter: 0,
                    at: 0.5
                },
            }
        );
    }

    /// #13790: a typed `SM_ERR` that replaced the linear solve's `Upstream` ends the
    /// iteration as a bad Jacobian, as the `Upstream` does. At layer 0 every `Failed` has
    /// status `SM_ERR` and `InvalidInput` has `SM_ERR_INVALID_INPUT`.
    #[test]
    fn bad_jacobian_reads_the_status_of_a_typed_failure() {
        let op = "geom::matrix::Matrix::solve_linear_system";
        let singular = NumError::failed(op, NumFailure::Singular { measure: 0.0 });
        assert!(is_bad_jacobian(&singular));
        assert!(is_bad_jacobian(&NumError::call(op)(singular)));
        assert!(!is_bad_jacobian(&NumError::InvalidInput {
            operation: op,
            reason: Invalid::SizeMismatch,
        }));
    }
}
