// Written for solidcorex. Apache-2.0.
//! Vertex identification by 04's merged-points row and its step-4 amendment (74 §4.3
//! step 1, R7).
//!
//! 1. **Pairs.** Every vertex of every sheet goes into one grid whose cell is twice the
//!    largest coincidence, so each pair within coincidence (`max` of the two reaches) lies
//!    in neighbouring cells. Each vertex is tested against the vertices of the 27 cells
//!    round it with a larger index: candidate pairs in O(V) on spread-out input.
//! 2. **Clearance.** A pair of one sheet is `Refused`, the lowest by sheet then ids.
//! 3. **Components** of "within coincidence", fixed before any merge, numbered by their
//!    lowest vertex (sheet, then id).
//! 4. **One vertex per sheet.** A component with at most one vertex of each sheet is one
//!    vertex. Its record is the vertex with the smallest reach, then the lowest point
//!    (x, y, z): an operand vertex, chosen by geometry alone, so the merged point does not
//!    depend on the sheets' order.
//! 5. **Chains.** A component where one sheet has two or more vertices (each pair beyond
//!    coincidence, or step 2 refused it) anchors every other vertex to its nearest vertex
//!    of that sheet, ties by id; each anchor keeps its own point. It is `Unsupported`
//!    with the chain when a second sheet has two vertices too, when a vertex lies beyond
//!    coincidence of its nearest anchor, or when two vertices of one sheet anchor to one.
//!
//! Nothing here depends on the order pairs are found in: the pairs are a set, and every
//! choice is by geometry, then by sheet and id.

use std::collections::BTreeMap;

use scx_brep::{Operand, VertexId};
use scx_num::budget::{Budget, Stop};
use scx_num::vector3d::Point3;

use crate::error::{StitchError, StitchRefusal, StitchUnsupported};

/// A vertex of a sheet, as identification reads it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Item {
    /// The sheet.
    pub(crate) operand: Operand,
    /// The vertex.
    pub(crate) vertex: VertexId,
    /// Its point.
    pub(crate) point: Point3,
    /// Its reach: its accuracy, at least the resolution's zone.
    pub(crate) reach: f64,
}

/// Vertices of several sheets that are one vertex of the result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Group {
    /// The items, in index order (sheet, then id); at most one per sheet.
    pub(crate) members: Vec<usize>,
    /// The item whose record is kept.
    pub(crate) kept: usize,
}

/// Work charged per vertex placed and per pair tested.
const UNIT: u64 = 1;

