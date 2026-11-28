// Written for solidcorex. Apache-2.0.
//! [`RepairError`]: why a repair did not finish, in 01 §5.1's shape, and [`HealCause`]: why
//! one heal item cannot be done.
//!
//! A cause's kind is computed from its variant, never stored. Distances are millimetres; a
//! [`Bracket`] holds the true value.

use core::fmt;

use scx_brep::{EdgeId, Entity, FaceId, FinishCause, VertexId};
use scx_num::error::ErrorKind;
use scx_num::units::Length;
use scx_surface::Bracket;

/// Why a repair did not finish.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum RepairError {
    /// One failure: the lowest-index failing item's, or one about the whole call.
    Cause(RepairCause),
    /// Every failing item with its cause, in item order ([`OnItemFailure::Collect`] only);
    /// never empty.
    ///
    /// [`OnItemFailure::Collect`]: crate::OnItemFailure::Collect
    Items(Vec<scx_brep::op::Skipped<RepairCause>>),
    /// The budget ran out at `item`, after `work` units spent on the items below it.
    Exhausted {
        /// The item the budget ran out in.
        item: usize,
        /// The work spent before it.
        work: u64,
    },
    /// The stop flag was set.
    Cancelled,
    /// The method failed on a body it accepted: a bug.
    Defect(RepairDefect),
}

/// One failure of a repair.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum RepairCause {
    /// The body was finished under another resolution than the operation's document.
    OtherDocument,
    /// A heal item cannot be done.
    Heal(HealCause),
}

/// Why one heal item cannot be done. Each names the entity it is at; the refused causes say
/// what accuracy the item would need, which the heal never grows.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum HealCause {
    /// A vertex of the cluster is not certified within the survivor's reach of the target.
    Spread {
        /// The vertex.
        vertex: VertexId,
        /// Its distance from the target.
        distance: Bracket,
        /// The survivor's reach.
        allowed: Length,
    },
    /// An edge squeezed to the target is not certified within the survivor's reach of it.
    TooLong {
        /// The edge.
        edge: EdgeId,
        /// The bound on its distance from the target.
        bound: Bracket,
        /// The survivor's reach.
        allowed: Length,
    },
    /// A re-ended edge is not certified within its reach and its use's of the face.
    OffFace {
        /// The edge.
        edge: EdgeId,
        /// The face.
        face: FaceId,
        /// The lift's gap from the curve at the worse end.
        gap: Bracket,
        /// The smaller of the edge's and the use's reach.
        allowed: Length,
    },
    /// A re-ended edge's curve is not certified to end at its vertex.
    EndOffVertex {
        /// The edge.
        edge: EdgeId,
        /// The gap at the worse end.
        gap: Bracket,
        /// The edge's reach and the vertex's, added.
        allowed: Length,
    },
    /// The target is not certified clear of another vertex: the heal would make a new pair
    /// within coincidence.
    NotClear {
        /// The other vertex.
        vertex: VertexId,
        /// Its distance from the target.
        distance: Bracket,
        /// The larger of the two reaches.
        allowed: Length,
    },
    /// A re-ended edge is not certified longer than its reach: it would be degenerate.
    NeighbourShort {
        /// The edge.
        edge: EdgeId,
        /// Its length.
        length: Bracket,
        /// Its reach.
        allowed: Length,
    },
    /// An edge at the cluster is not a line or a NURBS curve clamped at its ends (nor
    /// squeezed whole), or is collapsed.
    NeighbourKind {
        /// The edge.
        edge: EdgeId,
    },
    /// A face at the cluster is not a plane, or a use there is a point.
    FaceKind {
        /// The face.
        face: FaceId,
    },
    /// A loop would be left with fewer than three uses: a sliver face, a later heal's item.
    Sliver {
        /// The face.
        face: FaceId,
    },
    /// Every use of a loop would be squeezed.
    LoopVanishes {
        /// The face.
        face: FaceId,
    },
    /// A loop would pass through the survivor more than once.
    Pinch {
        /// The face.
        face: FaceId,
    },
    /// An edge's empty pieces, cut, would move it by more than its reach: their control
    /// points' spreads, added (RP2).
    PiecesSpread {
        /// The edge.
        edge: EdgeId,
        /// The spreads added.
        bound: Bracket,
        /// The edge's reach.
        allowed: Length,
    },
    /// An edge's empty pieces cut leave knots that do not make a curve (a later span
    /// rounded to nothing) (RP2).
    Cut {
        /// The edge.
        edge: EdgeId,
    },
    /// A rebuilt curve, or an enclosure of a distance, overflows.
    NotFinite {
        /// The entity it is at.
        at: Entity,
    },
}

/// Where a repair broke an invariant of its own.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum RepairDefect {
    /// The draft refused an edit of an entity the heal read from it.
    Draft,
    /// The healed draft did not finish, though every edit was certified.
    Finish(Box<FinishCause>),
}

