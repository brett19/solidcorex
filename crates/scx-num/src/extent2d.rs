// Moved from solidcore `solidcore/crates/solidcore/src/geom/extent2d.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/src/SmExtent2d.cpp`,
// `source/SMLib/inc/SmExtent2d.h`.
// Changes: the digest impl `Content::content` stays in solidcore; `min`/`max` fields private;
// errors are `NumError`; the test `inversion_of_degenerate_side` checks the error variant in
// place of the status number; paths.
// Then the inside-out empty extent went: an `Extent2d` is never inverted, and a fold is a
// `Hull2d`. `new`, `Default`, `init`, `is_init`, `has_negative_area`, `add_point`, the
// `set_*` setters and the mutators went: `from_bounds` and `from_corners` answer `Option`, the
// swapping constructor is `spanning`, an axis is replaced by `with_x_interval`/`with_y_interval`,
// and the transforms take and return the value (`scale(s, Option<f64>)` is `scaled` and
// `scaled_xy`, which reorder a negated axis where upstream left it inverted). u/v are x/y.
// `intersect` answers `Option`; `switch_params: bool` is `ParamOrder`.
// Then the ends became finite by construction (research 63 P0, D1): every constructor and
// transform refuses a non-finite end (`Option`), and `boundary_types`/`is_bounded` went.
//! Axis-aligned 2D extents (`[min, max]` rectangles, usually a surface's `(u, v)`
//! parameter domain): construction, growth, set operations, containment and boundary
//! classification, periodic containment, normalised evaluation/inversion, line
//! clipping and domain-to-domain mapping.
//!
//! Upstream: `source/SMLib/src/SmExtent2d.cpp`, `source/SMLib/inc/SmExtent2d.h`.
//!
//! | Rust                                   | Upstream                                       |
//! |----------------------------------------|------------------------------------------------|
//! | [`Extent2d`]                           | `SmExtent2d`                                   |
//! | [`Hull2d`]                             | `SmExtent2d()`, `Init`, `AddPoint2d`           |
//! | [`Extent2d::from_point`]               | `SmExtent2d(const SmPoint2d&)`                 |
//! | [`Extent2d::spanning`]                 | `SmExtent2d(const SmPoint2d&, const SmPoint2d&)` |
//! | [`Extent2d::from_bounds`]              | `SmExtent2d(double x4)`, `SetMinMax(double x4)` (`None` where it fails) |
//! | [`Extent2d::from_corners`]             | `SetMinMax(const SmPoint2d&, const SmPoint2d&)` (`None` where it fails) |
//! | [`Extent2d::from_intervals`]           | (the two intervals set)                        |
//! | [`Extent2d::from_extent3d`], `From<Extent3d>` | `SmExtent2d(const SmExtent3d&)`         |
//! | `Clone`/`Copy`                         | copy constructor, `operator=`, destructor      |
//! | [`Extent2d::are_same`]                 | `operator==`                                   |
//! | [`Extent2d::with_x_interval`], [`Extent2d::with_y_interval`] | `SetUInterval`, `SetVInterval`; with [`Extent1d::with_min`] and [`Extent1d::with_max`], `SetUMin`..`SetVMax` |
//! | [`Extent2d::transposed`]               | `Transpose`                                    |
//! | [`Extent2d::expanded`], [`Extent2d::expanded_vec`], [`Extent2d::shrunk`] | `ExpandAbsolute` |
//! | [`Extent2d::expanded_relative`]        | `ExpandRelative`                               |
//! | [`Extent2d::scaled`], [`Extent2d::scaled_xy`] | `Scale`                                 |
//! | [`Extent2d::min`], [`Extent2d::mid`], [`Extent2d::max`] | `GetMin`, `GetMid`, `GetMax`  |
//! | [`Extent2d::x_min`] .. [`Extent2d::y_max`] | `GetUMin`, `GetUMax`, `GetVMin`, `GetVMax` |
//! | [`Extent2d::x_interval`], [`Extent2d::y_interval`] | `GetUInterval`, `GetVInterval`     |
//! | [`Extent2d::max_dimension`]            | `GetMaxDimension`                              |
//! | [`Extent2d::size`]                     | `GetSize`                                      |
//! | [`Extent2d::x_length`], [`Extent2d::y_length`] | `XLength`, `YLength`                   |
//! | [`Extent2d::union`]                    | `Union`                                        |
//! | [`Extent2d::intersect`]                | `Intersect`                                    |
//! | [`Extent2d::max_boundary_dist`]        | `GetMaxBoundaryDist2d`                         |
//! | [`Extent2d::are_disjoint`]             | `AreDisjoint`                                  |
//! | [`Extent2d::are_equal`]                | `AreEqual`                                     |
//! | [`Extent2d::is_degenerate`]            | `IsDegenerate`                                 |
//! | [`Extent2d::is_contained_by`]          | `IsContainedBy`                                |
//! | [`Extent2d::contains_point`], [`Extent2d::contains_point_dist`] | `ContainsPoint2d`     |
//! | [`Extent2d::classify_point`]           | `ClassifyPoint2d`                              |
//! | [`Extent2d::is_point_on_boundary`]     | `IsPoint2dOnBoundary`                          |
//! | [`Extent2d::point_boundaries`]         | `GetPoint2dBoundaries`                         |
//! | [`Extent2d::contains_periodic_point`]  | `ContainsPeriodicPoint2d`                      |
//! | [`Extent2d::evaluate`]                 | `Evaluate`                                     |
//! | [`Extent2d::inversion`]                | `Inversion`                                    |
//! | [`Extent2d::clamp_point`]              | `ClampPoint2d`                                 |
//! | [`Extent2d::clip_line`]                | `ClipLine2d`                                   |
//! | [`Extent2d::intersect_with_infinite_line`] | `IntersectWithInfiniteLine`                |
//! | [`Extent2d::x_left_eval`], [`Extent2d::y_left_eval`] | `GetULeftEval`, `GetVLeftEval`   |
//! | [`Extent2d::point_map_to_domain`], [`ParamOrder`] | `Point2dMapToDomain`                |
//! | [`SS_NONE`], [`SS_UMIN`], [`SS_VMIN`], [`SS_UMAX`], [`SS_VMAX`] | `SM_SS_*` (`SmTypes.h`) |
//!
//! Methods of `SmExtent2d` that no in-scope upstream code reaches
//! (`harness/callgraph/unused_functions.txt`: `Set`, `SetUMin`..`SetVMax`, the
//! distance methods, `IsPoint`, `IsLine`, `AnyBounds`, `ClassifyPointDir2d`, the
//! periodic disjoint/intersect methods, `ApproximateUnbounded`, ...) are not ported.
//! `Scale` is listed there too but is ported for callers outside the closure: an
//! exporter scaling a plane's parameter domain (#1765).
//!
//! Deviations:
//! - Out-params become return values (tuples, `Option`, `Result`). Optional
//!   out-params (`ContainsPoint2d`'s distance, `IsPoint2dOnBoundary`'s binormal,
//!   `ContainsPeriodicPoint2d`'s periodic point) are always computed; they have no
//!   side effects.
//! - Status codes: `SM_ERR_INVALID_INPUT` is [`NumError::InvalidInput`]; `SM_ERR`
//!   is a typed [`crate::error::NumFailure`] (status 1001). `Intersect`'s failure (an
//!   `Init()` result) is `None`.
//!   `Inversion` may write a partial normalised point before failing; the Rust error
//!   carries none (no in-scope caller reads it).
//! - `IntersectWithInfiniteLine` always returns `SM_SUCCESS`; the Rust method
//!   returns the trimmed interval as an `Option` (`None`: `rbFoundInterval` FALSE).
//! - `SetUInterval`/`SetVInterval` only validate their argument under
//!   `SM_DEBUG_CODE`; in the release build they always succeed, so the Rust methods
//!   return the extent.
//! - No extent is inverted: a fold starts a [`Hull2d`] at ±inf (upstream ±1e20), and `Scale`
//!   by a negative factor reorders that axis.
//! - Every end is finite, as [`Extent1d`]'s (research 63 D1): upstream's
//!   `±SM_INFINITE_PARAMETER` markers and `IsBounded` have no counterpart, and a constructor
//!   or transform whose end would be ±inf or NaN answers `None`.
//! - The destructor's "undefined" sentinel values have no Rust counterpart.
//! - `SM_ASSERT*` checks and the error-log side effect of `SE(...)` are not ported
//!   (they do not change results in the release build).

