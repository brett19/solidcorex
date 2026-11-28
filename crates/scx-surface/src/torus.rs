// Written for solidcorex. Apache-2.0.
//! The torus `S(u, v) = O + (R + r cos w(v)) D(u) + r sin w(v) Z`, in the pin's uv (research
//! 63 D13).
//!
//! Layout, read from the pin (`surface/torus.rs`, `new` and `make_nurb`): `u` and `v` are
//! each four quarter arcs over `[0, 1]`, `u` from `X` and `v` from the tube's outer equator.

use scx_num::domain::{ParamDomain, UvDomain};
use scx_num::extent2d::Extent2d;
use scx_num::frame::{Frame, Sense};
use scx_num::ring::Ring;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_nurbs::Approach;
use scx_nurbs_surface::{NurbsSurface, ParamAxis};

use crate::Pole;
use crate::common::{Axes, check_lengths, unit};
use crate::error::{Size, SurfaceError, positive};
use crate::invert::{self, Closest, Inversion, Periods};
use crate::to_nurbs::{Dir, Reach, build};
use scx_curve::arcs::Arcs;
use scx_num::jet::SurfaceJet;

/// Both directions: a turn over `[0, 1]`.
pub(crate) const TURN: Arcs = Arcs::turn(1.0);

/// A torus about the frame's z axis: a ring torus, whose tube does not reach the axis, or a
/// horn torus ([`Torus::horn`]), whose tube meets the axis at one point, a pole.
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

    /// The horn torus whose tube of `radius` circles `frame`'s z axis at `radius`, so that
    /// it meets the axis at the frame's origin: the inner equator (`v = 1/2`) collapses to
    /// that point, the torus's [`pole`](Self::pole). `u = 0` is on the frame's x axis and
    /// `v = 0` on the outer equator, as [`Torus::new`]'s. Its other formulas are the ring
    /// torus's at equal radii.
    ///
    /// # Errors
    ///
    /// [`SurfaceError::NotFinite`], [`SurfaceError::Negative`] or
    /// [`SurfaceError::Degenerate`] (zero) for the radius, `Refused`.
    pub fn horn(frame: Frame, radius: Length, sense: Sense) -> Result<Self, SurfaceError> {
        let radius = positive(Size::Minor, radius.mm())?;
        Ok(Self {
            frame,
            major: radius,
            minor: radius,
            sense,
        })
    }

    /// The pole of a horn torus: its inner equator (`v = 1/2`), collapsed to the frame's
    /// origin. `None` for a ring torus, which has none.
    #[must_use]
    pub fn pole(&self) -> Option<Pole> {
        (self.minor == self.major).then(|| Pole {
            v: 0.5,
            point: self.frame.origin(),
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

    /// The same sizes on `frame` in `sense`: the edits' rebuild of a validated kind (the
    /// sizes are unchanged, so nothing is checked again).
    pub(crate) const fn placed(&self, frame: Frame, sense: Sense) -> Self {
        Self {
            frame,
            sense,
            ..*self
        }
    }

    pub(crate) fn axes(&self) -> Axes {
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
    /// one everywhere; a horn torus's at its pole is the limit along the meridian at `u`,
    /// `-D(u)` for [`Sense::Forward`].
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

    /// The exact rational image over `over`, in the same uv (63 §4.2, §4.8; D4, D13): the
    /// NURBS surface at `uv` is `point(uv)` within the rounding of its control data. Degree 2 in both directions, one span per quarter arc or its part.
    ///
    /// # Errors
    ///
    /// `Refused`: [`SurfaceError::EmptyExtent`] for an extent that is a single value along a
    /// direction, and [`SurfaceError::OutsideDomain`] for one that leaves the domain (either direction longer than a turn).
    /// [`SurfaceError::Nurbs`] only on overflow, for sizes near the largest double.
    pub fn to_nurbs(&self, over: &Extent2d) -> Result<NurbsSurface<3>, SurfaceError> {
        let u = Dir::new(ParamAxis::U, over.x_interval(), Reach::Arcs(TURN))?;
        let v = Dir::new(ParamAxis::V, over.y_interval(), Reach::Arcs(TURN))?;
        let axes = self.axes();
        build(u, v, true, |cs, (cw, sw)| {
            axes.at(cs, self.major + self.minor * cw, self.minor * sw)
        })
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

impl Torus {
    /// The nearest point to `p`, from the tube's centre circle: the point of the near
    /// meridian circle (centre `R` along `p`'s radial direction) along the direction from
    /// that centre to `p`. On the axis every centre is as near, so the feet are the circle
    /// of `u` at that `v` ([`Closest::AlongU`]); on the centre circle itself they are the
    /// meridian circle there ([`Closest::AlongV`]).
    ///
    /// The global nearest of a ring torus: the distance to the surface is `|d - r|` for `d`
    /// the distance to the centre circle, met where the nearest centre's meridian circle
    /// crosses the line to `p`; the far circle's centre is farther (`(rho + R)^2 + h^2`
    /// against `(rho - R)^2 + h^2`), and so are the other two stationary points.
    #[must_use]
    pub fn closest(&self, p: Point3) -> Closest {
        let (x, y, z) = self.axes().local(p);
        let at = |uv: Point2| invert::foot(p, uv, self.point(uv));
        if x == 0.0 && y == 0.0 {
            let v = TURN.of_direction(-self.major, z);
            return Closest::AlongU {
                v,
                distance: at(Point2::new(0.0, v)).distance,
            };
        }
        let rho = (x * x + y * y).sqrt();
        let u = TURN.of_direction(x, y);
        let off = rho - self.major;
        if off == 0.0 && z == 0.0 {
            return Closest::AlongV {
                u,
                distance: at(Point2::new(u, 0.0)).distance,
            };
        }
        Closest::One(at(Point2::new(u, TURN.of_direction(off, z))))
    }

    /// The nearest point to `p`, inside `domain` (both parameters wrapped by their period
    /// of 1) or not (`None`: the whole torus). Of a circle of feet, the one at the domain's
    /// start in the free parameter.
    #[must_use]
    pub fn invert(&self, p: Point3, domain: Option<&Extent2d>) -> Inversion<Point2> {
        let periods = Periods {
            u: Some(1.0),
            v: Some(1.0),
        };
        invert::invert(self.closest(p), periods, domain, |uv| {
            invert::foot(p, uv, self.point(uv))
        })
    }
}
