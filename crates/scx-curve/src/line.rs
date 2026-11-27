// Derived from solidcore `solidcore/crates/solidcore/src/curve/line.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/src/SmLine.cpp`,
// `source/SMLib/inc/SmLine.h`.
// Changes: the analytic line only, as plain data: `Line { origin, direction }` replaces
// `SmLine`'s point, unit vector and scale, its STEP domain and its NURB twin (63 X4, research
// 63 §3.2). The parameter is the pin's STEP parameter (`point + t * vector * scale`, 63 D13),
// over every `t`: a bounded line is a restriction (CU3). A direction shorter than the pin's
// `SM_EFF_ZERO` is refused (`CurveError`) where `from_points` built a zero line over
// `[0, 0]`, and non-finite data is refused where the pin built it. `evaluate_step` becomes
// `point` and the fixed-order `jet::<D>`, one scalar-generic evaluator (`point_over`,
// `jet_over`). The 2-D line (`Line2`) is the same in the plane, where the pin used a 3-D
// line of dimension 2.

use scx_num::domain::ParamDomain;
use scx_num::jet::{CurveJet, CurveJet2};
use scx_num::math::EFF_ZERO;
use scx_num::ring::Ring;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;

use crate::error::{CurveError, Degenerate, Part};

/// The line `origin + t direction`, `t` over every real.
///
/// `direction` is the velocity, kept as given: `t = 0` is `origin`, `t = 1` is
/// `origin + direction`. A line made [`Line::through`] two points runs from the first at
/// `t = 0` to the second at `t = 1`, as the pin's `SmLine(StartPoint, EndPoint)` does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    origin: Point3,
    direction: Vec3,
}

/// The line `origin + t direction` in the plane, `t` over every real.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line2 {
    origin: Point2,
    direction: Vec2,
}

impl Line {
    /// The line through `origin` with velocity `direction`.
    ///
    /// # Errors
    ///
    /// [`CurveError::NotFinite`] for an infinite or NaN coordinate;
    /// [`Degenerate::Direction`] for a direction shorter than
    /// [`EFF_ZERO`](scx_num::math::EFF_ZERO).
    pub fn new(origin: Point3, direction: Vec3) -> crate::Result<Self> {
        if !origin.is_finite() {
            return Err(CurveError::NotFinite(Part::Origin));
        }
        if !direction.is_finite() {
            return Err(CurveError::NotFinite(Part::Direction));
        }
        direction_ok(direction.length())?;
        Ok(Self { origin, direction })
    }

    /// The line from `start` at `t = 0` to `end` at `t = 1`.
    ///
    /// # Errors
    ///
    /// As [`Line::new`] with the direction `end - start` (which overflows to non-finite for
    /// points too far apart).
    pub fn through(start: Point3, end: Point3) -> crate::Result<Self> {
        if !start.is_finite() {
            return Err(CurveError::NotFinite(Part::Origin));
        }
        Self::new(start, end - start)
    }

    /// The point at `t = 0`.
    #[must_use]
    pub const fn origin(&self) -> Point3 {
        self.origin
    }

    /// The velocity: the first derivative everywhere.
    #[must_use]
    pub const fn direction(&self) -> Vec3 {
        self.direction
    }

    /// Every real.
    #[must_use]
    pub const fn domain(&self) -> ParamDomain {
        ParamDomain::Unbounded
    }

    /// The point at `t`.
    #[must_use]
    pub fn point(&self, t: f64) -> Point3 {
        self.point_over(t)
    }

    /// The point and the first `D` derivatives at `t`. A line is smooth, so `side` changes
    /// nothing.
    #[must_use]
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet<D> {
        let _ = side;
        self.jet_over(t)
    }

    /// The point at `t` in the scalar `T`: `origin + direction t` (an enclosure in
    /// [`Ivl`](scx_num::interval::Ivl)).
    #[must_use]
    pub fn point_over<T: Ring>(&self, t: T) -> Point3<T> {
        self.origin.map(T::lift) + self.direction.map(T::lift) * t
    }

    /// The point and the first `D` derivatives at `t` in the scalar `T`: the direction,
    /// then zeros.
    #[must_use]
    pub fn jet_over<T: Ring, const D: usize>(&self, t: T) -> CurveJet<D, T> {
        let d1 = self.direction.map(T::lift);
        let zero = Vec3::splat(T::lift(0.0));
        CurveJet {
            point: self.point_over(t),
            d: core::array::from_fn(|k| if k == 0 { d1 } else { zero }),
        }
    }
}

impl Line2 {
    /// The line through `origin` with velocity `direction`.
    ///
    /// # Errors
    ///
    /// As [`Line::new`].
    pub fn new(origin: Point2, direction: Vec2) -> crate::Result<Self> {
        if !(origin.x.is_finite() && origin.y.is_finite()) {
            return Err(CurveError::NotFinite(Part::Origin));
        }
        if !(direction.x.is_finite() && direction.y.is_finite()) {
            return Err(CurveError::NotFinite(Part::Direction));
        }
        direction_ok(direction.length())?;
        Ok(Self { origin, direction })
    }

    /// The line from `start` at `t = 0` to `end` at `t = 1`.
    ///
    /// # Errors
    ///
    /// As [`Line::through`].
    pub fn through(start: Point2, end: Point2) -> crate::Result<Self> {
        if !(start.x.is_finite() && start.y.is_finite()) {
            return Err(CurveError::NotFinite(Part::Origin));
        }
        Self::new(start, end - start)
    }

    /// The point at `t = 0`.
    #[must_use]
    pub const fn origin(&self) -> Point2 {
        self.origin
    }

    /// The velocity: the first derivative everywhere.
    #[must_use]
    pub const fn direction(&self) -> Vec2 {
        self.direction
    }

    /// Every real.
    #[must_use]
    pub const fn domain(&self) -> ParamDomain {
        ParamDomain::Unbounded
    }

    /// The point at `t`.
    #[must_use]
    pub fn point(&self, t: f64) -> Point2 {
        self.point_over(t)
    }

    /// The point and the first `D` derivatives at `t`; `side` changes nothing.
    #[must_use]
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet2<D> {
        let _ = side;
        self.jet_over(t)
    }

    /// The point at `t` in the scalar `T`.
    #[must_use]
    pub fn point_over<T: Ring>(&self, t: T) -> Point2<T> {
        self.origin.map(T::lift) + self.direction.map(T::lift) * t
    }

    /// The point and the first `D` derivatives at `t` in the scalar `T`.
    #[must_use]
    pub fn jet_over<T: Ring, const D: usize>(&self, t: T) -> CurveJet2<D, T> {
        let d1 = self.direction.map(T::lift);
        let zero = Vec2::new(T::lift(0.0), T::lift(0.0));
        CurveJet2 {
            point: self.point_over(t),
            d: core::array::from_fn(|k| if k == 0 { d1 } else { zero }),
        }
    }
}

/// The pin's degenerate-line test (`|end - start| < SM_EFF_ZERO`); a NaN length cannot
/// reach here (the coordinates are finite) and an overflowed one is long enough.
fn direction_ok(length: f64) -> crate::Result<()> {
    if length < EFF_ZERO {
        return Err(CurveError::Degenerate(Degenerate::Direction { length }));
    }
    Ok(())
}
