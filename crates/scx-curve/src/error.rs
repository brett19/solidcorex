// Written for solidcorex. Apache-2.0.
//! Why an analytic curve cannot be built.
//!
//! Errors follow research 62 §10: every cause has a kind ([`CurveError::kind`]), a stable
//! name ([`CurveError::name`]) and a Display of one frame. Every cause here is the caller's
//! data, kind `Refused` (63 X13). Evaluation of these kinds never fails: a point and a jet
//! are closed forms, total over `f64` (a NaN parameter gives NaN coordinates).
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `Line::from_points`, `Line::bounded`: a segment or vector shorter than `SM_EFF_ZERO` becomes the X axis over `[0, 0]` | [`Degenerate::Direction`] |
//! | `Line::infinite`: a zero vector gives non-finite data | [`Degenerate::Direction`] |
//! | `Ellipse::create_canonical_circle`: `RadiusNotPositive` (not above `SM_EFF_ZERO`, or NaN) | [`Degenerate::Radius`], or [`CurveError::NotFinite`] for NaN |
//! | none: an infinite radius builds | [`CurveError::NotFinite`] |
//! | none: `Ellipse::new` builds any radii | [`Degenerate::SemiAxis`], [`CurveError::NotFinite`] |
//! | none: non-finite points and vectors build | [`CurveError::NotFinite`] (a [`Frame`](scx_num::frame::Frame) refuses them itself) |
//!
//! `OutsideDomain`, `TooLarge` and `Solve` (63 §3.2) join with the first operation that can
//! produce them (inversion, CU4); nothing here can.

use core::fmt;

use scx_num::error::ErrorKind;

/// Why an analytic curve cannot be built.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum CurveError {
    /// A size or direction too small to give a curve.
    Degenerate(Degenerate),
    /// A coordinate or size is infinite or NaN.
    NotFinite(Part),
}

/// A size or direction too small to give a curve. The threshold is the pin's
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
    /// A line's origin.
    Origin,
    /// A line's direction.
    Direction,
    /// A circle's radius.
    Radius,
    /// An ellipse's semi-axis.
    SemiAxis(SemiAxis),
}

impl CurveError {
    /// The error's kind: always [`ErrorKind::Refused`] (the data is the caller's).
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Degenerate(_) | Self::NotFinite(_) => ErrorKind::Refused,
        }
    }

    /// The cause's stable name (62 D6).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Degenerate(Degenerate::Direction { .. }) => "degenerate_direction",
            Self::Degenerate(Degenerate::Radius { .. }) => "degenerate_radius",
            Self::Degenerate(Degenerate::SemiAxis { .. }) => "degenerate_semi_axis",
            Self::NotFinite(_) => "not_finite",
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
            Self::NotFinite(part) => write!(f, "{part} is not finite"),
        }
    }
}

impl std::error::Error for CurveError {}

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
            CurveError::NotFinite(Part::Origin),
            CurveError::NotFinite(Part::SemiAxis(SemiAxis::X)),
        ];
        let names: BTreeSet<_> = all.iter().map(CurveError::name).collect();
        assert_eq!(names.len(), 4);
        for e in all {
            assert_eq!(e.kind(), ErrorKind::Refused);
            assert!(!e.to_string().is_empty());
        }
        assert_eq!(
            CurveError::NotFinite(Part::SemiAxis(SemiAxis::X)).to_string(),
            "the x semi-axis is not finite"
        );
    }
}
