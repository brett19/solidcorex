// Written for solidcorex. Apache-2.0.
//! The cone: a frustum on the frame's z axis from the base radius at `z = 0` to the top
//! radius at its height, or a cone to an apex when one radius is zero.

use scx_brep::{Document, FaceId};
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_surface::{Cone, Cylinder, Plane, Surface};

use super::Made;
use super::assembly::Assembly;
use super::cylinder::{rim, walled};
use super::error::{self, Dimension, MakeError, Refusal};

/// The faces of a [`cone`] by role. An end with a zero radius is an apex and has no cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConeFaces {
    /// The cap on `z = 0`, its normal `-z`; none when the base radius is zero.
    pub bottom: Option<FaceId>,
    /// The cap on `z = height`, its normal `+z`; none when the top radius is zero.
    pub top: Option<FaceId>,
    /// The conical wall, its normal away from the axis (a cylinder's when the radii are
    /// equal).
    pub wall: FaceId,
}

/// The cone about `frame`'s z axis from `radii[0]` at `z = 0` to `radii[1]` at
/// `z = height`, built with its topology known.
///
/// - Both radii above zero: a frustum of 2 vertices, 3 edges (the rims and the seam ruling)
///   and 3 faces, as [`cylinder`](super::cylinder) is; equal radii give a cylinder's wall.
/// - One radius exactly zero: a cone to its apex, of 2 vertices (the rim's and the apex),
///   3 edges (the rim, the seam ruling and the apex's collapsed edge) and 2 faces.
///
/// Every pcurve is exact and the solid and outside regions are decided.
///
/// # Errors
///
/// [`Refusal::Size`] for the height, or a radius that is not exactly zero and not finite
/// and above the document's resolution; [`Refusal::ZeroRadii`] when both are zero;
/// [`MakeError::Defect`] if the body the constructor builds is refused by `finish`;
/// [`MakeError::Full`] if a table runs out of ids.
pub fn cone(
    frame: &Frame,
    radii: [Length; 2],
    height: Length,
    doc: &Document,
) -> Result<Made<ConeFaces>, MakeError> {
    let floor = doc.resolution().zone();
    // A radius is zero exactly (an apex) or a size; zero is the exception the type allows.
    let radius = |dimension, r: Length| {
        if r.mm() == 0.0 {
            Ok(0.0)
        } else {
            error::size(dimension, r, floor)
        }
    };
    let r0 = radius(Dimension::BaseRadius, radii[0])?;
    let r1 = radius(Dimension::TopRadius, radii[1])?;
    let h = error::size(Dimension::Height, height, floor)?;
    if r0 == 0.0 && r1 == 0.0 {
        return Err(MakeError::Refused(Refusal::ZeroRadii));
    }
    let wall = if r0 == r1 {
        Surface::Cylinder(Cylinder::new(*frame, Length::from_mm(r0), Sense::Forward)?)
    } else {
        // The pin's signed half-angle: negative when the cone narrows along z.
        let half_angle = fmath::atan((r1 - r0) / h);
        Surface::Cone(Cone::new(
            *frame,
            Length::from_mm(r0),
            half_angle,
            Sense::Forward,
        )?)
    };
    let mut asm = Assembly::new(doc)?;
    let faces = if r1 == 0.0 {
        // The apex on top: along the rim, up the seam, round the apex backwards in u (the
        // face lies below it), down the seam.
        let (base, b) = asm.circle(*frame, r0)?;
        let apex = asm.vertex(frame.origin() + *frame.z() * h)?;
        let (seam, up) = asm.line(b, apex)?;
        let pole = asm.collapsed(apex)?;
        let wall = asm.face(
            wall,
            &[vec![
                (base, Sense::Forward),
                (seam, up),
                (pole, Sense::Forward),
                (seam, up.reversed()),
            ]],
        )?;
        let bottom = asm.face(
            Surface::Plane(Plane::new(rim(frame, 0.0, -1.0)?)),
            &[vec![(base, Sense::Reversed)]],
        )?;
        ConeFaces {
            bottom: Some(bottom),
            top: None,
            wall,
        }
    } else if r0 == 0.0 {
        // The apex at the base: back along the top rim, down the seam, round the apex
        // forwards in u (the face lies above it), up the seam.
        let apex = asm.vertex(frame.origin())?;
        let lifted = rim(frame, h, 1.0)?;
        let (high, t) = asm.circle(lifted, r1)?;
        let (seam, up) = asm.line(apex, t)?;
        let pole = asm.collapsed(apex)?;
        let wall = asm.face(
            wall,
            &[vec![
                (high, Sense::Reversed),
                (seam, up.reversed()),
                (pole, Sense::Forward),
                (seam, up),
            ]],
        )?;
        let top = asm.face(
            Surface::Plane(Plane::new(lifted)),
            &[vec![(high, Sense::Forward)]],
        )?;
        ConeFaces {
            bottom: None,
            top: Some(top),
            wall,
        }
    } else {
        let [bottom, top, wall] = walled(&mut asm, frame, wall, [r0, r1], h)?;
        ConeFaces {
            bottom: Some(bottom),
            top: Some(top),
            wall,
        }
    };
    Ok(Made {
        body: asm.finish()?,
        faces,
    })
}
