// Written for solidcorex. Apache-2.0.
//! The transcendental functions, in plain `f64` arithmetic, so a result has the same bits
//! on every platform (`docs/01-architecture.md` §8.3, `docs/07-conventions.md` §7 rule 9).
//! The platform's libm (behind `f64::sin`, `f64::powi` and the rest) differs between
//! systems and versions; nothing in solidcorex calls it, and `clippy.toml` bans it.
//!
//! Every function reduces its argument exactly or in double-double, evaluates a short
//! polynomial against a table in double-double (through [`crate::eft`], whose fused
//! multiply-add is correctly rounded everywhere), and rounds once at the end. Only `+`,
//! `-`, `*`, `/`, `sqrt` and the error-free transformations are used, all of them
//! correctly rounded by IEEE 754, so the bits do not depend on the platform.
//!
//! Depends on nothing in the crate but [`crate::eft`], so the two can move to a crate of
//! their own.
//!
//! # Accuracy
//!
//! Every function is correctly rounded (round to nearest, ties to even) on every input
//! tried: 5,000 per function (4,997 for `sin` and 4,999 for `tan`, whose other inputs
//! were infinite), log-uniform over the whole domain and dense near the hard places
//! (arguments near multiples of `pi/2`, near 1 for the logarithms, near `±1` for `asin`,
//! subnormals), against Python's `decimal` at 90 digits. MEASURED 2026-11-27, off the
//! repository; the largest error found, in ulps of the true value:
//!
//! | function | max ulp | | function | max ulp | | function | max ulp |
//! |---|---|---|---|---|---|---|---|
//! | `exp` | 0.4998 | | `exp2` | 0.5000 | | `ln` | 0.5000 |
//! | `log2` | 0.4996 | | `log10` | 0.4999 | | `sin` | 0.4999 |
//! | `cos` | 0.5000 | | `tan` | 0.5000 | | `asin` | 0.4999 |
//! | `acos` | 0.4997 | | `atan` | 0.4998 | | `atan2` | 0.4998 |
//! | `sinh` | 0.5000 | | `cosh` | 0.4999 | | `tanh` | 0.5000 |
//! | `cbrt` | 0.4997 | | `hypot` | 0.4999 | | `powf` | 0.4999 |
//! | `powi` | 0.4999 | |  |  | |  |  |
//!
//! `tests/fmath.rs` holds the test that keeps this: random inputs and a table of hard
//! cases against references computed in double-double in the test itself, each within
//! 0.501 ulp, and the C99 special values bit for bit.
//!
//! # Speed
//!
//! MEASURED 2026-11-27, off the repository: nanoseconds per call (best of 7 passes over
//! 200,000 inputs drawn as above, one core pinned, `--release`, the default x86-64 target),
//! against `f64`'s functions (the platform's libm). Two back-to-back runs agreed within
//! 3%; runs hours apart moved every column together by up to 35% with the machine's load,
//! so read the ratio, not the nanoseconds.
//!
//! | function | fmath ns | std ns | ratio | | function | fmath ns | std ns | ratio |
//! |---|---|---|---|---|---|---|---|---|
//! | `exp` | 12.8 | 5.3 | 2.41 | | `exp2` | 11.7 | 4.3 | 2.73 |
//! | `ln` | 17.8 | 5.6 | 3.20 | | `log2` | 20.0 | 5.4 | 3.72 |
//! | `log10` | 20.4 | 7.8 | 2.60 | | `sin` | 34.8 | 20.0 | 1.74 |
//! | `cos` | 34.8 | 20.2 | 1.72 | | `tan` | 37.9 | 21.9 | 1.73 |
//! | `sin_cos` | 35.5 | 24.2 | 1.47 | | `asin` | 30.2 | 10.1 | 2.99 |
//! | `acos` | 34.1 | 10.1 | 3.38 | | `atan` | 17.9 | 11.1 | 1.61 |
//! | `atan2` | 28.6 | 18.5 | 1.55 | | `sinh` | 17.9 | 13.7 | 1.31 |
//! | `cosh` | 17.9 | 7.8 | 2.29 | | `tanh` | 20.5 | 13.3 | 1.53 |
//! | `cbrt` | 24.2 | 12.8 | 1.90 | | `hypot` | 13.0 | 12.1 | 1.07 |
//! | `powf` | 40.1 | 12.5 | 3.22 | | `powi` | 57.3 | 26.8 | 2.14 |
//!
//! The default x86-64 target has no fused multiply-add instruction, so every
//! [`crate::eft::two_prod`] is a call to the C library's `fma`. Built with
//! `-C target-cpu=native` (which the workspace does not do) on a machine with one, fmath
//! is 30 to 40% faster (`sin` 21.8 ns, `ln` 11.3 ns, `powi` 35.2 ns) with the same bits.
//! Correct rounding is the reason for the cost; the speed that matters is the corpus's,
//! not one function's.
//!
//! Special values follow C99 Annex F (for example `atan2(±0, -0) = ±pi`,
//! `powf(-1, ±inf) = 1`, `hypot(inf, NaN) = inf`). Every NaN result is the one quiet NaN
//! [`f64::NAN`], whatever NaN came in.

