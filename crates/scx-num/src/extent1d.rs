// Moved from solidcore `solidcore/crates/solidcore/src/geom/extent1d.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/src/SmExtent1d.cpp`,
// `source/SMLib/inc/SmExtent1d.h`, `source/SMLib/src/SmExtentNd.cpp`,
// `source/SMLib/inc/SmExtentNd.h`.
// Changes: the digest impl `Content::content` stays in solidcore; errors are `NumError`; the
// test `set_operations` checks the error variant in place of the status number; paths.
// `is_defined` (`AssertDefined`, the destructor's `SM_UNDEF_DOUBLE` sentinel) is gone
// (`docs/01-architecture.md` §10.1); the test `construct_and_access` drops its two checks.
// Then the inside-out empty interval (`min = 1e20`, `max = -1e20`) went: an `Extent1d` is never
// inverted, and a fold is a `Hull1d` (±inf until a value is added, `finish` gives `None` when
// none was). `new`, `Default`, `init`, `is_init`, `has_negative_length`, `from_raw`, the
// `set_*` setters, `add_value` and `ExtentPointType::code` went; `from_bounds`, `with_min`,
// `with_max`, `shrunk` and `intersect` answer `Option`; the swapping constructor is `spanning`;
// the transforms take and return the value; `dont_cross: bool` is `AtSeam`; `ExtentNd` is built
// from its intervals.
//! One-dimensional intervals (`[min, max]`, often a curve's parameter domain) with
//! clamping, snapping, normalised evaluation/inversion, set operations, predicates,
//! periodic wrapping/containment/intersection, and N-dimensional interval boxes.
//!
//! Upstream: `source/SMLib/src/SmExtent1d.cpp`, `source/SMLib/inc/SmExtent1d.h`,
//! `source/SMLib/src/SmExtentNd.cpp`, `source/SMLib/inc/SmExtentNd.h`.
//!
//! | Rust                                         | Upstream                                     |
//! |----------------------------------------------|----------------------------------------------|
//! | [`Extent1d`]                                 | `SmExtent1d`                                 |
//! | [`Hull1d`] ([`Hull1d::EMPTY`], [`Hull1d::add`], [`Hull1d::finish`]) | `SmExtent1d()`, `Init`, `AddValue`, `IsInit` |
//! | [`Extent1d::point`]                          | `SmExtent1d(double)`                         |
//! | [`Extent1d::spanning`]                       | `SmExtent1d(double, double)`                 |
//! | [`Extent1d::from_bounds`]                    | `SetMinMax` (`None` where it fails)          |
//! | [`Extent1d::with_min`], [`Extent1d::with_max`] | `SetMin`, `SetMax` (`None` where they fail) |
//! | `Clone`/`Copy`                               | copy constructor, `operator=`, destructor    |
//! | [`Extent1d::UNBOUNDED`]                      | `SetUnbounded`                               |
//! | [`Extent1d::negated`]                        | `Negate`                                     |
//! | [`Extent1d::translated`]                     | `Translate`                                  |
//! | [`Extent1d::min`], [`Extent1d::max`], [`Extent1d::mid`] | `GetMin`, `GetMax`, `GetMid`      |
//! | [`Extent1d::length`]                         | `GetLength`                                  |
//! | [`Extent1d::max_dimension`]                  | `GetMaxDimension`                            |
//! | [`Extent1d::t_left_eval`]                    | `GetTLeftEval`                               |
//! | [`Extent1d::clamp_value`]                    | `ClampValue`                                 |
//! | [`Extent1d::snap_value`]                     | `SnapValue`                                  |
//! | [`Extent1d::evaluate`]                       | `Evaluate`                                   |
//! | [`Extent1d::union`]                          | `Union`                                      |
//! | [`Extent1d::distance_from`]                  | `DistanceFrom`                               |
//! | [`Extent1d::intersect`]                      | `Intersect` (`None` where it fails)          |
//! | [`Extent1d::expanded`], [`Extent1d::shrunk`] | `ExpandAbsolute`                             |
//! | [`Extent1d::expanded_relative`]              | `ExpandRelative`                             |
//! | [`Extent1d::scaled`]                         | `Scale`                                      |
//! | [`Extent1d::inversion`], [`Inversion`]       | `Inversion`                                  |
//! | [`Extent1d::are_disjoint`]                   | `AreDisjoint`                                |
//! | [`Extent1d::are_equal`]                      | `AreEqual(const SmExtent1d&, double)`        |
//! | [`Extent1d::are_equal_bounds`]               | `AreEqual(double, double, double)`           |
//! | [`Extent1d::is_contained_by`]                | `IsContainedBy`                              |
//! | [`Extent1d::contains_value`]                 | `ContainsValue`                              |
//! | [`Extent1d::is_value_on_boundary`]           | `IsValueOnBoundary`                          |
//! | [`Extent1d::classify_point`], [`ExtentPointType`] | `ClassifyPoint`, `SmExtentPointType`    |
//! | [`Extent1d::is_degenerate`]                  | `IsDegenerate`                               |
//! | [`Extent1d::boundary_type`], [`Extent1d::is_bounded`] | `IsBounded`                         |
//! | [`Extent1d::approximate_unbounded`], [`Extent1d::approximate_unbounded_expanded`] | `ApproximateUnbounded` |
//! | [`Extent1d::are_same`]                       | `operator==`                                 |
//! | [`Extent1d::are_disjoint_periodic`]          | `AreDisjointPeriodic`                        |
//! | [`Extent1d::periodic_wrap`]                  | `PeriodicWrap`                               |
//! | [`Extent1d::intersect_periodic`], [`AtSeam`] | `IntersectPeriodic`                          |
//! | [`Extent1d::clamp_periodic_value`]           | `ClampPeriodicValue`                         |
//! | [`Extent1d::contains_periodic_value`], [`PeriodicContainment`] | `ContainsPeriodicValue`    |
//! | [`Extent1d::is_closed`]                      | `IsClosed`                                   |
//! | [`Extent1d::is_value_on_periodic_boundary`]  | `IsValueOnPeriodicBoundary`                  |
//! | [`ExtentNd`], [`ExtentNd::new`]              | `SmExtentNd`, `SmExtentNd(ULONG)` and the assignments, destructor |
//! | [`ExtentNd::get`], [`ExtentNd::get_mut`]     | `operator[] const`, `operator[]`             |
//! | [`ExtentNd::clamp_vector`]                   | `ClampVector`                                |
//! | `are_same`, `are_same_to_tol` (private)      | `SM_ARE_SAME`, `SM_ARE_SAME_TO_TOL`          |
//!
//! Deviations:
//! - Out-params become return values: [`Inversion`], [`PeriodicContainment`], tuples,
//!   `Option` for `ApproximateUnbounded`'s expanded extent (`None` where upstream's
//!   `SetMinMax` rejects it and leaves the caller's extent untouched).
//!   Optional out-params are always computed (they have no side effects).
//! - Status codes: `SM_ERR_INVALID_INPUT` is [`NumError::InvalidInput`]; `SM_ERR`
//!   is a typed [`crate::error::NumFailure`] (status 1001). The non-periodic branch of
//!   `IntersectPeriodic` leaves an `Init()` interval as the result when it fails; the Rust
//!   error carries no result. `Intersect`'s failure is `None`.
//! - No interval is inverted, so the pin's `Init()` state is a [`Hull1d`] with nothing added.
//!   A hull starts at ±inf, not ±1e20: values beyond ±1e20 keep their place in it.
//! - `ClampVector` keeps upstream's in/out arrays (`SetSize` keeps the existing
//!   prefix and zero-fills, unbounded dimensions are not written, counts are
//!   incremented); arrays shorter than the dimension (undefined behaviour upstream)
//!   are rejected with [`NumError::InvalidInput`].
//! - `ContainsPeriodicValue` moves a value within the tolerance of the top of the period
//!   above the start down by the period, and keeps it where that lies in the interval,
//!   instead of setting it to the start (WR-509, #1518; tangency-labs/smlib#46).
//! - `SM_ASSERT*` debug checks and the error-log side effect of `SE(...)` are not
//!   ported (they do not change results). The destructor's "undefined" sentinel has
//!   no Rust counterpart (a dropped value cannot be used), nor has `AssertDefined`.

