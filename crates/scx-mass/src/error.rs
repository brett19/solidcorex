// Written for solidcorex. Apache-2.0.
//! [`MassError`]: why a measurement did not finish, by [`ErrorKind`].
//!
//! No variant carries a solver status number; each names the entity it failed at.

use core::fmt;

use scx_brep::{EdgeId, Entity, FaceId, ShellId};
use scx_num::error::ErrorKind;

/// Why a measurement did not finish.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum MassError {
    /// The request is not one a body can answer.
    Refused(MassRefusal),
    /// The body is valid and this method cannot measure it yet.
    Unsupported(MassUnsupported),
    /// The budget ran out.
    Exhausted {
        /// The budget's limit.
        limit: u64,
    },
    /// The stop flag was set.
    Cancelled,
    /// The integrator failed on a valid body: a bug.
    Defect(MassDefect),
}

/// Why a request was refused.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum MassRefusal {
    /// A relative accuracy that is not finite or not in `(0, 1]`.
    BadAccuracy {
        /// The value asked for.
        value: f64,
    },
    /// A density that is not finite and above zero.
    BadDensity {
        /// The value asked for.
        value: f64,
    },
    /// A reference point that is not finite.
    BadReference,
    /// An enclosure width that is not finite and at least zero.
    BadWidth {
        /// The value asked for.
        value: f64,
    },
    /// A shell the body does not have.
    NoShell {
        /// The shell.
        shell: ShellId,
    },
    /// A shell whose certified volume sign is not the one its region gives it: the
    /// outside's shell positive, or a region with no positive shell. Its faces face the
    /// wrong way.
    NotOutward {
        /// The shell.
        shell: ShellId,
    },
    /// The faces measured do not close: this edge's uses do not balance.
    OpenBoundary {
        /// The edge.
        edge: EdgeId,
    },
}

/// What this method does not measure yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MassUnsupported {
    /// A loop that winds round the surface's `v` period: the boundary integral needs a
    /// closing path the loops do not give.
    VWrap {
        /// The face.
        face: FaceId,
    },
    /// Loops whose windings round the `u` period do not cancel: the face holds a pole or a
    /// seam end that no use names.
    NetWinding {
        /// The face.
        face: FaceId,
    },
    /// An entity with no box: it reaches infinity, or cannot be enclosed.
    NoBox {
        /// The entity.
        at: Entity,
    },
    /// A face kind with no series form for the enclosure.
    NoSeries {
        /// The face.
        face: FaceId,
    },
    /// The volume enclosure still holds zero at its cell cap or floor: a gap between faces
    /// as large as the volume, or a volume below the rounding.
    SignUndecided {
        /// The shell.
        shell: ShellId,
    },
}

/// The integrator's own failure on a valid body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MassDefect {
    /// The step control gave up: the depth or piece bound, before the accuracy was met.
    StepControl {
        /// The face or edge.
        at: Entity,
    },
    /// The integrand was not finite: a surface or curve evaluated off its domain.
    IntegrandNotFinite {
        /// The face or edge.
        at: Entity,
    },
    /// A shell's volume estimate contradicts its region by more than its error, yet the
    /// certified signs say the body is outward: the estimate is wrong.
    EstimateMissed {
        /// The shell.
        shell: ShellId,
    },
}

impl MassError {
    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Refused(_) => ErrorKind::Refused,
            Self::Unsupported(_) => ErrorKind::Unsupported,
            Self::Exhausted { .. } => ErrorKind::Exhausted,
            Self::Cancelled => ErrorKind::Cancelled,
            Self::Defect(_) => ErrorKind::Defect,
        }
    }

    /// A stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Refused(MassRefusal::BadAccuracy { .. }) => "bad_accuracy",
            Self::Refused(MassRefusal::BadDensity { .. }) => "bad_density",
            Self::Refused(MassRefusal::BadReference) => "bad_reference",
            Self::Refused(MassRefusal::BadWidth { .. }) => "bad_width",
            Self::Refused(MassRefusal::NoShell { .. }) => "no_shell",
            Self::Refused(MassRefusal::NotOutward { .. }) => "not_outward",
            Self::Refused(MassRefusal::OpenBoundary { .. }) => "open_boundary",
            Self::Unsupported(MassUnsupported::VWrap { .. }) => "v_wrap",
            Self::Unsupported(MassUnsupported::NetWinding { .. }) => "net_winding",
            Self::Unsupported(MassUnsupported::NoBox { .. }) => "no_box",
            Self::Unsupported(MassUnsupported::NoSeries { .. }) => "no_series",
            Self::Unsupported(MassUnsupported::SignUndecided { .. }) => "sign_undecided",
            Self::Exhausted { .. } => "exhausted",
            Self::Cancelled => "cancelled",
            Self::Defect(MassDefect::StepControl { .. }) => "step_control",
            Self::Defect(MassDefect::IntegrandNotFinite { .. }) => "integrand_not_finite",
            Self::Defect(MassDefect::EstimateMissed { .. }) => "estimate_missed",
        }
    }
}

impl fmt::Display for MassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Refused(MassRefusal::BadAccuracy { value }) => {
                write!(f, "relative accuracy {value} is not in (0, 1]")
            }
            Self::Refused(MassRefusal::BadDensity { value }) => {
                write!(f, "density {value} is not finite and above zero")
            }
            Self::Refused(MassRefusal::BadReference) => {
                f.write_str("the reference point is not finite")
            }
            Self::Refused(MassRefusal::BadWidth { value }) => {
                write!(f, "enclosure width {value} is not finite and at least zero")
            }
            Self::Refused(MassRefusal::NoShell { shell }) => write!(f, "{shell}: no such shell"),
            Self::Refused(MassRefusal::NotOutward { shell }) => {
                write!(f, "{shell}: its faces do not face out of its region")
            }
            Self::Refused(MassRefusal::OpenBoundary { edge }) => {
                write!(f, "{edge}: its uses do not balance, the faces do not close")
            }
            Self::Unsupported(MassUnsupported::VWrap { face }) => {
                write!(f, "{face}: a loop winds round the v period")
            }
            Self::Unsupported(MassUnsupported::NetWinding { face }) => {
                write!(f, "{face}: the loops' u windings do not cancel")
            }
            Self::Unsupported(MassUnsupported::NoBox { at }) => write!(f, "{at}: no box"),
            Self::Unsupported(MassUnsupported::NoSeries { face }) => {
                write!(f, "{face}: the surface kind has no series form")
            }
            Self::Unsupported(MassUnsupported::SignUndecided { shell }) => {
                write!(f, "{shell}: the volume enclosure still holds zero")
            }
            Self::Exhausted { limit } => write!(f, "the budget of {limit} ran out"),
            Self::Cancelled => f.write_str("stopped"),
            Self::Defect(MassDefect::StepControl { at }) => {
                write!(
                    f,
                    "{at}: the step control gave up before the accuracy was met"
                )
            }
            Self::Defect(MassDefect::IntegrandNotFinite { at }) => {
                write!(f, "{at}: the integrand is not finite")
            }
            Self::Defect(MassDefect::EstimateMissed { shell }) => {
                write!(
                    f,
                    "{shell}: the volume estimate's error misses the certified sign"
                )
            }
        }
    }
}

impl std::error::Error for MassError {}
