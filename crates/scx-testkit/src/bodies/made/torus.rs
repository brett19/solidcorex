// Written for solidcorex. Apache-2.0.
//! The torus: one face about the frame's z axis, its two seams the outer equator and the
//! meridian through the frame's x axis, meeting at one vertex.

use std::f64::consts::{PI, TAU};

use scx_brep::{Document, FaceId};
use scx_curve::{Circle, Curve};
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_surface::{Surface, Torus};

use super::Made;
use super::assembly::Assembly;
use super::error::{self, Defect, Dimension, MakeError, Refusal};

/// The face of a [`torus`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TorusFaces {
    /// The one face, its normal outward.
    pub face: FaceId,
}

/// The ring torus whose tube of radius `minor` circles `frame`'s z axis at `major` in the
/// frame's `z = 0` plane, built with its topology known: 1 vertex (on the frame's x axis,
/// at `major + minor`), 2 closed edges (the outer equator and the meridian through the
/// vertex) and 1 face, with exact pcurves and the solid and outside regions decided.
///
/// Radii within the resolution of each other make a horn torus, whose tube meets the axis:
/// 1 vertex (the pole at the frame's origin), 3 edges (the meridian through the pole and
/// the face's two collapsed edges) and 1 face.
///
/// # Errors
///
/// - [`Refusal::Size`] for a radius that is not finite and above the document's
///   resolution;
/// - [`Refusal::SelfIntersecting`] when `minor` exceeds `major` by more than the
///   resolution (a spindle torus);
/// - [`MakeError::Defect`] if the body the constructor builds is refused by `finish`;
///   [`MakeError::Full`] if a table runs out of ids.
pub fn torus(
    frame: &Frame,
    major: Length,
    minor: Length,
    doc: &Document,
) -> Result<Made<TorusFaces>, MakeError> {
    let floor = doc.resolution().zone();
    let big = error::size(Dimension::Major, major, floor)?;
    let small = error::size(Dimension::Minor, minor, floor)?;
    if small > big + floor.mm() {
        return Err(MakeError::Refused(Refusal::SelfIntersecting {
            major: big,
            minor: small,
        }));
    }
    // Within the resolution of each other the radii are one: the tube meets the axis.
    if (small - big).abs() <= floor.mm() {
        return horn(frame, 0.5 * (big + small), *doc);
    }
    let surface = Surface::Torus(Torus::new(
        *frame,
        Length::from_mm(big),
        Length::from_mm(small),
        Sense::Forward,
    )?);
    let mut asm = Assembly::new(doc)?;
    // The outer equator (v = 0), from the vertex counter-clockwise about z.
    let (equator, v) = asm.circle(*frame, big + small)?;
    // The meridian (u = 0) about the tube's centre on the x axis, from the vertex upward:
    // its x is the frame's and its z is -y, so its y is the frame's z.
    let (x, y) = (*frame.x(), *frame.y());
    let plane =
        Frame::new(frame.origin() + x * big, -y, x).ok_or(MakeError::Defect(Defect::PlaneFrame))?;
    let meridian = asm.edge(
        Curve::Circle(Circle::new(plane, Length::from_mm(small), Sense::Forward)?),
        Extent1d::new(0.0, TAU).ok_or(MakeError::Defect(Defect::Interval))?,
        [v, v],
    )?;
    // Along the equator, up the meridian a period on in u, back along the equator a period
    // on in v, down the meridian: counter-clockwise over one period each way.
    let face = asm.face(
        surface,
        &[vec![
            (equator, Sense::Forward),
            (meridian, Sense::Forward),
            (equator, Sense::Reversed),
            (meridian, Sense::Reversed),
        ]],
    )?;
    Ok(Made {
        body: asm.finish()?,
        faces: TorusFaces { face },
    })
}

/// The horn torus of `radius`: its tube meets the axis at the frame's origin. Its one vertex
/// is that pole, so the face covers `v` over `[1/2, 3/2]` and its loop runs the pole's
/// collapsed edge, the meridian (a circle through the pole, closed there), the collapsed
/// edge at the far end and the meridian down: 1 vertex, 3 edges, 1 face.
fn horn(frame: &Frame, radius: f64, doc: Document) -> Result<Made<TorusFaces>, MakeError> {
    let surface = Surface::Torus(Torus::horn(
        *frame,
        Length::from_mm(radius),
        Sense::Forward,
    )?);
    let mut asm = Assembly::new(&doc)?;
    let pole = asm.vertex(frame.origin())?;
    // The meridian about the tube's centre on the x axis, from the pole (angle pi) once
    // round: as the ring torus's, its x is the frame's and its z is -y.
    let (x, y) = (*frame.x(), *frame.y());
    let plane = Frame::new(frame.origin() + x * radius, -y, x)
        .ok_or(MakeError::Defect(Defect::PlaneFrame))?;
    let meridian = asm.edge(
        Curve::Circle(Circle::new(plane, Length::from_mm(radius), Sense::Forward)?),
        Extent1d::new(PI, PI + TAU).ok_or(MakeError::Defect(Defect::Interval))?,
        [pole, pole],
    )?;
    // The face's own collapsed edge at each end of its v range, as a revolve's whole circle
    // has (the inner equator is both v = 1/2 and v = 3/2).
    let (below, above) = (asm.collapsed(pole)?, asm.collapsed(pole)?);
    // Counter-clockwise over one period each way: along the pole (v = 1/2), up the
    // meridian a period on in u, back along the pole (v = 3/2), down the meridian.
    let face = asm.face(
        surface,
        &[vec![
            (below, Sense::Forward),
            (meridian, Sense::Forward),
            (above, Sense::Forward),
            (meridian, Sense::Reversed),
        ]],
    )?;
    Ok(Made {
        body: asm.finish()?,
        faces: TorusFaces { face },
    })
}
