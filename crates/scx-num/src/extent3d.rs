// Moved from solidcore `solidcore/crates/solidcore/src/geom/extent3d.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/src/SmExtent3d.cpp`,
// `source/SMLib/inc/SmExtent3d.h`.
// Changes: `from_pseudo_box` and `circumscribe` stay in solidcore (they need `PseudoBox`);
// `min`/`max` fields private; errors are `NumError`; paths. `is_defined` (`AssertDefined`, the
// destructor's `SM_UNDEF_DOUBLE` sentinel) is gone (`docs/01-architecture.md` §10.1), so
// `intersect_line` no longer refuses a box holding that value; the test `predicates` drops its
// check.
// Then the inside-out empty extent went: an `Extent3d` is never inverted, and a fold is a
// `Hull3d`. `new`, `Default`, `init`, `is_init`, `has_negative_volume`, `add_point`,
// `set_unbounded` (now `UNBOUNDED`), the `set_*` setters and `BoundaryType::code` went:
// `from_bounds` and `from_corners` answer `Option`, the swapping constructor is `spanning`, an
// axis is replaced by `with_x_interval`.., and the expansions take and return the value. u/v/w
// are x/y/z. `intersect` answers `Option`.
//! Axis-aligned 3D extents (bounding boxes): construction, growth, set operations,
//! distances, containment and classification predicates, line clipping, and
//! approximation of unbounded extents.
//!
//! Upstream: `source/SMLib/src/SmExtent3d.cpp`, `source/SMLib/inc/SmExtent3d.h`.
//!
//! | Rust                                   | Upstream                                       |
//! |----------------------------------------|------------------------------------------------|
//! | [`Extent3d`]                           | `SmExtent3d`                                   |
//! | [`Hull3d`]                             | `SmExtent3d()`, `Init`, `AddPoint3d`           |
//! | [`Extent3d::spanning`]                 | `SmExtent3d(const SmPoint3d&, const SmPoint3d&)` |
//! | [`Extent3d::from_point`]               | `SmExtent3d(const SmPoint3d&)`                 |
//! | [`Extent3d::from_bounds`]              | `SmExtent3d(double x6)`, `SetMinMax(double x6)` (`None` where it fails) |
//! | [`Extent3d::from_corners`]             | `SetMinMax(const SmPoint3d&, const SmPoint3d&)` (`None` where it fails) |
//! | [`Extent3d::from_intervals`]           | (the three intervals set)                      |
//! | [`Extent3d::UNBOUNDED`]                | `SetUnbounded`                                 |
//! | [`Extent3d::min`], [`Extent3d::max`], [`Extent3d::mid`] | `GetMin()`, `GetMax()`, `GetMid()` |
//! | [`Extent3d::x_min`] .. [`Extent3d::z_max`] | `GetUMin` .. `GetWMax`                     |
//! | [`Extent3d::min_at`], [`Extent3d::max_at`] | `GetMin(ULONG)`, `GetMax(ULONG)`           |
//! | [`Extent3d::max_length`]               | `GetMaxLength`                                 |
//! | [`Extent3d::max_dimension`]            | `GetMaxDimension`                              |
//! | [`Extent3d::evaluate`]                 | `Evaluate`                                     |
//! | [`Extent3d::union`]                    | `Union`                                        |
//! | [`Extent3d::intersect`]                | `Intersect`                                    |
//! | [`Extent3d::max_distance_squared`]     | `MaximumDistanceSquared`                       |
//! | [`Extent3d::min_distance_squared`]     | `MinimumDistanceSquared`                       |
//! | [`Extent3d::max_distance`]             | `MaximumDistance`                              |
//! | [`Extent3d::min_distance`]             | `MinimumDistance`                              |
//! | [`Extent3d::expanded_relative`]        | `ExpandRelative`                               |
//! | [`Extent3d::expanded`], [`Extent3d::shrunk`] | `ExpandAbsolute`                         |
//! | [`Extent3d::size`]                     | `GetSize`                                      |
//! | [`Extent3d::corners`]                  | `GetCorners`                                   |
//! | [`Extent3d::edges`]                    | `GetEdges`                                     |
//! | [`Extent3d::planes`]                   | `GetPlanes`                                    |
//! | [`Extent3d::x_length`] .. [`Extent3d::z_length`] | `XLength`, `YLength`, `ZLength`      |
//! | [`Extent3d::sphere_bound`]             | `ComputeSphereBound`                           |
//! | [`Extent3d::distance_to_plane`]        | `DistanceToPlane`                              |
//! | [`Extent3d::cone_bound_to`]            | `ComputeConeBoundTo`                           |
//! | [`Extent3d::are_disjoint`]             | `AreDisjoint`                                  |
//! | [`Extent3d::is_contained_by`]          | `IsContainedBy`                                |
//! | [`Extent3d::is_point_sized`]           | `IsPointSized`                                 |
//! | [`Extent3d::contains_point`]           | `ContainsPoint3d`                              |
//! | [`Extent3d::contains_line_seg`]        | `ContainsLineSeg3d`                            |
//! | [`Extent3d::is_point_on_boundary`]     | `IsPoint3dOnBoundary`                          |
//! | [`Extent3d::intersect_line`], [`LineHit`] | `IntersectLine`                             |
//! | [`Extent3d::boundary_types`], [`Extent3d::is_bounded`] | `IsBounded`                    |
//! | [`Extent3d::any_bounds`]               | `AnyBounds`                                    |
//! | [`Extent3d::x_interval`], [`Extent3d::y_interval`], [`Extent3d::z_interval`] | `GetUInterval`, `GetVInterval`, `GetWInterval` |
//! | [`Extent3d::with_x_interval`], [`Extent3d::with_y_interval`], [`Extent3d::with_z_interval`] | `SetUInterval`, `SetVInterval`; with [`Extent1d::with_min`] and [`Extent1d::with_max`], `SetUMin`..`SetVMax` |
//! | [`Extent3d::approximate_unbounded`], [`Extent3d::approximate_unbounded_expanded`] | `ApproximateUnbounded` |
//! | [`Extent3d::max_distance_to_point`]    | (not upstream) the farthest of the corners from a point |
//! | [`Extent3d::span_along`]               | (not upstream) the box's reach along a direction |
//! | [`Extent3d::meets_ray`]                | (not upstream) whether a forward ray meets the padded box |
//! | [`BoundaryType`]                       | `SmBoundaryType` (`SmTypes.h`)                 |
//!
//! Deviations:
//! - Out-params become return values ([`LineHit`], [`SphereBound`], tuples, arrays).
//!   `GetCorners`/`GetEdges`/`GetPlanes` return fixed-size arrays.
//! - `IntersectLine` leaves `rdTEnter`/`rdTExit` unset when nothing is found; the
//!   [`LineHit`] variants carry only the parameters upstream sets.
//! - `ComputeSphereBound`'s optional inner radius is always computed.
//! - `ApproximateUnbounded`'s optional expanded extent is returned as an `Option`:
//!   `None` where upstream's `SetMinMax` rejects it and leaves the caller's extent
//!   untouched. Upstream mutates `sMin` in place before reading it in
//!   `SM_BT_UNBOUNDED_MAX`, which is bit-identical to reading the original `min` on all
//!   binary64 inputs (WR-4;
//!   `plan/upstream-issues/WR-4--addendum-2026-10-31-approx-unbounded-original-min.md`).
//! - Status codes: `SM_ERR_INVALID_INPUT` is [`NumError::InvalidInput`]; `Intersect`'s
//!   failure (an `Init()` result) is `None`; the `SM_ERR` of `ComputeConeBoundTo` is [`NumFailure`](crate::error::NumFailure) with code 1001.
//! - `SetUInterval`/`SetVInterval` only validate their argument under
//!   `SM_DEBUG_CODE`; in the release build they always succeed, so the Rust methods
//!   return the extent.
//! - No extent is inverted: a fold starts a [`Hull3d`] at ±inf (upstream ±1e20, so a
//!   coordinate beyond 1e20 now counts where upstream lost it), and an expansion by a
//!   negative amount is [`Extent3d::shrunk`], `None` where upstream left the box inverted.
//! - The destructor's "undefined" sentinel values have no Rust counterpart (a dropped
//!   value cannot be used), nor has `AssertDefined`.
//! - `SM_ASSERT*` debug checks are not ported (they do not change results).
//! - Upstream bug kept: `ContainsLineSeg3d` returns true when *either* end point is
//!   inside (it ORs the two tests). See `plan/findings.md`.
//! - `SmExtent3d(const SmPseudoBox&)` and `Circumscribe` (`from_pseudo_box`,
//!   `circumscribe` in solidcore) stay with the pseudo box, above this layer.

