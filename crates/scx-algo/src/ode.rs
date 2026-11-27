// Moved from solidcore `solidcore/crates/solidcore/src/mass/integrator.rs` (the ODE core: the
// file above `pub mod properties`, without `integrate_with_kernel`) at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/inc/SmIntegrator.h`,
// `source/SMLib/src/SmIntegrator.cpp`.
// Changes: the integration is an options value and a pure call over a closure (research 57
// M6, report 53 section 13). `OdeIntegrator` holds every control: the pin's `OdeOptions`
// fields and its private `max_steps` and `tiny` are its public fields, so `OdeOptions`,
// `OdeIntegrator::new` (its refusal is `OdeIntegrator::integrate`'s first check) and
// `Default` are gone, and `OdeIntegrator::new` is the constructor of the defaults. The
// component count is the const parameter `N`: values, derivatives, scales and rounding
// bounds are `[f64; N]`, so the dimension checks and `at` are gone, and the integrand
// returns its derivative (`FnMut(f64, &[f64; N]) -> Result<[f64; N], E>`) where the pin
// wrote it into a zeroed `&mut [f64]`. `OdeIntegrator::integrate` returns an
// `OdeSolution` (values and `StepCounts`) where the pin wrote the values into `initial`, an
// in-out slice whose values it never read: the values start at zero, as the pin's did,
// and the early returns give zeros. `quadrature: bool` is `StepEstimate`. `step` is a free
// function of the step's start, its trial step, the accuracy and the error scale (taken
// by value: the pin floored the caller's slice in place, which no caller read), returning
// `OdeStep` with an array; `step_in` takes the scale by value and returns `Accepted`, the
// step's values and owed errors, where the pin left them in `CashKarpScratch`, which is
// gone with `CashKarpScratch::resize`, `StepEnd` and `step_samples` (`cash_karp` returns
// `CashKarp`, its values, error estimates and each component's six samples). `validate`,
// `validate_scale`, `validate_rounding` and `sample_rounding` read arrays. `gamma` is
// `scx_num::certified::gamma` (the same `n u / (1 - n u)`); `powf` is
// `scx_num::fmath::powf`. Errors are `SolveError<E>` over the integrand's error
// (`EvalError`): `MassFailure::StepControl` is `SolveFailure::StepControl` with its reason
// (`StepControlFailure`), `MassFailure::Integrand` is `SolveFailure::IntegrandNotFinite`
// (`step_control_failed` and `integrand_failed` build them), the evaluation at a step's
// start returns the integrand's error unchanged (`SolveError::Eval`) and one inside a step
// as `SolveError::call(OP)` (the pin's `SolidError::nurbs_routine(OP)`, which passes the same
// classes and the pin's own step-control and integrand failures: here the integrand's
// error says which pass through). The scalar entry points `integrate`,
// `integrate_rounded_with_kernel` and `integrate_scalar` are `Quadrature::integrate`, whose
// options are the accuracy, `rounding` and `rate`; the `Kernel` they read only to check
// cancellation (before starting, then at every 64th evaluation) is gone: a caller that can
// be cancelled checks in its integrand, whose error ends the integration. A step's error
// scale that is not a number (an overflow: `0 * inf` for a step grown past the largest
// float, or the values') is `StepControlFailure::Overflow` on `OP`, where the pin's step
// refused it as input and the integrator wrapped that as a failed NURBS routine. Results
// unchanged but for `powf`'s last bit, which can move the steps (`fmath::powf` is not the
// platform's).
//! Adaptive Cash–Karp integration of a system of `N` ordinary differential equations
//! `y' = f(x, y)` from `y(x1) = 0`, and of a scalar integrand as a quadrature.
//!
//! Upstream: `source/SMLib/inc/SmIntegrator.h`, `source/SMLib/src/SmIntegrator.cpp`.
//!
//! | Rust | Upstream |
//! |------|----------|
//! | [`Quadrature::integrate`] | `SmIntegrator::IntegrateIt`, `SmODEEval` |
//! | [`OdeIntegrator`] | `SmODEIntegrator` |
//! | [`OdeIntegrator::integrate`] | `RungeKuttaODEIntegrate` |
//! | [`step`] | `RungeKutta5thOrderStep`, `RungeKuttaCashKarpAdvance` |
//!
//! [`OdeIntegrator`] is the integration's controls, and [`OdeIntegrator::integrate`] a pure
//! call over the integrand, a closure returning the derivative or its own error `E`. Each
//! step takes the six samples of the Cash–Karp pair and is accepted when its error
//! estimate, relative to the component's error scale, is within the accuracy; otherwise
//! it shrinks and retries. [`StepEstimate::Quadrature`] (for an integrand of `x` alone) also
//! reads the fifth divided difference of the samples; [`OdeIntegrator::rounding`] floors
//! the error scale at the estimate's own rounding; [`OdeIntegrator::rate`] holds each step to
//! its share of the whole range.
//!
//! Failures are [`SolveError`]s: the controls refused ([`SolveError::InvalidInput`]); the
//! step control unable to meet the accuracy ([`SolveFailure::StepControl`], with why, on
//! [`OP_STEP_CONTROL`], or on [`OP`] for an overflow); an integrand that is not finite
//! ([`SolveFailure::IntegrandNotFinite`], on [`OP_INTEGRAND`]); the integrand's own error,
//! unchanged at a step's start and through [`SolveError::call`] inside a step.
//!
//! Deviations: closures replace evaluator subclasses; component arrays are zero-based;
//! out-parameters become return values. Like upstream, integration starts at zero
//! regardless of the initial values (except its early return, which gives zero here).
//! Scalar integration propagates failures from [`OdeIntegrator::integrate`] (WR-94), where
//! upstream returns zero with success; intervals shorter than `EFF_ZERO` still return zero
//! without evaluating. Algorithm selection is ignored upstream. Disabled Romberg/Simpson
//! and Bulirsch–Stoer paths are not exposed. No recursion; dimensions are bounded at 4096,
//! steps at 1,000,000, and rejected step retries at 1024 to bound malformed caller input.
//! Infinite error scales/accuracy retain their upstream behaviour. Not upstream:
//! [`OdeIntegrator::rate`] holds each step to its share of the whole range, not to the
//! integral so far, so the steps' errors together stay within the accuracy (the curve
//! length).

