// Moved from solidcore `solidcore/crates/solidcore/src/geom/vector3d.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/inc/SmVector3d.h`,
// `source/SMLib/src/SmVector3d.cpp`, `source/SMLib/src/SmMath.cpp`.
// Changes: `NOZ` and `NOW` moved down from `nurbs/math.rs`; `is_valid_double` private; errors
// are `NumError`; paths. No `SM_UNDEF_DOUBLE` sentinel (`docs/01-architecture.md` §10.1): `UNDEF`,
// `set_uninitialized` and `is_initialized` are gone (an unset vector is `None`), `is_undef` is
// `is_invalid` without the sentinel test, and `approx_eq` loses its two-unset-vectors case;
// the test `upstream_my_test_vector3d` drops its unset-vector checks and
// `operators_and_predicates` its `UNDEF` cases.
// Then the transcendental functions became `fmath`'s (correctly rounded, the same bits on every
// platform) in place of the C library's, so results may differ from the pin's in the last bit.
// Then `component(usize)` (0.0 past 2) and `component_mut` became `Index<Axis>` and
// `IndexMut<Axis>` on `Vec3` and `Point3`. Angle tolerances are radians:
// `is_parallel_to(o, max_angle)`, `is_perpendicular_to(o, max_angle)`, and
// `alignment(o, max_angle) -> Alignment` in place of `parallel_sense`'s `(bool, Option<bool>)`.
//! Three-dimensional vectors, points and unit vectors ([`Vec3`] / [`Point3`] /
//! [`UnitVec3`]): affine arithmetic, lengths,
//! angles, projections, rotation, orthonormal frames and derivatives of unit vectors,
//! plus the guarded scalar math helpers of `SmMath.cpp`.
//!
//! Upstream: `source/SMLib/inc/SmVector3d.h`, `source/SMLib/src/SmVector3d.cpp`,
//! `source/SMLib/src/SmMath.cpp`.
//!
//! | Rust                                         | Upstream                                         |
//! |----------------------------------------------|--------------------------------------------------|
//! | [`Vec3`]                                     | `SmVector3d`                                     |
//! | [`Point3`]                                   | `SmPoint3d` (same class upstream; here distinct) |
//! | [`UnitVec3`]                                 | a unitized `SmVector3d`                          |
//! | [`Vec3::new`]                                | `SmVector3d(double, double, double)`, `Set`      |
//! | [`Vec3::from_array`], `From<[f64; 3]>`       | `SmVector3d(double *)`                           |
//! | [`Vec3::from_nl`]                            | `SmVector3d(double *, bool fromNL)`              |
//! | `Point3 - Point3`                            | `SmVector3d(const SmPoint3d&, const SmPoint3d&)` |
//! | `From<Vec2>`                                 | `SmVector3d(const SmVector2d&)`                  |
//! | `Copy` / assignment, `core::mem::swap`       | copy constructor, `operator=`, `Copy`, `Swap`    |
//! | [`Vec3::is_invalid`]                         | `IsUndef` (without `SM_UNDEF_DOUBLE`)            |
//! | [`Vec3::is_zero`]                            | `IsZero`                                         |
//! | [`Vec3::is_2d`]                              | `Is2d`                                           |
//! | [`Vec3::length_squared`], [`Vec3::length`]   | `LengthSquared`, `Length`                        |
//! | [`Vec3::dot`]                                | `Dot`                                            |
//! | [`Vec3::cross`]                              | `operator*(const SmVector3d&, const SmVector3d&)` |
//! | `Neg`, `Add`, `Sub`                          | `operator-(v)`, `operator+`, `operator-`         |
//! | `Mul<f64>`, `f64 * Vec3`, `Div<f64>`         | `operator*(v, d)`, `operator*(d, v)`, `operator/` |
//! | `AddAssign`, `SubAssign`, `MulAssign`, `DivAssign` | `operator+=`, `operator-=`, `operator*=`, `operator/=` |
//! | [`Vec3::normalized`]                         | `Unitize`                                        |
//! | [`Vec3::triple_product`]                     | `TripleProduct`                                  |
//! | [`Vec3::normal_in_plane`]                    | `NormalInPlane`                                  |
//! | [`Vec3::max_dimension`], [`Vec3::min_dimension`] | `GetMaxDimension`, `GetMinDimension`         |
//! | [`Vec3::is_parallel_to`], [`Vec3::alignment`] | `IsParallelTo`                                  |
//! | [`Vec3::is_perpendicular_to`]                | `IsPerpendicularTo`                              |
//! | [`Vec3::project_to_plane`]                   | `ProjectToPlane`                                 |
//! | [`Point3::project_point_to_plane`]           | `ProjectPointToPlane`                            |
//! | [`Vec3::rotate_project_to_plane`]            | `RotateProjectToPlane`                           |
//! | [`Point3::rotate_project_point_to_plane`]    | `RotateProjectPointToPlane`                      |
//! | [`Vec3::perspective_project_to_plane`]       | `PerspectiveProjectToPlane`                      |
//! | [`Point3::perspective_project_point_to_plane`] | `PerspectiveProjectPointToPlane`                 |
//! | [`Point3::distance`], [`Point3::distance_squared`] | `DistanceBetween`, `DistanceBetweenSquared` |
//! | [`Point3::closer_than`]                      | `CloserThan`                                     |
//! | [`Vec3::multiply`]                           | `Multiply`                                       |
//! | [`Vec3::derivative_of_cross_product`]        | `DerivativeOfCrossProduct`                       |
//! | [`Vec3::solve_two_linear_equations`]         | `SolveTwoLinearEquations`                        |
//! | `v[Axis::X]` ([`Axis`], `Index`/`IndexMut`)   | `operator[] const`, `operator[]`                 |
//! | [`Vec3::approx_eq`]                          | `operator==`                                     |
//! | [`Vec3::is_inside_sector`]                   | `IsInsideSector`                                 |
//! | [`Vec3::make_unit_ortho_vectors`]            | `MakeUnitOrthoVectors`                           |
//! | [`Vec3::rotate_vec_about_axis`]              | `RotateVecAboutAxis`                             |
//! | [`Vec3::unitized_derivative`]                | `UnitizedDerivative`                             |
//! | [`Vec3::unitized_derivative2`]               | `UnitizedDerivative2`                            |
//! | [`Vec3::unitized_derivative3`]               | `UnitizedDerivative3`                            |
//! | [`Vec3::angle_between`]                      | `AngleBetween`                                   |
//! | [`Vec3::ccw_angle_between`]                  | `CCWAngleBetween`                                |
//! | [`Vec3::signed_unit_angle`]                  | not upstream: oriented angle of unit directions   |
//! | [`Vec3::square_turn`]                        | (none)                                           |
//! | [`Vec3::is_colinear_with`]                   | `IsColinearWith`                                 |
//! | [`sqrt`], [`cbrt`], [`acos`], [`asin`]       | `sm_sqrt`, `sm_cbrt`, `sm_acos`, `sm_asin` (`smos_Sqrt`, `smos_CubeRoot`, `smos_ArcCosine`, `smos_ArcSine`) |
//!
//! Deviations:
//! - Out-params and `SmStatus` become return values. `SM_ERR_INVALID_INPUT` is
//!   [`NumError::InvalidInput`]; `SM_ERR` is [`NumFailure`](crate::error::NumFailure) with code 1001.
//! - Points and vectors are distinct types (upstream: one class); only affine operations
//!   exist between them (`Point3 - Point3 = Vec3`, `Point3 ± Vec3 = Point3`).
//! - `Unitize` updates the vector in place and, for a zero vector, stores `(0, 0, 1)`
//!   and returns an error. [`Vec3::normalized`] returns `Option<UnitVec3>`: `None` for a
//!   vector of negligible length or with a component not finite; a vector whose squared
//!   length overflows is scaled first, so it keeps its direction (upstream answers zero).
//!   [`Vec3::make_unit_ortho_vectors`] inherits that, and answers no partial frame on
//!   failure (upstream's out-params as written before the error are not reproduced).
//! - The vector is generic over its scalar (`Vec3<T = f64>`): the series' and interval
//!   vectors (upstream's `series::V3` and `interval_vector`) are `Vec3<T>` and `Vec3<Ivl>`.
//! - `UnitizedDerivative2`/`3` return `SM_ERR` *and* the input derivatives for a
//!   degenerate `self`; the Rust versions return `Err(inputs)` so callers that ignore
//!   the status (`.unwrap_or_else(|d| d)`) see the same values.
//! - Angles are radians. `IsParallelTo`'s degree tolerance and its optional anti-parallel
//!   out-flag are [`Vec3::alignment`]'s radian threshold and [`Alignment`].
//! - Upstream's tolerant `operator==` is [`Vec3::approx_eq`]; Rust `==`
//!   (`PartialEq`) is exact component equality.
//! - `Default` is the zero vector. Upstream's default constructor gives an unset vector
//!   (all components `SM_UNDEF_DOUBLE`); here an unset vector is an `Option<Vec3>`.
//! - `operator[]` takes an [`Axis`], so no index is out of range. Upstream's `operator[] const`
//!   answered `0.0` above 2 and the non-const one dereferenced `NULL`.
//! - `operator/=` keeps the vector unchanged when `|d|` is negligible against every
//!   component; `operator/` always divides. `DivAssign` and `Div` differ the same way.
//! - Statuses ignored upstream through `SE(...)` (a no-op in the release build) are
//!   ignored here too; the one whose output would be uninitialised (`IsInsideSector`'s
//!   `CCWAngleBetween`) cannot fail, see the comment there.
//! - `SM_ASSERT_BREAK` checks and `sm_*`'s `ERR(SM_ERR)` logging are compiled out
//!   upstream (release build) and omitted.

use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use crate::error::{NumError, NumFailure, Result};
use crate::interval::Ivl;
use crate::math::{BIG_DOUBLE, EFF_ZERO, EFF_ZERO_SQ, PI};
use crate::tol;
use crate::vector2d::{Point2, Vec2};

