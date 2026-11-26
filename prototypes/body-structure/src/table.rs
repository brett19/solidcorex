//! Entity tables: the containers a body's faces, edges and vertices live in. An id is the index.
//!
//! `Clone` of a table is what an edit's draft starts from: a deep copy for [`Plain`], a pointer
//! copy for the others. A write through [`Table::set`] copies whatever the table still shares
//! with another body (`Arc::make_mut`): nothing when the draft is the only owner.

use std::sync::Arc;

/// A table of `T` addressed by dense index.
pub trait Table<T: Clone>: Clone + Send + Sync {
    /// Name for reports.
    const NAME: &'static str;
    /// A table holding `items` in order.
    fn from_vec(items: Vec<T>) -> Self;
    /// The item at `i`.
    fn get(&self, i: usize) -> Option<&T>;
    /// The item at `i` for writing: copies whatever part of the table is still shared first.
    fn get_mut(&mut self, i: usize) -> Option<&mut T>;
    /// Replaces the item at `i` (no-op when out of range).
    fn set(&mut self, i: usize, item: T) {
        if let Some(s) = self.get_mut(i) {
            *s = item;
        }
    }
    /// Appends an item; its id is the old length.
    fn push(&mut self, item: T);
    /// Number of slots.
    fn len(&self) -> usize;
    /// True when empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ---- 0: plain vector (the mutable baseline) ------------------------------------------------

/// A plain `Vec`: reads are a bounds check and a load; a copy is a deep copy.
#[derive(Clone, Debug)]
pub struct Plain<T>(pub Vec<T>);

impl<T: Clone + Send + Sync> Table<T> for Plain<T> {
    const NAME: &'static str = "plain";
    fn from_vec(items: Vec<T>) -> Self {
        Self(items)
    }
    #[inline]
    fn get(&self, i: usize) -> Option<&T> {
        self.0.get(i)
    }
    fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        self.0.get_mut(i)
    }
    fn push(&mut self, item: T) {
        self.0.push(item);
    }
    fn len(&self) -> usize {
        self.0.len()
    }
}

// ---- 1: whole-table copy-on-write ----------------------------------------------------------

/// One `Arc` around the whole table: a copy shares it; the first write copies all of it.
#[derive(Clone, Debug)]
pub struct WholeCow<T>(pub Arc<Vec<T>>);

impl<T: Clone + Send + Sync> Table<T> for WholeCow<T> {
    const NAME: &'static str = "whole-cow";
    fn from_vec(items: Vec<T>) -> Self {
        Self(Arc::new(items))
    }
    #[inline]
    fn get(&self, i: usize) -> Option<&T> {
        self.0.get(i)
    }
    fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        Arc::make_mut(&mut self.0).get_mut(i)
    }
    fn push(&mut self, item: T) {
        Arc::make_mut(&mut self.0).push(item);
    }
    fn len(&self) -> usize {
        self.0.len()
    }
}

// ---- 2: chunked persistent array -----------------------------------------------------------

/// Fixed-size chunks of `C` items, each behind its own `Arc`; the directory of chunk pointers is
/// copied by a draft (n / C pointers), a write copies only the chunk it lands in.
#[derive(Clone, Debug)]
pub struct Chunked<T, const C: usize> {
    chunks: Vec<Arc<Vec<T>>>,
    len: usize,
}

impl<T: Clone + Send + Sync, const C: usize> Table<T> for Chunked<T, C> {
    const NAME: &'static str = "chunked";
    fn from_vec(items: Vec<T>) -> Self {
        let len = items.len();
        let mut chunks = Vec::with_capacity(len.div_ceil(C));
        let mut it = items.into_iter().peekable();
        while it.peek().is_some() {
            let chunk: Vec<T> = it.by_ref().take(C).collect();
            chunks.push(Arc::new(chunk));
        }
        Self { chunks, len }
    }
    #[inline]
    fn get(&self, i: usize) -> Option<&T> {
        self.chunks.get(i / C)?.get(i % C)
    }
    fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        Arc::make_mut(self.chunks.get_mut(i / C)?).get_mut(i % C)
    }
    fn push(&mut self, item: T) {
        match self.chunks.last_mut() {
            Some(c) if c.len() < C => Arc::make_mut(c).push(item),
            _ => {
                let mut v = Vec::with_capacity(C);
                v.push(item);
                self.chunks.push(Arc::new(v));
            }
        }
        self.len += 1;
    }
    fn len(&self) -> usize {
        self.len
    }
}

