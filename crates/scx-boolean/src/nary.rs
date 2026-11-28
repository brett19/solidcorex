// Written for solidcorex. Apache-2.0.
//! [`boolean_all`]: one Boolean over many operands, as a balanced fold in operand order.

use scx_brep::op::{Cx, Done, Report};
use scx_brep::{Brep, Changes, Operand};

use crate::boolean::{BooleanOp, NoItems, boolean};
use crate::error::{BooleanError, BooleanRefusal};

/// A body of the fold: an operand as given, or an intermediate made on the way.
enum Held<'a> {
    Operand(&'a Brep),
    Made(Brep),
}

impl Held<'_> {
    fn body(&self) -> &Brep {
        match self {
            Held::Operand(b) => b,
            Held::Made(b) => b,
        }
    }
}

/// What one fold made: the body, the last step's changes (none for a lone operand) and the
/// work of every step so far.
struct Folded<'a> {
    body: Held<'a>,
    changes: Option<Changes>,
    work: u64,
}

/// The Boolean `op` over `first` and `rest`, in the order given.
///
/// - [`Union`](BooleanOp::Union) and [`Intersect`](BooleanOp::Intersect) fold all the
///   operands as a balanced tree: the operands split in halves (the left half the larger
///   when odd), each half folded, left then right, and the two results combined by
///   [`boolean`].
/// - [`Subtract`](BooleanOp::Subtract) is `first` less everything in `rest`: `rest` is
///   unioned as a balanced tree, then subtracted from `first` in one step.
/// - No `rest` makes no Boolean: the body is `first` unchanged, with no changes and no work.
///
/// Every step runs in a fixed order (left half, right half, then their combination), so the
/// same operands give the same body and the same work. The budget is the whole fold's: each
/// step is given what the earlier steps left, and the report's work is their sum. The body is
/// built in `first.edit()` when `rest` is empty and otherwise in the final step's left
/// operand.
///
/// **Changes** describe the final step only, against the body of its left operand (the fold
/// of the leading operands); the lineage of the earlier steps is not composed. The cellular
/// merge (BO12) gives lineage over every operand at once.
///
/// # Errors
///
/// - [`BooleanRefusal::OtherDocument`] for an operand finished under another resolution than
///   `cx.doc`'s, named by its place in the call (`first` is operand 0, `rest[i]` operand
///   `i + 1`), before any step runs.
/// - Any error of [`boolean`] for a step. Such an error names the step's own operands, which
///   past the first level are intermediate bodies; [`BooleanError::Exhausted`] carries the
///   whole fold's limit.
pub fn boolean_all(
    first: &Brep,
    rest: &[&Brep],
    op: BooleanOp,
    cx: &mut Cx<'_>,
) -> Result<Done<NoItems>, BooleanError> {
    let res = cx.doc.resolution();
    for (i, body) in core::iter::once(first)
        .chain(rest.iter().copied())
        .enumerate()
    {
        if body.resolution() != res {
            return Err(BooleanError::Refused(BooleanRefusal::OtherDocument {
                operand: Operand::from_raw(u32::try_from(i).unwrap_or(u32::MAX)),
            }));
        }
    }
    let limit = cx.settings.budget;
    let folded = if op == BooleanOp::Subtract && !rest.is_empty() {
        let tools = fold(rest, BooleanOp::Union, limit, cx)?;
        join(&fold(&[first], op, limit, cx)?, &tools, op, limit, cx)?
    } else {
        let all: Vec<&Brep> = core::iter::once(first)
            .chain(rest.iter().copied())
            .collect();
        fold(&all, op, limit, cx)?
    };
    let Folded {
        body,
        changes,
        work,
    } = folded;
    let (body, changes) = match (body, changes) {
        (Held::Made(b), Some(c)) => (b, c),
        (held, _) => held
            .body()
            .edit()
            .finish(cx.doc)
            .map_err(|_| BooleanError::Defect(crate::error::BooleanDefect::Build))?,
    };
    Ok(Done::new(body, changes, Report::empty(work)))
}

/// `operands` folded by `op` as a balanced tree.
fn fold<'a>(
    operands: &[&'a Brep],
    op: BooleanOp,
    limit: u64,
    cx: &mut Cx<'_>,
) -> Result<Folded<'a>, BooleanError> {
    if let [one] = operands {
        return Ok(Folded {
            body: Held::Operand(one),
            changes: None,
            work: 0,
        });
    }
    let (l, r) = operands.split_at(operands.len().div_ceil(2));
    let left = fold(l, op, limit, cx)?;
    let right = fold(r, op, limit, cx)?;
    join(&left, &right, op, limit, cx)
}

/// One step: `left` op `right`, given what the work so far leaves of the budget.
fn join<'a>(
    left: &Folded<'a>,
    right: &Folded<'a>,
    op: BooleanOp,
    limit: u64,
    cx: &mut Cx<'_>,
) -> Result<Folded<'a>, BooleanError> {
    let before = left.work.saturating_add(right.work);
    let settings = cx.settings.with_budget(limit.saturating_sub(before));
    let done = {
        let mut step = Cx::new(cx.doc, &settings, cx.scratch, cx.stop, cx.trace);
        boolean(left.body.body(), right.body.body(), op, &mut step)
    };
    let done = done.map_err(|e| match e {
        BooleanError::Exhausted { .. } => BooleanError::Exhausted { limit },
        other => other,
    })?;
    Ok(Folded {
        work: before.saturating_add(done.report.work),
        changes: Some(done.changes),
        body: Held::Made(done.body),
    })
}
