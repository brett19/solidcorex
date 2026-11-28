// Written for solidcorex. Apache-2.0.
//! [`check`]: the full validity check's result types and the part of it that is built so far
//! (research 74 §3, task CF1): [`Scope`], [`Finding`] and [`Findings`].
//!
//! `check` reads a finished body and never writes it (04 row 215's principle: a check never
//! changes the body it checks). Its answer is a list of typed [`Finding`]s, each naming its
//! entity and the measured amount where there is one; nothing is numbered, and nothing
//! unknown passes or fails: what a method cannot decide is listed apart in
//! [`Findings::undecided`].
//!
//! **What `check` adds to what a finished body already has.** `Draft::finish` proves the
//! records consistent and the ends, uses and pcurves within their own samples, and
//! [`contract`](crate::contract) proves closed, oriented and manifold-edge. `check` repeats
//! none of that. It adds, through the pass `contract` shares ([`pinched`](crate::structure)):
//!
//! - [`Finding::VertexNotManifold`]: the faces round a vertex are in more than one piece;
//! - [`Finding::NotSolid`]: a solid region's shell has a face with material on both sides;
//! - [`Findings::class`]: [`Class`], whether the body is a solid, a sheet or a mix. A sheet
//!   is valid input (an importer's open shell), so the class is a fact, not a defect;
//!
//! and, by certified gaps on the geometry (research 74 §28, task `CF2a`; [`crate::gaps`]):
//!
//! - [`Finding::EdgeOffFace`], [`Finding::PcurveOffEdge`], [`Finding::VertexOffEdge`]: an
//!   edge further from a face's surface than its reach, a use's lift further from its edge
//!   than its bound, an edge's end further from its vertex than the two reaches;
//! - [`Finding::DegenerateEdge`]: an edge whose whole curve spans no more than its reach;
//! - [`Finding::WideVertex`], [`Finding::WideEdge`]: under [`Scope::Full`], an accuracy at
//!   or past [`MAX_ACCURACY_RATIO`](scx_num::consts::MAX_ACCURACY_RATIO) times the
//!   resolution.
//!
//! and, by vertex pairs ([`crate::clearance`], RP1):
//!
//! - [`Finding::VerticesWithinCoincidence`]: two vertices within the larger of their reaches.
//!
//! A gap finding is made only on a lower bound past its allowance (74 R2); a bracket that
//! straddles it, a search that ran out of its share of the budget (R6) and a procedural
//! surface are listed in [`Findings::undecided`], never passed and never failed.
//!
//! Tube clearance (74 §3.3's `TubeNotClear`), radial order along the whole edge and face
//! crossing are added by later tasks
//! as further [`Finding`] variants.

use scx_brep::op::Cx;
use scx_brep::{Brep, EdgeId, FaceId, Fin, ShellId, VertexId};
use scx_num::units::Length;
use scx_surface::Bracket;

use crate::clearance;
use crate::error::CheckError;
use crate::gaps;
use crate::structure::{Class, classify, pinched};

/// How much of the check to run. Every scope runs the structure part and the gaps; the
/// scopes differ in what they hold a body to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum Scope {
    /// Structure, gaps against the recorded accuracies and (`CF2b`) clearance: the default at
    /// import. An accuracy past the ratio limit is not a defect of imported data (09 §6: it
    /// is a recorded raise with a warning, and clearance is what holds it), so no
    /// [`Finding::WideVertex`] or [`Finding::WideEdge`] is made.
    #[default]
    Import,
    /// As [`Scope::Import`], for a body a stitch produced. The pin's `ValidityScope::Stitched`
    /// excuses a gap past half a vertex's tolerance up to the sum of the two zones; here every
    /// allowance already is that sum (09 §3: distances add), so the two scopes ask the same
    /// gaps, and the clearance the pin then proves is `CF2b`'s.
    Stitched,
    /// Everything: as [`Scope::Import`], the ratio limit of results of our own operations
    /// ([`Finding::WideVertex`], [`Finding::WideEdge`]) and, later, faces crossing.
    Full,
}

