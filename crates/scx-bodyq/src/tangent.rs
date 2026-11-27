// Written for solidcorex. Apache-2.0.
//! [`tangent_all_along`]: whether the two faces across an edge meet within an angle at
//! every point of it (research 74 §25).

use scx_brep::{Brep, EdgeId};
use scx_num::budget::{Budget, Stop};
use scx_num::interval::Ivl;

use crate::along::{Edge, Faces, Proof, angle_claim};
use crate::error::BodyqError;

/// Whether an edge's faces meet within an angle all along it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tangency {
    /// Proved at every point: the normals' angle is below the asked angle everywhere, and
    /// below `bound` (an upper bound over the edge, from the enclosures that proved it).
    Tangent {
        /// An upper bound on the angle between the normals over the edge, radians.
        bound: f64,
    },
    /// Refuted: at the parameter `at` the angle is certainly above the asked angle.
    NotTangent {
        /// The edge curve's parameter of the witness.
        at: f64,
        /// The angle's bracket there, radians.
        angle: Ivl,
    },
    /// Neither: at `at` the angle's bracket straddles the asked angle (or the faces have no
    /// normal there to compare), and nothing elsewhere refuted it.
    Undecided {
        /// The edge curve's parameter where it could not be told.
        at: f64,
        /// The angle's bracket there (whole where there is no normal).
        angle: Ivl,
    },
}

/// Whether the oriented normals of the two faces across `edge` make an angle below
/// `max_angle` (radians, in `[0, pi)`) at every point of its curve.
///
/// A proof, not a sample: `Tangent` comes from Taylor-model enclosures of the normals over
/// pieces of the curve that cover it; `NotTangent` from a certified bracket at one
/// parameter. Where neither can be had (the angle is `max_angle` itself, to rounding) the
/// answer is `Undecided` at the worst such place. The pieces are split as the proof needs,
/// each enclosure and each bracket charging one unit to `budget`: one budget for the edge,
/// with no cap on depth of its own.
///
/// The normals are each face's surface normal at the foot of the curve's point along that
/// surface's normal field (a closed form on analytic surfaces; no pcurve is read), signed
/// to the faces' orientation across the edge, so a smooth edge's angle is 0 and a box edge's
/// is pi / 2. A seam, or any edge with one analytic surface on both sides oriented alike,
/// is `Tangent` with bound 0; a NURBS seam waits with every NURBS face.
///
/// # Errors
///
/// Refused for an edge the body does not have, a collapsed edge, an edge with fewer than
/// two faces ([`OneSided`](crate::BodyqRefusal::OneSided)), or an angle not finite,
/// negative or not below pi ([`Tolerance`](crate::BodyqRefusal::Tolerance)). Unsupported
/// for more than two faces, or a NURBS face. Exhausted or cancelled by `budget` and `stop`.
pub fn tangent_all_along(
    body: &Brep,
    edge: EdgeId,
    max_angle: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Tangency, BodyqError> {
    let claim = angle_claim(max_angle)?;
    let e = Edge::of(body, edge)?;
    let Faces::Two(f) = &e.faces else {
        return Ok(Tangency::Tangent { bound: 0.0 });
    };
    Ok(match claim.along(&e, f, budget, stop)? {
        Proof::Holds(r) => Tangency::Tangent { bound: r.hi },
        Proof::Fails(at, angle) => Tangency::NotTangent { at, angle },
        Proof::Open(at, angle) => Tangency::Undecided { at, angle },
    })
}
