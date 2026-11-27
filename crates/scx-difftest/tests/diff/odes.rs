// Written for solidcorex. Apache-2.0.
//! The ODE integrator's side of the facade (report 53 section 13): an integrand that
//! records every call and fails, on request, with an error of a chosen class at a chosen
//! call, the same on both sides; the one-for-one map from `scx_algo::ode`'s errors onto the
//! pin's `SolidError`; and the two integrators behind one signature ([`x`], [`s`]).
//!
//! The integrand's error is the pin's own `SolidError` on both sides (wrapped in [`OErr`]
//! on scx's, which says which errors pass a failed step unchanged as the pin's
//! `integrate` does), so only the integrators' handling of it is compared. Errors are
//! compared by [`Fp`]: class, operation, typed reason (the pin's `Debug` of its `Reason`,
//! which holds no location), text, upstream status and, recursively, cause.

use crate::harness::{Bits, Out};
use scx_algo::error::{EvalError, SolveError, SolveFailure, StepControlFailure};
use scx_num::fmath;
use solidcore::error::mass::MassFailure;
use solidcore::error::nurbs::{BSplineFailure, NurbsFailure};
use solidcore::error::Failure;
use solidcore::SolidError;

/// An integrand error class the pin's integrator tells apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OClass {
    /// Passes a failed step unchanged (`nurbs_routine`).
    Cancelled,
    /// Passes unchanged.
    Unported,
    /// Passes unchanged.
    PreparationRequired,
    /// An ordinary failed evaluation (`SM_ERR`): wrapped as a failed routine.
    SmErr,
    /// Another refusal: wrapped.
    InvalidInput,
    /// A nested integration's step-control failure: passes unchanged.
    StepControl,
    /// A nested integration's non-finite integrand: passes unchanged.
    Integrand,
}

/// Every class, in a fixed order.
pub const OCLASSES: [OClass; 7] = [
    OClass::Cancelled,
    OClass::Unported,
    OClass::PreparationRequired,
    OClass::SmErr,
    OClass::InvalidInput,
    OClass::StepControl,
    OClass::Integrand,
];

/// The pin's error of class `c`.
pub fn pin_error(c: OClass) -> SolidError {
    use crate::solvers::{pin_error as solver_error, Class};
    match c {
        OClass::Cancelled => solver_error(Class::Cancelled),
        OClass::Unported => solver_error(Class::Unported),
        OClass::PreparationRequired => solver_error(Class::PreparationRequired),
        OClass::SmErr => solver_error(Class::SmErr),
        OClass::InvalidInput => solver_error(Class::InvalidInput),
        OClass::StepControl => {
            Failure::new("test::nested_integral", MassFailure::StepControl).into_error()
        }
        OClass::Integrand => {
            Failure::new("test::nested_integral", MassFailure::Integrand).into_error()
        }
    }
}

/// The scx side's integrand error: the pin's error, classified as the pin's `integrate`
/// classifies a failed step's (`SolidError::nurbs_routine` and its own pass-through).
#[derive(Debug, Clone)]
pub struct OErr(pub SolidError);

impl core::fmt::Display for OErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

impl EvalError for OErr {
    fn ends_solve(&self) -> bool {
        matches!(
            self.0,
            SolidError::Unported { .. }
                | SolidError::Cancelled { .. }
                | SolidError::OutOfFuel { .. }
        )
    }
    fn passes_through(&self) -> bool {
        self.ends_solve()
            || matches!(self.0, SolidError::PreparationRequired { .. })
            || self.0.is_not_certified()
            || matches!(
                self.0.mass_failure(),
                Some(MassFailure::StepControl | MassFailure::Integrand)
            )
    }
    fn never_ignored(&self) -> bool {
        self.0.is_unported_or_cancelled()
            || matches!(self.0, SolidError::PreparationRequired { .. })
    }
}

/// The integrator's error as the pin reports it, one for one (report 53 section 13).
pub fn ode_pin(e: SolveError<OErr>) -> SolidError {
    match e {
        SolveError::InvalidInput { operation, reason } => {
            SolidError::InvalidInput { operation, reason }
        }
        SolveError::Failed { operation, failure } => match failure {
            // the pin's step refuses the scale, and the step's error is wrapped
            SolveFailure::StepControl(StepControlFailure::Overflow) => Failure::new(
                operation,
                NurbsFailure::BSpline(BSplineFailure::RoutineFailed),
            )
            .caused_by(SolidError::InvalidInput {
                operation,
                reason: "invalid error scale",
            })
            .into_error(),
            SolveFailure::StepControl(_) => {
                Failure::new(operation, MassFailure::StepControl).into_error()
            }
            SolveFailure::IntegrandNotFinite => {
                Failure::new(operation, MassFailure::Integrand).into_error()
            }
            // (the local solvers' failures: never the integrator's)
            _ => SolidError::InvalidInput {
                operation,
                reason: "a solver failure the integrator does not make",
            },
        },
        SolveError::Num(n) => crate::solvers::num_pin(n),
        SolveError::CallFailed { operation, cause } => Failure::new(
            operation,
            NurbsFailure::BSpline(BSplineFailure::RoutineFailed),
        )
        .caused_by(cause.0)
        .into_error(),
        SolveError::Eval(e) => e.0,
    }
}

