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
//! none of that. This first part adds only what neither tests, through the pass `contract`
//! shares ([`pinched`](crate::structure)):
//!
//! - [`Finding::VertexNotManifold`]: the faces round a vertex are in more than one piece;
//! - [`Finding::NotSolid`]: a solid region's shell has a face with material on both sides;
//! - [`Findings::class`]: [`Class`], whether the body is a solid, a sheet or a mix. A sheet
//!   is valid input (an importer's open shell), so the class is a fact, not a defect.
//!
//! The certified gaps, clearance, radial order along the whole edge and face crossing are
//! added by later tasks as further [`Finding`] variants; each runs under the scopes below.

use scx_brep::op::Cx;
use scx_brep::{Brep, ShellId, VertexId};

use crate::error::CheckError;
use crate::structure::{Class, classify, pinched};

/// How much of the check to run. Every scope runs the structure part; the later parts say
/// which scopes run them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum Scope {
    /// Structure, gaps against the recorded accuracies and clearance: the default at import.
    #[default]
    Import,
    /// As [`Scope::Import`], for a body a stitch produced.
    Stitched,
    /// Everything, including faces crossing.
    Full,
}

/// A defect of a body, with the entity it is at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
}

/// A question the check could not decide. Nothing is listed yet: the structure part is
/// decided from the records. Later parts add the cases (a gap whose bracket spans its
/// allowance, a face pair needing an intersector).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Undecided {}

/// The work a check spent, in budget units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub struct Spent {
    /// The units charged.
    pub work: u64,
}

/// What [`check`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Findings {
    /// The defects, by kind and then id: the same list on every run.
    pub list: Vec<Finding>,
    /// What could not be decided. A body with any is not known to be clean.
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
/// The work charged is one unit per edge use, to a fresh budget of `cx`'s settings.
///
/// # Errors
///
/// [`CheckError::Exhausted`] when the budget runs out, [`CheckError::Cancelled`] when
/// `cx.stop` is set. Neither says anything about the body.
pub fn check(body: &Brep, cx: &mut Cx<'_>, scope: Scope) -> Result<Findings, CheckError> {
    // Every scope runs the structure part, which is all there is so far.
    let _ = scope;
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
    Ok(Findings {
        list,
        undecided: Vec::new(),
        class,
        spent: Spent {
            work: budget.used(),
        },
    })
}
