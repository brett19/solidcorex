// Written for solidcorex. Apache-2.0.
//! [`boolean_given`]: the Boolean with incidences the caller states (research 75 §2.6, row
//! BO21), each checked, the true ones used to skip their curve-surface roots.
//!
//! A caller often knows where its operands meet: a boss is sketched on a face, a pocket's
//! floor is a face of the stock moved down, two parts are mated face to face. It states that
//! as [`Fact`]s in [`Incidences`]. Each fact is a claim the Boolean's own tests decide, so it
//! is checked against them, never taken on trust:
//!
//! - [`Fact::EdgeOnFace`]: the edge lies on the face's surface, within coincidence, along its
//!   whole length. Stage 2's edge-face test reads it through
//!   [`scx_surface::curve_on_surface`]: on a plane, a closed-form bound on the edge's height
//!   above it proves it with no root isolated; where the bound does not (or the surface is
//!   curved, whose roots are as cheap as any bound), the roots are isolated as without the
//!   fact, and a fact they show false is [`BooleanRefusal::FalseFact`].
//! - [`Fact::FaceOnFace`]: the faces lie on one surface and overlap over an area. Stage 4
//!   decides it as without the fact (the pair's closed form is a few operations); a pair it
//!   does not find coincident with an overlap of some width is
//!   [`BooleanRefusal::FalseFact`]. What it saves is in stage 2: on one surface, every
//!   edge of either face lies on the other's surface, so each such edge-face test is tried
//!   by the bound first. Those edge facts are implied, not stated: one the bound does not
//!   prove is answered by the roots as without any fact, and its answer is never a refusal.
//!
//! **The same result.** A fact only ever replaces [`scx_surface::intersect_curve`] by
//! [`scx_surface::curve_on_surface`] for one edge and face. That gives the same answer to
//! the bit: proved, every event the roots could give is near, so the answer is the
//! overlap's clip, the very function `intersect_curve` returns then; not proved, it is
//! `intersect_curve`. Everything else runs as in [`boolean`](crate::boolean). So with true
//! facts the body and changes are [`boolean`](crate::boolean)'s; only the work charged is
//! less (the bound's one unit against two root isolations per piece), and a budget too
//! small for `boolean` may suffice here.
//!
//! **A false fact changes nothing silently.** A stated fact is false exactly when the test
//! it stands for disagrees: the roots find the edge leaving the surface, or stage 4 finds
//! no coincident overlap. Proved facts are true by the bound, so they are never the false
//! ones; every other fact is judged by the same test that answers without it. A fact on an
//! edge or face its operand does not have, on an edge with no curve, or on a third
//! operand, is false too. The check of a stated edge fact whose edge and face stage 2 never
//! pairs (their boxes apart, or the face in no pair) runs the same test on its own.

use core::fmt;
use std::collections::BTreeMap;

use scx_brep::op::{Cx, Done};
use scx_brep::{Brep, EdgeId, EdgeShape, FaceId, Operand};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::units::{Length, Resolution};
use scx_surface::{OnSurface, SurfaceError, curve_on_surface};

use crate::boolean::{BooleanOp, Interference, NoItems, run};
use crate::error::{BooleanError, BooleanRefusal, BooleanUnsupported};
use crate::faceface::FaceFace;
use crate::geom::{Operands, Side, recorded};

/// An incidence of the two operands a caller states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Fact {
    /// Face `a` of operand 0 and face `b` of operand 1 lie on one surface, within
    /// coincidence, and overlap over an area.
    FaceOnFace {
        /// Operand 0's face.
        a: FaceId,
        /// Operand 1's face.
        b: FaceId,
    },
    /// The edge of `operand` lies on the surface of `face`, a face of the other operand,
    /// within coincidence along its whole length. The face's trim is not stated: an edge on
    /// the surface outside the face is true and meets nothing.
    EdgeOnFace {
        /// The edge's operand: 0 or 1.
        operand: Operand,
        /// The edge.
        edge: EdgeId,
        /// The other operand's face.
        face: FaceId,
    },
}

