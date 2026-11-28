// Written for solidcorex. Apache-2.0.
//! The errors of making a body (research 62 shape; 72 S3): each has a `kind()` and a stable
//! `name()`, and its causes are typed.

use core::fmt;

use scx_brep::{Absent, FinishCause, PcurveWriteError, TableFull};
use scx_curve::CurveError;
use scx_num::error::ErrorKind;
use scx_num::units::{Length, Resolution};
use scx_planar::{PlanarError, SegmentRef};
use scx_surface::SurfaceError;

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
    /// A cylinder's or sphere's radius.
    Radius,
    /// A cylinder's or cone's height, along the frame's z axis.
    Height,
    /// A cone's radius on the frame's `z = 0`.
    BaseRadius,
    /// A cone's radius on `z = height`.
    TopRadius,
    /// A torus's major radius: the frame's origin to the tube's centre.
    Major,
    /// A torus's minor radius: the tube's.
    Minor,
}

impl Dimension {
    /// The stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
            Self::Radius => "radius",
            Self::Height => "height",
            Self::BaseRadius => "base_radius",
            Self::TopRadius => "top_radius",
            Self::Major => "major",
            Self::Minor => "minor",
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
    /// The profile is not a region: the planar layer's verdict, as it gave it.
    Profile(PlanarError),
    /// An extrusion's heights are not finite, or are not more than `floor`, the document's
    /// resolution, apart (mm).
    Span {
        /// The start height.
        start: f64,
        /// The end height.
        end: f64,
        /// What their distance must exceed.
        floor: Length,
    },
    /// The region was validated at a resolution other than the document's: its segments are
    /// proved apart, and its joints closed, at the wrong length.
    Resolution {
        /// The region's.
        region: Resolution,
        /// The document's.
        document: Resolution,
    },
    /// A cone whose radii are both zero: a segment, not a solid.
    ZeroRadii,
    /// A prism or sheet of no regions.
    NoRegions,
    /// A spindle torus: the minor radius exceeds the major one by more than the resolution,
    /// so the tube crosses its own axis.
    SelfIntersecting {
        /// The major radius (mm).
        major: f64,
        /// The minor radius (mm).
        minor: f64,
    },
}

/// Valid input this build does not make yet, by name.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Unsupported {
    /// The planar layer could not decide the profile (its `Undecided`).
    Profile(PlanarError),
    /// A profile segment of a kind this constructor does not sweep yet.
    Segment(SegmentRef),
    /// Two regions of a prism or sheet overlap, touch or cross: they would merge in the
    /// plane first, which this build does not do.
    RegionsMeet {
        /// The index of the first region.
        first: usize,
        /// The index of the second.
        second: usize,
    },
    /// A horn torus: the radii are equal within the resolution, so the tube meets the axis
    /// at one point, a singular point that the pole rules do not cover (72 D3).
    HornTorus {
        /// The major radius (mm).
        major: f64,
        /// The minor radius (mm).
        minor: f64,
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
    /// A surface of the body could not be made from sizes already validated.
    Surface(SurfaceError),
    /// The writer gave a face no exact pcurves.
    Pcurve(PcurveWriteError),
    /// A solid face was written to a draft that has no solid region (a sheet's).
    NoSolid,
    /// `Draft::finish` refused the draft the constructor built.
    Finish(FinishCause),
}

/// Why no body was made.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum MakeError {
    /// The input is wrong.
    Refused(Refusal),
    /// The input is valid but this build does not make it.
    Unsupported(Unsupported),
    /// The constructor's own fault.
    Defect(Defect),
    /// A table ran out of ids.
    Full(TableFull),
}