/// `NL_NOZ`: the `z` value marking a 2D control point (`+DBL_MAX`).
/// (Moved down from solidcore `nurbs/math.rs`, where it was `nurbs::math::NOZ`.)
pub const NOZ: f64 = f64::MAX;
/// `NL_NOW`: the `w` value marking a non-rational control point (`-DBL_MAX`).
/// (Moved down from solidcore `nurbs/math.rs`, where it was `nurbs::math::NOW`.)
pub const NOW: f64 = -f64::MAX;

// ---- SmMath.cpp ----------------------------------------------------------------------

/// `sm_sqrt` (`smos_Sqrt`): square root that maps every non-positive input to `0.0`
/// (upstream also flags an error below `-SM_EFF_ZERO_SQ`).
#[must_use]
pub fn sqrt(x: f64) -> f64 {
    if x <= 0.0 {
        0.0
    } else {
        x.sqrt()
    }
}

/// `sm_cbrt` (`smos_CubeRoot`): cube root.
#[must_use]
pub fn cbrt(x: f64) -> f64 {
    crate::fmath::cbrt(x)
}

/// `sm_acos` (`smos_ArcCosine`): arc cosine clamped to `0` / `PI` for arguments
/// outside `(-1, 1)` (upstream also flags an error beyond `1 + SM_EFF_ZERO`).
#[must_use]
pub fn acos(x: f64) -> f64 {
    if x >= 1.0 {
        return 0.0;
    }
    if x <= -1.0 {
        return PI;
    }
    crate::fmath::acos(x)
}

/// `sm_asin` (`smos_ArcSine`): arc sine clamped to `±PI/2` for arguments outside
/// `(-1, 1)` (upstream also flags an error beyond `1 + SM_EFF_ZERO`).
#[must_use]
pub fn asin(x: f64) -> f64 {
    if x >= 1.0 {
        return PI / 2.0;
    }
    if x <= -1.0 {
        return -PI / 2.0;
    }
    crate::fmath::asin(x)
}

/// `SM_IS_VALID_DOUBLE(a)`: not NaN, finite and within `±SM_BIG_DOUBLE / 100`.
fn is_valid_double(a: f64) -> bool {
    // `(a) == (a)` is `!a.is_nan()`.
    !a.is_nan()
        && (-f64::MAX..=f64::MAX).contains(&a)
        && ((-BIG_DOUBLE / 100.0)..=(BIG_DOUBLE / 100.0)).contains(&a)
}

// ---- Types ---------------------------------------------------------------------------

/// A 3D displacement or direction (`SmVector3d` in its vector role), over the scalar `T`:
/// `f64` unless stated, [`Ivl`] for an enclosure, a series coefficient for a jet.
///
/// Linear: vectors add, subtract, negate and scale. A position is a [`Point3`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3<T = f64> {
    /// X component.
    pub x: T,
    /// Y component.
    pub y: T,
    /// Z component.
    pub z: T,
}

/// A 3D position (`SmPoint3d`, one class with `SmVector3d` upstream), over the scalar `T`.
///
/// Affine: `point - point` is a [`Vec3`], `point + vec` and `point - vec` are points, and two
/// points do not add. A weighted sum of points is [`Point3::affine_combination`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point3<T = f64> {
    /// X coordinate.
    pub x: T,
    /// Y coordinate.
    pub y: T,
    /// Z coordinate.
    pub z: T,
}

/// A coordinate axis of 3-space: what indexes a [`Vec3`] or a [`Point3`] (`v[Axis::Y]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    /// The x axis.
    X,
    /// The y axis.
    Y,
    /// The z axis.
    Z,
}

impl Axis {
    /// Every axis, in x, y, z order.
    pub const ALL: [Self; 3] = [Self::X, Self::Y, Self::Z];
}

/// How two directions lie ([`Vec3::alignment`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Alignment {
    /// Parallel and pointing the same way.
    Same,
    /// Parallel and pointing opposite ways.
    Opposite,
    /// Not parallel, or one has no direction.
    Neither,
}

/// A direction: a [`Vec3`] of length 1 (its square within [`EFF_ZERO_SQ`] of 1), every
/// component finite. Made by [`Vec3::normalized`]; read through `Deref` (every `Vec3`
/// method) or [`UnitVec3::as_vec`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitVec3(Vec3);

impl<T> Vec3<T> {
    /// Creates a vector from its components (`SmVector3d(dX, dY, dZ)`, `Set`).
    #[must_use]
    pub const fn new(x: T, y: T, z: T) -> Self {
        Self { x, y, z }
    }

    /// The vector with each component mapped by `f`, in x, y, z order.
    #[must_use]
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> Vec3<U> {
        Vec3::new(f(self.x), f(self.y), f(self.z))
    }

    /// The point this displacement reaches from the origin: the same coordinates.
    #[must_use]
    pub fn to_point(self) -> Point3<T> {
        Point3::new(self.x, self.y, self.z)
    }
}

impl<T: Copy> Vec3<T> {
    /// Creates a vector from `[x, y, z]` (`SmVector3d(double *)`).
    #[must_use]
    pub const fn from_array(a: [T; 3]) -> Self {
        Self::new(a[0], a[1], a[2])
    }

    /// Components as an array `[x, y, z]`.
    #[must_use]
    pub const fn to_array(self) -> [T; 3] {
        [self.x, self.y, self.z]
    }

    /// The vector with every component `v`.
    #[must_use]
    pub const fn splat(v: T) -> Self {
        Self::new(v, v, v)
    }
}

impl<T: Copy + Add<Output = T> + Mul<Output = T>> Vec3<T> {
    /// Dot product, `(x*o.x) + (y*o.y) + (z*o.z)` (`Dot`).
    #[must_use]
    pub fn dot(self, o: Self) -> T {
        (self.x * o.x) + (self.y * o.y) + (self.z * o.z)
    }
}

impl<T: Copy + Sub<Output = T> + Mul<Output = T>> Vec3<T> {
    /// Cross product `self × o` (upstream `operator*` on two vectors).
    #[must_use]
    pub fn cross(self, o: Self) -> Self {
        Self::new(
            (self.y * o.z) - (self.z * o.y),
            (self.z * o.x) - (self.x * o.z),
            (self.x * o.y) - (self.y * o.x),
        )
    }
}

impl<T> Point3<T> {
    /// Creates a point from its coordinates.
    #[must_use]
    pub const fn new(x: T, y: T, z: T) -> Self {
        Self { x, y, z }
    }

    /// The point with each coordinate mapped by `f`, in x, y, z order.
    #[must_use]
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> Point3<U> {
        Point3::new(f(self.x), f(self.y), f(self.z))
    }

    /// The displacement from the origin to this point: the same coordinates.
    #[must_use]
    pub fn to_vec(self) -> Vec3<T> {
        Vec3::new(self.x, self.y, self.z)
    }
}

impl<T: Copy> Point3<T> {
    /// Creates a point from `[x, y, z]`.
    #[must_use]
    pub const fn from_array(a: [T; 3]) -> Self {
        Self::new(a[0], a[1], a[2])
    }

    /// Coordinates as an array `[x, y, z]`.
    #[must_use]
    pub const fn to_array(self) -> [T; 3] {
        [self.x, self.y, self.z]
    }
}

/// Whether each of `x`, `y` and `z` is finite.
fn finite(x: f64, y: f64, z: f64) -> bool {
    x.is_finite() && y.is_finite() && z.is_finite()
}

impl Point3 {
    /// The origin.
    pub const ORIGIN: Self = Self::new(0.0, 0.0, 0.0);

    /// True when any coordinate is NaN, infinite or beyond `±SM_BIG_DOUBLE / 100`
    /// (`IsUndef`, less its `SM_UNDEF_DOUBLE` test).
    #[must_use]
    pub fn is_invalid(self) -> bool {
        self.to_vec().is_invalid()
    }

    /// Whether every coordinate is finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        finite(self.x, self.y, self.z)
    }

    /// Largest absolute coordinate (`GetMaxDimension`).
    #[must_use]
    pub fn max_dimension(self) -> f64 {
        self.to_vec().max_dimension()
    }

    /// How far this point stands from `origin` along `direction`: `(self - origin) . direction`,
    /// as many lengths as `direction` is long (a unit direction gives the distance itself).
    /// Negative behind `origin`.
    #[must_use]
    pub fn distance_along(self, origin: Self, direction: Vec3) -> f64 {
        (self - origin).dot(direction)
    }

    /// Distance between two points (`DistanceBetween`).
    #[must_use]
    pub fn distance(self, o: Self) -> f64 {
        sqrt(self.distance_squared(o))
    }

    /// Squared distance between two points (`DistanceBetweenSquared`).
    #[must_use]
    pub fn distance_squared(self, o: Self) -> f64 {
        (self.x - o.x) * (self.x - o.x)
            + (self.y - o.y) * (self.y - o.y)
            + (self.z - o.z) * (self.z - o.z)
    }

    /// True when the distance to `o` is at most `max` (`CloserThan`).
    ///
    /// Rejects early when any coordinate difference alone exceeds `max`.
    #[must_use]
    pub fn closer_than(self, max: f64, o: Self) -> bool {
        !((self.x - o.x).abs() > max
            || (self.y - o.y).abs() > max
            || (self.z - o.z).abs() > max
            || self.distance_squared(o) > max * max)
    }

    /// Upstream's tolerant equality (`operator==`) on points: the L1 distance is below
    /// `EFF_ZERO * (1 + self.max_dimension())`. Not symmetric in general.
    #[must_use]
    pub fn approx_eq(self, o: Self) -> bool {
        self.to_vec().approx_eq(o.to_vec())
    }

    /// The point `t` of the way from `self` to `q`: `self + (q - self) t`, exactly `self` at 0.
    #[must_use]
    pub fn lerp(self, q: Self, t: f64) -> Self {
        self + (q - self) * t
    }

    /// The point halfway between `self` and `q`: `(self + q) / 2`, coordinate by coordinate.
    #[must_use]
    pub fn midpoint(self, q: Self) -> Self {
        Self::new(
            (self.x + q.x) * 0.5,
            (self.y + q.y) * 0.5,
            (self.z + q.z) * 0.5,
        )
    }

    /// The weighted sum `Σ wᵢ pᵢ` of points whose weights sum to 1 (a barycentric or de Boor
    /// step), summed coordinate by coordinate in the order given. The weights are not
    /// checked: a sum other than 1 moves the result with the origin. The origin for no points.
    #[must_use]
    pub fn affine_combination(terms: &[(Self, f64)]) -> Self {
        terms.iter().fold(Self::ORIGIN, |s, &(p, w)| {
            Self::new(s.x + p.x * w, s.y + p.y * w, s.z + p.z * w)
        })
    }

    /// The point projected along the unit `plane_normal` onto the plane through
    /// `plane_point` (`ProjectPointToPlane`).
    #[must_use]
    pub fn project_point_to_plane(self, plane_point: Self, plane_normal: Vec3) -> Self {
        let diff = self - plane_point;
        self - plane_normal * diff.dot(plane_normal)
    }

    /// The point rotated about the line (`rotate_point`, unit `rotate_axis`) into the
    /// plane through it spanned by `rotate_axis` and the unit `x_axis`
    /// (`RotateProjectPointToPlane`).
    #[must_use]
    pub fn rotate_project_point_to_plane(
        self,
        rotate_point: Self,
        rotate_axis: Vec3,
        x_axis: Vec3,
    ) -> Self {
        let diff = self - rotate_point;
        rotate_point + rotate_axis * diff.dot(rotate_axis) + x_axis * diff.dot(x_axis)
    }

    /// The point projected towards `eye_point` onto the viewing plane (`plane_point`,
    /// unit `plane_normal`) (`PerspectiveProjectPointToPlane`).
    #[must_use]
    pub fn perspective_project_point_to_plane(
        self,
        plane_point: Self,
        plane_normal: Vec3,
        eye_point: Self,
    ) -> Self {
        let eye_plane_diff = plane_point - eye_point;
        let eye_point_diff = self - eye_point;
        eye_point
            + eye_point_diff * (eye_plane_diff.dot(plane_normal) / eye_point_diff.dot(plane_normal))
    }
}

