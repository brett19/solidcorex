// Written for solidcorex. Apache-2.0.
//! Parameter domains, and finite working ranges for parameters that have no end.
//!
//! An extent's ends are always finite ([`Extent1d`]). A curve or surface parameter can still
//! run without end (a line, a plane, a parabola) or repeat (a circle). [`ParamDomain`] says
//! which, one per direction ([`UvDomain`] for a surface), and holds only the ends it has.
//! Numeric work on a parameter with no end runs on a finite range,
//! [`ParamDomain::working`], picked by a named [`Clamp`] rule. The rule is stated where the
//! work is done, and no value marks a missing end.
//!
//! Upstream marks a missing end with a value and clamps with fixed constants (`SmMath.h`):
//!
//! | Rust                 | Upstream                                                        |
//! |----------------------|-----------------------------------------------------------------|
//! | [`Clamp::Sample`]    | `SM_BOUNDED_INFINITE_PARAM` (33), the half-size for sampling     |
//! | [`Clamp::Squared`]   | `SM_INFINITE_PARAMETER_SQRT` (1111.111 = √1234567), for kinds whose point grows as the square of the parameter |
//! | [`Clamp::Linear`]    | `SM_INFINITE_PARAMETER` (1234567) used as a bound (not a marker) |
//! | (none)               | `SM_INFINITE_PARAMETER` as an end marker, `SM_IS_INFINITE`       |
//! | [`ParamDomain`], [`UvDomain`] | `GetNaturalParamDomain`'s extent with marked ends, and the kind's periodicity flags |
//! | [`ParamDomain::working`] | `ApproximateUnbounded(center = 0, half)`                    |
//!
//! Deviations:
//! - The general bound is sized from the model ([`Reach`]), not fixed at 1234567. A fixed
//!   bound far larger than the part costs precision: solidcore #18922 found 3.4e-11 rounding
//!   on a plane copy sized at 1.2e6.
//! - [`Clamp::Squared`] takes the same [`Reach`] and clamps at its square root, rounded down so
//!   that the square of the half-width never exceeds the reach.
//! - A domain with no end is a variant, not an extent with marked ends: no arithmetic can
//!   reach a missing end, and a real end equal to the marker is not taken for one. A
//!   one-sided domain's working end saturates at `±f64::MAX` where upstream's overflows.

use crate::extent1d::Extent1d;
use crate::extent2d::Extent2d;
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

/// The parameter range a curve or surface kind defines in one direction.
///
/// Every value is valid by construction: the variants that carry numbers are
/// `#[non_exhaustive]`, so outside this crate they are made only through the checked
/// constructors ([`ParamDomain::periodic`], [`ParamDomain::at_least`],
/// [`ParamDomain::at_most`]) and matched with `..`. An [`Extent1d`] is already valid, so
/// [`ParamDomain::Bounded`] is built directly.
///
/// The domain holds no end it does not have: an endless direction is a variant, not a value.
/// Numeric work that needs finite ends asks for them with [`ParamDomain::working`] and names
/// the [`Clamp`] rule it uses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamDomain {
    /// A closed direction that repeats every `period` (finite, above zero); one period starts
    /// at `base` (finite), and `base + period` is finite.
    #[non_exhaustive]
    Periodic {
        /// The period.
        period: f64,
        /// Where the period the kind reports starts.
        base: f64,
    },
    /// A range with both ends.
    Bounded(Extent1d),
    /// From `from` (finite) without an upper end.
    #[non_exhaustive]
    AtLeast {
        /// The lower end.
        from: f64,
    },
    /// Up to `to` (finite) without a lower end.
    #[non_exhaustive]
    AtMost {
        /// The upper end.
        to: f64,
    },
    /// No end on either side.
    Unbounded,
}

impl ParamDomain {
    /// A periodic direction, `None` unless `period` is finite and above zero, `base` is
    /// finite and `base + period` is finite.
    #[must_use]
    pub fn periodic(period: f64, base: f64) -> Option<Self> {
        let ok =
            period.is_finite() && period > 0.0 && base.is_finite() && (base + period).is_finite();
        ok.then_some(Self::Periodic { period, base })
    }