use crate::error::{EvalError, SolveError, SolveFailure, StepControlFailure};
use scx_num::certified::gamma;
use scx_num::fmath::powf;
use scx_num::math::{EFF_ZERO, EFF_ZERO_SQ};
use scx_num::tol::are_same;

/// The operation the integration's refusals and failed calls name.
pub const OP: &str = "mass::integrator";
/// The operation a failed step control names: the step underflows, or the steps or
/// retries run out, or the accuracy is unachievable (upstream's status is `SM_ERR` for
/// every failure; the name says which).
pub const OP_STEP_CONTROL: &str = "mass::integrator::step_control";
/// The operation an integrand that is not finite names.
pub const OP_INTEGRAND: &str = "mass::integrator::integrand";

/// A step's refusal of an accuracy below its error's rounding: its step control cannot
/// meet it.
const UNACHIEVABLE: &str = "unachievable accuracy";
/// The most components an integration takes.
pub const MAX_COMPONENTS: usize = 4096;
/// The largest step budget an integration takes.
pub const MAX_STEPS: u32 = 1_000_000;
/// The most times one step is retried, shrinking.
pub const MAX_RETRIES: u32 = 1024;

fn step_control_failed<E>(why: StepControlFailure) -> SolveError<E> {
    SolveError::Failed {
        operation: OP_STEP_CONTROL,
        failure: SolveFailure::StepControl(why),
    }
}

fn integrand_failed<E>() -> SolveError<E> {
    SolveError::Failed {
        operation: OP_INTEGRAND,
        failure: SolveFailure::IntegrandNotFinite,
    }
}

fn invalid<E>(reason: &'static str) -> SolveError<E> {
    SolveError::InvalidInput {
        operation: OP,
        reason,
    }
}

/// How a step's error is estimated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepEstimate {
    /// Upstream's: the embedded fourth-order solution's difference from the fifth.
    #[default]
    Embedded,
    /// The derivative reads only `x`, never `y`: the integration is a quadrature. Each
    /// step's error is then also estimated from the fifth divided difference of its six
    /// samples ([`quadrature_estimate`]), and the step is accepted on the larger.
    Quadrature,
}

/// The controls of an integration of `N` components (`SmODEIntegrator` and
/// `RungeKuttaODEIntegrate`'s arguments).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OdeIntegrator<const N: usize> {
    /// Relative error tolerance (`eps`).
    pub accuracy: f64,
    /// Magnitude of the first step; its sign follows the interval.
    pub initial_step: f64,
    /// Smallest accepted next step (`hmin`, upstream compares against its sign).
    pub minimum_step: f64,
    /// Optional lower bounds for each component's error scale.
    pub scale: Option<[f64; N]>,
    /// How a step's error is estimated.
    pub estimate: StepEstimate,
    /// Optional bounds, one per component, on the absolute rounding error of each
    /// derivative sample (the integrand's, from its data). With them a step whose
    /// estimate is above the accuracy but within its own rounding is accepted, its error
    /// owed to a final check against the integral: within the accuracy, or the
    /// integration is refused as unachievable ([`estimate_rounding`]). `None` (the
    /// rounding is not known): upstream's step control.
    pub rounding: Option<[f64; N]>,
    /// The integral's size per unit of `x` over the whole range it is part of, when
    /// known (`r >= 0`). Each step of `h` is then held to the accuracy relative to its
    /// share of the range, `|h| r`, plus its own increment `|y_(k+1) - y_k|` (error per
    /// unit step), instead of upstream's scale `|y| + |h y'|`, the integral so far, and
    /// the owed errors to the accuracy relative to `r |x2 - x1| + |y|`. Against the
    /// integral so far, each of `n` steps may err by the accuracy relative to nearly all
    /// of it, and together by up to `n` times the accuracy; against their shares, the
    /// steps' errors add up to at most the accuracy times `r |x2 - x1| + ∫ |y'|`, over
    /// the whole range `r` was measured on at most twice the integral when `r` is a lower
    /// bound on its mean and the integrand is non-negative, as a curve's speed is. A lower
    /// bound in [`OdeIntegrator::scale`] still floors each step's scale.
    pub rate: Option<f64>,
    /// The most steps (upstream's constructor argument; at most [`MAX_STEPS`]).
    pub max_steps: u32,
    /// The floor added to each component's error scale (upstream's constructor argument;
    /// finite).
    pub tiny: f64,
}

/// Accepted full-size and reduced steps (`nok`, `nbad`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StepCounts {
    /// Accepted without shrinking the requested step.
    pub good: u32,
    /// Accepted after shrinking it.
    pub reduced: u32,
}

/// An integration's result: the integral of each component from `x1` to `x2`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OdeSolution<const N: usize> {
    /// The integrated components, `y(x2)` from `y(x1) = 0`.
    pub values: [f64; N],
    /// The steps taken.
    pub counts: StepCounts,
}