impl fmt::Display for Fact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::FaceOnFace { a, b } => {
                write!(f, "operand 0's {a} on operand 1's {b}")
            }
            Self::EdgeOnFace {
                operand,
                edge,
                face,
            } => write!(f, "{operand}'s {edge} on the other operand's {face}"),
        }
    }
}

/// The facts a caller states for [`boolean_given`], each once, in order.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Incidences(Vec<Fact>);

impl Incidences {
    /// No facts: [`boolean_given`] is then [`boolean`](crate::boolean).
    pub const NONE: Self = Self(Vec::new());

    /// The facts, sorted, each once.
    #[must_use]
    pub fn facts(&self) -> &[Fact] {
        &self.0
    }
}

impl FromIterator<Fact> for Incidences {
    fn from_iter<I: IntoIterator<Item = Fact>>(iter: I) -> Self {
        let mut facts: Vec<Fact> = iter.into_iter().collect();
        facts.sort_unstable();
        facts.dedup();
        Self(facts)
    }
}

/// The Boolean `op` of `a` and `b` as [`boolean`](crate::boolean), with `facts` stated:
/// the same body and changes when every fact is true, in less work where they hold.
///
/// See the [module](self) for how each fact is checked and used.
///
/// # Errors
///
/// [`BooleanRefusal::FalseFact`] for the first fact (in their order) found false: an edge
/// the roots find leaving the face's surface, a face pair stage 4 does not find coincident
/// over an area, or a fact naming an entity its operand does not have. Otherwise as
/// [`boolean`](crate::boolean); a stated edge fact stage 2 does not test is checked on its
/// own, so a NURBS edge or face in one is [`BooleanUnsupported::Nurbs`].
pub fn boolean_given(
    a: &Brep,
    b: &Brep,
    op: BooleanOp,
    facts: &Incidences,
    cx: &mut Cx<'_>,
) -> Result<Done<NoItems>, BooleanError> {
    run(a, b, op, facts, cx)
}

/// How a stage-2 edge-face test is to be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lies {
    /// The caller stated it: a refusal when the edge leaves.
    Stated(Fact),
    /// A stated face pair implies it: the roots' answer when the bound does not prove it.
    Implied,
}

/// The facts read against the operands: which edge-face tests try the bound first, and
/// what is left to judge after stages 2 and 4.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Given {
    /// The edge-face tests a fact covers, by the edge's side, edge and face.
    edges: BTreeMap<(Side, EdgeId, FaceId), Lies>,
    /// The stated face pairs, `a`'s face then `b`'s, with their facts.
    faces: Vec<(FaceId, FaceId, Fact)>,
}

impl Given {
    /// No facts.
    pub(crate) const NONE: Self = Self {
        edges: BTreeMap::new(),
        faces: Vec::new(),
    };

    /// `facts` read against `ops`.
    ///
    /// # Errors
    ///
    /// [`BooleanRefusal::FalseFact`] for a fact naming an entity its operand does not have.
    pub(crate) fn of(ops: Operands<'_>, facts: &Incidences) -> Result<Self, BooleanError> {
        let mut given = Self::NONE;
        // Implied first, so a stated fact on the same test replaces its entry.
        for &fact in facts.facts() {
            if let Fact::FaceOnFace { a, b } = fact {
                let (Some(fa), Some(fb)) = (ops.a.face(a), ops.b.face(b)) else {
                    return Err(false_fact(fact));
                };
                for (side, face, other) in [(Side::A, fa, b), (Side::B, fb, a)] {
                    for u in face.uses() {
                        given
                            .edges
                            .insert((side, u.edge().id(), other), Lies::Implied);
                    }
                }
                given.faces.push((a, b, fact));
            }
        }
        for &fact in facts.facts() {
            if let Fact::EdgeOnFace {
                operand,
                edge,
                face,
            } = fact
            {
                let side = Side::BOTH
                    .into_iter()
                    .find(|s| s.operand() == operand)
                    .ok_or_else(|| false_fact(fact))?;
                if ops.of(side).edge(edge).is_none() || ops.of(side.other()).face(face).is_none() {
                    return Err(false_fact(fact));
                }
                given.edges.insert((side, edge, face), Lies::Stated(fact));
            }
        }
        Ok(given)
    }

