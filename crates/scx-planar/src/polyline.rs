// Written for solidcorex. Apache-2.0.
//! The pin's 2-D polyline and tangent helpers (research 74 §1.3, BW9), as closed forms and
//! as [`crossings`](crate::crossings) of the polyline's segments.
//!
//! | pin (`measure`, `query::api_queries`) | here |
//! |---|---|
//! | `segment_2d_intersection(a1, a2, b1, b2, tol)`: the first hit | [`crossings`](crate::crossings) of two [`Curve2::Line`](crate::Curve2): every meet and overlap, certified |
//! | `polyline_2d_crossings(a, b, tol)`: the first hit of each segment pair, a `tol` that is also the box padding | [`polyline_crossings`]: every meet and overlap of each pair, with the segments' indices; the one length is the resolution |
//! | `polyline_2d_closest_point(p, pts)`: `None` for an empty polyline, one point is its own nearest | [`polyline_closest`]: the nearest point with its segment and parameter; an empty polyline is [`PolylineError::Empty`] |
//! | `polyline_2d_bounds` | `Extent2d::hull` of the points (`scx-num`) |
//! | `point_displacement(from, to)` | `to - from` (`Point2` subtraction, `scx-num`) |
//! | `centroid_and_mean_radius` | `scx_num::math::centroid_and_mean_radius` (already there) |
//! | `tangent_contact_line_circle_2d(a, b, c, r, tol)`: a segment within `tol` of touching, an end counted | [`tangent_segment_circle`]: the foot of the centre on the segment's interior within the resolution of the radius; an end on the circle is a crossing, not a tangent |
//! | `tangent_contact_circle_circle_2d(c1, r1, c2, r2, tol)` | [`tangent_circles`]: outside or inside tangent within the resolution; concentric circles have no contact point |
//!
//! No result depends on a tolerance the caller picks: the length is the [`Resolution`],
//! "the distance within which two places are one".

use core::fmt;

use scx_num::budget::{Budget, Stop};
use scx_num::error::ErrorKind;
use scx_num::extent2d::Extent2d;
use scx_num::units::Resolution;
use scx_num::vector2d::Point2;

use crate::cross::{CrossError, Curve2, Meet, Overlap, crossings};

/// Why a polyline or tangent request was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PolylineError {
    /// A coordinate or a radius that is not finite.
    NotFinite,
    /// No point: there is no nearest point of nothing.
    Empty,
    /// A segment whose ends are one point has no direction.
    Degenerate,
    /// A radius that is not above zero.
    NotPositive,
}

impl PolylineError {
    /// The kind: every cause is the caller's input.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        ErrorKind::Refused
    }

    /// The cause's stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::NotFinite => "not_finite",
            Self::Empty => "empty",
            Self::Degenerate => "degenerate",
            Self::NotPositive => "not_positive",
        }
    }
}

impl fmt::Display for PolylineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotFinite => "a coordinate or radius is not finite",
            Self::Empty => "there are no points",
            Self::Degenerate => "a segment's ends are one point",
            Self::NotPositive => "a radius is not above zero",
        })
    }
}

impl core::error::Error for PolylineError {}

fn finite(p: Point2) -> Result<(), PolylineError> {
    if p.x.is_finite() && p.y.is_finite() {
        Ok(())
    } else {
        Err(PolylineError::NotFinite)
    }
}

/// A meet of two polylines: which segment of each, and where.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolylineMeet {
    /// The index of the first polyline's segment (from point `a_segment` to the next).
    pub a_segment: usize,
    /// The index of the second polyline's segment.
    pub b_segment: usize,
    /// The meet, its parameters on the two segments (`[0, 1]` each).
    pub meet: Meet,
}

/// A stretch two polylines share.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolylineOverlap {
    /// The index of the first polyline's segment.
    pub a_segment: usize,
    /// The index of the second polyline's segment.
    pub b_segment: usize,
    /// The stretch, by the parameters on the two segments.
    pub overlap: Overlap,
}

/// Every meet and shared stretch of two polylines, ordered by the first polyline's
/// segment and then the second's.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PolylineCrossings {
    /// The meets.
    pub meets: Vec<PolylineMeet>,
    /// The shared stretches.
    pub overlaps: Vec<PolylineOverlap>,
}

/// Every place the polyline through `a` meets the polyline through `b`, segment pair by
/// segment pair ([`crossings`]): a vertex shared by two segments of one polyline that meets
/// the other there is reported once for each segment pair, so a caller that wants points
/// merges those with the same `at` itself. Segment pairs whose boxes are further apart than
/// the resolution are not examined.
///
/// A polyline of fewer than two points has no segment and meets nothing.
///
/// # Errors
///
/// [`CrossError`] for the first pair it refuses (a coordinate not finite or out of range, a
/// segment of no length) or cannot settle, the budget exhausted or the call stopped.
pub fn polyline_crossings(
    a: &[Point2],
    b: &[Point2],
    res: Resolution,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<PolylineCrossings, CrossError> {
    let pad = res.zone().mm();
    let mut out = PolylineCrossings::default();
    for (i, wa) in a.windows(2).enumerate() {
        let (Some(&a0), Some(&a1)) = (wa.first(), wa.get(1)) else {
            continue;
        };
        let box_a = Extent2d::spanning(a0, a1);
        for (j, wb) in b.windows(2).enumerate() {
            let (Some(&b0), Some(&b1)) = (wb.first(), wb.get(1)) else {
                continue;
            };
            if let (Some(x), Some(y)) = (box_a, Extent2d::spanning(b0, b1))
                && x.are_disjoint(&y, pad)
            {
                continue;
            }
            let found = crossings(
                &Curve2::Line { start: a0, end: a1 },
                &Curve2::Line { start: b0, end: b1 },
                res,
                budget,
                stop,
            )?;
            out.meets
                .extend(found.meets.into_iter().map(|meet| PolylineMeet {
                    a_segment: i,
                    b_segment: j,
                    meet,
                }));
            out.overlaps
                .extend(found.overlaps.into_iter().map(|overlap| PolylineOverlap {
                    a_segment: i,
                    b_segment: j,
                    overlap,
                }));
        }
    }
    Ok(out)
}

/// The nearest point of a polyline to a point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolylineClosest {
    /// The distance.
    pub distance: f64,
    /// The nearest point.
    pub at: Point2,
    /// The segment it lies on (the first, where two are equally near); `0` for a polyline of
    /// one point.
    pub segment: usize,
    /// Its parameter on that segment, in `[0, 1]`.
    pub t: f64,
}

