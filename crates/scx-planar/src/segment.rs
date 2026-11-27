// Written for solidcorex. Apache-2.0.
//! A profile's segment: a straight line or a circular arc between two given points, or a
//! whole circle, which is a loop on its own.

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
    /// ends lie on one circle within the resolution. Equal ends are a degenerate arc, not
    /// a whole circle: that is [`Segment::Circle`].
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
    /// The whole circle about `centre` of `radius`, counter-clockwise for
    /// [`Sense::Forward`]: a loop on its own, with no ends. Its seam, where
    /// [`Segment::start`] and [`Segment::end`] put it, is the point `centre + (radius, 0)`.
    Circle {
        /// The centre.
        centre: Point2,
        /// The radius, above the resolution.
        radius: f64,
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

    /// The whole circle about `centre` of `radius`, counter-clockwise when `sense` is
    /// [`Sense::Forward`].
    #[must_use]
    pub const fn circle(centre: Point2, radius: f64, sense: Sense) -> Self {
        Self::Circle {
            centre,
            radius,
            sense,
        }
    }

    /// Where it starts: a circle's seam.
    #[must_use]
    pub const fn start(&self) -> Point2 {
        match *self {
            Self::Line { start, .. } | Self::Arc { start, .. } => start,
            Self::Circle { centre, radius, .. } => seam(centre, radius),
        }
    }

    /// Where it ends: a circle's seam, where it starts.
    #[must_use]
    pub const fn end(&self) -> Point2 {
        match *self {
            Self::Line { end, .. } | Self::Arc { end, .. } => end,
            Self::Circle { centre, radius, .. } => seam(centre, radius),
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
            Self::Circle {
                centre,
                radius,
                sense,
            } => Self::circle(centre, radius, sense.reversed()),
        }
    }

    /// The same segment ending at `end` instead: a circle has no end to move.
    pub(crate) const fn with_end(self, end: Point2) -> Self {
        match self {
            Self::Line { start, .. } => Self::line(start, end),
            Self::Arc {
                centre,
                start,
                sense,
                ..
            } => Self::arc(centre, start, end, sense),
            Self::Circle { .. } => self,
        }
    }

    /// Every coordinate it holds, a circle's radius among them.
    pub(crate) const fn coordinates(&self) -> [f64; 6] {
        match *self {
            Self::Line { start, end } => [start.x, start.y, end.x, end.y, 0.0, 0.0],
            Self::Arc {
                centre, start, end, ..
            } => [start.x, start.y, end.x, end.y, centre.x, centre.y],
            Self::Circle { centre, radius, .. } => [centre.x, centre.y, radius, 0.0, 0.0, 0.0],
        }
    }
}

/// A circle's seam: the point at angle 0.
const fn seam(centre: Point2, radius: f64) -> Point2 {
    Point2::new(centre.x + radius, centre.y)
}
