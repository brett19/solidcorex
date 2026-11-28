// Written for solidcorex (docs/01-architecture.md §2 layer table, §10.1 item 8). Apache-2.0.
//! A box tree: a static bounding-box hierarchy over a slice of boxes, for the candidate
//! searches of the layers above (which faces may meet a box, which pairs of faces may
//! meet).
//!
//! The contract is exact: [`BoxTree::overlapping`] reports item `i` exactly when
//! `boxes[i].are_disjoint(q, tol)` is false, and [`BoxTree::pairs`] reports `(i, j)` exactly
//! when `a[i].are_disjoint(b[j], tol)` is false. The tree only skips work. A node's box is
//! the union of its items' boxes, so a node disjoint from the query has no item that is
//! not disjoint. Boxes with a NaN bound are not placed in the tree, because a union cannot
//! hold them; they are tested one by one on every query. Inside-out (empty) boxes are
//! placed, and add nothing to a union.
//!
//! The answer is a sorted `Vec` of item indices, or for [`BoxTree::overlapping_in`] a
//! sorted slice held in a caller's [`Overlaps`], reused across queries. It does not depend
//! on the tree's shape, and the shape depends only on the boxes and their order. The build splits each node at
//! the median of its items' centres, on the axis where the centres spread most, with ties
//! broken by item index. The median is found by selection, not a full sort: the key is a
//! strict total order, so the halves hold the same items either way. A half that becomes a
//! leaf is sorted, and so is a half whose union has a zero bound its items hold with both
//! signs (the one case where the union's bits depend on the order), so the tree is the one a
//! full sort per node builds, bit for bit.

use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::extent3d::Extent3d;
use scx_num::vector2d::Axis2;
use scx_num::vector3d::Axis;

/// An axis-aligned box the tree can hold: [`Extent1d`], [`Extent2d`] or [`Extent3d`].
pub trait Bounds: Copy {
    /// An axis of the box.
    type Axis: Copy + 'static;
    /// Every axis, in order.
    const AXES: &'static [Self::Axis];
    /// The lower bound on `axis`.
    fn lo(&self, axis: Self::Axis) -> f64;
    /// The upper bound on `axis`.
    fn hi(&self, axis: Self::Axis) -> f64;
    /// The smallest box holding both.
    #[must_use]
    fn union(&self, other: &Self) -> Self;
    /// Separated by more than `tol` on some axis (the extents' `are_disjoint`).
    fn are_disjoint(&self, other: &Self, tol: f64) -> bool;
}

impl Bounds for Extent1d {
    type Axis = ();
    const AXES: &'static [()] = &[()];
    #[inline]
    fn lo(&self, (): ()) -> f64 {
        self.min()
    }
    #[inline]
    fn hi(&self, (): ()) -> f64 {
        self.max()
    }
    #[inline]
    fn union(&self, other: &Self) -> Self {
        Extent1d::union(*self, *other)
    }
    #[inline]
    fn are_disjoint(&self, other: &Self, tol: f64) -> bool {
        Extent1d::are_disjoint(self, other, tol)
    }
}

impl Bounds for Extent2d {
    type Axis = Axis2;
    const AXES: &'static [Axis2] = &Axis2::ALL;
    #[inline]
    fn lo(&self, axis: Axis2) -> f64 {
        match axis {
            Axis2::X => self.x_min(),
            Axis2::Y => self.y_min(),
        }
    }
    #[inline]
    fn hi(&self, axis: Axis2) -> f64 {
        match axis {
            Axis2::X => self.x_max(),
            Axis2::Y => self.y_max(),
        }
    }
    #[inline]
    fn union(&self, other: &Self) -> Self {
        Extent2d::union(*self, *other)
    }
    #[inline]
    fn are_disjoint(&self, other: &Self, tol: f64) -> bool {
        Extent2d::are_disjoint(self, other, tol)
    }
}

impl Bounds for Extent3d {
    type Axis = Axis;
    const AXES: &'static [Axis] = &Axis::ALL;
    #[inline]
    fn lo(&self, axis: Axis) -> f64 {
        self.min_at(axis)
    }
    #[inline]
    fn hi(&self, axis: Axis) -> f64 {
        self.max_at(axis)
    }
    #[inline]
    fn union(&self, other: &Self) -> Self {
        Extent3d::union(*self, *other)
    }
    #[inline]
    fn are_disjoint(&self, other: &Self, tol: f64) -> bool {
        Extent3d::are_disjoint(self, other, tol)
    }
}

