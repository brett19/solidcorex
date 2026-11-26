// Moved from solidcore `solidcore/crates/solidcore/src/blend/fillet_section/tay.rs` (lines 26 to
// 131: the scalar rings and their functions) at commit 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Fork: `sm_Tay`, `sm_TayOf`, its `operator+ - * /`,
// `sm_Wide`, `sm_Acos`, `sm_SinCos`, `sm_TaySqrt`, `sm_TaySinCos`, `sm_PowN`,
// `sm_GnAreaIntegrand`, `sm_GnAreaSum`, `sm_GnWedgeCos`, `sm_GnHalfAngle`, `sm_GnTarget`,
// `sm_GnInvert` and `sm_GnAreaCells` in `source/SMLib/src/SmFilletSection.cpp`
// (tangency-labs/smlib@7102bc5; the cells' sums off the grid and at points kept once each,
// f9b133a, smlib#109); see [`super`] for the table.
// Changes: only the ring traits (`Ring`, `TayScalar`), their `f64` and `Ivl` impls, `wide` and
// `acos_clamped` moved; the series `Tay`, its functions and `GnAreaCells` (which take the kernel
// and a surface counter) stay in solidcore. `Ivl` from `crate::interval`, `sqrt` from
// `crate::vector3d`. Results unchanged bit for bit.
//! The scalar rings of the G^n certificate's templates (the fork's `Z` and `S`): `f64`
//! and [`Ivl`], with the functions a series takes in them (square root, arc cosine, sine
//! and cosine), the interval ones widened past the C library's error.
//!
//! The truncated series over these rings (`sm_Tay`) stays above, with the blend.

use core::ops::{Add, Div, Mul, Sub};

use crate::interval::Ivl;

/// The ring `Z` of the fork's templates: `double`, `sm_Ivl` and series over either.
pub trait Ring:
    Copy + Add<Output = Self> + Sub<Output = Self> + Mul<Output = Self> + Div<Output = Self>
{
    /// `Z(double)`.
    fn lift(d: f64) -> Self;
}

impl Ring for f64 {
    fn lift(d: f64) -> Self {
        d
    }
}

impl Ring for Ivl {
    fn lift(d: f64) -> Self {
        Ivl::from(d)
    }
}

/// The scalars a series' functions are taken in (`double`, `sm_Ivl`).
pub trait TayScalar: Ring {
    /// `sm_Sqrt`.
    #[must_use]
    fn sqrt_s(self) -> Self;
    /// `sm_Acos`.
    #[must_use]
    fn acos_s(self) -> Self;
    /// `sm_SinCos`: `(sin, cos)`.
    fn sin_cos_s(self) -> (Self, Self);
}

/// `smos_Min(a, b)`: `(a < b) ? a : b`.
#[inline]
fn smin(a: f64, b: f64) -> f64 {
    if a < b {
        a
    } else {
        b
    }
}

/// `smos_Max(a, b)`: `(a > b) ? a : b`.
#[inline]
fn smax(a: f64, b: f64) -> f64 {
    if a > b {
        a
    } else {
        b
    }
}

/// `sm_Acos(double)`: the arc cosine of x clamped to [-1, 1].
#[must_use]
pub fn acos_clamped(x: f64) -> f64 {
    smax(-1.0, smin(1.0, x)).acos()
}

impl TayScalar for f64 {
    fn sqrt_s(self) -> Self {
        crate::vector3d::sqrt(self)
    }
    fn acos_s(self) -> Self {
        acos_clamped(self)
    }
    fn sin_cos_s(self) -> (Self, Self) {
        (self.sin(), self.cos())
    }
}

/// `sm_Wide`: `[a, b]` widened by `1e-15` relative (the C library's `acos`, `sin` and
/// `cos` are within an ulp or two), or the whole line if either is NaN.
#[must_use]
pub fn wide(a: f64, b: f64) -> Ivl {
    if a.is_nan() || b.is_nan() {
        return Ivl::whole();
    }
    Ivl::new(
        a - (a.abs() * 1.0e-15 + 1.0e-300),
        b + (b.abs() * 1.0e-15 + 1.0e-300),
    )
}

impl TayScalar for Ivl {
    fn sqrt_s(self) -> Self {
        self.sqrt()
    }
    fn acos_s(self) -> Self {
        let inside = self.lo >= -1.0 && self.hi <= 1.0;
        if !inside {
            return Ivl::whole();
        }
        wide(self.hi.acos(), self.lo.acos())
    }
    /// On `[0, pi/2]` (the double below pi/2 its top), where sin rises and cos falls; the
    /// whole line elsewhere.
    fn sin_cos_s(self) -> (Self, Self) {
        let inside = self.lo >= 0.0 && self.hi <= core::f64::consts::FRAC_PI_2;
        if !inside {
            return (Ivl::whole(), Ivl::whole());
        }
        (
            wide(self.lo.sin(), self.hi.sin()),
            wide(self.hi.cos(), self.lo.cos()),
        )
    }
}