use crate::error::{NumError, Result};
use crate::extent1d::{Extent1d, ExtentPointType, Hull1d};
use crate::extent3d::Extent3d;
use crate::math::{BIG_DOUBLE, EFF_ZERO, EFF_ZERO_SQRT};
use crate::tol::{are_same, are_same_to_tol, is_zero_to_tol, scaled_zero, ScaledZero};
use crate::vector2d::{Point2, Vec2};
use crate::vector3d::sqrt;

/// `SM_SS_NONE`: the point is on no boundary ([`Extent2d::point_boundaries`]).
pub const SS_NONE: u32 = 0;
/// `SM_SS_UMIN`: the point is on the `u = UMin` boundary.
pub const SS_UMIN: u32 = 1;
/// `SM_SS_VMIN`: the point is on the `v = VMin` boundary.
pub const SS_VMIN: u32 = 2;
/// `SM_SS_UMAX`: the point is on the `u = UMax` boundary.
pub const SS_UMAX: u32 = 4;
/// `SM_SS_VMAX`: the point is on the `v = VMax` boundary.
pub const SS_VMAX: u32 = 8;

/// Which normalised coordinate of the source feeds which axis of the target in
/// [`Extent2d::point_map_to_domain`] (upstream's `bSwitchParams`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ParamOrder {
    /// x to x and y to y (`bSwitchParams = false`).
    #[default]
    Kept,
    /// x to y and y to x (`bSwitchParams = true`).
    Swapped,
}

/// One coordinate of `Inversion`: `num / den`, or `1` / `-1` for a (near) zero
/// denominator.
fn normalise(num: f64, den: f64) -> f64 {
    if !is_zero_to_tol(den, EFF_ZERO_SQRT) {
        num / den
    } else if is_zero_to_tol(num, EFF_ZERO_SQRT) {
        1.0
    } else {
        -1.0
    }
}

/// An axis-aligned rectangle `[min, max]` in the plane (`SmExtent2d`), often a
/// surface's parameter domain; never inverted.
///
/// The invariant is `min <= max` on each axis with every end finite. An extent that may be
/// absent is an `Option<Extent2d>`; a fold over points is a [`Hull2d`].
///
/// `PartialEq` compares the corner points exactly (not upstream `operator==`, which
/// is [`Extent2d::are_same`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extent2d {
    min: Point2,
    max: Point2,
}

/// A running hull of points, for folds (`SmExtent2d()` then `AddPoint2d`): the two
/// [`Hull1d`]s of its axes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Hull2d {
    x: Hull1d,
    y: Hull1d,
}

impl Hull2d {
    /// Nothing added.
    pub const EMPTY: Self = Self {
        x: Hull1d::EMPTY,
        y: Hull1d::EMPTY,
    };

    /// Grows the hull to cover `p` (`AddPoint2d`); a NaN coordinate is passed over.
    #[inline]
    pub fn add(&mut self, p: Point2) {
        self.x.add(p.x);
        self.y.add(p.y);
    }

    /// Grows the hull to cover the extent `e`.
    #[inline]
    pub fn add_extent(&mut self, e: Extent2d) {
        self.x.add_extent(e.x_interval());
        self.y.add_extent(e.y_interval());
    }