/// One accepted adaptive step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OdeStep<const N: usize> {
    /// New component values.
    pub values: [f64; N],
    /// New independent variable.
    pub x: f64,
    /// Step actually taken.
    pub taken: f64,
    /// Proposed next step.
    pub next: f64,
}

impl<const N: usize> OdeIntegrator<N> {
    /// Upstream's default step budget.
    pub const DEFAULT_MAX_STEPS: u32 = 10_000;

    /// The controls `accuracy`, `initial_step` and `minimum_step` with upstream's
    /// defaults for the rest: [`StepEstimate::Embedded`], no scale floor, rounding bounds
    /// or rate, [`OdeIntegrator::DEFAULT_MAX_STEPS`] steps and `tiny` = `EFF_ZERO_SQ`.
    #[must_use]
    pub const fn new(accuracy: f64, initial_step: f64, minimum_step: f64) -> Self {
        Self {
            accuracy,
            initial_step,
            minimum_step,
            scale: None,
            estimate: StepEstimate::Embedded,
            rounding: None,
            rate: None,
            max_steps: Self::DEFAULT_MAX_STEPS,
            tiny: EFF_ZERO_SQ,
        }
    }

    /// Integrates `y' = f(x, y)` from `y(x1) = 0` to `x2`.
    ///
    /// Returns zeros without evaluating `f` when `x1` and `x2` are the same, or the initial
    /// step is `EFF_ZERO` (upstream's early return; it reads no other control). Refuses a
    /// step budget above [`MAX_STEPS`], a `tiny` that is not finite, more than
    /// [`MAX_COMPONENTS`] components, controls that are not finite (a NaN accuracy, a rate
    /// that is not a finite non-negative number), a NaN scale floor and rounding bounds that
    /// are not finite and non-negative.
    pub fn integrate<E, F>(
        &self,
        x1: f64,
        x2: f64,
        mut f: F,
    ) -> Result<OdeSolution<N>, SolveError<E>>
    where
        E: EvalError,
        F: FnMut(f64, &[f64; N]) -> Result<[f64; N], E>,
    {
        if self.max_steps > MAX_STEPS || !self.tiny.is_finite() {
            return Err(invalid("invalid step budget or tiny value"));
        }
        if N > MAX_COMPONENTS {
            return Err(invalid("too many components"));
        }
        let mut counts = StepCounts::default();
        if are_same(x1, x2) || are_same(self.initial_step, EFF_ZERO) {
            return Ok(OdeSolution {
                values: [0.0; N],
                counts,
            });
        }
        if ![x1, x2, self.initial_step, self.minimum_step]
            .iter()
            .all(|v| v.is_finite())
            || self.accuracy.is_nan()
            || self.rate.is_some_and(|r| !(r.is_finite() && r >= 0.0))
        {
            return Err(invalid("non-finite integration control"));
        }
        if let Some(scale) = &self.scale {
            validate_scale(scale)?;
        }
        if let Some(rounding) = &self.rounding {
            validate_rounding(rounding)?;
        }
        let control = StepControl {
            estimate: self.estimate,
            rounding: self.rounding.as_ref(),
            rate: self.rate,
        };
        let mut y = [0.0; N];
        let mut x = x1;
        let mut h = if x2 - x1 >= 0.0 {
            self.initial_step.abs()
        } else {
            -self.initial_step.abs()
        };
        // per component, the errors of the steps accepted through the rounding floor
        let mut debt = [0.0; N];
        for _ in 0..self.max_steps {
            let dydx = f(x, &y).map_err(SolveError::Eval)?;
            finite(&dydx)?;
            let mut scale = [0.0; N];
            for ((s, yi), di) in scale.iter_mut().zip(&y).zip(&dydx) {
                // (with `rate`, the step's share of the range and its own increment are
                // added to this floor in `step_in`, for the step it takes)
                *s = if self.rate.is_some() {
                    self.tiny
                } else {
                    yi.abs() + (di * h).abs() + self.tiny
                };
            }
            if let Some(lower) = &self.scale {
                for (s, l) in scale.iter_mut().zip(lower) {
                    *s = s.max(*l);
                }
            }
            // a scale that is not a number comes from an overflow (`0 * inf` for a step
            // grown past the largest float, or the values' own): the integrator's failure,
            // where the step would refuse it as input (upstream: the step's `SM_ERR`, of
            // the integrator)
            if scale.iter().any(|s| s.is_nan()) {
                return Err(SolveError::Failed {
                    operation: OP,
                    failure: SolveFailure::StepControl(StepControlFailure::Overflow),
                });
            }
            // the step's end: `x2` itself for the final step, so the last node is the
            // interval's end exactly (`x + (x2 - x)` can round past it)
            let mut end = x + h;
            if (end - x2) * (end - x1) > 0.0 {
                h = x2 - x;
                end = x2;
            }
            let step = step_in(&y, &dydx, x, h, end, self.accuracy, scale, &mut f, control)
                .map_err(|e| match e {
                    // the step control cannot meet the accuracy (upstream: SM_ERR, as below)
                    SolveError::InvalidInput {
                        reason: UNACHIEVABLE,
                        ..
                    } => step_control_failed(StepControlFailure::Unachievable),
                    SolveError::Eval(e) => SolveError::call(OP)(e),
                    e => e,
                })?;
            if step.taken == h {
                counts.good += 1;
            } else {
                counts.reduced += 1;
            }
            x = step.x;
            for (total, owed) in debt.iter_mut().zip(&step.owed) {
                *total += owed;
            }
            y = step.values;
            if (x - x2) * (x2 - x1) >= 0.0 {
                // The steps accepted through the rounding floor: their errors, together,
                // within the accuracy relative to the integral (with `rate`, plus the
                // range's share; or its scale's lower bound), or the accuracy is below what
                // the integrand resolves.
                for (i, (owed, yi)) in debt.iter().zip(&y).enumerate() {
                    let mut size = yi.abs();
                    if let Some(rate) = self.rate {
                        size += rate * (x2 - x1).abs();
                    }
                    if let Some(lower) = self.scale.as_ref().and_then(|l| l.get(i)) {
                        size = size.max(*lower);
                    }
                    if *owed > self.accuracy * size {
                        return Err(step_control_failed(StepControlFailure::ErrorOwed));
                    }
                }
                return Ok(OdeSolution { values: y, counts });
            }
            if step.next.abs() <= self.minimum_step {
                return Err(step_control_failed(StepControlFailure::BelowMinimumStep));
            }
            h = step.next;
        }
        Err(step_control_failed(StepControlFailure::StepsExhausted))
    }
}

