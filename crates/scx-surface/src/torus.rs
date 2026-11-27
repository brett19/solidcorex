// Written for solidcorex. Apache-2.0.
//! The torus `S(u, v) = O + (R + r cos w(v)) D(u) + r sin w(v) Z`, in the pin's uv (research
//! 63 D13).
//!
//! Layout, read from the pin (`surface/torus.rs`, `new` and `make_nurb`): `u` and `v` are
//! each four quarter arcs over `[0, 1]`, `u` from `X` and `v` from the tube's outer equator.

use scx_num::domain::{ParamDomain, UvDomain};
use scx_num::frame::{Frame, Sense};
use scx_num::ring::Ring;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_nurbs::Approach;

use crate::arcs::Arcs;
use crate::common::{Axes, check_lengths, unit};
use crate::error::{Size, SurfaceError, positive};
use scx_num::jet::SurfaceJet;

/// Both directions: a turn over `[0, 1]`.
const TURN: Arcs = Arcs::turn(1.0);

/// A ring torus about the frame's z axis: the tube does not reach the axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Torus {
    frame: Frame,
    major: f64,
    minor: f64,
    sense: Sense,
}

impl Torus {
    /// The torus whose tube of radius `minor` circles `frame`'s z axis at `major`, `u = 0` on
    /// its x axis and `v = 0` on the outer equator.
    ///
    /// # Errors
    ///
    /// - [`SurfaceError::NotFinite`], [`SurfaceError::Negative`] or
    ///   [`SurfaceError::Degenerate`] (zero) for either radius, `Refused`;
    /// - [`SurfaceError::SelfIntersecting`] when `minor >= major`, `Unsupported`.
    pub fn new(
        frame: Frame,
        major: Length,
        minor: Length,
        sense: Sense,
    ) -> Result<Self, SurfaceError> {
        let major = positive(Size::Major, major.mm())?;
        let minor = positive(Size::Minor, minor.mm())?;
        if minor >= major {
            return Err(SurfaceError::SelfIntersecting { major, minor });
        }
        Ok(Self {
            frame,
            major,
            minor,
            sense,
        })
    }

    /// The frame.
    #[must_use]
    pub const fn frame(&self) -> &Frame {
        &self.frame
    }

    /// The major radius.
    #[must_use]
    pub const fn major(&self) -> Length {
        Length::from_mm(self.major)
    }

    /// The minor radius.
    #[must_use]
    pub const fn minor(&self) -> Length {
        Length::from_mm(self.minor)
    }

    /// Whether the axis is the frame's z or its opposite (D14).
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
        let cs = TURN.cos_sin(uv.x, TURN.piece(uv.x, Approach::Above));
        let (cw, sw) = TURN.cos_sin(uv.y, TURN.piece(uv.y, Approach::Above));
        self.axes()
            .at(cs, self.major + self.minor * cw, self.minor * sw)
    }

    /// `S(u, v)` in any ring (01 C10), with the formulas of the arcs that hold `near`.
    #[must_use]
    pub fn eval<T: Ring>(&self, u: T, v: T, near: Point2) -> Point3<T> {
        let cs = TURN.cos_sin(u, TURN.piece(near.x, Approach::Above));
        let (cw, sw) = TURN.cos_sin(v, TURN.piece(near.y, Approach::Above));
        let r = T::lift(self.minor);
        self.axes().at_in(cs, T::lift(self.major) + r * cw, r * sw)
    }

    /// The point at each of `uv`, into `out`: `out[i]` is `point(uv[i])`, bit for bit (a NaN
    /// is a NaN; Rust leaves its sign and payload open).
    ///
    /// # Panics
    ///
    /// If `uv` and `out` differ in length.
    pub fn eval_many(&self, uv: &[Point2], out: &mut [Point3]) {
        check_lengths(uv, out);
        for (o, &p) in out.iter_mut().zip(uv) {
            *o = self.point(p);
        }
    }

    /// The partials up to total order `D`; at a break, `side` picks the arc in each
    /// direction.
    #[must_use]
    pub fn jet<const D: usize>(&self, uv: Point2, side: [Approach; 2]) -> SurfaceJet<Vec3, D> {
        let u = TURN.trig::<D>(uv.x, side[0]);
        let w = TURN.trig::<D>(uv.y, side[1]);
        let (big, r) = (self.major, self.minor);
        self.axes().jet(&u, |j| {
            let (c, s) = w.nth(j);
            (if j == 0 { big + r * c } else { r * c }, r * s)
        })
    }

    /// The unit normal along `S_u x S_v`, outward for [`Sense::Forward`]. A ring torus has
    /// one everywhere.
    #[must_use]
    pub fn normal(&self, uv: Point2) -> UnitVec3 {
        let cs = TURN.cos_sin(uv.x, TURN.piece(uv.x, Approach::Above));
        let (cw, sw) = TURN.cos_sin(uv.y, TURN.piece(uv.y, Approach::Above));
        let axes = self.axes();
        let n = (axes.radial(cs) * cw + axes.z * sw) * self.sense.sign();
        unit(n, self.frame.z())
    }

    /// The domain: both directions periodic over `[0, 1]`.
    #[must_use]
    pub fn domain(&self) -> UvDomain {
        let turn = ParamDomain::periodic(1.0, 0.0).unwrap_or(ParamDomain::Unbounded);
        UvDomain::new(turn, turn)
    }

    /// STEP's `(u, v)` at `uv`: both angles in radians.
    #[must_use]
    pub fn to_step(&self, uv: Point2) -> Point2 {
        Point2::new(TURN.angle(uv.x), TURN.angle(uv.y))
    }

    /// `uv` at STEP's `(u, v)`.
    #[must_use]
    pub fn from_step(&self, step: Point2) -> Point2 {
        Point2::new(TURN.parameter(step.x), TURN.parameter(step.y))
    }
}