use crate::error::{NumError, Result};
use crate::extent1d::{Extent1d, Hull1d};
use crate::math::{
    is_infinite_param, Point3, Vec3, BIG_DOUBLE, EFF_ZERO, EFF_ZERO_SQ, INFINITE_PARAMETER, PI,
};
use crate::vector3d::{asin, sqrt, Axis, UnitVec3};

/// Classification of one interval of an extent (`SmBoundaryType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoundaryType {
    /// `SM_BT_UNKNOWN`: not classified yet.
    Unknown,
    /// `SM_BT_BOUNDED`: bounded below and above.
    Bounded,
    /// `SM_BT_UNBOUNDED_MIN`: the minimum is `-INFINITE_PARAMETER`.
    UnboundedMin,
    /// `SM_BT_UNBOUNDED_MAX`: the maximum is `INFINITE_PARAMETER`.
    UnboundedMax,
    /// `SM_BT_UNBOUNDED`: unbounded in both directions.
    Unbounded,
}

impl BoundaryType {
    /// Classifies the interval `[min, max]` (the per-axis expression of `IsBounded`).
    fn classify(min: f64, max: f64) -> Self {
        if is_infinite_param(max) && is_infinite_param(min) {
            Self::Unbounded
        } else if is_infinite_param(min) {
            Self::UnboundedMin
        } else if is_infinite_param(max) {
            Self::UnboundedMax
        } else {
            Self::Bounded
        }
    }
}

/// Result of [`Extent3d::intersect_line`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineHit {
    /// No intersection (upstream count 0).
    Miss,
    /// The line grazes the box at one parameter (upstream count 1,
    /// `rdTEnter == rdTExit == t`).
    Touch {
        /// Line parameter of the touching point.
        t: f64,
    },
    /// A portion of the line is inside the box (upstream count 2).
    Through {
        /// Parameter where the line enters the box.
        enter: f64,
        /// Parameter where the line leaves the box.
        exit: f64,
    },
}

impl LineHit {
    /// Upstream `rlNumFound` (0, 1 or 2).
    #[must_use]
    pub const fn count(self) -> u32 {
        match self {
            Self::Miss => 0,
            Self::Touch { .. } => 1,
            Self::Through { .. } => 2,
        }
    }
}

/// Result of [`Extent3d::sphere_bound`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SphereBound {
    /// Centre of the extent (and of both spheres).
    pub center: Point3,
    /// Radius of the sphere that contains the whole extent.
    pub radius: f64,
    /// Radius of the largest centred sphere inside the extent (smallest half side).
    pub min_radius: f64,
}

/// An axis-aligned 3D box `[min, max]` (`SmExtent3d`), often a bounding box; never
/// inverted.
///
/// The invariant is `!(min > max)` on each axis (a NaN end is held). A box that may be
/// absent is an `Option<Extent3d>`; a fold over points is a [`Hull3d`]. Unbounded interval
/// ends are stored as `±INFINITE_PARAMETER`.
///
/// `PartialEq` compares the corner points exactly (not upstream `operator==`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extent3d {
    min: Point3,
    max: Point3,
}

/// A running hull of points, for folds (`SmExtent3d()` then `AddPoint3d`): the three
/// [`Hull1d`]s of its axes, starting at ±inf so that [`Hull3d::add`] needs no branch.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Hull3d {
    x: Hull1d,
    y: Hull1d,
    z: Hull1d,
}

impl Hull3d {
    /// Nothing added.
    pub const EMPTY: Self = Self {
        x: Hull1d::EMPTY,
        y: Hull1d::EMPTY,
        z: Hull1d::EMPTY,
    };

    /// Grows the hull to cover `p` (`AddPoint3d`); a NaN coordinate is passed over.
    #[inline]
    pub fn add(&mut self, p: Point3) {
        self.x.add(p.x);
        self.y.add(p.y);
        self.z.add(p.z);
    }

    /// Grows the hull to cover the box `e`.
    #[inline]
    pub fn add_extent(&mut self, e: Extent3d) {
        self.x.add_extent(e.x_interval());
        self.y.add_extent(e.y_interval());
        self.z.add_extent(e.z_interval());
    }

    /// The hull of both.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            x: self.x.union(other.x),
            y: self.y.union(other.y),
            z: self.z.union(other.z),
        }
    }

    /// The box covered, `None` when no point was added (or an axis has only NaN).
    #[must_use]
    pub fn finish(self) -> Option<Extent3d> {
        Some(Extent3d::from_intervals(
            self.x.finish()?,
            self.y.finish()?,
            self.z.finish()?,
        ))
    }
}

impl FromIterator<Point3> for Hull3d {
    fn from_iter<I: IntoIterator<Item = Point3>>(points: I) -> Self {
        let mut h = Self::EMPTY;
        for p in points {
            h.add(p);
        }
        h
    }
}

/// `IntersectLine`'s slab parameters `(tmin, tmax)` along one axis.
fn slab(min: f64, max: f64, p: f64, v: f64) -> (f64, f64) {
    if v > EFF_ZERO {
        let lo = if is_infinite_param(min) {
            -INFINITE_PARAMETER
        } else {
            (min - p) / v
        };
        let hi = if is_infinite_param(max) {
            INFINITE_PARAMETER
        } else {
            (max - p) / v
        };
        (lo, hi)
    } else if v < -EFF_ZERO {
        let hi = if is_infinite_param(min) {
            INFINITE_PARAMETER
        } else {
            (min - p) / v
        };
        let lo = if is_infinite_param(max) {
            -INFINITE_PARAMETER
        } else {
            (max - p) / v
        };
        (lo, hi)
    } else {
        (-BIG_DOUBLE, BIG_DOUBLE)
    }
}

/// One axis of `ApproximateUnbounded`: the replacement `(min, max)` of an interval
/// classified as `ty`. `center` is the caller's optional centre, `c` its value or 0.
///
/// Upstream mutates `sMin` in place before computing `sMax`, but in
/// `SM_BT_UNBOUNDED_MAX` `sMin` changes only when `!(min > pc - h)` (becoming
/// `v = pc - h`), for which `v > v` is false on every binary64 value (including
/// `±0.0`, `±inf`, and `NaN`); reading `min` directly is therefore bit-identical
/// across all inputs (WR-4).
fn approx_axis(
    ty: BoundaryType,
    min: f64,
    max: f64,
    center: Option<f64>,
    c: f64,
    h: f64,
) -> (f64, f64) {
    // Upstream: `(pUnboundedCenter == NULL || <cmp>) ? <near> : <centred>`; the same
    // comparisons are used so NaN inputs take the same branch.
    let max_near_center = center.is_none_or(|pc| max < pc + h);
    let min_near_center = center.is_none_or(|pc| min > pc - h);
    let new_min = match (ty, center) {
        (BoundaryType::UnboundedMin, Some(pc)) if !max_near_center => pc - h,
        (BoundaryType::UnboundedMin, _) => max - 2.0 * h,
        (BoundaryType::UnboundedMax, Some(pc)) if !min_near_center => pc - h,
        (BoundaryType::Unbounded, _) => c - h,
        (BoundaryType::UnboundedMax | BoundaryType::Bounded | BoundaryType::Unknown, _) => min,
    };
    let new_max = match (ty, center) {
        (BoundaryType::UnboundedMin, Some(pc)) if !max_near_center => pc + h,
        (BoundaryType::UnboundedMax, Some(pc)) if !min_near_center => pc + h,
        (BoundaryType::UnboundedMax, _) => min + 2.0 * h,
        (BoundaryType::Unbounded, _) => c + h,
        (BoundaryType::UnboundedMin | BoundaryType::Bounded | BoundaryType::Unknown, _) => max,
    };
    (new_min, new_max)
}

impl Extent3d {
    /// Every interval `[-INFINITE_PARAMETER, INFINITE_PARAMETER]` (`SetUnbounded`).
    pub const UNBOUNDED: Self = Self::from_intervals(
        Extent1d::UNBOUNDED,
        Extent1d::UNBOUNDED,
        Extent1d::UNBOUNDED,
    );