/// A defect of a body, with the entity it is at. Distances are millimetres; a gap's
/// [`Bracket`] holds the true value and its lower bound is past `allowed`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Finding {
    /// The faces round the vertex are in more than one piece (see [`pinched`]).
    VertexNotManifold {
        /// The vertex.
        vertex: VertexId,
        /// The pieces, at least two.
        pieces: usize,
    },
    /// The shell bounds a solid region and has a face with material on its other side too.
    NotSolid {
        /// The shell.
        shell: ShellId,
    },
    /// The edge's curve strays from the face's surface (the kind's whole surface, which the
    /// face lies on) by more than the edge's reach: the edge cannot lie on the face.
    EdgeOffFace {
        /// The edge.
        edge: EdgeId,
        /// The face.
        face: FaceId,
        /// The largest distance of the curve from the surface.
        gap: Bracket,
        /// The edge's reach (its accuracy, at least the resolution's zone).
        allowed: Length,
    },
    /// A use's image on its face strays from its edge by more than the use's bound: for a
    /// curve edge the pcurve's lift against the curve at the same parameter, for a collapsed
    /// edge the lift against the vertex (allowed the use's, the edge's and the vertex's
    /// reaches together).
    PcurveOffEdge {
        /// The edge.
        edge: EdgeId,
        /// The use, by its fin.
        fin: Fin,
        /// The largest distance.
        gap: Bracket,
        /// The allowance.
        allowed: Length,
    },
    /// The edge's curve ends further from its vertex than the edge's and the vertex's reaches
    /// together.
    VertexOffEdge {
        /// The vertex.
        vertex: VertexId,
        /// The edge.
        edge: EdgeId,
        /// The edge's parameter at that end.
        at: f64,
        /// The distance.
        gap: Bracket,
        /// The allowance.
        allowed: Length,
    },
    /// The edge's whole curve lies within its own reach of a point: no two of its points are
    /// further apart than the reach, so the edge is smaller than its accuracy (74 Q5, a
    /// feature below the resolution).
    DegenerateEdge {
        /// The edge.
        edge: EdgeId,
        /// The curve's diameter (its largest distance between two points).
        extent: Bracket,
        /// The edge's reach.
        allowed: Length,
    },
    /// Under [`Scope::Full`]: the vertex's accuracy is at or past the validity limit.
    WideVertex {
        /// The vertex.
        vertex: VertexId,
        /// Its accuracy.
        accuracy: Length,
        /// The limit, [`MAX_ACCURACY_RATIO`](scx_num::consts::MAX_ACCURACY_RATIO) times the
        /// resolution.
        limit: Length,
    },
    /// Under [`Scope::Full`]: the edge's accuracy is at or past the validity limit.
    WideEdge {
        /// The edge.
        edge: EdgeId,
        /// Its accuracy.
        accuracy: Length,
        /// The limit.
        limit: Length,
    },
    /// Two vertices lie within coincidence of each other (74 §3.3, 04's merged-points rule):
    /// their distance is within the larger of their reaches, so they are one point that the
    /// body holds twice. A stitch refuses the same pair. Every scope asks it.
    VerticesWithinCoincidence {
        /// The lower vertex.
        a: VertexId,
        /// The higher vertex.
        b: VertexId,
        /// Their distance; its upper bound is within `allowed`.
        distance: Bracket,
        /// The larger of the two reaches.
        allowed: Length,
    },
}

/// Why a gap is undecided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Why {
    /// The item's share of the budget ran out (74 R6).
    Exhausted,
    /// The pieces that keep the bracket open cannot be cut finer in floating point.
    Precision,
    /// The bracket closed to within [`gaps::NEAR`] of the allowance and still holds it: the
    /// gap is the allowance, to that width.
    AtAllowance,
    /// The enclosure the method uses cannot be made tighter by its own cuts (a degenerate
    /// edge's box pairs over its fixed pieces).
    Enclosure,
}

