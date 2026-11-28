// Written for solidcorex. Apache-2.0.
//! The ruled surface between two curves (research 75 MK17; the pin's `surface::ruled`): one
//! face, `S(s, t) = (1 - t) a(s) + t b(s)`, whose sides `t = 0` and `t = 1` are the two
//! curves themselves.
//!
//! Built exactly, by what the curves are (R4, R5: the choice is a predicate on the input,
//! made before anything is built):
//!
//! - **Lines and arcs in one plane** make a face on that [`Plane`](scx_surface::Plane),
//!   bounded by the two curves and the two straight rulings between their ends. Two parallel
//!   lines are a plane strip. The face is the same point set the ruled map covers (its image
//!   is the region the four sides bound), on the plane's own uv, not the ruled map's
//!   `(s, t)`. When the four sides do not bound a simple region (the rulings cross, a side
//!   crosses another) the plane's face is not the surface, and the sides go the NURBS way.
//! - **Every other pair** becomes a NURBS surface of degree 1 in `t`. Each curve takes its
//!   exact NURBS form (a line's degree 1, a circle's rational quadratic arcs, a NURBS curve
//!   restricted to its interval); the two are made compatible by knot insertion and degree
//!   elevation ([`CompatibleCurves`]), exactly, with no knot moved; the net is the
//!   curves' control points in two columns. The surface contains both curves and every
//!   straight ruling between points of the same parameter.
//!
//! **The pairing** is by the compatible forms' parameter, `b` mapped affinely onto `a`'s
//! range. For two lines or two curves of one parameter that is the fraction of each range
//! (the pin's `s`). A circle's rational form is not constant speed inside a quarter turn, so
//! its parameter and its angle agree only at the pieces' ends. Where the two sides' weights
//! differ the homogeneous blend moves each point along its ruling: the surface and its
//! rulings are as given, and `t` is not linear in distance along them.
//!
//! With [`Join::Reversed`], `b` runs the other way first (its end is joined to `a`'s start).
//!
//! The front is the side of `S_s x S_t`: the plane face's normal is chosen so, and the NURBS
//! surface's natural normal is.

mod nurbs;
mod planar;

#[cfg(test)]
mod tests;

use core::fmt;

use scx_brep::{Document, FaceId};
use scx_curve::Curve;
use scx_num::error::ErrorKind;
use scx_num::extent1d::Extent1d;
use scx_nurbs::NurbsError;
use scx_nurbs_surface::NurbsSurfaceError;

use crate::Made;
use crate::error::MakeError;

/// A curve and the interval of it that is one side of a ruled surface, run from the
/// interval's start to its end.
#[derive(Debug, Clone, PartialEq)]
pub struct RuledSide {
    /// The curve.
    pub curve: Curve,
    /// The interval of its parameter.
    pub interval: Extent1d,
}

impl RuledSide {
    /// The side along `curve` over `interval`.
    #[must_use]
    pub const fn new(curve: Curve, interval: Extent1d) -> Self {
        Self { curve, interval }
    }
}

/// How the second side runs against the first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    /// Start to start and end to end.
    Same,
    /// The second side's end is joined to the first's start.
    Reversed,
}

/// Which side: the first or the second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// The first curve, at `t = 0`.
    First,
    /// The second curve, at `t = 1`.
    Second,
}

/// Which input is wrong.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum RuledRefusal {
    /// A side's interval is not finite, or not inside a NURBS curve's domain.
    Interval(Which),
    /// A side's two ends are within the resolution of each other: a closed curve, whose
    /// ruled surface has a seam this constructor does not write (a cylinder or cone is the
    /// constructor for that).
    Closed(Which),
    /// The two sides bound no area: both lines on one line, or a side a single point.
    NoArea,
}

/// What this build does not make.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum RuledUnsupported {
    /// A side with no exact NURBS form here (an ellipse, a parabola, a hyperbola) that is
    /// not met by the planar case.
    Form(Which),
    /// The two sides meet at an end, so a ruling is a point (a collapsed edge on a NURBS
    /// face, which is not written yet). Lines and arcs in one plane do not come here.
    MeetingSides,
}