/// `RungeKutta5thOrderStep`: one adaptive step of `htry` from `x`, where the values are `y`
/// and their derivatives `dydx`, retried smaller until its embedded error estimate,
/// relative to `scale` (each component floored at `EFF_ZERO`), is within `accuracy`.
///
/// Refuses values, derivatives or a scale that are not finite (an integrand failure for
/// the first two, as upstream), a NaN accuracy or controls that are not finite, and an
/// accuracy below the rounding of the estimate (`InvalidInput`, "unachievable accuracy").
/// The integrand's errors are returned unchanged.
pub fn step<const N: usize, E, F>(
    y: &[f64; N],
    dydx: &[f64; N],
    x: f64,
    htry: f64,
    accuracy: f64,
    scale: [f64; N],
    mut f: F,
) -> Result<OdeStep<N>, SolveError<E>>
where
    F: FnMut(f64, &[f64; N]) -> Result<[f64; N], E>,
{
    let step = step_in(
        y,
        dydx,
        x,
        htry,
        x + htry,
        accuracy,
        scale,
        &mut f,
        StepControl {
            estimate: StepEstimate::Embedded,
            rounding: None,
            rate: None,
        },
    )?;
    Ok(OdeStep {
        values: step.values,
        x: step.x,
        taken: step.taken,
        next: step.next,
    })
}

/// How [`step_in`] controls a step: [`OdeIntegrator::estimate`],
/// [`OdeIntegrator::rounding`] and [`OdeIntegrator::rate`].
#[derive(Clone, Copy)]
struct StepControl<'a, const N: usize> {
    estimate: StepEstimate,
    rounding: Option<&'a [f64; N]>,
    rate: Option<f64>,
}

/// The accepted step of [`step_in`].
struct Accepted<const N: usize> {
    /// The step's end.
    x: f64,
    /// The step taken.
    taken: f64,
    /// The next step proposed.
    next: f64,
    /// The values at the step's end.
    values: [f64; N],
    /// Per component, the error the step, accepted through the rounding floor, may carry
    /// beyond its scale (`2 E`); zero otherwise.
    owed: [f64; N],
}

/// [`step`] with its controls: `end` is the end of a step of `htry` (the step's last node,
/// exactly; a smaller retry ends at `x + h`). `control` as [`OdeIntegrator::estimate`],
/// [`OdeIntegrator::rounding`] and [`OdeIntegrator::rate`]: a step whose estimate is above
/// the accuracy but within its own rounding is accepted, its error owed
/// ([`Accepted::owed`], [`estimate_rounding`]).
fn step_in<const N: usize, E, F>(
    y: &[f64; N],
    dydx: &[f64; N],
    x: f64,
    htry: f64,
    end: f64,
    accuracy: f64,
    mut scale: [f64; N],
    f: &mut F,
    control: StepControl<'_, N>,
) -> Result<Accepted<N>, SolveError<E>>
where
    F: FnMut(f64, &[f64; N]) -> Result<[f64; N], E>,
{
    validate(y)?;
    validate(dydx)?;
    validate_scale(&scale)?;
    if ![x, htry, end].iter().all(|v| v.is_finite()) || accuracy.is_nan() {
        return Err(invalid("invalid step controls or dimensions"));
    }
    let StepControl {
        estimate,
        rounding,
        rate,
    } = control;
    let mut h = htry;
    for _ in 0..MAX_RETRIES {
        let step_end = if h == htry { end } else { x + h };
        let ck = cash_karp(y, dydx, x, h, step_end, f)?;
        // Per component: the larger estimate relative to its error scale, and relative
        // to the scale floored at the estimate's rounding `E` (`estimate_rounding`);
        // the step is controlled on the floored ratio. A component accepted only
        // through the floor owes its step's error, at most `2 E` (the estimate is
        // within `E` of the exact samples' and at most `E` itself), to the
        // integration's final check ([`OdeIntegrator::integrate`]).
        let mut errmax: f64 = 0.0;
        let mut embedded_max: f64 = 0.0;
        let mut owed = [0.0; N];
        let parts = scale
            .iter_mut()
            .zip(&mut owed)
            .zip(ck.values.iter().zip(y))
            .zip(ck.errors.iter().zip(&ck.samples));
        for (i, (((s, owed), (value, yi)), (&embedded, &samples))) in parts.enumerate() {
            if s.abs() < EFF_ZERO {
                *s = EFF_ZERO;
            }
            // this try's share of the range and its own increment
            let s = if let Some(rate) = rate {
                s.max(h.abs() * rate + (value - yi).abs())
            } else {
                *s
            };
            let delta = sample_rounding(accuracy, rounding, i);
            let rounding_of =
                |weights| delta.map_or(0.0, |delta| estimate_rounding(weights, h, delta, samples));
            let embedded_rounding = rounding_of(EMBEDDED_ERROR_WEIGHTS);
            let mut raw = (embedded / s).abs();
            let mut ratio = (embedded / floored(s, embedded_rounding / accuracy)).abs();
            embedded_max = embedded_max.max(ratio);
            let mut bound = embedded_rounding;
            if estimate == StepEstimate::Quadrature {
                let estimate = quadrature_estimate(h, samples)?;
                let rounding = rounding_of(QUADRATURE_ERROR_WEIGHTS);
                raw = raw.max((estimate / s).abs());
                ratio = ratio.max((estimate / floored(s, rounding / accuracy)).abs());
                bound = bound.max(rounding);
            }
            errmax = errmax.max(ratio);
            *owed = if raw > accuracy { 2.0 * bound } else { 0.0 };
        }
        if accuracy <= embedded_max * EFF_ZERO {
            return Err(invalid(UNACHIEVABLE));
        }
        // (the guard above reads the embedded estimate alone: a coarse step's larger
        // fifth-order estimate asks for a smaller step, not for a lower accuracy)
        errmax /= accuracy;
        if errmax <= 1.0 {
            let next = if errmax > 1.89e-4 {
                0.9 * h * powf(errmax, -0.2)
            } else {
                5.0 * h
            };
            return Ok(Accepted {
                x: step_end,
                taken: h,
                next,
                values: ck.values,
                owed,
            });
        }
        let htemp = 0.9 * h * powf(errmax, -0.25);
        h = if h >= 0.0 {
            htemp.max(0.1 * h)
        } else {
            htemp.min(0.1 * h)
        };
        if !h.is_finite() || are_same(x + h, x) {
            return Err(step_control_failed(StepControlFailure::StepUnderflow));
        }
    }
    Err(step_control_failed(StepControlFailure::RetriesExhausted))
}

