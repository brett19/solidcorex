// Written for solidcorex. Apache-2.0.
//! The torus: one face about the frame's z axis, its two seams the outer equator and the
//! meridian through the frame's x axis, meeting at one vertex.

use std::f64::consts::TAU;

use scx_brep::{Document, FaceId};
use scx_curve::{Circle, Curve};
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_surface::{Surface, Torus};

use crate::Made;
use crate::assembly::Assembly;
use crate::error::{self, Defect, Dimension, MakeError, Refusal, Unsupported};

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
/// # Errors
///
/// - [`Refusal::Size`] for a radius that is not finite and above the document's
///   resolution;
/// - [`Unsupported::HornTorus`] when the radii are equal within the resolution (72 D3);
/// - [`Refusal::SelfIntersecting`] when `minor` exceeds `major` by more (a spindle torus);
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
    // Within the resolution of each other the radii are one: the tube meets the axis.
    if (small - big).abs() <= floor.mm() {
        return Err(MakeError::Unsupported(Unsupported::HornTorus {
            major: big,
            minor: small,
        }));
    }
    if small > big {
        return Err(MakeError::Refused(Refusal::SelfIntersecting {
            major: big,
            minor: small,
        }));
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