use crate::error::{Invalid, NumError, Result};
use crate::extent3d::BoundaryType;
use crate::math::{is_infinite_param, EFF_ZERO, EFF_ZERO_DEG, INFINITE_PARAMETER};
use crate::tol::{are_same, are_same_to_tol};

/// Classification of a value against an interval (`SmExtentPointType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExtentPointType {
    /// `SM_EP_START`: on the minimum boundary.
    Start,
    /// `SM_EP_INSIDE`: in the interior.
    Inside,
    /// `SM_EP_END`: on the maximum boundary.
    End,
    /// `SM_EP_BOTH`: on both boundaries (degenerate interval).
    Both,
    /// `SM_EP_OUTSIDE`: outside the interval.
    Outside,
    /// `SM_EP_UNKNOWN`: not classified.
    Unknown,
}

/// Result of [`Extent1d::inversion`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Inversion {
    /// The normalised parameter, `None` for a (near) zero-length interval (upstream
    /// `SM_UNDEF_DOUBLE`).
    pub parameter: Option<f64>,
    /// True when the value lies in the closed interval.
    pub contained: bool,
}

impl Inversion {
    /// True when upstream returns `SM_SUCCESS` (contained and a parameter exists).
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.contained && self.parameter.is_some()
    }
}

/// Result of [`Extent1d::contains_periodic_value`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeriodicContainment {
    /// True when the value, after accounting for periodicity, is inside within the
    /// tolerance.
    pub contained: bool,
    /// Best "contained" periodic value (no snapping).
    pub periodic_value: f64,
    /// Distance from the periodic value to the minimum boundary.
    pub dist_to_min: f64,
    /// Distance from the periodic value to the maximum boundary.
    pub dist_to_max: f64,
}

/// What [`Extent1d::intersect_periodic`] does at this interval's ends when the other
/// interval runs across them (upstream's `bDontCross`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AtSeam {
    /// Keep a piece whole across the seam (`bDontCross = false`, upstream's default).
    #[default]
    Join,
    /// Split pieces at this interval's ends (`bDontCross = true`).
    Split,
}

/// A closed interval `[min, max]` on the real line (`SmExtent1d`), never inverted.
///
/// The invariant is `!(min > max)`: a NaN end is held (no comparison with it decides), an
/// inverted pair is not. An interval that may be absent is an `Option<Extent1d>`; a fold
/// over values is a [`Hull1d`]. Unbounded ends are stored as `±INFINITE_PARAMETER`.
///
/// `PartialEq` compares the ends exactly (not upstream `operator==`, which is
/// [`Extent1d::are_same`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extent1d {
    min: f64,
    max: f64,
}

/// A running hull of values, for folds (`SmExtent1d()` then `AddValue`).
///
/// It starts at the identity of the union, `[+inf, -inf]`, so [`Hull1d::add`] needs no
/// branch; [`Hull1d::finish`] is `None` until a value other than NaN was added. A NaN value
/// is passed over (`f64::min` and `f64::max`), as upstream's comparisons pass it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hull1d {
    min: f64,
    max: f64,
}

impl Default for Hull1d {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl Hull1d {
    /// Nothing added.
    pub const EMPTY: Self = Self {
        min: f64::INFINITY,
        max: f64::NEG_INFINITY,
    };

    /// Grows the hull to cover `v` (`AddValue`).
    #[inline]
    pub fn add(&mut self, v: f64) {
        self.min = self.min.min(v);
        self.max = self.max.max(v);
    }

    /// Grows the hull to cover the interval `e`.
    #[inline]
    pub fn add_extent(&mut self, e: Extent1d) {
        self.min = self.min.min(e.min);
        self.max = self.max.max(e.max);
    }

    /// The hull of both.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    /// The interval covered, `None` when nothing was added.
    #[must_use]
    pub fn finish(self) -> Option<Extent1d> {
        Extent1d::from_bounds(self.min, self.max)
    }
}

impl FromIterator<f64> for Hull1d {
    fn from_iter<I: IntoIterator<Item = f64>>(values: I) -> Self {
        let mut h = Self::EMPTY;
        for v in values {
            h.add(v);
        }
        h
    }
}

impl Extent1d {
    /// `[-INFINITE_PARAMETER, INFINITE_PARAMETER]` (`SetUnbounded`).
    pub const UNBOUNDED: Self = Self {
        min: -INFINITE_PARAMETER,
        max: INFINITE_PARAMETER,
    };