/// An integrator error, fingerprinted the same on both sides.
#[derive(Debug)]
pub struct Fp(pub SolidError);

impl Bits for Fp {
    fn bits(&self, o: &mut Out) {
        fp_bits(&self.0, o);
    }
}

fn fp_bits(e: &SolidError, o: &mut Out) {
    o.tag(&format!("{:?}", e.kind()));
    o.tag(e.operation());
    match e.failure() {
        // (`Failure`'s own `Debug` holds where it was built)
        Some(f) => o.tag(&format!("{:?}", f.reason)),
        None => o.tag(&format!("{e:?}")),
    }
    o.tag(&e.to_string());
    o.tag(&format!("{:?}", e.sm_status()));
    if let Some(c) = e.cause() {
        o.tag("caused by");
        fp_bits(c, o);
    }
}

/// The right-hand side `f(x, y)` of test system `func` (two components; a one-component
/// integration reads the first). Functions 0 to 8 read `x` alone (quadratures), 9 to 12
/// read `y`, 13 to 15 fail (a NaN past `x = 0.5`, a pole at `x = 0`, an overflowing
/// integral).
pub fn rhs(func: usize, x: f64, y: [f64; 2]) -> [f64; 2] {
    let [y0, y1] = y;
    match func {
        0 => [1.0, 2.0 * x],
        1 => [3.0 * x * x, fmath::cos(x)],
        2 => [x * x * x * x, x * x * x - x],
        3 => [x * x * x * x * x + 0.5 * x * x * x * x, 0.0],
        4 => [fmath::exp(x), -fmath::sin(x)],
        5 => [fmath::sin(x), 1.0 / (1.0 + x * x)],
        6 => [1.0 / (1.0 + x * x), 1.0 / (1.0 + 25.0 * x * x)],
        7 => [1.0 / (1.0 + 25.0 * x * x), 2.0 + fmath::sin(20.0 * x)],
        8 => [2.0 + fmath::sin(20.0 * x), fmath::exp(-x)],
        9 => [-50.0 * (y0 - fmath::cos(x)), y0 - y1],
        10 => [y0 + 1.0, -y0],
        11 => [y1 + 1.0, -y0],
        12 => [x - y0, y0 * y1 + 1.0],
        13 => [if x > 0.5 { f64::NAN } else { 1.0 }, 1.0],
        14 => [1.0 / x, 1.0],
        _ => [1e300 * (1.0 + x * x), 1.0],
    }
}

/// Test systems.
pub const FUNCS: usize = 16;
/// Test systems of `x` alone (quadratures), and the failing ones.
pub const QUADRATURES: [usize; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 13, 14, 15];

const fn fails(k: usize, fail_at: usize, persist: bool) -> bool {
    k == fail_at || (persist && k > fail_at)
}

/// An integrand that logs `(x, y, failed)` per call and fails at call `fail_at` (and every
/// later call when `persist`) with class `class`.
pub struct Rec {
    pub func: usize,
    pub fail_at: usize,
    pub persist: bool,
    pub class: OClass,
    pub log: Vec<(f64, [f64; 2], bool)>,
}

impl Rec {
    pub fn new(func: usize, fail_at: usize, persist: bool, class: OClass) -> Self {
        Self {
            func,
            fail_at,
            persist,
            class,
            log: Vec::new(),
        }
    }

    /// A clean integrand.
    pub fn clean(func: usize) -> Self {
        Self::new(func, usize::MAX, false, OClass::SmErr)
    }

    /// One call: the derivative's `N` components, or the pin's error of the class it fails
    /// with.
    pub fn call<const N: usize>(&mut self, x: f64, y: &[f64; N]) -> Result<[f64; N], SolidError> {
        let y2 = [
            y.first().copied().unwrap_or(0.0),
            y.get(1).copied().unwrap_or(0.0),
        ];
        let fail = fails(self.log.len(), self.fail_at, self.persist);
        self.log.push((x, y2, fail));
        if fail {
            return Err(pin_error(self.class));
        }
        let d = rhs(self.func, x, y2);
        Ok(core::array::from_fn(|i| d.get(i).copied().unwrap_or(0.0)))
    }

