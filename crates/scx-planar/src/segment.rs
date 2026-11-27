// Written for solidcorex. Apache-2.0.
//! A profile's segment: a straight line or a circular arc between two given points.

use scx_num::frame::Sense;
use scx_num::vector2d::Point2;

/// A segment of a profile loop, from `start` to `end`.
///
/// The ends are data, not computed: closure and touching between segments are decided on
/// the points given, so a profile whose joints are exact stays exact (research 72 §14.6).
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Segment {
    /// The straight segment.
    Line {
        /// First end.
        start: Point2,
        /// Second end.
        end: Point2,
    },
    /// The circular arc about `centre` from `start` to `end`: counter-clockwise for
    /// [`Sense::Forward`] (about the plane's z axis), clockwise for [`Sense::Reversed`]. The
    /// ends lie on one circle within the resolution; a full circle is two arcs.
    Arc {
        /// The circle's centre.
        centre: Point2,
        /// First end.
        start: Point2,
        /// Second end.
        end: Point2,
        /// The sense of travel about the centre.
        sense: Sense,
    },
}

impl Segment {
    /// The straight segment from `start` to `end`.
    #[must_use]
    pub const fn line(start: Point2, end: Point2) -> Self {
        Self::Line { start, end }
    }

    /// The arc about `centre` from `start` to `end`, counter-clockwise when `sense` is
    /// [`Sense::Forward`].
    #[must_use]
    pub const fn arc(centre: Point2, start: Point2, end: Point2, sense: Sense) -> Self {
        Self::Arc {
            centre,
            start,
            end,
            sense,
        }
    }

    /// Where it starts.
    #[must_use]
    pub const fn start(&self) -> Point2 {
        match *self {
            Self::Line { start, .. } | Self::Arc { start, .. } => start,
        }
    }

    /// Where it ends.
    #[must_use]
    pub const fn end(&self) -> Point2 {
        match *self {
            Self::Line { end, .. } | Self::Arc { end, .. } => end,
        }
    }

    /// The same point set traversed the other way.
    #[must_use]
    pub fn reversed(&self) -> Self {
        match *self {
            Self::Line { start, end } => Self::line(end, start),
            Self::Arc {
                centre,
                start,
                end,
                sense,
            } => Self::arc(centre, end, start, sense.reversed()),
        }
    }

    /// The same segment ending at `end` instead.
    pub(crate) const fn with_end(self, end: Point2) -> Self {
        match self {
            Self::Line { start, .. } => Self::line(start, end),
            Self::Arc {
                centre,
                start,
                sense,
                ..
            } => Self::arc(centre, start, end, sense),
        }
    }

    /// Every coordinate it holds.
    pub(crate) const fn coordinates(&self) -> [f64; 6] {
        match *self {
            Self::Line { start, end } => [start.x, start.y, end.x, end.y, 0.0, 0.0],
            Self::Arc {
                centre, start, end, ..
            } => [start.x, start.y, end.x, end.y, centre.x, centre.y],
        }
    }
}
