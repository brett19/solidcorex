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
//! | Bézier pieces, `BoxTree::pairs`, pairs dropped only by proof, halving to `Unresolved` | 4 | every pair is [`SsiError::Unresolved`] over the whole of both extents |
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

mod traced;

pub use traced::{Branch, Certificate, End, Ends, Node, NodeKind, OnSurface, Rim, Touch, Traced};

use scx_num::budget::{Budget, Stop};
use scx_num::extent2d::Extent2d;
use scx_num::units::{Accuracy, Length, Resolution};
use scx_surface::Surface;

use crate::error::SsiError;

/// Where `a` over `ae` meets `b` over `be`, by the general method (module docs). The
/// extents are in each surface's uv (research 63 D13); the tolerance is
/// `coincident(res, acc[0], acc[1])`, as [`analytic::intersect`](crate::analytic::intersect)'s.
///
/// It answers any pair, the closed-form ones included, but is meant for those the closed
/// forms answer `General` for: [`crate::intersect`] tries the closed forms first.
///
/// # Errors
///
/// [`SsiError::Unresolved`], [`SsiError::Stalled`], [`SsiError::Singular`] or
/// [`SsiError::Uncertified`] where the method cannot finish the pair, each naming the
/// place; [`SsiError::Budget`] when `budget` runs out; [`SsiError::Stopped`] when `stop` is
/// set. Until row SM4 lands, every pair that the budget and the flag let through is
/// `Unresolved` over the whole of both extents.
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
    // The one pair of pieces (the whole of each extent): charged, then the flag polled.
    budget.charge(1)?;
    stop.check()?;
    let _ = (acc, res);
    Err(whole_pair(a, ae, b, be))
}

/// The refusal of a pair left undecided over the whole of both extents: `Unresolved` at
/// the middle of the overlap of the surfaces' certified boxes (or of their union when they
/// do not overlap), of that box's longest side. A box that cannot be enclosed (a value not
/// finite) is an undecided region of unbounded size, named at the middle of the two
/// extents' middle points.
fn whole_pair(a: &Surface, ae: &Extent2d, b: &Surface, be: &Extent2d) -> SsiError {
    if let (Some(xa), Some(xb)) = (a.box_over(*ae), b.box_over(*be)) {
        let region = xa.intersect(xb).unwrap_or_else(|| xa.union(xb));
        SsiError::Unresolved {
            at: region.mid(),
            size: Length::from_mm(region.max_length()),
        }
    } else {
        let (pa, pb) = (a.point(ae.mid()), b.point(be.mid()));
        SsiError::Unresolved {
            at: pa + (pb - pa) * 0.5,
            size: Length::from_mm(f64::INFINITY),
        }
    }
}

#[cfg(test)]
mod tests;
