// Written for solidcorex. Apache-2.0.
//! Finite working ranges for parameters that have no end.
//!
//! An extent's ends are always finite ([`Extent1d`](crate::extent1d::Extent1d)). A curve or
//! surface parameter can still run without end (a line, a plane, a parabola). Numeric work on
//! such a parameter runs on a finite range picked by a named [`Clamp`] rule. The rule is stated
//! where the work is done, and no value marks a missing end.
//!
//! Upstream marks a missing end with a value and clamps with fixed constants (`SmMath.h`):
//!
//! | Rust                 | Upstream                                                        |
//! |----------------------|-----------------------------------------------------------------|
//! | [`Clamp::Sample`]    | `SM_BOUNDED_INFINITE_PARAM` (33), the half-size for sampling     |
//! | [`Clamp::Squared`]   | `SM_INFINITE_PARAMETER_SQRT` (1111.111 = √1234567), for kinds whose point grows as the square of the parameter |
//! | [`Clamp::Linear`]    | `SM_INFINITE_PARAMETER` (1234567) used as a bound (not a marker) |
//! | (none)               | `SM_INFINITE_PARAMETER` as an end marker, `SM_IS_INFINITE`       |
//!
//! Deviations:
//! - The general bound is sized from the model ([`Reach`]), not fixed at 1234567. A fixed
//!   bound far larger than the part costs precision: solidcore #18922 found 3.4e-11 rounding
//!   on a plane copy sized at 1.2e6.
//! - [`Clamp::Squared`] takes the same [`Reach`] and clamps at its square root, rounded down so
//!   that the square of the half-width never exceeds the reach.

use crate::extent3d::Extent3d;
use crate::math::Point3;

/// How far a model reaches: a finite, positive length that a working range must cover.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Reach(f64);

impl Reach {
    /// The reach `r`, `None` unless `r` is finite and above zero.
    #[must_use]
    pub fn new(r: f64) -> Option<Self> {
        (r.is_finite() && r > 0.0).then_some(Self(r))
    }

    /// Twice the distance from `origin` to the farthest corner of `model`, so that a line
    /// through `origin` parameterised by arc length covers the whole box with room to spare.
    /// `None` when that distance is zero or not finite.
    #[must_use]
    pub fn covering(model: &Extent3d, origin: Point3) -> Option<Self> {
        let far = |lo: f64, hi: f64, o: f64| (o - lo).abs().max((hi - o).abs());
        let (lo, hi) = (model.min(), model.max());
        let d = crate::math::Vec3::new(
            far(lo.x, hi.x, origin.x),
            far(lo.y, hi.y, origin.y),
            far(lo.z, hi.z, origin.z),
        );
        Self::new(2.0 * d.length())
    }

    /// The length.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// A named rule for the finite half-width of a parameter range that has no end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Clamp {
    /// A fixed half-width of 33 for sampling an endless range (upstream
    /// `SM_BOUNDED_INFINITE_PARAM`): enough samples to see the shape, not a covering bound.
    Sample,
    /// A half-width of the reach itself, for a kind whose point moves linearly with the
    /// parameter (line, plane, cylinder along its axis).
    Linear(Reach),
    /// A half-width of the square root of the reach, for a kind whose point grows as the
    /// square of the parameter (parabola), so the evaluated point stays within the reach
    /// (upstream `SM_INFINITE_PARAMETER_SQRT`).
    Squared(Reach),
}

impl Clamp {
    /// The half-size for [`Clamp::Sample`] (upstream `SM_BOUNDED_INFINITE_PARAM`).
    pub const SAMPLE_HALF_WIDTH: f64 = 33.0;

    /// The finite, positive half-width this rule gives.
    #[must_use]
    pub fn half_width(self) -> f64 {
        match self {
            Self::Sample => Self::SAMPLE_HALF_WIDTH,
            Self::Linear(r) => r.get(),
            Self::Squared(r) => sqrt_down(r.get()),
        }
    }
}

/// The largest `h` with `h * h <= r` (rounded), for finite `r > 0`.
fn sqrt_down(r: f64) -> f64 {
    let mut h = r.sqrt();
    // `sqrt` is correctly rounded, so at most a step or two separates it from the bound.
    while h > 0.0 && h * h > r {
        h = f64::from_bits(h.to_bits() - 1);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::Rng;

    #[test]
    fn reach_is_finite_and_positive() {
        assert_eq!(Reach::new(0.0), None);
        assert_eq!(Reach::new(-1.0), None);
        assert_eq!(Reach::new(f64::INFINITY), None);
        assert_eq!(Reach::new(f64::NAN), None);
        assert_eq!(Reach::new(2.5).map(Reach::get), Some(2.5));
    }

    #[test]
    fn reach_covers_the_model_box() {
        let b = Extent3d::from_bounds(-1.0, 0.0, 2.0, 3.0, 4.0, 2.0).unwrap();
        let r = Reach::covering(&b, Point3::new(0.0, 0.0, 0.0)).unwrap();
        // Farthest corner (3, 4, 2): distance √29.
        assert_eq!(r.get(), 2.0 * 29.0_f64.sqrt());
        let point = Extent3d::from_point(Point3::new(1.0, 1.0, 1.0)).unwrap();
        assert_eq!(Reach::covering(&point, Point3::new(1.0, 1.0, 1.0)), None);
    }

    #[test]
    fn sample_keeps_the_pin_half_size() {
        assert_eq!(Clamp::Sample.half_width(), 33.0);
    }

    #[test]
    fn pin_squared_bound_is_the_root_of_its_linear_bound() {
        // Upstream's 1111.111 is √1234567 truncated; the rule gives the exact root rounded down.
        let r = Reach::new(1_234_567.0).unwrap();
        let h = Clamp::Squared(r).half_width();
        assert!((h - 1111.111).abs() < 1e-3);
        assert!(h * h <= r.get());
        assert_eq!(Clamp::Linear(r).half_width(), 1_234_567.0);
    }

    /// Property: for any reach, the squared rule's half-width squares to at most the reach and
    /// sits within two steps below the rounded root; the linear rule is the reach itself.
    #[test]
    fn clamp_rules_hold_for_random_reaches() {
        let mut rng = Rng::new(0x5eed_c1a3);
        for _ in 0..20_000 {
            // Reaches across 1e-300 .. 1e300.
            let e = rng.next_signed_unit() * 300.0;
            let m = 1.0 + rng.next_f64();
            let r = m * crate::fmath::powf(10.0, e);
            let Some(reach) = Reach::new(r) else { continue };
            let h = Clamp::Squared(reach).half_width();
            assert!(h.is_finite() && h > 0.0, "{r}");
            assert!(h * h <= r, "{r}: {h}");
            assert!(
                r.sqrt().to_bits() - h.to_bits() <= 2,
                "{r}: {h} is too far below the root"
            );
            assert_eq!(Clamp::Linear(reach).half_width(), r);
        }
    }

    /// Property: every rule's half-width is finite and positive, so a working range built from
    /// it is a valid extent.
    #[test]
    fn half_widths_are_finite_and_positive() {
        for r in [f64::MIN_POSITIVE, 1e-12, 1.0, 1e12, f64::MAX] {
            let reach = Reach::new(r).unwrap();
            for c in [Clamp::Sample, Clamp::Linear(reach), Clamp::Squared(reach)] {
                let h = c.half_width();
                assert!(h.is_finite() && h > 0.0, "{c:?}");
            }
        }
    }
}
