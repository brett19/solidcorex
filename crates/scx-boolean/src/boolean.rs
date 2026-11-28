// Written for solidcorex. Apache-2.0.
//! [`boolean`]: the operation's signature, and the stages built so far.

use scx_brep::op::{Cx, Done, Report};
use scx_brep::{Brep, FaceId};
use scx_num::budget::{Budget, Stop};

use crate::build::build;
use crate::cells::Rule;
use crate::classify::classify;
use crate::contact::{Span, Touched, interferences};
use crate::error::{BooleanError, BooleanRefusal};
use crate::faceface::{FaceFace, face_face};
use crate::geom::{Operands, Side};
use crate::given::{Given, Incidences};
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
/// - [`BooleanUnsupported::NonManifold`] where two kept faces leave an edge in one
///   direction with tied bends, where a tangent line or circle ends inside a kept face,
///   where a touch inside a face falls on a piece's boundary, or where an operand is
///   non-manifold at a place a query reads.
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
    run(a, b, &Rule::of(op), &Incidences::NONE, cx)
}

/// The selection `rule` over the cells of `a` and `b`, built in `a.edit()` as [`boolean`]
/// builds: the cells `rule` makes solid, bounded by the faces between solid and void cells,
/// and the walls, laminas and sheets its [`Faces`](crate::Faces) keep (BO25). A result may
/// so hold faces with solid on both sides (each cell its own region) or void on both (a
/// lamina, or a sheet with free edges). [`Rule::of`] is the regularised Boolean.
///
/// # Errors
///
/// As [`boolean`].
pub fn combine(
    a: &Brep,
    b: &Brep,
    rule: &Rule<'_>,
    cx: &mut Cx<'_>,
) -> Result<Done<NoItems>, BooleanError> {
    run(a, b, rule, &Incidences::NONE, cx)
}

/// [`combine`] with `facts` stated ([`crate::boolean_given`]): no facts is [`combine`].
pub(crate) fn run(
    a: &Brep,
    b: &Brep,
    rule: &Rule<'_>,
    facts: &Incidences,
    cx: &mut Cx<'_>,
) -> Result<Done<NoItems>, BooleanError> {
    staged(a, b, rule, facts, cx).map(|(done, _)| done)
}

/// [`run`] with what stages 1 to 4 found, for a caller that reads the meet as well as the
/// built body ([`section`](crate::section)).
pub(crate) fn staged(
    a: &Brep,
    b: &Brep,
    rule: &Rule<'_>,
    facts: &Incidences,
    cx: &mut Cx<'_>,
) -> Result<(Done<NoItems>, (Interference, FaceFace)), BooleanError> {
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
    let given = Given::of(ops, facts)?;
    let found = interfere_given(ops, &given, &mut budget, cx.stop)?;
    given.judge_edges(ops, &found, &mut budget, cx.stop)?;
    let ff = face_face(ops, &found, &mut budget, cx.stop)?;
    given.judge_faces(&ff)?;
    let imprinted = imprint(ops, &found, &ff, *cx.doc, &mut budget, cx.stop)?;
    let classes = classify(ops, &ff, &imprinted, &mut budget, cx.stop)?;
    let (body, changes) = build(
        ops,
        imprinted,
        &ff,
        &classes,
        rule,
        *cx.doc,
        &mut budget,
        cx.stop,
    )?;
    Ok((
        Done::new(body, changes, Report::empty(budget.used())),
        (found, ff),
    ))
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

/// Stages 1 to 3 over `ops`, no facts given.
pub(crate) fn interfere(
    ops: Operands<'_>,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Interference, BooleanError> {
    interfere_given(ops, &Given::NONE, budget, stop)
}

/// Stages 1 to 3 over `ops`, no facts given, over only the pairs of `a`'s face `only` when
/// one is named (a section scoped to one face).
pub(crate) fn interfere_only(
    ops: Operands<'_>,
    only: Option<FaceId>,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Interference, BooleanError> {
    let mut pairs = face_pairs(ops, stop)?;
    if let Some(face) = only {
        pairs.retain(|&(fa, _)| fa == face);
    }
    interfere_pairs(ops, pairs, &Given::NONE, budget, stop)
}

/// Stages 1 to 3 over `ops`, the edge-face tests `given` covers read through it.
pub(crate) fn interfere_given(
    ops: Operands<'_>,
    given: &Given,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Interference, BooleanError> {
    let pairs = face_pairs(ops, stop)?;
    interfere_pairs(ops, pairs, given, budget, stop)
}

/// Stages 2 and 3 over `pairs` of `ops`.
fn interfere_pairs(
    ops: Operands<'_>,
    pairs: Vec<(FaceId, FaceId)>,
    given: &Given,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Interference, BooleanError> {
    let found = interferences(ops, &pairs, given, budget, stop)?;
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
