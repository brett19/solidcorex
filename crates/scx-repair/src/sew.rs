// Written for solidcorex. Apache-2.0.
//! Sewing within a gap (research 75 §7, RP5): a body's open edges that pair within `gap`
//! made one, over [`scx_stitch::sew_within`].
//!
//! **What is sewn.** The body is cut into its pieces and stitched as
//! [`sew_within`](scx_stitch::sew_within) says: ends of open edges within `gap` are one
//! vertex (raised, `Measured`, to how far the merge moves them), and two open edges of
//! different pieces with the same ends whose certified symmetric gap is within `gap` are one
//! edge, keeping one of the pair's curves. Its uses on the dropped faces are the curve's
//! projections, each bound the certified gap (`PcurveOnSurface`); the kept edge's accuracy
//! is raised to the largest, recorded in `report.raised`: an accuracy it measurably has. The pin's factors (`GlueEdgesGeneral`'s
//! twice the gap, the stitch tolerance's `widest * 1.05`) are not ported (75 R3). The body's
//! resolution and every other edge stay as they were.
//!
//! **What is not.** Every pair the sew leaves is an item, in order of its lowest input edge:
//!
//! - [`SewCause::TooFar`]: two open edges of the result, each the other's nearest, whose
//!   certified symmetric gap is past `gap` but within the ratio limit; the cause carries the
//!   bracket, and [`SewCause::resize`] the gap that would sew it;
//! - [`SewCause::Ambiguous`] and [`SewCause::Undecided`]: what the stitch left
//!   ([`scx_stitch::Left`]), an edge with two partners or a gap no budget share decided.
//!
//! An open edge with no partner within the ratio limit is a boundary, not an item.
//!
//! **The gap.** Finite, above zero and below the ratio limit (`MAX_ACCURACY_RATIO` times the
//! zone): a sewn edge's accuracy is at most the gap, and an accuracy at the limit is a
//! finding of the check's. The pin caps the gap at 1 mm; here the cap is the document's.

use std::collections::BTreeMap;

use scx_brep::op::{Cx, Done, Report, Skipped};
use scx_brep::{Brep, EdgeId, EdgeShape, Entity};
use scx_num::budget::Budget;
use scx_num::consts::MAX_ACCURACY_RATIO;
use scx_num::units::Length;
use scx_stitch::{Left, Sewn, StitchError, Want, sew_within};
use scx_surface::{Aim, Bracket, GapError, Short, curve_curve_gap};

use crate::error::{RepairCause, RepairDefect, RepairError, SewCause};
use crate::heal::OnItemFailure;

/// The width a too-far pair's gap is searched to: this times the gap.
const WIDTH: f64 = 1e-3;