    /// The degenerate interval `[v, v]` (`SmExtent1d(double)`).
    #[must_use]
    pub const fn point(v: f64) -> Self {
        Self { min: v, max: v }
    }

    /// The interval spanned by `a` and `b`, swapped when `a > b`
    /// (`SmExtent1d(double, double)`).
    #[must_use]
    pub fn spanning(a: f64, b: f64) -> Self {
        if a > b {
            Self { min: b, max: a }
        } else {
            Self { min: a, max: b }
        }
    }

    /// `[min, max]` for ends the caller holds ordered: a box's interval, whose invariant
    /// is this one's.
    pub(crate) const fn from_ordered(min: f64, max: f64) -> Self {
        Self { min, max }
    }

    /// The interval `[min, max]`, `None` when `min > max` (`SetMinMax`, which refuses it).
    #[must_use]
    pub fn from_bounds(min: f64, max: f64) -> Option<Self> {
        (min.partial_cmp(&max) != Some(core::cmp::Ordering::Greater)).then_some(Self { min, max })
    }

    /// The smallest interval covering `values`, `None` for none (a [`Hull1d`] fold).
    #[must_use]
    pub fn hull<I: IntoIterator<Item = f64>>(values: I) -> Option<Self> {
        values.into_iter().collect::<Hull1d>().finish()
    }

    /// This interval with its lower end `min`, `None` when that is above the upper end
    /// (`SetMin`).
    #[must_use]
    pub fn with_min(self, min: f64) -> Option<Self> {
        Self::from_bounds(min, self.max)
    }

    /// This interval with its upper end `max`, `None` when that is below the lower end
    /// (`SetMax`).
    #[must_use]
    pub fn with_max(self, max: f64) -> Option<Self> {
        Self::from_bounds(self.min, max)
    }

    /// Mirrored about zero: `[-max, -min]` (`Negate`).
    #[must_use]
    pub fn negated(self) -> Self {
        Self {
            min: -self.max,
            max: -self.min,
        }
    }

    /// Both ends shifted by `v`, unbounded markers left in place (`Translate`).
    #[must_use]
    pub fn translated(self, v: f64) -> Self {
        let shift = |e: f64| if is_infinite_param(e) { e } else { e + v };
        Self {
            min: shift(self.min),
            max: shift(self.max),
        }
    }

    /// Lower end (`GetMin`).
    #[must_use]
    pub const fn min(&self) -> f64 {
        self.min
    }

    /// Upper end (`GetMax`).
    #[must_use]
    pub const fn max(&self) -> f64 {
        self.max
    }

    /// Centre, `(max + min) / 2` (`GetMid`).
    #[must_use]
    pub fn mid(&self) -> f64 {
        (self.max + self.min) / 2.0
    }

    /// `max - min` (`GetLength`).
    #[must_use]
    pub fn length(&self) -> f64 {
        self.max - self.min
    }

    /// The larger absolute end value (`GetMaxDimension`).
    #[must_use]
    pub fn max_dimension(&self) -> f64 {
        self.max.abs().max(self.min.abs())
    }

    /// True when `t` is in the lower half of the interval (`t <= mid`): evaluate
    /// "from the left" at a boundary between intervals (`GetTLeftEval`).
    #[must_use]
    pub fn t_left_eval(&self, t: f64) -> bool {
        let mid = (self.min + self.max) / 2.0;
        t <= mid
    }

    /// `v` clamped to the interval (`ClampValue`).
    #[must_use]
    pub fn clamp_value(&self, v: f64) -> f64 {
        if v < self.min {
            self.min
        } else if v > self.max {
            self.max
        } else {
            v
        }
    }

    /// `v` clamped to the interval, snapping values within `tol` of an end to that
    /// end (`SnapValue`; upstream default `tol` is `EFF_ZERO`).
    #[must_use]
    pub fn snap_value(&self, v: f64, tol: f64) -> f64 {
        if v < self.min + tol {
            self.min
        } else if v > self.max - tol {
            self.max
        } else {
            v
        }
    }

    /// The value at normalised parameter `t`: `0` gives `min`, `1` gives `max`
    /// (exactly), other values interpolate or extrapolate (`Evaluate`).
    #[must_use]
    pub fn evaluate(&self, t: f64) -> f64 {
        if t == 0.0 {
            return self.min;
        }
        if t == 1.0 {
            return self.max;
        }
        self.min + t * (self.max - self.min)
    }

    /// The smallest interval containing both (`Union`).
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    /// Distance from `v` to the interval, `0` inside (`DistanceFrom`).
    #[must_use]
    pub fn distance_from(&self, v: f64) -> f64 {
        if v < self.min {
            return self.min - v;
        }
        if v > self.max {
            return v - self.max;
        }
        0.0
    }

    /// The overlap of two intervals, `None` when they are disjoint (`Intersect`, which
    /// then fails and sets the result to `Init()`).
    #[must_use]
    pub fn intersect(self, other: Self) -> Option<Self> {
        if self.are_disjoint(&other, 0.0) {
            return None;
        }
        Some(Self {
            min: self.min.max(other.min),
            max: self.max.min(other.max),
        })
    }

    /// Both bounded ends moved by `d`, outwards when `d > 0` and inwards when `d < 0`
    /// (`ExpandAbsolute`), `None` when moving inwards inverts the interval.
    fn moved_out(self, d: f64) -> Option<Self> {
        let min = if is_infinite_param(self.min) {
            self.min
        } else {
            self.min - d
        };
        let max = if is_infinite_param(self.max) {
            self.max
        } else {
            self.max + d
        };
        Self::from_bounds(min, max)
    }

    /// Both bounded ends moved outwards by `d` (`ExpandAbsolute` with `d >= 0`). A `d`
    /// below zero or NaN leaves the interval as it is: shrinking is [`Extent1d::shrunk`].
    #[must_use]
    pub fn expanded(self, d: f64) -> Self {
        if d > 0.0 {
            self.moved_out(d).unwrap_or(self)
        } else {
            self
        }
    }