/// The point of the polyline through `points` nearest `p`. A polyline of one point is that
/// point.
///
/// # Errors
///
/// [`PolylineError::NotFinite`] for a coordinate that is not; [`PolylineError::Empty`] for no
/// points.
pub fn polyline_closest(p: Point2, points: &[Point2]) -> Result<PolylineClosest, PolylineError> {
    finite(p)?;
    for &q in points {
        finite(q)?;
    }
    let Some(&first) = points.first() else {
        return Err(PolylineError::Empty);
    };
    let mut best = PolylineClosest {
        distance: p.distance(first),
        at: first,
        segment: 0,
        t: 0.0,
    };
    for (i, w) in points.windows(2).enumerate() {
        let (Some(&s), Some(&e)) = (w.first(), w.get(1)) else {
            continue;
        };
        let d = e - s;
        let len2 = d.dot(d);
        let t = if len2 > 0.0 {
            ((p - s).dot(d) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let at = s + d * t;
        let dist = p.distance(at);
        if dist < best.distance {
            best = PolylineClosest {
                distance: dist,
                at,
                segment: i,
                t,
            };
        }
    }
    Ok(best)
}

/// Where the segment from `a` to `b` touches the circle about `center` of `radius` without
/// crossing it, to the resolution: the foot of the centre on the segment lies within the
/// segment and its distance from the centre differs from `radius` by no more than
/// `res.zone()`. `None` where the segment crosses the circle, misses it, or only an end
/// lies on it (the foot is outside the segment: that is a crossing at the end, which
/// [`crossings`] finds).
///
/// The point is the foot on the segment.
///
/// # Errors
///
/// [`PolylineError::NotFinite`], [`PolylineError::Degenerate`] for `a == b`,
/// [`PolylineError::NotPositive`] for a radius not above zero.
pub fn tangent_segment_circle(
    a: Point2,
    b: Point2,
    center: Point2,
    radius: f64,
    res: Resolution,
) -> Result<Option<Point2>, PolylineError> {
    for p in [a, b, center] {
        finite(p)?;
    }
    if !radius.is_finite() {
        return Err(PolylineError::NotFinite);
    }
    if radius <= 0.0 {
        return Err(PolylineError::NotPositive);
    }
    let d = b - a;
    let len2 = d.dot(d);
    if len2 == 0.0 {
        return Err(PolylineError::Degenerate);
    }
    let t = (center - a).dot(d) / len2;
    if !(0.0..=1.0).contains(&t) {
        return Ok(None);
    }
    let foot = a + d * t;
    let gap = (foot.distance(center) - radius).abs();
    Ok((gap <= res.zone().mm()).then_some(foot))
}

/// Where the circles about `c1` of radius `r1` and about `c2` of radius `r2` touch without
/// crossing, to the resolution: outside (the distance of the centres within `res.zone()` of
/// `r1 + r2`) or inside (of `|r1 - r2|`). The point is half way between the two circles'
/// nearest points, on the line of the centres. `None` for circles that cross, miss, or are
/// concentric (equal circles are one circle: no contact point).
///
/// # Errors
///
/// [`PolylineError::NotFinite`], [`PolylineError::NotPositive`] for a radius not above zero.
pub fn tangent_circles(
    c1: Point2,
    r1: f64,
    c2: Point2,
    r2: f64,
    res: Resolution,
) -> Result<Option<Point2>, PolylineError> {
    finite(c1)?;
    finite(c2)?;
    if !(r1.is_finite() && r2.is_finite()) {
        return Err(PolylineError::NotFinite);
    }
    if r1 <= 0.0 || r2 <= 0.0 {
        return Err(PolylineError::NotPositive);
    }
    let z = res.zone().mm();
    let d = c1.distance(c2);
    // Concentric within the resolution: no line of centres.
    if d <= z {
        return Ok(None);
    }
    let u = (c2 - c1) / d;
    let outside = (d - (r1 + r2)).abs() <= z;
    let inside = (d - (r1 - r2).abs()).abs() <= z;
    if outside {
        // c1's nearest point is c1 + u r1, c2's is c2 - u r2.
        let (p, q) = (c1 + u * r1, c2 - u * r2);
        return Ok(Some(p + (q - p) * 0.5));
    }
    if inside {
        // The larger circle's point toward the smaller's centre, the smaller's the same
        // way: c1 + u r1 against c2 + u r2 when r1 > r2, mirrored otherwise.
        let s = if r1 > r2 { 1.0 } else { -1.0 };
        let (p, q) = (c1 + u * (s * r1), c2 + u * (s * r2));
        return Ok(Some(p + (q - p) * 0.5));
    }
    Ok(None)
}
