// Moved from solidcore `solidcore/crates/solidcore/src/geom/vector2d.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/inc/SmVector2d.h`,
// `source/SMLib/src/SmVector2d.cpp`.
// Changes: errors are `NumError` / `NumFailure` (`SolidError`, `GeomFailure` at the pin); paths.
// No `SM_UNDEF_DOUBLE` sentinel (`docs/01-architecture.md` §10.1): `UNDEF`, `set_uninitialized`
// and `is_initialized` are gone (an unset vector is `None`); the test
// `upstream_my_test_vector2d` drops its unset-vector checks.
// Then the transcendental functions became `fmath`'s (correctly rounded, the same bits on every
// platform) in place of the C library's, so results may differ from the pin's in the last bit.
//! Two-dimensional vectors and points ([`Vec2`] / [`Point2`]), usually surface
//! parameters `(u, v)`: arithmetic, lengths, distances, unitizing, rotation and angles.
//!
//! Upstream: `source/SMLib/inc/SmVector2d.h`, `source/SMLib/src/SmVector2d.cpp`.
//!
//! | Rust                                   | Upstream                                         |
//! |----------------------------------------|--------------------------------------------------|
//! | [`Vec2`]                               | `SmVector2d`                                     |
//! | [`Point2`]                             | `SmPoint2d` (same class upstream; here distinct) |
//! | [`UnitVec2`]                           | a unitized `SmVector2d`                          |
//! | [`Vec2::new`]                          | `SmVector2d(double, double)`, `Set`              |
//! | [`Vec2::from_array`], `From<[f64; 2]>` | `SmVector2d(double *)`                           |
//! | `Copy` / assignment                    | copy constructor, `operator=(const SmVector2d&)`, `Copy` |
//! | [`Vec2::from_xy`]                      | `operator=(const SmVector3d&)`                   |
//! | [`Vec2::is_zero`]                      | `IsZero`                                         |
//! | [`Vec2::length_squared`]               | `LengthSquared`                                  |
//! | [`Vec2::length`]                       | `Length`                                         |
//! | [`Vec2::dot`]                          | `Dot`                                            |
//! | `Neg`, `Add`, `Sub`                    | `operator-(v)`, `operator+`, `operator-`         |
//! | `Mul<f64>`, `f64 * Vec2`               | `operator*(v, d)`, `operator*(d, v)`             |
//! | `Div<f64>`                             | `operator/(v, d)`                                |
//! | `AddAssign`, `SubAssign`, `MulAssign`  | `operator+=`, `operator-=`, `operator*=`         |
//! | [`Vec2::normalized`]                   | `Unitize`                                        |
//! | [`Point2::distance`]                   | `DistanceBetween`                                |
//! | [`Point2::distance_squared`]           | `DistanceBetweenSquared`                         |
//! | [`Point2::closer_than`]                | `CloserThan`                                     |
//! | [`Vec2::max_dimension`]                | `GetMaxDimension`                                |
//! | [`Vec2::swap_xy`]                      | `SwapXY`                                         |
//! | [`Vec2::rotate`]                       | `Rotate`                                         |
//! | [`Vec2::approx_eq`]                    | `operator==`                                     |
//! | [`Vec2::approx_ne`]                    | `operator!=`                                     |
//! | [`Vec2::component`]                    | `operator[](ULONG) const`                        |
//! | [`Vec2::component_mut`]                | `operator[](ULONG)`                              |
//! | [`Vec2::angle_between`]                | `AngleBetween`                                   |
//! | [`Vec2::ccw_angle_between`]            | `CCWAngleBetween`                                |
//!
//! Deviations:
//! - Out-params and `SmStatus` become return values: `angle_between` and
//!   `ccw_angle_between` return `Result`; `SM_ERR_INVALID_INPUT` is
//!   [`NumError::InvalidInput`]. `Unitize` is [`Vec2::normalized`], an `Option` of a
//!   [`UnitVec2`]: `None` where upstream fails (and leaves the vector unchanged), and also
//!   for a non-finite vector, which upstream "unitizes" to NaN. An overflowing square is
//!   scaled first.
//! - Points and vectors are distinct types (upstream: one class): distances are on
//!   [`Point2`], lengths and angles on [`Vec2`].
//! - Upstream's tolerant `operator==` / `operator!=` are [`Vec2::approx_eq`] /
//!   [`Vec2::approx_ne`]; Rust `==` (`PartialEq`) is exact component equality.
//! - `Default` is the zero vector. Upstream's default constructor gives an unset vector
//!   (both components `SM_UNDEF_DOUBLE`); here an unset vector is an `Option<Vec2>`.
//! - `operator[]` with an index other than 0 or 1 returns `0.0` (const) or
//!   dereferences `NULL` (non-const) after logging `SM_ERR`; Rust returns
//!   a [`NumFailure`](crate::error::NumFailure) with status 1001 (`SM_ERR`) for both.
//! - `SM_ASSERT_BREAK` checks are compiled out upstream (release build) and omitted.
//! - `sm_sqrt`, `sm_acos`, `sm_asin` (`SmMath.cpp`) come from [`crate::vector3d`];
//!   `SM_IS_ZERO_SQUARED` = `SmTol::InTol(a, SmTol::GetScaledZeroSq())` calls
//!   [`crate::tol`].