    /// Both bounded ends moved inwards by `d` (`ExpandAbsolute(-d)`), `None` when that
    /// inverts the interval.
    #[must_use]
    pub fn shrunk(self, d: f64) -> Option<Self> {
        self.moved_out(-d)
    }

    /// Both bounded ends moved outwards by `factor * length` (`ExpandRelative` with
    /// `factor >= 0`). A `factor` below zero or NaN leaves the interval as it is.
    #[must_use]
    pub fn expanded_relative(self, factor: f64) -> Self {
        if factor > 0.0 {
            self.moved_out((self.max - self.min) * factor)
                .unwrap_or(self)
        } else {
            self
        }
    }

    /// Both bounded ends multiplied by `s` (any sign), re-ordered if needed (`Scale`).
    #[must_use]
    pub fn scaled(self, s: f64) -> Self {
        let times = |e: f64| if is_infinite_param(e) { e } else { e * s };
        Self::spanning(times(self.min), times(self.max))
    }

    /// The normalised parameter of `v` (inverse of [`Extent1d::evaluate`], not
    /// clamped) (`Inversion`).
    #[must_use]
    pub fn inversion(&self, v: f64) -> Inversion {
        let contained = self.contains_value(v, 0.0);
        let numer = v - self.min;
        let denom = self.max - self.min;
        let parameter = if denom < numer.abs() * EFF_ZERO {
            None
        } else {
            Some(numer / denom)
        };
        Inversion {
            parameter,
            contained,
        }
    }

    /// True when the intervals are separated by more than `-tol` (touching intervals
    /// are not disjoint for `tol = 0`) (`AreDisjoint`).
    #[must_use]
    pub fn are_disjoint(&self, other: &Self, tol: f64) -> bool {
        (self.min > other.max - tol) || (self.max < other.min + tol)
    }

    /// True when both ends agree to within `|tol|` (`AreEqual(const SmExtent1d&)`).
    #[must_use]
    pub fn are_equal(&self, other: &Self, tol: f64) -> bool {
        self.are_equal_bounds(other.min, other.max, tol)
    }

    /// True when the ends agree with `min`, `max` to within `|tol|`
    /// (`AreEqual(double, double, double)`).
    #[must_use]
    pub fn are_equal_bounds(&self, min: f64, max: f64, tol: f64) -> bool {
        are_same_to_tol(self.min, min, tol) && are_same_to_tol(self.max, max, tol)
    }

    /// True when `self` lies inside `other` grown by `tol` (`IsContainedBy`).
    #[must_use]
    pub fn is_contained_by(&self, other: &Self, tol: f64) -> bool {
        other.min - tol <= self.min && other.max + tol >= self.max
    }

    /// True when `min - tol <= v <= max + tol` (`ContainsValue`).
    #[must_use]
    pub fn contains_value(&self, v: f64, tol: f64) -> bool {
        v >= self.min - tol && v <= self.max + tol
    }

    /// Whether `v` is within `tol` (strictly) of an end, and the *other* end
    /// (`IsValueOnBoundary`; upstream default `tol` is `EFF_ZERO`; the second value is
    /// its optional out-param, meaningful only when the first is true).
    #[must_use]
    pub fn is_value_on_boundary(&self, v: f64, tol: f64) -> (bool, f64) {
        let del_min = (v - self.min).abs();
        let del_max = (v - self.max).abs();
        let on_min = del_min < tol;
        let on_max = del_max < tol;
        let other = if on_min && on_max {
            if del_min < del_max {
                self.max
            } else {
                self.min
            }
        } else if on_min {
            self.max
        } else {
            self.min
        };
        (on_min || on_max, other)
    }

    /// Classifies `v` as on an end, inside or outside (`ClassifyPoint`; upstream
    /// default `tol` is `EFF_ZERO`).
    #[must_use]
    pub fn classify_point(&self, v: f64, tol: f64) -> ExtentPointType {
        if (v - self.min).abs() <= tol && (v - self.max).abs() <= tol {
            ExtentPointType::Both
        } else if (v - self.min).abs() <= tol {
            ExtentPointType::Start
        } else if (v - self.max).abs() <= tol {
            ExtentPointType::End
        } else if v - self.min >= 0.0 && self.max - v >= 0.0 {
            ExtentPointType::Inside
        } else {
            ExtentPointType::Outside
        }
    }

    /// True when the length is within `tol` of zero (`IsDegenerate`).
    #[must_use]
    pub fn is_degenerate(&self, tol: f64) -> bool {
        (self.min >= self.max - tol) && (self.min <= self.max + tol)
    }

    /// Classifies the ends as bounded or unbounded (`IsBounded`'s out-param).
    #[must_use]
    pub fn boundary_type(&self) -> BoundaryType {
        if is_infinite_param(self.max) && is_infinite_param(self.min) {
            BoundaryType::Unbounded
        } else if is_infinite_param(self.min) {
            BoundaryType::UnboundedMin
        } else if is_infinite_param(self.max) {
            BoundaryType::UnboundedMax
        } else {
            BoundaryType::Bounded
        }
    }

    /// True when neither end is an unbounded marker (`IsBounded`).
    #[must_use]
    pub fn is_bounded(&self) -> bool {
        self.boundary_type() == BoundaryType::Bounded
    }

    /// The approximated ends of `ApproximateUnbounded`, before the final swap.
    fn approx_ends(&self, center: f64, half: f64) -> (BoundaryType, f64, f64) {
        let ty = self.boundary_type();
        let mut min = self.min;
        let mut max = self.max;
        if ty != BoundaryType::Bounded {
            min = match ty {
                BoundaryType::UnboundedMin => {
                    if max < center + half {
                        max - 2.0 * half
                    } else {
                        center - half
                    }
                }
                BoundaryType::UnboundedMax => {
                    if min > center - half {
                        min
                    } else {
                        center - half
                    }
                }
                BoundaryType::Unbounded => center - half,
                BoundaryType::Bounded | BoundaryType::Unknown => min,
            };
            max = match ty {
                BoundaryType::UnboundedMin => {
                    if max < center + half {
                        max
                    } else {
                        center + half
                    }
                }
                BoundaryType::UnboundedMax => {
                    if min > center - half {
                        min + 2.0 * half
                    } else {
                        center + half
                    }
                }
                BoundaryType::Unbounded => center + half,
                BoundaryType::Bounded | BoundaryType::Unknown => max,
            };
        }
        (ty, min, max)
    }

