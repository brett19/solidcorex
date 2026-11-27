// Written for solidcorex. Apache-2.0.
//! Body cells (research 63 §6.9): what crosses records, cached beside a finished body's
//! tables and filled on first read.
//!
//! - The adjacency a record does not store: each vertex's star, each shell's face sides,
//!   each region's shells, as compressed rows by slot, each row in id order. The views walk
//!   a row where they would scan a table.
//! - Every face's, edge's and vertex's box (grown by its accuracy and the resolution), a box
//!   tree over the faces' boxes, and the body's box.
//!
//! A [`Brep`](crate::Brep) holds its body cells behind an `Arc`, made fresh by `finish` and
//! `compact` and shared by its clones, which share its tables; a draft has none, so no
//! write has to reset one.

use core::fmt;
use std::sync::OnceLock;

use scx_num::extent3d::Extent3d;
use scx_num::units::Resolution;

use crate::id::{EdgeId, FaceId, ShellId};
use crate::record::FaceSide;
use crate::table::Id;
use crate::tables::Tables;

/// The body cells of one finished body.
#[derive(Default)]
pub(crate) struct BodyCells {
    adjacency: OnceLock<Adjacency>,
    boxes: OnceLock<Boxes>,
}

impl fmt::Debug for BodyCells {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BodyCells")
            .field("adjacency", &self.adjacency.get().is_some())
            .field("boxes", &self.boxes.get().is_some())
            .finish()
    }
}

impl BodyCells {
    /// The adjacency rows of `t`, built on first read.
    pub(crate) fn adjacency(&self, t: &Tables) -> &Adjacency {
        self.adjacency.get_or_init(|| Adjacency::build(t))
    }

    /// The boxes of `t` under `resolution`, built on first read.
    pub(crate) fn boxes(&self, t: &Tables, resolution: Resolution) -> &Boxes {
        self.boxes.get_or_init(|| Boxes::build(t, resolution))
    }

    /// Whether nothing is filled.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.adjacency.get().is_none() && self.boxes.get().is_none()
    }
}

/// Rows of items by slot: row `i` is `items[start[i]..start[i + 1]]`.
#[derive(Debug)]
pub(crate) struct Rows<T> {
    start: Vec<u32>,
    items: Vec<T>,
}

impl<T: Copy> Rows<T> {
    /// Rows for `slots` slots from `(slot, item)` pairs, each row keeping the pairs' order.
    fn build(slots: usize, pairs: impl Iterator<Item = (usize, T)> + Clone) -> Self {
        let mut start = vec![0_u32; slots + 1];
        for (s, _) in pairs.clone() {
            if let Some(c) = start.get_mut(s + 1) {
                *c += 1;
            }
        }
        let mut sum = 0;
        for c in &mut start {
            sum += *c;
            *c = sum;
        }
        let mut fill: Vec<u32> = start.clone();
        let mut items: Vec<Option<T>> = vec![None; sum as usize];
        for (s, item) in pairs {
            if let Some(at) = fill.get_mut(s)
                && let Some(slot) = items.get_mut(*at as usize)
            {
                *slot = Some(item);
                *at += 1;
            }
        }
        Self {
            start,
            items: items.into_iter().flatten().collect(),
        }
    }

    /// Row `slot`; empty past the last.
    pub(crate) fn row(&self, slot: usize) -> &[T] {
        let (Some(&a), Some(&b)) = (self.start.get(slot), self.start.get(slot + 1)) else {
            return &[];
        };
        self.items.get(a as usize..b as usize).unwrap_or(&[])
    }

    #[cfg(test)]
    fn bytes(&self) -> usize {
        self.start.capacity() * size_of::<u32>() + self.items.capacity() * size_of::<T>()
    }
}

/// The adjacency a finished body's records do not store.
#[derive(Debug)]
pub(crate) struct Adjacency {
    /// By vertex: the edges that start or end there, each once.
    pub(crate) stars: Rows<EdgeId>,
    /// By shell: its face sides, front before back.
    pub(crate) shell_faces: Rows<(FaceId, FaceSide)>,
    /// By region: its shells.
    pub(crate) region_shells: Rows<ShellId>,
}

impl Adjacency {
    fn build(t: &Tables) -> Self {
        let stars = t.edges.iter().flat_map(|(e, g)| {
            let [a, b] = g.ends;
            let second = (a != b).then_some((b.slot(), e));
            core::iter::once((a.slot(), e)).chain(second)
        });
        let faces = t.faces.iter().flat_map(|(f, rec)| {
            let sides = rec.sides;
            FaceSide::BOTH
                .into_iter()
                .zip(sides)
                .map(move |(side, s)| (s.slot(), (f, side)))
        });
        let shells = t.shells.iter().map(|(s, rec)| (rec.region.slot(), s));
        Self {
            stars: Rows::build(t.vertices.slots(), stars),
            shell_faces: Rows::build(t.shells.slots(), faces),
            region_shells: Rows::build(t.regions.slots(), shells),
        }
    }

    #[cfg(test)]
    pub(crate) fn bytes(&self) -> usize {
        self.stars.bytes() + self.shell_faces.bytes() + self.region_shells.bytes()
    }
}

/// Faces per leaf of the face box tree.
const LEAF: usize = 4;

