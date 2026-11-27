// Written for solidcorex. Apache-2.0.
//! The general surface-surface intersector (research 73, step 5): every pair the closed
//! forms answer [`Closed::General`](scx_surface::Closed) for.
//!
//! [`intersect`] takes a pair as [`analytic::intersect`](crate::analytic::intersect) does,
//! plus the caller's [`Budget`] and [`Stop`] as `scx_algo::global_solve::solve` takes them,
//! and answers [`Traced`] or a typed [`SsiError`]. It never answers an empty result in
//! place of a refusal: an empty [`Traced`] is a proof that the surfaces do not meet within
//! the extents.
//!
//! # The pipeline (73 §4.1), by builder row (73 §7, `SM` and the number)
//!
//! | stage | row | until it lands |
//! |---|---|---|
//! | Bézier pieces, `BoxTree::pairs`, pairs dropped only by proof ([`Candidates`]), halving to `Unresolved` | 4 | a pair not proved apart is [`SsiError::Unresolved`] at its first undecided piece pair |
//! | loop-free pairs (normal cones, Sederberg-Meyers, the critical-point solve), boundary seeds | 5 | |
//! | the field path when a side has an implicit form; critical cells, valleys, poles | 6a, 6b | |
//! | the march for NURBS against NURBS, [`SsiError::Stalled`] | 7 | |
//! | one knot vector, certification per span, [`SsiError::Uncertified`] | 8 | |
//!
//! # The answer
//!
//! A [`Traced`] holds [`Branch`]es (curves), [`Node`]s (points where branch ends meet)
//! and [`Touch`]es (isolated points). Topology is claimed up to the coincidence, as the
//! closed forms' depth rule claims it (04 R73, Q3): two branches passing within it are a
//! node, and a loop or a gap within it is a touch.
//!
//! Each branch is three B-splines on one parameter: its 3D curve and a pcurve on each
//! surface, with the [`Certificate`] that bounds them (73 §4.7, §4.8). The 3D curve is
//! stored on an edge as `Curve::Nurbs` with a `Basis::Certified` record (04 R73, Q1).
//!
//! # Termination and determinism (73 §4.9)
//!
//! - Every unit of work is charged to `budget`, one unit per pair of pieces and per
//!   marching step, and `stop` is polled per pair after the budget, as the global solver
//!   does. There is no fixed cap: running out is [`SsiError::Budget`], a set flag
//!   [`SsiError::Stopped`]. A call that finishes gives the same bits whatever the limit.
//! - Pairs, seeds, branches, nodes and touches are in a fixed order (by pair of piece
//!   indices, then boundary side, then parameter), with no hash-order iteration and only
//!   `fmath` functions. Each pair's answer depends only on the pair, so the output is
//!   bit-identical across runs and platforms.

mod field;
mod implicit;
mod net;
mod pieces;
mod traced;

pub use pieces::{Candidates, Piece, PiecePair};
pub use traced::{Branch, Certificate, End, Ends, Node, NodeKind, OnSurface, Rim, Touch, Traced};

use scx_num::budget::{Budget, Stop};
use scx_num::extent2d::Extent2d;
use scx_num::units::{Accuracy, Resolution};
use scx_surface::Surface;

use crate::error::SsiError;

/// Where `a` over `ae` meets `b` over `be`, by the general method (module docs). The
/// extents are in each surface's uv (research 63 D13); the tolerance is
/// `coincident(res, acc[0], acc[1])`, as [`analytic::intersect`](crate::analytic::intersect)'s.
///
/// It answers any pair, the closed-form ones included, but is meant for those the closed
/// forms answer `General` for: [`crate::intersect`] tries the closed forms first.
///
/// Until rows SM5 to SM8 land the answer is [`Traced::default`], proved (every pair of
/// pieces is proved apart: [`Candidates`]), or a refusal.
///
/// # Errors
///
/// [`SsiError::Unresolved`], [`SsiError::Stalled`], [`SsiError::Singular`] or
/// [`SsiError::Uncertified`] where the method cannot finish the pair, each naming the
/// place; [`SsiError::Budget`] when `budget` runs out; [`SsiError::Stopped`] when `stop` is
/// set. Until row SM5 lands, a pair of surfaces whose pieces are not all proved apart is
/// `Unresolved` where the first piece pair that [`Candidates::settle`] could not clear lies.
pub fn intersect(
    a: &Surface,
    ae: &Extent2d,
    b: &Surface,
    be: &Extent2d,
    acc: [Accuracy; 2],
    res: Resolution,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Traced, SsiError> {
    let found = Candidates::new(a, ae, b, be, acc, res, budget, stop)?;
    if let Some(t) = field::trace(&found, [a, b], [ae, be], found.floor(), budget, stop)? {
        return Ok(t);
    }
    for &pair in found.pairs() {
        if let Some(open) = found.settle(pair, FRONT, budget, stop)? {
            return Err(open.unresolved());
        }
    }
    Ok(Traced::default())
}

/// The widest front of surviving pairs [`intersect`] lets one piece pair's halving keep
/// before it gives the pair up as undecided: a policy of effort, not a proof (it never
/// drops a pair). A pair apart by a small margin keeps a front of a few pairs at the nearest
/// points, and a pair the surfaces cross widens it with every halving.
const FRONT: usize = 24;

#[cfg(test)]
mod tests;
