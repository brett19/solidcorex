// Written for solidcorex. Apache-2.0.
//! Why a region cannot be built, or a point cannot be placed in it.
//!
//! Errors follow research 62 §10: every cause has a kind ([`PlanarError::kind`]), a stable
//! name ([`PlanarError::name`]) and a Display of one frame. Every cause is the caller's
//! profile, kind `Refused`, except [`PlanarError::Undecided`], kind `Unsupported`: a valid
//! profile whose orientation or nesting the certified arithmetic cannot settle.
//!
//! [`Region::new`](crate::Region::new) reports one cause, found in a fixed order (research
//! 72 §14.6): numbers, then closure, then degenerate segments, then contacts between
//! segments, then nesting, then orientation.
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `check_regions`: a NaN or a coordinate past `1e9` is `ProfileRefusal::FreeForm` | [`PlanarError::NotFinite`], [`PlanarError::OutOfRange`] |
//! | `check_regions`: a loop whose ends miss by more than the zone is refused | [`PlanarError::Open`] |
//! | `check_regions`: an arc of radius within the zone is refused | [`Degenerate::Radius`] |
//! | none: a segment shorter than the zone builds | [`Degenerate::Short`] |
//! | none: an arc whose ends lie on different circles builds | [`Degenerate::EndOffCircle`] |
//! | none: crossing and touching loops build | [`PlanarError::Crossing`], [`PlanarError::Touching`] |
//! | none: a hole outside the outer loop, or inside another hole, builds | [`PlanarError::HoleOutside`], [`PlanarError::HoleInHole`] |
//! | `ProfileRefusal::Budget` (`MAX_SEGMENTS`) | none: the contact test is sub-quadratic on profiles, 72 §14.6 |

use core::fmt;

use scx_num::error::ErrorKind;

/// Which loop of a profile: the outer loop, or a hole by its index in the holes given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LoopId {
    /// The outer loop.
    Outer,
    /// Hole `i`, in the order given.
    Hole(usize),
}

/// One segment of a profile: its loop and its index in that loop, as given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SegmentRef {
    /// The loop.
    pub in_loop: LoopId,
    /// The index in the loop.
    pub segment: usize,
}

/// Why a segment is too small, or inconsistent, to be part of a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Degenerate {
    /// Its ends are not proved more than the resolution apart.
    Short,
    /// An arc whose radius is not proved more than the resolution.
    Radius,
    /// An arc whose ends are proved to lie on circles more than the resolution apart.
    EndOffCircle,
}

/// Why a region cannot be built, or a point cannot be placed in it.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum PlanarError {
    /// A coordinate is infinite or NaN: closure cannot be judged.
    NotFinite {
        /// The first such segment.
        at: SegmentRef,
    },
    /// A coordinate is outside `[1e-100, 1e9]` in magnitude (and not 0), where the exact
    /// orientation test would lose exactness to underflow or the resolution would drown in
    /// rounding.
    OutOfRange {
        /// The first such segment.
        at: SegmentRef,
    },
    /// A loop with no segments.
    Empty {
        /// The loop.
        in_loop: LoopId,
    },
    /// A loop does not close: segment `joint` ends further than the resolution from where
    /// the next segment (cyclically) starts, or is not proved within it.
    Open {
        /// The loop.
        in_loop: LoopId,
        /// The segment whose end misses.
        joint: usize,
        /// The distance left open, rounded.
        gap: f64,
    },
    /// A segment too small or inconsistent to be part of a profile.
    Degenerate {
        /// The segment.
        at: SegmentRef,
        /// Why.
        why: Degenerate,
    },
    /// Two segments are proved to cross at a point interior to both.
    Crossing {
        /// The lesser segment.
        a: SegmentRef,
        /// The greater segment.
        b: SegmentRef,
    },
    /// Two segments come within the resolution of each other away from a shared end (or
    /// are not proved apart there), and are not proved to cross.
    Touching {
        /// The lesser segment.
        a: SegmentRef,
        /// The greater segment.
        b: SegmentRef,
    },
    /// Hole `hole` is not inside the outer loop.
    HoleOutside {
        /// The hole's index.
        hole: usize,
    },
    /// Hole `inner` lies inside hole `outer`.
    HoleInHole {
        /// The enclosing hole.
        outer: usize,
        /// The enclosed hole.
        inner: usize,
    },
    /// The certified arithmetic cannot settle a loop's orientation or a winding number.
    Undecided {
        /// The loop.
        in_loop: LoopId,
    },
}

impl PlanarError {
    /// The cause's kind (research 62 §10): the profile's fault, or a method's limit.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Undecided { .. } => ErrorKind::Unsupported,
            Self::NotFinite { .. }
            | Self::OutOfRange { .. }
            | Self::Empty { .. }
            | Self::Open { .. }
            | Self::Degenerate { .. }
            | Self::Crossing { .. }
            | Self::Touching { .. }
            | Self::HoleOutside { .. }
            | Self::HoleInHole { .. } => ErrorKind::Refused,
        }
    }

    /// A stable name for the cause.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::NotFinite { .. } => "NotFinite",
            Self::OutOfRange { .. } => "OutOfRange",
            Self::Empty { .. } => "Empty",
            Self::Open { .. } => "Open",
            Self::Degenerate { why, .. } => match why {
                Degenerate::Short => "Degenerate.Short",
                Degenerate::Radius => "Degenerate.Radius",
                Degenerate::EndOffCircle => "Degenerate.EndOffCircle",
            },
            Self::Crossing { .. } => "Crossing",
            Self::Touching { .. } => "Touching",
            Self::HoleOutside { .. } => "HoleOutside",
            Self::HoleInHole { .. } => "HoleInHole",
            Self::Undecided { .. } => "Undecided",
        }
    }
}

impl fmt::Display for LoopId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Outer => f.write_str("the outer loop"),
            Self::Hole(i) => write!(f, "hole {i}"),
        }
    }
}

impl fmt::Display for SegmentRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "segment {} of {}", self.segment, self.in_loop)
    }
}

impl fmt::Display for PlanarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFinite { at } => write!(f, "{at} has a non-finite coordinate"),
            Self::OutOfRange { at } => write!(f, "{at} has a coordinate out of range"),
            Self::Empty { in_loop } => write!(f, "{in_loop} has no segments"),
            Self::Open {
                in_loop,
                joint,
                gap,
            } => write!(
                f,
                "{in_loop} does not close: segment {joint} ends {gap:e} from the next start"
            ),
            Self::Degenerate { at, why } => {
                let why = match why {
                    Degenerate::Short => "is not longer than the resolution",
                    Degenerate::Radius => "has a radius not above the resolution",
                    Degenerate::EndOffCircle => "has its ends on different circles",
                };
                write!(f, "{at} {why}")
            }
            Self::Crossing { a, b } => write!(f, "{a} crosses {b}"),
            Self::Touching { a, b } => write!(f, "{a} touches {b} away from a shared end"),
            Self::HoleOutside { hole } => write!(f, "hole {hole} is not inside the outer loop"),
            Self::HoleInHole { outer, inner } => write!(f, "hole {inner} lies inside hole {outer}"),
            Self::Undecided { in_loop } => {
                write!(f, "the orientation or winding of {in_loop} is not decided")
            }
        }
    }
}

impl core::error::Error for PlanarError {}