/// A question the check could not decide: the bracket reached, the allowance and why.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Undecided {
    /// As [`Finding::EdgeOffFace`], undecided.
    EdgeOffFace {
        /// The edge.
        edge: EdgeId,
        /// The face.
        face: FaceId,
        /// The curve's distance from the surface, as far as it was bounded.
        gap: Bracket,
        /// The allowance.
        allowed: Length,
        /// Why.
        why: Why,
    },
    /// As [`Finding::PcurveOffEdge`], undecided.
    PcurveOffEdge {
        /// The edge.
        edge: EdgeId,
        /// The use, by its fin.
        fin: Fin,
        /// The bracket reached.
        gap: Bracket,
        /// The allowance.
        allowed: Length,
        /// Why.
        why: Why,
    },
    /// As [`Finding::VertexOffEdge`], undecided.
    VertexOffEdge {
        /// The vertex.
        vertex: VertexId,
        /// The edge.
        edge: EdgeId,
        /// The edge's parameter at that end.
        at: f64,
        /// The bracket.
        gap: Bracket,
        /// The allowance.
        allowed: Length,
        /// Why.
        why: Why,
    },
    /// As [`Finding::DegenerateEdge`], undecided.
    DegenerateEdge {
        /// The edge.
        edge: EdgeId,
        /// The bracket on the diameter.
        extent: Bracket,
        /// The allowance.
        allowed: Length,
        /// Why.
        why: Why,
    },
    /// A gap to a procedural surface, whose arm is not built
    /// ([`GapError::Procedural`](scx_surface::GapError::Procedural)).
    Procedural {
        /// The edge.
        edge: EdgeId,
        /// The face.
        face: FaceId,
    },
    /// As [`Finding::VerticesWithinCoincidence`], undecided: the distance's bracket holds
    /// the allowance.
    VerticesWithinCoincidence {
        /// The lower vertex.
        a: VertexId,
        /// The higher vertex.
        b: VertexId,
        /// The bracket.
        distance: Bracket,
        /// The allowance.
        allowed: Length,
        /// Why.
        why: Why,
    },
    /// The geometry cannot be enclosed over the edge (a value overflows).
    NotFinite {
        /// The edge.
        edge: EdgeId,
        /// The face, where the gap was to one.
        face: Option<FaceId>,
    },
}

/// The work a check spent, in budget units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub struct Spent {
    /// The units charged.
    pub work: u64,
}

/// What [`check`] found.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Findings {
    /// The defects, by kind and then id: the same list on every run.
    pub list: Vec<Finding>,
    /// What could not be decided, by kind and then id. A body with any is not known to be
    /// clean; its length is the count of items left undecided (74 R6).
    pub undecided: Vec<Undecided>,
    /// What the body is made of.
    pub class: Class,
    /// The work spent.
    pub spent: Spent,
}

impl Findings {
    /// Whether nothing was found and nothing is undecided.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.list.is_empty() && self.undecided.is_empty()
    }
}

/// Checks `body` over `scope`. Reads only: the body, the document and `cx.scratch` are as
/// they were.
///
/// The work is charged to a fresh budget of `cx`'s settings: one unit per edge use for the
/// structure, one per vertex and one per vertex pair tested for clearance, then each edge in id order gets an equal share of what is left for its gaps
/// (74 R6), one unit per point evaluated; an edge that spends its share is listed as
/// undecided with what it reached, and its unspent share passes to the edges after it.
///
/// # Errors
///
/// [`CheckError::Exhausted`] when the budget runs out in the structure part,
/// [`CheckError::Cancelled`] when `cx.stop` is set. Neither says anything about the body.
pub fn check(body: &Brep, cx: &mut Cx<'_>, scope: Scope) -> Result<Findings, CheckError> {
    cx.stop.check().map_err(|_| CheckError::Cancelled)?;
    let mut budget = cx.budget();
    let uses: usize = body
        .faces()
        .map(|f| f.loops().map(|l| l.uses().len()).sum::<usize>())
        .sum();
    budget
        .charge(uses as u64)
        .map_err(|e| CheckError::Exhausted { limit: e.limit })?;
    let (class, shells) = classify(body);
    let mut list: Vec<Finding> = pinched(body)
        .into_iter()
        .map(|p| Finding::VertexNotManifold {
            vertex: p.vertex,
            pieces: p.pieces,
        })
        .collect();
    list.extend(shells.into_iter().map(|shell| Finding::NotSolid { shell }));
    let found = gaps::run(body, scope, &mut budget, cx.stop)?;
    let clear = clearance::run(body, &mut budget, cx.stop)?;
    list.extend(found.list);
    list.extend(clear.list);
    let mut undecided = found.undecided;
    undecided.extend(clear.undecided);
    list.sort_by_key(gaps::finding_key);
    undecided.sort_by_key(gaps::undecided_key);
    Ok(Findings {
        list,
        undecided,
        class,
        spent: Spent {
            work: budget.used(),
        },
    })
}
