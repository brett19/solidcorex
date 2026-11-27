// Written for solidcorex. Apache-2.0.
//! The cone `S(u, v) = O + (r + k v) D(u) + v Z` with `k = tan(half-angle)`, in the pin's uv
//! (research 63 D13).
//!
//! Layout, read from the pin (`surface/cone/p1.rs`, `new_unbounded` and
//! `convert_uv_from_step_to_nurbs`): `u` is four quarter arcs from `X` over `[0, 1]`; `v` is
//! the height along `Z`, STEP's `v`. The half-angle is signed as the pin's semi-angle is: a
//! negative one narrows the cone along `Z`, with the apex above `O`. The domain runs from the
//! apex along the nappe that holds the circle of radius `r`.

use scx_num::domain::{ParamDomain, UvDomain};
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::ring::Ring;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_nurbs::Approach;

use crate::Pole;
use crate::arcs::Arcs;
use crate::common::{Axes, check_lengths, linear, unit};
use crate::error::{Size, SurfaceError, at_least_zero};
use scx_num::jet::SurfaceJet;

/// The `u` direction: a turn over `[0, 1]`.
const U: Arcs = Arcs::turn(1.0);

/// A right circular cone about the frame's z axis.
///
/// The slope `k = tan(half_angle)` is computed once at construction, beside the half-angle
/// it comes from; neither changes after.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cone {
    frame: Frame,
    radius: f64,
    half_angle: f64,
    slope: f64,
    sense: Sense,
}

impl Cone {
    /// The cone through the circle of `radius` about `frame`'s origin in its xy plane, opening
    /// at `half_angle` radians to its z axis (signed: `r + tan(half_angle) v` is the radius at
    /// height `v`), `u = 0` on its x axis.
    ///
    /// # Errors
    ///
    /// All `Refused`:
    /// - [`SurfaceError::NotFinite`] or [`SurfaceError::Negative`] for the radius (zero puts
    ///   the apex at the origin);
    /// - [`SurfaceError::NotFinite`] for the half-angle, and [`SurfaceError::Degenerate`] for
    ///   zero (a cylinder, its own kind) or a magnitude of `pi/2` or more (a plane; the pin
    ///   reduces an angle modulo `pi`, which this does not).
    pub fn new(
        frame: Frame,
        radius: Length,
        half_angle: f64,
        sense: Sense,
    ) -> Result<Self, SurfaceError> {
        let radius = at_least_zero(Size::Radius, radius.mm())?;
        if !half_angle.is_finite() {
            return Err(SurfaceError::NotFinite {
                size: Size::HalfAngle,
                value: half_angle,
            });
        }
        if half_angle == 0.0 || half_angle.abs() >= core::f64::consts::FRAC_PI_2 {
            return Err(SurfaceError::Degenerate {
                size: Size::HalfAngle,
                value: half_angle,
            });
        }
        let slope = fmath::tan(half_angle);
        Ok(Self {
            frame,
            radius,
            half_angle,
            slope,
            sense,
        })
    }

    /// The frame.
    #[must_use]
    pub const fn frame(&self) -> &Frame {
        &self.frame
    }

    /// The radius at `v = 0`.
    #[must_use]
    pub const fn radius(&self) -> Length {
        Length::from_mm(self.radius)
    }

    /// The half-angle, radians, signed.
    #[must_use]
    pub const fn half_angle(&self) -> f64 {
        self.half_angle
    }

    /// Whether the axis is the frame's z or its opposite (D14).
    #[must_use]
    pub const fn sense(&self) -> Sense {
        self.sense
    }

    fn axes(&self) -> Axes {
        Axes::new(&self.frame, self.sense)
    }

    /// The height of the apex, `-r / k`.
    fn apex_v(&self) -> f64 {
        -self.radius / self.slope
    }