/// The most items in a leaf.
const LEAF: usize = 4;

#[derive(Debug, Clone, Copy)]
struct Node<B> {
    /// The union of the items' boxes.
    bounds: B,
    /// The items: `order[start..end]`.
    start: usize,
    end: usize,
    /// The two children, or `None` for a leaf.
    children: Option<(usize, usize)>,
}

/// A static box tree over a slice of boxes. See the module documentation for the contract.
#[derive(Debug, Clone)]
pub struct BoxTree<B> {
    boxes: Vec<B>,
    nodes: Vec<Node<B>>,
    /// The placed items, grouped by node.
    order: Vec<usize>,
    /// The items with a NaN bound, tested one by one.
    unplaced: Vec<usize>,
}

fn has_nan<B: Bounds>(b: &B) -> bool {
    B::AXES
        .iter()
        .any(|&a| b.lo(a).is_nan() || b.hi(a).is_nan())
}

/// The union of `items`' boxes, folded in their order, or `None` when there are none.
fn union_of<B: Bounds>(boxes: &[B], items: &[usize]) -> Option<B> {
    let mut boxes = items.iter().filter_map(|&i| boxes.get(i));
    let first = *boxes.next()?;
    Some(boxes.fold(first, |u, b| u.union(b)))
}

/// Some bound of `u`, the union of `items`' boxes, is a zero that an item holds with the
/// other sign: the one case in which the union's bits depend on the order of the fold.
fn zero_tie<B: Bounds>(boxes: &[B], items: &[usize], u: &B) -> bool {
    let differs = |x: f64, y: f64| x == 0.0 && y == 0.0 && x.to_bits() != y.to_bits();
    B::AXES.iter().any(|&a| {
        (u.lo(a) == 0.0 || u.hi(a) == 0.0)
            && items
                .iter()
                .filter_map(|&i| boxes.get(i))
                .any(|b| differs(u.lo(a), b.lo(a)) || differs(u.hi(a), b.hi(a)))
    })
}

fn centre<B: Bounds>(b: &B, axis: B::Axis) -> f64 {
    0.5 * (b.lo(axis) + b.hi(axis))
}

/// The axis, of `0..dims`, on which the values `values(axis)` spread most (max minus min),
/// the first of the widest.
fn widest<I: Iterator<Item = f64>>(dims: usize, values: impl Fn(usize) -> I) -> usize {
    let mut axis = 0;
    let mut spread = f64::NEG_INFINITY;
    for a in 0..dims {
        // The ends `f64::min` and `max` give (a NaN value fails both tests, as they skip
        // it), but for the sign of a zero end, which cannot change a comparison of spreads.
        let (lo, hi) = values(a).fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), c| {
            (if c < lo { c } else { lo }, if c > hi { c } else { hi })
        });
        if hi - lo > spread {
            spread = hi - lo;
            axis = a;
        }
    }
    axis
}

/// The split keys of a build: every item's centre on every axis, computed once, and a
/// buffer of `(key, item)` pairs that selection reorders, so a comparison reads two
/// adjacent pairs rather than two boxes.
struct Keys {
    /// Item `i`'s centre on axis `a` (the `a`-th of [`Bounds::AXES`]) at `i * dims + a`.
    centres: Vec<f64>,
    dims: usize,
    keyed: Vec<(i64, usize)>,
}

/// `x` as an integer in the order of [`f64::total_cmp`] (the same map that method makes
/// before it compares), so the split order is a plain integer order: by key, then by item
/// index, a strict total order.
fn total_key(x: f64) -> i64 {
    let bits = x.to_bits() as i64;
    bits ^ (((bits >> 63) as u64) >> 1) as i64
}

impl Keys {
    fn new<B: Bounds>(boxes: &[B]) -> Self {
        let dims = B::AXES.len();
        let mut centres = Vec::with_capacity(boxes.len() * dims);
        for b in boxes {
            centres.extend(B::AXES.iter().map(|&a| centre(b, a)));
        }
        Self {
            centres,
            dims,
            keyed: Vec::new(),
        }
    }