impl<T> core::ops::Index<Axis> for Vec3<T> {
    type Output = T;
    fn index(&self, axis: Axis) -> &T {
        match axis {
            Axis::X => &self.x,
            Axis::Y => &self.y,
            Axis::Z => &self.z,
        }
    }
}

impl<T> core::ops::IndexMut<Axis> for Vec3<T> {
    fn index_mut(&mut self, axis: Axis) -> &mut T {
        match axis {
            Axis::X => &mut self.x,
            Axis::Y => &mut self.y,
            Axis::Z => &mut self.z,
        }
    }
}

impl<T> core::ops::Index<Axis> for Point3<T> {
    type Output = T;
    fn index(&self, axis: Axis) -> &T {
        match axis {
            Axis::X => &self.x,
            Axis::Y => &self.y,
            Axis::Z => &self.z,
        }
    }
}

impl<T> core::ops::IndexMut<Axis> for Point3<T> {
    fn index_mut(&mut self, axis: Axis) -> &mut T {
        match axis {
            Axis::X => &mut self.x,
            Axis::Y => &mut self.y,
            Axis::Z => &mut self.z,
        }
    }
}

impl UnitVec3 {
    /// Unit X.
    pub const X: Self = Self(Vec3::X);
    /// Unit Y.
    pub const Y: Self = Self(Vec3::Y);
    /// Unit Z.
    pub const Z: Self = Self(Vec3::Z);

    /// The direction as a plain vector.
    #[must_use]
    pub const fn as_vec(self) -> Vec3 {
        self.0
    }
}

impl core::ops::Deref for UnitVec3 {
    type Target = Vec3;
    fn deref(&self) -> &Vec3 {
        &self.0
    }
}

impl From<UnitVec3> for Vec3 {
    fn from(u: UnitVec3) -> Self {
        u.0
    }
}

/// The reverse direction, still unit.
impl Neg for UnitVec3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

/// The direction scaled to a vector of length `|d|`.
impl Mul<f64> for UnitVec3 {
    type Output = Vec3;
    fn mul(self, d: f64) -> Vec3 {
        self.0 * d
    }
}

/// The direction scaled to a vector of length `|d|`.
impl Mul<UnitVec3> for f64 {
    type Output = Vec3;
    fn mul(self, u: UnitVec3) -> Vec3 {
        self * u.0
    }
}

/// `x^2`, enclosed with a nonnegative lower bound.
fn square(x: Ivl) -> Ivl {
    let s = x * x;
    Ivl::new(s.lo.max(0.0), s.hi)
}

/// Enclosures: each component an [`Ivl`] holding the true value. (Formerly
/// `interval_vector`'s free functions over `[Ivl; 3]`, the same operations in the same order.)
impl Vec3<Ivl> {
    /// `|self|`, enclosed with nonnegative squared components.
    #[must_use]
    pub fn norm(self) -> Ivl {
        (square(self.x) + square(self.y) + square(self.z)).sqrt()
    }

    /// A unit direction, enclosed, when the norm is finite and bounded away from zero.
    #[must_use]
    pub fn unit(self) -> Option<Self> {
        let length = self.norm();
        if !length.is_finite() || length.lo <= 0.0 {
            return None;
        }
        Some(self.map(|v| v / length))
    }

    /// The componentwise interval hull of `self` and `o`.
    #[must_use]
    pub fn hull(self, o: Self) -> Self {
        Self::new(self.x.hull(o.x), self.y.hull(o.y), self.z.hull(o.z))
    }
}

// ---- Vec3 ----------------------------------------------------------------------------

impl Vec3 {
    /// The zero vector.
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    /// Unit X axis.
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    /// Unit Y axis.
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    /// Unit Z axis.
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    /// Orient an undirected axis toward the positive side of its largest component, keeping
    /// its length. Components within a relative 1e-9 of the largest magnitude tie in X, Y, Z
    /// order, at any scale, so a zero component never decides; the zero vector stays as it is.
    #[must_use]
    pub fn canonical_axis_direction(self) -> Self {
        let coordinates = self.to_array();
        let largest = coordinates
            .iter()
            .fold(0.0_f64, |size, coordinate| size.max(coordinate.abs()));
        match coordinates
            .iter()
            .find(|coordinate| coordinate.abs() >= largest * (1.0 - 1e-9))
        {
            Some(coordinate) if *coordinate < 0.0 => -self,
            _ => self,
        }
    }

    /// The unit direction of an undirected axis, oriented as [`Vec3::canonical_axis_direction`]
    /// orients it: the vector divided by its length. `None` for a vector no longer than
    /// `min_length` or one whose length is not finite.
    #[must_use]
    pub fn unit_axis_direction(self, min_length: f64) -> Option<UnitVec3> {
        let length = self.length();
        (length > min_length && length.is_finite())
            .then(|| UnitVec3((self / length).canonical_axis_direction()))
    }

    /// Converts an NLib control point `[x, y, z, w]` (`SmVector3d(double *, bool)`):
    /// `z == NL_NOZ` becomes `0`, and unless `w == NL_NOW` the point is divided by `w`.
    #[must_use]
    pub fn from_nl(p: [f64; 4]) -> Self {
        let [px, py, pz, pw] = p;
        let mut v = Self::new(px, py, if pz == NOZ { 0.0 } else { pz });
        if NOW != pw {
            v.x /= pw;
            v.y /= pw;
            v.z /= pw;
        }
        v
    }