use core::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

use crate::error::{NumError, NumFailure, Result};
use crate::math::{Vec3, EFF_ZERO, EFF_ZERO_SQ, PI};
use crate::tol;
use crate::vector3d::{acos as sm_acos, asin as sm_asin, sqrt as sm_sqrt};

/// A 2D displacement or direction (`SmVector2d` in its vector role), such as a step in a
/// surface's parameters. A position is a [`Point2`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    /// X (or `u`) component.
    pub x: f64,
    /// Y (or `v`) component.
    pub y: f64,
}

/// A 2D position (`SmPoint2d`, an alias of `SmVector2d` upstream), typically a surface
/// parameter pair `(u, v)`. Affine as [`Point3`](crate::vector3d::Point3) is: `point - point`
/// is a [`Vec2`], `point ± vec` a point.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point2 {
    /// X (or `u`) coordinate.
    pub x: f64,
    /// Y (or `v`) coordinate.
    pub y: f64,
}

/// A 2D direction: a [`Vec2`] of length 1 (its square within [`EFF_ZERO_SQ`] of 1), both
/// components finite. Made by [`Vec2::normalized`]; read through `Deref`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitVec2(Vec2);

impl UnitVec2 {
    /// The direction as a plain vector.
    #[must_use]
    pub const fn as_vec(self) -> Vec2 {
        self.0
    }
}

impl core::ops::Deref for UnitVec2 {
    type Target = Vec2;
    fn deref(&self) -> &Vec2 {
        &self.0
    }
}

impl From<UnitVec2> for Vec2 {
    fn from(u: UnitVec2) -> Self {
        u.0
    }
}

impl Vec2 {
    /// The zero vector.
    pub const ZERO: Self = Self::new(0.0, 0.0);

    /// Creates a vector from its components (`SmVector2d(dX, dY)`, `Set`).
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Creates a vector from `[x, y]` (`SmVector2d(double *)`).
    #[must_use]
    pub const fn from_array(a: [f64; 2]) -> Self {
        Self::new(a[0], a[1])
    }

    /// Components as an array `[x, y]`.
    #[must_use]
    pub const fn to_array(self) -> [f64; 2] {
        [self.x, self.y]
    }

    /// The `x` and `y` of a 3D vector, dropping `z`
    /// (`operator=(const SmVector3d&)`).
    #[must_use]
    pub const fn from_xy(v: Vec3) -> Self {
        Self::new(v.x, v.y)
    }

    /// The point this displacement reaches from the origin: the same coordinates.
    #[must_use]
    pub const fn to_point(self) -> Point2 {
        Point2::new(self.x, self.y)
    }

    /// True when the length is below `tol` (`IsZero`; upstream default
    /// `tol` is [`EFF_ZERO`]).
    #[must_use]
    pub fn is_zero(self, tol: f64) -> bool {
        self.length_squared() < tol * tol
    }

    /// Squared Euclidean length, `(x*x) + (y*y)` (`LengthSquared`).
    #[must_use]
    pub fn length_squared(self) -> f64 {
        (self.x * self.x) + (self.y * self.y)
    }

    /// Euclidean length (`Length`).
    #[must_use]
    pub fn length(self) -> f64 {
        sm_sqrt((self.x * self.x) + (self.y * self.y))
    }

    /// Dot product, `(x*o.x) + (y*o.y)` (`Dot`).
    #[must_use]
    pub fn dot(self, o: Self) -> f64 {
        (self.x * o.x) + (self.y * o.y)
    }

