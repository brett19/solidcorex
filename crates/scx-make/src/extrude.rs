// Written for solidcorex. Apache-2.0.
//! The extrusion: a planar region swept along its frame's z axis between two heights, one
//! wall face per segment of each loop and a planar cap at each height (research 72 §4.2).
//!
//! The region is in the frame's `x, y` plane; a point `(x, y)` of it at height `z` is
//! `origin + x X + y Y + z Z`. Each loop gives a ring of vertices at each height, one per
//! segment start (a whole circle has its one at its seam), the rim edges between them, and a
//! vertical edge at each vertex. A line's wall is a plane, an arc's or a circle's a cylinder
//! about the vertical through its centre, convex (`Forward`) for a counter-clockwise arc and
//! concave for a clockwise one, so that every wall's front faces away from the solid.

use core::f64::consts::TAU;

use scx_brep::{Document, EdgeId, FaceId, VertexId};
use scx_curve::{Circle, Curve};
use scx_num::extent1d::Extent1d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_planar::{LoopId, Region, Segment, SegmentRef};
use scx_surface::{Cylinder, Plane, Surface};

use crate::Made;
use crate::assembly::Assembly;
use crate::error::{Defect, MakeError, Refusal, Unsupported};

/// The two heights an extrusion runs between, along its frame's z axis (mm). Either order:
/// an end below the start extrudes along `-z`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    /// The height of the start cap.
    pub start: Length,
    /// The height of the end cap.
    pub end: Length,
}

impl Span {
    /// From `start` to `end`.
    #[must_use]
    pub const fn new(start: Length, end: Length) -> Self {
        Self { start, end }
    }
}

/// The faces of an extrusion, by role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtrudeFaces {
    /// The cap at the span's start: its front faces away from the end.
    pub start: FaceId,
    /// The cap at the span's end.
    pub end: FaceId,
    /// The wall of each segment, `[loop][segment]`: loop 0 is the outer loop and loop
    /// `1 + k` hole `k`, each segment at its index as given to
    /// [`Region::new`], whichever way the region turned the loop.
    pub walls: Vec<Vec<FaceId>>,
}

/// Extrudes `region`, placed in `frame`'s `x, y` plane, along `frame`'s z axis from
/// `span.start` to `span.end`.
///
/// # Errors
///
/// - [`Refusal::Resolution`] when the region was built at a resolution other than `doc`'s.
/// - [`Refusal::Span`] when a height is not finite or they are not more than the
///   resolution apart.
/// - [`Unsupported::Segment`] for a segment kind other than a line, an arc or a circle.
/// - A [`Defect`] when the body does not finish, or [`MakeError::Full`].
///
/// A profile the planar layer refuses never reaches here: `Region::new`'s error converts to
/// [`MakeError`] by `?`, as [`Refusal::Profile`] or, undecided,
/// [`Unsupported::Profile`](crate::Unsupported::Profile).
pub fn extrude(
    region: &Region,
    frame: &Frame,
    span: Span,
    doc: &Document,
) -> Result<Made<ExtrudeFaces>, MakeError> {
    if region.resolution() != doc.resolution() {
        return Err(MakeError::Refused(Refusal::Resolution {
            region: region.resolution(),
            document: doc.resolution(),
        }));
    }
    let floor = doc.resolution().zone();
    let (start, end) = (span.start.mm(), span.end.mm());
    let apart = (end - start).abs();
    // NaN is not finite and not above the floor, so it is refused here too.
    if !(start.is_finite() && end.is_finite() && apart.is_finite() && apart > floor.mm()) {
        return Err(MakeError::Refused(Refusal::Span { start, end, floor }));
    }
    let (lo, hi) = if start < end {
        (start, end)
    } else {
        (end, start)
    };
    let place = Place::of(frame);
    let mut asm = Assembly::new(doc)?;
    let faces = extrude_into(&mut asm, region, &place, [lo, hi], start < end)?;
    Ok(Made {
        body: asm.finish()?,
        faces,
    })
}

/// The loops of `region` as the writer takes them, each with whether the region turned it
/// and its id, in the order outer, holes.
pub(crate) fn loops_of(region: &Region) -> impl Iterator<Item = (&scx_planar::Loop, bool, LoopId)> {
    let loops = core::iter::once(region.outer()).chain(region.holes());
    let reversed =
        core::iter::once(region.outer_reversed()).chain(region.holes_reversed().iter().copied());
    let ids = core::iter::once(LoopId::Outer).chain((0..region.holes().len()).map(LoopId::Hole));
    loops.zip(reversed).zip(ids).map(|((l, t), id)| (l, t, id))
}

/// Segment `i` of a loop of `n` as `Region::new` was given it, `turned` when the region ran
/// the loop the other way.
pub(crate) const fn given(in_loop: LoopId, turned: bool, n: usize, i: usize) -> SegmentRef {
    SegmentRef {
        in_loop,
        segment: if turned { n - 1 - i } else { i },
    }
}

