// Written for solidcorex. Apache-2.0.
//! The sphere `S(u, v) = O + r cos lat(v) D(u) + r sin lat(v) Z`, in the pin's uv (research
//! 63 D13).
//!
//! Layout, read from the pin (`surface/sphere.rs`, `new` and `make_nurb`): `u` is four
//! quarter arcs from `X` over `[0, 1]`; `v` is the latitude in two quarter arcs from the south
//! pole (`v = 0`) to the north pole (`v = 1`).

use scx_num::domain::{ParamDomain, UvDomain};
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Sense};
use scx_num::ring::Ring;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_nurbs::Approach;

use crate::Pole;
use crate::arcs::Arcs;
use crate::common::{Axes, unit};
use crate::error::{Size, SurfaceError, positive};
use crate::jet::SurfaceJet;

/// The `u` direction: a turn over `[0, 1]`.
const U: Arcs = Arcs::turn(1.0);
/// The `v` direction: the latitude over `[0, 1]`.
const V: Arcs = Arcs::half();

/// A sphere about the frame's origin, poles on its z axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sphere {
    frame: Frame,
    radius: f64,
    sense: Sense,
}

impl Sphere {
    /// The sphere of `radius` about `frame`'s origin, `u = 0` on its x axis and the south
    /// pole (`v = 0`) at `-r Z`.
    ///
    /// # Errors
    ///
    /// [`SurfaceError::NotFinite`], [`SurfaceError::Negative`] or
    /// [`SurfaceError::Degenerate`] (zero: a point) for the radius; all `Refused`.
    pub fn new(frame: Frame, radius: Length, sense: Sense) -> Result<Self, SurfaceError> {
        let radius = positive(Size::Radius, radius.mm())?;
        Ok(Self {
            frame,
            radius,
            sense,
        })
    }

    /// The frame.
    #[must_use]
    pub const fn frame(&self) -> &Frame {
        &self.frame
    }

    /// The radius.
    #[must_use]
    pub const fn radius(&self) -> Length {
        Length::from_mm(self.radius)
    }

    /// Whether the polar axis is the frame's z or its opposite (D14).
    #[must_use]
    pub const fn sense(&self) -> Sense {
        self.sense
    }

    fn axes(&self) -> Axes {
        Axes::new(&self.frame, self.sense)
    }

    /// `S(u, v)`.
    #[must_use]
    pub fn point(&self, uv: Point2) -> Point3 {
        let cs = U.cos_sin(uv.x, U.piece(uv.x, Approach::Above));
        let (cl, sl) = V.cos_sin(uv.y, V.piece(uv.y, Approach::Above));
        self.axes().at(cs, self.radius * cl, self.radius * sl)
    }

    /// `S(u, v)` in any ring (01 C10), with the formulas of the arcs that hold `near`.
    #[must_use]
    pub fn eval<T: Ring>(&self, u: T, v: T, near: Point2) -> Point3<T> {
        let cs = U.cos_sin(u, U.piece(near.x, Approach::Above));
        let (cl, sl) = V.cos_sin(v, V.piece(near.y, Approach::Above));
        let r = T::lift(self.radius);
        self.axes().at_in(cs, r * cl, r * sl)
    }

    /// The points at `uv`, into `out` (cleared first).
    pub fn eval_many(&self, uv: &[Point2], out: &mut Vec<Point3>) {
        out.clear();
        out.extend(uv.iter().map(|&p| self.point(p)));
    }

    /// The partials up to total order `D`; at a break, `side` picks the arc in each
    /// direction.
    #[must_use]
    pub fn jet<const D: usize>(&self, uv: Point2, side: [Approach; 2]) -> SurfaceJet<Vec3, D> {
        let u = U.trig::<D>(uv.x, side[0]);
        let lat = V.trig::<D>(uv.y, side[1]);
        let r = self.radius;
        self.axes().jet(&u, |j| {
            let (c, s) = lat.nth(j);
            (r * c, r * s)
        })
    }

    /// The unit normal, outward for [`Sense::Forward`]: along `S_u x S_v`, and at a pole,
    /// where that vanishes, its limit (the sphere is smooth there: `-Z` at `v = 0`, `+Z` at
    /// `v = 1`, times the sense).
    #[must_use]
    pub fn normal(&self, uv: Point2) -> UnitVec3 {
        // S_u x S_v = r^2 cos(lat) phi' lat' s (cos(lat) D + sin(lat) Z) for the sense s. On
        // the domain cos(lat) >= 0, and the bracket is the outward normal, poles included;
        // past the poles the arcs run back over the sphere and the cross product turns.
        let (cl, sl) = V.cos_sin(uv.y, V.piece(uv.y, Approach::Above));
        let cs = U.cos_sin(uv.x, U.piece(uv.x, Approach::Above));
        let axes = self.axes();
        let turn = if cl < 0.0 { -1.0 } else { 1.0 };
        let n = (axes.radial(cs) * cl + axes.z * sl) * (self.sense.sign() * turn);
        unit(n, self.frame.z())
    }

    /// The poles: the iso lines `v = 0` (at `O - r Z`) and `v = 1` (at `O + r Z`).
    #[must_use]
    pub fn poles(&self) -> [Pole; 2] {
        let axes = self.axes();
        [
            Pole {
                v: 0.0,
                point: axes.o + axes.z * -self.radius,
            },
            Pole {
                v: 1.0,
                point: axes.o + axes.z * self.radius,
            },
        ]
    }

    /// The domain: `u` periodic over `[0, 1]`, `v` the range `[0, 1]`.
    #[must_use]
    pub fn domain(&self) -> UvDomain {
        let u = ParamDomain::periodic(1.0, 0.0).unwrap_or(ParamDomain::Unbounded);
        let v = Extent1d::new(0.0, 1.0).map_or(ParamDomain::Unbounded, ParamDomain::Bounded);
        UvDomain::new(u, v)
    }

    /// STEP's `(u, v)` at `uv`: longitude and latitude in radians.
    #[must_use]
    pub fn to_step(&self, uv: Point2) -> Point2 {
        Point2::new(U.angle(uv.x), V.angle(uv.y))
    }

    /// `uv` at STEP's `(u, v)`.
    #[must_use]
    pub fn from_step(&self, step: Point2) -> Point2 {
        Point2::new(U.parameter(step.x), V.parameter(step.y))
    }
}