    /// The unit vector in this direction (`Unitize`): `None` for a vector whose squared
    /// length is below [`EFF_ZERO_SQ`] or is not finite.
    ///
    /// A vector whose squared length is within [`EFF_ZERO_SQ`] of 1 is returned unchanged.
    /// One whose squared length overflows is first divided by its largest coordinate.
    #[must_use]
    pub fn normalized(self) -> Option<UnitVec2> {
        let len_sq = self.length_squared();
        if len_sq.is_infinite() && self.x.is_finite() && self.y.is_finite() {
            return (self / self.max_dimension()).normalized();
        }
        if !(len_sq >= EFF_ZERO_SQ && len_sq.is_finite()) {
            return None;
        }
        if (len_sq - 1.0).abs() > EFF_ZERO_SQ {
            let len = sm_sqrt(len_sq);
            return Some(UnitVec2(Self::new(self.x / len, self.y / len)));
        }
        Some(UnitVec2(self))
    }

    /// Largest absolute coordinate (`GetMaxDimension`).
    #[must_use]
    pub fn max_dimension(self) -> f64 {
        if self.x.abs() > self.y.abs() {
            self.x.abs()
        } else {
            self.y.abs()
        }
    }

    /// The vector with `x` and `y` exchanged (`SwapXY`).
    #[must_use]
    pub const fn swap_xy(self) -> Self {
        Self::new(self.y, self.x)
    }

    /// The vector rotated counter-clockwise by `ang_rad` about the Z axis (`Rotate`).
    #[must_use]
    pub fn rotate(self, ang_rad: f64) -> Self {
        let cos = crate::fmath::cos(ang_rad);
        let sin = crate::fmath::sin(ang_rad);
        Self::new(cos * self.x - sin * self.y, sin * self.x + cos * self.y)
    }

    /// Upstream's tolerant equality (`operator==`): the L1 distance is below
    /// `EFF_ZERO * (1 + self.max_dimension())`. Not symmetric in general.
    #[must_use]
    pub fn approx_eq(self, o: Self) -> bool {
        let scale = 1.0 + self.max_dimension();
        let dist = (self.x - o.x).abs() + (self.y - o.y).abs();
        dist < EFF_ZERO * scale
    }

    /// Upstream's tolerant inequality (`operator!=`): the L1 distance is at least
    /// `EFF_ZERO * (1 + self.max_dimension())`. Both this and [`Vec2::approx_eq`]
    /// are false when the distance is NaN.
    #[must_use]
    pub fn approx_ne(self, o: Self) -> bool {
        let scale = 1.0 + self.max_dimension();
        let dist = (self.x - o.x).abs() + (self.y - o.y).abs();
        dist >= EFF_ZERO * scale
    }

    /// Component `0` (`x`) or `1` (`y`) (`operator[] const`).
    pub fn component(self, index: usize) -> Result<f64> {
        match index {
            0 => Ok(self.x),
            1 => Ok(self.y),
            _ => Err(Self::bad_index(index)),
        }
    }

    /// Mutable component `0` (`x`) or `1` (`y`) (`operator[]`).
    pub fn component_mut(&mut self, index: usize) -> Result<&mut f64> {
        match index {
            0 => Ok(&mut self.x),
            1 => Ok(&mut self.y),
            _ => Err(Self::bad_index(index)),
        }
    }

    fn bad_index(index: usize) -> NumError {
        NumError::failed(
            "geom::vector2d::Vec2::component",
            NumFailure::IndexOutOfRange { index, len: 2 },
        )
    }

    /// Angle between two vectors in radians, in `[0, PI]` (`AngleBetween`).
    ///
    /// Uses `acos` of the normalised dot product when its square is below `0.81`,
    /// else `asin` of the normalised cross product (more accurate near `0` and `PI`).
    /// Fails when either vector has squared length within `EFF_ZERO_SQ` of zero.
    pub fn angle_between(self, o: Self) -> Result<f64> {
        let v1_len_sq = self.length_squared();
        let v2_len_sq = o.length_squared();

        if tol::is_zero_squared(v1_len_sq) || tol::is_zero_squared(v2_len_sq) {
            return Err(NumError::InvalidInput {
                operation: "geom::vector2d::Vec2::angle_between",
                reason: "zero-length vector",
            });
        }
        let dot_prep = self.dot(o);
        let dot_sign = if dot_prep < 0.0 { -1.0 } else { 1.0 };
        let dot_squared = (dot_prep * dot_prep) / v1_len_sq / v2_len_sq;
        let dot = dot_sign * sm_sqrt(dot_squared);

        if dot_squared < 0.81 {
            Ok(sm_acos(dot))
        } else {
            let cross_prep = self.x * o.y - self.y * o.x;
            let cross = sm_sqrt((cross_prep * cross_prep) / v1_len_sq / v2_len_sq);
            Ok(if dot < 0.0 {
                PI - sm_asin(cross)
            } else {
                sm_asin(cross)
            })
        }
    }