    /// `S(u, v)`.
    #[must_use]
    pub fn point(&self, uv: Point2) -> Point3 {
        let cs = U.cos_sin(uv.x, U.piece(uv.x, Approach::Above));
        self.axes().at(cs, self.radius + self.slope * uv.y, uv.y)
    }

    /// `S(u, v)` in any ring (01 C10), with the formula of the arc that holds `near.x`.
    #[must_use]
    pub fn eval<T: Ring>(&self, u: T, v: T, near: Point2) -> Point3<T> {
        let cs = U.cos_sin(u, U.piece(near.x, Approach::Above));
        let rho = T::lift(self.radius) + T::lift(self.slope) * v;
        self.axes().at_in(cs, rho, v)
    }

    /// The point at each of `uv`, into `out`: `out[i]` is `point(uv[i])`, bit for bit (a NaN
    /// is a NaN; Rust leaves its sign and payload open).
    ///
    /// # Panics
    ///
    /// If `uv` and `out` differ in length.
    pub fn eval_many(&self, uv: &[Point2], out: &mut [Point3]) {
        check_lengths(uv, out);
        let axes = self.axes();
        for (o, p) in out.iter_mut().zip(uv) {
            let cs = U.cos_sin(p.x, U.piece(p.x, Approach::Above));
            *o = axes.at(cs, self.radius + self.slope * p.y, p.y);
        }
    }

    /// The partials up to total order `D`; at a break of `u`, `side[0]` picks the arc.
    #[must_use]
    pub fn jet<const D: usize>(&self, uv: Point2, side: [Approach; 2]) -> SurfaceJet<Vec3, D> {
        let u = U.trig::<D>(uv.x, side[0]);
        let (r, k) = (self.radius, self.slope);
        self.axes().jet(&u, |j| {
            let f = match j {
                0 => r + k * uv.y,
                1 => k,
                _ => 0.0,
            };
            (f, linear(uv.y, j))
        })
    }

    /// The unit normal along `S_u x S_v` (outward on the domain for [`Sense::Forward`]),
    /// `None` at the apex.
    #[must_use]
    pub fn normal(&self, uv: Point2) -> Option<UnitVec3> {
        let rho = self.radius + self.slope * uv.y;
        if rho == 0.0 {
            return None;
        }
        // S_u x S_v = rho phi' (s D - k z) for the frame's z and sense s.
        let cs = U.cos_sin(uv.x, U.piece(uv.x, Approach::Above));
        let axes = self.axes();
        let c = fmath::cos(self.half_angle);
        let n = (axes.radial(cs) * self.sense.sign() - *self.frame.z() * self.slope)
            * (c * rho.signum());
        Some(unit(n, self.frame.z()))
    }

    /// The apex: the iso line `v = -r / k`, collapsed to a point.
    #[must_use]
    pub fn pole(&self) -> Pole {
        let v = self.apex_v();
        let axes = self.axes();
        Pole {
            v,
            point: axes.o + axes.z * v,
        }
    }

    /// The domain: `u` periodic over `[0, 1]`, `v` from the apex along the nappe of `O`'s
    /// circle (upward for a positive half-angle).
    #[must_use]
    pub fn domain(&self) -> UvDomain {
        let u = ParamDomain::periodic(1.0, 0.0).unwrap_or(ParamDomain::Unbounded);
        let apex = self.apex_v();
        let v = if self.slope > 0.0 {
            ParamDomain::at_least(apex)
        } else {
            ParamDomain::at_most(apex)
        };
        UvDomain::new(u, v.unwrap_or(ParamDomain::Unbounded))
    }

    /// STEP's `(u, v)` at `uv`: the angle in radians (continuous, not reduced to a turn) and
    /// the height.
    #[must_use]
    pub fn to_step(&self, uv: Point2) -> Point2 {
        Point2::new(U.angle(uv.x), uv.y)
    }

    /// `uv` at STEP's `(u, v)`.
    #[must_use]
    pub fn from_step(&self, step: Point2) -> Point2 {
        Point2::new(U.parameter(step.x), step.y)
    }
}