    /// A bounded interval approximating this one: unbounded ends are replaced
    /// using `center` and `half` (upstream defaults `0` and
    /// `BOUNDED_INFINITE_PARAM`); a bounded interval is returned unchanged
    /// (`ApproximateUnbounded` without the expanded extent).
    #[must_use]
    pub fn approximate_unbounded(&self, center: f64, half: f64) -> Self {
        let (_, min, max) = self.approx_ends(center, half);
        Self::spanning(min, max)
    }

    /// [`Extent1d::approximate_unbounded`] plus the extent expanded by `half / 12`
    /// at each replaced end (`ApproximateUnbounded` with `pOptExpandedApprox`);
    /// `None` where upstream's `SetMinMax` rejects it (min > max).
    #[must_use]
    pub fn approximate_unbounded_expanded(&self, center: f64, half: f64) -> (Self, Option<Self>) {
        let (ty, min, max) = self.approx_ends(center, half);
        let mut min_inc = min;
        let mut max_inc = max;
        let inf_inc = half / 12.0;
        if ty != BoundaryType::Bounded {
            if ty == BoundaryType::UnboundedMin || ty == BoundaryType::Unbounded {
                min_inc -= inf_inc;
            }
            if ty == BoundaryType::UnboundedMax || ty == BoundaryType::Unbounded {
                max_inc += inf_inc;
            }
        }
        (
            Self::spanning(min, max),
            Self::from_bounds(min_inc, max_inc),
        )
    }

    /// Both ends equal up to the scaled zero (`operator==`).
    #[must_use]
    pub fn are_same(&self, other: &Self) -> bool {
        are_same(self.min, other.min) && are_same(self.max, other.max)
    }

    /// True when, in a periodic space of length `period`, neither interval contains
    /// an end of the other (`AreDisjointPeriodic`; upstream default `tol` is `-1`,
    /// meaning `EFF_ZERO_DEG`).
    #[must_use]
    pub fn are_disjoint_periodic(&self, other: &Self, period: f64, tol: f64) -> bool {
        !self
            .contains_periodic_value(other.min, period, tol)
            .contained
            && !self
                .contains_periodic_value(other.max, period, tol)
                .contained
            && !other
                .contains_periodic_value(self.min, period, tol)
                .contained
            && !other
                .contains_periodic_value(self.max, period, tol)
                .contained
    }

    /// `v` wrapped into the interval, treating its length as the period; values
    /// within `EFF_ZERO` of an end snap to it. Returns `v` for a (near) zero-length
    /// interval or when `v` is too large to wrap (`PeriodicWrap`).
    #[must_use]
    pub fn periodic_wrap(&self, v: f64) -> f64 {
        let period = self.max - self.min;
        if period <= EFF_ZERO {
            return v;
        }
        let mut v = v;
        if (v - self.min).abs() < EFF_ZERO {
            v = self.min;
        }
        if (v - self.max).abs() < EFF_ZERO {
            v = self.max;
        }
        if v + period == v {
            return v;
        }
        // The whole periods to the interval in one step: a value below lands in
        // `[min, max)`, one above in `(min, max]`, as upstream's one-period-a-time loop
        // leaves them, which never ended for a value many periods out.
        // The count is the fewest periods that reach the interval to within `EFF_ZERO`, where
        // the loop stopped: the quotient's rounding can make its ceiling one too many.
        if v < self.min {
            let mut k = ((self.min - v) / period).ceil();
            if k > 1.0 && v + (k - 1.0) * period >= self.min - EFF_ZERO {
                k -= 1.0;
            }
            v += k * period;
        } else if v > self.max {
            let mut k = ((v - self.max) / period).ceil();
            if k > 1.0 && v - (k - 1.0) * period <= self.max + EFF_ZERO {
                k -= 1.0;
            }
            v -= k * period;
        }
        // The product's rounding can leave `v` just past an end: one period more at most.
        if v < self.min - EFF_ZERO {
            v += period;
        } else if v > self.max + EFF_ZERO {
            v -= period;
        }
        if (v - self.min).abs() < EFF_ZERO {
            v = self.min;
        }
        if (v - self.max).abs() < EFF_ZERO {
            v = self.max;
        }
        v
    }

    /// The intersection of two intervals of the same periodic space of length
    /// `period` (`0`: not periodic, as [`Extent1d::intersect`]): 0, 1 or 2 intervals,
    /// contiguous where possible. With [`AtSeam::Split`], results are split at this
    /// interval's boundaries (`IntersectPeriodic`; upstream default [`AtSeam::Join`]).
    ///
    /// Fails (`SM_ERR`) only for `period == 0` and disjoint intervals (upstream's
    /// result is then one `Init()` interval).
    pub fn intersect_periodic(
        &self,
        other: &Self,
        period: f64,
        at_seam: AtSeam,
    ) -> Result<Vec<Self>> {
        let dont_cross = at_seam == AtSeam::Split;
        let mut out = Vec::new();
        if period == 0.0 {
            out.push(self.intersect(*other).ok_or_else(|| {
                NumError::failed(
                    "geom::extent1d::Extent1d::intersect_periodic",
                    crate::error::NumFailure::Disjoint {
                        a: [self.min, self.max],
                        b: [other.min, other.max],
                    },
                )
            })?);
            return Ok(out);
        }

        let ivl = Self::spanning(self.min, self.min + period);
        let this_min = self.min;
        let this_max = self.max;
        let mut other_min = ivl.periodic_wrap(other.min);
        let mut other_max = ivl.periodic_wrap(other.max);

        if other_min < other_max - EFF_ZERO {
            if (this_min <= other_max + EFF_ZERO) && (this_max >= other_min - EFF_ZERO) {
                let min = this_min.max(other_min);
                let mut max = this_max.min(other_max);
                if are_same(min, max) {
                    max = min;
                }
                out.push(Self::spanning(min, max));
            }
            if !self.is_closed(period)
                && !other.is_closed(period)
                && are_same(other_max, this_min + period)
            {
                out.push(Self::spanning(this_min, this_min));
            }
        } else if other_min > other_max + EFF_ZERO {
            let this_closed = self.is_closed(period);
            if this_closed && !dont_cross {
                out.push(*other);
            } else {
                if this_min <= other_max + EFF_ZERO {
                    if this_min > other_max {
                        other_max = this_min;
                    }
                    if !(this_closed && are_same(this_min, other_max)) {
                        out.push(Self::spanning(this_min, this_max.min(other_max)));
                    }
                }
                if this_max >= other_min - EFF_ZERO {
                    if this_max < other_min {
                        other_min = this_max;
                    }
                    if !(this_closed && are_same(this_max, other_min)) {
                        out.push(Self::spanning(this_min.max(other_min), this_max));
                    }
                }
            }
        } else if other.is_closed(period) {
            if dont_cross {
                if self.contains_value(other_min, -EFF_ZERO) {
                    out.push(Self::spanning(this_min, other_max));
                    out.push(Self::spanning(other_min, this_max));
                } else {
                    out.push(*self);
                }
            } else {
                out.push(*self);
            }
        } else if other_min <= this_max {
            out.push(Self::spanning(other_min, other_min));
        }
        Ok(out)
    }