    /// Counter-clockwise angle from `self` to `o` in radians, in `[0, 2*PI)`
    /// (`CCWAngleBetween`).
    ///
    /// Pairs whose cross product is in `[-2 * scaled_zero, 0)` snap to `0` or `PI`;
    /// tangent vectors give `0`, never `2*PI`.
    pub fn ccw_angle_between(self, o: Self) -> Result<f64> {
        let mut ang = self.angle_between(o)?;
        let a = o.max_dimension();
        let b = self.max_dimension();
        let scaled_zero = EFF_ZERO * (1.0 + if a > b { a } else { b });
        let cross = self.x * o.y - self.y * o.x;
        if cross < -2.0 * scaled_zero {
            ang = 2.0 * PI - ang;
        } else if cross < 0.0 {
            ang = if ang.abs() < 0.1 { 0.0 } else { PI };
        }
        Ok(ang)
    }
}

impl Point2 {
    /// The origin.
    pub const ORIGIN: Self = Self::new(0.0, 0.0);

    /// Creates a point from its coordinates.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Creates a point from `[x, y]`.
    #[must_use]
    pub const fn from_array(a: [f64; 2]) -> Self {
        Self::new(a[0], a[1])
    }

    /// Coordinates as an array `[x, y]`.
    #[must_use]
    pub const fn to_array(self) -> [f64; 2] {
        [self.x, self.y]
    }

    /// The displacement from the origin to this point: the same coordinates.
    #[must_use]
    pub const fn to_vec(self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }

    /// Distance between two points (`DistanceBetween`).
    #[must_use]
    pub fn distance(self, o: Self) -> f64 {
        (self - o).length()
    }

    /// Squared distance between two points (`DistanceBetweenSquared`).
    #[must_use]
    pub fn distance_squared(self, o: Self) -> f64 {
        (self - o).length_squared()
    }

    /// True when the distance to `o` is at most `max` (`CloserThan`).
    ///
    /// Rejects early when either coordinate difference alone exceeds `max`.
    #[must_use]
    pub fn closer_than(self, max: f64, o: Self) -> bool {
        !((self.x - o.x).abs() > max
            || (self.y - o.y).abs() > max
            || ((self.x - o.x) * (self.x - o.x) + (self.y - o.y) * (self.y - o.y)) > max * max)
    }

    /// Largest absolute coordinate (`GetMaxDimension`).
    #[must_use]
    pub fn max_dimension(self) -> f64 {
        self.to_vec().max_dimension()
    }

    /// The point with `x` and `y` exchanged (`SwapXY`).
    #[must_use]
    pub const fn swap_xy(self) -> Self {
        Self::new(self.y, self.x)
    }

    /// The point halfway between `self` and `q`: `(self + q) / 2`, coordinate by coordinate.
    #[must_use]
    pub fn midpoint(self, q: Self) -> Self {
        Self::new((self.x + q.x) * 0.5, (self.y + q.y) * 0.5)
    }

    /// Upstream's tolerant equality (`operator==`) on points, as [`Vec2::approx_eq`].
    #[must_use]
    pub fn approx_eq(self, o: Self) -> bool {
        self.to_vec().approx_eq(o.to_vec())
    }

    /// Upstream's tolerant inequality (`operator!=`) on points, as [`Vec2::approx_ne`].
    #[must_use]
    pub fn approx_ne(self, o: Self) -> bool {
        self.to_vec().approx_ne(o.to_vec())
    }
}

impl From<[f64; 2]> for Vec2 {
    fn from(a: [f64; 2]) -> Self {
        Self::from_array(a)
    }
}