impl MakeError {
    /// The kind: [`ErrorKind::Refused`] for a refusal, [`ErrorKind::Unsupported`] for an
    /// unsupported input, [`ErrorKind::Defect`] for a defect, [`ErrorKind::Exhausted`] for a
    /// full table.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Refused(_) => ErrorKind::Refused,
            Self::Unsupported(_) => ErrorKind::Unsupported,
            Self::Defect(_) => ErrorKind::Defect,
            Self::Full(_) => ErrorKind::Exhausted,
        }
    }

    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Refused(Refusal::Size { .. }) => "size",
            Self::Refused(Refusal::Profile(_)) => "profile",
            Self::Refused(Refusal::Span { .. }) => "span",
            Self::Refused(Refusal::Resolution { .. }) => "resolution",
            Self::Unsupported(Unsupported::Profile(_)) => "profile_undecided",
            Self::Unsupported(Unsupported::Segment(_)) => "segment_kind",
            Self::Refused(Refusal::ZeroRadii) => "zero_radii",
            Self::Refused(Refusal::NoRegions) => "no_regions",
            Self::Unsupported(Unsupported::RegionsMeet { .. }) => "regions_meet",
            Self::Refused(Refusal::SelfIntersecting { .. }) => "self_intersecting",
            Self::Unsupported(Unsupported::HornTorus { .. }) => "horn_torus",
            Self::Defect(Defect::Surface(_)) => "surface",
            Self::Defect(Defect::Curve(_)) => "curve",
            Self::Defect(Defect::Absent(_)) => "absent",
            Self::Defect(Defect::Interval) => "interval",
            Self::Defect(Defect::PlaneFrame) => "plane_frame",
            Self::Defect(Defect::Pcurve(_)) => "pcurve",
            Self::Defect(Defect::Finish(_)) => "finish",
            Self::Defect(Defect::NoSolid) => "no_solid",
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
            Self::Refused(Refusal::Profile(e)) => write!(f, "the profile: {e}"),
            Self::Refused(Refusal::Span { start, end, floor }) => write!(
                f,
                "the heights {start} mm and {end} mm are not finite and more than the resolution {} mm apart",
                floor.mm()
            ),
            Self::Refused(Refusal::Resolution { region, document }) => write!(
                f,
                "the region's resolution {} mm is not the document's {} mm",
                region.zone().mm(),
                document.zone().mm()
            ),
            Self::Unsupported(Unsupported::Profile(e)) => {
                write!(f, "the profile is undecided: {e}")
            }
            Self::Unsupported(Unsupported::Segment(at)) => {
                write!(f, "{at} is of a kind not swept yet")
            }
            Self::Refused(Refusal::ZeroRadii) => write!(f, "both radii of the cone are zero"),
            Self::Refused(Refusal::NoRegions) => write!(f, "no regions were given"),
            Self::Unsupported(Unsupported::RegionsMeet { first, second }) => write!(
                f,
                "regions {first} and {second} overlap, touch or cross: merging them is not done yet"
            ),
            Self::Refused(Refusal::SelfIntersecting { major, minor }) => write!(
                f,
                "the torus's minor radius {minor} mm exceeds its major radius {major} mm: the tube crosses its axis"
            ),
            Self::Unsupported(Unsupported::HornTorus { major, minor }) => write!(
                f,
                "the horn torus (major {major} mm, minor {minor} mm) is not made yet: its tube meets the axis"
            ),
            Self::Defect(Defect::Surface(e)) => write!(f, "a surface of the body: {e}"),
            Self::Defect(Defect::Curve(e)) => write!(f, "a curve of the body: {e}"),
            Self::Defect(Defect::Absent(e)) => e.fmt(f),
            Self::Defect(Defect::Interval) => write!(f, "an edge's interval is empty"),
            Self::Defect(Defect::PlaneFrame) => write!(f, "a face's plane has no frame"),
            Self::Defect(Defect::Pcurve(e)) => write!(f, "no exact pcurves: {e}"),
            Self::Defect(Defect::Finish(c)) => write!(f, "the draft is not a body: {c}"),
            Self::Defect(Defect::NoSolid) => write!(f, "a solid face was written to a sheet"),
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
            Self::Defect(Defect::Surface(e)) => Some(e),
            Self::Defect(Defect::Finish(c)) => Some(c),
            Self::Refused(Refusal::Profile(e)) | Self::Unsupported(Unsupported::Profile(e)) => {
                Some(e)
            }
            Self::Full(e) => Some(e),
            Self::Refused(_)
            | Self::Unsupported(_)
            | Self::Defect(Defect::PlaneFrame | Defect::Interval | Defect::NoSolid) => None,
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

impl From<SurfaceError> for MakeError {
    fn from(e: SurfaceError) -> Self {
        Self::Defect(Defect::Surface(e))
    }
}

/// A profile the planar layer refused is the caller's to change; one it could not decide is
/// a limit of this build.
impl From<PlanarError> for MakeError {
    fn from(e: PlanarError) -> Self {
        match e.kind() {
            ErrorKind::Unsupported => Self::Unsupported(Unsupported::Profile(e)),
            _ => Self::Refused(Refusal::Profile(e)),
        }
    }
}

/// `value` in mm if it is finite and above `floor`, else [`Refusal::Size`] naming
/// `dimension`. NaN is not above the floor, so it is refused too.
pub(crate) fn size(dimension: Dimension, value: Length, floor: Length) -> Result<f64, MakeError> {
    let mm = value.mm();
    if mm.is_finite() && mm > floor.mm() {
        Ok(mm)
    } else {
        Err(MakeError::Refused(Refusal::Size {
            dimension,
            value: mm,
            floor,
        }))
    }
}