    /// The box spanned by two corner points; coordinates are swapped per axis so
    /// that `min <= max` (`SmExtent3d(rMin, rMax)`).
    #[must_use]
    pub fn spanning(a: Point3, b: Point3) -> Self {
        Self::from_intervals(
            Extent1d::spanning(a.x, b.x),
            Extent1d::spanning(a.y, b.y),
            Extent1d::spanning(a.z, b.z),
        )
    }

    /// The point-sized box at `p` (`SmExtent3d(rMinAndMax)`).
    #[must_use]
    pub const fn from_point(p: Point3) -> Self {
        Self { min: p, max: p }
    }

    /// The box with corners `min` and `max`, `None` when `min > max` on an axis
    /// (`SetMinMax(rMin, rMax)`, which refuses it).
    #[must_use]
    pub fn from_corners(min: Point3, max: Point3) -> Option<Self> {
        Some(Self::from_intervals(
            Extent1d::from_bounds(min.x, max.x)?,
            Extent1d::from_bounds(min.y, max.y)?,
            Extent1d::from_bounds(min.z, max.z)?,
        ))
    }

    /// The box with the given bounds, `None` when one is inverted
    /// (`SmExtent3d(UMin, VMin, WMin, UMax, VMax, WMax)`, `SetMinMax(double x6)`).
    #[must_use]
    pub fn from_bounds(
        x_min: f64,
        y_min: f64,
        z_min: f64,
        x_max: f64,
        y_max: f64,
        z_max: f64,
    ) -> Option<Self> {
        Self::from_corners(
            Point3::new(x_min, y_min, z_min),
            Point3::new(x_max, y_max, z_max),
        )
    }

    /// The box `x × y × z`.
    #[must_use]
    pub const fn from_intervals(x: Extent1d, y: Extent1d, z: Extent1d) -> Self {
        Self {
            min: Point3::new(x.min(), y.min(), z.min()),
            max: Point3::new(x.max(), y.max(), z.max()),
        }
    }

    /// The smallest box covering `points`, `None` for none (a [`Hull3d`] fold).
    #[must_use]
    pub fn hull<I: IntoIterator<Item = Point3>>(points: I) -> Option<Self> {
        points.into_iter().collect::<Hull3d>().finish()
    }

    /// Minimum corner (`GetMin()`).
    #[must_use]
    pub const fn min(&self) -> Point3 {
        self.min
    }

    /// Maximum corner (`GetMax()`).
    #[must_use]
    pub const fn max(&self) -> Point3 {
        self.max
    }

    /// Centre, `(max + min) / 2` (`GetMid()`).
    #[must_use]
    pub fn mid(&self) -> Point3 {
        self.max.midpoint(self.min)
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

    /// Minimum z (`GetWMin`).
    #[must_use]
    pub const fn z_min(&self) -> f64 {
        self.min.z
    }

    /// Maximum z (`GetWMax`).
    #[must_use]
    pub const fn z_max(&self) -> f64 {
        self.max.z
    }

    /// Minimum along `axis` (`GetMin(ULONG)`, which answered 0.0 past 2).
    #[must_use]
    pub fn min_at(&self, axis: Axis) -> f64 {
        self.min[axis]
    }

    /// Maximum along `axis` (`GetMax(ULONG)`).
    #[must_use]
    pub fn max_at(&self, axis: Axis) -> f64 {
        self.max[axis]
    }

    /// Largest side length (`GetMaxLength`).
    #[must_use]
    pub fn max_length(&self) -> f64 {
        self.x_length().max(self.y_length()).max(self.z_length())
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

    /// The z interval (`GetWInterval`).
    #[must_use]
    pub const fn z_interval(&self) -> Extent1d {
        Extent1d::from_ordered(self.min.z, self.max.z)
    }

    /// This box with its x interval `ivl` (`SetUInterval`).
    #[must_use]
    pub const fn with_x_interval(self, ivl: Extent1d) -> Self {
        Self::from_intervals(ivl, self.y_interval(), self.z_interval())
    }

    /// This box with its y interval `ivl` (`SetVInterval`).
    #[must_use]
    pub const fn with_y_interval(self, ivl: Extent1d) -> Self {
        Self::from_intervals(self.x_interval(), ivl, self.z_interval())
    }

    /// This box with its z interval `ivl` (not upstream, which sets only U and V).
    #[must_use]
    pub const fn with_z_interval(self, ivl: Extent1d) -> Self {
        Self::from_intervals(self.x_interval(), self.y_interval(), ivl)
    }

    /// Largest absolute coordinate of either corner, a NaN one skipped (`GetMaxDimension`).
    #[must_use]
    pub fn max_dimension(&self) -> f64 {
        let (x, y, z) = (self.x_interval(), self.y_interval(), self.z_interval());
        x.max_dimension()
            .max(y.max_dimension())
            .max(z.max_dimension())
    }

    /// The point at normalised coordinates `(nx, ny, nz)`: `0` gives the minimum, `1`
    /// the maximum (exactly), values outside `[0, 1]` extrapolate (`Evaluate`).
    #[must_use]
    pub fn evaluate(&self, nx: f64, ny: f64, nz: f64) -> Point3 {
        Point3::new(
            self.x_interval().evaluate(nx),
            self.y_interval().evaluate(ny),
            self.z_interval().evaluate(nz),
        )
    }

    /// The smallest box containing both boxes (`Union`).
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            min: Point3::new(
                self.min.x.min(other.min.x),
                self.min.y.min(other.min.y),
                self.min.z.min(other.min.z),
            ),
            max: Point3::new(
                self.max.x.max(other.max.x),
                self.max.y.max(other.max.y),
                self.max.z.max(other.max.z),
            ),
        }
    }

    /// The overlap of two boxes, `None` when they are disjoint (`Intersect`).
    #[must_use]
    pub fn intersect(self, other: Self) -> Option<Self> {
        if self.are_disjoint(&other, 0.0) {
            return None;
        }
        Some(Self {
            min: Point3::new(
                self.min.x.max(other.min.x),
                self.min.y.max(other.min.y),
                self.min.z.max(other.min.z),
            ),
            max: Point3::new(
                self.max.x.min(other.max.x),
                self.max.y.min(other.max.y),
                self.max.z.min(other.max.z),
            ),
        })
    }

    /// Squared diagonal of the union: the largest squared distance between two
    /// points of the two boxes (`MaximumDistanceSquared`).
    #[must_use]
    pub fn max_distance_squared(&self, other: &Self) -> f64 {
        let u = self.union(*other);
        (u.max - u.min).length_squared()
    }

    /// Smallest squared distance between a point of each box; `0` when they touch
    /// or overlap (`MinimumDistanceSquared`).
    #[must_use]
    pub fn min_distance_squared(&self, other: &Self) -> f64 {
        let mut d = Vec3::ZERO;
        if self.min.x > other.max.x {
            d.x += self.min.x - other.max.x;
        }
        if self.max.x < other.min.x {
            d.x += other.min.x - self.max.x;
        }
        if self.min.y > other.max.y {
            d.y += self.min.y - other.max.y;
        }
        if self.max.y < other.min.y {
            d.y += other.min.y - self.max.y;
        }
        if self.min.z > other.max.z {
            d.z += self.min.z - other.max.z;
        }
        if self.max.z < other.min.z {
            d.z += other.min.z - self.max.z;
        }
        d.length_squared()
    }

    /// Square root of [`Extent3d::max_distance_squared`] (`MaximumDistance`).
    #[must_use]
    pub fn max_distance(&self, other: &Self) -> f64 {
        sqrt(self.max_distance_squared(other))
    }

    /// (not upstream) The largest distance from `p` to a point of the box: the distance to
    /// the farthest of its eight corners, which on each axis is the bound farther from `p`.
    #[must_use]
    pub fn max_distance_to_point(&self, p: Point3) -> f64 {
        let far = |lo: f64, hi: f64, x: f64| {
            if (x - lo).abs() >= (hi - x).abs() {
                x - lo
            } else {
                hi - x
            }
        };
        Vec3::new(
            far(self.min.x, self.max.x, p.x),
            far(self.min.y, self.max.y, p.y),
            far(self.min.z, self.max.z, p.z),
        )
        .length()
    }