/// Writes the solid of `region` between heights `[lo, hi]` into the solid `asm` is on: the
/// walls, then the two caps. `ascending` is whether the span started at `lo`.
pub(crate) fn extrude_into(
    asm: &mut Assembly<'_>,
    region: &Region,
    place: &Place,
    [lo, hi]: [f64; 2],
    ascending: bool,
) -> Result<ExtrudeFaces, MakeError> {
    let mut rings = Vec::with_capacity(1 + region.holes().len());
    let mut walls = Vec::with_capacity(1 + region.holes().len());
    for (l, turned, in_loop) in loops_of(region) {
        let n = l.segments().len();
        let at = |i: usize| given(in_loop, turned, n, i);
        let ring = ring(asm, place, l.segments(), [lo, hi], &at)?;
        let mut as_given = ring.walls.clone();
        if turned {
            as_given.reverse();
        }
        walls.push(as_given);
        rings.push(ring);
    }
    let top_frame = Frame::new(place.at(Point2::new(0.0, 0.0), hi), place.z, place.x)
        .ok_or(MakeError::Defect(Defect::PlaneFrame))?;
    let top_loops: Vec<Vec<(EdgeId, Sense)>> = rings.iter().map(|r| r.top.clone()).collect();
    let top = asm.face(Surface::Plane(Plane::new(top_frame)), &top_loops)?;
    let bottom_frame = Frame::new(place.at(Point2::new(0.0, 0.0), lo), place.z * -1.0, place.x)
        .ok_or(MakeError::Defect(Defect::PlaneFrame))?;
    let bottom_loops: Vec<Vec<(EdgeId, Sense)>> = rings
        .iter()
        .map(|r| {
            r.bottom
                .iter()
                .rev()
                .map(|&(e, s)| (e, s.reversed()))
                .collect()
        })
        .collect();
    let bottom = asm.face(Surface::Plane(Plane::new(bottom_frame)), &bottom_loops)?;
    let (start, end) = if ascending {
        (bottom, top)
    } else {
        (top, bottom)
    };
    Ok(ExtrudeFaces { start, end, walls })
}

/// Where a point of the region lands in space.
pub(crate) struct Place {
    origin: Point3,
    x: Vec3,
    y: Vec3,
    z: Vec3,
}

impl Place {
    /// The placement of `frame`'s axes.
    pub(crate) fn of(frame: &Frame) -> Self {
        Self {
            origin: frame.origin(),
            x: *frame.x(),
            y: *frame.y(),
            z: *frame.z(),
        }
    }

    /// `(p, z)` in the frame, placed once as the sum of its offsets added to the origin.
    pub(crate) fn at(&self, p: Point2, z: f64) -> Point3 {
        self.origin + (self.x * p.x + self.y * p.y + self.z * z)
    }

    /// The in-plane direction `(x, y)` in space.
    fn dir(&self, x: f64, y: f64) -> Vec3 {
        self.x * x + self.y * y
    }
}

/// One loop's rim edges at each height, as uses along the loop, and its walls in the
/// region's segment order.
struct Ring {
    bottom: Vec<(EdgeId, Sense)>,
    top: Vec<(EdgeId, Sense)>,
    walls: Vec<FaceId>,
}

fn ring(
    asm: &mut Assembly<'_>,
    place: &Place,
    segments: &[Segment],
    [lo, hi]: [f64; 2],
    at: &dyn Fn(usize) -> SegmentRef,
) -> Result<Ring, MakeError> {
    let n = segments.len();
    let mut low = Vec::with_capacity(n);
    let mut high = Vec::with_capacity(n);
    for s in segments {
        low.push(asm.vertex(place.at(s.start(), lo))?);
        high.push(asm.vertex(place.at(s.start(), hi))?);
    }
    let next =
        |v: &[VertexId]| -> Vec<VertexId> { v.iter().cycle().skip(1).take(n).copied().collect() };
    let (low_next, high_next) = (next(&low), next(&high));
    let mut bottom = Vec::with_capacity(n);
    let mut top = Vec::with_capacity(n);
    let mut uprights = Vec::with_capacity(n);
    for (i, s) in segments.iter().enumerate() {
        let (Some(&b), Some(&bn), Some(&t), Some(&tn)) =
            (low.get(i), low_next.get(i), high.get(i), high_next.get(i))
        else {
            return Err(MakeError::Defect(Defect::Interval));
        };
        bottom.push(rim(asm, place, s, lo, [b, bn], at(i))?);
        top.push(rim(asm, place, s, hi, [t, tn], at(i))?);
        uprights.push(asm.line(b, t)?);
    }
    let uprights_next: Vec<(EdgeId, Sense)> =
        uprights.iter().cycle().skip(1).take(n).copied().collect();
    let mut walls = Vec::with_capacity(n);
    for (i, ((((s, &b), &t), &(here, up)), &(there, up_next))) in segments
        .iter()
        .zip(&bottom)
        .zip(&top)
        .zip(&uprights)
        .zip(&uprights_next)
        .enumerate()
    {
        let surface = wall(place, s, lo, at(i))?;
        // Seen from the front: along the bottom, up at the segment's end, back along the
        // top, down at its start.
        let uses = vec![
            b,
            (there, up_next),
            (t.0, t.1.reversed()),
            (here, up.reversed()),
        ];
        walls.push(asm.face(surface, &[uses])?);
    }
    Ok(Ring { bottom, top, walls })
}