    /// The hull of both.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            x: self.x.union(other.x),
            y: self.y.union(other.y),
        }
    }

    /// The extent covered, `None` when no point was added (or an axis has only NaN, or
    /// ±inf was added).
    #[must_use]
    pub fn finish(self) -> Option<Extent2d> {
        Some(Extent2d::from_intervals(self.x.finish()?, self.y.finish()?))
    }
}

impl FromIterator<Point2> for Hull2d {
    fn from_iter<I: IntoIterator<Item = Point2>>(points: I) -> Self {
        let mut h = Self::EMPTY;
        for p in points {
            h.add(p);
        }
        h
    }
}

impl From<Extent3d> for Extent2d {
    fn from(e: Extent3d) -> Self {
        Self::from_extent3d(&e)
    }
}

impl ScaledZero for Extent2d {
    /// `SmTol::GetScaledZero(const SmExtent2d&)`: `SM_EFF_ZERO * (1 + max |coord|)`
    /// over both corners.
    fn scaled_zero(&self) -> f64 {
        scaled_zero([self.min.max_dimension(), self.max.max_dimension()])
    }
}

impl Extent2d {
    /// The point-sized extent at `p`, `None` when a coordinate is not finite
    /// (`SmExtent2d(rMinAndMax)`).
    #[must_use]
    pub const fn from_point(p: Point2) -> Option<Self> {
        if p.x.is_finite() && p.y.is_finite() {
            Some(Self { min: p, max: p })
        } else {
            None
        }
    }

    /// The rectangle spanned by two corner points; coordinates are swapped per axis
    /// so that `min <= max`, `None` when a coordinate is not finite
    /// (`SmExtent2d(rMin, rMax)`).
    #[must_use]
    pub fn spanning(a: Point2, b: Point2) -> Option<Self> {
        Some(Self::from_intervals(
            Extent1d::spanning(a.x, b.x)?,
            Extent1d::spanning(a.y, b.y)?,
        ))
    }

    /// The rectangle with corners `min` and `max`, `None` when `min > max` on an axis or a
    /// coordinate is not finite (`SetMinMax(rMin, rMax)`, which refuses the inverted pair).
    #[must_use]
    pub fn from_corners(min: Point2, max: Point2) -> Option<Self> {
        Some(Self::from_intervals(
            Extent1d::new(min.x, max.x)?,
            Extent1d::new(min.y, max.y)?,
        ))
    }

    /// The rectangle with the given bounds, `None` when one is inverted or not finite
    /// (`SmExtent2d(UMin, VMin, UMax, VMax)`, `SetMinMax(double x4)`).
    #[must_use]
    pub fn from_bounds(x_min: f64, y_min: f64, x_max: f64, y_max: f64) -> Option<Self> {
        Self::from_corners(Point2::new(x_min, y_min), Point2::new(x_max, y_max))
    }

    /// The rectangle `x × y`.
    #[must_use]
    pub const fn from_intervals(x: Extent1d, y: Extent1d) -> Self {
        Self {
            min: Point2::new(x.min(), y.min()),
            max: Point2::new(x.max(), y.max()),
        }
    }

    /// The smallest extent covering `points`, `None` for none (a [`Hull2d`] fold).
    #[must_use]
    pub fn hull<I: IntoIterator<Item = Point2>>(points: I) -> Option<Self> {
        points.into_iter().collect::<Hull2d>().finish()
    }

    /// The x/y part of a 3D extent (`SmExtent2d(const SmExtent3d&)`).
    #[must_use]
    pub fn from_extent3d(e: &Extent3d) -> Self {
        Self::from_intervals(e.x_interval(), e.y_interval())
    }

    /// Both corners equal up to their scaled zeros (`operator==`).
    #[must_use]
    pub fn are_same(&self, other: &Self) -> bool {
        are_same(self.min.x, other.min.x)
            && are_same(self.min.y, other.min.y)
            && are_same(self.max.x, other.max.x)
            && are_same(self.max.y, other.max.y)
    }

    /// This extent with its x interval `ivl` (`SetUInterval`).
    #[must_use]
    pub const fn with_x_interval(self, ivl: Extent1d) -> Self {
        Self::from_intervals(ivl, self.y_interval())
    }

    /// This extent with its y interval `ivl` (`SetVInterval`).
    #[must_use]
    pub const fn with_y_interval(self, ivl: Extent1d) -> Self {
        Self::from_intervals(self.x_interval(), ivl)
    }

    /// The x and y intervals swapped (`Transpose`).
    #[must_use]
    pub const fn transposed(self) -> Self {
        Self::from_intervals(self.y_interval(), self.x_interval())
    }

    /// Every end moved outward by `d` (`ExpandAbsolute(double)` with `d >= 0`), `None` when
    /// an end would not be finite. A `d` below zero or NaN leaves the extent: shrinking is
    /// [`Extent2d::shrunk`].
    #[must_use]
    pub fn expanded(self, d: f64) -> Option<Self> {
        self.expanded_vec(Vec2::new(d, d))
    }

    /// The x ends moved outward by `d.x` and the y ends by `d.y`
    /// (`ExpandAbsolute(const SmVector2d)`), `None` when an end would not be finite; a
    /// component below zero or NaN leaves its axis.
    #[must_use]
    pub fn expanded_vec(self, d: Vec2) -> Option<Self> {
        Some(Self::from_intervals(
            self.x_interval().expanded(d.x)?,
            self.y_interval().expanded(d.y)?,
        ))
    }

    /// Every end moved inward by `d` (`ExpandAbsolute(-d)`), `None` when that inverts an
    /// axis or an end is not finite.
    #[must_use]
    pub fn shrunk(self, d: f64) -> Option<Self> {
        Some(Self::from_intervals(
            self.x_interval().shrunk(d)?,
            self.y_interval().shrunk(d)?,
        ))
    }

