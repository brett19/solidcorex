// Written for solidcorex. Apache-2.0.
//! The cylinder: a round bar on the frame's z axis from `z = 0` to its height. Its wall is
//! one face whose loop runs round the seam ruling at the frame's x axis; the frustum of a
//! cone (both radii above zero) is written the same way.

use scx_brep::{Document, FaceId};
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_surface::{Cylinder, Plane, Surface};

use crate::Made;
use crate::assembly::Assembly;
use crate::error::{self, Defect, Dimension, MakeError};
use scx_num::frame::Sense;

/// The faces of a [`cylinder`] by role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CylinderFaces {
    /// The cap on `z = 0`, its normal `-z`.
    pub bottom: FaceId,
    /// The cap on `z = height`, its normal `+z`.
    pub top: FaceId,
    /// The round wall, its normal away from the axis.
    pub wall: FaceId,
}

/// The cylinder of `radius` about `frame`'s z axis from `z = 0` to `z = height`, built with
/// its topology known: 2 vertices on the frame's x axis side, 3 edges (the two rim circles
/// and the seam ruling between their vertices) and 3 faces (the wall and two planar caps),
/// with exact pcurves and the solid and outside regions decided.
///
/// # Errors
///
/// [`Refusal::Size`](crate::Refusal::Size) for a size that is not finite and above the
/// document's resolution; [`MakeError::Defect`] if the body the constructor builds is
/// refused by `finish`; [`MakeError::Full`] if a table runs out of ids.
pub fn cylinder(
    frame: &Frame,
    radius: Length,
    height: Length,
    doc: &Document,
) -> Result<Made<CylinderFaces>, MakeError> {
    let floor = doc.resolution().zone();
    let r = error::size(Dimension::Radius, radius, floor)?;
    let h = error::size(Dimension::Height, height, floor)?;
    let wall = Surface::Cylinder(Cylinder::new(*frame, Length::from_mm(r), Sense::Forward)?);
    let mut asm = Assembly::new(doc)?;
    let [bottom, top, wall] = walled(&mut asm, frame, wall, [r, r], h)?;
    Ok(Made {
        body: asm.finish()?,
        faces: CylinderFaces { bottom, top, wall },
    })
}

/// Writes the wall on `surface` between the rims of `radii` at `z = 0` and `z = h`, and the
/// two caps: the faces bottom, top, wall.
///
/// The wall's loop runs along the bottom rim, up the seam, back along the top rim and down
/// the seam, counter-clockwise in uv over one period, so its front is the outside.
pub(crate) fn walled(
    asm: &mut Assembly<'_>,
    frame: &Frame,
    surface: Surface,
    radii: [f64; 2],
    h: f64,
) -> Result<[FaceId; 3], MakeError> {
    let (low, b) = asm.circle(*frame, radii[0])?;
    let lifted = rim(frame, h, 1.0)?;
    let (high, t) = asm.circle(lifted, radii[1])?;
    let (seam, up) = asm.line(b, t)?;
    let wall = asm.face(
        surface,
        &[vec![
            (low, Sense::Forward),
            (seam, up),
            (high, Sense::Reversed),
            (seam, up.reversed()),
        ]],
    )?;
    let bottom = asm.face(
        Surface::Plane(Plane::new(rim(frame, 0.0, -1.0)?)),
        &[vec![(low, Sense::Reversed)]],
    )?;
    let top = asm.face(
        Surface::Plane(Plane::new(lifted)),
        &[vec![(high, Sense::Forward)]],
    )?;
    Ok([bottom, top, wall])
}

/// The frame at height `z` on `frame`'s axis, its x the frame's and its z the frame's
/// times `side` (`1` or `-1`).
pub(crate) fn rim(frame: &Frame, z: f64, side: f64) -> Result<Frame, MakeError> {
    Frame::new(
        frame.origin() + *frame.z() * z,
        *frame.z() * side,
        *frame.x(),
    )
    .ok_or(MakeError::Defect(Defect::PlaneFrame))
}