    /// The periodic value of `v` inside the interval, or the nearer end when it is
    /// outside (`ClampPeriodicValue`; upstream default `tol` is `-1`).
    #[must_use]
    pub fn clamp_periodic_value(&self, v: f64, period: f64, tol: f64) -> f64 {
        let c = self.contains_periodic_value(v, period, tol);
        if c.contained {
            c.periodic_value
        } else if c.dist_to_min < c.dist_to_max {
            self.min
        } else {
            self.max
        }
    }

    /// Whether `v`, accounting for a period of length `period` (`0`: not periodic),
    /// lies in the interval to within `tol` (`tol <= 0`: `EFF_ZERO_DEG`), with the
    /// periodic value and its distances to the ends (`ContainsPeriodicValue`).
    #[must_use]
    pub fn contains_periodic_value(&self, v: f64, period: f64, tol: f64) -> PeriodicContainment {
        let zero = if tol > 0.0 { tol } else { EFF_ZERO_DEG };
        let mut contained = self.contains_value(v, zero);
        if contained || period == 0.0 {
            return PeriodicContainment {
                contained,
                periodic_value: v,
                dist_to_min: (v - self.min).abs(),
                dist_to_max: (v - self.max).abs(),
            };
        }

        let ivl = Self::spanning(self.min, self.min + period);
        let mut map = v;
        if !ivl.contains_value(v, zero) {
            map = ivl.periodic_wrap(v);
        }

        contained = map > self.min - zero && map < self.max + zero;

        let mut kept_down = false;
        if !contained && are_same_to_tol(map, ivl.max(), zero) {
            // a period down, kept where it lies in the interval; the C++ sets it to the
            // start instead (WR-509, #1518; tangency-labs/smlib#46)
            let down = map - period;
            if down > self.min - zero && down < self.max + zero {
                map = down;
                contained = true;
                kept_down = true;
            }
        }

        let periodic_value =
            if contained && ((v - self.min).abs() < zero || (v - self.max).abs() < zero) {
                v
            } else if contained && map > self.max + zero {
                map - period
            } else {
                map
            };
        let mut dist_to_min = map - self.min;
        if kept_down {
            // it can lie up to the tolerance below the start: its distance from it, not a
            // signed offset (assert_valid takes the smaller of the two). Only this new case: other
            // inputs keep the signed value upstream gives them.
            dist_to_min = dist_to_min.abs();
        }
        if dist_to_min > period / 2.0 {
            dist_to_min = period - dist_to_min;
        }
        let mut dist_to_max = (self.max - map).abs();
        if dist_to_max > period / 2.0 {
            dist_to_max = period - dist_to_max;
        }
        PeriodicContainment {
            contained,
            periodic_value,
            dist_to_min,
            dist_to_max,
        }
    }

    /// True when the interval covers a whole period (`IsClosed`).
    #[must_use]
    pub fn is_closed(&self, period: f64) -> bool {
        period != 0.0 && are_same(self.max - self.min, period)
    }

    /// Whether `v` is within `tol` (strictly) of an end, accounting for periodicity,
    /// and its distance to the nearer end (`IsValueOnPeriodicBoundary`; upstream
    /// default `tol` is `EFF_ZERO`).
    #[must_use]
    pub fn is_value_on_periodic_boundary(&self, v: f64, period: f64, tol: f64) -> (bool, f64) {
        let c = self.contains_periodic_value(v, period, tol);
        let on = c.dist_to_min < tol || c.dist_to_max < tol;
        (on, c.dist_to_min.min(c.dist_to_max))
    }
}

/// An N-dimensional box of intervals (`SmExtentNd`).
#[derive(Debug, Clone, PartialEq)]
pub struct ExtentNd {
    extents: Vec<Extent1d>,
}

impl ExtentNd {
    /// The box of `extents`, one interval per dimension (`SmExtentNd(ULONG)`, then each
    /// interval set).
    #[must_use]
    pub const fn new(extents: Vec<Extent1d>) -> Self {
        Self { extents }
    }

    /// Number of dimensions.
    #[must_use]
    pub fn dimension(&self) -> usize {
        self.extents.len()
    }

    /// Interval `i` (`operator[] const`).
    #[must_use]
    pub fn get(&self, i: usize) -> Option<Extent1d> {
        self.extents.get(i).copied()
    }

    /// Mutable interval `i` (`operator[]`).
    pub fn get_mut(&mut self, i: usize) -> Option<&mut Extent1d> {
        self.extents.get_mut(i)
    }