    fn centre(&self, i: usize, axis: usize) -> f64 {
        self.centres
            .get(i * self.dims + axis)
            .copied()
            .unwrap_or(0.0)
    }

    /// Fills the buffer with `items` and their keys on `axis`.
    fn fill(&mut self, items: &[usize], axis: usize) {
        self.keyed.clear();
        let centres = &self.centres;
        let dims = self.dims;
        self.keyed.extend(items.iter().map(|&i| {
            (
                total_key(centres.get(i * dims + axis).copied().unwrap_or(0.0)),
                i,
            )
        }));
    }

    /// Writes the buffer's items back to `items`.
    fn write(&self, items: &mut [usize]) {
        for (slot, &(_, i)) in items.iter_mut().zip(&self.keyed) {
            *slot = i;
        }
    }

    /// Reorders `items` so the first `half` are the smallest on the axis where the centres
    /// spread most, each half that holds at most `leaf` items sorted; returns the axis.
    fn select(&mut self, items: &mut [usize], half: usize, leaf: usize) -> usize {
        let this = &*self;
        let axis = widest(this.dims, |a| items.iter().map(move |&i| this.centre(i, a)));
        self.fill(items, axis);
        // The key is a strict total order, so selection puts the same items in each half
        // as a full sort.
        self.keyed.select_nth_unstable(half);
        let (lo, hi) = self.keyed.split_at_mut(half);
        for part in [lo, hi] {
            // A half that will be a leaf is sorted, so its items stand in the order a full
            // sort gives; a larger half is reordered by its own split.
            if part.len() <= leaf {
                part.sort_unstable();
            }
        }
        self.write(items);
        axis
    }

    /// Sorts `items` by their keys on `axis`.
    fn sort(&mut self, items: &mut [usize], axis: usize) {
        self.fill(items, axis);
        self.keyed.sort_unstable();
        self.write(items);
    }
}

/// The working space of repeated [`BoxTree::overlapping_in`] queries: the walk's stack and
/// the answer, kept between queries. It holds no state a query reads: each query clears
/// it first, so any space gives the same answer.
#[derive(Debug, Clone, Default)]
pub struct Overlaps {
    stack: Vec<usize>,
    found: Vec<usize>,
}

impl Overlaps {
    /// An empty space; its buffers grow on the first queries.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            stack: Vec::new(),
            found: Vec::new(),
        }
    }
}

impl<B: Bounds> BoxTree<B> {
    /// The tree over `boxes`; item `i` is `boxes[i]`.
    #[must_use]
    pub fn new(boxes: &[B]) -> Self {
        Self::build::<false>(boxes)
    }

    /// The tree over `boxes`, its nodes split by selection, or by a full sort with
    /// `FULL_SORT`.
    fn build<const FULL_SORT: bool>(boxes: &[B]) -> Self {
        let mut order = Vec::with_capacity(boxes.len());
        let mut unplaced = Vec::new();
        for (i, b) in boxes.iter().enumerate() {
            if has_nan(b) {
                unplaced.push(i);
            } else {
                order.push(i);
            }
        }
        let mut tree = Self {
            boxes: boxes.to_vec(),
            // A node splits only above `LEAF` items, so every leaf but a lone item's holds
            // at least two: at most `n` nodes for `n` items.
            nodes: Vec::with_capacity(order.len()),
            order,
            unplaced,
        };
        let mut keys = Keys::new(boxes);
        if let Some(root) = tree.node(0, tree.order.len()) {
            tree.nodes.push(root);
            let mut stack = vec![0];
            while let Some(k) = stack.pop() {
                if let Some(children) = tree.split::<FULL_SORT>(k, &mut keys) {
                    stack.push(children.1);
                    stack.push(children.0);
                }
            }
        }
        tree
    }

