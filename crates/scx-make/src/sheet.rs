// Written for solidcorex. Apache-2.0.
//! The sheet of several planar regions: one planar face for each region, in the frame's
//! `x, y` plane, in one body with no solid (research 75 MK14; the pin's `sheet_of`).
//!
//! Each face is its own shell in the outside region, both of its sides in it, so a sheet
//! has area and no volume. Its front is the frame's `+z` side, and its outer loop runs
//! counter-clockwise about it. The regions follow [`prism`](crate::prism)'s rules: they must
//! not overlap, touch or cross.

use scx_brep::{Document, EdgeId, FaceId, VertexId};
use scx_num::frame::{Frame, Sense};
use scx_num::vector2d::Point2;
use scx_planar::Region;
use scx_surface::{Plane, Surface};

use crate::Made;
use crate::assembly::Assembly;
use crate::error::{Defect, MakeError};
use crate::extrude::{Place, given, loops_of, rim};
use crate::prism::check;

/// The faces of a sheet, by region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetFaces {
    /// The face of each region, in the order the regions were given.
    pub faces: Vec<FaceId>,
}

/// The sheet of `regions`, placed in `frame`'s `x, y` plane.
///
/// # Errors
///
/// - [`Refusal::NoRegions`](crate::Refusal::NoRegions) for none, and
///   [`Refusal::Resolution`](crate::Refusal::Resolution) for a region at another
///   resolution than `doc`'s.
/// - [`Unsupported::RegionsMeet`](crate::Unsupported::RegionsMeet) when two regions overlap,
///   touch or cross.
/// - [`Unsupported::Segment`](crate::Unsupported::Segment) for a segment other than a line,
///   an arc or a circle.
/// - A [`Defect`] when the body does not finish, or [`MakeError::Full`].
pub fn sheet(
    regions: &[Region],
    frame: &Frame,
    doc: &Document,
) -> Result<Made<SheetFaces>, MakeError> {
    check(regions, doc.resolution())?;
    let place = Place::of(frame);
    let plane = Frame::new(place.at(Point2::new(0.0, 0.0), 0.0), *frame.z(), *frame.x())
        .ok_or(MakeError::Defect(Defect::PlaneFrame))?;
    let mut asm = Assembly::sheets(doc);
    let mut faces = Vec::with_capacity(regions.len());
    for region in regions {
        let mut loops = Vec::with_capacity(1 + region.holes().len());
        for (l, turned, in_loop) in loops_of(region) {
            let segments = l.segments();
            let n = segments.len();
            let mut vertices: Vec<VertexId> = Vec::with_capacity(n);
            for s in segments {
                vertices.push(asm.vertex(place.at(s.start(), 0.0))?);
            }
            let mut uses: Vec<(EdgeId, Sense)> = Vec::with_capacity(n);
            for (i, (s, &from)) in segments.iter().zip(&vertices).enumerate() {
                let to = vertices
                    .get((i + 1) % n)
                    .copied()
                    .ok_or(MakeError::Defect(Defect::Interval))?;
                uses.push(rim(
                    &mut asm,
                    &place,
                    s,
                    0.0,
                    [from, to],
                    given(in_loop, turned, n, i),
                )?);
            }
            loops.push(uses);
        }
        faces.push(asm.sheet_face(Surface::Plane(Plane::new(plane)), &loops)?);
    }
    Ok(Made {
        body: asm.finish()?,
        faces: SheetFaces { faces },
    })
}