    /// Whether every component is finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        finite(self.x, self.y, self.z)
    }
    /// True when any component is NaN, infinite or beyond `±SM_BIG_DOUBLE / 100`
    /// (`IsUndef`, less its `SM_UNDEF_DOUBLE` test).
    #[must_use]
    pub fn is_invalid(self) -> bool {
        !is_valid_double(self.x) || !is_valid_double(self.y) || !is_valid_double(self.z)
    }

    /// True when the length is below `tol` (`IsZero`; upstream default `tol` is
    /// [`EFF_ZERO`]).
    #[must_use]
    pub fn is_zero(self, tol: f64) -> bool {
        self.length_squared() < tol * tol
    }

    /// True when `z` is NLib's 2D marker `NL_NOZ` (`Is2d`).
    #[must_use]
    pub fn is_2d(self) -> bool {
        self.z == NOZ
    }

    /// Squared Euclidean length, `(x*x) + (y*y) + (z*z)` (`LengthSquared`).
    #[must_use]
    pub fn length_squared(self) -> f64 {
        (self.x * self.x) + (self.y * self.y) + (self.z * self.z)
    }

    /// Euclidean length (`Length`).
    #[must_use]
    pub fn length(self) -> f64 {
        sqrt(self.length_squared())
    }

    /// The unit vector in this direction (`Unitize`): `None` for a vector whose squared
    /// length is below [`EFF_ZERO_SQ`] or with a component that is not finite.
    ///
    /// A vector whose squared length is within [`EFF_ZERO_SQ`] of 1 is returned unchanged.
    /// One whose squared length overflows is scaled first, as [`Vec3::length_and_unit`]
    /// scales it, so every finite vector longer than the threshold has its direction.
    #[must_use]
    pub fn normalized(self) -> Option<UnitVec3> {
        let len_sq = self.length_squared();
        if len_sq.is_finite() {
            // A finite square means finite components.
            (len_sq >= EFF_ZERO_SQ).then(|| UnitVec3(self.unitize_nonzero(len_sq)))
        } else {
            self.length_and_unit().map(|(_, unit)| unit)
        }
    }

    /// Upstream's in-place `Unitize` result with the status ignored: the unit vector,
    /// or `(0, 0, 1)` for a zero-length vector; NaN or overflow as upstream gives them.
    /// Kept private for the bodies ported with it; callers use [`Vec3::normalized`].
    fn unit_or_z(self) -> Self {
        let len_sq = self.length_squared();
        if len_sq < EFF_ZERO_SQ {
            Self::Z
        } else {
            self.unitize_nonzero(len_sq)
        }
    }

    /// The length and the unit direction of a vector of any finite length (not upstream). The
    /// vector is first divided by its largest absolute coordinate, so its squared length lies
    /// in `[1, 3]` and neither overflows nor underflows, then made unit as [`Vec3::normalized`]
    /// makes it. The direction is exact at every finite length; the length is infinite past
    /// `f64::MAX` and rounds to 0 below the smallest positive `f64`.
    ///
    /// `None` for a coordinate that is not finite, or the zero vector.
    #[must_use]
    pub fn length_and_unit(self) -> Option<(f64, UnitVec3)> {
        let scale = self.max_dimension();
        if !(self.is_finite() && scale > 0.0) {
            return None;
        }
        let scaled = self / scale;
        let len_sq = scaled.length_squared();
        Some((
            scale * sqrt(len_sq),
            UnitVec3(scaled.unitize_nonzero(len_sq)),
        ))
    }

    fn unitize_nonzero(self, len_sq: f64) -> Self {
        if (len_sq - 1.0).abs() > EFF_ZERO_SQ {
            let len = sqrt(len_sq);
            Self::new(self.x / len, self.y / len, self.z / len)
        } else {
            self
        }
    }

    /// Triple product `self · (b × c)` (`TripleProduct`).
    #[must_use]
    pub fn triple_product(self, b: Self, c: Self) -> f64 {
        self.x * ((b.y * c.z) - (b.z * c.y))
            + self.y * ((b.z * c.x) - (b.x * c.z))
            + self.z * ((b.x * c.y) - (b.y * c.x))
    }

    /// `(self × v) × self`: a vector normal to `self` in the plane of `self` and `v`
    /// (`NormalInPlane`).
    #[must_use]
    pub fn normal_in_plane(self, v: Self) -> Self {
        let (x, y, z) = (self.x, self.y, self.z);
        Self::new(
            y * ((v.x * y) - (v.y * x)) - z * ((v.z * x) - (v.x * z)),
            z * ((v.y * z) - (v.z * y)) - x * ((v.x * y) - (v.y * x)),
            x * ((v.z * x) - (v.x * z)) - y * ((v.y * z) - (v.z * y)),
        )
    }

    /// Largest absolute coordinate (`GetMaxDimension`).
    #[must_use]
    pub fn max_dimension(self) -> f64 {
        let mut d = if self.x.abs() > self.y.abs() {
            self.x.abs()
        } else {
            self.y.abs()
        };
        if self.z.abs() > d {
            d = self.z.abs();
        }
        d
    }

    /// Smallest absolute coordinate (`GetMinDimension`).
    #[must_use]
    pub fn min_dimension(self) -> f64 {
        let mut d = if self.x.abs() < self.y.abs() {
            self.x.abs()
        } else {
            self.y.abs()
        };
        if self.z.abs() < d {
            d = self.z.abs();
        }
        d
    }

    /// True when `self` and `o` are parallel or anti-parallel within `max_angle` radians
    /// (`IsParallelTo`, whose tolerance is in degrees). Zero-length vectors are parallel to
    /// nothing.
    #[must_use]
    pub fn is_parallel_to(self, o: Self, max_angle: f64) -> bool {
        self.alignment(o, max_angle) != Alignment::Neither
    }

    /// How `self` and `o` lie: [`Alignment::Same`] or [`Alignment::Opposite`] when they are
    /// parallel within `max_angle` radians, as their angle is at most or more than a right
    /// angle; [`Alignment::Neither`] otherwise, and when either has zero length
    /// (`IsParallelTo` with its anti-parallel out-flag).
    #[must_use]
    pub fn alignment(self, o: Self, max_angle: f64) -> Alignment {
        match self.angle_off_line(o) {
            Some((ang, opposite)) if ang < max_angle => {
                if opposite {
                    Alignment::Opposite
                } else {
                    Alignment::Same
                }
            }
            _ => Alignment::Neither,
        }
    }

    /// The angle between the lines of `self` and `o`, in `[0, PI/2]`, and whether they point
    /// more than a right angle apart; `None` when either has zero length.
    fn angle_off_line(self, o: Self) -> Option<(f64, bool)> {
        if self.length_squared() <= EFF_ZERO_SQ || o.length_squared() <= EFF_ZERO_SQ {
            return None;
        }
        // Both have a length, so only a non-finite one fails; it reads as angle 0, as the
        // pin's ignored status left it.
        let ang = self.angle_between(o).unwrap_or(0.0);
        Some(if ang > PI / 2.0 {
            (PI - ang, true)
        } else {
            (ang, false)
        })
    }

    /// Not upstream: this direction, reversed when it points more than a right angle away from
    /// `like` (by their angle, as [`Vec3::alignment`] reads it): a line's direction given the sense
    /// of another, as a turn about an edge found again keeps the sense it was picked with.
    /// `None` when either has zero length or a coordinate that is not finite. The two are
    /// compared scaled by their largest coordinate, so a vector whose squared length overflows
    /// still has its sense.
    #[must_use]
    pub fn with_sense_of(self, like: Self) -> Option<Self> {
        if !self.is_finite() || !like.is_finite() {
            return None;
        }
        if self.length_squared() <= EFF_ZERO_SQ || like.length_squared() <= EFF_ZERO_SQ {
            return None;
        }
        let scaled = |v: Self| v * (1.0 / v.x.abs().max(v.y.abs()).max(v.z.abs()));
        let (_, opposite) = scaled(self).angle_off_line(scaled(like))?;
        Some(if opposite { -self } else { self })
    }

    /// True when `self` and `o` are perpendicular within `max_angle` radians
    /// (`IsPerpendicularTo`, whose tolerance is in degrees). Zero-length vectors are
    /// perpendicular to nothing.
    #[must_use]
    pub fn is_perpendicular_to(self, o: Self, max_angle: f64) -> bool {
        if self.length_squared() > EFF_ZERO_SQ && o.length_squared() > EFF_ZERO_SQ {
            // SE(): a failed AngleBetween leaves 0.0.
            let ang = self.angle_between(o).unwrap_or(0.0);
            if (ang - PI / 2.0).abs() < max_angle {
                return true;
            }
        }
        false
    }

    /// The vector projected along the unit `plane_normal` onto the plane
    /// (`ProjectToPlane`).
    #[must_use]
    pub fn project_to_plane(self, plane_normal: Self) -> Self {
        self - plane_normal * self.dot(plane_normal)
    }

    /// The vector rotated about the unit `rotate_axis` into the plane spanned by
    /// `rotate_axis` and the unit `x_axis` (`RotateProjectToPlane`).
    #[must_use]
    pub fn rotate_project_to_plane(self, rotate_axis: Self, x_axis: Self) -> Self {
        rotate_axis * self.dot(rotate_axis) + x_axis * self.dot(x_axis)
    }

    /// The vector based at `base_point`, projected towards `eye_point` onto the
    /// viewing plane (`plane_point`, unit `plane_normal`)
    /// (`PerspectiveProjectToPlane`).
    #[must_use]
    pub fn perspective_project_to_plane(
        self,
        base_point: Point3,
        plane_point: Point3,
        plane_normal: Self,
        eye_point: Point3,
    ) -> Self {
        let eye_plane_diff = plane_point - eye_point;
        let eye_point_diff = base_point - eye_point;
        let eye_vec_diff = base_point + self - eye_point;
        eye_vec_diff * (eye_plane_diff.dot(plane_normal) / eye_vec_diff.dot(plane_normal))
            - eye_point_diff * (eye_plane_diff.dot(plane_normal) / eye_point_diff.dot(plane_normal))
    }

    /// Component-wise product (`Multiply`).
    #[must_use]
    pub fn multiply(self, o: Self) -> Self {
        Self::new(self.x * o.x, self.y * o.y, self.z * o.z)
    }

    /// Derivative of `self × other` given the derivatives of both vectors
    /// (`DerivativeOfCrossProduct`).
    #[must_use]
    pub fn derivative_of_cross_product(
        self,
        other: Self,
        self_prime: Self,
        other_prime: Self,
    ) -> Self {
        let (a, b, da, db) = (self, other, self_prime, other_prime);
        Self::new(
            da.y * b.z + a.y * db.z - (da.z * b.y + a.z * db.y),
            da.z * b.x + a.z * db.x - (da.x * b.z + a.x * db.z),
            da.x * b.y + a.x * db.y - (da.y * b.x + a.y * db.x),
        )
    }

    /// Solves `self.x*t + self.y*u = self.z`, `o.x*t + o.y*u = o.z` for `(t, u)`
    /// (`SolveTwoLinearEquations`).
    ///
    /// # Errors
    ///
    /// `SM_ERR` ([`NumFailure`](crate::error::NumFailure)) when the system is (numerically) singular.
    pub fn solve_two_linear_equations(self, o: Self) -> Result<Point2> {
        let (x, y, z) = (self.x, self.y, self.z);
        let denom = (o.y * x) - (o.x * y);
        if denom.abs() >= EFF_ZERO_SQ {
            let u = (o.z * x - o.x * z) / denom;
            if (x >= EFF_ZERO_SQ || x <= -EFF_ZERO_SQ) && x.abs() > o.x.abs() {
                return Ok(Point2::new((z - y * u) / x, u));
            } else if o.x.abs() >= EFF_ZERO_SQ {
                return Ok(Point2::new((o.z - o.y * u) / o.x, u));
            }
        }
        Err(NumError::failed(
            "geom::vector3d::Vec3::solve_two_linear_equations",
            NumFailure::Singular { measure: denom },
        ))
    }

    /// Upstream's tolerant equality (`operator==`): the L1 distance is below
    /// `EFF_ZERO * (1 + self.max_dimension())`. Not symmetric in general.
    #[must_use]
    pub fn approx_eq(self, o: Self) -> bool {
        let scaled_zero = EFF_ZERO * (1.0 + self.max_dimension());
        let dist = (self.x - o.x).abs() + (self.y - o.y).abs() + (self.z - o.z).abs();
        dist < scaled_zero
    }

    /// True when `self` lies in the sector swept from `bound1` towards `binormal1` up
    /// to `bound2`, or on either bound (`IsInsideSector`). All vectors are assumed
    /// coplanar; three vectors in essentially the same direction count as inside.
    #[must_use]
    pub fn is_inside_sector(self, bound1: Self, binormal1: Self, bound2: Self) -> bool {
        // SE(sNorm.Unitize()): a zero normal becomes (0,0,1).
        let norm = bound1.cross(binormal1).unit_or_z();
        // SE(...CCWAngleBetween(...)): it only fails through AngleBetween on
        // zero-length projections, which it rejects (as 0.0) before that call, so the
        // fallback is never used.
        let mut sector_angle = norm.ccw_angle_between(bound1, bound2).unwrap_or(0.0);
        let mut this_angle = norm.ccw_angle_between(bound1, self).unwrap_or(0.0);

        // CCWAngleBetween() returns [-PI,PI], we need [0,2PI]
        if sector_angle < 0.0 {
            sector_angle += 2.0 * PI;
        }
        if this_angle < -EFF_ZERO {
            this_angle += 2.0 * PI;
        }

        // Angles near 0 and near 2*PI are the same direction: call that inside.
        if (this_angle - sector_angle).abs() >= 2.0 * PI - EFF_ZERO {
            return true;
        }

        this_angle > -EFF_ZERO && this_angle < sector_angle + EFF_ZERO
    }

    /// An orthonormal frame `[x, y, z]` with `x` along `self` (`MakeUnitOrthoVectors`).
    ///
    /// `y` lies in the plane of `self` and `y_reference` (on its side) when that is
    /// given, has a direction and is not parallel to `self` (the cross product of the two
    /// unit vectors has length); otherwise `y` is chosen arbitrarily (perpendicular to `x`,
    /// trying `(0,1,0)` then `(0,0,1)`). `z = x × y`.
    ///
    /// # Errors
    ///
    /// [`NumError::InvalidInput`] when `self` has no direction ([`Vec3::normalized`] is
    /// `None`). Upstream also fails, with `SM_ERR`, for a finite `self` whose squared length
    /// overflows; here that has its direction. Upstream crosses with `y_reference` as
    /// given, so one too long passes for parallel (its cross product overflows) and one
    /// too short for parallel too; here only its direction counts.
    pub fn make_unit_ortho_vectors(self, y_reference: Option<Self>) -> Result<[UnitVec3; 3]> {
        let x_axis = self.normalized().ok_or(NumError::InvalidInput {
            operation: "geom::vector3d::Vec3::make_unit_ortho_vectors",
            reason: "zero-length or non-finite vector",
        })?;
        let frame = |z_axis: UnitVec3| {
            let y_axis = UnitVec3(z_axis.cross(*x_axis).unit_or_z());
            [x_axis, y_axis, z_axis]
        };
        if let Some(z_axis) = y_reference
            .and_then(Self::normalized)
            .map(|y_ref| x_axis.cross(*y_ref))
            .filter(|z_axis| z_axis.length_squared() > EFF_ZERO_SQ)
            .and_then(Self::normalized)
        {
            return Ok(frame(z_axis));
        }
        // If made it to here choose arbitrary axis. A unit `x` is square to `(0,1,0)` or
        // `(0,0,1)` within rounding, so one of the two crosses has length.
        let mut z_axis = x_axis.cross(Self::Y);
        if z_axis.length_squared() < EFF_ZERO_SQ {
            z_axis = x_axis.cross(Self::Z);
        }
        Ok(frame(UnitVec3(z_axis.unit_or_z())))
    }

    /// The vector rotated by `ang_rad` about `axis` (right-handed; `axis` need not be
    /// unit length) (`RotateVecAboutAxis`).
    #[must_use]
    pub fn rotate_vec_about_axis(self, axis: Self, ang_rad: f64) -> Self {
        let cos = crate::fmath::cos(ang_rad);
        let sin = crate::fmath::sin(ang_rad);

        // avoid errors - unitize the input (status ignored)
        let z = axis.unit_or_z();

        // Get the component perp to the rotation axis
        let dot = self.dot(z);
        let mut x = self - dot * z;
        let x_len = x.length();
        if x_len < EFF_ZERO {
            return self;
        }
        x /= x_len;

        // get component perp to rotate and x axes
        let y = z.cross(x);

        x_len * cos * x + x_len * sin * y + dot * z
    }

    /// Derivative of the unit vector `self/|self|` given `d1 = self'`
    /// (`UnitizedDerivative`). For a zero-length `self`, returns `d1`.
    #[must_use]
    pub fn unitized_derivative(self, d1: Self) -> Self {
        let mag = self.length();
        if mag < EFF_ZERO {
            return d1;
        }
        let unit = self / mag;
        let mag_t = unit.dot(d1);
        let mut unit_t = d1 - mag_t * unit;
        unit_t /= mag;
        unit_t
    }

    /// First two derivatives of the unit vector `self/|self|` given `d1 = self'` and
    /// `d2 = self''` (`UnitizedDerivative2`).
    ///
    /// # Errors
    ///
    /// Upstream's `SM_ERR` for a zero-length `self`; the error carries `[d1, d2]`,
    /// the values upstream writes to its outputs in that case.
    pub fn unitized_derivative2(
        self,
        d1: Self,
        d2: Self,
    ) -> core::result::Result<[Self; 2], [Self; 2]> {
        let mag = self.length();
        if mag < EFF_ZERO {
            return Err([d1, d2]);
        }

        // 1st derivative
        let unit = self / mag;
        let mag_t = unit.dot(d1);
        let mut unit_t = d1 - mag_t * unit;
        unit_t /= mag;

        // 2nd derivative
        let mag_tt = unit_t.dot(d1) + unit.dot(d2);
        let mut unit_tt = d2 - 2.0 * mag_t * unit_t - mag_tt * unit;
        unit_tt /= mag;

        Ok([unit_t, unit_tt])
    }

    /// First three derivatives of the unit vector `self/|self|` given `d1 = self'`,
    /// `d2 = self''` and `d3 = self'''` (`UnitizedDerivative3`).
    ///
    /// # Errors
    ///
    /// Upstream's `SM_ERR` for a zero-length `self`; the error carries `[d1, d2, d3]`,
    /// the values upstream writes to its outputs in that case.
    pub fn unitized_derivative3(
        self,
        d1: Self,
        d2: Self,
        d3: Self,
    ) -> core::result::Result<[Self; 3], [Self; 3]> {
        let mag = self.length();
        if mag < EFF_ZERO {
            return Err([d1, d2, d3]);
        }

        let unit = self / mag;
        let mag_t = unit.dot(d1);
        let mut unit_t = d1 - mag_t * unit;
        unit_t /= mag;

        // 2nd deriv:
        let mag_tt = unit_t.dot(d1) + unit.dot(d2);
        let mut unit_tt = d2 - 2.0 * mag_t * unit_t - mag_tt * unit;
        unit_tt /= mag;

        // 3rd deriv:
        let mag_ttt = unit_tt.dot(d1) + 2.0 * unit_t.dot(d2) + unit.dot(d3);
        let mut unit_ttt = d3 - 3.0 * mag_t * unit_tt - 3.0 * mag_tt * unit_t - mag_ttt * unit;
        unit_ttt /= mag;

        Ok([unit_t, unit_tt, unit_ttt])
    }

    /// Angle between two vectors in radians, in `[0, PI]` (`AngleBetween`).
    ///
    /// Uses `acos` of the normalised dot product when its square is below `0.81`,
    /// else `asin` of the normalised cross product (more accurate near `0` and `PI`).
    ///
    /// # Errors
    ///
    /// [`NumError::InvalidInput`] when either squared length is within the scaled
    /// zero (`SM_IS_ZERO_SQUARED`); upstream then also reports an angle of `0`.
    pub fn angle_between(self, o: Self) -> Result<f64> {
        let v1_len_sq = self.length_squared();
        let v2_len_sq = o.length_squared();

        if tol::is_zero_squared(v1_len_sq) || tol::is_zero_squared(v2_len_sq) {
            return Err(NumError::InvalidInput {
                operation: "geom::vector3d::Vec3::angle_between",
                reason: "zero-length vector",
            });
        }
        let dot_prep = self.dot(o);
        let dot_sign = if dot_prep < 0.0 { -1.0 } else { 1.0 };
        let dot_squared = (dot_prep * dot_prep) / v1_len_sq / v2_len_sq;
        let dot = dot_sign * sqrt(dot_squared);

        if dot_squared < 0.81 {
            Ok(acos(dot))
        } else {
            let crs = self.cross(o);
            let crs_norm = crs.length_squared();
            let mag = sqrt(crs_norm / v1_len_sq / v2_len_sq);
            Ok(if dot < 0.0 { PI - asin(mag) } else { asin(mag) })
        }
    }

    /// Signed angle in radians about the unit normal `self`, from unit `start` to unit `end`.
    /// The directions must be perpendicular to the normal, within `1e-9` for rounding.
    /// They are not renormalised. A half turn with a triple product below `1e-12` in
    /// magnitude always chooses positive `PI`, including a negative rounded zero.
    ///
    /// # Errors
    /// [`NumError::InvalidInput`] for non-finite, non-unit or non-perpendicular directions.
    pub fn signed_unit_angle(self, start: Self, end: Self) -> Result<f64> {
        if ![self, start, end]
            .iter()
            .all(|v| (v.length_squared() - 1.0).abs() <= 1e-9)
            || self.dot(start).abs() > 1e-9
            || self.dot(end).abs() > 1e-9
        {
            return Err(NumError::InvalidInput {
                operation: "geom::vector3d::Vec3::signed_unit_angle",
                reason: "expected unit directions perpendicular to a unit normal",
            });
        }
        let sin = self.dot(start.cross(end));
        let cos = start.dot(end).clamp(-1.0, 1.0);
        Ok(if cos < 0.0 && sin.abs() < 1e-12 {
            PI
        } else {
            crate::fmath::atan2(sin, cos)
        })
    }

    /// Counter-clockwise angle in radians, in `[-PI, PI]`, from `start` to `end`
    /// after projecting both into the plane normal to `self` (`CCWAngleBetween`).
    /// Returns `0` when either vector is parallel to `self`.
    ///
    /// # Errors
    ///
    /// Propagates [`Vec3::angle_between`] on the projected vectors.
    pub fn ccw_angle_between(self, start: Self, end: Self) -> Result<f64> {
        let start_proj = self.cross(start).cross(self);
        if start_proj.length_squared() < 100.0 * EFF_ZERO_SQ {
            return Ok(0.0);
        }
        let end_proj = self.cross(end).cross(self);
        if end_proj.length_squared() < 100.0 * EFF_ZERO_SQ {
            return Ok(0.0);
        }

        let mut proj_ang = start_proj.angle_between(end_proj)?;

        // negate angle's sign when Cross(start,end) opposes this vector
        if start_proj.cross(end_proj).dot(self) < 0.0 {
            proj_ang = -proj_ang;
        }
        Ok(proj_ang)
    }

    /// The turn about unit axis `self` that brings `normal` square to `line`, folded into
    /// (-PI/2, PI/2]: the nearer of the two answers PI apart. `normal` and `line` act through
    /// their projections on the plane square to the axis.
    ///
    /// # Errors
    ///
    /// [`NumError::InvalidInput`] for a non-finite vector, or a `normal` or `line` along
    /// the axis (it has no direction square to it).
    pub fn square_turn(self, normal: Self, line: Self) -> Result<f64> {
        if !(self.is_finite() && normal.is_finite() && line.is_finite()) {
            return Err(NumError::InvalidInput {
                operation: "geom::vector3d::Vec3::square_turn",
                reason: "non-finite vector",
            });
        }
        // Only their directions count: scaled to a largest component of 1, so squaring a huge
        // (or tiny) finite vector can't overflow (or underflow) on the way.
        let scaled = |v: Self| {
            let m = v.x.abs().max(v.y.abs()).max(v.z.abs());
            if m > 0.0 {
                v / m
            } else {
                v
            }
        };
        let (normal, line) = (scaled(normal), scaled(line));
        let target = self.cross(line);
        if self.cross(normal).length_squared() < 100.0 * EFF_ZERO_SQ
            || target.length_squared() < 100.0 * EFF_ZERO_SQ
        {
            return Err(NumError::InvalidInput {
                operation: "geom::vector3d::Vec3::square_turn",
                reason: "a vector along the axis",
            });
        }
        let turn = self.ccw_angle_between(normal, target)?;
        Ok(if turn > PI / 2.0 {
            turn - PI
        } else if turn <= -PI / 2.0 {
            turn + PI
        } else {
            turn
        })
    }

    /// True when `self` and `end_vec` both lie along the segment `start_pnt`..`end_pnt`
    /// (either sense), or the segment is degenerate (`IsColinearWith`).
    #[must_use]
    pub fn is_colinear_with(self, end_vec: Self, start_pnt: Point3, end_pnt: Point3) -> bool {
        let vec = end_pnt - start_pnt;
        if vec.length_squared() < EFF_ZERO_SQ {
            return true;
        }
        let vec = vec.unit_or_z();
        let dir1 = self.unit_or_z();
        let dir2 = end_vec.unit_or_z();

        1.0 - vec.dot(dir1).abs() < EFF_ZERO && 1.0 - vec.dot(dir2).abs() < EFF_ZERO
    }
}