    /// One scalar call (component 0 of `x` alone).
    pub fn call1(&mut self, t: f64) -> Result<f64, SolidError> {
        self.call(t, &[0.0]).map(|[v]| v)
    }
}

/// One integration's controls and interval, as the pin's `OdeIntegrator::new` and
/// `OdeOptions` take them.
#[derive(Debug, Clone, Copy)]
pub struct Cfg<const N: usize> {
    pub accuracy: f64,
    pub initial_step: f64,
    pub minimum_step: f64,
    pub scale: Option<[f64; N]>,
    pub quadrature: bool,
    pub rounding: Option<[f64; N]>,
    pub rate: Option<f64>,
    pub max_steps: u32,
    pub tiny: f64,
    pub x1: f64,
    pub x2: f64,
}

/// An integration's outcome: the values and the step counts (good, reduced).
pub type Integral<const N: usize> = Result<([f64; N], (u32, u32)), Fp>;
/// A step's outcome: the values, the new `x`, the step taken and the next proposed.
pub type Stepped<const N: usize> = Result<([f64; N], f64, f64, f64), Fp>;

pub mod x {
    use super::{ode_pin, Cfg, Fp, Integral, OErr, Rec, Stepped};
    use scx_algo::ode::{self, OdeIntegrator, Quadrature, StepEstimate};

    pub fn integrate<const N: usize>(c: &Cfg<N>, rec: &mut Rec) -> Integral<N> {
        let ode = OdeIntegrator {
            scale: c.scale,
            estimate: if c.quadrature {
                StepEstimate::Quadrature
            } else {
                StepEstimate::Embedded
            },
            rounding: c.rounding,
            rate: c.rate,
            max_steps: c.max_steps,
            tiny: c.tiny,
            ..OdeIntegrator::new(c.accuracy, c.initial_step, c.minimum_step)
        };
        let s = ode
            .integrate(c.x1, c.x2, |x, y| rec.call(x, y).map_err(OErr))
            .map_err(|e| Fp(ode_pin(e)))?;
        Ok((s.values, (s.counts.good, s.counts.reduced)))
    }

    pub fn step<const N: usize>(
        y: [f64; N],
        dydx: [f64; N],
        x: f64,
        htry: f64,
        accuracy: f64,
        scale: [f64; N],
        rec: &mut Rec,
    ) -> Stepped<N> {
        let s = ode::step(&y, &dydx, x, htry, accuracy, scale, |x, y| {
            rec.call(x, y).map_err(OErr)
        })
        .map_err(|e| Fp(ode_pin(e)))?;
        Ok((s.values, s.x, s.taken, s.next))
    }

    pub fn quadrature(
        a: f64,
        b: f64,
        accuracy: f64,
        rounding: Option<f64>,
        rate: Option<f64>,
        rec: &mut Rec,
    ) -> Result<f64, Fp> {
        Quadrature {
            accuracy,
            rounding,
            rate,
        }
        .integrate(a, b, |t| rec.call1(t).map_err(OErr))
        .map_err(|e| Fp(ode_pin(e)))
    }
}

pub mod s {
    use super::{Cfg, Fp, Integral, Rec, Stepped};
    use solidcore::mass::integrator::{self, OdeIntegrator, OdeOptions};

    pub fn integrate<const N: usize>(c: &Cfg<N>, rec: &mut Rec) -> Integral<N> {
        let ode = OdeIntegrator::new(c.max_steps, c.tiny).map_err(Fp)?;
        let mut values = [0.0; N];
        let counts = ode
            .integrate(
                &mut values,
                c.x1,
                c.x2,
                OdeOptions {
                    accuracy: c.accuracy,
                    initial_step: c.initial_step,
                    minimum_step: c.minimum_step,
                    scale: c.scale.as_ref().map(|s| &s[..]),
                    quadrature: c.quadrature,
                    rounding: c.rounding.as_ref().map(|r| &r[..]),
                    rate: c.rate,
                },
                |x, y, dy| {
                    let y: [f64; N] = core::array::from_fn(|i| y[i]);
                    dy.copy_from_slice(&rec.call(x, &y)?);
                    Ok(())
                },
            )
            .map_err(Fp)?;
        Ok((values, (counts.good, counts.reduced)))
    }

    pub fn step<const N: usize>(
        y: [f64; N],
        dydx: [f64; N],
        x: f64,
        htry: f64,
        accuracy: f64,
        scale: [f64; N],
        rec: &mut Rec,
    ) -> Stepped<N> {
        let mut scale = scale;
        let s = OdeIntegrator::default()
            .step(&y, &dydx, x, htry, accuracy, &mut scale, |x, y, dy| {
                let y: [f64; N] = core::array::from_fn(|i| y[i]);
                dy.copy_from_slice(&rec.call(x, &y)?);
                Ok(())
            })
            .map_err(Fp)?;
        let values: [f64; N] = core::array::from_fn(|i| s.values[i]);
        Ok((values, s.x, s.taken, s.next))
    }