    /// How the test of `edge` of `side` against `face` is to be read, if a fact covers it.
    pub(crate) fn edge(&self, side: Side, edge: EdgeId, face: FaceId) -> Option<Lies> {
        self.edges.get(&(side, edge, face)).copied()
    }

    /// The stated edge facts stage 2 left unjudged: those with no span, which it either
    /// never tested or found on the surface outside the face. Each is tested on its own.
    ///
    /// # Errors
    ///
    /// [`BooleanRefusal::FalseFact`] for one the edge leaves; as stage 2's edge-face test.
    pub(crate) fn judge_edges(
        &self,
        ops: Operands<'_>,
        found: &Interference,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<(), BooleanError> {
        for (&(side, edge, face), lies) in &self.edges {
            let Lies::Stated(fact) = *lies else {
                continue;
            };
            let spanned = found
                .spans
                .iter()
                .any(|s| (s.side, s.edge, s.face) == (side, edge, face));
            if !spanned && !lies_on(ops, side, edge, face, budget, stop)? {
                return Err(false_fact(fact));
            }
        }
        Ok(())
    }

    /// The stated face pairs against stage 4's coincident pairs.
    ///
    /// # Errors
    ///
    /// [`BooleanRefusal::FalseFact`] for a pair stage 4 did not find coincident over an
    /// area.
    pub(crate) fn judge_faces(&self, ff: &FaceFace) -> Result<(), BooleanError> {
        for &(a, b, fact) in &self.faces {
            if !ff.coincident.iter().any(|c| c.faces == [a, b]) {
                return Err(false_fact(fact));
            }
        }
        Ok(())
    }
}

/// The refusal of `fact`.
pub(crate) const fn false_fact(fact: Fact) -> BooleanError {
    BooleanError::Refused(BooleanRefusal::FalseFact { fact })
}

/// The error of stage 2's edge-face test for edge `e` of `side` (curve `curve`) against face
/// `f` of the other side.
pub(crate) fn of_edge_face(
    x: SurfaceError,
    side: Side,
    e: EdgeId,
    curve: &Curve,
    f: FaceId,
) -> BooleanError {
    match (x, curve) {
        // The NURBS one named: the edge when its curve is, else the face.
        (SurfaceError::NurbsIntersection, Curve::Nurbs(_)) => {
            BooleanError::Unsupported(BooleanUnsupported::Nurbs {
                operand: side.operand(),
                at: scx_brep::Entity::Edge(e),
            })
        }
        (x, _) => BooleanError::of_surface(x, side.other().operand(), f),
    }
}

/// Whether edge `e` of `side` lies on the surface of face `f` of the other side, by the
/// edge-face test's own reading: within the coincidence of the edge and the face, the
/// bound first and then the roots. An edge with no curve lies on no surface.
fn lies_on(
    ops: Operands<'_>,
    side: Side,
    e: EdgeId,
    f: FaceId,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<bool, BooleanError> {
    let (Some(ed), Some(fc)) = (ops.of(side).edge(e), ops.of(side.other()).face(f)) else {
        return Ok(false);
    };
    let EdgeShape::Curve { curve, interval } = ed.shape() else {
        return Ok(false);
    };
    let Some(uv_box) = fc.uv_box() else {
        return Err(BooleanError::Defect(crate::error::BooleanDefect::NoUvBox {
            operand: side.other().operand(),
            face: f,
        }));
    };
    let reach = ops.coincidence(recorded(ed.accuracy()), fc.accuracy());
    let res = Resolution::new(Length::from_mm(reach)).unwrap_or_else(|| ops.a.resolution());
    let on = curve_on_surface(curve, interval, fc.surface(), &uv_box, res, budget, stop)
        .map_err(|x| of_edge_face(x, side, e, curve, f))?;
    Ok(!matches!(on, OnSurface::Leaves(_)))
}