    /// (not upstream) How far the box reaches from `origin` along `direction`, `[low, high]`:
    /// the least and the greatest [`Vec3::distance_along`] of its points, as many lengths as
    /// `direction` is long. Those are corners; on each axis the bound farther along `direction`
    /// adds to `high` and the other to `low`.
    #[must_use]
    pub fn span_along(&self, origin: Point3, direction: Vec3) -> [f64; 2] {
        let (mut low, mut high) = (0.0, 0.0);
        for axis in Axis::ALL {
            let d = direction[axis];
            let ends = [
                (self.min_at(axis) - origin[axis]) * d,
                (self.max_at(axis) - origin[axis]) * d,
            ];
            low += ends[0].min(ends[1]);
            high += ends[0].max(ends[1]);
        }
        [low, high]
    }

    /// Square root of [`Extent3d::min_distance_squared`] (`MinimumDistance`).
    #[must_use]
    pub fn min_distance(&self, other: &Self) -> f64 {
        sqrt(self.min_distance_squared(other))
    }

    /// Every bounded side moved outwards by `factor` times the box size on that axis;
    /// unbounded ends stay (`ExpandRelative` with `factor >= 0`; below zero or NaN leaves the
    /// box).
    #[must_use]
    pub fn expanded_relative(self, factor: f64) -> Self {
        Self::from_intervals(
            self.x_interval().expanded_relative(factor),
            self.y_interval().expanded_relative(factor),
            self.z_interval().expanded_relative(factor),
        )
    }

    /// Every bounded side moved outwards by `d`; unbounded ends stay (`ExpandAbsolute` with
    /// `d >= 0`). A `d` below zero or NaN leaves the box: shrinking is [`Extent3d::shrunk`].
    #[must_use]
    pub fn expanded(self, d: f64) -> Self {
        Self::from_intervals(
            self.x_interval().expanded(d),
            self.y_interval().expanded(d),
            self.z_interval().expanded(d),
        )
    }

    /// Every bounded side moved inwards by `d` (`ExpandAbsolute(-d)`), `None` when that
    /// inverts an axis.
    #[must_use]
    pub fn shrunk(self, d: f64) -> Option<Self> {
        Some(Self::from_intervals(
            self.x_interval().shrunk(d)?,
            self.y_interval().shrunk(d)?,
            self.z_interval().shrunk(d)?,
        ))
    }

    /// The diagonal vector `max - min` (`GetSize`).
    #[must_use]
    pub fn size(&self) -> Vec3 {
        Vec3::new(
            self.max.x - self.min.x,
            self.max.y - self.min.y,
            self.max.z - self.min.z,
        )
    }

    /// The eight corners in upstream's fixed order
    /// `000 010 100 110 001 011 101 111` (digits are x, y, z) (`GetCorners`).
    #[must_use]
    pub fn corners(&self) -> [Point3; 8] {
        [
            self.evaluate(0.0, 0.0, 0.0),
            self.evaluate(0.0, 1.0, 0.0),
            self.evaluate(1.0, 0.0, 0.0),
            self.evaluate(1.0, 1.0, 0.0),
            self.evaluate(0.0, 0.0, 1.0),
            self.evaluate(0.0, 1.0, 1.0),
            self.evaluate(1.0, 0.0, 1.0),
            self.evaluate(1.0, 1.0, 1.0),
        ]
    }

