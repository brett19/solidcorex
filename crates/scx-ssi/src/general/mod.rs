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
//! | loop-free pairs (normal cones, Sederberg-Meyers, the critical-point solve), boundary seeds, splitting to `Unresolved` ([`Survey`]) | 5 | |
//! | the field path when a side has an implicit form; critical cells, valleys, poles | 6a, 6b | 6a landed; 6b: valleys of the survey refused by name before its solve (a contact of order 2 [`SsiError::Untraced`], above 2 `Singular`); the field path's touches, tangencies and crossings inside a piece stay `Unresolved` |
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
mod march;
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
/// (row SM6a): branches of degree-1 curves, coincident pieces, or a refusal. Otherwise the
/// survey's seeds and nodes (row SM5) are marched (row SM7): branches of degree-1 curves
/// through them, the nodes their ends meet at, and the touches; a pair with no seed and no
/// node holds touches only. The march runs in one order of the operands (by their extents,
/// then their points), so either order gives the same bits.
///
/// # Errors
///
/// [`SsiError::Unresolved`], [`SsiError::Stalled`], [`SsiError::Singular`],
/// [`SsiError::Uncertified`] or [`SsiError::Untraced`] where the method cannot finish the
/// pair, each naming the place; [`SsiError::Procedural`] at once for a procedural operand
/// (in the caller's order); [`SsiError::Budget`] when `budget` runs out;
/// [`SsiError::Stopped`] when `stop` is set. A valley in a candidate pair
/// ([`Survey::valleys`]) refuses the pair before any plan: `Singular` at the least flat one,
/// else `Untraced` at the least one. A singular contact is `Singular` at the least such
/// point (by x, then y, then z), whatever the operands' order.
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
    // No patch enclosure reads a procedural surface's net yet (75 §13.27): refuse before any
    // work rather than subdivide to the budget.
    if let Some(operand) = [a, b]
        .iter()
        .position(|s| matches!(s, Surface::Procedural(_)))
    {
        return Err(SsiError::Procedural { operand });
    }
    // NURBS against NURBS is marched in one order of the operands, so that either order
    // gives the same bits; the field path is symmetric by its own choice of side.
    let swap = field::field_side([a, b], [ae, be]).is_none() && key(b, be) < key(a, ae);
    let [acc_a, acc_b] = acc;
    let (a, ae, b, be, acc) = if swap {
        (b, be, a, ae, [acc_b, acc_a])
    } else {
        (a, ae, b, be, acc)
    };
    let survey = Survey::new(a, ae, b, be, acc, res, budget, stop)?;
    let found = survey.candidates();
    if let Some(t) = field::trace(found, [a, b], [ae, be], found.floor(), budget, stop)? {
        return Ok(t);
    }
    let tol = found.tolerance();
    // A valley refuses the whole intersection: its least point, a contact of order above 2
    // (singular) before one of order 2 (a curve to trace).
    let valleys = survey.valleys(budget, stop)?;
    let flat = valleys
        .iter()
        .filter(|c| c.kind == CriticalKind::Valley { flat: true })
        .map(|c| c.point)
        .reduce(|p, q| if less(q, p) { q } else { p });
    if let Some(at) = flat {
        return Err(SsiError::Singular {
            at,
            kind: SingularKind::DegenerateForm,
        });
    }
    if let Some(at) = valleys
        .iter()
        .map(|c| c.point)
        .reduce(|p, q| if less(q, p) { q } else { p })
    {
        return Err(SsiError::Untraced { at });
    }
    let mut touches = Vec::new();
    let mut seeds: Vec<Seed> = Vec::new();
    let mut nodes: Vec<&Critical> = Vec::new();
    // The refusal names the least point (x, then y, then z), so it does not depend on the
    // order of the operands.
    let mut singular: Option<(Point3, SingularKind)> = None;
    let mut untraced: Option<Point3> = None;
    let plans = survey.plans(budget, stop)?;
    let mut add = |s: &Seed| {
        if !seeds.iter().any(|o| o.same_as(s, tol)) {
            seeds.push(*s);
        }
    };
    for plan in &plans {
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
                        uv: if swap { [c.uv[1], c.uv[0]] } else { c.uv },
                    }),
                    CriticalKind::Node => nodes.push(c),
                    // A contact of order above 2: an isolated touch and a curve of them are
                    // not told apart.
                    CriticalKind::Valley { flat: true } => {
                        let kind = SingularKind::DegenerateForm;
                        if singular.is_none_or(|(p, _)| less(c.point, p)) {
                            singular = Some((c.point, kind));
                        }
                    }
                    CriticalKind::Valley { flat: false } => least(&mut untraced, c.point),
                    _ => {}
                }
                c.ray.iter().for_each(&mut add);
            }
        }
        plan.seeds.iter().for_each(&mut add);
    }
    if let Some((at, kind)) = singular {
        return Err(SsiError::Singular { at, kind });
    }
    if let Some(at) = untraced {
        return Err(SsiError::Untraced { at });
    }
    if seeds.is_empty() && nodes.is_empty() {
        return Ok(Traced {
            touches,
            ..Traced::default()
        });
    }
    let m = march::march(
        [a, b],
        [ae, be],
        &seeds,
        &nodes,
        tol,
        found.floor(),
        swap,
        budget,
        stop,
    )?;
    Ok(Traced {
        branches: m.branches,
        nodes: m.nodes,
        touches,
        coincident: Vec::new(),
    })
}

/// The order key of an operand: its extent's bits, then its points' at a 3 x 3 grid.
fn key(s: &Surface, e: &Extent2d) -> Vec<u64> {
    let mut k = vec![
        e.x_min().to_bits(),
        e.x_max().to_bits(),
        e.y_min().to_bits(),
        e.y_max().to_bits(),
    ];
    for i in 0..3 {
        for j in 0..3 {
            let (fu, fv) = (f64::from(i) / 2.0, f64::from(j) / 2.0);
            let uv = scx_num::vector2d::Point2::new(
                e.x_min() + (e.x_max() - e.x_min()) * fu,
                e.y_min() + (e.y_max() - e.y_min()) * fv,
            );
            k.extend(s.point(uv).to_array().map(f64::to_bits));
        }
    }
    k
}

/// Keeps in `slot` the least point (x, then y, then z) of those offered.
fn least(slot: &mut Option<Point3>, p: Point3) {
    if slot.is_none_or(|q| less(p, q)) {
        *slot = Some(p);
    }
}

/// Whether `a` is before `b`, by x, then y, then z.
fn less(a: Point3, b: Point3) -> bool {
    a.x.total_cmp(&b.x)
        .then(a.y.total_cmp(&b.y))
        .then(a.z.total_cmp(&b.z))
        .is_lt()
}

#[cfg(test)]
mod tests;