/// The groups of `items` (in sheet then id order) with two or more members, by their
/// lowest member.
pub(crate) fn identify(
    items: &[Item],
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<Group>, StitchError> {
    let pairs = pairs(items, budget, stop)?;
    if let Some(&(i, j)) = pairs
        .iter()
        .filter(|&&(i, j)| same_sheet(items, i, j))
        .min()
        && let (Some(a), Some(b)) = (items.get(i), items.get(j))
    {
        return Err(StitchError::Refused(
            StitchRefusal::VerticesWithinCoincidence {
                operand: a.operand,
                vertices: [a.vertex, b.vertex],
            },
        ));
    }
    let mut sets = Sets::new(items.len());
    for &(i, j) in &pairs {
        sets.join(i, j);
    }
    let mut comps: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..items.len() {
        if sets.size(i) > 1 {
            comps.entry(sets.root(i)).or_default().push(i);
        }
    }
    let mut out = Vec::new();
    for members in comps.into_values() {
        stop.check()?;
        settle(items, members, &mut out)?;
    }
    Ok(out)
}

fn same_sheet(items: &[Item], i: usize, j: usize) -> bool {
    matches!((items.get(i), items.get(j)), (Some(a), Some(b)) if a.operand == b.operand)
}

/// The distance within which `a` and `b` are one point: the larger reach.
fn coincidence(a: &Item, b: &Item) -> f64 {
    a.reach.max(b.reach)
}

/// The grid cell of `p` at cell size `cell`.
fn cell_of(p: Point3, cell: f64) -> [i64; 3] {
    // `as` saturates: far points share an end cell, which only adds candidates.
    [p.x, p.y, p.z].map(|c| (c / cell).floor() as i64)
}

/// Every pair `(i, j)`, `i < j`, within coincidence, in order.
fn pairs(
    items: &[Item],
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<(usize, usize)>, StitchError> {
    // Twice the largest coincidence: two points within it differ by at most one cell on
    // each axis, with half a cell to spare for the division's rounding.
    let cell = 2.0 * items.iter().map(|it| it.reach).fold(0.0, f64::max);
    let mut grid: BTreeMap<[i64; 3], Vec<usize>> = BTreeMap::new();
    for (i, it) in items.iter().enumerate() {
        grid.entry(cell_of(it.point, cell)).or_default().push(i);
    }
    budget.charge(UNIT * items.len() as u64)?;
    let mut out = Vec::new();
    for (i, a) in items.iter().enumerate() {
        if i % 1024 == 0 {
            stop.check()?;
        }
        let [x, y, z] = cell_of(a.point, cell);
        for dx in -1..=1_i64 {
            for dy in -1..=1_i64 {
                for dz in -1..=1_i64 {
                    let key = [
                        x.saturating_add(dx),
                        y.saturating_add(dy),
                        z.saturating_add(dz),
                    ];
                    let Some(cands) = grid.get(&key) else {
                        continue;
                    };
                    for &j in cands {
                        let Some(b) = items.get(j).filter(|_| j > i) else {
                            continue;
                        };
                        budget.charge(UNIT)?;
                        if a.point.distance(b.point) <= coincidence(a, b) {
                            out.push((i, j));
                        }
                    }
                }
            }
        }
    }
    // A saturated cell can be its own neighbour: keep each pair once.
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

/// The group or groups of one component, pushed onto `out`.
fn settle(items: &[Item], members: Vec<usize>, out: &mut Vec<Group>) -> Result<(), StitchError> {
    let mut by_sheet: BTreeMap<Operand, Vec<usize>> = BTreeMap::new();
    for &m in &members {
        if let Some(it) = items.get(m) {
            by_sheet.entry(it.operand).or_default().push(m);
        }
    }
    let mut dup = by_sheet.iter().filter(|(_, v)| v.len() > 1);
    let Some((&anchor_sheet, anchors)) = dup.next() else {
        let kept = members
            .iter()
            .copied()
            .min_by(|&i, &j| keep_order(items, i, j))
            .unwrap_or_default();
        out.push(Group { members, kept });
        return Ok(());
    };
    if let Some((&operand, two)) = dup.next() {
        return Err(chain(items, operand, two));
    }
    let anchors = anchors.clone();
    let mut groups: BTreeMap<usize, Vec<usize>> = anchors.iter().map(|&a| (a, vec![a])).collect();
    for &m in &members {
        let Some(it) = items.get(m).filter(|it| it.operand != anchor_sheet) else {
            continue;
        };
        let nearest = anchors
            .iter()
            .copied()
            .filter_map(|a| items.get(a).map(|at| (a, at)))
            .min_by(|(i, p), (j, q)| {
                it.point
                    .distance(p.point)
                    .total_cmp(&it.point.distance(q.point))
                    .then(i.cmp(j))
            });
        let Some((a, at)) = nearest else {
            continue;
        };
        if it.point.distance(at.point) > coincidence(it, at) {
            return Err(chain(items, anchor_sheet, &anchors));
        }
        let group = groups.entry(a).or_default();
        if let Some(&twin) = group
            .iter()
            .find(|&&g| items.get(g).is_some_and(|g| g.operand == it.operand))
        {
            return Err(chain(items, it.operand, &[twin, m]));
        }
        group.push(m);
    }
    for (kept, mut members) in groups {
        if members.len() > 1 {
            members.sort_unstable();
            out.push(Group { members, kept });
        }
    }
    Ok(())
}

/// The order a component's kept vertex is chosen by: the smallest reach, then the lowest
/// point, then sheet and id.
fn keep_order(items: &[Item], i: usize, j: usize) -> core::cmp::Ordering {
    match (items.get(i), items.get(j)) {
        (Some(a), Some(b)) => a
            .reach
            .total_cmp(&b.reach)
            .then(a.point.x.total_cmp(&b.point.x))
            .then(a.point.y.total_cmp(&b.point.y))
            .then(a.point.z.total_cmp(&b.point.z))
            .then(i.cmp(&j)),
        _ => i.cmp(&j),
    }
}

/// The chain error naming the first two of `of`, vertices of `operand`.
fn chain(items: &[Item], operand: Operand, of: &[usize]) -> StitchError {
    let mut ids: Vec<VertexId> = of
        .iter()
        .filter_map(|&i| items.get(i).map(|it| it.vertex))
        .collect();
    ids.sort_unstable();
    let (v, w) = match ids.as_slice() {
        [v, w, ..] => (*v, *w),
        [v] => (*v, *v),
        [] => (VertexId::from_raw(0), VertexId::from_raw(0)),
    };
    StitchError::Unsupported(StitchUnsupported::Chain {
        operand,
        vertices: [v, w],
    })
}

/// Disjoint sets with path compression; a set's root is its lowest member, so components
/// are numbered by their lowest member (sheet, then id).
struct Sets {
    parent: Vec<usize>,
    size: Vec<usize>,
}

impl Sets {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            size: vec![1; n],
        }
    }

    fn root(&mut self, mut i: usize) -> usize {
        let mut r = i;
        while let Some(&p) = self.parent.get(r) {
            if p == r {
                break;
            }
            r = p;
        }
        while let Some(&p) = self.parent.get(i) {
            if p == r {
                break;
            }
            if let Some(slot) = self.parent.get_mut(i) {
                *slot = r;
            }
            i = p;
        }
        r
    }

    fn size(&mut self, i: usize) -> usize {
        let r = self.root(i);
        self.size.get(r).copied().unwrap_or(1)
    }

    fn join(&mut self, i: usize, j: usize) {
        let (a, b) = (self.root(i), self.root(j));
        if a == b {
            return;
        }
        // The lower index is the root: the root is the component's lowest member.
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        let total =
            self.size.get(lo).copied().unwrap_or(1) + self.size.get(hi).copied().unwrap_or(1);
        if let Some(p) = self.parent.get_mut(hi) {
            *p = lo;
        }
        if let Some(s) = self.size.get_mut(lo) {
            *s = total;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn item(sheet: u32, id: u32, x: f64, y: f64, reach: f64) -> Item {
        Item {
            operand: Operand::from_raw(sheet),
            vertex: VertexId::from_raw(id),
            point: Point3::new(x, y, 0.0),
            reach,
        }
    }

    fn run(items: &[Item]) -> Result<Vec<Group>, StitchError> {
        identify(items, &mut Budget::unlimited(), &Stop::new())
    }

    const Z: f64 = 1e-5;

    #[test]
    fn a_chain_anchors_to_the_nearest_vertex_of_its_sheet() {
        // Sheet 0's a and c are 1 apart; sheet 1's b reaches both, nearer a.
        let items = [
            item(0, 0, 0.0, 0.0, Z),
            item(0, 1, 0.0, 1.0, Z),
            item(1, 0, 0.0, 0.4, 0.6),
        ];
        let groups = run(&items).unwrap();
        assert_eq!(
            groups,
            vec![Group {
                members: vec![0, 2],
                kept: 0
            }]
        );
    }

    #[test]
    fn a_chain_with_two_sheets_doubled_is_unsupported() {
        let items = [
            item(0, 0, 0.0, 0.0, Z),
            item(0, 1, 0.0, 1.0, Z),
            item(1, 0, 0.0, 0.4, 0.6),
            item(1, 1, 0.0, 1.5, 0.6),
        ];
        let e = run(&items).unwrap_err();
        assert!(
            matches!(e, StitchError::Unsupported(StitchUnsupported::Chain { .. })),
            "{e:?}"
        );
    }

    #[test]
    fn two_of_one_sheet_on_one_anchor_is_unsupported() {
        // Sheet 0 has a and c far apart; sheet 1 has two vertices both nearest a, joined
        // to c only through sheet 2's long reach.
        let items = [
            item(0, 0, 0.0, 0.0, Z),
            item(0, 1, 0.0, 3.0, Z),
            item(1, 0, 0.0, 0.5, 0.6),
            item(1, 1, 0.0, -0.5, 0.6),
            item(2, 0, 0.0, 1.6, 1.5),
        ];
        let e = run(&items).unwrap_err();
        assert!(
            matches!(e, StitchError::Unsupported(StitchUnsupported::Chain { .. })),
            "{e:?}"
        );
    }

    #[test]
    fn a_pair_of_one_sheet_is_refused_lowest_first() {
        let items = [
            item(0, 0, 0.0, 0.0, Z),
            item(1, 0, 5.0, 0.0, 0.1),
            item(1, 1, 5.05, 0.0, Z),
        ];
        let e = run(&items).unwrap_err();
        assert_eq!(
            e,
            StitchError::Refused(StitchRefusal::VerticesWithinCoincidence {
                operand: Operand::from_raw(1),
                vertices: [VertexId::from_raw(0), VertexId::from_raw(1)],
            })
        );
    }

    #[test]
    fn the_kept_vertex_is_chosen_by_geometry() {
        let a = item(0, 0, 0.0, 0.0, Z);
        let b = item(1, 0, -0.5 * Z, 0.0, Z);
        let g = run(&[a, b]).unwrap();
        assert_eq!(g[0].kept, 1, "the lower point");
        let g = run(&[b, a]).unwrap();
        assert_eq!(g[0].kept, 0, "the same point, whatever the order");
    }
}