    /// `integrate` without rounding or rate (the pin's plain entry point), else
    /// `integrate_rounded_with_kernel` with a kernel that is never cancelled.
    pub fn quadrature(
        a: f64,
        b: f64,
        accuracy: f64,
        rounding: Option<f64>,
        rate: Option<f64>,
        rec: &mut Rec,
    ) -> Result<f64, Fp> {
        if rounding.is_none() && rate.is_none() {
            integrator::integrate(a, b, accuracy, |t| rec.call1(t)).map_err(Fp)
        } else {
            integrator::integrate_rounded_with_kernel(
                &solidcore::Kernel::default(),
                a,
                b,
                accuracy,
                rounding,
                rate,
                |t| rec.call1(t),
            )
            .map_err(Fp)
        }
    }
}

/// The pin's integrator with `fmath::powf` for the platform's `powf` in its two step
/// factors (the build script's `pin_integrator`; every other line is the pin's), behind
/// the same signature as [`s`]: the oracle for the inputs on which the platform's last bit
/// of `powf` moves the pin's steps (`pin_defects::ode_powf`). Use as `m` (`m::od::...`).
pub mod pinf {
    #[allow(clippy::all, clippy::pedantic, dead_code, unused_imports)]
    mod pin {
        use solidcore::error::mass::MassFailure;
        use solidcore::error::nurbs::{BSplineFailure, NurbsFailure};
        use solidcore::error::Failure;
        use solidcore::geom::tol::are_same;
        use solidcore::math::{EFF_ZERO, EFF_ZERO_SQ};
        use solidcore::{Kernel, Result, SolidError};
        /// The pin's `SolidError::mass` (`pub(crate)` there), as its file writes it.
        fn pin_mass(operation: &'static str, failure: MassFailure) -> SolidError {
            Failure::new(operation, failure).into_error()
        }
        /// The pin's `SolidError::nurbs_routine` (`pub(crate)` there), as its file writes it.
        fn pin_nurbs_routine(operation: &'static str) -> impl Fn(SolidError) -> SolidError {
            move |e| match e {
                SolidError::Unported { .. }
                | SolidError::Cancelled { .. }
                | SolidError::OutOfFuel { .. }
                | SolidError::PreparationRequired { .. } => e,
                e if e.is_not_certified() => e,
                _ => Failure::new(
                    operation,
                    NurbsFailure::BSpline(BSplineFailure::RoutineFailed),
                )
                .caused_by(e)
                .into_error(),
            }
        }
        include!(concat!(env!("OUT_DIR"), "/pin_integrator.rs"));
    }

    pub mod od {
        use super::pin::{self as integrator, OdeIntegrator, OdeOptions};
        use crate::odes::{Cfg, Fp, Integral, Rec};

        pub fn integrate<const N: usize>(c: &Cfg<N>, rec: &mut Rec) -> Integral<N> {
            let ode = OdeIntegrator::new(c.max_steps, c.tiny).map_err(Fp)?;
            let mut values = [0.0; N];
            let counts = ode
                .integrate(
                    &mut values,
                    c.x1,
                    c.x2,
                    OdeOptions {
                        accuracy: c.accuracy,
                        initial_step: c.initial_step,
                        minimum_step: c.minimum_step,
                        scale: c.scale.as_ref().map(|s| &s[..]),
                        quadrature: c.quadrature,
                        rounding: c.rounding.as_ref().map(|r| &r[..]),
                        rate: c.rate,
                    },
                    |x, y, dy| {
                        let y: [f64; N] = core::array::from_fn(|i| y[i]);
                        dy.copy_from_slice(&rec.call(x, &y)?);
                        Ok(())
                    },
                )
                .map_err(Fp)?;
            Ok((values, (counts.good, counts.reduced)))
        }

        /// `integrate` without rounding or rate (the pin's plain entry point), else
        /// `integrate_rounded_with_kernel` with a kernel that is never cancelled.
        pub fn quadrature(
            a: f64,
            b: f64,
            accuracy: f64,
            rounding: Option<f64>,
            rate: Option<f64>,
            rec: &mut Rec,
        ) -> Result<f64, Fp> {
            if rounding.is_none() && rate.is_none() {
                integrator::integrate(a, b, accuracy, |t| rec.call1(t)).map_err(Fp)
            } else {
                integrator::integrate_rounded_with_kernel(
                    &solidcore::Kernel::default(),
                    a,
                    b,
                    accuracy,
                    rounding,
                    rate,
                    |t| rec.call1(t),
                )
                .map_err(Fp)
            }
        }
    }
}