impl<T: Copy> From<[T; 3]> for Vec3<T> {
    fn from(a: [T; 3]) -> Self {
        Self::from_array(a)
    }
}

impl<T: Copy> From<[T; 3]> for Point3<T> {
    fn from(a: [T; 3]) -> Self {
        Self::from_array(a)
    }
}

/// `SmVector3d(const SmVector2d&)`: `z = 0`.
impl From<Vec2> for Vec3 {
    fn from(v: Vec2) -> Self {
        Self::new(v.x, v.y, 0.0)
    }
}

/// The point of the plane `z = 0` with the same `x` and `y`.
impl From<Point2> for Point3 {
    fn from(p: Point2) -> Self {
        Self::new(p.x, p.y, 0.0)
    }
}

impl<T: Neg<Output = T>> Neg for Vec3<T> {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

impl<T: Add<Output = T>> Add for Vec3<T> {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl<T: Sub<Output = T>> Sub for Vec3<T> {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl<T: Copy + Mul<Output = T>> Mul<T> for Vec3<T> {
    type Output = Self;
    fn mul(self, d: T) -> Self {
        Self::new(self.x * d, self.y * d, self.z * d)
    }
}

impl Mul<Vec3> for f64 {
    type Output = Vec3;
    fn mul(self, v: Vec3) -> Vec3 {
        Vec3::new(v.x * self, v.y * self, v.z * self)
    }
}

/// `operator/(v, d)`: plain component division (no guard against tiny `d`).
impl<T: Copy + Div<Output = T>> Div<T> for Vec3<T> {
    type Output = Self;
    fn div(self, d: T) -> Self {
        Self::new(self.x / d, self.y / d, self.z / d)
    }
}

impl<T: AddAssign> AddAssign for Vec3<T> {
    fn add_assign(&mut self, o: Self) {
        self.x += o.x;
        self.y += o.y;
        self.z += o.z;
    }
}

impl<T: SubAssign> SubAssign for Vec3<T> {
    fn sub_assign(&mut self, o: Self) {
        self.x -= o.x;
        self.y -= o.y;
        self.z -= o.z;
    }
}

impl<T: Copy + MulAssign> MulAssign<T> for Vec3<T> {
    fn mul_assign(&mut self, d: T) {
        self.x *= d;
        self.y *= d;
        self.z *= d;
    }
}

/// `operator/=(d)`: leaves the vector unchanged when `|d| <= EFF_ZERO * |c|` for
/// every component `c` (so also for `0 / 0`), else divides.
impl DivAssign<f64> for Vec3 {
    fn div_assign(&mut self, d: f64) {
        let denom_chk = d.abs();
        if denom_chk <= EFF_ZERO * self.x.abs()
            && denom_chk <= EFF_ZERO * self.y.abs()
            && denom_chk <= EFF_ZERO * self.z.abs()
        {
            return;
        }
        self.x /= d;
        self.y /= d;
        self.z /= d;
    }
}

/// A point moved by a displacement.
impl<T: Add<Output = T>> Add<Vec3<T>> for Point3<T> {
    type Output = Self;
    fn add(self, v: Vec3<T>) -> Self {
        Self::new(self.x + v.x, self.y + v.y, self.z + v.z)
    }
}

/// A point moved back by a displacement.
impl<T: Sub<Output = T>> Sub<Vec3<T>> for Point3<T> {
    type Output = Self;
    fn sub(self, v: Vec3<T>) -> Self {
        Self::new(self.x - v.x, self.y - v.y, self.z - v.z)
    }
}

/// The displacement from `o` to `self`.
impl<T: Sub<Output = T>> Sub for Point3<T> {
    type Output = Vec3<T>;
    fn sub(self, o: Self) -> Vec3<T> {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl<T: AddAssign> AddAssign<Vec3<T>> for Point3<T> {
    fn add_assign(&mut self, v: Vec3<T>) {
        self.x += v.x;
        self.y += v.y;
        self.z += v.z;
    }
}

impl<T: SubAssign> SubAssign<Vec3<T>> for Point3<T> {
    fn sub_assign(&mut self, v: Vec3<T>) {
        self.x -= v.x;
        self.y -= v.y;
        self.z -= v.z;
    }
}

/// A point moved along a direction by its unit length.
impl Add<UnitVec3> for Point3 {
    type Output = Self;
    fn add(self, u: UnitVec3) -> Self {
        self + u.0
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn signed_unit_angles_keep_orientation_and_choose_positive_half_turns() {
        for (normal, start, quarter) in [
            (Vec3::Z, Vec3::X, Vec3::Y),
            (Vec3::X, Vec3::Y, Vec3::Z),
            (Vec3::Y, Vec3::Z, Vec3::X),
            (Vec3::new(0.0, 0.6, 0.8), Vec3::X, Vec3::new(0.0, 0.8, -0.6)),
        ] {
            assert_eq!(normal.signed_unit_angle(start, start).unwrap(), 0.0);
            assert!((normal.signed_unit_angle(start, quarter).unwrap() - PI / 2.0).abs() < 1e-15);
            assert!((normal.signed_unit_angle(quarter, start).unwrap() + PI / 2.0).abs() < 1e-15);
            assert_eq!(normal.signed_unit_angle(start, -start).unwrap(), PI);
        }
        for tiny in [-5e-13, -0.0, 0.0, 5e-13] {
            assert_eq!(
                Vec3::Z
                    .signed_unit_angle(Vec3::X, Vec3::new(-1.0, tiny, 0.0))
                    .unwrap(),
                PI
            );
        }
        for tiny in [-2e-12_f64, 2e-12] {
            assert_eq!(
                Vec3::Z
                    .signed_unit_angle(Vec3::X, Vec3::new(-1.0, tiny, 0.0))
                    .unwrap(),
                crate::fmath::atan2(tiny, -1.0)
            );
        }
    }

    #[test]
    fn signed_unit_angles_refuse_invalid_frames_without_normalising_them() {
        for invalid in [
            Vec3::ZERO,
            Vec3::X * 2.0,
            Vec3::new(f64::NAN, 0.0, 0.0),
            Vec3::new(0.0, f64::INFINITY, 0.0),
            Vec3::new(1e300, 1e300, 0.0),
        ] {
            assert!(invalid.signed_unit_angle(Vec3::X, Vec3::Y).is_err());
            assert!(Vec3::Z.signed_unit_angle(invalid, Vec3::Y).is_err());
            assert!(Vec3::Z.signed_unit_angle(Vec3::X, invalid).is_err());
        }
        assert!(Vec3::Z.signed_unit_angle(Vec3::Z, Vec3::X).is_err());
        assert!(Vec3::Z.signed_unit_angle(Vec3::X, Vec3::Z).is_err());
    }

    #[test]
    fn distance_along_is_signed_and_ignores_the_sideways_offset() {
        let (o, d) = (Point3::new(1.0, 2.0, 3.0), Vec3::new(0.0, 1.0, 0.0));
        assert!((Point3::new(9.0, 6.0, -4.0).distance_along(o, d) - 4.0).abs() < 1e-12);
        assert!((Point3::new(1.0, -1.0, 3.0).distance_along(o, d) + 3.0).abs() < 1e-12);
        assert_eq!(o.distance_along(o, d), 0.0);
    }

    use super::*;

    /// A side with normal X, turned about Z to square with a line.
    #[test]
    fn square_turn_folds_to_the_nearer() {
        let z = Vec3::new(0.0, 0.0, 1.0);
        let x = Vec3::new(1.0, 0.0, 0.0);
        let turn = |l: [f64; 3]| z.square_turn(x, Vec3::from(l)).unwrap();
        assert!(turn([0.0, 1.0, 0.0]).abs() < 1e-12);
        assert!((turn([1.0, 1.0, 5.0]) + PI / 4.0).abs() < 1e-12);
        assert!((turn([1.0, 0.0, 0.0]) - PI / 2.0).abs() < 1e-12);
        assert!(z.square_turn(x, z).is_err());
        assert!(z.square_turn(x, Vec3::new(f64::NAN, 0.0, 0.0)).is_err());
        // Huge and tiny finite vectors: the same turn, never NaN.
        assert!((turn([1e200, 1e200, 0.0]) + PI / 4.0).abs() < 1e-12);
        assert!(
            (z.square_turn(x * 1e300, Vec3::new(1e-300, 1e-300, 0.0))
                .unwrap()
                + PI / 4.0)
                .abs()
                < 1e-12
        );
    }

    #[test]
    fn a_direction_takes_the_sense_of_another_and_none_without_length() {
        let y = Vec3::new(0.1, 1.0, 0.0);
        assert_eq!(
            Vec3::new(0.0, -2.0, 0.0).with_sense_of(y),
            Some(Vec3::new(0.0, 2.0, 0.0))
        );
        assert_eq!(
            Vec3::new(0.0, 2.0, 0.0).with_sense_of(y),
            Some(Vec3::new(0.0, 2.0, 0.0))
        );
        // Not along one line at all: the sense of the nearer half.
        assert_eq!(
            Vec3::new(1.0, -1.0, 0.0).with_sense_of(Vec3::new(0.0, 1.0, 0.0)),
            Some(Vec3::new(-1.0, 1.0, 0.0))
        );
        assert_eq!(Vec3::new(0.0, 0.0, 0.0).with_sense_of(y), None);
        assert_eq!(y.with_sense_of(Vec3::new(0.0, 0.0, 0.0)), None);
        assert_eq!(Vec3::new(f64::NAN, 1.0, 0.0).with_sense_of(y), None);
        // Squared lengths that overflow keep their sense (#21447 review).
        assert_eq!(
            Vec3::new(1e200, 0.0, 0.0).with_sense_of(Vec3::new(-1.0, 0.0, 0.0)),
            Some(Vec3::new(-1e200, 0.0, 0.0))
        );
        assert_eq!(
            Vec3::new(0.0, 1e200, 0.0).with_sense_of(Vec3::new(0.0, 1e300, 0.0)),
            Some(Vec3::new(0.0, 1e200, 0.0))
        );
        // Tiny but above the zero-length tolerance: still compared.
        assert_eq!(
            Vec3::new(0.0, -1e-9, 0.0).with_sense_of(y),
            Some(Vec3::new(0.0, 1e-9, 0.0))
        );
    }

    /// Fuzz regression: when `|v|^2` overflows, upstream's unit vector underflows to zero
    /// and it fails; here the vector is scaled first, so it has a frame.
    #[test]
    fn ortho_vectors_on_overflow() {
        let v = Vec3::new(1.0, -1.0, -1e300);
        let [x, y, z] = v.make_unit_ortho_vectors(None).unwrap();
        assert!((x.as_vec() + Vec3::Z).length() < 1e-15);
        for (a, b) in [(x, y), (y, z), (z, x)] {
            assert!(a.dot(*b).abs() < 1e-15);
        }
        assert!(Vec3::ZERO.make_unit_ortho_vectors(None).is_err());
        assert!(Vec3::new(f64::NAN, 0.0, 1.0)
            .make_unit_ortho_vectors(None)
            .is_err());
    }
    use crate::math::EFF_ZERO_RAD;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= EFF_ZERO
    }

    fn close3(v: Vec3, x: f64, y: f64, z: f64) -> bool {
        close(v.x, x) && close(v.y, y) && close(v.z, z)
    }

    /// `my_test_vector3d` (`tests/prog_test/src/smCurveTest.cpp:1907`), owned parts.
    #[test]
    fn upstream_my_test_vector3d() {
        let mut v3 = Vec3::from([1.0, 2.0, 2.0]);
        assert!(close(v3.length_squared(), 9.0));
        let mut v4 = Vec3::new(3.0, 4.0, 0.0);
        assert!(close(v4.length(), 5.0));
        let mut v5 = v4;
        v4 = -v4;
        let mut v6 = v5;
        v6 += v4;
        assert!(close3(v6, 0.0, 0.0, 0.0));
        v6 -= v4;
        assert!(close3(v6, 3.0, 4.0, 0.0));

        v3 = v3.normalized().unwrap().as_vec();
        assert!(close(v3.length(), 1.0));
        assert!(close(v3.dot(v3), 1.0));

        let v8 = Vec3::new(5.0, 1.0, 2.0);
        let v9 = Vec3::new(1.0, 3.0, 6.0);
        assert!(close(v8.to_point().distance(v9.to_point()), 6.0));
        assert!(close(v8.angle_between(v9).unwrap(), 1.0022805307621478));
        v5 = Vec3::new(0.0, 0.0, 1.0);
        assert!(close(
            v5.ccw_angle_between(v8, v9).unwrap(),
            1.0516502125483738
        ));
        assert!(close(v8.to_point().distance_squared(v4.to_point()), 93.0));
        assert!(!v8.to_point().closer_than(1.0, v9.to_point()));
        assert!(close(v8.min_dimension(), 1.0));
        assert!(close(v8.max_dimension(), 5.0));

        let v10 = Vec3::new(-5.0, -1.0, -2.0);
        assert_eq!(v10 + v8, Vec3::ZERO);
        assert_eq!(v8 - v8, Vec3::ZERO);
        let mut v11 = v8 * 2.0;
        assert!(close3(v11, 10.0, 2.0, 4.0));
        v11 *= 3.0;
        assert!(close3(v11, 30.0, 6.0, 12.0));
        assert!(close3(v8 / 2.0, 2.5, 0.5, 1.0));
        v11 /= 2.0;
        assert!(close3(v11, 15.0, 3.0, 6.0));
        assert!(close3(v8.multiply(v9), 5.0, 3.0, 12.0));
        assert_eq!(v8[Axis::X], 5.0);
        v11[Axis::Y] = 7.0;
        assert_eq!(v11.y, 7.0);
        let mut p = v8.to_point();
        p[Axis::Z] = -1.0;
        assert_eq!(Axis::ALL.map(|a| p[a]), [5.0, 1.0, -1.0]);

        let mut tmp0 = Vec3::new(0.0, 0.0, 1.0);
        let mut tmp1 = Vec3::new(0.0, 1.0, 0.0);
        let mut v21 = tmp1.to_point() - tmp0.to_point();
        assert!(close3(v21, 0.0, 1.0, -1.0));
        let mut v22 = Vec3::from(Vec2::new(1.0, 0.0));
        assert!(close3(v22, 1.0, 0.0, 0.0));
        assert!(!v21.is_invalid());

        tmp0 = Vec3::new(1.0, 1.0, 1.0);
        tmp1 = Vec3::new(3.0, 3.0, 3.0);
        assert!(tmp0.is_colinear_with(tmp1, Point3::ORIGIN, Point3::new(1.0, 1.0, 1.0)));
        assert!(tmp0.is_parallel_to(tmp1, 1.0e-6_f64.to_radians()));
        tmp1 = Vec3::new(-3.0, 2.0, 1.0);
        assert!(tmp0.is_perpendicular_to(tmp1, 1.0e-6_f64.to_radians()));
        v21 = Vec3::new(0.0, 2.0, 1.0);
        assert!(tmp0.is_inside_sector(tmp1, v21, v22));
        assert!(close(tmp0.triple_product(v21, v22), -1.0));

        tmp0 = Vec3::new(0.0, 1.0, 0.0);
        tmp1 = Vec3::new(0.0, 2.0, 1.0);
        let [x, y, z] = tmp0.make_unit_ortho_vectors(Some(tmp1)).unwrap();
        assert!(close3(*x, 0.0, 1.0, 0.0));
        assert!(close3(*y, 0.0, 0.0, 1.0));
        assert!(close3(*z, 1.0, 0.0, 0.0));
        v22 = *y;

        tmp1 = tmp1.unit_or_z();
        assert!(close3(tmp0.project_to_plane(tmp1), 0.0, 0.2, -0.4));
        assert!(close3(
            tmp0.to_point()
                .project_point_to_plane(v22.to_point(), tmp1)
                .to_vec(),
            0.0,
            0.6,
            -0.2
        ));

        let half_pi = core::f64::consts::FRAC_PI_2;
        tmp1 = Vec3::new(0.0, 0.0, 1.0);
        assert!(close3(
            tmp0.rotate_vec_about_axis(tmp1, half_pi),
            -1.0,
            0.0,
            0.0
        ));
        assert!(close3(tmp0.normal_in_plane(tmp1), 0.0, 0.0, 1.0));
        assert!(close3(tmp0.unitized_derivative(tmp1), 0.0, 0.0, 1.0));

        let tmp2 = Vec3::ZERO;
        let [d1, d2] = tmp0.unitized_derivative2(tmp1, tmp2).unwrap();
        assert!(close3(d1, 0.0, 0.0, 1.0));
        assert!(close3(d2, 0.0, -1.0, 0.0));
        let [d1, d2, d3] = tmp0.unitized_derivative3(tmp1, tmp2, Vec3::ZERO).unwrap();
        assert!(close3(d1, 0.0, 0.0, 1.0));
        assert!(close3(d2, 0.0, -1.0, 0.0));
        assert!(close3(d3, 0.0, 0.0, -3.0));

        let dc = tmp0.derivative_of_cross_product(tmp1, Vec3::new(-1.0, 0.0, 0.0), Vec3::ZERO);
        assert!(close3(dc, 0.0, 1.0, 0.0));

        let sol = Vec3::new(0.0, 3.0, 6.0)
            .solve_two_linear_equations(Vec3::new(5.0, 2.0, 9.0))
            .unwrap();
        assert!(close(sol.x, 1.0) && close(sol.y, 2.0));

        let mut a = Vec3::new(2.0, 3.0, 1.0);
        let mut b = Vec3::new(4.0, 1.0, 5.0);
        core::mem::swap(&mut a, &mut b);
        assert!(close3(a.cross(b), -14.0, 6.0, 10.0));

        assert!(Vec3::new(3.0, 6.0, 5.0).approx_eq(Vec3::new(3.0, 6.0, 5.0)));
        assert!(!Vec3::new(3.0, 6.0, 5.0).approx_eq(Vec3::new(3.0, 7.0, 5.0)));
    }

    #[test]
    fn normalized_edge_cases() {
        assert_eq!(Vec3::ZERO.normalized(), None);
        assert_eq!(Vec3::new(1e-13, 0.0, 0.0).normalized(), None);
        assert_eq!(Vec3::new(1e-13, 0.0, 0.0).unit_or_z(), Vec3::Z);
        let n = Vec3::new(0.6, 0.0, 0.8);
        assert_eq!(n.normalized().unwrap().as_vec(), n);
        assert_eq!(
            Vec3::new(3.0, 0.0, 4.0).normalized().unwrap().as_vec(),
            Vec3::new(0.6, 0.0, 0.8)
        );
        // Not finite: no direction. Overflowing square: scaled, so it has one.
        assert_eq!(Vec3::new(f64::NAN, 0.0, 1.0).normalized(), None);
        assert_eq!(Vec3::new(0.0, f64::INFINITY, 1.0).normalized(), None);
        let big = Vec3::new(0.0, 3e200, -4e200).normalized().unwrap();
        assert!((big.as_vec() - Vec3::new(0.0, 0.6, -0.8)).length() < 1e-15);
    }

    #[test]
    fn length_and_unit_holds_at_any_finite_length() {
        // (3, 0, 4) scaled so its squared length underflows or overflows
        let unit = Vec3::new(0.6, 0.0, 0.8);
        for s in [1.0, 1e-200, 1e200, 1e-300, 1e300] {
            let (length, u) = Vec3::new(3.0 * s, 0.0, 4.0 * s).length_and_unit().unwrap();
            assert!((length / s - 5.0).abs() < 1e-12, "{s:e}: {length:e}");
            assert!((*u - unit).length() < 1e-15, "{s:e}: {u:?}");
        }
        let (length, u) = Vec3::new(f64::MAX, f64::MAX, 0.0)
            .length_and_unit()
            .unwrap();
        assert_eq!(length, f64::INFINITY);
        let half = core::f64::consts::FRAC_1_SQRT_2;
        assert!((*u - Vec3::new(half, half, 0.0)).length() < 1e-15, "{u:?}");
        for v in [
            Vec3::ZERO,
            Vec3::new(-0.0, 0.0, -0.0),
            Vec3::new(f64::NAN, 0.0, 1.0),
            Vec3::new(0.0, f64::NEG_INFINITY, 1.0),
        ] {
            assert_eq!(v.length_and_unit(), None, "{v:?}");
        }
    }

    #[test]
    fn a_canonical_axis_direction_is_one_orientation_at_any_scale() {
        let half = core::f64::consts::FRAC_1_SQRT_2;
        // unit vectors: the largest component turns positive, the first of equal ones decides
        for (v, expected) in [
            (Vec3::new(0.0, -1.0, 0.0), Vec3::Y),
            (Vec3::new(0.0, 0.0, 1.0), Vec3::Z),
            (Vec3::new(half, -half, 0.0), Vec3::new(half, -half, 0.0)),
            (Vec3::new(-half, half, 0.0), Vec3::new(half, -half, 0.0)),
        ] {
            assert_eq!(v.canonical_axis_direction(), expected, "{v:?}");
        }
        // X within a relative 1e-12 of the larger Y still ties first, whichever way the axis points
        let (x, y) = (half, half * (1.0 + 1e-12));
        for v in [Vec3::new(-x, y, 0.0), Vec3::new(x, -y, 0.0)] {
            assert_eq!(v.canonical_axis_direction(), Vec3::new(x, -y, 0.0), "{v:?}");
        }
        // below 1e-9 a zero component never decides; the length is kept
        for (y, z) in [(1e-10, 0.0), (0.0, 1e-200), (0.0, 5e-324)] {
            for sign in [1.0, -1.0] {
                let v = Vec3::new(0.0, sign * y, sign * z);
                assert_eq!(v.canonical_axis_direction(), Vec3::new(0.0, y, z), "{v:?}");
            }
        }
        assert_eq!(Vec3::ZERO.canonical_axis_direction(), Vec3::ZERO);
    }

    /// An axis comes out unit and pointing its largest component positive, whichever way it
    /// was given; a vector too short or not finite has no direction.
    #[test]
    fn a_unit_axis_direction_is_unit_and_oriented() {
        let unit = |x, y, z| {
            Vec3::new(x, y, z)
                .unit_axis_direction(1e-9)
                .map(UnitVec3::as_vec)
        };
        assert_eq!(unit(0.0, -2.0, 0.0), Some(Vec3::Y));
        assert_eq!(unit(0.0, 0.0, 4.0), Some(Vec3::Z));
        assert_eq!(unit(-3.0, 4.0, 0.0), Some(Vec3::new(-0.6, 0.8, 0.0)));
        assert_eq!(unit(0.0, 0.0, 0.0), None);
        assert_eq!(unit(1e-10, 0.0, 0.0), None);
        assert_eq!(unit(f64::NAN, 1.0, 0.0), None);
        assert_eq!(unit(f64::INFINITY, 0.0, 0.0), None);
    }

    #[test]
    fn degenerate_paths() {
        // Zero-length self: derivatives pass through.
        let d = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(Vec3::ZERO.unitized_derivative(d), d);
        assert_eq!(Vec3::ZERO.unitized_derivative2(d, -d), Err([d, -d]));
        assert_eq!(Vec3::ZERO.unitized_derivative3(d, d, d), Err([d, d, d]));
        // Zero vectors are neither parallel nor perpendicular.
        assert_eq!(Vec3::ZERO.alignment(Vec3::X, 1.0), Alignment::Neither);
        assert!(!Vec3::X.is_perpendicular_to(Vec3::ZERO, 1.0));
        assert_eq!(
            Vec3::X.alignment(-Vec3::X, EFF_ZERO_RAD),
            Alignment::Opposite
        );
        assert_eq!(Vec3::X.alignment(Vec3::X, EFF_ZERO_RAD), Alignment::Same);
        assert_eq!(Vec3::X.alignment(Vec3::Y, 1.0), Alignment::Neither);
        assert!(Vec3::X.angle_between(Vec3::ZERO).is_err());
        // Parallel to the projection axis gives 0.
        assert_eq!(Vec3::Z.ccw_angle_between(Vec3::Z, Vec3::X).unwrap(), 0.0);
        assert!(close(
            Vec3::Z.ccw_angle_between(Vec3::Y, Vec3::X).unwrap(),
            -PI / 2.0
        ));
        assert!(Vec3::ZERO.make_unit_ortho_vectors(None).is_err());
        let [x, y, z] = Vec3::new(0.0, 5.0, 0.0)
            .make_unit_ortho_vectors(None)
            .unwrap();
        assert_eq!((*x, *y, *z), (Vec3::Y, Vec3::Z, Vec3::X));
        assert!(Vec3::new(1.0, 2.0, 3.0)
            .solve_two_linear_equations(Vec3::new(2.0, 4.0, 6.0))
            .is_err());
        // Rotation about the vector itself is the identity.
        assert_eq!(Vec3::Z.rotate_vec_about_axis(Vec3::Z, 1.0), Vec3::Z);
        assert!(Vec3::X.is_colinear_with(
            Vec3::Y,
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0)
        ));
    }

    #[test]
    fn operators_and_predicates() {
        let mut v = Vec3::new(1.0, 2.0, 3.0);
        v /= 1e-13;
        assert_eq!(v, Vec3::new(1.0, 2.0, 3.0));
        let mut z = Vec3::ZERO;
        z /= 0.0;
        assert_eq!(z, Vec3::ZERO);
        assert!((Vec3::ZERO / 0.0).x.is_nan());
        assert!(Vec3::new(f64::NAN, 0.0, 0.0).is_invalid());
        assert!(Vec3::new(1e19, 0.0, 0.0).is_invalid());
        assert!(!Vec3::new(1e17, 0.0, 0.0).is_invalid());
        assert!(Vec3::new(0.0, 0.0, NOZ).is_2d());
        assert_eq!(
            Vec3::from_nl([2.0, 4.0, NOZ, 2.0]),
            Vec3::new(1.0, 2.0, 0.0)
        );
        assert_eq!(
            Vec3::from_nl([2.0, 4.0, 6.0, NOW]),
            Vec3::new(2.0, 4.0, 6.0)
        );
        assert_eq!(sqrt(-1.0), 0.0);
        assert_eq!(cbrt(27.0), 3.0);
        assert_eq!(acos(1.5), 0.0);
        assert_eq!(acos(-1.5), PI);
        assert_eq!(asin(-1.5), -PI / 2.0);
    }
}