fn finite<E>(values: &[f64]) -> Result<(), SolveError<E>> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(integrand_failed())
    }
}

fn validate_scale<E>(scale: &[f64]) -> Result<(), SolveError<E>> {
    if scale.len() > MAX_COMPONENTS || scale.iter().any(|v| v.is_nan()) {
        return Err(invalid("invalid error scale"));
    }
    Ok(())
}

fn validate_rounding<E>(rounding: &[f64]) -> Result<(), SolveError<E>> {
    if rounding.iter().any(|v| !(v.is_finite() && *v >= 0.0)) {
        return Err(invalid("invalid rounding bounds"));
    }
    Ok(())
}

fn validate<E>(values: &[f64]) -> Result<(), SolveError<E>> {
    if values.len() > MAX_COMPONENTS {
        return Err(invalid("too many components"));
    }
    finite(values)
}

/// One Cash–Karp step (`RungeKuttaCashKarpAdvance`).
struct CashKarp<const N: usize> {
    /// The fifth-order values (`ytemp`).
    values: [f64; N],
    /// The embedded error estimates (`yerr`).
    errors: [f64; N],
    /// Each component's six samples, in node order: the derivative at the step's start,
    /// then `ak2` to `ak6`.
    samples: [[f64; 6]; N],
}

/// One Cash–Karp step of `h` from `x`. The step's last node (`c = 1`) is `end`, exactly,
/// and every node lies between `x` and `end`: `x + c h` rounded can land an ulp past the
/// end (`-0.1 + 0.4` is `0.30000000000000004`), where an integrand whose end is a knot
/// would read the span beyond it.
fn cash_karp<const N: usize, E, F>(
    y: &[f64; N],
    dydx: &[f64; N],
    x: f64,
    h: f64,
    end: f64,
    f: &mut F,
) -> Result<CashKarp<N>, SolveError<E>>
where
    F: FnMut(f64, &[f64; N]) -> Result<[f64; N], E>,
{
    // (`max` then `min`: no panic; `x` and `end` are ordered as the step's direction)
    let node = |c: f64| {
        let t = x + c * h;
        if h >= 0.0 {
            t.max(x).min(end)
        } else {
            t.min(x).max(end)
        }
    };
    let mut samples = [[0.0; 6]; N];
    for (s, d) in samples.iter_mut().zip(dydx) {
        s[0] = *d;
    }
    // The values for each stage, from `y` and the samples so far.
    let stage = |samples: &[[f64; 6]; N], combine: &dyn Fn(f64, &[f64; 6]) -> f64| {
        let mut temp = [0.0; N];
        for ((t, yi), s) in temp.iter_mut().zip(y).zip(samples) {
            *t = combine(*yi, s);
        }
        temp
    };
    let mut sample = |samples: &mut [[f64; 6]; N], j: usize, at: f64, temp: &[f64; N]| {
        let ak = f(at, temp).map_err(SolveError::Eval)?;
        finite(&ak)?;
        for (s, a) in samples.iter_mut().zip(ak) {
            if let Some(s) = s.get_mut(j) {
                *s = a;
            }
        }
        Ok::<(), SolveError<E>>(())
    };
    let temp = stage(&samples, &|y, s| y + 0.2 * h * s[0]);
    sample(&mut samples, 1, node(0.2), &temp)?;
    let temp = stage(&samples, &|y, s| {
        y + h * ((3.0 / 40.0) * s[0] + (9.0 / 40.0) * s[1])
    });
    sample(&mut samples, 2, node(0.3), &temp)?;
    let temp = stage(&samples, &|y, s| {
        y + h * (0.3 * s[0] - 0.9 * s[1] + 1.2 * s[2])
    });
    sample(&mut samples, 3, node(0.6), &temp)?;
    let temp = stage(&samples, &|y, s| {
        y + h * ((-11.0 / 54.0) * s[0] + 2.5 * s[1] + (-70.0 / 27.0) * s[2] + (35.0 / 27.0) * s[3])
    });
    sample(&mut samples, 4, end, &temp)?;
    let temp = stage(&samples, &|y, s| {
        y + h
            * ((1631.0 / 55296.0) * s[0]
                + (175.0 / 512.0) * s[1]
                + (575.0 / 13824.0) * s[2]
                + (44275.0 / 110592.0) * s[3]
                + (253.0 / 4096.0) * s[4])
    });
    sample(&mut samples, 5, node(0.875), &temp)?;
    let values = stage(&samples, &|y, s| {
        y + h
            * ((37.0 / 378.0) * s[0]
                + (250.0 / 621.0) * s[2]
                + (125.0 / 594.0) * s[3]
                + (512.0 / 1771.0) * s[5])
    });
    finite(&values)?;
    let mut errors = [0.0; N];
    for (e, s) in errors.iter_mut().zip(&samples) {
        *e = weighted(EMBEDDED_ERROR_WEIGHTS, h, *s);
    }
    finite(&errors)?;
    Ok(CashKarp {
        values,
        errors,
        samples,
    })
}

