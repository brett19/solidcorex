// Written for solidcorex (docs/09-tolerances.md §7.1): the tolerance newtypes. The
// scalar tolerance functions of `tol` read them in place of solidcore's `Tolerances`.
// Then the zone, section and approximation tolerances moved onto `Resolution` as
// `Length`s (`zone`, `xsect`, `approx`; `length` became `zone`), and the `mm`/`from_mm`
// shortcuts on `Accuracy` and `Resolution` went, so `f64` meets a length only at
// `Length::from_mm` and `Length::mm`.
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
//!
//! # Where a length meets `f64`
//!
//! A tolerance is typed from where it is made to where it is compared: a [`Resolution`]
//! gives its zone, section and approximation tolerances as [`Length`]s
//! ([`Resolution::zone`], [`Resolution::xsect`], [`Resolution::approx`]), and sums,
//! maxima and scalings of tolerances stay [`Length`]. Geometry (coordinates, distances,
//! sizes, scaled zeros) stays `f64`. The two meet in one place only:
//! [`Length::from_mm`] in and [`Length::mm`] out, with no `From`, `Deref` or
//! `mm()` on the tolerance types. A predicate that compares a distance against a
//! tolerance (`crate::tol::within_coincidence_tol`, `crate::tol::is_degenerate`) calls
//! `mm()` at the comparison; a caller that feeds a tolerance into `f64` arithmetic calls
//! it where the arithmetic starts.

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

    /// The bound, as a length.
    #[must_use]
    pub const fn length(self) -> Length {
        self.0
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

    /// The zone tolerance: the resolution itself, the distance within which two places
    /// are one (solidcore's `zone_tol3d`).
    #[must_use]
    pub const fn zone(self) -> Length {
        self.0
    }

    /// The section tolerance, twice the zone (solidcore's `xsect_tol3d`): how far a
    /// computed intersection may stand from each of the entities it lies on.
    #[must_use]
    pub fn xsect(self) -> Length {
        self.0 * 2.0
    }

    /// The approximation tolerance, half the zone (solidcore's `approx_tol3d`): how far a
    /// fitted entity may stand from the one it approximates.
    #[must_use]
    pub fn approx(self) -> Length {
        self.0 * 0.5
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
        let a = Accuracy::new(Length::from_mm(x)).map(|a| a.length().mm());
        let r = Resolution::new(Length::from_mm(x)).map(|r| r.zone().mm());
        assert_eq!(a.map(f64::to_bits), Some(x.to_bits()));
        assert_eq!(r.map(f64::to_bits), Some(x.to_bits()));
        assert_eq!(Length::from_mm(x).mm().to_bits(), x.to_bits());
        assert_eq!(ParamTol::new(x).value().to_bits(), x.to_bits());
    }

    #[test]
    fn only_checked_values_are_tolerances() {
        for bad in [0.0, -1.0e-5, f64::NAN, f64::INFINITY] {
            assert_eq!(Resolution::new(Length::from_mm(bad)), None, "{bad}");
        }
        for bad in [-1.0e-5, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(Accuracy::new(Length::from_mm(bad)), None, "{bad}");
        }
        let neg_zero = Accuracy::new(Length::from_mm(-0.0));
        assert_eq!(neg_zero.map(|a| a.length().mm().to_bits()), Some(0));
        assert_eq!(Resolution::DEFAULT.zone().mm(), crate::consts::ZONE_DEFAULT);
    }

    #[test]
    fn coincident_is_the_largest() {
        let res = Resolution::DEFAULT;
        let small = Accuracy::ZERO;
        let big = Accuracy::new(Length::from_mm(4.0e-5)).unwrap_or(Accuracy::ZERO);
        assert_eq!(coincident(res, small, small), res.zone());
        assert_eq!(coincident(res, big, small), big.length());
        assert_eq!(coincident(res, small, big), big.length());
    }
}
