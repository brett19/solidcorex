// Written for solidcorex. Apache-2.0.
//! The block: a box with its corner on the frame's origin and its extents along the
//! frame's axes.

use std::collections::BTreeMap;

use scx_brep::{Document, FaceId, VertexId};
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector3d::{Axis, Point3, Vec3};
use scx_surface::{Plane, Surface};

use super::Made;
use super::assembly::Assembly;
use super::error::{self, Defect, Dimension, MakeError};

/// The faces of a [`block`] by role.
///
/// "Bottom" and "top" are the faces on the frame's `z = 0` and `z = size z`; the sides run
/// counter-clockwise about the frame's z axis, starting at the face on `y = 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockFaces {
    /// The face on `z = 0`, its normal `-z`.
    pub bottom: FaceId,
    /// The face on `z = size z`, its normal `+z`.
    pub top: FaceId,
    /// The faces on `y = 0` (normal `-y`), `x = size x` (`+x`), `y = size y` (`+y`) and
    /// `x = 0` (`-x`).
    pub sides: [FaceId; 4],
}

/// One face of the block: the corners of its ring, each written as the binary literal `0bxyz` (a bit
/// set when the corner is at the far end of that axis), running counter-clockwise about its outward normal; and that normal as
/// an axis and a sign.
struct Role {
    ring: [u8; 4],
    axis: Axis,
    outward: f64,
}

/// In the order of [`BlockFaces`]: bottom, top, then the four sides.
const ROLES: [Role; 6] = [
    Role {
        ring: [0b000, 0b010, 0b110, 0b100],
        axis: Axis::Z,
        outward: -1.0,
    },
    Role {
        ring: [0b001, 0b101, 0b111, 0b011],
        axis: Axis::Z,
        outward: 1.0,
    },
    Role {
        ring: [0b000, 0b100, 0b101, 0b001],
        axis: Axis::Y,
        outward: -1.0,
    },
    Role {
        ring: [0b100, 0b110, 0b111, 0b101],
        axis: Axis::X,
        outward: 1.0,
    },
    Role {
        ring: [0b110, 0b010, 0b011, 0b111],
        axis: Axis::Y,
        outward: 1.0,
    },
    Role {
        ring: [0b010, 0b000, 0b001, 0b011],
        axis: Axis::X,
        outward: -1.0,
    },
];

/// The block at `frame`: the box `[0, size x] x [0, size y] x [0, size z]` in the frame's
/// coordinates, built with its topology known: 8 vertices, 12 edges and 6 planar faces, each
/// edge made once, with exact pcurves, and the solid and outside regions decided.
///
/// # Errors
///
/// [`Refusal::Size`](super::Refusal::Size) for a size that is not finite and above the document's resolution;
/// [`MakeError::Defect`] if the body the constructor builds is refused by `finish`;
/// [`MakeError::Full`] if a table runs out of ids.
pub fn block(
    frame: &Frame,
    size: [Length; 3],
    doc: &Document,
) -> Result<Made<BlockFaces>, MakeError> {
    let floor = doc.resolution().zone();
    let [dx, dy, dz] = [
        error::size(Dimension::X, size[0], floor)?,
        error::size(Dimension::Y, size[1], floor)?,
        error::size(Dimension::Z, size[2], floor)?,
    ];
    let (ax, ay, az): (Vec3, Vec3, Vec3) = (*frame.x(), *frame.y(), *frame.z());
    // A corner is placed once, as the sum of its offsets along the axes added to the origin.
    let point = |bits: u8| -> Point3 {
        let bit = |b: u8| f64::from(bits >> b & 1);
        frame.origin() + (ax * (bit(2) * dx) + ay * (bit(1) * dy) + az * (bit(0) * dz))
    };
    let mut asm = Assembly::new(doc)?;
    let mut vertices: BTreeMap<u8, VertexId> = BTreeMap::new();
    let mut faces = [None; 6];
    for (slot, role) in faces.iter_mut().zip(&ROLES) {
        let mut ring = Vec::with_capacity(4);
        for bits in role.ring {
            let v = if let Some(&v) = vertices.get(&bits) {
                v
            } else {
                let v = asm.vertex(point(bits))?;
                vertices.insert(bits, v);
                v
            };
            ring.push(v);
        }
        let mut uses = Vec::with_capacity(4);
        for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
            uses.push(asm.line(a, b)?);
        }
        let [p0, p1, ..] = role.ring.map(point);
        let normal = match role.axis {
            Axis::X => ax,
            Axis::Y => ay,
            Axis::Z => az,
        } * role.outward;
        let plane = Frame::new(p0, normal, p1 - p0).ok_or(MakeError::Defect(Defect::PlaneFrame))?;
        *slot = Some(asm.face(Surface::Plane(Plane::new(plane)), &[uses])?);
    }
    let [
        Some(bottom),
        Some(top),
        Some(s0),
        Some(s1),
        Some(s2),
        Some(s3),
    ] = faces
    else {
        return Err(MakeError::Defect(Defect::PlaneFrame));
    };
    Ok(Made {
        body: asm.finish()?,
        faces: BlockFaces {
            bottom,
            top,
            sides: [s0, s1, s2, s3],
        },
    })
}
