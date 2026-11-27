// Written for solidcorex (research 57 M10). Apache-2.0. The part of solidcore's `geom/tree.rs`
// at commit 03e16c9ee766de5b9b1ced845e5149d62ff493b8 that the global solver reads: a node's
// box and its two children (`SmTree`, `SmTreeNode::GetBBox`, `GetChild1`, `GetChild2`).
//! A subdivision tree: boxes over a caller's pieces (Bezier spans, patches, faces), each
//! either a leaf or split into two children, for the global solver
//! ([`crate::global_solve`]).
//!
//! The tree is built bottom up: [`SplitTree::leaf`] adds a piece, [`SplitTree::split`] adds
//! a node over two nodes already in the tree. So the child links are acyclic and every
//! node's height is known when it is added; the last node added is the root. A node's box
//! is the caller's (a piece's own bound is often tighter than the union of its children's).
//!
//! | Rust | Upstream |
//! |------|----------|
//! | [`SplitTree`], [`SplitTree::root`] | `SmTree`, `SmTree::GetTopNode` |
//! | [`Node`], [`Node::bbox`], [`Node::children`] | `SmTreeNode`, `GetBBox`, `GetChild1`/`GetChild2` |
//! | [`Node::item`] | the node's aux data (`GetAuxType`, `GetAuxData`, `GetOwnerObject`) |
//! | [`NodeId`] | an `SmTreeNode*` |
//!
//! Not here (research 57 M10): the spatial object tree of `tree.rs` (`AddObject`,
//! `RemoveObject`, `GetObjectsInBox`) is [`crate::box_tree`]'s job; the surface neighbour
//! searches and their marks belong to the surface crate, and the span data (`BezierSpan`,
//! `BezierAux1d`) to the curve crate, as the item type `T` of their trees. A node that
//! upstream treats as a leaf although it has children (a Bezier-surface node over
//! non-surface children, `AreReadyForLocalSolve`) is added here as a leaf, its other
//! structure kept in its item.

use scx_num::extent3d::Extent3d;
use scx_num::vector3d::Point3;

/// A node of a [`SplitTree`]: an index into the tree that made it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u32);

impl NodeId {
    /// The node's index in its tree, in the order the nodes were added.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// One node: a box, a piece, and either no children or two.
#[derive(Debug, Clone, PartialEq)]
pub struct Node<T> {
    bbox: Extent3d,
    children: Option<[NodeId; 2]>,
    height: u32,
    item: T,
    /// The box's squared diagonal and middle, which a solve reads at every split.
    diagonal_sq: f64,
    middle: Point3,
}

impl<T> Node<T> {
    fn new(bbox: Extent3d, children: Option<[NodeId; 2]>, height: u32, item: T) -> Self {
        Self {
            diagonal_sq: (bbox.max() - bbox.min()).length_squared(),
            middle: bbox.evaluate(0.5, 0.5, 0.5),
            bbox,
            children,
            height,
            item,
        }
    }

    /// The squared length of the box's diagonal, `(max - min)²`.
    #[must_use]
    pub(crate) const fn diagonal_squared(&self) -> f64 {
        self.diagonal_sq
    }

    /// The box's middle, [`Extent3d::evaluate`] at `(0.5, 0.5, 0.5)`.
    #[must_use]
    pub(crate) const fn middle(&self) -> Point3 {
        self.middle
    }

    /// The node's box.
    #[must_use]
    pub const fn bbox(&self) -> &Extent3d {
        &self.bbox
    }

    /// The two children, in the order they were given; `None` for a leaf.
    #[must_use]
    pub const fn children(&self) -> Option<[NodeId; 2]> {
        self.children
    }

    /// Whether the node has no children.
    #[must_use]
    pub const fn is_leaf(&self) -> bool {
        self.children.is_none()
    }

    /// The levels below the node: 0 for a leaf.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The caller's piece.
    #[must_use]
    pub const fn item(&self) -> &T {
        &self.item
    }
}

/// A tree of boxes, built bottom up (see the module).
#[derive(Debug, Clone, PartialEq)]
pub struct SplitTree<T> {
    nodes: Vec<Node<T>>,
}

impl<T> Default for SplitTree<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> SplitTree<T> {
    /// An empty tree.
    #[must_use]
    pub const fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    /// An empty tree with room for `n` nodes.
    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(n),
        }
    }

    fn push(&mut self, node: Node<T>) -> Option<NodeId> {
        let id = NodeId(u32::try_from(self.nodes.len()).ok()?);
        self.nodes.push(node);
        Some(id)
    }

    /// Adds a leaf. `None` once the tree holds `u32::MAX` nodes.
    pub fn leaf(&mut self, bbox: Extent3d, item: T) -> Option<NodeId> {
        self.push(Node::new(bbox, None, 0, item))
    }

    /// Adds a node over `children`, which must be nodes of this tree. `None` when a child
    /// is not (or the tree is full); the tree is then unchanged.
    pub fn split(&mut self, bbox: Extent3d, item: T, children: [NodeId; 2]) -> Option<NodeId> {
        let [a, b] = children;
        let ha = self.nodes.get(a.index())?.height;
        let hb = self.nodes.get(b.index())?.height;
        self.push(Node::new(
            bbox,
            Some(children),
            ha.max(hb).saturating_add(1),
            item,
        ))
    }

    /// The node `id`; `None` for an id of another, larger tree.
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&Node<T>> {
        self.nodes.get(id.index())
    }

    /// The root: the last node added; `None` for an empty tree.
    #[must_use]
    pub fn root(&self) -> Option<NodeId> {
        self.nodes.len().checked_sub(1).map(|i| NodeId(i as u32))
    }

    /// The nodes, in the order they were added (a node's index is its [`NodeId::index`]).
    #[must_use]
    pub fn nodes(&self) -> &[Node<T>] {
        &self.nodes
    }

    /// The number of nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the tree has no node.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scx_num::vector3d::Point3;

    fn cube(x: f64) -> Extent3d {
        Extent3d::spanning(Point3::new(x, 0.0, 0.0), Point3::new(x + 1.0, 1.0, 1.0)).unwrap()
    }

    #[test]
    fn built_bottom_up() {
        let mut t = SplitTree::new();
        assert_eq!(t.root(), None);
        let a = t.leaf(cube(0.0), 'a').unwrap();
        let b = t.leaf(cube(1.0), 'b').unwrap();
        let c = t.leaf(cube(2.0), 'c').unwrap();
        let ab = t.split(cube(0.0).union(cube(1.0)), 'x', [a, b]).unwrap();
        let top = t.split(cube(0.0).union(cube(2.0)), 'y', [ab, c]).unwrap();
        assert_eq!(t.root(), Some(top));
        let n = t.get(top).unwrap();
        assert_eq!(n.children(), Some([ab, c]));
        assert_eq!(n.height(), 2);
        assert_eq!(*t.get(c).unwrap().item(), 'c');
        assert!(t.get(c).unwrap().is_leaf());
    }

    #[test]
    fn children_must_be_in_the_tree() {
        let mut big = SplitTree::new();
        for i in 0..4 {
            big.leaf(cube(f64::from(i)), ()).unwrap();
        }
        let far = big.root().unwrap();
        let mut t = SplitTree::new();
        let a = t.leaf(cube(0.0), ()).unwrap();
        assert_eq!(t.split(cube(0.0), (), [a, far]), None);
        assert_eq!(t.len(), 1);
    }
}