mod dd;
mod exp;
mod hyper;
mod inverse;
mod log;
mod pow;
mod root;
// Generated: one entry per line would bury the tables, so rustfmt leaves them alone.
#[rustfmt::skip]
mod tables;
mod trig;

/// `e^x`.
#[inline]
#[must_use]
pub fn exp(x: f64) -> f64 {
    exp::exp(x)
}

/// `2^x`.
#[inline]
#[must_use]
pub fn exp2(x: f64) -> f64 {
    exp::exp2(x)
}

/// The natural logarithm.
#[inline]
#[must_use]
pub fn ln(x: f64) -> f64 {
    log::ln(x)
}

/// The base-2 logarithm.
#[inline]
#[must_use]
pub fn log2(x: f64) -> f64 {
    log::log2(x)
}

/// The base-10 logarithm.
#[inline]
#[must_use]
pub fn log10(x: f64) -> f64 {
    log::log10(x)
}

/// The sine, of any finite argument (reduced exactly).
#[inline]
#[must_use]
pub fn sin(x: f64) -> f64 {
    trig::sin(x)
}

/// The cosine, of any finite argument (reduced exactly).
#[inline]
#[must_use]
pub fn cos(x: f64) -> f64 {
    trig::cos(x)
}

/// `(sin x, cos x)`, sharing one reduction.
#[inline]
#[must_use]
pub fn sin_cos(x: f64) -> (f64, f64) {
    trig::sin_cos(x)
}

/// The tangent.
#[inline]
#[must_use]
pub fn tan(x: f64) -> f64 {
    trig::tan(x)
}

/// The arcsine, in `[-pi/2, pi/2]`; NaN outside `[-1, 1]`.
#[inline]
#[must_use]
pub fn asin(x: f64) -> f64 {
    inverse::asin(x)
}

/// The arccosine, in `[0, pi]`; NaN outside `[-1, 1]`.
#[inline]
#[must_use]
pub fn acos(x: f64) -> f64 {
    inverse::acos(x)
}

/// The arctangent, in `[-pi/2, pi/2]`.
#[inline]
#[must_use]
pub fn atan(x: f64) -> f64 {
    inverse::atan(x)
}

/// The angle of the point `(x, y)`, in `[-pi, pi]`.
#[inline]
#[must_use]
pub fn atan2(y: f64, x: f64) -> f64 {
    inverse::atan2(y, x)
}

/// The hyperbolic sine.
#[inline]
#[must_use]
pub fn sinh(x: f64) -> f64 {
    hyper::sinh(x)
}

/// The hyperbolic cosine.
#[inline]
#[must_use]
pub fn cosh(x: f64) -> f64 {
    hyper::cosh(x)
}

/// The hyperbolic tangent.
#[inline]
#[must_use]
pub fn tanh(x: f64) -> f64 {
    hyper::tanh(x)
}

/// `sqrt(x^2 + y^2)`, without overflow or underflow on the way.
#[inline]
#[must_use]
pub fn hypot(x: f64, y: f64) -> f64 {
    root::hypot(x, y)
}

/// The cube root (negative for negative `x`).
#[inline]
#[must_use]
pub fn cbrt(x: f64) -> f64 {
    root::cbrt(x)
}

/// `x^n` for an integer `n`. Rounded once, not by repeated multiplication: for `n` other
/// than `0`, `±1` and `2` this differs from `f64::powi` in the last bit now and then.
#[inline]
#[must_use]
pub fn powi(x: f64, n: i32) -> f64 {
    pow::powi(x, n)
}

/// `x^y`.
#[inline]
#[must_use]
pub fn powf(x: f64, y: f64) -> f64 {
    pow::powf(x, y)
}
