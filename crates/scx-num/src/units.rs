// Written for solidcorex (docs/09-tolerances.md §7.1): the tolerance newtypes. The
// scalar tolerance functions of `tol` read them in place of solidcore's `Tolerances`.
//! Typed lengths and tolerances.
//!
//! [`Length`] is a length in millimetres. [`Accuracy`] (per entity: a proved bound on
//! how far the stored entity lies from the one it stands for) and [`Resolution`] (per
//! operation: how close two things must be to count as one) both wrap a length and are
//! distinct types, with no arithmetic between them except [`coincident`].
//! [`ParamTol`] is a tolerance in a curve's or surface's parameter space. The glossary
//! (`docs/08-glossary.md`) gives each word its one meaning.
//!
//! Every field is private: a [`Resolution`] is above 0 and finite, an [`Accuracy`] at
//! least 0 and finite, by construction, so no reader checks again. Angle tolerances are
//! named constants ([`crate::consts::PARALLEL_ANGLE_RAD`]), not a type.

use core::ops::{Add, Div, Mul, Sub};

/// A length, in millimetres. Any `f64`, a signed distance included; the tolerance
/// types above it hold the checked ones.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Length(f64);

/// Per entity: a proved upper bound on how far the stored entity lies from the exact
/// entity it stands for. A length, at least 0 and finite.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Accuracy(Length);

/// Per operation: how close two things must be for the operation to treat them as one.
/// A length, above 0 and finite.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Resolution(Length);

/// A tolerance in parameter space (a curve's `t`, a surface's `u` or `v`).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct ParamTol(f64);

impl Length {
    /// No length.
    pub const ZERO: Self = Self(0.0);

    /// `mm` millimetres.
    #[must_use]
    pub const fn from_mm(mm: f64) -> Self {
        Self(mm)
    }

    /// The length in millimetres.
    #[must_use]
    pub const fn mm(self) -> f64 {
        self.0
    }

    /// The larger of two lengths (`f64::max`: a NaN gives way to the other).
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        Self(self.0.max(other.0))
    }

    /// The length's size, `|self|`.
    #[must_use]
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }
}

impl Add for Length {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Length {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl Mul<f64> for Length {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self(self.0 * rhs)
    }
}

impl Div<f64> for Length {
    type Output = Self;
    fn div(self, rhs: f64) -> Self {
        Self(self.0 / rhs)
    }
}

impl Accuracy {
    /// Exact: the stored entity is the one it stands for.
    pub const ZERO: Self = Self(Length::ZERO);

    /// The accuracy `bound`, if it is one: finite and at least 0 (`-0` is read as `0`).
    /// A NaN, an infinite or a negative bound is not an accuracy.
    #[must_use]
    pub fn new(bound: Length) -> Option<Self> {
        let b = bound.mm();
        (b.is_finite() && b >= 0.0).then(|| Self(Length(b.abs())))
    }

    /// [`Accuracy::new`] of `mm` millimetres.
    #[must_use]
    pub fn from_mm(mm: f64) -> Option<Self> {
        Self::new(Length(mm))
    }

    /// The bound, as a length.
    #[must_use]
    pub const fn length(self) -> Length {
        self.0
    }

    /// The bound in millimetres.
    #[must_use]
    pub const fn mm(self) -> f64 {
        self.0 .0
    }

    /// The bound of two errors that add: `self + other`.
    #[must_use]
    pub fn sum(self, other: Self) -> Length {
        self.0 + other.0
    }
}

impl Resolution {
    /// The default resolution, [`crate::consts::ZONE_DEFAULT`] (solidcore's system zone
    /// tolerance).
    pub const DEFAULT: Self = Self(Length(crate::consts::ZONE_DEFAULT));

    /// The resolution `r`, if it is one: finite and above 0.
    #[must_use]
    pub fn new(r: Length) -> Option<Self> {
        (r.mm().is_finite() && r.mm() > 0.0).then_some(Self(r))
    }

    /// [`Resolution::new`] of `mm` millimetres.
    #[must_use]
    pub fn from_mm(mm: f64) -> Option<Self> {
        Self::new(Length(mm))
    }

    /// The resolution, as a length.
    #[must_use]
    pub const fn length(self) -> Length {
        self.0
    }

    /// The resolution in millimetres.
    #[must_use]
    pub const fn mm(self) -> f64 {
        self.0 .0
    }
}

impl Default for Resolution {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The distance within which two places, on entities of accuracies `a` and `b`, are one
/// place for an operation of resolution `res`: the largest of the three
/// (`docs/09-tolerances.md` §3.1, solidcore's `coincidence_tol_3d`). The one arithmetic
/// between the two kinds of tolerance.
#[must_use]
pub fn coincident(res: Resolution, a: Accuracy, b: Accuracy) -> Length {
    res.0.max(a.0).max(b.0)
}

impl ParamTol {
    /// A tolerance of `value` parameter units.
    #[must_use]
    pub const fn new(value: f64) -> Self {
        Self(value)
    }

    /// The tolerance in parameter units.
    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_pass_through_unchanged() {
        let x = 1.0e-5;
        let a = Accuracy::from_mm(x).map(Accuracy::mm);
        let r = Resolution::from_mm(x).map(Resolution::mm);
        assert_eq!(a.map(f64::to_bits), Some(x.to_bits()));
        assert_eq!(r.map(f64::to_bits), Some(x.to_bits()));
        assert_eq!(Length::from_mm(x).mm().to_bits(), x.to_bits());
        assert_eq!(ParamTol::new(x).value().to_bits(), x.to_bits());
    }

    #[test]
    fn only_checked_values_are_tolerances() {
        for bad in [0.0, -1.0e-5, f64::NAN, f64::INFINITY] {
            assert_eq!(Resolution::from_mm(bad), None, "{bad}");
        }
        for bad in [-1.0e-5, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(Accuracy::from_mm(bad), None, "{bad}");
        }
        assert_eq!(Accuracy::from_mm(-0.0).map(|a| a.mm().to_bits()), Some(0));
        assert_eq!(Resolution::DEFAULT.mm(), crate::consts::ZONE_DEFAULT);
    }

    #[test]
    fn coincident_is_the_largest() {
        let res = Resolution::DEFAULT;
        let small = Accuracy::ZERO;
        let big = Accuracy::from_mm(4.0e-5).unwrap_or(Accuracy::ZERO);
        assert_eq!(coincident(res, small, small), res.length());
        assert_eq!(coincident(res, big, small), big.length());
        assert_eq!(coincident(res, small, big), big.length());
    }
}
