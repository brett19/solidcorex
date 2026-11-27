// Written for solidcorex. Apache-2.0.
//! [`SsiError`]: why the general intersector did not answer a pair (research 73 §4.10).
//!
//! Errors follow research 62 §10: every cause has a kind ([`SsiError::kind`]), a stable
//! [`SsiError::name`] and a one-frame Display. Each refusal names where it stopped, so a
//! caller (`scx-boolean`'s `SsiPair`) can say why. There is no string detail, no solver
//! status number and no empty answer in place of a refusal.
//!
//! | the pin's `ssi` crate | here |
//! |---|---|
//! | `Tangent { at, detail: String }` | [`SsiError::Singular`] with a [`SingularKind`] |
//! | `BadPatch(String)` | none: a surface that cannot be read is refused where it is built (`scx-nurbs`) |
//! | `Stalled(V3)` | [`SsiError::Stalled`], with the step it could not take |
//! | `ExitDiverged` | none: an arc's exits are seeds proved before it is marched (73 §4.4) |
//! | `Unfinished` (100,000 steps; a pair budget of 2,000,000; `/64` for B-spline pairs) | [`SsiError::Budget`]: the caller's one [`Budget`](scx_num::budget::Budget), no fixed caps |
//! | none: subdivision stopped by a fixed resolution and answered from what it had | [`SsiError::Unresolved`] |
//! | none: the output was tested at samples, never proved | [`SsiError::Uncertified`] |

use core::fmt;

use scx_num::budget::{Exhausted, Stopped};
use scx_num::error::ErrorKind;
use scx_num::units::Length;
use scx_num::vector3d::Point3;

/// A contact the general intersector has not built (73 §4.6): it is refused by name, never
/// answered as if it were an ordinary node or touch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SingularKind {
    /// A point where the normals are collinear and the relative form `II_A - II_B` is
    /// singular within its bound: a tacnode, a cusp or a touch of order above 2, not told
    /// apart. Neither a touch (definite form) nor a node (indefinite form).
    DegenerateForm,
    /// A tangent curve (a valley of the field) whose contact is of order above 2. Order 2
    /// is answered; orders 3 and 4 are row SM12's.
    HighOrderTangency,
}

impl SingularKind {
    /// The kind's stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::DegenerateForm => "degenerate_form",
            Self::HighOrderTangency => "high_order_tangency",
        }
    }
}

impl fmt::Display for SingularKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::DegenerateForm => "a contact whose relative curvature form is singular",
            Self::HighOrderTangency => "a tangent curve of contact order above 2",
        })
    }
}

/// Why the general intersector did not answer a pair (73 §4.10).
///
/// Every loop it runs is bounded: subdivision, marching steps and certification splits
/// stop at the resolution with the cause below, and every unit of work is charged to the
/// caller's budget. So each call ends with an answer or one of these.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum SsiError {
    /// A pair of pieces neither proved apart nor loop-free at the resolution: no proof
    /// says where its curves are, so nothing is answered for it (and nothing dropped).
    Unresolved {
        /// The middle of the undecided region.
        at: Point3,
        /// Its size: the longest side of its box.
        size: Length,
    },
    /// The march could not take a step of at least the resolution.
    Stalled {
        /// The last point it reached.
        at: Point3,
        /// The step it last tried.
        step: Length,
    },
    /// A contact this method has not built.
    Singular {
        /// Where.
        at: Point3,
        /// Which.
        kind: SingularKind,
    },
    /// A span of a traced curve whose certificate could not be closed at the resolution
    /// (73 §4.8). A curve is never kept with an unproven bound by default.
    Uncertified {
        /// A point of the span.
        at: Point3,
    },
    /// The caller's work budget ran out.
    Budget(Exhausted),
    /// The caller stopped the call.
    Stopped,
}

impl SsiError {
    /// The error's kind: [`ErrorKind::Unsupported`] for the four refusals (the pair is
    /// valid and this method cannot finish it), [`ErrorKind::Exhausted`] for the budget and
    /// [`ErrorKind::Cancelled`] for the stop.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Unresolved { .. }
            | Self::Stalled { .. }
            | Self::Singular { .. }
            | Self::Uncertified { .. } => ErrorKind::Unsupported,
            Self::Budget(_) => ErrorKind::Exhausted,
            Self::Stopped => ErrorKind::Cancelled,
        }
    }

    /// The cause's stable name (62 D6).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Unresolved { .. } => "ssi_unresolved",
            Self::Stalled { .. } => "ssi_stalled",
            Self::Singular { .. } => "ssi_singular",
            Self::Uncertified { .. } => "ssi_uncertified",
            Self::Budget(e) => e.name(),
            Self::Stopped => Stopped.name(),
        }
    }

    /// The point the refusal names; `None` for the budget and the stop, which name none.
    #[must_use]
    pub const fn at(&self) -> Option<Point3> {
        match *self {
            Self::Unresolved { at, .. }
            | Self::Stalled { at, .. }
            | Self::Singular { at, .. }
            | Self::Uncertified { at } => Some(at),
            Self::Budget(_) | Self::Stopped => None,
        }
    }
}

impl From<Exhausted> for SsiError {
    fn from(e: Exhausted) -> Self {
        Self::Budget(e)
    }
}

impl From<Stopped> for SsiError {
    fn from(_: Stopped) -> Self {
        Self::Stopped
    }
}

/// A point as Display writes it.
struct At(Point3);

impl fmt::Display for At {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Point3 { x, y, z } = self.0;
        write!(f, "({x}, {y}, {z})")
    }
}

impl fmt::Display for SsiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Unresolved { at, size } => write!(
                f,
                "a region of size {} at {} is neither proved apart nor loop-free",
                size.mm(),
                At(at)
            ),
            Self::Stalled { at, step } => write!(
                f,
                "the march stalled at {} with a step of {}",
                At(at),
                step.mm()
            ),
            Self::Singular { at, kind } => write!(f, "{kind} at {}", At(at)),
            Self::Uncertified { at } => {
                write!(f, "a span through {} could not be certified", At(at))
            }
            Self::Budget(e) => e.fmt(f),
            Self::Stopped => Stopped.fmt(f),
        }
    }
}

impl std::error::Error for SsiError {}
