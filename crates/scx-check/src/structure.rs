// Written for solidcorex. Apache-2.0.
//! The structure pass `contract` and `check` share (research 74 §18): the two questions of
//! structure that neither `Draft::finish` nor the contract's other tests answer.
//!
//! - **Vertex manifoldness.** A vertex is manifold when the faces round it form one piece:
//!   its *link* is connected. The link has a node for each end of an edge at the vertex
//!   (an edge, and which of its two ends) and an arc for each corner of a face there (a use
//!   ending at the vertex, and the next use of its loop that is not a collapsed point, which
//!   joins the two nodes the corner touches). One block's corner is a circle, a sheet's
//!   border vertex a path, a cone's apex a loop on one node: each is one piece. Two solids
//!   that touch at one vertex, two sheets that meet at one, and a face that touches itself
//!   at one are two pieces. A wire edge ending there is a node with no arc, so it is a piece
//!   of its own. `finish` proves the loops are connected and checks each edge's fins agree
//!   about the sides between them; neither looks at what meets at a vertex across shells or
//!   across a face's own corners, and the contract's edge test (two fins per edge) refuses
//!   only the bodies whose pinch is also an edge.
//! - **Solid or sheet.** Each face has a region on each side. A face between a solid and a
//!   non-solid bounds material; one between two non-solids is a sheet; one between two
//!   solids is inside material. `finish` accepts all three; [`Class`] says which the body
//!   has.
//!
//! The pass reads the records only: no geometry, no budget, one walk over the uses.

use std::collections::BTreeMap;

use scx_brep::{Brep, EdgeId, EdgeShape, FaceSide, ShellId, Use, VertexId};
use scx_num::frame::Sense;

/// What a body is made of, by the regions on the two sides of its faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Class {
    /// No face.
    Empty,
    /// Every face has a solid on one side and a non-solid on the other.
    Solid,
    /// Every face has a non-solid on both sides: a surface body, open or closed.
    Sheet,
    /// Faces of more than one of those kinds, or a face with a solid on both sides.
    Mixed,
}

impl Class {
    /// A stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Solid => "solid",
            Self::Sheet => "sheet",
            Self::Mixed => "mixed",
        }
    }
}

/// A vertex whose link is in more than one piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pinch {
    /// The vertex.
    pub vertex: VertexId,
    /// The pieces its link falls into, at least two.
    pub pieces: usize,
}

/// A node of a vertex's link: one end of an edge there (`0` is the edge's start).
type Node = (EdgeId, u8);

/// The end of the use's edge that the use starts at (`0`) or ends at (`1`), in the edge's own
/// sense.
const fn ends_at(sense: Sense, end_of_use: bool) -> u8 {
    match (sense, end_of_use) {
        (Sense::Forward, false) | (Sense::Reversed, true) => 0,
        (Sense::Forward, true) | (Sense::Reversed, false) => 1,
    }
}

/// Whether the use is of a collapsed edge (a point).
fn collapsed(u: Use<'_>) -> bool {
    matches!(u.edge().shape(), EdgeShape::Collapsed)
}

/// The vertices whose link is not connected, in id order. A vertex with no corner (a wire's
/// or a lone one) is not examined.
pub(crate) fn pinched(body: &Brep) -> Vec<Pinch> {
    let mut arcs: BTreeMap<VertexId, Vec<(Node, Node)>> = BTreeMap::new();
    for f in body.faces() {
        for lp in f.loops() {
            let uses: Vec<Use<'_>> = lp.uses().filter(|&u| !collapsed(u)).collect();
            for (i, &u) in uses.iter().enumerate() {
                // The next use that is not a point: collapsed uses sit at one vertex.
                let Some(&next) = uses.get((i + 1) % uses.len()) else {
                    continue;
                };
                arcs.entry(u.end().id()).or_default().push((
                    (u.edge().id(), ends_at(u.sense(), true)),
                    (next.edge().id(), ends_at(next.sense(), false)),
                ));
            }
        }
    }
    let mut wires: BTreeMap<VertexId, Vec<Node>> = BTreeMap::new();
    for s in body.shells() {
        for e in s.wires() {
            if matches!(e.shape(), EdgeShape::Collapsed) {
                continue;
            }
            for (i, v) in e.ends().into_iter().enumerate() {
                wires.entry(v).or_default().push((e.id(), u8::from(i == 1)));
            }
        }
    }
    let mut out = Vec::new();
    for (vertex, arcs) in arcs {
        let mut nodes: Vec<Node> = arcs.iter().flat_map(|&(a, b)| [a, b]).collect();
        nodes.extend(wires.get(&vertex).into_iter().flatten().copied());
        nodes.sort_unstable();
        nodes.dedup();
        let at = |n: Node| nodes.binary_search(&n).unwrap_or(0);
        let mut root: Vec<usize> = (0..nodes.len()).collect();
        for &(a, b) in &arcs {
            let (ra, rb) = (find(&mut root, at(a)), find(&mut root, at(b)));
            if ra != rb
                && let Some(slot) = root.get_mut(ra.max(rb))
            {
                *slot = ra.min(rb);
            }
        }
        let pieces = (0..nodes.len())
            .filter(|&i| find(&mut root, i) == i)
            .count();
        if pieces > 1 {
            out.push(Pinch { vertex, pieces });
        }
    }
    out
}

/// The union-find root of `i`, flattening the path.
fn find(root: &mut [usize], mut i: usize) -> usize {
    while let Some(&p) = root.get(i) {
        if p == i {
            break;
        }
        let up = root.get(p).copied().unwrap_or(p);
        if let Some(slot) = root.get_mut(i) {
            *slot = up;
        }
        i = up;
    }
    i
}

/// The body's [`Class`], and each shell of a solid region that has a face with a solid on its
/// other side too (material on both sides of that face), in id order.
pub(crate) fn classify(body: &Brep) -> (Class, Vec<ShellId>) {
    let (mut bounding, mut sheet, mut inside) = (false, false, false);
    let mut shells = Vec::new();
    for f in body.faces() {
        let (front, back) = (f.shell(FaceSide::Front), f.shell(FaceSide::Back));
        match (front.region().is_solid(), back.region().is_solid()) {
            (true, true) => {
                inside = true;
                shells.extend([front.id(), back.id()]);
            }
            (false, false) => sheet = true,
            _ => bounding = true,
        }
    }
    shells.sort_unstable();
    shells.dedup();
    let class = match (bounding, sheet, inside) {
        (false, false, false) => Class::Empty,
        (true, false, false) => Class::Solid,
        (false, true, false) => Class::Sheet,
        _ => Class::Mixed,
    };
    (class, shells)
}
