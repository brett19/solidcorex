// Written for solidcorex. Apache-2.0.
//! The radial sweep (research 75 §8.2, task MK13): the hollow cone, the solid between an
//! outer cone and an inner one on one axis over one height. It is built directly: the
//! outer wall, the inner wall and an annular cap at each end the walls do not meet at, in
//! one draft with the shared edges made once. There is no stitch, no outward check and no
//! subtraction (the pin sweeps each support, stitches once, checks the result is outward
//! and subtracts the sweep from the body it contracts).

use scx_brep::{Document, FaceId};
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_surface::{Cone, Cylinder, Plane, Surface};

use crate::Made;
use crate::assembly::Assembly;
use crate::cylinder::rim;
use crate::error::{self, Dimension, MakeError, Refusal};

/// A hollow cone: radii at `z = 0` and `z = height` of the frame's axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadialCone {
    /// The axis and the plane `z = 0` of the base.
    pub frame: Frame,
    /// The distance along the axis from the base to the top.
    pub height: Length,
    /// The outer cone's radii at the base and at the top.
    pub outer: [Length; 2],
    /// The inner cone's radii at the base and at the top. At each end it is either smaller
    /// than the outer radius by more than the resolution (an annular cap closes the end) or
    /// exactly equal to it (a hinge: the walls meet at the rim and the end has no cap), and
    /// the two ends are not both hinges.
    pub inner: [Length; 2],
}

/// The faces of a [`radial`] by role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadialFaces {
    /// The outer wall, its normal away from the axis.
    pub outer: FaceId,
    /// The inner wall, its normal toward the axis.
    pub inner: FaceId,
    /// The annular cap on `z = 0`, its normal `-z`; none at a hinge.
    pub bottom: Option<FaceId>,
    /// The annular cap on `z = height`, its normal `+z`; none at a hinge.
    pub top: Option<FaceId>,
}

/// The wall on the cone of the radii `r` over the height `h`, a cylinder's when they are
/// equal.
fn wall(frame: &Frame, r: [f64; 2], h: f64, sense: Sense) -> Result<Surface, MakeError> {
    if r[0] == r[1] {
        Ok(Surface::Cylinder(Cylinder::new(
            *frame,
            Length::from_mm(r[0]),
            sense,
        )?))
    } else {
        // A reversed cone runs `v` along `-z`, so its radius grows by the slope per unit of
        // `-z`: the half-angle is negated to keep the radius `r[0]` at `z = 0` and `r[1]` at
        // `z = h`.
        let slope = (r[1] - r[0]) / h;
        let half_angle = match sense {
            Sense::Forward => fmath::atan(slope),
            Sense::Reversed => fmath::atan(-slope),
        };
        Ok(Surface::Cone(Cone::new(
            *frame,
            Length::from_mm(r[0]),
            half_angle,
            sense,
        )?))
    }
}

/// The hollow cone of `cone`, built with its topology known.
///
/// - Both ends capped: 4 vertices, 6 edges (the four rims and the two seam rulings) and 4
///   faces (the walls and two annuli).
/// - A hinge at one end: the two walls share that rim and its vertex, so 3 vertices, 5
///   edges and 3 faces, with no cap there.
///
/// Every pcurve is exact and the solid and outside regions are decided; the volume is the
/// difference of the two frusta.
///
/// # Errors
///
/// [`Refusal::Size`] for the height or a radius that is not finite and above the
/// document's resolution; [`Refusal::Annulus`] when the inner radius at an end is above the
/// outer one, or within the resolution of it without being equal, or both ends are hinges;
/// [`MakeError::Defect`] if the body is refused by `finish`; [`MakeError::Full`] if a table
/// runs out of ids.
pub fn radial(cone: &RadialCone, doc: &Document) -> Result<Made<RadialFaces>, MakeError> {
    let floor = doc.resolution().zone();
    let frame = &cone.frame;
    let h = error::size(Dimension::Height, cone.height, floor)?;
    let o = [
        error::size(Dimension::BaseRadius, cone.outer[0], floor)?,
        error::size(Dimension::TopRadius, cone.outer[1], floor)?,
    ];
    let i = [
        error::size(Dimension::InnerBaseRadius, cone.inner[0], floor)?,
        error::size(Dimension::InnerTopRadius, cone.inner[1], floor)?,
    ];
    let hinge = [i[0] == o[0], i[1] == o[1]];
    let open = |hinge: bool, o: f64, i: f64| hinge || o - i > floor.mm();
    if !(open(hinge[0], o[0], i[0]) && open(hinge[1], o[1], i[1]))
        || (hinge[0] && hinge[1])
        || i[0] > o[0]
        || i[1] > o[1]
    {
        return Err(MakeError::Refused(Refusal::Annulus { outer: o, inner: i }));
    }
    let outer_wall = wall(frame, o, h, Sense::Forward)?;
    let inner_wall = wall(frame, i, h, Sense::Reversed)?;
    let lifted = rim(frame, h, 1.0)?;
    let mut asm = Assembly::new(doc)?;
    // The outer rims, then the inner ones, shared with the outer at a hinge.
    let (outer_low, ov0) = asm.circle(*frame, o[0])?;
    let (outer_high, ov1) = asm.circle(lifted, o[1])?;
    let (inner_low, iv0) = if hinge[0] {
        (outer_low, ov0)
    } else {
        asm.circle(*frame, i[0])?
    };
    let (inner_high, iv1) = if hinge[1] {
        (outer_high, ov1)
    } else {
        asm.circle(lifted, i[1])?
    };
    let (outer_seam, o_up) = asm.line(ov0, ov1)?;
    let (inner_seam, i_up) = asm.line(iv0, iv1)?;
    let outer = asm.face(
        outer_wall,
        &[vec![
            (outer_low, Sense::Forward),
            (outer_seam, o_up),
            (outer_high, Sense::Reversed),
            (outer_seam, o_up.reversed()),
        ]],
    )?;
    // The inner wall's normal is toward the axis, so its loop runs the other way round.
    let inner = asm.face(
        inner_wall,
        &[vec![
            (inner_seam, i_up),
            (inner_high, Sense::Forward),
            (inner_seam, i_up.reversed()),
            (inner_low, Sense::Reversed),
        ]],
    )?;
    let bottom = if hinge[0] {
        None
    } else {
        Some(asm.face(
            Surface::Plane(Plane::new(rim(frame, 0.0, -1.0)?)),
            &[
                vec![(outer_low, Sense::Reversed)],
                vec![(inner_low, Sense::Forward)],
            ],
        )?)
    };
    let top = if hinge[1] {
        None
    } else {
        Some(asm.face(
            Surface::Plane(Plane::new(lifted)),
            &[
                vec![(outer_high, Sense::Forward)],
                vec![(inner_high, Sense::Reversed)],
            ],
        )?)
    };
    Ok(Made {
        body: asm.finish()?,
        faces: RadialFaces {
            outer,
            inner,
            bottom,
            top,
        },
    })
}