    /// The number of items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.boxes.len()
    }

    /// No items.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty()
    }

    /// The union of the placed boxes (those without a NaN bound), if there are any.
    #[must_use]
    pub fn bounds(&self) -> Option<B> {
        self.nodes.first().map(|n| n.bounds)
    }

    /// The items `order[start..end]` as a leaf, or `None` when there are none.
    fn node(&self, start: usize, end: usize) -> Option<Node<B>> {
        Some(Node {
            bounds: union_of(&self.boxes, self.order.get(start..end)?)?,
            start,
            end,
            children: None,
        })
    }

    /// Splits node `k` if it holds more than [`LEAF`] items; returns its children. The
    /// halves come from selection over the items' keys; with `FULL_SORT` (the tests'
    /// reference build) from a full sort of the node by keys computed from the boxes, which
    /// gives the same tree.
    fn split<const FULL_SORT: bool>(
        &mut self,
        k: usize,
        keys: &mut Keys,
    ) -> Option<(usize, usize)> {
        let Node { start, end, .. } = *self.nodes.get(k)?;
        if end - start <= LEAF {
            return None;
        }
        let boxes = &self.boxes;
        let items = self.order.get_mut(start..end)?;
        let half = (end - start) / 2;
        let mut axis = 0;
        if FULL_SORT {
            // The reference: the spread by `f64::min` and `max`, the keys from the boxes.
            let mut axis = *B::AXES.first()?;
            let mut spread = f64::NEG_INFINITY;
            for &a in B::AXES {
                let (lo, hi) = items
                    .iter()
                    .filter_map(|&i| boxes.get(i))
                    .map(|b| centre(b, a))
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), c| {
                        (lo.min(c), hi.max(c))
                    });
                if hi - lo > spread {
                    spread = hi - lo;
                    axis = a;
                }
            }
            let key = |i: usize| boxes.get(i).map_or(0.0, |b| centre(b, axis));
            items.sort_unstable_by(|p, q| key(*p).total_cmp(&key(*q)).then(p.cmp(q)));
        } else {
            axis = keys.select(items, half, LEAF);
        }
        let mut halves = [None, None];
        let (lo, hi) = items.split_at_mut(half);
        for (part, out) in [lo, hi].into_iter().zip(&mut halves) {
            let mut bounds = union_of(boxes, part);
            // The union is the same for any order of the items but for the sign of a zero
            // bound, when both zeros are bounds (`f64::min` may return either). Then the half
            // is sorted and its union taken again, in the full sort's order.
            if !FULL_SORT && part.len() > LEAF && bounds.is_some_and(|u| zero_tie(boxes, part, &u))
            {
                keys.sort(part, axis);
                bounds = union_of(boxes, part);
            }
            *out = bounds;
        }
        let mid = start + half;
        let [Some(lb), Some(rb)] = halves else {
            return None;
        };
        let left = Node {
            bounds: lb,
            start,
            end: mid,
            children: None,
        };
        let right = Node {
            bounds: rb,
            start: mid,
            end,
            children: None,
        };
        let l = self.nodes.len();
        self.nodes.push(left);
        self.nodes.push(right);
        if let Some(n) = self.nodes.get_mut(k) {
            n.children = Some((l, l + 1));
        }
        Some((l, l + 1))
    }

    fn items(&self, n: &Node<B>) -> &[usize] {
        self.order.get(n.start..n.end).unwrap_or(&[])
    }

    fn meets(&self, i: usize, q: &B, tol: f64) -> bool {
        self.boxes.get(i).is_some_and(|b| !b.are_disjoint(q, tol))
    }

    /// `a` is not disjoint from item `j`'s box (`a` tested against it).
    fn met_by(&self, j: usize, a: &B, tol: f64) -> bool {
        self.boxes.get(j).is_some_and(|b| !a.are_disjoint(b, tol))
    }

    /// The items whose box is not disjoint from `q` within `tol`, ascending.
    #[must_use]
    pub fn overlapping(&self, q: &B, tol: f64) -> Vec<usize> {
        let mut space = Overlaps::new();
        self.walk(q, tol, &mut space);
        space.found
    }

    /// [`overlapping`](Self::overlapping), its answer held in `space`, whose buffers are
    /// kept from one query to the next: a loop of queries allocates only when an answer or
    /// the walk outgrows every earlier one. The answer is the same as `overlapping`'s.
    pub fn overlapping_in<'s>(&self, q: &B, tol: f64, space: &'s mut Overlaps) -> &'s [usize] {
        self.walk(q, tol, space);
        &space.found
    }

    /// The query: `space.found` cleared and filled with the answer, ascending.
    fn walk(&self, q: &B, tol: f64, space: &mut Overlaps) {
        let Overlaps { stack, found } = space;
        stack.clear();
        found.clear();
        if !self.nodes.is_empty() {
            stack.push(0);
        }
        while let Some(k) = stack.pop() {
            let Some(n) = self.nodes.get(k) else {
                continue;
            };
            if n.bounds.are_disjoint(q, tol) {
                continue;
            }
            match n.children {
                Some((a, b)) => {
                    stack.push(b);
                    stack.push(a);
                }
                None => found.extend(self.items(n).iter().filter(|&&i| self.meets(i, q, tol))),
            }
        }
        found.extend(self.unplaced.iter().filter(|&&i| self.meets(i, q, tol)));
        found.sort_unstable();
    }

    /// The pairs `(i, j)` of an item of `self` and an item of `other` whose boxes are not
    /// disjoint within `tol` (`self`'s box tested against `other`'s), ascending.
    #[must_use]
    pub fn pairs(&self, other: &Self, tol: f64) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut stack: Vec<(usize, usize)> = if self.nodes.is_empty() || other.nodes.is_empty() {
            Vec::new()
        } else {
            vec![(0, 0)]
        };
        while let Some((ka, kb)) = stack.pop() {
            let (Some(na), Some(nb)) = (self.nodes.get(ka), other.nodes.get(kb)) else {
                continue;
            };
            if na.bounds.are_disjoint(&nb.bounds, tol) {
                continue;
            }
            let descend_self = match (na.children, nb.children) {
                (None, None) => {
                    for &i in self.items(na) {
                        let Some(bi) = self.boxes.get(i) else {
                            continue;
                        };
                        for &j in other.items(nb) {
                            if other.met_by(j, bi, tol) {
                                out.push((i, j));
                            }
                        }
                    }
                    continue;
                }
                (Some(_), None) => true,
                (None, Some(_)) => false,
                // Descend the side with more items.
                (Some(_), Some(_)) => na.end - na.start >= nb.end - nb.start,
            };
            if descend_self {
                if let Some((a0, a1)) = na.children {
                    stack.push((a1, kb));
                    stack.push((a0, kb));
                }
            } else if let Some((b0, b1)) = nb.children {
                stack.push((ka, b1));
                stack.push((ka, b0));
            }
        }
        // Pairs with an unplaced item on either side, one by one.
        for &i in &self.unplaced {
            if let Some(bi) = self.boxes.get(i) {
                out.extend(
                    (0..other.len())
                        .filter(|&j| other.met_by(j, bi, tol))
                        .map(|j| (i, j)),
                );
            }
        }
        for &i in &self.order {
            if let Some(bi) = self.boxes.get(i) {
                out.extend(
                    other
                        .unplaced
                        .iter()
                        .filter(|&&j| other.met_by(j, bi, tol))
                        .map(|&j| (i, j)),
                );
            }
        }
        out.sort_unstable();
        out
    }

    /// The pairs `i < j` of items whose boxes are not disjoint within `tol` (`boxes[i]`
    /// tested against `boxes[j]`), ascending.
    ///
    /// A self-join: each unordered pair is reached once. A node against itself goes to its
    /// children against themselves and to its two children against each other, whose items
    /// are distinct.
    #[must_use]
    pub fn self_pairs(&self, tol: f64) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        // `(k, k)` is node `k` against itself; `(ka, kb)` with `ka != kb`, two disjoint sets.
        let mut stack: Vec<(usize, usize)> = if self.nodes.is_empty() {
            Vec::new()
        } else {
            vec![(0, 0)]
        };
        while let Some((ka, kb)) = stack.pop() {
            let (Some(na), Some(nb)) = (self.nodes.get(ka), self.nodes.get(kb)) else {
                continue;
            };
            if ka == kb {
                // Both items of a pair lie in the node's box, so a box disjoint from itself
                // holds no pair.
                if na.bounds.are_disjoint(&na.bounds, tol) {
                    continue;
                }
                if let Some((a, b)) = na.children {
                    stack.push((a, b));
                    stack.push((b, b));
                    stack.push((a, a));
                } else {
                    let items = self.items(na);
                    for (p, &i) in items.iter().enumerate() {
                        for &j in items.get(p + 1..).unwrap_or(&[]) {
                            self.push_self_pair(&mut out, i, j, tol);
                        }
                    }
                }
                continue;
            }
            // A pair is tested lower index first, and that item may be on either side, so
            // the nodes are skipped only when disjoint in both orders.
            if na.bounds.are_disjoint(&nb.bounds, tol) && nb.bounds.are_disjoint(&na.bounds, tol) {
                continue;
            }
            let descend_a = match (na.children, nb.children) {
                (None, None) => {
                    for &i in self.items(na) {
                        for &j in self.items(nb) {
                            self.push_self_pair(&mut out, i, j, tol);
                        }
                    }
                    continue;
                }
                (Some(_), None) => true,
                (None, Some(_)) => false,
                // Descend the side with more items.
                (Some(_), Some(_)) => na.end - na.start >= nb.end - nb.start,
            };
            if descend_a {
                if let Some((a0, a1)) = na.children {
                    stack.push((a1, kb));
                    stack.push((a0, kb));
                }
            } else if let Some((b0, b1)) = nb.children {
                stack.push((ka, b1));
                stack.push((ka, b0));
            }
        }
        // Pairs with an unplaced item, one by one.
        for (p, &u) in self.unplaced.iter().enumerate() {
            for &j in &self.order {
                self.push_self_pair(&mut out, u, j, tol);
            }
            for &v in self.unplaced.get(p + 1..).unwrap_or(&[]) {
                self.push_self_pair(&mut out, u, v, tol);
            }
        }
        out.sort_unstable();
        out
    }

    /// Pushes the distinct items `i` and `j` as `(lower, higher)` if the lower's box is not
    /// disjoint from the higher's.
    fn push_self_pair(&self, out: &mut Vec<(usize, usize)>, i: usize, j: usize, tol: f64) {
        let (i, j) = if i < j { (i, j) } else { (j, i) };
        if self.boxes.get(i).is_some_and(|bi| self.met_by(j, bi, tol)) {
            out.push((i, j));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scx_num::hash::Rng;
    use scx_num::vector2d::Point2;
    use scx_num::vector3d::{Point3, Vec3};

    fn random_boxes(rng: &mut Rng, n: usize) -> Vec<Extent3d> {
        (0..n)
            .map(|_| {
                let c = Point3::new(
                    10.0 * rng.next_signed_unit(),
                    10.0 * rng.next_signed_unit(),
                    rng.next_signed_unit(),
                );
                let h = Vec3::new(rng.next_f64(), rng.next_f64(), 0.1 * rng.next_f64());
                Extent3d::spanning(c - h, c + h).unwrap()
            })
            .collect()
    }

    fn brute(boxes: &[Extent3d], q: &Extent3d, tol: f64) -> Vec<usize> {
        (0..boxes.len())
            .filter(|&i| !boxes[i].are_disjoint(q, tol))
            .collect()
    }

    fn brute_pairs(a: &[Extent3d], b: &[Extent3d], tol: f64) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (i, x) in a.iter().enumerate() {
            for (j, y) in b.iter().enumerate() {
                if !x.are_disjoint(y, tol) {
                    out.push((i, j));
                }
            }
        }
        out
    }

    /// The pairs as before the self-join: `pairs(self, self)`, keeping `i < j`.
    fn self_pairs_by_pairs<B: Bounds>(t: &BoxTree<B>, tol: f64) -> Vec<(usize, usize)> {
        let mut out = t.pairs(t, tol);
        out.retain(|&(i, j)| i < j);
        out
    }

    /// Every bound's bits.
    fn bits<B: Bounds>(b: &B) -> Vec<(u64, u64)> {
        B::AXES
            .iter()
            .map(|&a| (b.lo(a).to_bits(), b.hi(a).to_bits()))
            .collect()
    }

    /// The tree built by selection is the one a full sort per node builds, field by field
    /// and bit for bit: the same nodes in the same order (ranges, children, boxes), the
    /// same item order (so the same leaves), the same unplaced items.
    fn assert_same_tree<B: Bounds>(boxes: &[B], what: &str) {
        let (a, b) = (BoxTree::new(boxes), BoxTree::build::<true>(boxes));
        assert_eq!(a.order, b.order, "{what}");
        assert_eq!(a.unplaced, b.unplaced, "{what}");
        assert_eq!(
            a.boxes.iter().map(bits).collect::<Vec<_>>(),
            b.boxes.iter().map(bits).collect::<Vec<_>>(),
            "{what}"
        );
        assert_eq!(a.nodes.len(), b.nodes.len(), "{what}");
        for (k, (x, y)) in a.nodes.iter().zip(&b.nodes).enumerate() {
            assert_eq!(
                (x.start, x.end, x.children),
                (y.start, y.end, y.children),
                "{what} node {k}"
            );
            assert_eq!(bits(&x.bounds), bits(&y.bounds), "{what} node {k}");
        }
    }

    /// The self-join gives the same pairs, in the same order, as `pairs(self, self)`.
    fn assert_same_self_pairs<B: Bounds>(boxes: &[B], what: &str) {
        let t = BoxTree::new(boxes);
        for tol in [0.0, 0.1, 0.5, -0.1, -0.0, 1e-300, f64::NAN] {
            assert_eq!(
                t.self_pairs(tol),
                self_pairs_by_pairs(&t, tol),
                "{what} tol {tol}"
            );
        }
    }

    /// Seeded box sets of every kind the build must order the same way: random; centres
    /// on a coarse grid (tied keys, different boxes); each box repeated (tied keys and
    /// equal boxes); all one box; bounds at `-0.0` and `+0.0` mixed (a union that could
    /// depend on the fold order); and decimal grid values that touch, where the two orders
    /// of a disjointness test can round differently.
    fn box_sets(seed: u64) -> Vec<(String, Vec<Extent3d>)> {
        let mut rng = Rng::new(seed);
        let mut out = Vec::new();
        for n in [0, 1, 4, 5, 8, 9, 10, 17, 64, 100, 257, 1000, 3000] {
            out.push((format!("random {n}"), random_boxes(&mut rng, n)));
            let grid: Vec<Extent3d> = (0..n)
                .map(|_| {
                    let mut g = || (4.0 * rng.next_f64()).floor();
                    let c = Point3::new(g(), g(), g());
                    let h = Vec3::new(rng.next_f64(), rng.next_f64(), rng.next_f64());
                    Extent3d::spanning(c - h, c + h).unwrap()
                })
                .collect();
            out.push((format!("tied {n}"), grid));
            let base = random_boxes(&mut rng, n / 3 + 1);
            let dup: Vec<Extent3d> = (0..n).map(|i| base[(i * 7) % base.len()]).collect();
            out.push((format!("repeated {n}"), dup));
            out.push((
                format!("one box {n}"),
                base.iter().take(1).cycle().take(n).copied().collect(),
            ));
            let zeros: Vec<Extent3d> = (0..n)
                .map(|i| {
                    let z = if (rng.next_u64() & 1) == 0 { 0.0 } else { -0.0 };
                    let y = rng.next_signed_unit();
                    let (lo, hi) = if i % 2 == 0 {
                        (z, rng.next_f64())
                    } else {
                        (-rng.next_f64(), -z)
                    };
                    Extent3d::from_bounds(lo, y, z, hi, y + 1.0, -z).unwrap()
                })
                .collect();
            out.push((format!("signed zeros {n}"), zeros));
            let decimal: Vec<Extent3d> = (0..n)
                .map(|_| {
                    let mut d = || f64::from(u32::try_from(rng.next_u64() % 30).unwrap()) * 0.1;
                    let (x, y, z) = (d(), d(), d());
                    Extent3d::from_bounds(x, y, z, x + 0.3, y + 0.1, z + 0.7).unwrap()
                })
                .collect();
            out.push((format!("decimal {n}"), decimal));
        }
        out
    }

    #[test]
    fn selection_builds_the_full_sort_tree() {
        for seed in [1, 2, 3, Rng::DEFAULT_SEED] {
            for (what, boxes) in box_sets(seed) {
                assert_same_tree(&boxes, &what);
                let flat: Vec<Extent1d> = boxes
                    .iter()
                    .map(|b| Extent1d::spanning(b.min().x, b.max().x).unwrap())
                    .collect();
                assert_same_tree(&flat, &what);
                let rects: Vec<Extent2d> = boxes
                    .iter()
                    .map(|b| {
                        Extent2d::spanning(
                            Point2::new(b.min().x, b.min().y),
                            Point2::new(b.max().x, b.max().y),
                        )
                        .unwrap()
                    })
                    .collect();
                assert_same_tree(&rects, &what);
            }
        }
    }

    #[test]
    fn self_join_matches_pairs_with_itself() {
        for seed in [4, 5] {
            for (what, boxes) in box_sets(seed) {
                if boxes.len() > 300 {
                    continue;
                }
                assert_same_self_pairs(&boxes, &what);
                let flat: Vec<Extent1d> = boxes
                    .iter()
                    .map(|b| Extent1d::spanning(b.min().x, b.max().x).unwrap())
                    .collect();
                assert_same_self_pairs(&flat, &what);
            }
        }
    }

    #[test]
    fn queries_match_brute_force() {
        let mut rng = Rng::new(Rng::DEFAULT_SEED);
        // One space for every query of every tree: what one query leaves in it cannot
        // change the next one's answer.
        let mut space = Overlaps::new();
        for n in [0, 1, 3, 4, 5, 17, 200] {
            let mut boxes = random_boxes(&mut rng, n);
            if n > 3 {
                // A point, and boxes at the far ends of the finite range (a box cannot hold
                // NaN or an infinite end).
                boxes[1] = Extent3d::from_point(Point3::new(0.0, 0.0, 0.0)).unwrap();
                boxes[2] = Extent3d::from_bounds(f64::MAX, 0.0, 0.0, f64::MAX, 1.0, 1.0).unwrap();
                boxes[3] = Extent3d::from_bounds(f64::MIN, -1.0, -1.0, f64::MAX, 1.0, 1.0).unwrap();
            }
            let tree = BoxTree::new(&boxes);
            assert_eq!(tree.len(), n);
            let queries = random_boxes(&mut rng, 50);
            for tol in [0.0, 0.5, -0.1, f64::NAN] {
                for q in &queries {
                    let want = brute(&boxes, q, tol);
                    assert_eq!(tree.overlapping(q, tol), want, "n {n} tol {tol}");
                    assert_eq!(
                        tree.overlapping_in(q, tol, &mut space),
                        want.as_slice(),
                        "n {n} tol {tol}"
                    );
                }
                let other = random_boxes(&mut rng, n / 2 + 1);
                let ot = BoxTree::new(&other);
                assert_eq!(tree.pairs(&ot, tol), brute_pairs(&boxes, &other, tol));
                assert_eq!(ot.pairs(&tree, tol), brute_pairs(&other, &boxes, tol));
                let selfp: Vec<_> = brute_pairs(&boxes, &boxes, tol)
                    .into_iter()
                    .filter(|&(i, j)| i < j)
                    .collect();
                assert_eq!(tree.self_pairs(tol), selfp);
            }
        }
    }

    #[test]
    fn other_dimensions() {
        let ivls: Vec<Extent1d> = (0..20)
            .map(|i| Extent1d::spanning(f64::from(i), f64::from(i) + 1.5).unwrap())
            .collect();
        let t = BoxTree::new(&ivls);
        assert_eq!(
            t.overlapping(&Extent1d::spanning(4.2, 4.3).unwrap(), 0.0),
            vec![3, 4]
        );
        let rects: Vec<Extent2d> = (0..20)
            .map(|i| {
                let x = f64::from(i);
                Extent2d::spanning(Point2::new(x, -x), Point2::new(x + 1.0, -x + 1.0)).unwrap()
            })
            .collect();
        let t = BoxTree::new(&rects);
        assert_eq!(
            t.overlapping(
                &Extent2d::spanning(Point2::new(5.5, -4.5), Point2::new(5.6, -4.4)).unwrap(),
                0.0
            ),
            vec![5]
        );
        assert_eq!(t.self_pairs(0.0).len(), 19);
    }

    #[test]
    fn the_answer_does_not_depend_on_the_input_order() {
        let mut rng = Rng::new(7);
        let boxes = random_boxes(&mut rng, 300);
        let mut rev = boxes.clone();
        rev.reverse();
        let n = boxes.len();
        let (a, b) = (BoxTree::new(&boxes), BoxTree::new(&rev));
        let mut from_rev: Vec<(usize, usize)> = b
            .self_pairs(0.0)
            .into_iter()
            .map(|(i, j)| (n - 1 - j, n - 1 - i))
            .collect();
        from_rev.sort_unstable();
        assert_eq!(a.self_pairs(0.0), from_rev);
        assert_eq!(a.bounds().map(|e| e.min()), b.bounds().map(|e| e.min()));
    }
}