/// Why no ruled surface was made.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum RuledError {
    /// The input is wrong.
    Refused(RuledRefusal),
    /// The input is valid but this build does not make it.
    Unsupported(RuledUnsupported),
    /// The two NURBS forms could not be made compatible exactly.
    Compatible(NurbsError),
    /// The constructor's own fault: the ruled net was refused as a surface.
    Surface(NurbsSurfaceError),
    /// The planar or the finished body's constructor refused.
    Make(MakeError),
    /// The constructor's own fault.
    Defect {
        /// Where.
        why: &'static str,
    },
}

impl RuledError {
    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Refused(_) => ErrorKind::Refused,
            Self::Unsupported(_) => ErrorKind::Unsupported,
            Self::Compatible(e) => e.kind(),
            Self::Make(e) => e.kind(),
            Self::Surface(_) | Self::Defect { .. } => ErrorKind::Defect,
        }
    }

    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Refused(RuledRefusal::Interval(_)) => "ruled_interval",
            Self::Refused(RuledRefusal::Closed(_)) => "ruled_closed_side",
            Self::Refused(RuledRefusal::NoArea) => "ruled_no_area",
            Self::Unsupported(RuledUnsupported::Form(_)) => "ruled_curve_form",
            Self::Unsupported(RuledUnsupported::MeetingSides) => "ruled_meeting_sides",
            Self::Compatible(_) => "ruled_not_compatible",
            Self::Surface(_) => "ruled_surface",
            Self::Make(e) => e.name(),
            Self::Defect { .. } => "ruled_defect",
        }
    }
}

impl fmt::Display for RuledError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(r) => write!(f, "ruled: refused: {r:?}"),
            Self::Unsupported(u) => write!(f, "ruled: unsupported: {u:?}"),
            Self::Compatible(e) => write!(f, "ruled: the sides cannot be made compatible: {e}"),
            Self::Surface(e) => write!(f, "ruled: the net is not a surface: {e}"),
            Self::Make(e) => write!(f, "ruled: {e:?}"),
            Self::Defect { why } => write!(f, "ruled: internal defect: {why}"),
        }
    }
}

impl core::error::Error for RuledError {}

impl From<MakeError> for RuledError {
    fn from(e: MakeError) -> Self {
        Self::Make(e)
    }
}

/// The ruled surface between `a` and `b`, as a sheet of one face (module docs).
///
/// # Errors
///
/// - [`RuledRefusal::Interval`] for a side whose interval is not in its curve's domain,
///   [`RuledRefusal::Closed`] for a closed side, [`RuledRefusal::NoArea`] for two sides on
///   one line.
/// - [`RuledUnsupported::Form`] for a side with no exact NURBS form (and not in the planar
///   case), [`RuledUnsupported::MeetingSides`] for sides that meet at an end off a plane.
/// - [`RuledError::Compatible`] when the forms cannot be made compatible exactly, and a
///   [`MakeError`] when the body does not finish.
pub fn ruled(
    a: &RuledSide,
    b: &RuledSide,
    join: Join,
    doc: &Document,
) -> Result<Made<FaceId>, RuledError> {
    let zone = doc.resolution().zone().mm();
    for (side, which) in [(a, Which::First), (b, Which::Second)] {
        if !(side.interval.min().is_finite() && side.interval.max().is_finite()) {
            return Err(RuledError::Refused(RuledRefusal::Interval(which)));
        }
        let ends = side
            .curve
            .point(side.interval.min())
            .distance(side.curve.point(side.interval.max()));
        if ends <= zone {
            return Err(RuledError::Refused(RuledRefusal::Closed(which)));
        }
    }
    if let Some(made) = planar::ruled(a, b, join, doc)? {
        return Ok(made);
    }
    nurbs::ruled(a, b, join, doc)
}
