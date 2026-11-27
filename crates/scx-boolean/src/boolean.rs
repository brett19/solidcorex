// Written for solidcorex. Apache-2.0.
//! [`boolean`]: the operation's signature, and the stages built so far.

use scx_brep::op::{Cx, Done, Report};
use scx_brep::{Brep, FaceId};
use scx_num::budget::{Budget, Stop};

use crate::build::build;
use crate::classify::classify;
use crate::contact::{Span, Touched, interferences};
use crate::error::{BooleanError, BooleanRefusal};
use crate::faceface::face_face;
use crate::geom::{Operands, Side};
use crate::imprint::imprint;
use crate::pairs::face_pairs;
use crate::points::{Point, merge};

/// Which Boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BooleanOp {
    /// What is in either.
    Union,
    /// What is in `a` and not in `b`.
    Subtract,
    /// What is in both.
    Intersect,
}

/// A Boolean has no items to skip: it finishes whole or fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoItems {}

/// The Boolean `op` of `a` and `b`, built in `a.edit()`: `a` is operand 0 and `b` operand 1
/// in the result's lineage (72 §5.4).
///
/// Both operands are finished bodies of `cx.doc`: one operation, one document, one
/// resolution (72 §12.4 row 4). Where they meet is judged within coincidence of that
/// resolution and the entities' accuracies; neither operand's tolerances are rewritten.
///
/// # Errors
///
/// - [`BooleanRefusal::OtherDocument`] for an operand finished under another resolution
///   than `cx.doc`'s.
/// - [`BooleanRefusal::VerticesWithinCoincidence`] where two vertices of one operand meet
///   the other operand at one place and lie within coincidence of each other.
/// - [`BooleanUnsupported::Chain`] where two vertices of one operand farther apart than
///   coincidence are one place only through a chain of contacts.
/// - [`BooleanUnsupported::Nurbs`] for NURBS geometry where the operands meet.
/// - [`BooleanUnsupported::SsiPair`] for a pair of analytic faces whose intersection has
///   no closed form here.
/// - [`BooleanUnsupported::Pcurve`] where a face's new loops need a pcurve with no exact
///   form (a fitted one).
/// - [`BooleanUnsupported::NonManifold`] where the result, or an operand at a place a
///   query reads, has an edge more than two faces bound.
/// - [`BooleanUnsupported::Nesting`] where the result's connected sets of faces nest in a
///   way other than cavities of one set.
/// - [`BooleanError::Exhausted`] when `cx`'s budget runs out, [`BooleanError::Cancelled`]
///   when `cx.stop` is set, and [`BooleanError::Defect`] for the method's own failure.
pub fn boolean(
    a: &Brep,
    b: &Brep,
    op: BooleanOp,
    cx: &mut Cx<'_>,
) -> Result<Done<NoItems>, BooleanError> {
    let res = cx.doc.resolution();
    for (side, body) in Side::BOTH.into_iter().zip([a, b]) {
        if body.resolution() != res {
            return Err(BooleanError::Refused(BooleanRefusal::OtherDocument {
                operand: side.operand(),
            }));
        }
    }
    let mut budget = cx.budget();
    let ops = Operands { a, b };
    let found = interfere(ops, &mut budget, cx.stop)?;
    let ff = face_face(ops, &found, &mut budget, cx.stop)?;
    let imprinted = imprint(ops, &found, &ff, *cx.doc, &mut budget, cx.stop)?;
    let classes = classify(ops, &ff, &imprinted, &mut budget, cx.stop)?;
    let (body, changes) = build(ops, imprinted, &classes, op, *cx.doc, &mut budget, cx.stop)?;
    Ok(Done::new(body, changes, Report::empty(budget.used())))
}

/// Where the operands meet: stages 1 to 3, with what they read of the faces.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Interference {
    /// The face pairs whose boxes meet, by `a`'s id then `b`'s.
    pub(crate) pairs: Vec<(FaceId, FaceId)>,
    /// The merged points, in order.
    pub(crate) points: Vec<Point>,
    /// Where an edge lies on a face of the other operand, by side, edge, face, start.
    pub(crate) spans: Vec<Span>,
    /// The faces the pairs name, with their parts, read once.
    pub(crate) touched: Touched,
}

/// Stages 1 to 3 over `ops`.
pub(crate) fn interfere(
    ops: Operands<'_>,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Interference, BooleanError> {
    let pairs = face_pairs(ops, stop)?;
    let found = interferences(ops, &pairs, budget, stop)?;
    let points = merge(ops, &found.touched, &found.contacts)?;
    let mut spans = found.spans;
    spans.sort_by(|p, q| {
        (p.side, p.edge, p.face)
            .cmp(&(q.side, q.edge, q.face))
            .then(p.t.min().total_cmp(&q.t.min()))
    });
    spans.dedup();
    Ok(Interference {
        pairs,
        points,
        spans,
        touched: found.touched,
    })
}