/// `h sum_j w_j g_j`: a step's estimate of weights `weights` on its samples `samples`
/// ([`CashKarp::samples`]), summed in node order. The embedded estimate, the quadrature
/// estimate and their rounding bound ([`estimate_rounding`]) are each one.
fn weighted(weights: [f64; 6], h: f64, samples: [f64; 6]) -> f64 {
    let sum: f64 = weights.iter().zip(samples).map(|(w, g)| w * g).sum();
    h * sum
}

/// The weights of the embedded error estimate (fifth- minus fourth-order Cash–Karp
/// weights) on the samples in order: the derivative at the step's start, then `ak2` to
/// `ak6`.
pub const EMBEDDED_ERROR_WEIGHTS: [f64; 6] = [
    (37.0 / 378.0) - (2825.0 / 27648.0),
    0.0,
    (250.0 / 621.0) - (18575.0 / 48384.0),
    (125.0 / 594.0) - (13525.0 / 55296.0),
    -277.0 / 14336.0,
    (512.0 / 1771.0) - 0.25,
];
/// The weights of the quadrature estimate on the same samples: `(-1/960)` times the
/// fifth divided difference ([`quadrature_estimate`]).
pub const QUADRATURE_ERROR_WEIGHTS: [f64; 6] = [
    FIFTH_ORDER_ERROR_ON_S5 * DIVIDED_DIFFERENCE_5[0],
    FIFTH_ORDER_ERROR_ON_S5 * DIVIDED_DIFFERENCE_5[1],
    FIFTH_ORDER_ERROR_ON_S5 * DIVIDED_DIFFERENCE_5[2],
    FIFTH_ORDER_ERROR_ON_S5 * DIVIDED_DIFFERENCE_5[3],
    FIFTH_ORDER_ERROR_ON_S5 * DIVIDED_DIFFERENCE_5[4],
    FIFTH_ORDER_ERROR_ON_S5 * DIVIDED_DIFFERENCE_5[5],
];

/// Not upstream: the bound on the rounding error of a step's error estimate
/// `h sum_j w_j g_j` (weights `weights`, [`EMBEDDED_ERROR_WEIGHTS`] or
/// [`QUADRATURE_ERROR_WEIGHTS`]) on the samples `samples` ([`CashKarp::samples`]), from the
/// bound `delta` on the absolute rounding error of each sample ([`OdeIntegrator::rounding`]):
///
/// `E = (1 + gamma_16) |h| sum_j |w_j| (delta + gamma_11 |g_j|)`, `g_j` the computed samples.
///
/// Each computed sample is `g_j` with `|g_j - g*_j| <= delta + gamma_3 |g*_j|` against
/// the exact `g*_j` (the caller's absolute part, plus up to three roundings relative to
/// the sample, as a Euclidean norm takes), so `gamma_3 |g*_j| <= gamma_4 (|g_j| + delta)`.
/// The estimate ([`weighted`]: six products, five additions and the product with `h`)
/// rounds by at most `gamma_7 |h| sum_j |w_j g_j|` (Higham, eq. 3.5), and
/// `gamma_4 + gamma_7 <= gamma_11`. That bound, `|h| sum_j |w_j| ((1 + gamma_4) delta +
/// gamma_11 |g_j|)`, is itself evaluated in at most ten roundings of non-negative terms:
/// the factor `1 + gamma_16` covers them and the `gamma_4` on `delta`.
/// The computed estimate is then within `E` of the estimate of the exact samples:
/// an estimate at or below `E` cannot be told from zero.
///
/// A step's error scale `s` is floored at `E / accuracy`: a step whose estimate is
/// above `accuracy * s` but within `E` cannot be told from one of zero estimate, and no
/// smaller step resolves it where `s` vanishes with the step (the integral's start at a
/// zero of the integrand: `s = |h g(x)|`). Such a step is accepted, and its error, at
/// most `2 E` (the exact samples' estimate is within `E` of the computed one, at most
/// `E`), is owed: the integration returns its value only when the owed errors of its
/// steps sum within `accuracy` relative to the integral, and is refused
/// ([`StepControlFailure::ErrorOwed`], [`OP_STEP_CONTROL`]) otherwise. So the floor never
/// loosens the accuracy asked: where the integrand's rounding is below it, as for a speed
/// evaluated in difference form, it only accepts steps at a zero of the integrand; where
/// its rounding is above it, the integration is refused.
fn estimate_rounding(weights: [f64; 6], h: f64, delta: f64, samples: [f64; 6]) -> f64 {
    let relative = gamma(11);
    (1.0 + gamma(16))
        * weighted(
            weights.map(f64::abs),
            h.abs(),
            samples.map(|g| delta + relative * g.abs()),
        )
}

