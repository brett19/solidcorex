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
//! write has to reset one. A body finished from a draft of another whose boxes were built
//! takes those boxes as a [`Seed`]: its first box read keeps the old box of every face the
//! edit left alone and computes the rest (research 63 §6.14). The tree over them is one
//! function of the faces' boxes, so a seeded build and a fresh one are the same.

use core::fmt;
use std::sync::{Arc, Mutex, OnceLock};

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
    boxes: OnceLock<Arc<Boxes>>,
    /// What the first box read may take from the body this one was edited from; taken by
    /// that read.
    seed: Mutex<Option<Seed>>,
}

/// A base body's boxes, and the faces (sorted) whose box may differ in the new body: every
/// face the edit wrote or made, and every face using an edge whose geometry or end vertex
/// it wrote. A face's box reads its record, its uses' edges' accuracy and their ends'
/// accuracy, and the resolution, which is the base's.
#[derive(Debug)]
pub(crate) struct Seed {
    pub(crate) boxes: Arc<Boxes>,
    pub(crate) dirty: Vec<FaceId>,
}

impl fmt::Debug for BodyCells {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BodyCells")
            .field("adjacency", &self.adjacency.get().is_some())
            .field("boxes", &self.boxes.get().is_some())
            .field("seed", &self.seed.lock().is_ok_and(|s| s.is_some()))
            .finish()
    }
}

impl BodyCells {
    /// Empty cells whose boxes start from `seed`.
    pub(crate) fn seeded(seed: Option<Seed>) -> Self {
        Self {
            seed: Mutex::new(seed),
            ..Self::default()
        }
    }

    /// The adjacency rows, if they have been built.
    pub(crate) fn built_adjacency(&self) -> Option<&Adjacency> {
        self.adjacency.get()
    }

    /// The boxes, if they have been built.
    pub(crate) fn built_boxes(&self) -> Option<Arc<Boxes>> {
        self.boxes.get().cloned()
    }

    /// The adjacency rows of `t`, built on first read.
    pub(crate) fn adjacency(&self, t: &Tables) -> &Adjacency {
        self.adjacency.get_or_init(|| Adjacency::build(t))
    }