impl From<[f64; 2]> for Point2 {
    fn from(a: [f64; 2]) -> Self {
        Self::from_array(a)
    }
}

impl Neg for Vec2 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

impl Add for Vec2 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f64> for Vec2 {
    type Output = Self;
    fn mul(self, d: f64) -> Self {
        Self::new(self.x * d, self.y * d)
    }
}

impl Mul<Vec2> for f64 {
    type Output = Vec2;
    fn mul(self, v: Vec2) -> Vec2 {
        Vec2::new(v.x * self, v.y * self)
    }
}

/// `operator/(v, d)`: returns `v` unchanged when `|d|` is negligible relative to
/// either component (`|d| < |c| * EFF_ZERO`), instead of dividing.
impl Div<f64> for Vec2 {
    type Output = Self;
    fn div(self, d: f64) -> Self {
        if d.abs() < self.x.abs() * EFF_ZERO {
            return self;
        }
        if d.abs() < self.y.abs() * EFF_ZERO {
            return self;
        }
        Self::new(self.x / d, self.y / d)
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, o: Self) {
        self.x += o.x;
        self.y += o.y;
    }
}

impl SubAssign for Vec2 {
    fn sub_assign(&mut self, o: Self) {
        self.x -= o.x;
        self.y -= o.y;
    }
}

impl MulAssign<f64> for Vec2 {
    fn mul_assign(&mut self, d: f64) {
        self.x *= d;
        self.y *= d;
    }
}

/// A point moved by a displacement.
impl Add<Vec2> for Point2 {
    type Output = Self;
    fn add(self, v: Vec2) -> Self {
        Self::new(self.x + v.x, self.y + v.y)
    }
}

/// A point moved back by a displacement.
impl Sub<Vec2> for Point2 {
    type Output = Self;
    fn sub(self, v: Vec2) -> Self {
        Self::new(self.x - v.x, self.y - v.y)
    }
}

/// The displacement from `o` to `self`.
impl Sub for Point2 {
    type Output = Vec2;
    fn sub(self, o: Self) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}

impl AddAssign<Vec2> for Point2 {
    fn add_assign(&mut self, v: Vec2) {
        self.x += v.x;
        self.y += v.y;
    }
}

impl SubAssign<Vec2> for Point2 {
    fn sub_assign(&mut self, v: Vec2) {
        self.x -= v.x;
        self.y -= v.y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= EFF_ZERO
    }

    /// `my_test_vector2d` (`tests/prog_test/src/smCurveTest.cpp:1758`), owned parts.
    #[test]
    fn upstream_my_test_vector2d() {
        let mut v3 = Vec2::from([1.0, 2.0]);
        assert!(close(v3.length_squared(), 5.0));
        let mut v4 = Vec2::new(3.0, 4.0);
        assert!(close(v4.length(), 5.0));
        let v5 = v4;
        v4 = -v4;
        let mut v6 = v5;
        v6 += v4;
        assert_eq!(v6, Vec2::ZERO);

        v3 = v3.normalized().unwrap().as_vec();
        assert!(close(v3.length(), 1.0));
        assert!(close(v3.dot(v3), 1.0));

        let v8 = Vec2::new(5.0, 1.0);
        let v9 = Vec2::new(1.0, 4.0);
        assert!(close(v8.to_point().distance(v9.to_point()), 5.0));
        assert!(close(v8.angle_between(v9).unwrap(), 1.1284221038181517));
        assert!(close(v9.ccw_angle_between(v8).unwrap(), 5.1547632033614343));
        assert!(close(v8.to_point().distance_squared(v4.to_point()), 89.0));
        assert!(!v8.to_point().closer_than(1.0, v9.to_point()));
        assert!(close(v8.max_dimension(), 5.0));

        let v10 = -v8;
        assert_eq!(v10 + v8, Vec2::ZERO);
        assert_eq!(v8 - v8, Vec2::ZERO);
        let mut v11 = v8 * 2.0;
        assert_eq!(v11, Vec2::new(10.0, 2.0));
        v11 *= 3.0;
        assert_eq!(v11, Vec2::new(30.0, 6.0));
        assert_eq!(v8 / 2.0, Vec2::new(2.5, 0.5));
        assert_eq!(2.0 * v8, v8 * 2.0);
        assert_eq!(v8.component(0).unwrap(), 5.0);
        *v11.component_mut(1).unwrap() = 6.0;
        assert_eq!(v11.y, 6.0);

        let r = v5.rotate(PI / 2.0);
        assert!(close(r.x, -4.0) && close(r.y, 3.0));
    }