    /// A direction from `from` upward, `None` unless `from` is finite.
    #[must_use]
    pub fn at_least(from: f64) -> Option<Self> {
        from.is_finite().then_some(Self::AtLeast { from })
    }

    /// A direction up to `to`, `None` unless `to` is finite.
    #[must_use]
    pub fn at_most(to: f64) -> Option<Self> {
        to.is_finite().then_some(Self::AtMost { to })
    }

    /// The range of one period, for a periodic direction.
    #[must_use]
    pub fn period(&self) -> Option<f64> {
        match *self {
            Self::Periodic { period, .. } => Some(period),
            _ => None,
        }
    }

    /// A finite range to work on, for half-width `h = rule.half_width()`:
    /// - a bounded direction: its range;
    /// - a periodic direction: the period from `base`;
    /// - no end: `[-h, h]`;
    /// - from `a`: `[a, a + 2h]` when `a > -h`, else `[-h, h]`;
    /// - up to `b`: `[b - 2h, b]` when `b < h`, else `[-h, h]`.
    ///
    /// A one-sided range is a window `2h` wide at its end, or the window about the parameter
    /// origin when that window lies inside the domain: the origin is where the kind is
    /// placed, so `[-h, h]` is the part a [`Reach`] measured from it covers. An end beyond
    /// the `f64` range saturates at `±f64::MAX`.
    ///
    /// Upstream: `ApproximateUnbounded(center = 0, half)` on the extent with
    /// `SM_INFINITE_PARAMETER` marking the missing ends.
    #[must_use]
    pub fn working(&self, rule: Clamp) -> Extent1d {
        let h = rule.half_width();
        let sat = |x: f64| x.clamp(-f64::MAX, f64::MAX);
        let (min, max) = match *self {
            Self::Bounded(e) => return e,
            Self::Periodic { period, base } => (base, base + period),
            Self::AtLeast { from } if from > -h => (from, sat(from + 2.0 * h)),
            Self::AtMost { to } if to < h => (sat(to - 2.0 * h), to),
            // No end, or the window about the origin lies inside the domain.
            Self::Unbounded | Self::AtLeast { .. } | Self::AtMost { .. } => (-h, h),
        };
        Extent1d::from_ordered(min, max)
    }
}

/// The parameter domain of a surface: one [`ParamDomain`] per direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UvDomain {
    /// The `u` direction.
    pub u: ParamDomain,
    /// The `v` direction.
    pub v: ParamDomain,
}

impl UvDomain {
    /// The domain `u × v`.
    #[must_use]
    pub const fn new(u: ParamDomain, v: ParamDomain) -> Self {
        Self { u, v }
    }

    /// [`ParamDomain::working`] in each direction, each under its own rule (a kind may grow
    /// linearly in one direction and as a square in the other).
    #[must_use]
    pub fn working(&self, u: Clamp, v: Clamp) -> Extent2d {
        Extent2d::from_intervals(self.u.working(u), self.v.working(v))
    }
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

    #[test]
    fn domain_constructors_refuse_ends_that_are_not_finite() {
        assert_eq!(ParamDomain::at_least(f64::INFINITY), None);
        assert_eq!(ParamDomain::at_most(f64::NAN), None);
        assert_eq!(
            ParamDomain::at_least(-2.0),
            Some(ParamDomain::AtLeast { from: -2.0 })
        );
        assert_eq!(ParamDomain::periodic(0.0, 0.0), None);
        assert_eq!(ParamDomain::periodic(-1.0, 0.0), None);
        assert_eq!(ParamDomain::periodic(f64::INFINITY, 0.0), None);
        assert_eq!(ParamDomain::periodic(1.0, f64::NAN), None);
        assert_eq!(ParamDomain::periodic(f64::MAX, f64::MAX), None);
        let p = ParamDomain::periodic(core::f64::consts::TAU, 1.0).unwrap();
        assert_eq!(p.period(), Some(core::f64::consts::TAU));
        assert_eq!(ParamDomain::Unbounded.period(), None);
    }