/// `body`'s open edges sewn within `gap` (see the [module](self)).
///
/// Items are the pairs the sew leaves, in order of their lowest input edge; `on_failure`
/// says what one does (01 §5.2). Under [`OnItemFailure::Skip`] the sewn body is returned with
/// each in `report.skipped` (round 1: a pair left is left whatever the others do, so one pass
/// is the fixpoint). A body with no open edge is returned as it is.
///
/// # Errors
///
/// - [`RepairCause::OtherDocument`] for a body finished under another resolution;
///   [`RepairCause::Gap`] for a gap not finite, not above zero, or at the ratio limit;
/// - [`RepairCause::Stitch`] for what the stitch refuses or cannot do yet (a piece with two
///   vertices within `gap`, a join whose kept curve has no projection the pcurve writer
///   gives);
/// - by `on_failure`: [`RepairError::Cause`] with the lowest item's [`SewCause`], or
///   [`RepairError::Items`] with every one;
/// - [`RepairError::Exhausted`] when the budget runs out, [`RepairError::Cancelled`] when
///   `cx.stop` is set; [`RepairError::Defect`] for the stitch's own failure.
pub fn sew(
    body: &Brep,
    gap: Length,
    on_failure: OnItemFailure,
    cx: &mut Cx<'_>,
) -> Result<Done<SewCause>, RepairError> {
    let res = cx.doc.resolution();
    if body.resolution() != res {
        return Err(RepairError::Cause(RepairCause::OtherDocument));
    }
    let limit = MAX_ACCURACY_RATIO * res.zone().mm();
    let g = gap.mm();
    if !(g.is_finite() && g > 0.0 && g < limit) {
        return Err(RepairError::Cause(RepairCause::Gap {
            gap,
            limit: Length::from_mm(limit),
        }));
    }
    let sewn = sew_within(body, gap, Want::Closed, cx).map_err(stitch_error)?;
    let mut budget = cx.budget();
    let mut items = left(&sewn, gap)?;
    items.extend(too_far(&sewn, g, limit, &mut budget, cx.stop)?);
    items.sort_by_key(SewCause::lowest);
    let skipped: Vec<Skipped<SewCause>> = items
        .into_iter()
        .enumerate()
        .map(|(item, cause)| Skipped {
            item,
            round: 1,
            cause,
        })
        .collect();
    let skipped = match on_failure {
        OnItemFailure::Stop => match skipped.into_iter().next() {
            Some(s) => return Err(RepairError::Cause(RepairCause::Sew(s.cause))),
            None => Vec::new(),
        },
        OnItemFailure::Collect if !skipped.is_empty() => {
            return Err(RepairError::Items(
                skipped
                    .into_iter()
                    .map(|s| Skipped {
                        item: s.item,
                        round: s.round,
                        cause: RepairCause::Sew(s.cause),
                    })
                    .collect(),
            ));
        }
        OnItemFailure::Collect | OnItemFailure::Skip => skipped,
    };
    let done = sewn.done;
    let report = done.report;
    Ok(Done::new(
        done.body,
        done.changes,
        Report::new(
            report.warnings,
            skipped,
            report.raised,
            report.work.saturating_add(budget.used()),
        ),
    ))
}

/// The stitch's error as the repair's: its budget and stop as the repair's own, its own
/// failure a defect, the rest the cause.
fn stitch_error(e: StitchError) -> RepairError {
    match e {
        // No item comes before the stitch.
        StitchError::Exhausted { .. } => RepairError::Exhausted { item: 0, work: 0 },
        StitchError::Cancelled => RepairError::Cancelled,
        StitchError::Defect(_) => RepairError::Defect(RepairDefect::Stitch(Box::new(e))),
        e => RepairError::Cause(RepairCause::Stitch(e)),
    }
}

/// The input edge a result edge is (a piece of), or itself where the stitch did not touch
/// it (the pieces keep the input's ids).
fn input(sewn: &Sewn, e: EdgeId) -> EdgeId {
    sewn.inputs.get(&e).copied().unwrap_or(e)
}

/// What the stitch left, by input edges.
fn left(sewn: &Sewn, gap: Length) -> Result<Vec<SewCause>, RepairError> {
    let mut out = Vec::new();
    for l in &sewn.report.left {
        match l {
            Left::Ambiguous { edge, partners } => {
                let mut partners: Vec<EdgeId> = partners.iter().map(|&p| input(sewn, p)).collect();
                partners.sort_unstable();
                out.push(SewCause::Ambiguous {
                    edge: input(sewn, *edge),
                    partners,
                });
            }
            Left::Undecided {
                edge,
                with,
                lower,
                upper,
                ..
            } => {
                let with = match *with {
                    Entity::Edge(w) => Entity::Edge(input(sewn, w)),
                    other => other,
                };
                out.push(SewCause::Undecided {
                    edge: input(sewn, *edge),
                    with,
                    gap: Bracket::new(lower.mm(), upper.mm())
                        .ok_or(RepairError::Defect(RepairDefect::Draft))?,
                    allowed: gap,
                });
            }
            // A later kind of what the stitch leaves: not a pair this sew names.
            _ => {}
        }
    }
    Ok(out)
}