/// The boxes of a finished body.
#[derive(Debug)]
pub(crate) struct Boxes {
    /// The bounded faces' grown boxes, in tree order.
    faces: Vec<(FaceId, Extent3d)>,
    /// The faces with no box (unbounded, or not enclosed), in id order.
    unboxed: Vec<FaceId>,
    nodes: Vec<Node>,
    /// The body's box: `None` when it is empty or a face or edge has no box.
    body: Option<Extent3d>,
}

/// A tree node over `faces[first..end]`; its left child follows it, its right child is at
/// `right` (0 for a leaf).
#[derive(Debug, Clone, Copy)]
struct Node {
    bx: Extent3d,
    first: u32,
    end: u32,
    right: u32,
}

impl Boxes {
    fn build(t: &Tables, resolution: Resolution) -> Self {
        let mut faces = Vec::new();
        let mut unboxed = Vec::new();
        for (id, rec) in t.faces.iter() {
            match crate::query::face_box(t, rec, resolution) {
                Some(b) => faces.push((id, b)),
                None => unboxed.push(id),
            }
        }
        let edges = t
            .edges
            .iter()
            .map(|(_, g)| crate::query::edge_box(t, g, resolution));
        let vertices = t
            .vertices
            .iter()
            .map(|(_, v)| crate::query::vertex_box(v, resolution));
        let body = if unboxed.is_empty() {
            faces
                .iter()
                .map(|&(_, b)| Some(b))
                .chain(edges)
                .chain(vertices)
                .try_fold(None, |acc: Option<Extent3d>, b| {
                    let b = b?;
                    Some(Some(acc.map_or(b, |a| a.union(b))))
                })
                .flatten()
        } else {
            None
        };
        let mut nodes = Vec::new();
        let n = faces.len();
        if n > 0 {
            split(&mut faces, 0, n, &mut nodes);
        }
        faces.shrink_to_fit();
        unboxed.shrink_to_fit();
        nodes.shrink_to_fit();
        Self {
            faces,
            unboxed,
            nodes,
            body,
        }
    }

    /// The body's box.
    pub(crate) const fn body(&self) -> Option<Extent3d> {
        self.body
    }

    /// The faces whose box meets `b`, and those with no box, in id order.
    pub(crate) fn faces_meeting(&self, b: Extent3d) -> Vec<FaceId> {
        let mut out = self.unboxed.clone();
        if !self.nodes.is_empty() {
            self.collect(0, b, &mut out);
        }
        out.sort_unstable();
        out
    }

    fn collect(&self, node: usize, b: Extent3d, out: &mut Vec<FaceId>) {
        let Some(n) = self.nodes.get(node) else {
            return;
        };
        if !meets(n.bx, b) {
            return;
        }
        if n.right == 0 {
            let leaf = self
                .faces
                .get(n.first as usize..n.end as usize)
                .unwrap_or(&[]);
            out.extend(leaf.iter().filter(|(_, fb)| meets(*fb, b)).map(|&(f, _)| f));
        } else {
            self.collect(node + 1, b, out);
            self.collect(n.right as usize, b, out);
        }
    }

    #[cfg(test)]
    pub(crate) fn bytes(&self) -> usize {
        self.faces.capacity() * size_of::<(FaceId, Extent3d)>()
            + self.unboxed.capacity() * size_of::<FaceId>()
            + self.nodes.capacity() * size_of::<Node>()
    }
}

/// Builds the tree over `faces[first..end]`, ordering them by their boxes' middles along
/// the longest side of their middles' spread (ties by id, so the tree is one function of
/// the boxes); returns the node's index.
fn split(faces: &mut [(FaceId, Extent3d)], first: usize, end: usize, nodes: &mut Vec<Node>) -> u32 {
    let idx = nodes.len() as u32;
    let part = faces.get_mut(first..end).unwrap_or(&mut []);
    let Some(bx) = part.iter().map(|&(_, b)| b).reduce(Extent3d::union) else {
        return idx;
    };
    nodes.push(Node {
        bx,
        first: first as u32,
        end: end as u32,
        right: 0,
    });
    if end - first > LEAF {
        let mids = part.iter().map(|(_, b)| b.mid());
        let (lo, hi) = mids.fold(
            ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]),
            |(lo, hi), m| {
                let p = [m.x, m.y, m.z];
                (
                    [lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])],
                    [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])],
                )
            },
        );
        let spread = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
        let axis = if spread[0] >= spread[1] && spread[0] >= spread[2] {
            0
        } else if spread[1] >= spread[2] {
            1
        } else {
            2
        };
        let key = |b: &Extent3d| {
            let m = b.mid();
            match axis {
                0 => m.x,
                1 => m.y,
                _ => m.z,
            }
        };
        part.sort_unstable_by(|a, b| key(&a.1).total_cmp(&key(&b.1)).then(a.0.cmp(&b.0)));
        let mid = first + (end - first) / 2;
        split(faces, first, mid, nodes);
        let right = split(faces, mid, end, nodes);
        if let Some(n) = nodes.get_mut(idx as usize) {
            n.right = right;
        }
    }
    idx
}

/// Whether two closed boxes share a point.
fn meets(a: Extent3d, b: Extent3d) -> bool {
    let (a0, a1, b0, b1) = (a.min(), a.max(), b.min(), b.max());
    a0.x <= b1.x && b0.x <= a1.x && a0.y <= b1.y && b0.y <= a1.y && a0.z <= b1.z && b0.z <= a1.z
}