    /// The boxes of `t` under `resolution`, built on first read.
    pub(crate) fn boxes(&self, t: &Tables, resolution: Resolution) -> &Boxes {
        self.boxes.get_or_init(|| {
            let seed = self.seed.lock().ok().and_then(|mut s| s.take());
            Arc::new(Boxes::build(t, resolution, seed.as_ref()))
        })
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
    /// The faces' boxes and the tree over them, shared with the body this one was edited
    /// from when no face's box changed.
    tree: Arc<Tree>,
    /// The body's box: `None` when it is empty or a face or edge has no box.
    body: Option<Extent3d>,
}

/// The faces' boxes and the box tree over them: one function of the set of faces' boxes.
#[derive(Debug)]
struct Tree {
    /// The bounded faces' grown boxes, in tree order.
    faces: Vec<(FaceId, Extent3d)>,
    /// The faces with no box (unbounded, or not enclosed), in id order.
    unboxed: Vec<FaceId>,
    nodes: Vec<Node>,
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
    /// The boxes of `t`, each face's taken from `seed` where the edit left it alone. When
    /// that leaves every face with the box it had, and no face came or went, the tree is
    /// the base's, as it is one function of the faces' boxes.
    fn build(t: &Tables, resolution: Resolution, seed: Option<&Seed>) -> Self {
        // The base's box of each face by slot: `Some(None)` for a face it had no box for.
        let mut was: Vec<Option<Option<Extent3d>>> = Vec::new();
        let mut dirty: Vec<bool> = Vec::new();
        if let Some(seed) = seed {
            was = vec![None; t.faces.slots()];
            for &(id, b) in &seed.boxes.tree.faces {
                if let Some(w) = was.get_mut(id.slot()) {
                    *w = Some(Some(b));
                }
            }
            for &id in &seed.boxes.tree.unboxed {
                if let Some(w) = was.get_mut(id.slot()) {
                    *w = Some(None);
                }
            }
            dirty = vec![false; t.faces.slots()];
            for &id in &seed.dirty {
                if let Some(d) = dirty.get_mut(id.slot()) {
                    *d = true;
                }
            }
        }
        let mut faces = Vec::new();
        let mut unboxed = Vec::new();
        let mut same = seed.is_some();
        for (id, rec) in t.faces.iter() {
            let old = was.get(id.slot()).copied().flatten();
            let bx = match old {
                Some(b) if !dirty.get(id.slot()).copied().unwrap_or(true) => b,
                _ => crate::query::face_box(t, rec, resolution),
            };
            same &= old == Some(bx);
            match bx {
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
        let tree = match seed {
            // Every live face had its box in the base, so if the counts match, no face went.
            Some(seed)
                if same
                    && faces.len() == seed.boxes.tree.faces.len()
                    && unboxed.len() == seed.boxes.tree.unboxed.len() =>
            {
                Arc::clone(&seed.boxes.tree)
            }
            _ => Arc::new(Tree::build(faces, unboxed)),
        };
        Self { tree, body }
    }

    /// The body's box.
    pub(crate) const fn body(&self) -> Option<Extent3d> {
        self.body
    }

    /// The faces whose box meets `b`, and those with no box, in id order.
    pub(crate) fn faces_meeting(&self, b: Extent3d) -> Vec<FaceId> {
        let mut out = self.tree.unboxed.clone();
        if !self.tree.nodes.is_empty() {
            self.collect(0, b, &mut out);
        }
        out.sort_unstable();
        out
    }

    fn collect(&self, node: usize, b: Extent3d, out: &mut Vec<FaceId>) {
        let Some(n) = self.tree.nodes.get(node) else {
            return;
        };
        if !meets(n.bx, b) {
            return;
        }
        if n.right == 0 {
            let leaf = self
                .tree
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
        let t = &self.tree;
        t.faces.capacity() * size_of::<(FaceId, Extent3d)>()
            + t.unboxed.capacity() * size_of::<FaceId>()
            + t.nodes.capacity() * size_of::<Node>()
    }
}

impl Tree {
    /// The tree over `faces` (in id order), with `unboxed` beside it.
    fn build(mut faces: Vec<(FaceId, Extent3d)>, mut unboxed: Vec<FaceId>) -> Self {
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
        }
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
        let order = |a: &(FaceId, Extent3d), b: &(FaceId, Extent3d)| {
            key(&a.1).total_cmp(&key(&b.1)).then(a.0.cmp(&b.0))
        };
        // The halves are fixed by the order, which is total; only a leaf keeps the order
        // within it, so only a half that becomes a leaf is sorted.
        let half = (end - first) / 2;
        if half > 0 {
            part.select_nth_unstable_by(half, order);
        }
        let (lo_half, hi_half) = part.split_at_mut(half);
        for h in [lo_half, hi_half] {
            if h.len() <= LEAF {
                h.sort_unstable_by(order);
            }
        }
        let mid = first + half;
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

#[cfg(test)]
mod tests {
    use scx_num::hash::Rng;
    use scx_num::vector3d::Point3;

    use super::{LEAF, Node, split};
    use crate::id::FaceId;
    use crate::table::Id;
    use scx_num::extent3d::Extent3d;

    /// The tree as built before (research 63 §6.9): every node sorted whole.
    fn split_sorting(
        faces: &mut [(FaceId, Extent3d)],
        first: usize,
        end: usize,
        nodes: &mut Vec<Node>,
    ) -> u32 {
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
            let mids: Vec<Point3> = part.iter().map(|(_, b)| b.mid()).collect();
            let lo = mids.iter().fold([f64::INFINITY; 3], |a, m| {
                [a[0].min(m.x), a[1].min(m.y), a[2].min(m.z)]
            });
            let hi = mids.iter().fold([f64::NEG_INFINITY; 3], |a, m| {
                [a[0].max(m.x), a[1].max(m.y), a[2].max(m.z)]
            });
            let spread = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
            let axis = if spread[0] >= spread[1] && spread[0] >= spread[2] {
                0
            } else if spread[1] >= spread[2] {
                1
            } else {
                2
            };
            let key = |b: &Extent3d| [b.mid().x, b.mid().y, b.mid().z][axis];
            part.sort_unstable_by(|a, b| key(&a.1).total_cmp(&key(&b.1)).then(a.0.cmp(&b.0)));
            let mid = first + (end - first) / 2;
            split_sorting(faces, first, mid, nodes);
            let right = split_sorting(faces, mid, end, nodes);
            if let Some(n) = nodes.get_mut(idx as usize) {
                n.right = right;
            }
        }
        idx
    }

    /// Selecting each node's halves builds the same tree, in the same order, as sorting
    /// each node whole: on random boxes, with repeated middles, of every size up to 300.
    #[test]
    fn selecting_halves_builds_the_sorted_tree() {
        let mut rng = Rng::new(0x6314);
        for n in 1..300 {
            let mut faces: Vec<(FaceId, Extent3d)> = (0..n)
                .map(|i| {
                    // Coarse coordinates, so middles tie and the id decides.
                    let mut c = || (rng.next_u64() % 7) as f64;
                    let p = Point3::new(c(), c(), c());
                    let q = Point3::new(p.x + c(), p.y + c(), p.z + c());
                    (FaceId::from_slot(i), Extent3d::spanning(p, q).unwrap())
                })
                .collect();
            // Shuffle, so the input order is not the id order.
            for i in (1..faces.len()).rev() {
                faces.swap(i, (rng.next_u64() % (i as u64 + 1)) as usize);
            }
            let mut a = faces.clone();
            let mut b = faces;
            let (mut na, mut nb) = (Vec::new(), Vec::new());
            split(&mut a, 0, n as usize, &mut na);
            split_sorting(&mut b, 0, n as usize, &mut nb);
            assert_eq!(format!("{a:?}{na:?}"), format!("{b:?}{nb:?}"), "{n} faces");
        }
    }
}
