// Written for solidcorex. Apache-2.0.
//! The cylinder `S(u, v) = O + r D(u) + v Z`, in the pin's uv (research 63 D13).
//!
//! Layout, read from the pin (`surface/cone/p1.rs`, `new_cylinder_unbounded`;
//! `nurbs/b_spline_surface/p2.rs`, `create_surf_of_revolution_with_line_vector`): `u` is the
//! sweep circle reparameterised by arc length, four quarter arcs from `X` over
//! `[0, 2 pi r]` with breaks at `k pi r / 2`; `v` is the height along `Z`.

use core::f64::consts::PI;

use scx_num::domain::{ParamDomain, UvDomain};
use scx_num::frame::{Frame, Sense};
use scx_num::ring::Ring;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_nurbs::Approach;

use crate::arcs::Arcs;
use crate::common::{Axes, check_lengths, linear, unit};
use crate::error::{Size, SurfaceError, positive};
use scx_num::jet::SurfaceJet;

/// A circular cylinder about the frame's z axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cylinder {
    frame: Frame,
    radius: f64,
    sense: Sense,
}

impl Cylinder {
    /// The cylinder of `radius` about `frame`'s z axis, `u = 0` on its x axis.
    ///
    /// # Errors
    ///
    /// [`SurfaceError::NotFinite`], [`SurfaceError::Negative`] or
    /// [`SurfaceError::Degenerate`] (zero: a line) for the radius; all `Refused`.
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

    /// Whether the axis is the frame's z or its opposite (D14).
    #[must_use]
    pub const fn sense(&self) -> Sense {
        self.sense
    }

    /// The period of `u`, `2 pi r`.
    fn period(&self) -> f64 {
        2.0 * PI * self.radius
    }

    fn arcs(&self) -> Arcs {
        Arcs::turn(self.period())
    }

    fn axes(&self) -> Axes {
        Axes::new(&self.frame, self.sense)
    }

    /// `S(u, v)`.
    #[must_use]
    pub fn point(&self, uv: Point2) -> Point3 {
        let a = self.arcs();
        let cs = a.cos_sin(uv.x, a.piece(uv.x, Approach::Above));
        self.axes().at(cs, self.radius, uv.y)
    }

    /// `S(u, v)` in any ring (01 C10), with the formula of the arc that holds `near.x`: exact
    /// on that arc, and its analytic continuation off it.
    #[must_use]
    pub fn eval<T: Ring>(&self, u: T, v: T, near: Point2) -> Point3<T> {
        let a = self.arcs();
        let cs = a.cos_sin(u, a.piece(near.x, Approach::Above));
        self.axes().at_in(cs, T::lift(self.radius), v)
    }

    /// The point at each of `uv`, into `out`: `out[i]` is `point(uv[i])`, bit for bit (a NaN
    /// is a NaN; Rust leaves its sign and payload open).
    ///
    /// # Panics
    ///
    /// If `uv` and `out` differ in length.
    pub fn eval_many(&self, uv: &[Point2], out: &mut [Point3]) {
        check_lengths(uv, out);
        let (a, axes) = (self.arcs(), self.axes());
        for (o, p) in out.iter_mut().zip(uv) {
            let cs = a.cos_sin(p.x, a.piece(p.x, Approach::Above));
            *o = axes.at(cs, self.radius, p.y);
        }
    }

    /// The partials up to total order `D`; at a break of `u`, `side[0]` picks the arc.
    #[must_use]
    pub fn jet<const D: usize>(&self, uv: Point2, side: [Approach; 2]) -> SurfaceJet<Vec3, D> {
        let u = self.arcs().trig::<D>(uv.x, side[0]);
        let r = self.radius;
        self.axes()
            .jet(&u, |j| (if j == 0 { r } else { 0.0 }, linear(uv.y, j)))
    }

    /// The unit normal, along `S_u x S_v`: outward for [`Sense::Forward`].
    #[must_use]
    pub fn normal(&self, uv: Point2) -> UnitVec3 {
        let a = self.arcs();
        let cs = a.cos_sin(uv.x, a.piece(uv.x, Approach::Above));
        let n = self.axes().radial(cs) * self.sense.sign();
        unit(n, self.frame.x())
    }

    /// The domain: `u` periodic over `[0, 2 pi r]`, `v` without ends.
    #[must_use]
    pub fn domain(&self) -> UvDomain {
        let u = ParamDomain::periodic(self.period(), 0.0).unwrap_or(ParamDomain::Unbounded);
        UvDomain::new(u, ParamDomain::Unbounded)
    }

    /// STEP's `(u, v)` at `uv`: the angle in radians (continuous, not reduced to a turn) and
    /// the height.
    #[must_use]
    pub fn to_step(&self, uv: Point2) -> Point2 {
        Point2::new(self.arcs().angle(uv.x), uv.y)
    }

    /// `uv` at STEP's `(u, v)`.
    #[must_use]
    pub fn from_step(&self, step: Point2) -> Point2 {
        Point2::new(self.arcs().parameter(step.x), step.y)
    }
}