impl HealCause {
    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Spread { .. }
            | Self::TooLong { .. }
            | Self::OffFace { .. }
            | Self::EndOffVertex { .. }
            | Self::NotClear { .. }
            | Self::NeighbourShort { .. }
            | Self::PiecesSpread { .. } => ErrorKind::Refused,
            Self::NeighbourKind { .. }
            | Self::FaceKind { .. }
            | Self::Sliver { .. }
            | Self::LoopVanishes { .. }
            | Self::Pinch { .. }
            | Self::Cut { .. }
            | Self::NotFinite { .. } => ErrorKind::Unsupported,
        }
    }

    /// A stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Spread { .. } => "spread",
            Self::TooLong { .. } => "too_long",
            Self::OffFace { .. } => "off_face",
            Self::EndOffVertex { .. } => "end_off_vertex",
            Self::NotClear { .. } => "not_clear",
            Self::NeighbourShort { .. } => "neighbour_short",
            Self::NeighbourKind { .. } => "neighbour_kind",
            Self::FaceKind { .. } => "face_kind",
            Self::Sliver { .. } => "sliver",
            Self::LoopVanishes { .. } => "loop_vanishes",
            Self::Pinch { .. } => "pinch",
            Self::PiecesSpread { .. } => "pieces_spread",
            Self::Cut { .. } => "cut",
            Self::NotFinite { .. } => "not_finite",
        }
    }
}

impl RepairCause {
    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::OtherDocument => ErrorKind::Refused,
            Self::Heal(c) => c.kind(),
        }
    }

    /// A stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::OtherDocument => "other_document",
            Self::Heal(c) => c.name(),
        }
    }
}

impl RepairError {
    /// The kind: of the lowest item's cause for [`RepairError::Items`].
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Cause(c) => c.kind(),
            Self::Items(items) => items.first().map_or(ErrorKind::Defect, |s| s.cause.kind()),
            Self::Exhausted { .. } => ErrorKind::Exhausted,
            Self::Cancelled => ErrorKind::Cancelled,
            Self::Defect(_) => ErrorKind::Defect,
        }
    }

    /// A stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Cause(c) => c.name(),
            Self::Items(_) => "items",
            Self::Exhausted { .. } => "exhausted",
            Self::Cancelled => "cancelled",
            Self::Defect(RepairDefect::Draft) => "draft",
            Self::Defect(RepairDefect::Finish(_)) => "finish",
        }
    }
}

fn bracket(f: &mut fmt::Formatter<'_>, b: &Bracket, allowed: Length) -> fmt::Result {
    write!(
        f,
        "[{:e}, {:e}] mm against {:e} mm",
        b.lower(),
        b.upper(),
        allowed.mm()
    )
}

impl fmt::Display for HealCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spread {
                vertex,
                distance,
                allowed,
            } => {
                write!(
                    f,
                    "{vertex}: not within the survivor's reach of the target: "
                )?;
                bracket(f, distance, *allowed)
            }
            Self::TooLong {
                edge,
                bound,
                allowed,
            } => {
                write!(f, "{edge}: not within the survivor's reach of the target: ")?;
                bracket(f, bound, *allowed)
            }
            Self::OffFace {
                edge,
                face,
                gap,
                allowed,
            } => {
                write!(f, "{edge}: re-ended, not within reach of {face}: ")?;
                bracket(f, gap, *allowed)
            }
            Self::EndOffVertex { edge, gap, allowed } => {
                write!(f, "{edge}: re-ended, its curve off its vertex: ")?;
                bracket(f, gap, *allowed)
            }
            Self::NotClear {
                vertex,
                distance,
                allowed,
            } => {
                write!(f, "{vertex}: the target is not clear of it: ")?;
                bracket(f, distance, *allowed)
            }
            Self::NeighbourShort {
                edge,
                length,
                allowed,
            } => {
                write!(f, "{edge}: re-ended, not longer than its reach: ")?;
                bracket(f, length, *allowed)
            }
            Self::NeighbourKind { edge } => {
                write!(
                    f,
                    "{edge}: at the cluster and not a line or clamped NURBS to re-end"
                )
            }
            Self::FaceKind { face } => write!(f, "{face}: at the cluster and not a plane"),
            Self::Sliver { face } => write!(f, "{face}: a loop would keep fewer than three uses"),
            Self::LoopVanishes { face } => write!(f, "{face}: a loop would be squeezed whole"),
            Self::Pinch { face } => write!(f, "{face}: a loop would pass the survivor twice"),
            Self::PiecesSpread {
                edge,
                bound,
                allowed,
            } => {
                write!(
                    f,
                    "{edge}: its empty pieces cut would move it past its reach: "
                )?;
                bracket(f, bound, *allowed)
            }
            Self::Cut { edge } => write!(f, "{edge}: its empty pieces cut leave no curve"),
            Self::NotFinite { at } => write!(f, "{at}: a rebuilt curve or an enclosure overflows"),
        }
    }
}

impl fmt::Display for RepairCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OtherDocument => {
                f.write_str("the body was finished under another document's resolution")
            }
            Self::Heal(c) => c.fmt(f),
        }
    }
}

impl fmt::Display for RepairError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cause(c) => c.fmt(f),
            Self::Items(items) => {
                write!(f, "{} items failed", items.len())?;
                if let Some(s) = items.first() {
                    write!(f, "; item {}: {}", s.item, s.cause)?;
                }
                Ok(())
            }
            Self::Exhausted { item, work } => {
                write!(f, "the budget ran out at item {item}, after {work} units")
            }
            Self::Cancelled => f.write_str("stopped"),
            Self::Defect(RepairDefect::Draft) => f.write_str("the draft refused an edit"),
            Self::Defect(RepairDefect::Finish(cause)) => {
                write!(f, "the healed body did not finish: {cause}")
            }
        }
    }
}

impl std::error::Error for RepairError {}
