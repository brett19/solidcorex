// Written for solidcorex. Apache-2.0.
//! The prism of several planar regions: each region extruded between the same two heights
//! as a solid of its own, all in one body (research 75 MK14; the pin's
//! `extrude_planar_regions`).
//!
//! - **One solid per region.** Each region has its own solid region and shell, so the body
//!   has as many solids as regions, side by side. Region `i` of the input is solid `i` of
//!   the record.
//! - **Regions that meet are refused, not merged.** The pin unites overlapping regions in
//!   the plane first and refuses ones that touch. There is no planar Boolean here yet, so
//!   regions that overlap, touch or cross are [`Unsupported::RegionsMeet`], naming the
//!   first pair by index. A region lying in another's hole, or beside it, is an island and
//!   is kept (the pin's `retains_islands`).
//! - **Built as [`extrude`](crate::extrude) builds.** The same writer, one solid at a time,
//!   so each solid is exactly what `extrude` makes of its region.

use scx_brep::Document;
use scx_num::frame::Frame;
use scx_num::units::Resolution;
use scx_planar::{Place, PlanarError, Region, Segment};

use crate::Made;
use crate::assembly::Assembly;
use crate::error::{MakeError, Refusal, Unsupported};
use crate::extrude::{ExtrudeFaces, Place as Placement, Span, extrude_into};

/// The faces of a prism, by region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrismFaces {
    /// The solid of each region, in the order the regions were given.
    pub solids: Vec<ExtrudeFaces>,
}

/// Extrudes each of `regions`, placed in `frame`'s `x, y` plane, along `frame`'s z axis from
/// `span.start` to `span.end`, as one body with a solid for each.
///
/// # Errors
///
/// - [`Refusal::NoRegions`] for none.
/// - Those of [`extrude`](crate::extrude) for any region and the span.
/// - [`Unsupported::RegionsMeet`] when two regions overlap, touch or cross.
/// - A [`Defect`](crate::Defect) when the body does not finish, or [`MakeError::Full`].
pub fn prism(
    regions: &[Region],
    frame: &Frame,
    span: Span,
    doc: &Document,
) -> Result<Made<PrismFaces>, MakeError> {
    let floor = doc.resolution().zone();
    let (start, end) = (span.start.mm(), span.end.mm());
    let apart = (end - start).abs();
    if !(start.is_finite() && end.is_finite() && apart.is_finite() && apart > floor.mm()) {
        return Err(MakeError::Refused(Refusal::Span { start, end, floor }));
    }
    check(regions, doc.resolution())?;
    let (lo, hi) = if start < end {
        (start, end)
    } else {
        (end, start)
    };
    let place = Placement::of(frame);
    let mut asm = Assembly::new(doc)?;
    let mut solids = Vec::with_capacity(regions.len());
    for (i, region) in regions.iter().enumerate() {
        if i > 0 {
            asm.begin_solid()?;
        }
        solids.push(extrude_into(
            &mut asm,
            region,
            &place,
            [lo, hi],
            start < end,
        )?);
    }
    Ok(Made {
        body: asm.finish()?,
        faces: PrismFaces { solids },
    })
}

/// The checks `prism` and [`sheet`](crate::sheet) share: some regions, each at the
/// document's resolution, none meeting another.
pub(crate) fn check(regions: &[Region], resolution: Resolution) -> Result<(), MakeError> {
    if regions.is_empty() {
        return Err(MakeError::Refused(Refusal::NoRegions));
    }
    for r in regions {
        if r.resolution() != resolution {
            return Err(MakeError::Refused(Refusal::Resolution {
                region: r.resolution(),
                document: resolution,
            }));
        }
    }
    for (i, a) in regions.iter().enumerate() {
        for (j, b) in regions.iter().enumerate().skip(i + 1) {
            if meet(a, b)? {
                return Err(MakeError::Unsupported(Unsupported::RegionsMeet {
                    first: i,
                    second: j,
                }));
            }
        }
    }
    Ok(())
}

/// Whether two regions of one resolution overlap, touch or cross.
///
/// Their boundaries are tested together as one probe region (`a`'s outer loop with every
/// other loop as a hole), whose check of every pair of segments within the resolution of
/// each other names a crossing or a touch before it looks at nesting. If the boundaries
/// are apart, the regions are either side by side, one inside the other's hole, or one in
/// the other's material; a vertex of each decides which.
fn meet(a: &Region, b: &Region) -> Result<bool, MakeError> {
    let segments = |l: &scx_planar::Loop| -> Vec<Segment> { l.segments().to_vec() };
    let holes: Vec<Vec<Segment>> = a
        .holes()
        .iter()
        .chain(core::iter::once(b.outer()))
        .chain(b.holes())
        .map(segments)
        .collect();
    match Region::new(segments(a.outer()), holes, a.resolution()) {
        Err(PlanarError::Crossing { .. } | PlanarError::Touching { .. }) => return Ok(true),
        Err(PlanarError::HoleOutside { .. } | PlanarError::HoleInHole { .. }) | Ok(_) => {}
        Err(e) => return Err(MakeError::Unsupported(Unsupported::Profile(e))),
    }
    for (x, y) in [(a, b), (b, a)] {
        let Some(s) = y.outer().segments().first() else {
            continue;
        };
        match x
            .classify(s.start())
            .map_err(|e| MakeError::Unsupported(Unsupported::Profile(e)))?
        {
            Place::Outside => {}
            Place::Inside | Place::Boundary => return Ok(true),
        }
    }
    Ok(false)
}
