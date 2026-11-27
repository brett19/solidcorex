// Written for solidcorex. Apache-2.0.
//! The local solvers' side of the facade (report 53 section 12): evaluators that record
//! every call and fail, on request, with an error of a chosen class at a chosen call, the
//! same on both sides; and the one-for-one map from `scx_algo::error::SolveError` onto the
//! pin's `SolidError`, through which the difftests compare the solvers' errors.
//!
//! The evaluators' error is the pin's own `SolidError` on both sides (wrapped in [`XErr`]
//! here, so that it can carry `EvalError`), so the injected errors are identical and only
//! the solvers' handling of them is compared. The pin's side implements its evaluator
//! traits over the recorders ([`Rec1`], [`RecN`]); the scx side passes closures over the
//! same recorders, and its options and outcomes are mapped onto the pin's here (the
//! status codes, research 57 M6, live only in this facade).

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
            SolidError::Unported { .. }
                | SolidError::Cancelled { .. }
                | SolidError::OutOfFuel { .. }
        )
    }
    fn passes_through(&self) -> bool {
        self.ends_solve() || matches!(self.0, SolidError::PreparationRequired { .. })
    }
    fn never_ignored(&self) -> bool {
        self.0.is_unported_or_cancelled()
            || matches!(self.0, SolidError::PreparationRequired { .. })
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
    let (a, b) = (
        x.first().copied().unwrap_or(0.0),
        x.get(1).copied().unwrap_or(0.0),
    );
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

/// Whether call `k` of a recorder failing at `fail_at` (and from there on when
/// `persist`) fails.
const fn fails(k: usize, fail_at: usize, persist: bool) -> bool {
    k == fail_at || (persist && k > fail_at)
}

/// A 1-D test function that logs `(t, failed)` per call and fails at call `fail_at` (and
/// every later call when `persist`) with class `class`.
pub struct Rec1 {
    pub func: usize,
    pub fail_at: usize,
    pub persist: bool,
    pub class: Class,
    pub log: Vec<(f64, bool)>,
}

impl Rec1 {
    pub fn new(func: usize, fail_at: usize, persist: bool, class: Class) -> Self {
        Self {
            func,
            fail_at,
            persist,
            class,
            log: Vec::new(),
        }
    }

    /// One call: `(F, F', found)`, or the class it fails with.
    pub fn call(&mut self, t: f64) -> Result<(f64, f64, bool), Class> {
        let fail = fails(self.log.len(), self.fail_at, self.persist);
        self.log.push((t, fail));
        if fail {
            Err(self.class)
        } else {
            Ok(f1(self.func, t))
        }
    }
}

/// An N-D equation set that logs `(x, jacobian asked, failed)` per call and fails as
/// [`Rec1`] does.
pub struct RecN {
    pub func: usize,
    pub fail_at: usize,
    pub persist: bool,
    pub class: Class,
    pub log: Vec<([f64; 2], bool, bool)>,
}

impl RecN {
    pub fn new(func: usize, fail_at: usize, persist: bool, class: Class) -> Self {
        Self {
            func,
            fail_at,
            persist,
            class,
            log: Vec::new(),
        }
    }

    /// One call: the values and the row-major Jacobian, or the class it fails with.
    pub fn call(&mut self, x: [f64; 2], jacobian: bool) -> Result<([f64; 2], [f64; 4]), Class> {
        let fail = fails(self.log.len(), self.fail_at, self.persist);
        self.log.push((x, jacobian, fail));
        if fail {
            Err(self.class)
        } else {
            Ok(fn_n(self.func, &x))
        }
    }
}

/// The diagonal system `x_i^2 + x_i - 0.75 = 0` (roots 0.5) in any number of unknowns,
/// counting its calls.
pub struct Diag {
    pub calls: usize,
}

impl Diag {
    pub fn call(&mut self, x: &[f64], f: &mut [f64], jac: &mut [f64]) {
        self.calls += 1;
        let n = x.len();
        for (i, (&xi, fi)) in x.iter().zip(f.iter_mut()).enumerate() {
            *fi = xi * xi + xi - 0.75;
            for j in 0..n {
                if let Some(d) = jac.get_mut(i * n + j) {
                    *d = if i == j { 2.0 * xi + 1.0 } else { 0.0 };
                }
            }
        }
    }
}

/// A solver error as `(Display, the pin's error)`.
pub type PinErr = (String, SolidError);

/// An iterate: `(t, F, F', step)`.
pub type Iv1 = (f64, f64, f64, f64);
/// A 1-D solve's final state: `((iter, curr, prev1, prev2), (lower bracket (t, F), upper
/// bracket (t, F)), (found accuracy, lower and upper bound hits, termination code))`.
pub type St1 = (
    (u64, Iv1, Option<Iv1>, Option<Iv1>),
    (Option<(f64, f64)>, Option<(f64, f64)>),
    (Option<f64>, u64, u64, i32),
);
/// An N-D solve's report: `((found, x), (termination code, iterations, guess
/// unevaluated))`.
pub type RepN = ((bool, [f64; 2]), (i32, u64, bool));
/// [`RepN`] for the diagonal system, of any size.
pub type RepD = ((bool, Vec<f64>), (i32, u64, bool));

/// The scx side.
pub mod x {
    use super::{f1, fd, found_n, solve_pin, Class, Diag, PinErr, Rec1, RecN, RepD, St1, XErr};
    use core::num::NonZeroUsize;
    use scx_algo::error::SolveError;
    use scx_algo::local_solve1d::{
        Bracket3, Domain1d, Goal, IterQueue1d, IterationValue, LocalSolve1d, Sample,
        SolverAlgorithm,
    };
    use scx_algo::local_solve_nd::{
        jacobian, BoundaryHandler, LocalSolveNd, NdReport, Outcome, TerminationReason,
    };
    use scx_num::extent1d::Extent1d;

    fn err(c: Class) -> XErr {
        XErr(super::pin_error(c))
    }

    /// A solver error as `(Display, the pin's error)`.
    pub fn pin(e: SolveError<XErr>) -> PinErr {
        (e.to_string(), solve_pin(e))
    }

    /// The pin's `SmLocalSolverAlgorithmType` code `code` (`SM_SA_BISECTION` solves as
    /// Newton; an unknown code keeps the default).
    pub const fn algorithm(code: i32) -> Option<SolverAlgorithm> {
        match code {
            0 | 1 => Some(SolverAlgorithm::Newton),
            2 => Some(SolverAlgorithm::Brent(Goal::Maximize)),
            3 => Some(SolverAlgorithm::Brent(Goal::Minimize)),
            _ => None,
        }
    }

    /// The pin's termination code of `r` and whether the guess went unevaluated.
    pub const fn reason(r: TerminationReason) -> (i32, bool) {
        match r {
            TerminationReason::FoundAnswerConverged => (1, false),
            TerminationReason::FoundAnswerClose => (2, false),
            TerminationReason::OutOfBounds => (3, false),
            TerminationReason::BadJacobian => (4, false),
            TerminationReason::UnableToConverge => (5, false),
            TerminationReason::GuessUnevaluated => (5, true),
        }
    }

    /// The interval spanning `a` and `b`, which the configurations give finite (and
    /// `solve1d_settings` filters to plain ends).
    fn finite(a: f64, b: f64) -> Extent1d {
        Extent1d::spanning(a, b).unwrap()
    }

    /// `(min, max, periodic)` as a domain; `None` the whole line.
    pub fn domain1d(ivl: Option<(f64, f64, bool)>) -> Domain1d {
        match ivl {
            None => Domain1d::Line,
            Some((a, b, true)) => Domain1d::Periodic(finite(a, b)),
            Some((a, b, false)) => Domain1d::Bounded(finite(a, b)),
        }
    }

    /// `(is root, t)`.
    fn found(o: Outcome<f64>) -> (bool, f64) {
        (o.is_root(), o.into_value())
    }

    /// `solve` on `ivl` with the algorithm of `alg`, accepting `acc`, with the state it
    /// left.
    pub fn solve1d(
        rec: &mut Rec1,
        ivl: Option<(f64, f64, bool)>,
        alg: i32,
        guess: f64,
        acc: f64,
    ) -> Result<((bool, f64), St1), PinErr> {
        let func = rec.func;
        let s = LocalSolve1d {
            domain: domain1d(ivl),
            algorithm: algorithm(alg).unwrap_or_default(),
            acceptable_accuracy: acc,
            ..LocalSolve1d::new()
        }
        .with_convergence(move |t: f64, _: f64| f1(func, t).2);
        let iv = |v: IterationValue| (v.t, v.f, v.f_prime, v.prev_step);
        let tf = |b: Sample| (b.t, b.f);
        let (o, st) = s
            .solve_traced(|t| rec.call(t).map(|(v, d, _)| (v, d)).map_err(err), guess)
            .map_err(pin)?;
        let state = (
            (st.iter, iv(st.curr), st.prev1.map(iv), st.prev2.map(iv)),
            (st.lower.map(tf), st.upper.map(tf)),
            (
                st.found_accuracy,
                st.lower_bound_hits,
                st.upper_bound_hits,
                reason(st.termination_reason).0,
            ),
        );
        Ok((found(o), state))
    }

    /// `solve_by_brent` from `b` between `a` and `c`: `(t, F)`.
    pub fn brent(
        rec: &mut Rec1,
        bracket: (f64, f64, f64),
        tol: f64,
        maximize: bool,
    ) -> Result<(f64, f64), PinErr> {
        let (a, b, c) = bracket;
        let goal = if maximize {
            Goal::Maximize
        } else {
            Goal::Minimize
        };
        LocalSolve1d::new()
            .solve_by_brent(
                |t| rec.call(t).map(|(v, d, _)| (v, d)).map_err(err),
                Bracket3 { a, b, c },
                tol,
                goal,
            )
            .map(|s| (s.t, s.f))
            .map_err(pin)
    }

    /// `solve_by_bisection` between `(lo, flo)` and `(hi, fhi)`: `(t, F)`.
    pub fn bisection(
        rec: &mut Rec1,
        levels: u64,
        lo: (f64, f64),
        hi: (f64, f64),
    ) -> Result<(f64, f64), PinErr> {
        let func = rec.func;
        LocalSolve1d::new()
            .with_convergence(move |t: f64, _: f64| f1(func, t).2)
            .solve_by_bisection(
                |t| rec.call(t).map(|(v, _, _)| v).map_err(err),
                Sample { t: lo.0, f: lo.1 },
                Sample { t: hi.0, f: hi.1 },
                levels,
            )
            .map(|s| (s.t, s.f))
            .map_err(pin)
    }

    /// The finite-difference Jacobian of equation set `func`'s values on `fd`'s
    /// intervals (`None` the whole line).
    fn fd_jacobian(
        rec: &mut RecN,
        x: [f64; 2],
        ivl: Option<[[f64; 2]; 2]>,
    ) -> Result<[[f64; 2]; 2], SolveError<XErr>> {
        let domain = ivl.map_or([Domain1d::Line; 2], |a| {
            a.map(|[lo, hi]| Domain1d::Bounded(finite(lo, hi)))
        });
        let mut values = |p: &[f64; 2]| rec.call(*p, false).map(|(v, _)| v).map_err(err);
        jacobian(&mut values, x, &domain)
    }

    /// The solver's view of equation set `rec.func`: values and Jacobian, by differences
    /// where the set computes it so.
    fn eval_n(rec: &mut RecN, x: [f64; 2]) -> Result<([f64; 2], [[f64; 2]; 2]), XErr> {
        let (vals, jac) = rec.call(x, true).map_err(err)?;
        let jac = match fd(rec.func) {
            Some(ivl) => fd_jacobian(rec, x, ivl).map_err(|e| XErr(solve_pin(e)))?,
            None => [[jac[0], jac[1]], [jac[2], jac[3]]],
        };
        Ok((vals, jac))
    }

    /// `(termination code, iterations, guess unevaluated)` and `(found, x)` of a report.
    fn report<const N: usize>(r: &NdReport<N>) -> ((bool, [f64; N]), (i32, u64, bool)) {
        let (code, unevaluated) = reason(r.reason);
        (
            (r.outcome.is_root(), *r.outcome.value()),
            (code, r.iterations, unevaluated),
        )
    }

    /// An N-D solve: intervals `ivl` (the first parameter periodic when `periodic`),
    /// boundary handler `handler` (0 total hits, at most 3; 1 hits all; 2 ignore).
    #[allow(clippy::too_many_arguments)]
    pub fn solve_nd(
        rec: &mut RecN,
        ivl: Option<[[f64; 2]; 2]>,
        periodic: bool,
        handler: u8,
        cutting: bool,
        guess: [f64; 2],
        maxit: u64,
        acc: f64,
    ) -> Result<super::RepN, PinErr> {
        let func = rec.func;
        let domain = ivl.map_or([Domain1d::Line; 2], |a| {
            let mut d = [Domain1d::Line; 2];
            for (i, (di, [lo, hi])) in d.iter_mut().zip(a).enumerate() {
                let e = finite(lo, hi);
                *di = if periodic && i == 0 {
                    Domain1d::Periodic(e)
                } else {
                    Domain1d::Bounded(e)
                };
            }
            d
        });
        let s = LocalSolveNd {
            domain,
            acceptable_accuracy: acc,
            max_iter: maxit,
            boundary_handler: match handler {
                0 => BoundaryHandler::TotalHits { max: 3 },
                1 => BoundaryHandler::HitsAllBoundaries,
                _ => BoundaryHandler::Ignore,
            },
            step_cutting: cutting,
            ..LocalSolveNd::new()
        }
        .with_convergence(move |_: &[f64; 2], f: &[f64; 2]| found_n(func, f));
        s.solve(|x: &[f64; 2]| eval_n(rec, *x), guess)
            .map(|r| report(&r))
            .map_err(pin)
    }

    /// The finite-difference Jacobian of `rec` at `x`, row-major.
    pub fn jacobian_at(
        rec: &mut RecN,
        x: [f64; 2],
        ivl: Option<[[f64; 2]; 2]>,
    ) -> Result<[f64; 4], PinErr> {
        fd_jacobian(rec, x, ivl)
            .map(|[[a, b], [c, d]]| [a, b, c, d])
            .map_err(pin)
    }

    fn diag<const N: usize>(d: &mut Diag) -> Result<RepD, PinErr> {
        let s = LocalSolveNd::<N> {
            domain: [Domain1d::Bounded(finite(-2.0, 2.0)); N],
            acceptable_accuracy: 1e-9,
            ..LocalSolveNd::new()
        };
        let r = s
            .solve(
                |x: &[f64; N]| {
                    let mut f = [0.0; N];
                    let mut jac = vec![0.0; N * N];
                    d.call(x, &mut f, &mut jac);
                    let j: [[f64; N]; N] = core::array::from_fn(|i| {
                        core::array::from_fn(|k| jac.get(i * N + k).copied().unwrap_or(0.0))
                    });
                    Ok::<_, XErr>((f, j))
                },
                [0.25; N],
            )
            .map_err(pin)?;
        let ((found, x), st) = report(&r);
        Ok(((found, x.to_vec()), st))
    }

    /// The diagonal system in `n` unknowns (0 to 4) from 0.25 on `[-2, 2]`: the
    /// report, and the number of calls.
    pub fn solve_diag(n: usize) -> (Result<RepD, PinErr>, usize) {
        let mut d = Diag { calls: 0 };
        let r = match n {
            0 => diag::<0>(&mut d),
            1 => diag::<1>(&mut d),
            2 => diag::<2>(&mut d),
            3 => diag::<3>(&mut d),
            _ => diag::<4>(&mut d),
        };
        (r, d.calls)
    }

    /// A queue of `size` slots: whether `try_new` allocates it, `try_new` of the largest
    /// size, and `is_oscillating` after each of 20 alternating steps.
    pub fn queue(size: NonZeroUsize) -> (bool, Result<(), String>, Vec<bool>) {
        let ok = IterQueue1d::try_new(size).is_ok();
        let big = IterQueue1d::try_new(NonZeroUsize::MAX)
            .map(|_| ())
            .map_err(|e| e.to_string());
        let mut q = IterQueue1d::new(size);
        let mut osc = Vec::new();
        for i in 0..20u32 {
            q.add(if i % 2 == 0 { 1.0 } else { -1.01 });
            osc.push(q.is_oscillating());
        }
        (ok, big, osc)
    }

    /// The settings: whether the algorithm of `code` is a Brent one, the default desired
    /// accuracy, the interval `[lo, hi]` and desired accuracy `|acc|` as set, and a solve
    /// of function 0 from 1 with them, accepting 1e-3.
    #[allow(clippy::type_complexity)]
    pub fn settings(
        code: i32,
        lo: f64,
        hi: f64,
        acc: f64,
    ) -> (
        Option<bool>,
        f64,
        (f64, f64, f64),
        Result<(bool, f64), PinErr>,
        Vec<(f64, bool)>,
    ) {
        let brent = algorithm(code).map(|a| a.goal().is_some());
        let d = LocalSolve1d::default();
        let s = LocalSolve1d {
            domain: Domain1d::Bounded(finite(lo, hi)),
            desired_accuracy: acc.abs(),
            acceptable_accuracy: 1e-3,
            ..LocalSolve1d::new()
        };
        // Bounded just above.
        let ivl = s.domain.interval().unwrap();
        let mut rec = Rec1::new(0, usize::MAX, false, Class::SmErr);
        let r = s
            .solve(|t| rec.call(t).map(|(v, d, _)| (v, d)).map_err(err), 1.0)
            .map(found)
            .map_err(pin);
        (
            brent,
            d.desired_accuracy,
            (ivl.min(), ivl.max(), s.desired_accuracy),
            r,
            rec.log,
        )
    }
}

/// The pin's side.
pub mod s {
    use super::{pin_error, Class, Diag, PinErr, Rec1, RecN, RepD, St1};
    use core::num::NonZeroUsize;
    use solidcore::geom::extent1d::{Extent1d, ExtentNd};
    use solidcore::geom::local_solve1d::{
        EvalFunction, FunctionValue, IterQueue1d, IterationValue, LocalSolve1d, SolverAlgorithm,
    };
    use solidcore::geom::local_solve_nd::{
        compute_jacobian, BoundaryHandler, EvalNFunctions, LocalSolveNd,
    };
    use solidcore::geom::matrix::Matrix;
    use solidcore::SolidError;

    /// A solver error as `(Display, the error)`.
    pub fn pin(e: SolidError) -> PinErr {
        (e.to_string(), e)
    }

    impl EvalFunction for Rec1 {
        fn evaluate(&mut self, t: f64, out: &mut FunctionValue, _: bool) -> Result<(), SolidError> {
            match self.call(t) {
                Err(c) => {
                    // as `SmCurvePropertyEFO::Evaluate`: zeroed outputs, then the error
                    *out = FunctionValue::default();
                    Err(pin_error(c))
                }
                Ok((f, f_prime, found)) => {
                    out.f = f;
                    out.f_prime = f_prime;
                    out.found_answer = found;
                    Ok(())
                }
            }
        }
    }

    /// `x` as the two unknowns of a test equation set.
    fn two(x: &[f64]) -> [f64; 2] {
        [
            x.first().copied().unwrap_or(0.0),
            x.get(1).copied().unwrap_or(0.0),
        ]
    }

    impl EvalNFunctions for RecN {
        fn evaluate(
            &mut self,
            x: &[f64],
            f: &mut [f64],
            jacobian: Option<&mut Matrix>,
        ) -> Result<bool, SolidError> {
            let (vals, jac) = match self.call(two(x), jacobian.is_some()) {
                Ok(v) => v,
                Err(c) => {
                    f.fill(0.0);
                    return Err(pin_error(c));
                }
            };
            for (d, v) in f.iter_mut().zip(vals) {
                *d = v;
            }
            if let Some(jm) = jacobian {
                if let Some(ivls) = super::fd(self.func) {
                    let mut xx = x.to_vec();
                    let ff = f.to_vec();
                    let e = ivls.map(|a| a.map(|[lo, hi]| Extent1d::from_min_max(lo, hi)));
                    compute_jacobian(self, &mut xx, &ff, jm, e.as_ref().map(|a| &a[..]))?;
                } else {
                    for i in 0..2 {
                        for j in 0..2 {
                            jm.set_at(i, j, jac[i * 2 + j])?;
                        }
                    }
                }
            }
            Ok(super::found_n(self.func, &vals))
        }
    }

    /// The diagonal system in `x.len()` unknowns.
    struct DiagN<'a>(&'a mut Diag);

    impl EvalNFunctions for DiagN<'_> {
        fn evaluate(
            &mut self,
            x: &[f64],
            f: &mut [f64],
            jacobian: Option<&mut Matrix>,
        ) -> Result<bool, SolidError> {
            let n = x.len();
            let mut jac = vec![0.0; n * n];
            self.0.call(x, f, &mut jac);
            if let Some(jm) = jacobian {
                for i in 0..n {
                    for j in 0..n {
                        jm.set_at(i, j, jac[i * n + j])?;
                    }
                }
            }
            Ok(false)
        }
    }

    /// An unset iterate (`SM_UNDEF_DOUBLE`) as `None`.
    fn set(v: &IterationValue) -> Option<super::Iv1> {
        (v.t.to_bits() != solidcore::math::UNDEF_DOUBLE.to_bits()).then_some((
            v.t,
            v.f,
            v.f_prime,
            v.prev_step,
        ))
    }

    /// A solver on `ivl` (`(min, max, periodic)`) with the algorithm of `alg`, if any.
    fn solver1d(ivl: Option<(f64, f64, bool)>, alg: i32) -> LocalSolve1d {
        let mut s = match ivl {
            None => LocalSolve1d::new(),
            Some((a, b, p)) => LocalSolve1d::with_interval(Extent1d::from_min_max(a, b), p),
        };
        if let Some(a) = SolverAlgorithm::from_code(alg) {
            s.set_solver_algorithm(a);
        }
        s
    }

    /// `solve_it`, with the members it left. The found accuracy is `None` where the solve
    /// left it at `SM_UNDEF_DOUBLE` (not written).
    pub fn solve1d(
        rec: &mut Rec1,
        ivl: Option<(f64, f64, bool)>,
        alg: i32,
        guess: f64,
        acc: f64,
    ) -> Result<((bool, f64), St1), PinErr> {
        let mut s = solver1d(ivl, alg);
        let r = s.solve_it(rec, guess, acc).map_err(pin)?;
        let fa = s.found_accuracy();
        let bracket = |have: bool, t: f64, f: f64| have.then_some((t, f));
        let state = (
            (
                s.iter,
                set(&s.curr).unwrap_or((f64::NAN, f64::NAN, f64::NAN, f64::NAN)),
                set(&s.prev1),
                set(&s.prev2),
            ),
            (
                bracket(s.have_lower_bracket, s.lower_bracket_t, s.lower_bracket_f),
                bracket(s.have_upper_bracket, s.upper_bracket_t, s.upper_bracket_f),
            ),
            (
                (fa.to_bits() != solidcore::math::UNDEF_DOUBLE.to_bits()).then_some(fa),
                s.lower_bound_hits(),
                s.upper_bound_hits(),
                s.termination_reason().code(),
            ),
        );
        Ok(((r.found, r.t), state))
    }

    /// `solve_by_brent` from `b` between `a` and `c`: `(t, F)`, `F` as evaluated (the pin
    /// returns `-F` when maximising).
    pub fn brent(
        rec: &mut Rec1,
        bracket: (f64, f64, f64),
        tol: f64,
        maximize: bool,
    ) -> Result<(f64, f64), PinErr> {
        let (a, b, c) = bracket;
        LocalSolve1d::new()
            .solve_by_brent(rec, a, b, c, tol, maximize)
            .map(|(t, f)| (t, if maximize { -f } else { f }))
            .map_err(pin)
    }

    /// `solve_by_bisection` between `(lo, flo)` and `(hi, fhi)`: `(t, F)`.
    pub fn bisection(
        rec: &mut Rec1,
        levels: u64,
        lo: (f64, f64),
        hi: (f64, f64),
    ) -> Result<(f64, f64), PinErr> {
        LocalSolve1d::new()
            .solve_by_bisection(rec, levels, lo.0, hi.0, lo.1, hi.1)
            .map_err(pin)
    }

    /// An N-D solve: intervals `ivl` (the first parameter periodic when `periodic`),
    /// boundary handler `handler` (0 total hits, at most 3; 1 hits all; 2 ignore).
    #[allow(clippy::too_many_arguments)]
    pub fn solve_nd(
        rec: &mut RecN,
        ivl: Option<[[f64; 2]; 2]>,
        periodic: bool,
        handler: u8,
        cutting: bool,
        guess: [f64; 2],
        maxit: u64,
        acc: f64,
    ) -> Result<super::RepN, PinErr> {
        let ivls = ivl.map(|a| {
            let mut e = ExtentNd::new(2);
            for (i, [lo, hi]) in a.into_iter().enumerate() {
                if let Some(s) = e.get_mut(i) {
                    *s = Extent1d::from_min_max(lo, hi);
                }
            }
            e
        });
        let per = [true, false];
        let periodicities = periodic.then_some(&per[..]);
        let mut s = LocalSolveNd::new(rec, ivls.as_ref(), periodicities);
        s.set_step_cutting_enabled(cutting);
        s.set_maximum_iterations(maxit);
        let handler = match handler {
            0 => BoundaryHandler::TotalBoundaryHits,
            1 => BoundaryHandler::HitsAllBoundaries,
            _ => BoundaryHandler::IgnoreBoundaryHits,
        };
        s.set_boundary_handler(handler, 3);
        let r = s.solve_it(&guess, acc).map_err(pin)?;
        Ok((
            (r.found, two(&r.x)),
            (
                s.termination_reason().code(),
                s.current_iteration(),
                s.guess_unevaluated(),
            ),
        ))
    }

    /// The finite-difference Jacobian of `rec` at `x`, row-major. `x` after the call is not
    /// read: scx takes it by value, and the pin's in-place perturbation (left in place on a
    /// zero step) is the out-parameter research 57 M6 removes.
    pub fn jacobian_at(
        rec: &mut RecN,
        x: [f64; 2],
        ivl: Option<[[f64; 2]; 2]>,
    ) -> Result<[f64; 4], PinErr> {
        let e = ivl.map(|v| v.map(|[lo, hi]| Extent1d::from_min_max(lo, hi)));
        let mut xx = x;
        let f = [0.0, 0.0];
        let mut j = Matrix::new(2, 2);
        compute_jacobian(rec, &mut xx, &f, &mut j, e.as_ref().map(|v| &v[..]))
            .map(|()| core::array::from_fn(|i| j.at(i / 2, i % 2).unwrap_or(f64::NAN)))
            .map_err(pin)
    }

    /// The diagonal system in `n` unknowns (0 to 4) from 0.25 on `[-2, 2]`: the
    /// report, and the number of calls.
    pub fn solve_diag(n: usize) -> (Result<RepD, PinErr>, usize) {
        let n = n.min(4);
        let mut d = Diag { calls: 0 };
        let mut e = ExtentNd::new(n);
        for i in 0..n {
            if let Some(s) = e.get_mut(i) {
                *s = Extent1d::from_min_max(-2.0, 2.0);
            }
        }
        let guess = vec![0.25; n];
        let mut ev = DiagN(&mut d);
        let mut s = LocalSolveNd::new(&mut ev, Some(&e), None);
        let r = s.solve_it(&guess, 1e-9).map_err(pin).map(|o| {
            (
                (o.found, o.x),
                (
                    s.termination_reason().code(),
                    s.current_iteration(),
                    s.guess_unevaluated(),
                ),
            )
        });
        (r, d.calls)
    }

    /// A queue of `size` slots: whether `try_new` allocates it, `try_new` of the largest
    /// size, and `is_oscillating` after each of 20 alternating steps.
    pub fn queue(size: NonZeroUsize) -> (bool, Result<(), String>, Vec<bool>) {
        let size = size.get() as u64;
        let ok = IterQueue1d::try_new(size).is_ok();
        let big = IterQueue1d::try_new(u64::MAX)
            .map(|_| ())
            .map_err(|e| e.to_string());
        let mut q = IterQueue1d::new(size);
        let mut osc = Vec::new();
        for i in 0..20u32 {
            q.add(&IterationValue {
                prev_step: if i % 2 == 0 { 1.0 } else { -1.01 },
                ..Default::default()
            });
            osc.push(q.is_oscillating(2));
        }
        (ok, big, osc)
    }

    /// As the scx side's `settings`.
    #[allow(clippy::type_complexity)]
    pub fn settings(
        code: i32,
        lo: f64,
        hi: f64,
        acc: f64,
    ) -> (
        Option<bool>,
        f64,
        (f64, f64, f64),
        Result<(bool, f64), PinErr>,
        Vec<(f64, bool)>,
    ) {
        let brent = SolverAlgorithm::from_code(code).map(SolverAlgorithm::is_brent);
        let d = LocalSolve1d::default();
        let mut s = LocalSolve1d::with_interval(Extent1d::from_min_max(-3.0, 3.0), false);
        s.set_solution_interval(Extent1d::from_min_max(lo, hi));
        s.set_desired_accuracy(acc.abs());
        let ivl = s.interval();
        let mut rec = Rec1::new(0, usize::MAX, false, Class::SmErr);
        let r = s
            .solve_it(&mut rec, 1.0, 1e-3)
            .map(|o| (o.found, o.t))
            .map_err(pin);
        (
            brent,
            d.desired_accuracy(),
            (ivl.min(), ivl.max(), s.desired_accuracy()),
            r,
            rec.log,
        )
    }
}