    /// Every end moved outward by `factor` times its interval's length (`ExpandRelative`
    /// with `factor >= 0`; below zero or NaN leaves the extent), `None` when an end would
    /// not be finite.
    #[must_use]
    pub fn expanded_relative(self, factor: f64) -> Option<Self> {
        Some(Self::from_intervals(
            self.x_interval().expanded_relative(factor)?,
            self.y_interval().expanded_relative(factor)?,
        ))
    }

    /// Every end scaled by `s` (`Scale(s)`), `None` when an end would not be finite.
    #[must_use]
    pub fn scaled(self, s: f64) -> Option<Self> {
        self.scaled_xy(s, s)
    }

    /// The x ends scaled by `sx` and the y ends by `sy` (`Scale(sx, &sy)`), `None` when an
    /// end would not be finite. A negative factor reorders its axis, where upstream leaves
    /// it inverted.
    #[must_use]
    pub fn scaled_xy(self, sx: f64, sy: f64) -> Option<Self> {
        Some(Self::from_intervals(
            self.x_interval().scaled(sx)?,
            self.y_interval().scaled(sy)?,
        ))
    }

    /// Minimum corner (`GetMin`).
    #[must_use]
    pub const fn min(&self) -> Point2 {
        self.min
    }

    /// Centre, `(min + max) / 2` (`GetMid`; whose guarded `Vec2` division returns the
    /// plain sum past 2e13).
    #[must_use]
    pub fn mid(&self) -> Point2 {
        self.min.midpoint(self.max)
    }

    /// Maximum corner (`GetMax`).
    #[must_use]
    pub const fn max(&self) -> Point2 {
        self.max
    }

    /// Minimum x (`GetUMin`).
    #[must_use]
    pub const fn x_min(&self) -> f64 {
        self.min.x
    }

    /// Maximum x (`GetUMax`).
    #[must_use]
    pub const fn x_max(&self) -> f64 {
        self.max.x
    }

    /// Minimum y (`GetVMin`).
    #[must_use]
    pub const fn y_min(&self) -> f64 {
        self.min.y
    }

    /// Maximum y (`GetVMax`).
    #[must_use]
    pub const fn y_max(&self) -> f64 {
        self.max.y
    }

    /// The x interval (`GetUInterval`).
    #[must_use]
    pub const fn x_interval(&self) -> Extent1d {
        Extent1d::from_ordered(self.min.x, self.max.x)
    }

    /// The y interval (`GetVInterval`).
    #[must_use]
    pub const fn y_interval(&self) -> Extent1d {
        Extent1d::from_ordered(self.min.y, self.max.y)
    }

    /// Largest absolute coordinate of either corner, a NaN one skipped (`GetMaxDimension`).
    #[must_use]
    pub fn max_dimension(&self) -> f64 {
        let (x, y) = (self.x_interval(), self.y_interval());
        x.max_dimension().max(y.max_dimension())
    }

    /// The diagonal `max - min` (`GetSize`).
    #[must_use]
    pub fn size(&self) -> Vec2 {
        Vec2::new(self.max.x - self.min.x, self.max.y - self.min.y)
    }

    /// `max.x - min.x` (`XLength`).
    #[must_use]
    pub fn x_length(&self) -> f64 {
        self.max.x - self.min.x
    }

    /// `max.y - min.y` (`YLength`).
    #[must_use]
    pub fn y_length(&self) -> f64 {
        self.max.y - self.min.y
    }

