// Written for solidcorex. Apache-2.0.
//! The errors of making a body (research 62 shape; 72 S3): each has a `kind()` and a stable
//! `name()`, and its causes are typed.

use core::fmt;

use scx_brep::{Absent, FinishCause, PcurveWriteError, TableFull};
use scx_curve::CurveError;
use scx_num::error::ErrorKind;
use scx_num::units::Length;

/// Which size of a constructor is meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Dimension {
    /// The extent along the frame's x axis.
    X,
    /// The extent along the frame's y axis.
    Y,
    /// The extent along the frame's z axis.
    Z,
}

impl Dimension {
    /// The stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
        }
    }
}

/// Why the input is refused: the caller can change it.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Refusal {
    /// A size is not finite, or is not above `floor`, the document's resolution. NaN is
    /// refused too.
    Size {
        /// Which size.
        dimension: Dimension,
        /// What was given (mm).
        value: f64,
        /// What it must exceed.
        floor: Length,
    },
}

/// A fault of the constructor itself, detected: our bug.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Defect {
    /// A curve of the body could not be made.
    Curve(CurveError),
    /// The draft holds no entity the constructor just made.
    Absent(Absent),
    /// An edge's parameter interval is empty.
    Interval,
    /// A face's plane has no frame: its normal and an edge direction do not span one.
    PlaneFrame,
    /// The writer gave a face no exact pcurves.
    Pcurve(PcurveWriteError),
    /// `Draft::finish` refused the draft the constructor built.
    Finish(FinishCause),
}

/// Why no body was made.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum MakeError {
    /// The input is wrong.
    Refused(Refusal),
    /// The constructor's own fault.
    Defect(Defect),
    /// A table ran out of ids.
    Full(TableFull),
}

impl MakeError {
    /// The kind: [`ErrorKind::Refused`] for a refusal, [`ErrorKind::Defect`] for a defect,
    /// [`ErrorKind::Exhausted`] for a full table.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Refused(_) => ErrorKind::Refused,
            Self::Defect(_) => ErrorKind::Defect,
            Self::Full(_) => ErrorKind::Exhausted,
        }
    }

    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Refused(Refusal::Size { .. }) => "size",
            Self::Defect(Defect::Curve(_)) => "curve",
            Self::Defect(Defect::Absent(_)) => "absent",
            Self::Defect(Defect::Interval) => "interval",
            Self::Defect(Defect::PlaneFrame) => "plane_frame",
            Self::Defect(Defect::Pcurve(_)) => "pcurve",
            Self::Defect(Defect::Finish(_)) => "finish",
            Self::Full(_) => "table_full",
        }
    }
}

impl fmt::Display for MakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(Refusal::Size {
                dimension,
                value,
                floor,
            }) => write!(
                f,
                "the {} size {value} mm is not finite and above the resolution {} mm",
                dimension.name(),
                floor.mm()
            ),
            Self::Defect(Defect::Curve(e)) => write!(f, "a curve of the body: {e}"),
            Self::Defect(Defect::Absent(e)) => e.fmt(f),
            Self::Defect(Defect::Interval) => write!(f, "an edge's interval is empty"),
            Self::Defect(Defect::PlaneFrame) => write!(f, "a face's plane has no frame"),
            Self::Defect(Defect::Pcurve(e)) => write!(f, "no exact pcurves: {e}"),
            Self::Defect(Defect::Finish(c)) => write!(f, "the draft is not a body: {c}"),
            Self::Full(e) => e.fmt(f),
        }
    }
}

impl core::error::Error for MakeError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Defect(Defect::Curve(e)) => Some(e),
            Self::Defect(Defect::Absent(e)) => Some(e),
            Self::Defect(Defect::Pcurve(e)) => Some(e),
            Self::Defect(Defect::Finish(c)) => Some(c),
            Self::Full(e) => Some(e),
            Self::Refused(_) | Self::Defect(Defect::PlaneFrame | Defect::Interval) => None,
        }
    }
}

impl From<TableFull> for MakeError {
    fn from(e: TableFull) -> Self {
        Self::Full(e)
    }
}

impl From<PcurveWriteError> for MakeError {
    fn from(e: PcurveWriteError) -> Self {
        Self::Defect(Defect::Pcurve(e))
    }
}

impl From<CurveError> for MakeError {
    fn from(e: CurveError) -> Self {
        Self::Defect(Defect::Curve(e))
    }
}