/// The error scale `s` floored at `floor` when there is one (a zero floor leaves `s`
/// as upstream's, also a negative one a caller's lower bound gave).
fn floored(s: f64, floor: f64) -> f64 {
    if floor > 0.0 {
        s.max(floor)
    } else {
        s
    }
}

/// The bound `delta` on the rounding of component `i`'s samples
/// ([`OdeIntegrator::rounding`]); `None` without sample rounding bounds (the integrand's
/// rounding is then not known, and the step control is upstream's: no floor) and at a
/// non-positive accuracy, which the step refuses as unachievable.
fn sample_rounding<const N: usize>(
    accuracy: f64,
    rounding: Option<&[f64; N]>,
    i: usize,
) -> Option<f64> {
    match rounding {
        Some(rounding) if accuracy > 0.0 => rounding.get(i).copied(),
        _ => None,
    }
}

/// The weights of the fifth divided difference on the Cash–Karp nodes `0, 1/5, 3/10, 3/5,
/// 1, 7/8` (in that order: the derivative at the step's start, then `ak2` to `ak6`):
/// `1 / prod_{j != i} (c_i - c_j)`.
pub const DIVIDED_DIFFERENCE_5: [f64; 6] = [
    -2000.0 / 63.0,
    6250.0 / 27.0,
    -400_000.0 / 1449.0,
    12500.0 / 99.0,
    250.0 / 7.0,
    -4_096_000.0 / 47817.0,
];
/// The error of the fifth-order Cash–Karp weights on `s^5` over `[0, 1]`:
/// `37/378 (3/10)^5 + 250/621 (3/5)^5 + 125/594 + 512/1771 (7/8)^5 - 1/6`.
pub const FIFTH_ORDER_ERROR_ON_S5: f64 = -1.0 / 960.0;

/// Not upstream: the error of a quadrature step's fifth-order value from its samples
/// `samples` ([`CashKarp::samples`]).
///
/// For a derivative of `x` alone, a Cash–Karp step is a quadrature rule on six samples
/// of `g(s) = f(x + s h)`: the fifth-order value integrates polynomials of degree 4
/// exactly, and its error is `h (-1/960) g^(5)(xi) / 120 = h (-1/960) g[c_0..c_5]` to
/// leading order, the fifth divided difference of the six samples. The embedded
/// estimate is the difference of the fourth- and fifth-order values, a rule that is
/// zero on cubics and so measures `g^(4)` only: where `g^(4)` changes sign within the
/// step it can vanish while the step's error does not. On a step of half a quarter
/// circle's trim it read 3.1e-5 where the step was off by 3.5e-3. The divided
/// difference measures `g^(5)`; the step is accepted on the larger of the two, which
/// misses only where both vanish together. (A pair of null rules of consecutive
/// degrees: Berntsen and Espelid, "Error estimation in automatic quadrature routines",
/// ACM TOMS 17 (1991).)
fn quadrature_estimate<E>(h: f64, samples: [f64; 6]) -> Result<f64, SolveError<E>> {
    let error = weighted(QUADRATURE_ERROR_WEIGHTS, h, samples);
    if error.is_finite() {
        Ok(error)
    } else {
        Err(integrand_failed())
    }
}

/// The controls of a scalar integration `∫_a^b g(t) dt` (`SmIntegrator::IntegrateIt`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quadrature {
    /// Relative error tolerance.
    pub accuracy: f64,
    /// The bound on the absolute rounding error of each sample of the integrand, when
    /// known ([`OdeIntegrator::rounding`]): the value is within the accuracy by the step
    /// control's estimates, or, where the accuracy asks for less than the integrand
    /// resolves, the integration is refused ([`StepControlFailure::ErrorOwed`]).
    pub rounding: Option<f64>,
    /// The integral's size per unit of `t` over the whole range it is part of, when known
    /// ([`OdeIntegrator::rate`]): each step is held to its share of that range plus its
    /// own increment, so the steps of every piece of the range err together by at most
    /// the accuracy times `rate` × the range plus the integral. `None`: upstream's error
    /// scale.
    pub rate: Option<f64>,
}

impl Quadrature {
    /// The controls `accuracy`, with no rounding bound or rate.
    #[must_use]
    pub const fn new(accuracy: f64) -> Self {
        Self {
            accuracy,
            rounding: None,
            rate: None,
        }
    }

