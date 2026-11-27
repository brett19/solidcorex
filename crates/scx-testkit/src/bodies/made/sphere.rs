// Written for solidcorex. Apache-2.0.
//! The sphere: one face about the frame's origin, its seam the meridian through the frame's
//! x axis and its poles on the frame's z axis.

use std::f64::consts::PI;

use scx_brep::{Document, FaceId};
use scx_curve::{Circle, Curve};
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_surface::{Sphere, Surface};

use super::Made;
use super::assembly::Assembly;
use super::error::{self, Defect, Dimension, MakeError};

/// The face of a [`sphere`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SphereFaces {
    /// The one face, its normal outward.
    pub face: FaceId,
}

/// The sphere of `radius` about `frame`'s origin, built with its topology known: 2 vertices
/// (the poles on the frame's z axis), 3 edges (the seam meridian through the frame's x axis
/// and the two poles' collapsed edges) and 1 face, with exact pcurves and the solid and
/// outside regions decided.
///
/// # Errors
///
/// [`Refusal::Size`](super::Refusal::Size) for a radius that is not finite and above the
/// document's resolution; [`MakeError::Defect`] if the body the constructor builds is
/// refused by `finish`; [`MakeError::Full`] if a table runs out of ids.
pub fn sphere(
    frame: &Frame,
    radius: Length,
    doc: &Document,
) -> Result<Made<SphereFaces>, MakeError> {
    let r = error::size(Dimension::Radius, radius, doc.resolution().zone())?;
    let surface = Surface::Sphere(Sphere::new(*frame, Length::from_mm(r), Sense::Forward)?);
    let (o, z) = (frame.origin(), *frame.z());
    let mut asm = Assembly::new(doc)?;
    let south = asm.vertex(o - z * r)?;
    let north = asm.vertex(o + z * r)?;
    // The half circle from the south pole (t = 0) through the frame's x axis (t = pi/2) to
    // the north (t = pi): its x is -z and its y the frame's x, so its z is -y.
    let plane = Frame::new(o, -*frame.y(), -z).ok_or(MakeError::Defect(Defect::PlaneFrame))?;
    let meridian = asm.edge(
        Curve::Circle(Circle::new(plane, Length::from_mm(r), Sense::Forward)?),
        Extent1d::new(0.0, PI).ok_or(MakeError::Defect(Defect::Interval))?,
        [south, north],
    )?;
    let (sp, np) = (asm.collapsed(south)?, asm.collapsed(north)?);
    // Down the seam, forwards round the south pole (the face lies above it), up the seam
    // a period on, backwards round the north pole: counter-clockwise in uv, over u in
    // [0, period].
    let face = asm.face(
        surface,
        &[vec![
            (meridian, Sense::Reversed),
            (sp, Sense::Forward),
            (meridian, Sense::Forward),
            (np, Sense::Forward),
        ]],
    )?;
    Ok(Made {
        body: asm.finish()?,
        faces: SphereFaces { face },
    })
}