    #[test]
    fn normalized_edge_cases() {
        assert_eq!(Vec2::ZERO.normalized(), None);
        assert_eq!(Vec2::new(1e-13, 0.0).normalized(), None);
        // Nearly-unit vectors are returned bit for bit.
        let n = Vec2::new(0.6, 0.8);
        assert_eq!(n.normalized().unwrap().as_vec(), n);
        assert_eq!(
            Vec2::new(0.0, -7.0).normalized().unwrap().as_vec(),
            Vec2::new(0.0, -1.0)
        );
        // Not finite: no direction. Overflowing square: scaled, so it has one.
        assert_eq!(Vec2::new(f64::NAN, 1.0).normalized(), None);
        assert_eq!(Vec2::new(f64::INFINITY, 1.0).normalized(), None);
        let big = Vec2::new(1e200, -1e200).normalized().unwrap();
        assert!((big.length() - 1.0).abs() < 1e-15 && big.x > 0.0 && big.y < 0.0);
    }

    #[test]
    fn angles() {
        let x = Vec2::new(1.0, 0.0);
        let y = Vec2::new(0.0, 2.0);
        assert_eq!(x.angle_between(x).unwrap(), 0.0);
        assert_eq!(x.angle_between(-x).unwrap(), PI);
        assert!(close(x.angle_between(y).unwrap(), PI / 2.0));
        assert!(close(x.ccw_angle_between(y).unwrap(), PI / 2.0));
        assert!(close(y.ccw_angle_between(x).unwrap(), 1.5 * PI));
        assert_eq!(x.ccw_angle_between(x).unwrap(), 0.0);
        // Tiny negative cross product snaps to 0 or PI.
        assert_eq!(x.ccw_angle_between(Vec2::new(1.0, -1e-13)).unwrap(), 0.0);
        assert_eq!(x.ccw_angle_between(Vec2::new(-1.0, -1e-13)).unwrap(), PI);
        assert!(x.angle_between(Vec2::ZERO).is_err());
        assert!(Vec2::ZERO.ccw_angle_between(x).is_err());
    }

    #[test]
    fn division_guard_and_comparisons() {
        let v = Vec2::new(1.0, 2.0);
        assert_eq!(v / 1e-13, v);
        assert_eq!(v / 2.0, Vec2::new(0.5, 1.0));
        assert!((Vec2::ZERO / 0.0).x.is_nan());
        assert!(v.approx_eq(Vec2::new(1.0 + 1e-13, 2.0)));
        assert!(!v.approx_ne(Vec2::new(1.0 + 1e-13, 2.0)));
        assert!(v.approx_ne(Vec2::new(1.0 + 1e-10, 2.0)));
        let nan = Vec2::new(f64::NAN, 0.0);
        assert!(!v.approx_eq(nan) && !v.approx_ne(nan));
        let p = v.to_point();
        assert!(p.closer_than(0.0, p));
        assert!(!p.closer_than(-1.0, p));
        assert_eq!(p + Vec2::new(1.0, 1.0) - p, Vec2::new(1.0, 1.0));
    }

    #[test]
    fn misc() {
        let mut v = Vec2::new(1.0, 2.0);
        assert_eq!(v.swap_xy(), Vec2::new(2.0, 1.0));
        assert!(v.component(2).is_err());
        assert!(v.component_mut(5).is_err());
        v -= Vec2::new(1.0, 1.0);
        assert_eq!(v, Vec2::new(0.0, 1.0));
        assert!(v.is_zero(1.5));
        assert!(!v.is_zero(EFF_ZERO));
        assert_eq!(Vec2::from_xy(Vec3::new(1.0, 2.0, 3.0)), Vec2::new(1.0, 2.0));
        assert_eq!(Vec2::new(-3.0, 3.0).max_dimension(), 3.0);
        assert_eq!(sm_sqrt(-1.0), 0.0);
        assert_eq!(sm_sqrt(-0.0).to_bits(), 0.0f64.to_bits());
        assert_eq!(sm_acos(1.5), 0.0);
        assert_eq!(sm_asin(-1.5), -PI / 2.0);
        assert!(Vec2::new(f64::NAN, 0.0).length().is_nan());
    }
}