    /// `∫_a^b g(t) dt`. Intervals shorter than `EFF_ZERO` return zero without evaluating
    /// `g`.
    ///
    /// Any error of [`OdeIntegrator::integrate`] (an integrand error, a non-finite sample
    /// or control, unachievable accuracy, step underflow below the minimum step `(b - a)
    /// 1e-8`, or exceeding the step budget) is returned (WR-94): upstream returns `0.0`
    /// with success instead.
    ///
    /// Not upstream: each step is accepted on the larger of the embedded estimate and the
    /// fifth divided difference of its samples ([`StepEstimate::Quadrature`]); upstream
    /// reads the embedded estimate alone, which vanishes where `g''''` changes sign.
    pub fn integrate<E, F>(&self, a: f64, b: f64, mut g: F) -> Result<f64, SolveError<E>>
    where
        E: EvalError,
        F: FnMut(f64) -> Result<f64, E>,
    {
        if (a - b).abs() < EFF_ZERO {
            return Ok(0.0);
        }
        let ode = OdeIntegrator {
            initial_step: (b - a) / 10.0,
            minimum_step: (b - a) / 100000000.0,
            // `g` reads `t` alone: the steps are accepted on the quadrature estimate
            estimate: StepEstimate::Quadrature,
            rounding: self.rounding.map(|r| [r]),
            rate: self.rate,
            ..OdeIntegrator::new(self.accuracy, 0.0, 0.0)
        };
        let [value] = ode.integrate(a, b, |t, _| g(t).map(|v| [v]))?.values;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::EvalError;
    use core::fmt;

    /// A test integrand's error.
    #[derive(Debug, Clone, PartialEq)]
    struct Stop;
    impl fmt::Display for Stop {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("stop")
        }
    }
    impl EvalError for Stop {
        fn ends_solve(&self) -> bool {
            false
        }
        fn passes_through(&self) -> bool {
            false
        }
        fn never_ignored(&self) -> bool {
            false
        }
    }

    #[allow(clippy::trivially_copy_pass_by_ref, clippy::unnecessary_wraps)]
    fn unit(_: f64, _: &[f64; 1]) -> Result<[f64; 1], Stop> {
        Ok([1.0])
    }

    /// Upstream's early return gives zeros without evaluating, whatever the other
    /// controls; infinite accuracy and scales integrate as upstream.
    #[test]
    fn early_return_and_infinite_controls() {
        for (accuracy, scale) in [(1e-6, f64::INFINITY), (f64::INFINITY, 1.0)] {
            let ode = OdeIntegrator {
                scale: Some([scale]),
                ..OdeIntegrator::new(accuracy, 0.1, 0.0)
            };
            let r = ode.integrate(0.0, 1.0, unit).map(|s| s.values);
            assert!(matches!(r, Ok([v]) if (v - 1.0).abs() < 1e-14), "{r:?}");
        }
        let ode = OdeIntegrator {
            scale: Some([f64::NAN]),
            ..OdeIntegrator::<1>::new(f64::NAN, f64::NAN, f64::NAN)
        };
        let r = ode.integrate(2.0, 2.0, |_, _| Err(Stop));
        assert_eq!(
            r,
            Ok(OdeSolution {
                values: [0.0],
                counts: StepCounts::default()
            })
        );
    }

    /// The step budget, `tiny`, a rate, rounding bounds and a scale floor are refused
    /// before the integrand is evaluated.
    #[test]
    fn refused_controls_do_not_evaluate() {
        let base = OdeIntegrator::<1>::new(1e-6, 0.1, 0.0);
        for ode in [
            OdeIntegrator {
                max_steps: MAX_STEPS + 1,
                ..base
            },
            OdeIntegrator {
                tiny: f64::NAN,
                ..base
            },
            OdeIntegrator {
                rate: Some(-1.0),
                ..base
            },
            OdeIntegrator {
                rounding: Some([f64::INFINITY]),
                ..base
            },
            OdeIntegrator {
                scale: Some([f64::NAN]),
                ..base
            },
        ] {
            let r = ode.integrate(0.0, 1.0, |_, _| Err(Stop));
            assert!(
                matches!(r, Err(SolveError::InvalidInput { operation: OP, .. })),
                "{r:?}"
            );
        }
    }

    /// The integrand's error at the step's start is returned unchanged; inside a step,
    /// as a failed call; a non-finite sample is the integrand's failure.
    #[test]
    fn a_failure_names_its_cause() {
        let ode = OdeIntegrator::<1>::new(1e-6, 0.1, 0.0);
        assert_eq!(
            ode.integrate(0.0, 1.0, |_, _| Err(Stop)),
            Err(SolveError::Eval(Stop))
        );
        let r = ode.integrate(0.0, 1.0, |x, _| if x > 0.0 { Err(Stop) } else { Ok([1.0]) });
        assert_eq!(
            r,
            Err(SolveError::CallFailed {
                operation: OP,
                cause: Stop
            })
        );
        let r = ode.integrate(0.0, 1.0, |x, _| {
            Ok::<_, Stop>([if x > 0.5 { f64::NAN } else { 1.0 }])
        });
        assert_eq!(r, Err(integrand_failed()));
        let tight = OdeIntegrator::<1>::new(0.0, 0.1, 0.0).integrate(0.0, 1.0, unit);
        assert_eq!(
            tight,
            Err(step_control_failed(StepControlFailure::Unachievable))
        );
    }

    /// The fifth divided difference's weights are zero on polynomials of degree 4 and one
    /// on `s^5` (so the quadrature estimate's are `-1/960` there), and the fifth-order
    /// weights miss `s^5` by `-1/960`.
    #[test]
    fn quadrature_error_constants() {
        let nodes = [0.0_f64, 0.2, 0.3, 0.6, 1.0, 0.875];
        let pow = |c: f64, n: u32| (0..n).fold(1.0, |p, _| p * c);
        for power in 0..=5 {
            let d: f64 = DIVIDED_DIFFERENCE_5
                .iter()
                .zip(nodes)
                .map(|(w, c)| w * pow(c, power))
                .sum();
            let want = if power == 5 { 1.0 } else { 0.0 };
            assert!((d - want).abs() < 1e-9, "s^{power}: {d}");
            let q = weighted(QUADRATURE_ERROR_WEIGHTS, 1.0, nodes.map(|c| pow(c, power)));
            assert!(
                (q - want * FIFTH_ORDER_ERROR_ON_S5).abs() < 1e-12,
                "s^{power}: {q}"
            );
        }
    }
}
