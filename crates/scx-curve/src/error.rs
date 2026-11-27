// Written for solidcorex. Apache-2.0.
//! Why a curve cannot be built, restricted or transformed.
//!
//! Errors follow research 62 §10: every cause has a kind ([`CurveError::kind`]), a stable
//! name ([`CurveError::name`]) and a Display of one frame. Every cause of this crate's own
//! is the caller's data, kind `Refused` (63 X13); a NURBS cause ([`CurveError::Nurbs`]) is
//! passed through with its own kind and name. Evaluation of the analytic kinds never fails:
//! a point and a jet are closed forms, total over `f64` (a NaN parameter gives NaN
//! coordinates).
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `Line::from_points`, `Line::bounded`: a segment or vector shorter than `SM_EFF_ZERO` becomes the X axis over `[0, 0]` | [`Degenerate::Direction`] |
//! | `Line::infinite`: a zero vector gives non-finite data | [`Degenerate::Direction`] |
//! | `Ellipse::create_canonical_circle`: `RadiusNotPositive` (not above `SM_EFF_ZERO`, or NaN) | [`Degenerate::Radius`], or [`CurveError::NotFinite`] for NaN |
//! | none: an infinite radius builds | [`CurveError::NotFinite`] |
//! | none: `Ellipse::new` builds any radii | [`Degenerate::SemiAxis`], [`CurveError::NotFinite`] |
//! | none: non-finite points and vectors build | [`CurveError::NotFinite`] (a [`Frame`](scx_num::frame::Frame) refuses them itself) |
//! | new (CU5): a restriction past a NURBS curve's domain | [`CurveError::OutsideDomain`] |
//! | new (CU5): a restriction to one parameter | [`Degenerate::Extent`] |
//! | new (CU5): a map that takes a conic's plane to a line or a point | [`Degenerate::Plane`], or a degenerate semi-axis |
//!
//! `TooLarge` and `Solve` (63 §3.2) join with the first operation that can produce them
//! (inversion, CU4).

use core::fmt;

use scx_num::error::ErrorKind;
use scx_num::extent1d::Extent1d;
use scx_nurbs::NurbsError;

/// Why a curve cannot be built, restricted or transformed.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum CurveError {
    /// A size or direction too small to give a curve.
    Degenerate(Degenerate),
    /// A coordinate or size is infinite or NaN.
    NotFinite(Part),
    /// A restriction reaches past a bounded curve's domain.
    OutsideDomain {
        /// The extent asked for.
        extent: Extent1d,
        /// The curve's domain.
        domain: Extent1d,
    },
    /// A NURBS curve's own cause (its split, or its build from mapped points), with its
    /// kind and name.
    Nurbs(NurbsError),
}

/// A size or direction too small to give a curve. A length's threshold is the pin's
/// [`EFF_ZERO`](scx_num::math::EFF_ZERO) (`1e-12` mm), as there.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Degenerate {
    /// A line's direction (velocity) is shorter than `EFF_ZERO`: no tangent.
    Direction {
        /// Its length.
        length: f64,
    },
    /// A circle's radius is not above `EFF_ZERO`.
    Radius {
        /// The radius.
        radius: f64,
    },
    /// An ellipse's semi-axis is not above `EFF_ZERO`.
    SemiAxis {
        /// Which.
        axis: SemiAxis,
        /// Its length.
        length: f64,
    },
    /// A restriction to the single parameter `at`: an extent of no length.
    Extent {
        /// The parameter.
        at: f64,
    },
    /// A map that takes a conic's plane to a line or a point: the image has no plane.
    Plane,
}

/// An ellipse's semi-axis: along its frame's x or y.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemiAxis {
    /// Along x (where the angle starts).
    X,
    /// Along y.
    Y,
}

/// The part of a curve's data that is not finite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Part {
    /// A line's origin, or a conic's centre.
    Origin,
    /// A line's direction.
    Direction,
    /// A circle's radius.
    Radius,
    /// An ellipse's semi-axis.
    SemiAxis(SemiAxis),
    /// The affine map a curve is transformed by.
    Map,
}