    /// The smallest extent containing both (`Union`).
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            min: Point2::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: Point2::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }

    /// The common part of two extents, `None` when they are disjoint (`Intersect`).
    #[must_use]
    pub fn intersect(self, other: Self) -> Option<Self> {
        if self.are_disjoint(&other, 0.0) {
            return None;
        }
        Some(Self {
            min: Point2::new(self.min.x.max(other.min.x), self.min.y.max(other.min.y)),
            max: Point2::new(self.max.x.min(other.max.x), self.max.y.min(other.max.y)),
        })
    }

    /// Largest distance between corresponding bounds (`GetMaxBoundaryDist2d`).
    #[must_use]
    pub fn max_boundary_dist(&self, other: &Self) -> f64 {
        let mut d = 0.0;
        for t in [
            (self.min.x - other.min.x).abs(),
            (self.max.x - other.max.x).abs(),
            (self.min.y - other.min.y).abs(),
            (self.max.y - other.max.y).abs(),
        ] {
            if t > d {
                d = t;
            }
        }
        d
    }

    /// True when the extents are separated by more than `tol`; touching extents are
    /// not disjoint (`AreDisjoint`, upstream default `tol = 0`).
    #[must_use]
    pub fn are_disjoint(&self, other: &Self, tol: f64) -> bool {
        self.min.x > other.max.x + tol
            || self.min.y > other.max.y + tol
            || self.max.x < other.min.x - tol
            || self.max.y < other.min.y - tol
    }

    /// True when all four bounds agree within `|tol|` (`AreEqual`, upstream default
    /// `tol = 0`).
    #[must_use]
    pub fn are_equal(&self, other: &Self, tol: f64) -> bool {
        are_same_to_tol(self.min.x, other.min.x, tol)
            && are_same_to_tol(self.min.y, other.min.y, tol)
            && are_same_to_tol(self.max.x, other.max.x, tol)
            && are_same_to_tol(self.max.y, other.max.y, tol)
    }

    /// True when a side is no longer than `tol` (`IsDegenerate`, upstream default
    /// `tol = 0`).
    #[must_use]
    pub fn is_degenerate(&self, tol: f64) -> bool {
        self.min.x >= self.max.x - tol || self.min.y >= self.max.y - tol
    }

    /// True when `self` lies inside `other` enlarged by `tol` (`IsContainedBy`,
    /// upstream default `tol = 0`).
    #[must_use]
    pub fn is_contained_by(&self, other: &Self, tol: f64) -> bool {
        !(self.min.x < other.min.x - tol
            || self.min.y < other.min.y - tol
            || self.max.x > other.max.x + tol
            || self.max.y > other.max.y + tol)
    }

    /// True when `p` lies in the extent enlarged by `tol` (`ContainsPoint2d` without
    /// the distance output; upstream default `tol = 0`).
    #[must_use]
    pub fn contains_point(&self, p: Point2, tol: f64) -> bool {
        !(p.x + tol < self.min.x
            || p.x - tol > self.max.x
            || p.y + tol < self.min.y
            || p.y - tol > self.max.y)
    }

    /// [`Extent2d::contains_point`] plus, for points outside, the distance to the
    /// extent (0 for points inside) (`ContainsPoint2d` with `pOptMinOutsideDist`).
    #[must_use]
    pub fn contains_point_dist(&self, p: Point2, tol: f64) -> (bool, f64) {
        let mut inside = true;
        let mut d2 = 0.0;
        if p.x + tol < self.min.x {
            inside = false;
            d2 += (self.min.x - p.x) * (self.min.x - p.x);
        } else if p.x - tol > self.max.x {
            inside = false;
            d2 += (p.x - self.max.x) * (p.x - self.max.x);
        }
        if p.y + tol < self.min.y {
            inside = false;
            d2 += (self.min.y - p.y) * (self.min.y - p.y);
        } else if p.y - tol > self.max.y {
            inside = false;
            d2 += (p.y - self.max.y) * (p.y - self.max.y);
        }
        (inside, sqrt(d2))
    }

    /// Classifies each coordinate of `p` against its interval, with boundary
    /// tolerance `tol_u` for u and `tol_v` (default `tol_u`) for v
    /// (`ClassifyPoint2d`; upstream default `tol_u = SM_EFF_ZERO`).
    #[must_use]
    pub fn classify_point(
        &self,
        p: Point2,
        tol_u: f64,
        tol_v: Option<f64>,
    ) -> (ExtentPointType, ExtentPointType) {
        let tol_v = tol_v.unwrap_or(tol_u);
        (
            self.x_interval().classify_point(p.x, tol_u),
            self.y_interval().classify_point(p.y, tol_v),
        )
    }

    /// Whether `p` is within `tol` of a boundary, and a vector pointing inward from
    /// the (first) boundary found per coordinate (`IsPoint2dOnBoundary`; upstream
    /// default `tol = SM_EFF_ZERO`).
    #[must_use]
    pub fn is_point_on_boundary(&self, p: Point2, tol: f64) -> (bool, Vec2) {
        let mut on = false;
        let (mut du, mut dv) = (0.0, 0.0);
        if (p.x - self.min.x).abs() <= tol {
            on = true;
            du = 1.0;
        } else if (p.x - self.max.x).abs() <= tol {
            on = true;
            du = -1.0;
        }
        if (p.y - self.min.y).abs() <= tol {
            on = true;
            dv = 1.0;
        } else if (p.y - self.max.y).abs() <= tol {
            on = true;
            dv = -1.0;
        }
        (on, Vec2::new(du, dv))
    }

    /// Bit set of the boundaries within `tol` of `p`: [`SS_NONE`] or an OR of
    /// [`SS_UMIN`], [`SS_VMIN`], [`SS_UMAX`], [`SS_VMAX`] (`GetPoint2dBoundaries`;
    /// upstream default `tol = SM_EFF_ZERO`).
    #[must_use]
    pub fn point_boundaries(&self, p: Point2, tol: f64) -> u32 {
        let mut b = SS_NONE;
        if (p.x - self.min.x).abs() <= tol {
            b |= SS_UMIN;
        }
        if (p.y - self.min.y).abs() <= tol {
            b |= SS_VMIN;
        }
        if (p.x - self.max.x).abs() <= tol {
            b |= SS_UMAX;
        }
        if (p.y - self.max.y).abs() <= tol {
            b |= SS_VMAX;
        }
        b
    }

    /// Whether `p` lies in the extent when u and/or v are periodic (period `0`: not
    /// periodic), with the point mapped to the extent's period
    /// (`ContainsPeriodicPoint2d`; `tol <= 0` means `SM_EFF_ZERO_DEG`, upstream
    /// default `tol = 0`).
    #[must_use]
    pub fn contains_periodic_point(
        &self,
        p: Point2,
        u_period: f64,
        v_period: f64,
        tol: f64,
    ) -> (bool, Point2) {
        let u = self
            .x_interval()
            .contains_periodic_value(p.x, u_period, tol);
        let v = self
            .y_interval()
            .contains_periodic_value(p.y, v_period, tol);
        (
            u.contained && v.contained,
            Point2::new(u.periodic_value, v.periodic_value),
        )
    }

    /// The point at normalised coordinates `(nx, ny)` (`(0, 0)` = min, `(1, 1)` =
    /// max), clamped to the extent (`Evaluate`).
    #[must_use]
    pub fn evaluate(&self, nx: f64, ny: f64) -> Point2 {
        self.clamp_point(Point2::new(
            self.x_interval().evaluate(nx),
            self.y_interval().evaluate(ny),
        ))
    }

    /// Normalised coordinates of `p` (snapped to the boundary when within `tol` of
    /// it, clamped to `[0, 1]`); a [`crate::error::NumFailure::PointOutsideExtent`]
    /// (`SM_ERR`) when `p` is outside or a side is degenerate and `p` is off it
    /// (`Inversion`; upstream default `tol = SM_EFF_ZERO`).
    pub fn inversion(&self, p: Point2, tol: f64) -> Result<Point2> {
        let err = || {
            NumError::failed(
                "geom::extent2d::Extent2d::inversion",
                crate::error::NumFailure::PointOutsideExtent { u: p.x, v: p.y },
            )
        };
        let mut pnt = p;
        if self.is_point_on_boundary(pnt, tol).0 {
            pnt = self.clamp_point(pnt);
        }
        if !self.contains_point(pnt, tol) {
            return Err(err());
        }
        let n = Point2::new(
            normalise(pnt.x - self.min.x, self.max.x - self.min.x),
            normalise(pnt.y - self.min.y, self.max.y - self.min.y),
        );
        if n.x == -1.0 || n.y == -1.0 {
            return Err(err());
        }
        let unit = Self {
            min: Point2::new(0.0, 0.0),
            max: Point2::new(1.0, 1.0),
        };
        Ok(unit.clamp_point(n))
    }

    /// `p` with each coordinate clamped to its interval (`ClampPoint2d`).
    #[must_use]
    pub fn clamp_point(&self, p: Point2) -> Point2 {
        let mut r = p;
        if p.x < self.min.x {
            r.x = self.min.x;
        }
        if p.y < self.min.y {
            r.y = self.min.y;
        }
        if p.x > self.max.x {
            r.x = self.max.x;
        }
        if p.y > self.max.y {
            r.y = self.max.y;
        }
        r
    }

    /// Clips the segment `p + s v`, `s` from 0 to `t`: returns `t` when its end
    /// point is inside, else the parameter of the first boundary crossing; `0` when
    /// `p` is on a boundary and the segment points out; `BIG_DOUBLE` when the
    /// segment cannot be clipped (`ClipLine2d`).
    #[must_use]
    pub fn clip_line(&self, p: Point2, v: Vec2, t: f64) -> f64 {
        let sign = if t < 0.0 { -1.0 } else { 1.0 };
        let out_tol = EFF_ZERO * 100.0;
        if (are_same(self.min.x, p.x) && v.x * sign < -out_tol)
            || (are_same(self.min.y, p.y) && v.y * sign < -out_tol)
            || (are_same(self.max.x, p.x) && v.x * sign > out_tol)
            || (are_same(self.max.y, p.y) && v.y * sign > out_tol)
        {
            return 0.0;
        }

        let mut r = t;
        let ps_tol = EFF_ZERO_SQRT / 10000.00;
        let mut end = p + r * v;
        if self.contains_point(end, ps_tol) {
            return r;
        }

        if end.x < self.min.x - ps_tol {
            if (end.x - p.x).abs() < EFF_ZERO * (self.min.x - p.x).abs() {
                return BIG_DOUBLE;
            }
            r = r * (self.min.x - p.x) / (end.x - p.x);
            end = p + r * v;
        } else if end.x > self.max.x + ps_tol {
            if (end.x - p.x).abs() < EFF_ZERO * (self.max.x - p.x).abs() {
                return BIG_DOUBLE;
            }
            r = r * (self.max.x - p.x) / (end.x - p.x);
            end = p + r * v;
        }

        if end.y < self.min.y - ps_tol {
            if (end.y - p.y).abs() < EFF_ZERO * (self.min.y - p.y).abs() {
                return BIG_DOUBLE;
            }
            r = r * (self.min.y - p.y) / (end.y - p.y);
        } else if end.y > self.max.y + ps_tol {
            if (end.y - p.y).abs() < EFF_ZERO * (self.max.y - p.y).abs() {
                return BIG_DOUBLE;
            }
            r = r * (self.max.y - p.y) / (end.y - p.y);
        }
        r
    }

    /// The parameter interval of the infinite line `p + s v` inside the extent
    /// enlarged by `tol_u` / `tol_v` (defaults `100 SM_EFF_ZERO` and `tol_u`);
    /// `None` when the line misses it (`IntersectWithInfiniteLine`).
    #[must_use]
    pub fn intersect_with_infinite_line(
        &self,
        p: Point2,
        v: Vec2,
        tol_u: Option<f64>,
        tol_v: Option<f64>,
    ) -> Option<Extent1d> {
        let mut ivl = Hull1d::EMPTY;
        let tol_u = tol_u.unwrap_or(EFF_ZERO * 100.0);
        let tol_v = tol_v.unwrap_or(tol_u);
        let min_x = self.min.x - tol_u;
        let max_x = self.max.x + tol_u;
        let min_y = self.min.y - tol_v;
        let max_y = self.max.y + tol_v;

        if v.x.abs() > EFF_ZERO {
            for bound in [self.min.x, self.max.x] {
                let prm = (bound - p.x) / v.x;
                let int = p.y + prm * v.y;
                if int >= min_y && int <= max_y {
                    ivl.add(prm);
                }
            }
        }
        if v.y.abs() > EFF_ZERO {
            for bound in [self.min.y, self.max.y] {
                let prm = (bound - p.y) / v.y;
                let int = p.x + prm * v.x;
                if int >= min_x && int <= max_x {
                    ivl.add(prm);
                }
            }
        }

        // A negative tolerance also refuses a crossing shorter than it.
        let ivl = ivl.finish()?;
        (ivl.length() > -tol_u.max(tol_v)).then_some(ivl)
    }

    /// True when `u` is in the lower half of the x interval (`GetULeftEval`).
    #[must_use]
    pub fn x_left_eval(&self, u: f64) -> bool {
        let mid = (self.min.x + self.max.x) / 2.0;
        // upstream: TRUE unless `u > mid` (so NaN gives TRUE)
        u <= mid || u.is_nan() || mid.is_nan()
    }

    /// True when `v` is in the lower half of the y interval (`GetVLeftEval`).
    #[must_use]
    pub fn y_left_eval(&self, v: f64) -> bool {
        let mid = (self.min.y + self.max.y) / 2.0;
        v <= mid || v.is_nan() || mid.is_nan()
    }

    /// Maps `p` from this domain to `other` through normalised coordinates,
    /// swapping x and y for [`ParamOrder::Swapped`]; a
    /// [`crate::error::NumFailure::PointOutsideExtent`] (`SM_ERR`) when
    /// [`Extent2d::inversion`] fails (`Point2dMapToDomain`).
    pub fn point_map_to_domain(
        &self,
        p: Point2,
        other: &Self,
        order: ParamOrder,
    ) -> Result<Point2> {
        let n = self.inversion(p, EFF_ZERO).map_err(|_| {
            NumError::failed(
                "geom::extent2d::Extent2d::point_map_to_domain",
                crate::error::NumFailure::PointOutsideExtent { u: p.x, v: p.y },
            )
        })?;
        Ok(if order == ParamOrder::Swapped {
            other.evaluate(n.y, n.x)
        } else {
            other.evaluate(n.x, n.y)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn bx(x_min: f64, y_min: f64, x_max: f64, y_max: f64) -> Extent2d {
        Extent2d::from_bounds(x_min, y_min, x_max, y_max).unwrap()
    }

    /// `Scale` (#1765): the ends scale, by the y factor in y, and a negative factor reorders
    /// its axis (upstream left it inverted).
    #[test]
    fn scale_scales_the_bounded_ends() {
        let e = Extent2d::spanning(p(-1.0, 2.0), p(3.0, 4.0))
            .unwrap()
            .scaled(2.0)
            .unwrap();
        assert_eq!((e.min(), e.max()), (p(-2.0, 4.0), p(6.0, 8.0)));
        let e = e.scaled_xy(0.5, -1.0).unwrap().scaled(1.0).unwrap();
        assert_eq!((e.min(), e.max()), (p(-1.0, -8.0), p(3.0, -4.0)));
        assert_eq!(e.scaled_xy(1.0, f64::MAX), None);
    }

    /// `my_test_extent2d` (`tests/prog_test/src/smCurveTest.cpp:2688`), owned parts.
    #[test]
    fn upstream_my_test_extent2d() {
        let e1 = Extent2d::spanning(p(1.0, 2.0), p(3.0, 4.0)).unwrap();
        assert_eq!(e1.min(), p(1.0, 2.0));
        assert_eq!(e1.max(), p(3.0, 4.0));
        let e3 = bx(11.0, 12.0, 13.0, 14.0);
        assert_eq!((e3.min(), e3.max()), (p(11.0, 12.0), p(13.0, 14.0)));
        let e4 = Extent2d::from_point(p(1.0, 2.0)).unwrap();
        assert_eq!(e4.max(), p(1.0, 2.0));
        let x3 = Extent3d::from_bounds(1.0, 2.0, 3.0, 4.0, 5.0, 6.0).unwrap();
        assert_eq!(Extent2d::from(x3).max(), p(4.0, 5.0));
        assert_eq!(Hull2d::EMPTY.finish(), None);
        let mut h = Hull2d::EMPTY;
        h.add_extent(e1);
        h.add(p(5.0, 6.0));
        let e2 = h.finish().unwrap();
        assert_eq!(e2.max(), p(5.0, 6.0));
        let e2 = Extent2d::from_corners(p(1.0, 2.0), p(3.0, 4.0)).unwrap();
        assert_eq!(e2.min(), p(1.0, 2.0));

        let e6 = bx(-3.0, -2.0, 0.0, 1.0);
        assert!(!e1.are_disjoint(&e6, 2.0));
        assert!(e1.are_disjoint(&e6, 0.0));

        let e6 = bx(-4.0, -2.0, -3.0, -1.0);
        let u = e1.union(e6);
        assert_eq!((u.min(), u.max()), (p(-4.0, -2.0), p(3.0, 4.0)));
        let i = e1.intersect(bx(0.0, 0.0, 4.0, 3.0)).unwrap();
        assert_eq!(i.max(), p(3.0, 3.0));
        assert_eq!(e1.intersect(e6), None);

        let e2 = e2
            .with_x_interval(Extent1d::spanning(7.0, 8.0).unwrap())
            .with_y_interval(Extent1d::spanning(4.0, 5.0).unwrap());
        assert_eq!((e2.min(), e2.max()), (p(7.0, 4.0), p(8.0, 5.0)));

        let e2 = bx(2.0, 3.0, 5.0, 6.0).transposed();
        assert_eq!(e2.min(), p(3.0, 2.0));
        let e2 = e2.expanded(1.0).unwrap();
        assert_eq!(e2.min(), p(2.0, 1.0));
        let e2 = e2.expanded_relative(1.0).unwrap();
        assert_eq!((e2.min(), e2.max()), (p(-3.0, -4.0), p(12.0, 11.0)));
        assert_eq!(e2.size(), Vec2::new(15.0, 15.0));
        assert_eq!(e2.x_interval(), Extent1d::spanning(-3.0, 12.0).unwrap());
        assert_eq!(e2.y_interval(), Extent1d::spanning(-4.0, 11.0).unwrap());
        assert_eq!((e2.x_length(), e2.y_length()), (15.0, 15.0));
        assert_eq!(e2.max_dimension(), 12.0);

        let e2 = bx(1.0, 2.1, 3.0, 4.0);
        assert!(e1.are_equal(&e2, 0.2));
        assert!(!e1.are_equal(&e2, 0.0));
        assert!(e1.are_same(&e1));

        let e2 = bx(4.0, 5.0, 4.1, 5.1);
        assert!(e2.is_degenerate(0.2));
        assert!(!e2.is_degenerate(0.0));
        assert_eq!(bx(0.0, 0.0, 0.0, 0.0).shrunk(0.1), None);

        let e2 = e1.expanded(0.01).unwrap();
        assert!(e2.is_contained_by(&e1, 0.1));
        assert!(!e2.is_contained_by(&e1, 0.0));

        assert!(e1.contains_point(p(0.9, 3.0), 0.2));
        assert!(!e1.contains_point(p(0.9, 3.0), 0.0));

        let e2 = bx(4.0, 5.0, 4.1, 5.1);
        let (cu, cv) = e2.classify_point(p(1.1, 2.1), 0.2, None);
        assert_eq!(
            (cu, cv),
            (ExtentPointType::Outside, ExtentPointType::Outside)
        );
        let (cu, cv) = e1.classify_point(p(1.1, 2.1), 0.2, None);
        assert_eq!((cu, cv), (ExtentPointType::Start, ExtentPointType::Start));
        let (cu, cv) = e1.classify_point(p(1.1, 2.1), EFF_ZERO, None);
        assert_eq!((cu, cv), (ExtentPointType::Inside, ExtentPointType::Inside));

        assert_eq!(
            e1.is_point_on_boundary(p(1.1, 2.1), 0.2),
            (true, Vec2::new(1.0, 1.0))
        );
        assert!(!e1.is_point_on_boundary(p(1.1, 2.1), EFF_ZERO).0);

        // u = 8.9 is 0.9 a period down, within the tolerance of [1, 3]: it keeps 0.9
        // (WR-509, #1518; upstream's MyTest expects the start, 1.0)
        let (inside, q) = e1.contains_periodic_point(p(8.9, 3.0), 4.0, 0.0, 0.2);
        assert!(inside);
        assert!((q.x - 0.9).abs() <= 1e-12, "{}", q.x);
        assert_eq!(q.y, 3.0);
        assert!(!e1.contains_periodic_point(p(8.9, 3.0), 4.0, 0.0, 0.0).0);

        assert_eq!(e1.evaluate(0.5, 0.5), p(2.0, 3.0));
        assert_eq!(e1.inversion(p(2.0, 3.0), 0.1).unwrap(), p(0.5, 0.5));
        assert_eq!(e1.inversion(p(2.0, 3.0), EFF_ZERO).unwrap(), p(0.5, 0.5));
        assert_eq!(e1.clamp_point(p(0.0, 0.0)), p(1.0, 2.0));
        assert_eq!(e1.clip_line(p(2.0, 3.0), Vec2::new(0.0, 1.0), 4.0), 1.0);

        let ivl = e1
            .intersect_with_infinite_line(p(0.0, 3.1), Vec2::new(1.0, 1.0), Some(0.2), None)
            .unwrap();
        assert!((ivl.min() - 0.9).abs() <= EFF_ZERO);
        assert_eq!(ivl.max(), 1.0);
        assert!(e1
            .intersect_with_infinite_line(p(0.0, 3.1), Vec2::new(1.0, 1.0), None, None)
            .is_none());

        assert!(!e1.x_left_eval(2.5));
        assert!(e1.y_left_eval(2.5));

        let e2 = bx(4.0, 0.0, 10.0, 8.0);
        assert_eq!(
            e1.point_map_to_domain(p(2.0, 3.0), &e2, ParamOrder::Kept)
                .unwrap(),
            p(7.0, 4.0)
        );
    }

    #[test]
    fn inverted_bounds_are_refused() {
        assert_eq!(Extent2d::from_corners(p(2.0, 0.0), p(1.0, 1.0)), None);
        assert_eq!(Extent2d::from_bounds(0.0, 2.0, 1.0, 1.0), None);
    }

    /// No constructor or transform stores ±inf or NaN (research 63 D1).
    #[test]
    fn non_finite_ends_are_refused() {
        let (inf, nan) = (f64::INFINITY, f64::NAN);
        assert_eq!(Extent2d::from_bounds(-inf, 0.0, 1.0, 1.0), None);
        assert_eq!(Extent2d::from_corners(p(0.0, 0.0), p(1.0, nan)), None);
        assert_eq!(Extent2d::spanning(p(0.0, inf), p(1.0, 1.0)), None);
        assert_eq!(Extent2d::from_point(p(nan, 0.0)), None);
        assert_eq!(Extent2d::hull([p(0.0, 0.0), p(1.0, -inf)]), None);
        let e = bx(0.0, 0.0, 1.0, f64::MAX);
        assert_eq!(e.expanded(f64::MAX), None);
        assert_eq!(
            e.expanded_vec(Vec2::new(1.0, 0.0)),
            Some(bx(-1.0, 0.0, 2.0, f64::MAX))
        );
        assert_eq!(e.expanded_relative(1.0), None);
        assert_eq!(e.scaled(nan), None);
    }

    #[test]
    fn point_boundaries_and_distance() {
        let e = bx(0.0, 0.0, 1.0, 1.0);
        assert_eq!(e.point_boundaries(p(0.0, 1.0), EFF_ZERO), SS_UMIN | SS_VMAX);
        assert_eq!(e.point_boundaries(p(0.5, 0.5), EFF_ZERO), SS_NONE);
        assert_eq!(e.contains_point_dist(p(4.0, 5.0), 0.0), (false, 5.0));
        assert_eq!(e.contains_point_dist(p(0.5, 0.5), 0.0), (true, 0.0));
    }

    #[test]
    fn inversion_of_degenerate_side() {
        let e = bx(0.0, 0.0, 0.0, 1.0);
        assert_eq!(e.inversion(p(0.0, 0.5), EFF_ZERO).unwrap(), p(1.0, 0.5));
        let outside = e.inversion(p(2.0, 0.5), EFF_ZERO).unwrap_err();
        assert_eq!(
            outside.failure(),
            Some(&crate::error::NumFailure::PointOutsideExtent { u: 2.0, v: 0.5 })
        );
        // status SM_ERR (1001): every `NumError::Failed` has it
        assert!(matches!(outside, NumError::Failed { .. }));
        let mapped = e
            .point_map_to_domain(p(2.0, 0.5), &bx(0.0, 0.0, 1.0, 1.0), ParamOrder::Kept)
            .unwrap_err();
        assert_eq!(mapped.failure(), outside.failure());
        // status SM_ERR (1001): every `NumError::Failed` has it
        assert!(matches!(mapped, NumError::Failed { .. }));
    }

    #[test]
    fn scaled_zero_of_extent() {
        let e = bx(-3.0, 1.0, 2.0, 1.0);
        assert_eq!(e.scaled_zero(), EFF_ZERO * 4.0);
    }
}