    /// Clamps (or, where `periodic[i]`, wraps) each `values[i]` to interval `i`;
    /// unbounded intervals are skipped (`ClampVector`).
    ///
    /// As upstream, `clamped` and `counts` are resized to the dimension keeping their
    /// existing prefix and zero-filling; `clamped[i]` is written only for bounded
    /// intervals, and `counts[i]` is incremented when a non-periodic value moved by
    /// more than `EFF_ZERO`. Returns true when at least one value was clamped.
    /// Fails when `values` or `periodic` is shorter than the dimension.
    pub fn clamp_vector(
        &self,
        values: &[f64],
        periodic: Option<&[bool]>,
        clamped: &mut Vec<f64>,
        mut counts: Option<&mut Vec<u64>>,
    ) -> Result<bool> {
        let dim = self.extents.len();
        if values.len() < dim || periodic.is_some_and(|p| p.len() < dim) {
            return Err(NumError::InvalidInput {
                operation: "geom::extent1d::ExtentNd::clamp_vector",
                reason: Invalid::SizeMismatch,
            });
        }
        let mut rtn = false;
        clamped.resize(dim, 0.0);
        if let Some(c) = counts.as_deref_mut() {
            c.resize(dim, 0);
        }
        for (i, (ext, &value)) in self.extents.iter().zip(values).enumerate() {
            if !ext.is_bounded() {
                continue;
            }
            let is_periodic = periodic.and_then(|p| p.get(i)).copied().unwrap_or(false);
            let Some(slot) = clamped.get_mut(i) else {
                continue;
            };
            if is_periodic {
                *slot = ext.periodic_wrap(value);
            } else if let Some(c) = counts.as_deref_mut() {
                if ext.contains_value(value, 0.0) {
                    *slot = value;
                } else {
                    let tmp = ext.clamp_value(value);
                    *slot = tmp;
                    if (tmp - value).abs() > EFF_ZERO {
                        if let Some(n) = c.get_mut(i) {
                            *n = n.wrapping_add(1);
                        }
                        rtn = true;
                    }
                }
            } else {
                let tmp = ext.clamp_value(value);
                if (tmp - value).abs() > EFF_ZERO {
                    rtn = true;
                }
                *slot = tmp;
            }
        }
        Ok(rtn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::BOUNDED_INFINITE_PARAM;

    #[test]
    fn construct_and_access() {
        let e = Extent1d::spanning(3.0, 1.0);
        assert_eq!((e.min(), e.max()), (1.0, 3.0));
        assert_eq!(e.mid(), 2.0);
        assert_eq!(e.length(), 2.0);
        assert_eq!(Extent1d::spanning(-5.0, 1.0).max_dimension(), 5.0);
        assert_eq!(Hull1d::EMPTY.finish(), None);
        assert_eq!(Extent1d::from_bounds(1.0, 0.0), None);
        assert_eq!(
            Extent1d::from_bounds(0.0, 1.0),
            Some(Extent1d::spanning(1.0, 0.0))
        );
        assert!(e.t_left_eval(2.0));
        assert!(!e.t_left_eval(2.5));
    }

    #[test]
    fn add_clamp_snap_evaluate() {
        let e = Extent1d::hull([2.0, f64::NAN, -1.0]).unwrap();
        assert_eq!((e.min(), e.max()), (-1.0, 2.0));
        assert_eq!(e.clamp_value(5.0), 2.0);
        assert_eq!(e.clamp_value(-5.0), -1.0);
        assert_eq!(e.snap_value(1.9, 0.2), 2.0);
        assert_eq!(e.snap_value(0.5, 0.2), 0.5);
        assert_eq!(e.evaluate(0.0), -1.0);
        assert_eq!(e.evaluate(1.0), 2.0);
        assert_eq!(e.evaluate(0.5), 0.5);
        let inv = e.inversion(0.5);
        assert!(inv.is_success());
        assert_eq!(inv.parameter, Some(0.5));
        let out = e.inversion(5.0);
        assert!(!out.is_success());
        assert_eq!(out.parameter, Some(2.0));
        assert_eq!(Extent1d::point(1.0).inversion(2.0).parameter, None);
    }

    #[test]
    fn setters() {
        let e = Extent1d::spanning(0.0, 1.0);
        assert_eq!(e.with_min(2.0), None);
        assert_eq!(e.with_max(-1.0), None);
        let e = e.with_min(0.5).and_then(|e| e.with_max(0.5));
        assert_eq!(e, Some(Extent1d::point(0.5)));
    }

    #[test]
    fn set_operations() {
        let a = Extent1d::spanning(0.0, 2.0);
        let b = Extent1d::spanning(1.0, 3.0);
        assert_eq!(a.union(b), Extent1d::spanning(0.0, 3.0));
        assert_eq!(a.intersect(b), Some(Extent1d::spanning(1.0, 2.0)));
        let c = Extent1d::spanning(5.0, 6.0);
        assert_eq!(a.intersect(c), None);
        let disjoint = a.intersect_periodic(&c, 0.0, AtSeam::Join).unwrap_err();
        assert_eq!(
            disjoint.failure(),
            Some(&crate::error::NumFailure::Disjoint {
                a: [0.0, 2.0],
                b: [5.0, 6.0]
            })
        );
        // status SM_ERR (1001): every `NumError::Failed` has it
        assert!(matches!(disjoint, NumError::Failed { .. }));
        assert!(a.are_disjoint(&c, 0.0));
        assert!(!a.are_disjoint(&Extent1d::spanning(2.0, 3.0), 0.0));
        assert_eq!(a.distance_from(-1.0), 1.0);
        assert_eq!(a.distance_from(1.0), 0.0);
        assert!(Extent1d::spanning(0.5, 1.0).is_contained_by(&a, 0.0));
        assert!(a.are_equal(&Extent1d::spanning(0.1, 2.0), -0.2));
        assert!(a.are_same(&Extent1d::spanning(1e-13, 2.0)));
    }

    #[test]
    fn expand_scale_negate_translate() {
        let e = Extent1d::spanning(1.0, 3.0).expanded(1.0);
        assert_eq!((e.min(), e.max()), (0.0, 4.0));
        assert_eq!(e.expanded(-1.0), e);
        assert_eq!(e.shrunk(1.5), Some(Extent1d::spanning(1.5, 2.5)));
        assert_eq!(e.shrunk(2.5), None);
        let e = e.expanded_relative(0.5);
        assert_eq!((e.min(), e.max()), (-2.0, 6.0));
        let e = e.scaled(-1.0);
        assert_eq!((e.min(), e.max()), (-6.0, 2.0));
        let e = e.negated();
        assert_eq!((e.min(), e.max()), (-2.0, 6.0));
        let u = Extent1d::UNBOUNDED.translated(5.0);
        assert_eq!(u.min(), -INFINITE_PARAMETER);
        assert!(!u.is_bounded());
        assert_eq!(u.boundary_type(), BoundaryType::Unbounded);
    }

    #[test]
    fn classify_and_boundary() {
        let e = Extent1d::spanning(0.0, 1.0);
        assert_eq!(e.classify_point(0.0, EFF_ZERO), ExtentPointType::Start);
        assert_eq!(e.classify_point(1.0, EFF_ZERO), ExtentPointType::End);
        assert_eq!(e.classify_point(0.5, EFF_ZERO), ExtentPointType::Inside);
        assert_eq!(e.classify_point(2.0, EFF_ZERO), ExtentPointType::Outside);
        assert_eq!(
            Extent1d::point(0.0).classify_point(0.0, EFF_ZERO),
            ExtentPointType::Both
        );
        assert_eq!(e.is_value_on_boundary(1.0, EFF_ZERO), (true, 0.0));
        assert!(Extent1d::point(1.0).is_degenerate(0.0));
    }

    #[test]
    fn approximate() {
        let e = Extent1d::UNBOUNDED.with_max(5.0).unwrap();
        let a = e.approximate_unbounded(0.0, BOUNDED_INFINITE_PARAM);
        assert_eq!((a.min(), a.max()), (5.0 - 66.0, 5.0));
        let e = Extent1d::UNBOUNDED;
        let (a, x) = e.approximate_unbounded_expanded(0.0, 12.0);
        assert_eq!((a.min(), a.max()), (-12.0, 12.0));
        assert_eq!(x, Some(Extent1d::spanning(-13.0, 13.0)));
        let b = Extent1d::spanning(1.0, 2.0);
        assert_eq!(b.approximate_unbounded(0.0, 33.0), b);
    }

    #[test]
    fn periodic() {
        let e = Extent1d::spanning(0.0, 2.0);
        assert_eq!(e.periodic_wrap(4.5), 0.5);
        assert_eq!(e.periodic_wrap(-1.0), 1.0);
        // Whole periods out: below lands on `min`, above on `max`.
        assert_eq!(e.periodic_wrap(-4.0), 0.0);
        assert_eq!(e.periodic_wrap(6.0), 2.0);
        // One period out by a quotient that rounds above 1 still takes one step.
        let e = Extent1d::spanning(-1.0, 0.945_266_454_097_540_8);
        assert_eq!(e.periodic_wrap(-2.945_266_454_097_541), -1.0);
        // Many periods out wraps in one step (the period-at-a-time loop never ended).
        let short = Extent1d::spanning(0.0, 1e-6);
        for v in [1e9, -1e9, 12_345.678_9] {
            let w = short.periodic_wrap(v);
            assert!((0.0..=1e-6).contains(&w), "{v} wrapped to {w}");
        }
        let full = Extent1d::spanning(0.0, 360.0);
        let r = full
            .intersect_periodic(&Extent1d::spanning(-90.0, 90.0), 360.0, AtSeam::Join)
            .unwrap_or_default();
        assert_eq!(r, vec![Extent1d::spanning(-90.0, 90.0)]);
        let r = full
            .intersect_periodic(&Extent1d::spanning(-90.0, 90.0), 360.0, AtSeam::Split)
            .unwrap_or_default();
        assert_eq!(
            r,
            vec![
                Extent1d::spanning(0.0, 90.0),
                Extent1d::spanning(270.0, 360.0)
            ]
        );
        let q = Extent1d::spanning(0.0, 90.0);
        let c = q.contains_periodic_value(370.0, 360.0, -1.0);
        assert!(c.contained);
        assert_eq!(c.periodic_value, 10.0);
        assert_eq!(q.clamp_periodic_value(200.0, 360.0, -1.0), 90.0);
        assert!(full.is_closed(360.0));
        assert!(q.is_value_on_periodic_boundary(360.0, 360.0, EFF_ZERO).0);
        assert!(q.are_disjoint_periodic(&Extent1d::spanning(100.0, 200.0), 360.0, -1.0));
        assert!(q
            .intersect_periodic(&Extent1d::spanning(100.0, 200.0), 0.0, AtSeam::Join)
            .is_err());
    }

    #[test]
    fn issue_1518_value_past_start_moves_down_a_period() {
        // An interval shorter than the tolerance (1e-4 degrees), and a value 3e-5 past
        // its start given a period up: the C++ returns the start (-10).
        let q = Extent1d::spanning(-10.0, -10.0 + 7.0e-5);
        let v = 350.0 + 3.0e-5;
        let c = q.contains_periodic_value(v, 360.0, -1.0);
        assert!(c.contained);
        assert_eq!(c.periodic_value, v - 360.0);
        assert!((c.dist_to_min - 3.0e-5).abs() < 1e-12);
        assert_eq!(q.clamp_periodic_value(v, 360.0, -1.0), v - 360.0);

        // a value within the tolerance below the start keeps its place a period down
        // (the C++: the start, 0)
        let r = Extent1d::spanning(0.0, 90.0);
        let w = 360.0 - 5.0e-5;
        let c = r.contains_periodic_value(w, 360.0, -1.0);
        assert!(c.contained);
        assert_eq!(c.periodic_value, w - 360.0);
        // its distance to the start is a distance, not the signed offset -5e-5
        assert!((c.dist_to_min - 5.0e-5).abs() < 1e-12, "{}", c.dist_to_min);
    }

    #[test]
    fn clamp_vector() {
        let nd = ExtentNd::new(vec![
            Extent1d::spanning(0.0, 1.0),
            Extent1d::spanning(0.0, 2.0),
            Extent1d::UNBOUNDED,
        ]);
        let mut out = vec![9.0];
        let mut counts = vec![5];
        let r = nd.clamp_vector(
            &[2.0, 3.0, 7.0],
            Some(&[false, true, false]),
            &mut out,
            Some(&mut counts),
        );
        assert_eq!(r.ok(), Some(true));
        assert_eq!(out, vec![1.0, 1.0, 0.0]);
        assert_eq!(counts, vec![6, 0, 0]);
        assert!(nd.clamp_vector(&[1.0], None, &mut out, None).is_err());
        assert_eq!(nd.get(5), None);
    }
}