impl CurveError {
    /// The error's kind: [`ErrorKind::Refused`] (the data is the caller's), or a NURBS
    /// cause's own.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Degenerate(_) | Self::NotFinite(_) | Self::OutsideDomain { .. } => {
                ErrorKind::Refused
            }
            Self::Nurbs(e) => e.kind(),
        }
    }

    /// The cause's stable name (62 D6).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Degenerate(Degenerate::Direction { .. }) => "degenerate_direction",
            Self::Degenerate(Degenerate::Radius { .. }) => "degenerate_radius",
            Self::Degenerate(Degenerate::SemiAxis { .. }) => "degenerate_semi_axis",
            Self::Degenerate(Degenerate::Extent { .. }) => "degenerate_extent",
            Self::Degenerate(Degenerate::Plane) => "degenerate_plane",
            Self::NotFinite(_) => "not_finite",
            Self::OutsideDomain { .. } => "outside_domain",
            Self::Nurbs(e) => e.name(),
        }
    }
}

impl fmt::Display for SemiAxis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::X => "x",
            Self::Y => "y",
        })
    }
}

impl fmt::Display for Part {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Origin => f.write_str("the origin"),
            Self::Direction => f.write_str("the direction"),
            Self::Radius => f.write_str("the radius"),
            Self::SemiAxis(a) => write!(f, "the {a} semi-axis"),
            Self::Map => f.write_str("the map"),
        }
    }
}

impl fmt::Display for CurveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Degenerate(Degenerate::Direction { length }) => {
                write!(f, "a line direction of length {length} gives no tangent")
            }
            Self::Degenerate(Degenerate::Radius { radius }) => {
                write!(f, "a circle radius of {radius} is not positive")
            }
            Self::Degenerate(Degenerate::SemiAxis { axis, length }) => {
                write!(f, "an ellipse {axis} semi-axis of {length} is not positive")
            }
            Self::Degenerate(Degenerate::Extent { at }) => {
                write!(
                    f,
                    "a restriction to the single parameter {at} gives no curve"
                )
            }
            Self::Degenerate(Degenerate::Plane) => {
                f.write_str("the map takes the conic's plane to a line or a point")
            }
            Self::NotFinite(part) => write!(f, "{part} is not finite"),
            Self::OutsideDomain { extent, domain } => write!(
                f,
                "the extent [{}, {}] reaches past the domain [{}, {}]",
                extent.min(),
                extent.max(),
                domain.min(),
                domain.max()
            ),
            Self::Nurbs(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for CurveError {}

impl From<NurbsError> for CurveError {
    fn from(e: NurbsError) -> Self {
        Self::Nurbs(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_cause_is_refused_named_and_displayed() {
        let all = [
            CurveError::Degenerate(Degenerate::Direction { length: 0.0 }),
            CurveError::Degenerate(Degenerate::Radius { radius: -1.0 }),
            CurveError::Degenerate(Degenerate::SemiAxis {
                axis: SemiAxis::Y,
                length: 0.0,
            }),
            CurveError::Degenerate(Degenerate::Extent { at: 1.0 }),
            CurveError::Degenerate(Degenerate::Plane),
            CurveError::NotFinite(Part::Origin),
            CurveError::NotFinite(Part::SemiAxis(SemiAxis::X)),
            CurveError::NotFinite(Part::Map),
            CurveError::OutsideDomain {
                extent: Extent1d::new(0.0, 2.0).unwrap(),
                domain: Extent1d::new(0.0, 1.0).unwrap(),
            },
        ];
        let names: BTreeSet<_> = all.iter().map(CurveError::name).collect();
        assert_eq!(names.len(), 7);
        for e in all {
            assert_eq!(e.kind(), ErrorKind::Refused);
            assert!(!e.to_string().is_empty());
        }
        assert_eq!(
            CurveError::NotFinite(Part::SemiAxis(SemiAxis::X)).to_string(),
            "the x semi-axis is not finite"
        );
        let nurbs = NurbsError::EmptyDomain { at: 1.0 };
        let e = CurveError::from(nurbs);
        assert_eq!((e.kind(), e.name()), (nurbs.kind(), nurbs.name()));
        assert_eq!(e.to_string(), nurbs.to_string());
    }
}
