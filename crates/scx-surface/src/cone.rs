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
use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::ring::Ring;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_nurbs::Approach;
use scx_nurbs_surface::{NurbsSurface, ParamAxis};

use crate::Pole;
use crate::common::{Axes, check_lengths, linear, unit};
use crate::error::{Size, SurfaceError, at_least_zero};
use crate::invert::{self, Closest, Inversion, Periods};
use crate::to_nurbs::{Dir, Reach, build};
use scx_curve::arcs::Arcs;
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

    /// The same sizes on `frame` in `sense`: the edits' rebuild of a validated kind (the
    /// sizes are unchanged, so nothing is checked again).
    pub(crate) const fn placed(&self, frame: Frame, sense: Sense) -> Self {
        Self {
            frame,
            sense,
            ..*self
        }
    }

    /// The cone with its axis and half-angle negated: `v` runs the other way and the radius
    /// at `-v` is the radius at `v`, to the bit (the slope is negated, not recomputed).
    pub(crate) const fn axis_reversed(&self) -> Self {
        Self {
            half_angle: -self.half_angle,
            slope: -self.slope,
            sense: self.sense.reversed(),
            ..*self
        }
    }

    fn axes(&self) -> Axes {
        Axes::new(&self.frame, self.sense)
    }

    /// `k = tan(half-angle)`, as stored (the radius grows by `k` per unit of `v`).
    pub(crate) const fn slope(&self) -> f64 {
        self.slope
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

    /// The exact rational image over `over`, in the same uv (63 §4.2, §4.8; D4, D13): the
    /// NURBS surface at `uv` is `point(uv)` within the rounding of its control data. Degree 2 in `u` (one span per quarter arc or its part), 1 in `v`.
    ///
    /// # Errors
    ///
    /// `Refused`: [`SurfaceError::EmptyExtent`] for an extent that is a single value along a
    /// direction, and [`SurfaceError::OutsideDomain`] for one that leaves the domain (`u` longer than a turn, or `v` past the apex).
    /// [`SurfaceError::Nurbs`] only on overflow, for sizes near the largest double.
    pub fn to_nurbs(&self, over: &Extent2d) -> Result<NurbsSurface<3>, SurfaceError> {
        let u = Dir::new(ParamAxis::U, over.x_interval(), Reach::Arcs(U))?;
        let apex = self.apex_v();
        let reach = if self.slope > 0.0 {
            Reach::AtLeast(apex)
        } else {
            Reach::AtMost(apex)
        };
        let v = Dir::new(ParamAxis::V, over.y_interval(), reach)?;
        let axes = self.axes();
        build(u, v, true, |cs, (h, _)| {
            axes.at(cs, self.radius + self.slope * h, h)
        })
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

impl Cone {
    /// The nearest point to `p`, in `p`'s meridian half-plane: the foot of `p` on the
    /// generator there, `v = ((rho - r) k + h) / (k^2 + 1)`, while that lies on the nappe
    /// (`r + k v > 0`), else the apex. For `p` on the axis inside the opening the feet are the
    /// circle at that `v` ([`Closest::AlongU`]); the apex is [`Closest::AlongU`] at its `v`
    /// too, every `u` being the one point.
    ///
    /// The global nearest on the domain's one nappe: the generator of the opposite
    /// half-plane is at angle `theta + a` from `p` about the apex, against `|theta - a|` for
    /// the near one, so it is never nearer; a foot past the apex on the near generator leaves
    /// the apex as the nearest point of both.
    #[must_use]
    pub fn closest(&self, p: Point3) -> Closest {
        let (x, y, h) = self.axes().local(p);
        let at = |uv: Point2| invert::foot(p, uv, self.point(uv));
        let (r, k) = (self.radius, self.slope);
        let rho = (x * x + y * y).sqrt();
        let v = ((rho - r) * k + h) / (k * k + 1.0);
        let on_axis = x == 0.0 && y == 0.0;
        if r + k * v <= 0.0 {
            let apex = self.apex_v();
            return Closest::AlongU {
                v: apex,
                distance: at(Point2::new(0.0, apex)).distance,
            };
        }
        if on_axis {
            return Closest::AlongU {
                v,
                distance: at(Point2::new(0.0, v)).distance,
            };
        }
        Closest::One(at(Point2::new(U.of_direction(x, y), v)))
    }

    /// The nearest point to `p`, inside `domain` (`u` wrapped by its period, 1) or not
    /// (`None`: the cone's nappe). Of a circle of feet, and at the apex, the one at the
    /// domain's start in `u`.
    #[must_use]
    pub fn invert(&self, p: Point3, domain: Option<&Extent2d>) -> Inversion<Point2> {
        let periods = Periods {
            u: Some(1.0),
            v: None,
        };
        invert::invert(self.closest(p), periods, domain, |uv| {
            invert::foot(p, uv, self.point(uv))
        })
    }
}