impl<T, const C: usize> Chunked<T, C> {
    /// Chunks this table shares with `other` (pointer-equal), and its chunk count.
    pub fn shared_chunks_with(&self, other: &Self) -> (usize, usize) {
        let shared = self
            .chunks
            .iter()
            .zip(&other.chunks)
            .filter(|(a, b)| Arc::ptr_eq(a, b))
            .count();
        (shared, self.chunks.len())
    }
}

// ---- 4: persistent radix trie (a persistent vector, branching 32) ---------------------------

const BITS: u32 = 5;
const WIDTH: usize = 1 << BITS;

#[derive(Clone, Debug)]
enum Node<T> {
    Branch(Vec<Arc<Node<T>>>),
    Leaf(Vec<T>),
}

/// A persistent vector: a 32-way trie of `Arc` nodes. A draft copies the root pointer; a write
/// copies the path from the root to its leaf (depth nodes of 32 slots each).
#[derive(Clone, Debug)]
pub struct Trie<T> {
    root: Arc<Node<T>>,
    depth: u32,
    len: usize,
}

impl<T: Clone> Trie<T> {
    fn capacity(depth: u32) -> usize {
        WIDTH << (BITS * depth)
    }
    fn build(items: &mut std::vec::IntoIter<T>, depth: u32) -> Option<Arc<Node<T>>> {
        if depth == 0 {
            let leaf: Vec<T> = items.by_ref().take(WIDTH).collect();
            if leaf.is_empty() {
                return None;
            }
            return Some(Arc::new(Node::Leaf(leaf)));
        }
        let mut kids = Vec::new();
        while kids.len() < WIDTH {
            match Self::build(items, depth - 1) {
                Some(k) => kids.push(k),
                None => break,
            }
        }
        if kids.is_empty() {
            None
        } else {
            Some(Arc::new(Node::Branch(kids)))
        }
    }
    fn push_in(node: &mut Arc<Node<T>>, depth: u32, i: usize, item: T) {
        match Arc::make_mut(node) {
            Node::Leaf(v) => v.push(item),
            Node::Branch(kids) => {
                let slot = (i >> (BITS * depth)) & (WIDTH - 1);
                if slot < kids.len() {
                    if let Some(k) = kids.get_mut(slot) {
                        Self::push_in(k, depth - 1, i, item);
                    }
                } else {
                    // a new path down to a new leaf
                    let mut n = Arc::new(Node::Leaf(vec![item]));
                    for _ in 0..depth {
                        n = Arc::new(Node::Branch(vec![n]));
                    }
                    kids.push(n);
                }
            }
        }
    }
}

impl<T: Clone + Send + Sync> Table<T> for Trie<T> {
    const NAME: &'static str = "trie32";
    fn from_vec(items: Vec<T>) -> Self {
        let len = items.len();
        let mut depth = 0;
        while Self::capacity(depth) < len {
            depth += 1;
        }
        let mut it = items.into_iter();
        let root = Self::build(&mut it, depth).unwrap_or_else(|| Arc::new(Node::Leaf(Vec::new())));
        Self { root, depth, len }
    }
    #[inline]
    fn get(&self, i: usize) -> Option<&T> {
        if i >= self.len {
            return None;
        }
        let mut node = &*self.root;
        let mut d = self.depth;
        loop {
            match node {
                Node::Leaf(v) => return v.get(i & (WIDTH - 1)),
                Node::Branch(kids) => {
                    node = kids.get((i >> (BITS * d)) & (WIDTH - 1))?;
                    d -= 1;
                }
            }
        }
    }
    fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        if i >= self.len {
            return None;
        }
        let mut node = &mut self.root;
        let mut d = self.depth;
        loop {
            match Arc::make_mut(node) {
                Node::Leaf(v) => return v.get_mut(i & (WIDTH - 1)),
                Node::Branch(kids) => {
                    node = kids.get_mut((i >> (BITS * d)) & (WIDTH - 1))?;
                    d -= 1;
                }
            }
        }
    }
    fn push(&mut self, item: T) {
        if self.len == Self::capacity(self.depth) {
            let old = std::mem::replace(&mut self.root, Arc::new(Node::Leaf(Vec::new())));
            self.root = Arc::new(Node::Branch(vec![old]));
            self.depth += 1;
        }
        Self::push_in(&mut self.root, self.depth, self.len, item);
        self.len += 1;
    }
    fn len(&self) -> usize {
        self.len
    }
}
