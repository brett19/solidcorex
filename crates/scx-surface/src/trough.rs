// Written for solidcorex. Apache-2.0.
//! The half of a cylinder a point on it faces a travel from, as a sheet's section (research
//! 74 §30, SU8; the pin's `geom/facing_trough`, #20389).
//!
//! A straight line along a travel across a cylinder's axis meets the cylinder, extended,
//! twice, once on each half either side of the axis. A tool run along the travel and cut at
//! the cylinder ends where it crosses the half a face of it lies on. That half, its long
//! edges carried on along the travel as flat wings tangent to it, is a smooth sheet such a
//! line crosses once at most: back the way the travel came where the line leaves the
//! cylinder on that half, on ahead where it goes in.
//!
//! [`facing_trough`] gives the sheet's section in a frame across the axis, and how far along
//! the axis and out along the wings it must reach to pass a box (a tool's): the caller
//! extrudes the section and cuts the tool at its round face.
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `facing_trough(origin, axis, radius, near, travel, box_min, box_max) -> Option<Trough>`: `None` for a bad radius, a travel along the axis, a pick on the axis, a pick on the edge, or a number not finite | `facing_trough(&Cylinder, near, travel, &Extent3d, Resolution) -> Result<Trough, `[`TroughError`]`>`: the radius, box and axis are the cylinder's and the box's own (finite and valid by construction); the three refusals are named |
//! | "on the edge" is `abs(cos) < 1e-9` between the pick's and the travel's directions across the axis, and a pick on the axis is a separate `None` | one test in length: the pick's distance from the plane through the axis square to the travel is within the resolution ([`TroughError::OnEdge`], which a pick on the axis is too) |
//! | the reach is the farthest box corner, plus the radius, plus 1 mm | the same, times `1 + `[`MARGIN`]: a 1 mm margin is nothing on a metre part and a lot on a 0.01 mm one |
//! | a [`Trough`]'s fields: the section's points as bare pairs, `sweep` as `-PI` or `PI` | a [`Trough`] holds its [`Frame`], radius, [`Half`] and reach, and gives the section's points and the arc's sweep as methods of them |

use core::fmt;

use scx_num::error::ErrorKind;
use scx_num::extent3d::Extent3d;
use scx_num::frame::Frame;
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};

use crate::Cylinder;

/// The relative margin the trough's reach has beyond the farthest corner of the box and the
/// radius: the extruded sheet and its wings end this much past the box, so no face of the
/// tool it cuts is coincident with one of the sheet's.
pub const MARGIN: f64 = 1.0 / 16.0;

/// Which half of the cylinder the trough follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Half {
    /// The half facing the travel's direction (the frame's `+x`): the line leaves the
    /// cylinder there, and the wings run back along `-x`.
    Facing,
    /// The other half: the line goes in there, and the wings run on ahead along `+x`.
    Away,
}

/// A trough's section and reach, in a frame across the axis: the frame's origin is the
/// cylinder's axis point, `x` the travel across the axis (unit), `y = z x x` square to it
/// across the axis, `z` the cylinder's axis. The section is one open chain in that frame: the
/// line from [`Trough::wing_top`] to [`Trough::top`], the arc about the origin of the radius
/// from angle [`Trough::START`] sweeping [`Trough::sweep`] to [`Trough::bottom`], and the
/// line from `bottom` to [`Trough::wing_bottom`]. Extruded along the axis from `-reach` to
/// `reach`, it passes the box it was made for every way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trough {
    frame: Frame,
    radius: f64,
    half: Half,
    reach: f64,
}

impl Trough {
    /// The angle along the frame's arc where it starts, at [`Trough::top`].
    pub const START: f64 = core::f64::consts::FRAC_PI_2;

    /// The frame: origin on the axis, `x` the travel across it, `z` the cylinder's axis.
    #[must_use]
    pub const fn frame(&self) -> &Frame {
        &self.frame
    }

    /// The cylinder's radius.
    #[must_use]
    pub const fn radius(&self) -> Length {
        Length::from_mm(self.radius)
    }

    /// The half followed.
    #[must_use]
    pub const fn half(&self) -> Half {
        self.half
    }

    /// How far the extrusion runs either way along the axis from the frame's origin, and how
    /// long the wings are.
    #[must_use]
    pub const fn reach(&self) -> Length {
        Length::from_mm(self.reach)
    }

    /// The arc's sweep from [`Trough::START`]: `-PI` on the half facing `x`, `PI` on the
    /// other.
    #[must_use]
    pub const fn sweep(&self) -> f64 {
        match self.half {
            Half::Facing => -core::f64::consts::PI,
            Half::Away => core::f64::consts::PI,
        }
    }