    /// The twelve edges as 24 end points; points `2k` and `2k+1` bound edge `k`
    /// (`GetEdges`; upstream order).
    #[must_use]
    pub fn edges(&self) -> [Point3; 24] {
        const N: [[f64; 3]; 24] = [
            // x == 0 plane
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0],
            // x == 1 plane
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
            // edges from x == 0 to x == 1
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ];
        N.map(|[x, y, z]| self.evaluate(x, y, z))
    }

    /// A point and inward normal for each of the six bounding planes: the min
    /// corner with `+x, +y, +z`, then the max corner with `-x, -y, -z` (`GetPlanes`).
    #[must_use]
    pub fn planes(&self) -> ([Point3; 6], [Vec3; 6]) {
        (
            [self.min, self.min, self.min, self.max, self.max, self.max],
            [
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(-1.0, 0.0, 0.0),
                Vec3::new(0.0, -1.0, 0.0),
                Vec3::new(0.0, 0.0, -1.0),
            ],
        )
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

    /// `max.z - min.z` (`ZLength`).
    #[must_use]
    pub fn z_length(&self) -> f64 {
        self.max.z - self.min.z
    }

    /// The signed distance along the unit `plane_unit_normal` from the plane through
    /// `plane_point` to the nearest corner of the box, `0` when corners lie on both
    /// sides of (or in) the plane (`DistanceToPlane`).
    #[must_use]
    pub fn distance_to_plane(&self, plane_point: Point3, plane_unit_normal: Vec3) -> f64 {
        let mut min_dist = BIG_DOUBLE;
        let mut x = self.min.x;
        for _ in 0..2 {
            let mut y = self.min.y;
            for _ in 0..2 {
                let mut z = self.min.z;
                for _ in 0..2 {
                    let this_pt = Point3::new(x, y, z);
                    let this_dist = plane_unit_normal.dot(this_pt - plane_point);
                    if min_dist < BIG_DOUBLE - 1.0 && this_dist * min_dist <= 0.0 {
                        return 0.0; // opposite sides of (or in) plane
                    }
                    if this_dist.abs() < min_dist.abs() {
                        min_dist = this_dist;
                    }
                    z = self.max.z;
                }
                y = self.max.y;
            }
            x = self.max.x;
        }
        min_dist
    }

    /// Centre and radii of the enclosing and the inscribed centred spheres
    /// (`ComputeSphereBound`).
    #[must_use]
    pub fn sphere_bound(&self) -> SphereBound {
        let center = self.evaluate(0.5, 0.5, 0.5);
        let corner = self.evaluate(1.0, 1.0, 1.0);
        let radius = corner.distance(center);
        let dx = (center.x - corner.x).abs();
        let dy = (center.y - corner.y).abs();
        let dz = (center.z - corner.z).abs();
        let d = dx.min(dy);
        SphereBound {
            center,
            radius,
            min_radius: d.min(dz),
        }
    }

    /// The cone of directions from points of `self` to points of `other`: an axis
    /// and a half angle in radians (`ComputeConeBoundTo`).
    ///
    /// When the boxes overlap (the centre of `self` lies in the bounding sphere of
    /// the grown `other`) the angle is `PI` and the axis is the unit vector between
    /// the centres (or `+z` if they coincide); otherwise the axis is the
    /// (non-unit) vector between the centres.
    pub fn cone_bound_to(&self, other: &Self) -> Result<(Vec3, f64)> {
        let size = self.size();
        let half = Vec3::new(size.x / 2.0, size.y / 2.0, size.z / 2.0);
        let temp = Self {
            min: other.min - half,
            max: other.max + half,
        };
        let sphere = temp.sphere_bound();
        let (sph_cent, sph_rad) = (sphere.center, sphere.radius);
        let this_cent = self.evaluate(0.5, 0.5, 0.5);
        let vec = sph_cent - this_cent;
        let dist = vec.length();
        let scale = 1.0 + sph_cent.max_dimension();
        if dist < sph_rad + EFF_ZERO * scale {
            let axis = if vec.length_squared() > EFF_ZERO * scale {
                // `Unitize` fails only below EFF_ZERO_SQ, which the test above excludes
                // (`scale >= 1`, so the squared length exceeds SM_EFF_ZERO; a NaN fails the
                // test): upstream's SM_ERR exit is unreachable and not ported, and the
                // non-failing unitize gives the same vector here.
                vec.normalized().map_or(Vec3::Z, UnitVec3::as_vec)
            } else {
                Vec3::new(0.0, 0.0, 1.0)
            };
            Ok((axis, PI))
        } else {
            Ok((vec, asin(sph_rad / dist)))
        }
    }

    /// True when the boxes are separated by more than `tol` on some axis
    /// (`AreDisjoint`).
    #[must_use]
    pub fn are_disjoint(&self, other: &Self, tol: f64) -> bool {
        self.min.x > other.max.x + tol
            || self.min.y > other.max.y + tol
            || self.min.z > other.max.z + tol
            || self.max.x < other.min.x - tol
            || self.max.y < other.min.y - tol
            || self.max.z < other.min.z - tol
    }

    /// True when `self` lies inside `other` grown by `tol` (`IsContainedBy`).
    #[must_use]
    pub fn is_contained_by(&self, other: &Self, tol: f64) -> bool {
        !(self.min.x < other.min.x - tol
            || self.min.y < other.min.y - tol
            || self.min.z < other.min.z - tol
            || self.max.x > other.max.x + tol
            || self.max.y > other.max.y + tol
            || self.max.z > other.max.z + tol)
    }

    /// True when no side is longer than `tol` (`IsPointSized`; upstream's default
    /// `tol` is `EFF_ZERO`).
    #[must_use]
    pub fn is_point_sized(&self, tol: f64) -> bool {
        !((self.max.x - self.min.x).abs() > tol
            || (self.max.y - self.min.y).abs() > tol
            || (self.max.z - self.min.z).abs() > tol)
    }

    /// True when `p` lies inside the box grown by `tol` (`ContainsPoint3d`).
    #[must_use]
    pub fn contains_point(&self, p: Point3, tol: f64) -> bool {
        !(p.x < self.min.x - tol
            || p.y < self.min.y - tol
            || p.z < self.min.z - tol
            || p.x > self.max.x + tol
            || p.y > self.max.y + tol
            || p.z > self.max.z + tol)
    }

    /// Upstream `ContainsLineSeg3d`: true when **either** end point is in the box
    /// grown by `tol` (upstream ORs the two point tests although its comment says
    /// both; kept for fidelity).
    #[must_use]
    pub fn contains_line_seg(&self, start: Point3, end: Point3, tol: f64) -> bool {
        let a = self.contains_point(start, tol);
        let b = self.contains_point(end, tol);
        a | b
    }

    /// Whether `p` is within `tol` of a side, and the inward "binormal" `(du, dv, dw)`
    /// with `+1` near a min side, `-1` near a max side (min checked first), `0`
    /// otherwise (`IsPoint3dOnBoundary`; upstream's default `tol` is `EFF_ZERO`).
    #[must_use]
    pub fn is_point_on_boundary(&self, p: Point3, tol: f64) -> (bool, Vec3) {
        let mut on = false;
        let axis = |pc: f64, lo: f64, hi: f64, on: &mut bool| {
            if (pc - lo).abs() <= tol {
                *on = true;
                1.0
            } else if (pc - hi).abs() <= tol {
                *on = true;
                -1.0
            } else {
                0.0
            }
        };
        let du = axis(p.x, self.min.x, self.max.x, &mut on);
        let dv = axis(p.y, self.min.y, self.max.y, &mut on);
        let dw = axis(p.z, self.min.z, self.max.z, &mut on);
        (on, Vec3::new(du, dv, dw))
    }

    /// Clips the line `point + t * dir` against the box (any mix of unbounded
    /// intervals is allowed) (`IntersectLine`).
    ///
    /// Fails when `dir` is shorter than `EFF_ZERO`, when a bound of the box is NaN, or when
    /// `point` or `dir` is not finite: no clip of an undefined box or line is defined.
    pub fn intersect_line(&self, point: Point3, dir: Vec3) -> Result<LineHit> {
        let nan_box = [self.min, self.max]
            .iter()
            .any(|c| c.x.is_nan() || c.y.is_nan() || c.z.is_nan());
        let line = [point.x, point.y, point.z, dir.x, dir.y, dir.z];
        if nan_box || !line.iter().all(|c| c.is_finite()) || dir.length_squared() < EFF_ZERO_SQ {
            return Err(NumError::InvalidInput {
                operation: "geom::extent3d::Extent3d::intersect_line",
                reason: "zero-length line direction, or an undefined box or line",
            });
        }
        let (tmin_x, tmax_x) = slab(self.min.x, self.max.x, point.x, dir.x);
        let (tmin_y, tmax_y) = slab(self.min.y, self.max.y, point.y, dir.y);
        let (tmin_z, tmax_z) = slab(self.min.z, self.max.z, point.z, dir.z);
        let d_min = tmin_z.max(tmin_x).max(tmin_y);
        let d_max = tmax_z.min(tmax_x).min(tmax_y);

        // Upstream note [B70]: in an extreme grazing case the midpoint can be far
        // from the box although the line passes within tolerance, so the ends are
        // checked too when the midpoint is outside.
        let mid = point + ((d_min + d_max) / 2.0) * dir;
        let tol = EFF_ZERO * (1.0 + mid.max_dimension());

        if self.contains_point(mid, tol) {
            if (d_min - d_max).abs() < EFF_ZERO {
                // Average so the interval is exactly degenerate and never inverted.
                return Ok(LineHit::Touch {
                    t: (d_min + d_max) / 2.0,
                });
            }
            if d_min < d_max {
                return Ok(LineHit::Through {
                    enter: d_min,
                    exit: d_max,
                });
            }
            return Ok(LineHit::Miss);
        }
        if self.contains_point(point + d_min * dir, tol) {
            return Ok(LineHit::Touch { t: d_min });
        }
        if self.contains_point(point + d_max * dir, tol) {
            return Ok(LineHit::Touch { t: d_max });
        }
        Ok(LineHit::Miss)
    }

    /// (not upstream) Whether the forward ray `origin + t * dir` (`t >= 0`) meets this finite box
    /// inflated by a conservative margin derived from its diagonal and `tolerance`.
    ///
    /// Conservative on non-finite inputs: returns `true` when the box, `origin`, `dir`, or the
    /// padded bounds are non-finite or when any slab endpoint is `NaN`, so callers using this for
    /// culling never skip a box prematurely.
    #[must_use]
    pub fn meets_ray(&self, origin: Point3, dir: Vec3, tolerance: f64) -> bool {
        if !self.min.is_finite() || !self.max.is_finite() || !origin.is_finite() || !dir.is_finite()
        {
            return true;
        }
        let tol_pad = if tolerance.is_finite() && tolerance > 0.0 {
            tolerance * 10.0
        } else {
            0.0
        };
        let pad = (self.size().length() * 1e-6).max(tol_pad).max(1e-4);
        if !pad.is_finite() {
            return true;
        }
        let lo = Vec3::new(self.min.x - pad, self.min.y - pad, self.min.z - pad);
        let hi = Vec3::new(self.max.x + pad, self.max.y + pad, self.max.z + pad);
        if !lo.is_finite() || !hi.is_finite() {
            return true;
        }
        let (mut t_lo, mut t_hi) = (0.0_f64, f64::INFINITY);
        for (oo, dd, min, max) in [
            (origin.x, dir.x, lo.x, hi.x),
            (origin.y, dir.y, lo.y, hi.y),
            (origin.z, dir.z, lo.z, hi.z),
        ] {
            if dd == 0.0 {
                if !(min..=max).contains(&oo) {
                    return false;
                }
                continue;
            }
            let (a, c) = ((min - oo) / dd, (max - oo) / dd);
            if a.is_nan() || c.is_nan() {
                return true;
            }
            t_lo = t_lo.max(a.min(c));
            t_hi = t_hi.min(a.max(c));
            if t_lo > t_hi {
                return false;
            }
        }
        t_lo <= t_hi
    }

    /// Classification of the x, y and z intervals (the out-param of `IsBounded`).
    #[must_use]
    pub fn boundary_types(&self) -> [BoundaryType; 3] {
        [
            BoundaryType::classify(self.min.x, self.max.x),
            BoundaryType::classify(self.min.y, self.max.y),
            BoundaryType::classify(self.min.z, self.max.z),
        ]
    }

    /// True when no interval end is an unbounded marker (`IsBounded`).
    #[must_use]
    pub fn is_bounded(&self) -> bool {
        self.boundary_types() == [BoundaryType::Bounded; 3]
    }

    /// True when some interval is bounded at one end or more (`AnyBounds`).
    #[must_use]
    pub fn any_bounds(&self) -> bool {
        self.boundary_types()
            .iter()
            .any(|t| *t != BoundaryType::Unbounded)
    }

    /// A bounded extent approximating this one: unbounded ends are replaced using
    /// `half_size` and the optional `center` (`[0, 0, 0]` if `None`); a bounded
    /// extent is returned unchanged (`ApproximateUnbounded` without the expanded
    /// output; upstream's default `half_size` is
    /// [`BOUNDED_INFINITE_PARAM`](crate::math::BOUNDED_INFINITE_PARAM)).
    #[must_use]
    pub fn approximate_unbounded(&self, center: Option<Point3>, half_size: f64) -> Self {
        self.approximate_unbounded_expanded(center, half_size).0
    }

    /// [`Extent3d::approximate_unbounded`] plus upstream's optional second extent,
    /// which pushes the replaced ends out by a further `half_size / 12` (used for
    /// graphics). The second value is `None` where upstream's `SetMinMax` rejects it
    /// (an inverted interval), leaving its output untouched.
    #[must_use]
    pub fn approximate_unbounded_expanded(
        &self,
        center: Option<Point3>,
        half_size: f64,
    ) -> (Self, Option<Self>) {
        let c = center.unwrap_or(Point3::ORIGIN);
        let types = self.boundary_types();
        let bounded = types == [BoundaryType::Bounded; 3];
        let (mut lo, mut hi) = (self.min, self.max);
        if !bounded {
            let h = half_size;
            let [tx, ty, tz] = types;
            (lo.x, hi.x) = approx_axis(tx, lo.x, hi.x, center.map(|p| p.x), c.x, h);
            (lo.y, hi.y) = approx_axis(ty, lo.y, hi.y, center.map(|p| p.y), c.y, h);
            (lo.z, hi.z) = approx_axis(tz, lo.z, hi.z, center.map(|p| p.z), c.z, h);
        }

        let (mut lo_inc, mut hi_inc) = (lo, hi);
        if !bounded {
            let inc = half_size / 12.0;
            let grows_min =
                |t: BoundaryType| matches!(t, BoundaryType::UnboundedMin | BoundaryType::Unbounded);
            let grows_max =
                |t: BoundaryType| matches!(t, BoundaryType::UnboundedMax | BoundaryType::Unbounded);
            let [tx, ty, tz] = types;
            if grows_min(tx) {
                lo_inc.x -= inc;
            }
            if grows_max(tx) {
                hi_inc.x += inc;
            }
            if grows_min(ty) {
                lo_inc.y -= inc;
            }
            if grows_max(ty) {
                hi_inc.y += inc;
            }
            if grows_min(tz) {
                lo_inc.z -= inc;
            }
            if grows_max(tz) {
                hi_inc.z += inc;
            }
        }
        (Self::spanning(lo, hi), Self::from_corners(lo_inc, hi_inc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::BOUNDED_INFINITE_PARAM;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3::new(x, y, z)
    }

    fn bx(x_min: f64, y_min: f64, z_min: f64, x_max: f64, y_max: f64, z_max: f64) -> Extent3d {
        Extent3d::from_bounds(x_min, y_min, z_min, x_max, y_max, z_max).unwrap()
    }

    /// The farthest point of a box from a point is its farthest corner, from inside, from
    /// outside and from a corner itself.
    #[test]
    fn the_largest_distance_to_a_point_is_to_the_farthest_corner() {
        let e = Extent3d::spanning(p(-1.0, 0.0, 2.0), p(3.0, 5.0, 4.0));
        for at in [
            p(0.0, 1.0, 3.0),
            p(10.0, -2.0, 0.0),
            p(-1.0, 0.0, 2.0),
            p(1.0, 2.5, 3.0),
        ] {
            let want = e
                .corners()
                .iter()
                .map(|c| (*c - at).length())
                .fold(0.0, f64::max);
            assert!((e.max_distance_to_point(at) - want).abs() < 1e-12, "{at:?}");
        }
        assert!((e.max_distance_to_point(p(-1.0, 0.0, 2.0)) - e.max_distance(&e)).abs() < 1e-12);
    }

    /// A box's reach along a direction runs from its lowest corner to its highest, each axis
    /// taking the bound on its side, along an axis, a slanted direction, one pointing back, a
    /// longer one and from an origin inside the box.
    #[test]
    fn the_span_along_a_direction_runs_from_the_lowest_corner_to_the_highest() {
        let e = Extent3d::spanning(p(-1.0, 0.0, 2.0), p(3.0, 5.0, 4.0));
        for (origin, direction, want) in [
            (p(0.0, 0.0, 0.0), v(0.0, 0.0, 1.0), [2.0, 4.0]),
            (p(0.0, 0.0, 0.0), v(0.6, 0.0, -0.8), [-3.8, 0.2]),
            (p(1.0, 1.0, 1.0), v(-1.0, 0.0, 0.0), [-2.0, 2.0]),
            (p(1.0, 1.0, 1.0), v(1.0, 2.0, 2.0), [-2.0, 16.0]),
        ] {
            let got = e.span_along(origin, direction);
            let corners = e.corners().map(|c| c.distance_along(origin, direction));
            let lowest = corners.iter().copied().fold(f64::INFINITY, f64::min);
            let highest = corners.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            for (a, b) in [
                (got[0], want[0]),
                (got[1], want[1]),
                (lowest, want[0]),
                (highest, want[1]),
            ] {
                assert!((a - b).abs() < 1e-12, "{direction:?}: {got:?} vs {want:?}");
            }
        }
    }

    /// upstream `my_test_extent3d` (`tests/prog_test/src/smCurveTest.cpp`).
    #[test]
    fn upstream_my_test_extent3d() {
        let (v1, v2) = (p(1.0, 1.0, 1.0), p(2.0, 2.0, 1.0));
        let e1 = Extent3d::spanning(v1, v2);
        assert_eq!(e1.min(), v1);
        assert_eq!(Extent3d::from_point(v2).max(), v2);
        assert_eq!(Hull3d::EMPTY.finish(), None);
        let (v3, v4, v5) = (p(6.0, 6.0, 6.0), p(5.0, 5.0, 5.0), p(7.0, 7.0, 7.0));
        let e4 = Extent3d::hull([v3, v4, v5]).unwrap();
        assert_eq!((e4.min(), e4.max()), (v4, v5));
        let e5 = Extent3d::from_corners(v4, v5).unwrap();
        let e6 = Extent3d::spanning(p(-7.0, -7.0, -7.0), p(-5.0, -5.0, -5.0));
        assert!(e5.are_disjoint(&e6, 0.0));
        let u = e5.union(e6);
        assert_eq!((u.min(), u.max()), (p(-7.0, -7.0, -7.0), v5));
        let e7 = Extent3d::spanning(v4, v5);
        assert_eq!(e4.intersect(e7), Some(e7));
        assert_eq!(e5.intersect(e6), None);
    }

    #[test]
    fn swap_and_replaced_intervals() {
        let e = Extent3d::spanning(p(3.0, -1.0, 2.0), p(1.0, 4.0, -2.0));
        assert_eq!(e.min(), p(1.0, -1.0, -2.0));
        assert_eq!(e.max(), p(3.0, 4.0, 2.0));
        assert_eq!(
            Extent3d::from_corners(p(1.0, 0.0, 0.0), p(0.0, 1.0, 1.0)),
            None
        );
        assert_eq!(e.x_interval().with_min(5.0), None);
        let x = e.x_interval().with_max(10.0).and_then(|i| i.with_min(5.0));
        let f = e.with_x_interval(x.unwrap());
        assert_eq!(f.y_interval().with_max(-3.0), None);
        let f = f.with_y_interval(f.y_interval().with_min(4.0).unwrap());
        assert_eq!(f.min(), p(5.0, 4.0, -2.0));
        assert_eq!(f.min_at(Axis::Y), 4.0);
        assert_eq!(f.max_at(Axis::Z), 2.0);
    }

    #[test]
    fn evaluate_exact_ends_and_corners() {
        let e = Extent3d::spanning(p(0.1, 0.2, 0.3), p(0.7, 1.9, 2.3));
        assert_eq!(e.evaluate(1.0, 1.0, 1.0), e.max());
        assert_eq!(e.corners()[3], p(0.7, 1.9, 0.3));
        let edges = e.edges();
        assert_eq!(edges[23], p(0.7, 1.9, 0.3));
        let (pts, nrm) = e.planes();
        assert_eq!((pts[4], nrm[4]), (e.max(), v(0.0, -1.0, 0.0)));
    }

    #[test]
    fn distances() {
        let a = Extent3d::spanning(p(0.0, 0.0, 0.0), p(1.0, 1.0, 1.0));
        let b = Extent3d::spanning(p(4.0, 5.0, 1.0), p(6.0, 6.0, 2.0));
        assert_eq!(a.min_distance_squared(&b), 9.0 + 16.0);
        assert_eq!(a.min_distance(&b), 5.0);
        assert_eq!(a.max_distance_squared(&b), 36.0 + 36.0 + 4.0);
        assert_eq!(a.min_distance(&a), 0.0);
    }

    #[test]
    fn line_intersections() {
        let e = Extent3d::spanning(p(0.0, 0.0, 0.0), p(1.0, 1.0, 1.0));
        let hit = e
            .intersect_line(p(-1.0, 0.5, 0.5), v(1.0, 0.0, 0.0))
            .unwrap();
        assert_eq!(
            hit,
            LineHit::Through {
                enter: 1.0,
                exit: 2.0
            }
        );
        assert_eq!(
            e.intersect_line(p(-1.0, 2.0, 0.5), v(1.0, 0.0, 0.0))
                .unwrap(),
            LineHit::Miss
        );
        let graze = e
            .intersect_line(p(0.0, 0.0, 0.0), v(1.0, -1.0, 0.0))
            .unwrap();
        assert_eq!(graze.count(), 1);
        assert!(e.intersect_line(p(0.0, 0.0, 0.0), Vec3::ZERO).is_err());
    }

    #[test]
    fn bounded_and_approximation() {
        let e = Extent3d::UNBOUNDED;
        assert!(!e.is_bounded() && !e.any_bounds());
        let a = e.approximate_unbounded(None, BOUNDED_INFINITE_PARAM);
        assert_eq!(a.min(), p(-33.0, -33.0, -33.0));
        let half = bx(-INFINITE_PARAMETER, 0.0, 0.0, 2.0, 1.0, 1.0);
        assert_eq!(
            half.boundary_types(),
            [
                BoundaryType::UnboundedMin,
                BoundaryType::Bounded,
                BoundaryType::Bounded
            ]
        );
        assert!(half.any_bounds());
        let (a, x) = half.approximate_unbounded_expanded(None, 10.0);
        assert_eq!((a.x_min(), a.x_max()), (-18.0, 2.0));
        let x = x.unwrap();
        assert_eq!(x.x_min(), -18.0 - 10.0 / 12.0);
        // A bounded extent is copied unchanged.
        let b = Extent3d::spanning(p(0.0, 0.0, 0.0), p(1.0, 2.0, 3.0));
        assert_eq!(b.approximate_unbounded(None, 1.0), b);
    }

    #[test]
    fn sphere_and_cone() {
        let a = Extent3d::spanning(p(0.0, 0.0, 0.0), p(2.0, 2.0, 2.0));
        let s = a.sphere_bound();
        assert_eq!(s.center, p(1.0, 1.0, 1.0));
        assert_eq!(s.min_radius, 1.0);
        let (axis, ang) = a.cone_bound_to(&a).unwrap();
        assert_eq!((axis, ang), (Vec3::Z, PI));
        let far = Extent3d::spanning(p(100.0, 0.0, 0.0), p(102.0, 2.0, 2.0));
        let (axis, ang) = a.cone_bound_to(&far).unwrap();
        assert_eq!(axis, v(100.0, 0.0, 0.0));
        assert!(ang > 0.0 && ang < 0.1);
    }

    #[test]
    fn predicates() {
        let e = Extent3d::spanning(p(0.0, 0.0, 0.0), p(1.0, 1.0, 1.0));
        assert!(e.contains_point(p(1.0 + 1e-13, 0.5, 0.5), 1e-12));
        assert!(!e.contains_point(p(1.1, 0.5, 0.5), 0.0));
        // Upstream bug kept: one end inside is enough.
        assert!(e.contains_line_seg(p(0.5, 0.5, 0.5), p(9.0, 9.0, 9.0), 0.0));
        let (on, bn) = e.is_point_on_boundary(p(0.0, 1.0, 0.5), EFF_ZERO);
        assert!(on);
        assert_eq!(bn, v(1.0, -1.0, 0.0));
        assert!(Extent3d::from_point(p(1.0, 1.0, 1.0)).is_point_sized(EFF_ZERO));
        assert!(e.is_contained_by(&e, 0.0));
        let g = e.expanded(1.0);
        assert_eq!(g.min(), p(-1.0, -1.0, -1.0));
        assert_eq!(e.shrunk(0.6), None);
        assert_eq!(e.shrunk(0.25).map(|b| b.min()), Some(p(0.25, 0.25, 0.25)));
        let g = g.expanded_relative(0.5);
        assert_eq!(g.max(), p(3.5, 3.5, 3.5));
        assert_eq!(g.max_length(), 6.0);
        assert_eq!(g.max_dimension(), 3.5);
        assert_eq!(e.mid(), p(0.5, 0.5, 0.5));
    }

    /// Upstream's literal two-step `SmExtent3d::ApproximateUnbounded` axis evaluation
    /// (`SmExtent3d.cpp:1784-1849`), which mutates `sMin` in place before computing `sMax`.
    fn upstream_mutated_approx_axis(
        ty: BoundaryType,
        min: f64,
        max: f64,
        center: Option<f64>,
        c: f64,
        h: f64,
    ) -> (f64, f64) {
        let max_near_center = center.is_none_or(|pc| max < pc + h);
        let min_near_center = center.is_none_or(|pc| min > pc - h);
        let new_min = match (ty, center) {
            (BoundaryType::UnboundedMin, Some(pc)) if !max_near_center => pc - h,
            (BoundaryType::UnboundedMin, _) => max - 2.0 * h,
            (BoundaryType::UnboundedMax, Some(pc)) if !min_near_center => pc - h,
            (BoundaryType::Unbounded, _) => c - h,
            (BoundaryType::UnboundedMax | BoundaryType::Bounded | BoundaryType::Unknown, _) => min,
        };
        let new_min_near_center = center.is_none_or(|pc| new_min > pc - h);
        let new_max = match (ty, center) {
            (BoundaryType::UnboundedMin, Some(pc)) if !max_near_center => pc + h,
            (BoundaryType::UnboundedMax, Some(pc)) if !new_min_near_center => pc + h,
            (BoundaryType::UnboundedMax, _) => new_min + 2.0 * h,
            (BoundaryType::Unbounded, _) => c + h,
            (BoundaryType::UnboundedMin | BoundaryType::Bounded | BoundaryType::Unknown, _) => max,
        };
        (new_min, new_max)
    }

    fn same_bits(a: f64, b: f64) -> bool {
        (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
    }

    #[test]
    fn wr4_unbounded_max_center_regimes_and_expanded() {
        // 1. center = None: keeps original min, max = min + 2*h, expanded pushes max by h/12.
        let e = bx(-5.0, 1.0, -2.0, INFINITE_PARAMETER, 3.0, 4.0);
        let (a, x) = e.approximate_unbounded_expanded(None, 10.0);
        assert_eq!(a, bx(-5.0, 1.0, -2.0, 15.0, 3.0, 4.0));
        assert_eq!(e.approximate_unbounded(None, 10.0), a);
        assert_eq!(x, Some(bx(-5.0, 1.0, -2.0, 15.0 + 10.0 / 12.0, 3.0, 4.0)));

        // 2. center = Some(pc) with min > pc - h (-5.0 > -10.0): keeps original min.
        let (a, x) = e.approximate_unbounded_expanded(Some(p(0.0, 50.0, 50.0)), 10.0);
        assert_eq!(a, bx(-5.0, 1.0, -2.0, 15.0, 3.0, 4.0));
        assert_eq!(x, Some(bx(-5.0, 1.0, -2.0, 15.0 + 10.0 / 12.0, 3.0, 4.0)));

        // 3. center = Some(pc) with min == pc - h (-10.0 == -10.0): focuses on [pc - h, pc + h].
        let tie = bx(-10.0, 1.0, -2.0, INFINITE_PARAMETER, 3.0, 4.0);
        let (a, x) = tie.approximate_unbounded_expanded(Some(p(0.0, 50.0, 50.0)), 10.0);
        assert_eq!(a, bx(-10.0, 1.0, -2.0, 10.0, 3.0, 4.0));
        assert_eq!(x, Some(bx(-10.0, 1.0, -2.0, 10.0 + 10.0 / 12.0, 3.0, 4.0)));

        // 4. center = Some(pc) with min < pc - h (-100.0 < -6.0): focuses on [pc - h, pc + h].
        let deep = bx(-100.0, 1.0, -2.0, INFINITE_PARAMETER, 3.0, 4.0);
        let (a, x) = deep.approximate_unbounded_expanded(Some(p(4.0, 50.0, 50.0)), 10.0);
        assert_eq!(a, bx(-6.0, 1.0, -2.0, 14.0, 3.0, 4.0));
        assert_eq!(x, Some(bx(-6.0, 1.0, -2.0, 14.0 + 10.0 / 12.0, 3.0, 4.0)));

        // 5. 1-ULP witness where `(pc - h) + 2*h != pc + h` (`pc = 1.0, h = 0.3`):
        // because `(pc - h) > (pc - h)` is false, both upstream's mutated-`sMin`
        // expression and `approx_axis` select `pc + h` directly.
        let (pc, h) = (1.0_f64, 0.3_f64);
        assert_ne!(((pc - h) + 2.0 * h).to_bits(), (pc + h).to_bits());
        let (lo_dir, hi_dir) = approx_axis(
            BoundaryType::UnboundedMax,
            -10.0,
            INFINITE_PARAMETER,
            Some(pc),
            pc,
            h,
        );
        let (lo_mut, hi_mut) = upstream_mutated_approx_axis(
            BoundaryType::UnboundedMax,
            -10.0,
            INFINITE_PARAMETER,
            Some(pc),
            pc,
            h,
        );
        assert_eq!(lo_dir.to_bits(), (pc - h).to_bits());
        assert_eq!(hi_dir.to_bits(), (pc + h).to_bits());
        assert_eq!(lo_dir.to_bits(), lo_mut.to_bits());
        assert_eq!(hi_dir.to_bits(), hi_mut.to_bits());
    }

    #[test]
    fn wr4_mixed_boundary_types_and_negative_half_size() {
        // X: UnboundedMin [-inf, 2], Y: UnboundedMax [-50, +inf], Z: Unbounded [-inf, +inf].
        let mixed = bx(
            -INFINITE_PARAMETER,
            -50.0,
            -INFINITE_PARAMETER,
            2.0,
            INFINITE_PARAMETER,
            INFINITE_PARAMETER,
        );
        let (a0, x0) = mixed.approximate_unbounded_expanded(None, 12.0);
        assert_eq!(a0, bx(-22.0, -50.0, -12.0, 2.0, -26.0, 12.0));
        assert_eq!(x0, Some(bx(-23.0, -50.0, -13.0, 2.0, -25.0, 13.0)));

        let (a1, x1) = mixed.approximate_unbounded_expanded(Some(p(-20.0, 5.0, 3.0)), 12.0);
        assert_eq!(a1, bx(-32.0, -7.0, -9.0, -8.0, 17.0, 15.0));
        assert_eq!(x1, Some(bx(-33.0, -7.0, -10.0, -8.0, 18.0, 16.0)));

        // Bounded extent is returned unchanged even when a center is supplied.
        let bounded = Extent3d::spanning(p(-2.0, 0.0, 1.0), p(3.0, 4.0, 5.0));
        let (ab, xb) = bounded.approximate_unbounded_expanded(Some(p(100.0, 200.0, 300.0)), 12.0);
        assert_eq!(ab, bounded);
        assert_eq!(xb, Some(bounded));

        // Negative half_size: primary extent swaps per-axis via `spanning`, whereas
        // `SetMinMax` rejects the inverted expanded extent (`None`).
        let umax = bx(3.0, 0.0, 0.0, INFINITE_PARAMETER, 1.0, 1.0);
        let (an0, xn0) = umax.approximate_unbounded_expanded(None, -6.0);
        assert_eq!(an0, bx(-9.0, 0.0, 0.0, 3.0, 1.0, 1.0));
        assert_eq!(xn0, None);

        let (an1, xn1) = umax.approximate_unbounded_expanded(Some(p(0.0, 0.0, 0.0)), -6.0);
        assert_eq!(an1, bx(-6.0, 0.0, 0.0, 6.0, 1.0, 1.0));
        assert_eq!(xn1, None);
    }

    #[test]
    fn wr4_approx_axis_bit_identical_to_mutated_min_across_ieee754() {
        let vals = [
            f64::NEG_INFINITY,
            -1.0e300,
            -INFINITE_PARAMETER,
            -100.0,
            -10.0,
            -1.0,
            -0.3,
            -1.0e-300,
            -0.0,
            0.0,
            1.0e-300,
            0.1,
            0.3,
            1.0 / 3.0,
            1.0,
            10.0,
            100.0,
            INFINITE_PARAMETER,
            1.0e300,
            f64::INFINITY,
            f64::NAN,
        ];
        let centers: Vec<Option<f64>> = core::iter::once(None)
            .chain(vals.into_iter().map(Some))
            .collect();

        for v in vals {
            for &center in &centers {
                let c = center.unwrap_or(0.0);
                for h in vals {
                    for (ty, min, max) in [
                        (BoundaryType::UnboundedMax, v, INFINITE_PARAMETER),
                        (BoundaryType::UnboundedMin, -INFINITE_PARAMETER, v),
                        (
                            BoundaryType::Unbounded,
                            -INFINITE_PARAMETER,
                            INFINITE_PARAMETER,
                        ),
                        (BoundaryType::Bounded, v, 5.0),
                        (BoundaryType::Unknown, v, 5.0),
                    ] {
                        let (lo_a, hi_a) = approx_axis(ty, min, max, center, c, h);
                        let (lo_b, hi_b) = upstream_mutated_approx_axis(ty, min, max, center, c, h);
                        assert!(
                            same_bits(lo_a, lo_b) && same_bits(hi_a, hi_b),
                            "mismatch for {ty:?} min={min} max={max} center={center:?} h={h}: \
                             direct=({lo_a}, {hi_a}) vs mutated=({lo_b}, {hi_b})"
                        );
                    }
                }
            }
        }

        // NaN coordinate / center / half_size through the public API neither panics nor diverges.
        let nan_ext = bx(f64::NAN, 0.0, 0.0, INFINITE_PARAMETER, 1.0, 1.0);
        let (a_nan, x_nan) = nan_ext.approximate_unbounded_expanded(Some(p(2.0, 0.0, 0.0)), 5.0);
        assert_eq!(a_nan, bx(-3.0, 0.0, 0.0, 7.0, 1.0, 1.0));
        assert_eq!(x_nan, Some(bx(-3.0, 0.0, 0.0, 7.0 + 5.0 / 12.0, 1.0, 1.0)));
    }

    #[test]
    fn meets_ray_handles_hits_misses_interior_and_degenerate_boxes() {
        let b = bx(0.0, 0.0, 0.0, 10.0, 10.0, 10.0);
        assert!(b.meets_ray(p(5.0, 5.0, 50.0), v(0.0, 0.0, -1.0), 1e-6));
        assert!(b.meets_ray(p(5.0, 5.0, 50.0), v(0.0, 0.0, -7.0), 1e-6));
        assert!(b.meets_ray(p(5.0, 5.0, 5.0), v(1.0, 0.0, 0.0), 1e-6));
        assert!(b.meets_ray(p(-10.0, -10.0, 5.0), v(1.0, 1.0, 0.0), 1e-6));
        // Grazing just outside the exact face is still kept within the conservative tolerance pad.
        assert!(b.meets_ray(p(10.05, 5.0, 50.0), v(0.0, 0.0, -1.0), 0.01));
        assert!(!b.meets_ray(p(5.0, 5.0, 50.0), v(0.0, 0.0, 1.0), 1e-6));
        assert!(!b.meets_ray(p(25.0, 5.0, 50.0), v(0.0, 0.0, -1.0), 1e-6));
        // Non-finite inputs return true conservatively.
        let nan_box = bx(f64::NAN, 0.0, 0.0, 10.0, 10.0, 10.0);
        assert!(nan_box.meets_ray(p(5.0, 5.0, 50.0), v(0.0, 0.0, -1.0), 1e-6));
        assert!(b.meets_ray(p(f64::NAN, 5.0, 50.0), v(0.0, 0.0, -1.0), 1e-6));
    }
}
