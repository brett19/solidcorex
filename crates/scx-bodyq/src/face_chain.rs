// Written for solidcorex. Apache-2.0.
//! [`tangent_face_chain_ordered`]: the faces a pick runs on into, across edges where the
//! faces meet within an angle all along (research 74 §52, the pin's
//! `tangent::tangent_face_chain_ordered`).
//!
//! The picked faces, each once, in the order given; then every face across a smooth edge of a
//! face already in the chain, in the order the chain reaches it, and so on. An edge is smooth
//! when [`tangent_all_along`] proves its faces' normals within the caller's angle at every
//! point of it. The angle is the one tolerance and the caller's: the pin's walk was handed the
//! shaded view's half a degree and chose none.
//!
//! **An edge the proof cannot settle joins nothing and is named.** A bound that straddles the
//! angle ([`Doubt::Straddles`]) and an edge between faces the proof does not read yet
//! ([`Doubt::Unsupported`]: a NURBS face, or three faces at an edge) are listed in
//! [`FaceChain::undecided`] once each. An edge that is not between faces (one fin, or collapsed)
//! is not one the walk can cross and is not listed. The pin returned the kernel's refusal for a
//! NURBS face and lost the whole chain; here the chain stops short of that edge and says so.
//!
//! **Deterministic.** A face's edges are visited in id order, each edge's faces in fin order,
//! and each edge is decided once, whichever face reaches it first, under the one budget.

use std::collections::{BTreeMap, BTreeSet};

use scx_brep::{Brep, EdgeId, Entity, FaceId};
use scx_num::budget::{Budget, Stop};
use scx_num::interval::Ivl;

use crate::along::angle_claim;
use crate::error::{BodyqError, BodyqRefusal, BodyqUnsupported};
use crate::tangent::{Tangency, tangent_all_along};

/// Why an edge could not be settled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Doubt {
    /// The normals' angle straddles the asked angle at `at` (to rounding) and nothing
    /// elsewhere refuted it.
    Straddles {
        /// The edge curve's parameter where it could not be told.
        at: f64,
        /// The angle's bracket there.
        angle: Ivl,
    },
    /// The proof does not read this edge yet.
    Unsupported(BodyqUnsupported),
}

/// An edge of the chain's faces that the walk could neither cross nor rule out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Undecided {
    /// The edge.
    pub edge: EdgeId,
    /// Why.
    pub why: Doubt,
}

/// The faces of a tangent chain.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FaceChain {
    /// The picks (each once, in order), then the faces reached, in the order reached.
    pub faces: Vec<FaceId>,
    /// The edges met that were neither smooth nor proved not, in the order met.
    pub undecided: Vec<Undecided>,
}

/// `faces` of `body` and every face across a smooth edge from one of them, as the
/// [module](self) says, at the caller's `angle` (radians, finite, from 0 to below pi).
///
/// Each edge charges its proof to `budget`; `stop` is polled by it.
///
/// # Errors
///
/// Refused for a face the body does not have and an angle that is not finite, negative or not
/// below pi ([`Tolerance`](BodyqRefusal::Tolerance)); Exhausted or Cancelled.
pub fn tangent_face_chain_ordered(
    body: &Brep,
    faces: &[FaceId],
    angle: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<FaceChain, BodyqError> {
    angle_claim(angle)?;
    let mut out = FaceChain::default();
    let mut seen: BTreeSet<FaceId> = BTreeSet::new();
    for &f in faces {
        if body.face(f).is_none() {
            return Err(BodyqError::Refused(BodyqRefusal::NoEntity {
                entity: Entity::Face(f),
            }));
        }
        if seen.insert(f) {
            out.faces.push(f);
        }
    }
    // Each edge's verdict: `Some(true)` smooth, `Some(false)` not or not between faces,
    // `None` listed as undecided.
    let mut known: BTreeMap<EdgeId, bool> = BTreeMap::new();
    let mut next = 0;
    while let Some(&f) = out.faces.get(next) {
        next += 1;
        let Some(face) = body.face(f) else { continue };
        let edges: BTreeSet<EdgeId> = face.uses().map(|u| u.edge().id()).collect();
        for id in edges {
            stop.check()?;
            let smooth = if let Some(&s) = known.get(&id) {
                s
            } else {
                let s = settle(body, id, angle, budget, stop, &mut out.undecided)?;
                known.insert(id, s);
                s
            };
            if !smooth {
                continue;
            }
            let Some(edge) = body.edge(id) else { continue };
            for fin in edge.fins() {
                if seen.insert(fin.face) {
                    out.faces.push(fin.face);
                }
            }
        }
    }
    Ok(out)
}

/// Whether edge `id` is proved smooth; an edge the proof leaves open is added to `undecided`.
fn settle(
    body: &Brep,
    id: EdgeId,
    angle: f64,
    budget: &mut Budget,
    stop: &Stop,
    undecided: &mut Vec<Undecided>,
) -> Result<bool, BodyqError> {
    let mut doubt = |why| {
        undecided.push(Undecided { edge: id, why });
        Ok(false)
    };
    match tangent_all_along(body, id, angle, budget, stop) {
        Ok(Tangency::Tangent { .. }) => Ok(true),
        // proved not smooth, or no edge between faces to cross
        Ok(Tangency::NotTangent { .. })
        | Err(BodyqError::Refused(
            BodyqRefusal::OneSided { .. } | BodyqRefusal::Collapsed { .. },
        )) => Ok(false),
        Ok(Tangency::Undecided { at, angle }) => doubt(Doubt::Straddles { at, angle }),
        Err(BodyqError::Unsupported(u)) => doubt(Doubt::Unsupported(u)),
        Err(e) => Err(e),
    }
}
