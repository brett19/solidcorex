// Written for solidcorex. Apache-2.0.
//! A curve's jet at a parameter: the point and its first `D` derivatives with respect to
//! the curve's own parameter, in 3-D ([`CurveJet`]) and 2-D ([`CurveJet2`]).
//!
//! `d[k]` is the derivative of order `k + 1`, so `CurveJet<D>` holds what
//! `NurbsCurve::derivs::<{ D + 1 }>` returns as rows (row 0 the point). The scalar `T` is
//! `f64` for values and an interval for enclosures (`crate::trig::Trig`).
//!
//! Upstream: the `Vec<Vec3>` of `SmCurve::Evaluate` (`pDerivs[0..=lNumDerivs]`), here sized
//! by the type, with no sentinel past the order asked for.

use crate::vector2d::{Point2, Vec2};
use crate::vector3d::{Point3, Vec3};

/// A 3-D curve's point and its first `D` derivatives: `d[k]` is `C^(k+1)(t)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurveJet<const D: usize, T = f64> {
    /// `C(t)`.
    pub point: Point3<T>,
    /// `C'(t)`, `C''(t)`, ...
    pub d: [Vec3<T>; D],
}

/// A 2-D curve's point and its first `D` derivatives: `d[k]` is `C^(k+1)(t)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurveJet2<const D: usize, T = f64> {
    /// `C(t)`.
    pub point: Point2<T>,
    /// `C'(t)`, `C''(t)`, ...
    pub d: [Vec2<T>; D],
}
