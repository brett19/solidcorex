// Written for solidcorex. Apache-2.0.
//! Why an analytic surface cannot be built, or a NURBS surface's inversion did not finish.
//!
//! Errors follow research 62 §10: every cause has a kind ([`SurfaceError::kind`]), a stable
//! [`SurfaceError::name`] and a one-frame Display. A kind is validated once, when it is built;
//! its evaluation is a bounded closed form and cannot fail (63 X13). The one iterating call,
//! [`Surface::invert`](crate::Surface::invert) on a NURBS surface, ends with
//! [`SurfaceError::Exhausted`] or [`SurfaceError::Stopped`] when its budget runs out or the
//! caller stops it, and [`SurfaceError::Defect`] where the search itself goes wrong.
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `Cone::new_unbounded`: `InvalidInput` (origin or radius not finite; semi-angle not finite) | [`SurfaceError::NotFinite`] |
//! | `Cone::new_unbounded`: `InvalidInput` (semi-angle 90 degrees modulo 180) | [`SurfaceError::Degenerate`] of [`Size::HalfAngle`]; an angle outside `(-pi/2, pi/2)` is refused, not reduced modulo `pi` |
//! | `Cone::new_unbounded` with semi-angle 0 builds a cylinder | [`SurfaceError::Degenerate`] of [`Size::HalfAngle`]: a cylinder is its own kind |
//! | `canonical_frame`: `InvalidInput` (axes with no frame) | none: a [`Frame`](scx_num::frame::Frame) is valid by construction |
//! | none: a zero or negative radius builds a surface | [`SurfaceError::Degenerate`] or [`SurfaceError::Negative`] |
//! | none: a torus with minor radius at least its major builds | [`SurfaceError::SelfIntersecting`] (Unsupported) |

use core::fmt;

use scx_num::budget::{Exhausted, Stopped};
use scx_num::error::ErrorKind;

/// A size of an analytic kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Size {
    /// The radius of a cylinder or sphere, or a cone's radius at `v = 0`.
    Radius,
    /// A cone's half-angle.
    HalfAngle,
    /// A torus's major radius (centre to tube centre).
    Major,
    /// A torus's minor radius (the tube's).
    Minor,
}

/// Why an analytic surface cannot be built.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum SurfaceError {
    /// The size is infinite or NaN.
    NotFinite {
        /// Which size.
        size: Size,
        /// The value given.
        value: f64,
    },
    /// The size makes no surface of the kind: a zero radius (a line, a point or a circle),
    /// a zero half-angle (a cylinder) or one of `pi/2` or more in magnitude (a plane).
    Degenerate {
        /// Which size.
        size: Size,
        /// The value given.
        value: f64,
    },
    /// The size is below zero.
    Negative {
        /// Which size.
        size: Size,
        /// The value given.
        value: f64,
    },
    /// A torus whose minor radius is not below its major one (a horn or spindle torus)
    /// passes through its own axis. Not a step-2 kind.
    SelfIntersecting {
        /// The major radius.
        major: f64,
        /// The minor radius.
        minor: f64,
    },
    /// The work budget of a NURBS surface's inversion ran out.
    Exhausted(Exhausted),
    /// The caller stopped a NURBS surface's inversion.
    Stopped,
    /// A NURBS surface's inversion went wrong inside: a bug, not the input's fault.
    Defect {
        /// What went wrong.
        reason: &'static str,
    },
}

impl SurfaceError {
    /// The error's kind: [`ErrorKind::Refused`] for sizes that make no surface,
    /// [`ErrorKind::Unsupported`] for a self-intersecting torus, and the inversion's
    /// [`ErrorKind::Exhausted`], [`ErrorKind::Cancelled`] and [`ErrorKind::Defect`].
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::NotFinite { .. } | Self::Degenerate { .. } | Self::Negative { .. } => {
                ErrorKind::Refused
            }
            Self::SelfIntersecting { .. } => ErrorKind::Unsupported,
            Self::Exhausted(_) => ErrorKind::Exhausted,
            Self::Stopped => ErrorKind::Cancelled,
            Self::Defect { .. } => ErrorKind::Defect,
        }
    }

    /// The cause's stable name (62 D6).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::NotFinite { .. } => "size_not_finite",
            Self::Degenerate { .. } => "size_degenerate",
            Self::Negative { .. } => "size_negative",
            Self::SelfIntersecting { .. } => "torus_self_intersecting",
            Self::Exhausted(e) => e.name(),
            Self::Stopped => Stopped.name(),
            Self::Defect { .. } => "inversion_defect",
        }
    }
}

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Radius => "radius",
            Self::HalfAngle => "half-angle",
            Self::Major => "major radius",
            Self::Minor => "minor radius",
        })
    }
}

impl fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::NotFinite { size, value } => write!(f, "the {size} {value} is not finite"),
            Self::Degenerate { size, value } => {
                write!(f, "the {size} {value} makes no surface of this kind")
            }
            Self::Negative { size, value } => write!(f, "the {size} {value} is negative"),
            Self::SelfIntersecting { major, minor } => write!(
                f,
                "a torus with minor radius {minor} not below its major radius {major} \
                 intersects itself"
            ),
            Self::Exhausted(e) => write!(f, "{e}"),
            Self::Stopped => write!(f, "{Stopped}"),
            Self::Defect { reason } => write!(f, "the inversion failed inside: {reason}"),
        }
    }
}

impl std::error::Error for SurfaceError {}

/// A finite, positive size.
pub(crate) fn positive(size: Size, value: f64) -> Result<f64, SurfaceError> {
    let value = at_least_zero(size, value)?;
    if value == 0.0 {
        return Err(SurfaceError::Degenerate { size, value });
    }
    Ok(value)
}

/// A finite size of zero or more.
pub(crate) fn at_least_zero(size: Size, value: f64) -> Result<f64, SurfaceError> {
    if !value.is_finite() {
        return Err(SurfaceError::NotFinite { size, value });
    }
    if value < 0.0 {
        return Err(SurfaceError::Negative { size, value });
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_names() {
        let e = positive(Size::Radius, 0.0);
        assert_eq!(e.map_err(|e| e.name()), Err("size_degenerate"));
        let e = positive(Size::Radius, -1.0);
        assert_eq!(e.map_err(|e| e.kind()), Err(ErrorKind::Refused));
        let e = positive(Size::Minor, f64::NAN).map_err(|e| e.name());
        assert_eq!(e, Err("size_not_finite"));
        let t = SurfaceError::SelfIntersecting {
            major: 1.0,
            minor: 2.0,
        };
        assert_eq!(t.kind(), ErrorKind::Unsupported);
        assert!(t.to_string().contains("intersects itself"));
        let x = SurfaceError::Exhausted(Exhausted { limit: 3 });
        assert_eq!(
            (x.kind(), x.name()),
            (ErrorKind::Exhausted, "budget_exhausted")
        );
        assert_eq!(x.to_string(), "the work budget of 3 units ran out");
        let s = SurfaceError::Stopped;
        assert_eq!((s.kind(), s.name()), (ErrorKind::Cancelled, "stopped"));
        let d = SurfaceError::Defect { reason: "r" };
        assert_eq!(
            (d.kind(), d.name()),
            (ErrorKind::Defect, "inversion_defect")
        );
        assert!(d.to_string().ends_with(": r"));
    }
}