/// The result's open edges that pair past `gap`: each the other's nearest by certified
/// symmetric gap, that gap past `gap` and within `limit` (mm). Candidates are the pairs
/// whose boxes come within `limit`; every pair is a budget charge.
fn too_far(
    sewn: &Sewn,
    gap: f64,
    limit: f64,
    budget: &mut Budget,
    stop: &scx_num::budget::Stop,
) -> Result<Vec<SewCause>, RepairError> {
    let body = &sewn.done.body;
    let named: std::collections::BTreeSet<EdgeId> = sewn
        .report
        .left
        .iter()
        .flat_map(|l| match l {
            Left::Ambiguous { edge, partners } => {
                let mut v = partners.clone();
                v.push(*edge);
                v
            }
            Left::Undecided { edge, with, .. } => match with {
                Entity::Edge(w) => vec![*edge, *w],
                _ => vec![*edge],
            },
            _ => Vec::new(),
        })
        .collect();
    let mut open = Vec::new();
    for &id in &sewn.open {
        if named.contains(&id) {
            continue;
        }
        let e = body
            .edge(id)
            .ok_or(RepairError::Defect(RepairDefect::Draft))?;
        let EdgeShape::Curve { curve, interval } = e.shape() else {
            continue;
        };
        let bx = e
            .bounding_box()
            .ok_or(RepairError::Defect(RepairDefect::Draft))?;
        open.push((id, curve, *interval, bx));
    }
    let aim = Aim::Width(WIDTH * gap);
    // Each edge's nearest partner within the limit: (the bracket, the partner).
    let mut nearest: BTreeMap<usize, (Bracket, usize)> = BTreeMap::new();
    for (i, a) in open.iter().enumerate() {
        for (j, b) in open.iter().enumerate().skip(i + 1) {
            stop.check().map_err(|_| RepairError::Cancelled)?;
            budget
                .charge(1)
                .map_err(|_| RepairError::Exhausted { item: 0, work: 0 })?;
            if a.3.min_distance(&b.3) > limit {
                continue;
            }
            let mut both: Option<Bracket> = None;
            for (x, y) in [(a, b), (b, a)] {
                let r = match curve_curve_gap(x.1, &x.2, y.1, &y.2, aim, budget, stop) {
                    Ok(r) => r,
                    Err(GapError::Undecided { why, reached }) => match why {
                        Short::Exhausted(_) => {
                            return Err(RepairError::Exhausted { item: 0, work: 0 });
                        }
                        Short::Stopped => return Err(RepairError::Cancelled),
                        _ => reached,
                    },
                    Err(_) => {
                        both = None;
                        break;
                    }
                };
                both = Some(match both {
                    None => r,
                    Some(p) => Bracket::new(p.lower().max(r.lower()), p.upper().max(r.upper()))
                        .ok_or(RepairError::Defect(RepairDefect::Draft))?,
                });
                if r.lower() > limit {
                    both = None;
                    break;
                }
            }
            let Some(both) = both.filter(|b| b.lower() <= limit) else {
                continue;
            };
            for (k, other) in [(i, j), (j, i)] {
                let better = nearest
                    .get(&k)
                    .is_none_or(|(n, _)| both.upper() < n.upper());
                if better {
                    nearest.insert(k, (both, other));
                }
            }
        }
    }
    let mut out = Vec::new();
    for (&i, &(bracket, j)) in &nearest {
        let mutual = nearest.get(&j).is_some_and(|&(_, back)| back == i);
        // Each pair once, from its lower index; a pair the sew could have made is the
        // stitch's (left or joined), not too far.
        if !mutual || i > j || bracket.exceeds(gap) != Some(true) {
            continue;
        }
        let (Some(a), Some(b)) = (open.get(i), open.get(j)) else {
            continue;
        };
        let (x, y) = (input(sewn, a.0), input(sewn, b.0));
        out.push(SewCause::TooFar {
            edges: [x.min(y), x.max(y)],
            gap: bracket,
            allowed: Length::from_mm(gap),
        });
    }
    Ok(out)
}