    /// Where the upper wing meets the arc, `(0, radius)`.
    #[must_use]
    pub const fn top(&self) -> Point2 {
        Point2::new(0.0, self.radius)
    }

    /// Where the arc ends and the lower wing starts, `(0, -radius)`.
    #[must_use]
    pub const fn bottom(&self) -> Point2 {
        Point2::new(0.0, -self.radius)
    }

    /// The far end of the upper wing, in the frame.
    #[must_use]
    pub const fn wing_top(&self) -> Point2 {
        Point2::new(self.back(), self.radius)
    }

    /// The far end of the lower wing, in the frame.
    #[must_use]
    pub const fn wing_bottom(&self) -> Point2 {
        Point2::new(self.back(), -self.radius)
    }

    const fn back(&self) -> f64 {
        match self.half {
            Half::Facing => -self.reach,
            Half::Away => self.reach,
        }
    }
}

/// Why a cylinder has no one half a pick faces a travel from.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum TroughError {
    /// The travel or the pick is not finite.
    NotFinite,
    /// The travel runs along the axis: across it, it has no direction.
    AlongAxis,
    /// The pick lies within the resolution of the plane through the axis square to the
    /// travel: on the cylinder's edge as seen along the travel, or on its axis. Neither half
    /// is the one it faces.
    OnEdge {
        /// How far the pick is from that plane, signed along the travel.
        offset: f64,
    },
}

impl TroughError {
    /// The cause's kind (research 62 §10): the travel and a non-finite number are
    /// `Refused`, a pick between the halves is `Unsupported` (valid, and no one answer).
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::NotFinite | Self::AlongAxis => ErrorKind::Refused,
            Self::OnEdge { .. } => ErrorKind::Unsupported,
        }
    }

    /// A stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::NotFinite => "trough_not_finite",
            Self::AlongAxis => "trough_travel_along_axis",
            Self::OnEdge { .. } => "trough_pick_on_edge",
        }
    }
}

impl fmt::Display for TroughError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFinite => f.write_str("the travel or the pick is not finite"),
            Self::AlongAxis => f.write_str("the travel runs along the axis"),
            Self::OnEdge { offset } => write!(
                f,
                "the pick is {offset} mm from the plane through the axis square to the \
                 travel, so no one half is the one it faces"
            ),
        }
    }
}

impl std::error::Error for TroughError {}

/// The trough of `cylinder`, on the half the point `near` (on the cylinder) faces the travel
/// `travel` (any length) from, reaching past the box `over`.
///
/// # Errors
///
/// [`TroughError::NotFinite`]; [`TroughError::AlongAxis`] when the travel has no direction
/// across the axis; [`TroughError::OnEdge`] when `near` is within `res` of the plane through
/// the axis square to the travel (which includes a pick on the axis).
pub fn facing_trough(
    cylinder: &Cylinder,
    near: Point3,
    travel: Vec3,
    over: &Extent3d,
    res: Resolution,
) -> Result<Trough, TroughError> {
    if !(near.is_finite() && travel.is_finite()) {
        return Err(TroughError::NotFinite);
    }
    let origin = cylinder.frame().origin();
    let axis = *cylinder.frame().z() * cylinder.sense().sign();
    let radius = cylinder.radius().mm();
    let x = (travel - axis * travel.dot(axis))
        .normalized()
        .ok_or(TroughError::AlongAxis)?;
    // The frame: x the travel across the axis, z the axis, so y = z x x. `from_axes` keeps
    // the two exactly (x is square to the axis to rounding).
    let frame = Frame::new(origin, axis, *x).ok_or(TroughError::AlongAxis)?;
    // How far the pick stands ahead of the plane through the axis square to the travel:
    // positive on the half the line leaves.
    let offset = (near - origin).dot(*x);
    if !offset.is_finite() {
        return Err(TroughError::NotFinite);
    }
    if offset.abs() <= res.zone().mm() {
        return Err(TroughError::OnEdge { offset });
    }
    // Far enough from the axis's origin to pass every corner of the box, and the arc.
    let far = (0..8_usize)
        .map(|i| {
            let pick = |bit: usize, lo: f64, hi: f64| if i & bit == 0 { lo } else { hi };
            let corner = Point3::new(
                pick(1, over.x_min(), over.x_max()),
                pick(2, over.y_min(), over.y_max()),
                pick(4, over.z_min(), over.z_max()),
            );
            (corner - origin).length()
        })
        .fold(0.0_f64, f64::max);
    let reach = (far + radius) * (1.0 + MARGIN);
    if !reach.is_finite() {
        return Err(TroughError::NotFinite);
    }
    Ok(Trough {
        frame,
        radius,
        half: if offset > 0.0 {
            Half::Facing
        } else {
            Half::Away
        },
        reach,
    })
}