    #[test]
    fn working_ranges_follow_the_window_rule() {
        let w = |d: ParamDomain| {
            let e = d.working(Clamp::Sample);
            (e.min(), e.max())
        };
        assert_eq!(w(ParamDomain::Unbounded), (-33.0, 33.0));
        assert_eq!(w(ParamDomain::AtLeast { from: 5.0 }), (5.0, 71.0));
        assert_eq!(w(ParamDomain::AtLeast { from: -40.0 }), (-33.0, 33.0));
        assert_eq!(w(ParamDomain::AtMost { to: -5.0 }), (-71.0, -5.0));
        assert_eq!(w(ParamDomain::AtMost { to: 40.0 }), (-33.0, 33.0));
        let b = Extent1d::new(-1e9, 2.0).unwrap();
        assert_eq!(w(ParamDomain::Bounded(b)), (-1e9, 2.0));
        let p = ParamDomain::periodic(4.0, -1.0).unwrap();
        assert_eq!(w(p), (-1.0, 3.0));
        // The window's far end saturates at the end of the `f64` range.
        let huge = Clamp::Linear(Reach::new(f64::MAX).unwrap());
        let e = ParamDomain::AtLeast { from: 1.0 }.working(huge);
        assert_eq!((e.min(), e.max()), (1.0, f64::MAX));
        let e = ParamDomain::AtMost { to: -1.0 }.working(huge);
        assert_eq!((e.min(), e.max()), (-f64::MAX, -1.0));
    }

    #[test]
    fn surface_domains_take_a_rule_per_direction() {
        let d = UvDomain::new(
            ParamDomain::periodic(2.0, 0.0).unwrap(),
            ParamDomain::Unbounded,
        );
        let r = Clamp::Linear(Reach::new(10.0).unwrap());
        let e = d.working(Clamp::Sample, r);
        let (lo, hi) = (e.min(), e.max());
        assert_eq!([lo.x, lo.y, hi.x, hi.y], [0.0, -10.0, 2.0, 10.0]);
    }

    /// Property: every domain's working range, under every rule, is a valid finite extent
    /// that lies in the domain and holds the domain's finite end.
    #[test]
    fn working_ranges_are_valid_for_random_domains() {
        let mut rng = Rng::new(0x5eed_d0a1);
        let wide = |rng: &mut Rng| {
            let e = rng.next_signed_unit() * 300.0;
            rng.next_signed_unit() * crate::fmath::powf(10.0, e)
        };
        for _ in 0..20_000 {
            let (a, b) = (wide(&mut rng), wide(&mut rng));
            let Some(reach) = Reach::new(wide(&mut rng).abs()) else {
                continue;
            };
            let domains = [
                Some(ParamDomain::Unbounded),
                ParamDomain::at_least(a),
                ParamDomain::at_most(a),
                Extent1d::spanning(a, b).map(ParamDomain::Bounded),
                ParamDomain::periodic(b.abs(), a),
            ];
            for d in domains.into_iter().flatten() {
                for c in [Clamp::Sample, Clamp::Linear(reach), Clamp::Squared(reach)] {
                    let e = d.working(c);
                    let (lo, hi) = (e.min(), e.max());
                    assert!(lo.is_finite() && hi.is_finite() && lo <= hi, "{d:?} {c:?}");
                    match d {
                        ParamDomain::AtLeast { from } => assert!(lo >= from, "{d:?} {c:?}"),
                        ParamDomain::AtMost { to } => assert!(hi <= to, "{d:?} {c:?}"),
                        ParamDomain::Bounded(x) => assert_eq!(e, x),
                        _ => {}
                    }
                    if let ParamDomain::AtLeast { from } = d {
                        assert!(lo == from || (lo..=hi).contains(&0.0), "{d:?} {c:?}");
                    }
                }
            }
        }
    }
}
