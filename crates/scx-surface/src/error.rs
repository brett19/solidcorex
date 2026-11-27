// Written for solidcorex. Apache-2.0.
//! Why an analytic surface cannot be built.
//!
//! Errors follow research 62 §10: every cause has a kind ([`SurfaceError::kind`]), a stable
//! [`SurfaceError::name`] and a one-frame Display. A kind is validated once, when it is built;
//! its evaluation is a bounded closed form and cannot fail (63 X13).
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

use scx_num::error::ErrorKind;
use scx_nurbs_surface::{NurbsSurfaceError, ParamAxis};

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
    /// An extent that is a single value along `axis`: no surface has an empty domain.
    EmptyExtent {
        /// The direction.
        axis: ParamAxis,
        /// The value.
        at: f64,
    },
    /// An extent that leaves the surface's domain along `axis`: past a pole of the sphere,
    /// past the cone's apex, or outside a NURBS surface's domain.
    OutsideDomain {
        /// The direction.
        axis: ParamAxis,
        /// The extent's low end.
        min: f64,
        /// The extent's high end.
        max: f64,
    },
    /// The transform's matrix has a coefficient that is infinite or NaN, or the image of
    /// the frame's origin overflows.
    MapNotFinite,
    /// The transform flattens the surface: its linear part is singular on the frame's axes.
    MapFlattens,
    /// A cylinder, cone, sphere or torus under a map that is not a similarity: the image is
    /// not a surface of the kind, and its exact image as a NURBS surface is not built yet
    /// (research 63 SU3). Refused, not snapped to the nearest similarity.
    NotSimilar,
    /// An edit of a NURBS surface failed: a context frame over the `scx-nurbs-surface`
    /// error, whose kind it has.
    Nurbs(NurbsSurfaceError),
}

impl SurfaceError {
    /// The error's kind: [`ErrorKind::Refused`] for sizes that make no surface and for an
    /// edit's extent or map, [`ErrorKind::Unsupported`] for a self-intersecting torus and a
    /// kind's image under a map that is not a similarity, and the NURBS error's own kind
    /// for [`SurfaceError::Nurbs`].
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::NotFinite { .. } | Self::Degenerate { .. } | Self::Negative { .. } => {
                ErrorKind::Refused
            }
            Self::SelfIntersecting { .. } | Self::NotSimilar => ErrorKind::Unsupported,
            Self::EmptyExtent { .. }
            | Self::OutsideDomain { .. }
            | Self::MapNotFinite
            | Self::MapFlattens => ErrorKind::Refused,
            Self::Nurbs(e) => e.kind(),
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
            Self::EmptyExtent { .. } => "extent_empty",
            Self::OutsideDomain { .. } => "extent_outside_domain",
            Self::MapNotFinite => "map_not_finite",
            Self::MapFlattens => "map_flattens",
            Self::NotSimilar => "map_not_similar",
            Self::Nurbs(_) => "nurbs_edit_failed",
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
            Self::EmptyExtent { axis, at } => {
                write!(f, "the extent is the single value {at} along {axis}")
            }
            Self::OutsideDomain { axis, min, max } => write!(
                f,
                "the extent [{min}, {max}] along {axis} leaves the surface's domain"
            ),
            Self::MapNotFinite => f.write_str("the transform or its image is not finite"),
            Self::MapFlattens => f.write_str("the transform flattens the surface"),
            Self::NotSimilar => f.write_str(
                "the transform is not a similarity, so the image is not a surface of the kind",
            ),
            Self::Nurbs(_) => f.write_str("the NURBS surface edit failed"),
        }
    }
}

impl std::error::Error for SurfaceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Nurbs(e) => Some(e),
            _ => None,
        }
    }
}

impl From<NurbsSurfaceError> for SurfaceError {
    fn from(e: NurbsSurfaceError) -> Self {
        Self::Nurbs(e)
    }
}

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
    }
}