/// An arc's circle: from its counter-clockwise-first end over `sweep`, the radius the mean
/// of its two ends' distances (they agree within the resolution).
struct ArcGeom {
    centre: Point2,
    from: (f64, f64),
    radius: f64,
    sweep: f64,
}

fn arc_geom(centre: Point2, start: Point2, end: Point2, sense: Sense) -> ArcGeom {
    let (first, last) = match sense {
        Sense::Forward => (start, end),
        Sense::Reversed => (end, start),
    };
    let u = (first.x - centre.x, first.y - centre.y);
    let w = (last.x - centre.x, last.y - centre.y);
    let radius = 0.5 * ((u.0 * u.0 + u.1 * u.1).sqrt() + (w.0 * w.0 + w.1 * w.1).sqrt());
    let mut sweep = fmath::atan2(u.0 * w.1 - u.1 * w.0, u.0 * w.0 + u.1 * w.1);
    if sweep <= 0.0 {
        sweep += TAU;
    }
    ArcGeom {
        centre,
        from: u,
        radius,
        sweep,
    }
}

/// The rim edge of `s` at height `z` between `ends` (its start's vertex and its end's), as
/// the use that runs along the segment.
pub(crate) fn rim(
    asm: &mut Assembly<'_>,
    place: &Place,
    s: &Segment,
    z: f64,
    [from, to]: [VertexId; 2],
    at: SegmentRef,
) -> Result<(EdgeId, Sense), MakeError> {
    match *s {
        Segment::Line { .. } => asm.line(from, to),
        Segment::Arc {
            centre,
            start,
            end,
            sense,
        } => {
            let g = arc_geom(centre, start, end, sense);
            let frame = Frame::new(place.at(centre, z), place.z, place.dir(g.from.0, g.from.1))
                .ok_or(MakeError::Defect(Defect::PlaneFrame))?;
            let circle = Circle::new(frame, Length::from_mm(g.radius), Sense::Forward)?;
            let interval =
                Extent1d::new(0.0, g.sweep).ok_or(MakeError::Defect(Defect::Interval))?;
            let ends = match sense {
                Sense::Forward => [from, to],
                Sense::Reversed => [to, from],
            };
            Ok((asm.edge(Curve::Circle(circle), interval, ends)?, sense))
        }
        Segment::Circle {
            centre,
            radius,
            sense,
        } => {
            let frame = Frame::new(place.at(centre, z), place.z, place.x)
                .ok_or(MakeError::Defect(Defect::PlaneFrame))?;
            let circle = Circle::new(frame, Length::from_mm(radius), Sense::Forward)?;
            let turn = Extent1d::new(0.0, TAU).ok_or(MakeError::Defect(Defect::Interval))?;
            Ok((asm.edge(Curve::Circle(circle), turn, [from, from])?, sense))
        }
        _ => Err(MakeError::Unsupported(Unsupported::Segment(at))),
    }
}

/// The wall surface of `s`, its front away from the solid: right of the direction of
/// travel, which is outward for the counter-clockwise outer loop and into the hole for a
/// clockwise hole.
fn wall(place: &Place, s: &Segment, lo: f64, at: SegmentRef) -> Result<Surface, MakeError> {
    let cylinder = |centre: Point2, x: Vec3, radius: f64, sense: Sense| {
        let frame = Frame::new(place.at(centre, lo), place.z, x)
            .ok_or(MakeError::Defect(Defect::PlaneFrame))?;
        Ok::<_, MakeError>(Surface::Cylinder(Cylinder::new(
            frame,
            Length::from_mm(radius),
            sense,
        )?))
    };
    match *s {
        Segment::Arc {
            centre,
            start,
            end,
            sense,
        } => {
            let g = arc_geom(centre, start, end, sense);
            cylinder(g.centre, place.dir(g.from.0, g.from.1), g.radius, sense)
        }
        Segment::Circle {
            centre,
            radius,
            sense,
        } => cylinder(centre, place.x, radius, sense),
        Segment::Line { start: a, end: b } => {
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let frame = Frame::new(place.at(a, lo), place.dir(dy, -dx), place.dir(dx, dy))
                .ok_or(MakeError::Defect(Defect::PlaneFrame))?;
            Ok(Surface::Plane(Plane::new(frame)))
        }
        _ => Err(MakeError::Unsupported(Unsupported::Segment(at))),
    }
}
