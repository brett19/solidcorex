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
//! | Bézier pieces, `BoxTree::pairs`, pairs dropped only by proof ([`Candidates`]) | 4 | |
//! | loop-free pairs (normal cones, Sederberg-Meyers, the critical-point solve), boundary seeds, splitting to `Unresolved` ([`Survey`]) | 5 | where no side has an implicit form, a pair with a seed, a node or a point a loop may surround is [`SsiError::Untraced`] there |
//! | the field path when a side has an implicit form; critical cells, valleys, poles | 6a, 6b | 6a landed: a touch, a tangency or a crossing inside a piece is `Unresolved` until 6b |
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
mod loops;
mod net;
mod patch;
mod pieces;
mod solve;
mod traced;

pub use loops::{Critical, CriticalKind, Loops, Plan, Seed, Side, Site, Survey};
pub use pieces::{Candidates, Piece, PiecePair};
pub use traced::{Branch, Certificate, End, Ends, Node, NodeKind, OnSurface, Rim, Touch, Traced};

use scx_num::budget::{Budget, Stop};
use scx_num::extent2d::Extent2d;
use scx_num::units::{Accuracy, Resolution};
use scx_num::vector3d::Point3;
use scx_surface::Surface;

use crate::error::{SingularKind, SsiError};

/// Where `a` over `ae` meets `b` over `be`, by the general method (module docs). The
/// extents are in each surface's uv (research 63 D13); the tolerance is
/// `coincident(res, acc[0], acc[1])`, as [`analytic::intersect`](crate::analytic::intersect)'s.
///
/// It answers any pair, the closed-form ones included, but is meant for those the closed
/// forms answer `General` for: [`crate::intersect`] tries the closed forms first.
///
/// When a side has an implicit form (an analytic kind) the field path traces the pair
/// (row SM6a): branches of degree-1 curves, coincident pieces, or a refusal. Otherwise,
/// until rows SM7 and SM8 land, the answer holds touches only: every pair of pieces is
/// proved apart ([`Candidates`]) or settled by a [`Plan`] with no seed and no point a loop
/// may surround (row SM5); anything else is a refusal.
///
/// # Errors
///
/// [`SsiError::Unresolved`], [`SsiError::Stalled`], [`SsiError::Singular`],
/// [`SsiError::Uncertified`] or [`SsiError::Untraced`] where the method cannot finish the
/// pair, each naming the place; [`SsiError::Budget`] when `budget` runs out;
/// [`SsiError::Stopped`] when `stop` is set. Until row SM7 lands, a pair with a seed, a node or a
/// point a loop may surround is `Untraced`, and a singular contact `Singular`, each at the
/// least such point (by x, then y, then z), whatever the operands' order.
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
    let survey = Survey::new(a, ae, b, be, acc, res, budget, stop)?;
    let found = survey.candidates();
    if let Some(t) = field::trace(found, [a, b], [ae, be], found.floor(), budget, stop)? {
        return Ok(t);
    }
    let mut touches = Vec::new();
    // The refusal names the least point (x, then y, then z) of its kind, so it does not
    // depend on the order of the operands: a singular contact before a curve to trace.
    let mut singular: Option<(Point3, SingularKind)> = None;
    let mut untraced: Option<Point3> = None;
    for plan in survey.plans(budget, stop)? {
        if let Loops::Critical { points, .. } = &plan.loops {
            for c in points {
                match c.kind {
                    CriticalKind::Singular(kind) => {
                        if singular.is_none_or(|(p, _)| less(c.point, p)) {
                            singular = Some((c.point, kind));
                        }
                    }
                    CriticalKind::Touch => touches.push(Touch {
                        point: c.point,
                        uv: c.uv,
                    }),
                    CriticalKind::Node => least(&mut untraced, c.point),
                    _ if c.may_surround() => least(&mut untraced, c.point),
                    _ => {}
                }
            }
        }
        for s in &plan.seeds {
            least(&mut untraced, s.point);
        }
    }
    if let Some((at, kind)) = singular {
        return Err(SsiError::Singular { at, kind });
    }
    if let Some(at) = untraced {
        return Err(SsiError::Untraced { at });
    }
    Ok(Traced {
        touches,
        ..Traced::default()
    })
}

/// Whether `a` is before `b`, by x, then y, then z.
fn less(a: Point3, b: Point3) -> bool {
    a.x.total_cmp(&b.x)
        .then(a.y.total_cmp(&b.y))
        .then(a.z.total_cmp(&b.z))
        .is_lt()
}

fn least(slot: &mut Option<Point3>, p: Point3) {
    if slot.is_none_or(|q| less(p, q)) {
        *slot = Some(p);
    }
}

#[cfg(test)]
mod tests;
