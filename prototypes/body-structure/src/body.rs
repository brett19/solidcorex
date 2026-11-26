//! Bodies over the tables, their drafts, and the result an edit reports.
//!
//! Two layouts, each generic over the table kind ([`Family`]):
//! - [`TableBody`]: one table per entity kind (faces, loops, edges, vertices), records by id.
//! - [`FaceBody`]: per-face records. A face and what it owns (its loops, edge uses and trims, its
//!   surface, its cells) is one `Arc<FaceRecord>`; edges and vertices live in tables.
//!
//! A finished body is never written. `body.edit()` gives a [`Draft`] that shares every table
//! with the body; writes through the draft copy only what they reach (whole table, chunk, trie
//! path or face record). `into_draft()` takes the body by value: where nothing else holds a part,
//! writes go in place (the transient mode). `Draft::finish` resets the cells of every touched
//! face and returns the new body with each touched input face's fate.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use crate::model::{geo, Edge, Face, FaceCells, Geo, Loop, Source, Use, Vertex, CURVE_LEN, SURFACE_LEN, TRIM_LEN};
use crate::table::{Chunked, Plain, Table, Trie, WholeCow};

/// A kind of table, so a body layout can be built over any of them.
pub trait Family: Send + Sync + 'static {
    const NAME: &'static str;
    type T<X: Clone + Send + Sync + 'static>: Table<X>;
}
pub struct PlainF;
pub struct WholeF;
pub struct ChunkF;
pub struct Chunk16F;
pub struct TrieF;
impl Family for PlainF {
    const NAME: &'static str = "plain";
    type T<X: Clone + Send + Sync + 'static> = Plain<X>;
}
impl Family for WholeF {
    const NAME: &'static str = "whole-cow";
    type T<X: Clone + Send + Sync + 'static> = WholeCow<X>;
}
impl Family for ChunkF {
    const NAME: &'static str = "chunk64";
    type T<X: Clone + Send + Sync + 'static> = Chunked<X, 64>;
}
impl Family for Chunk16F {
    const NAME: &'static str = "chunk16";
    type T<X: Clone + Send + Sync + 'static> = Chunked<X, 16>;
}
impl Family for TrieF {
    const NAME: &'static str = "trie32";
    type T<X: Clone + Send + Sync + 'static> = Trie<X>;
}

/// What became of one input face (sparse: a face absent from the map is unchanged, same id).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FaceFate {
    /// Same id; `geometry` true when its surface changed, false when only its boundary did.
    Modified { geometry: bool },
    /// Replaced by these faces (the first keeps the input's id).
    SplitInto(Vec<u32>),
    /// Absorbed into this face.
    MergedInto(u32),
    Deleted,
}

/// Where a created face came from, for naming.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    FromEdge(u32),
    FromVertex(u32),
    FromFace(u32),
    Free,
}

/// What an edit reports about its input.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub fates: BTreeMap<u32, FaceFate>,
    pub created: Vec<(u32, Origin)>,
}

/// The read and write surface every layout offers.
pub trait Body: Clone + Send + Sync + Sized + 'static {
    fn name() -> String;
    fn build(src: Source) -> Self;
    fn n_faces(&self) -> usize;
    // reads
    fn face_edges(&self, f: u32, out: &mut Vec<u32>);
    fn edge(&self, e: u32) -> &Edge;
    fn vertex(&self, v: u32) -> &Vertex;
    fn surface(&self, f: u32) -> &Geo;
    fn face_tol(&self, f: u32) -> f64;
    fn cells(&self, f: u32) -> &FaceCells;
    fn trims(&self, f: u32) -> Vec<&[f64]>;
    /// Sum over every face, loop, use, edge and vertex point (the full traversal).
    fn traverse(&self) -> f64;
    /// `steps` moves to a neighbouring face across successive edges of each face's first loop.
    fn walk(&self, start: u32, steps: usize) -> u32;
    // writes (only through a Draft)
    fn set_surface(&mut self, f: u32, g: Geo);
    fn move_vertex(&mut self, v: u32, dz: f64);
    fn set_curve(&mut self, e: u32, g: Geo, tol: f64);
    fn set_edge_tol(&mut self, e: u32, tol: f64);
    /// New trims on `f`'s uses of the edges in `edges`.
    fn retrim(&mut self, f: u32, edges: &BTreeSet<u32>, salt: f64);
    /// Fresh, empty cells on `f`.
    fn reset_cells(&mut self, f: u32);

    /// A draft sharing everything with this body.
    fn edit(&self) -> Draft<Self> {
        Draft { body: self.clone(), touched: BTreeMap::new() }
    }
    /// A draft that takes the body: unshared parts are written in place.
    fn into_draft(self) -> Draft<Self> {
        Draft { body: self, touched: BTreeMap::new() }
    }
    /// Cell read: the face's box (filled on first read).
    fn bbox(&self, f: u32) -> [f64; 6] {
        self.cells(f).bbox(self.surface(f))
    }
    /// Cell read: the face's tree (filled on first read).
    fn tree(&self, f: u32) -> Geo {
        self.cells(f).tree(self.surface(f), &self.trims(f))
    }
}

/// The mutable working form.
pub struct Draft<B: Body> {
    pub body: B,
    /// Faces touched so far, with whether their surface changed.
    touched: BTreeMap<u32, bool>,
}

impl<B: Body> Draft<B> {
    pub fn read(&self) -> &B {
        &self.body
    }
    pub fn set_surface(&mut self, f: u32, g: Geo) {
        self.body.set_surface(f, g);
        self.touched.insert(f, true);
    }
    /// Changes an edge's tolerance; both faces on it are touched (their cells read it).
    pub fn set_edge_tol(&mut self, e: u32, tol: f64) {
        self.body.set_edge_tol(e, tol);
        for f in self.body.edge(e).faces {
            self.touched.entry(f).or_insert(false);
        }
    }
    /// Seals the draft: touched faces get fresh cells; everything else keeps its cells.
    pub fn finish(mut self) -> (B, Outcome) {
        let mut out = Outcome::default();
        for (&f, &geometry) in &self.touched {
            self.body.reset_cells(f);
            out.fates.insert(f, FaceFate::Modified { geometry });
        }
        (self.body, out)
    }

    /// The benchmark edit: `patch` faces get new surfaces; their edges and vertices move; every
    /// edge at a moved vertex gets a new curve; every face on a changed edge gets new trims.
    pub fn apply_patch(&mut self, patch: &[u32], salt: f64) {
        let mut edges = BTreeSet::new();
        let mut buf = Vec::new();
        for &f in patch {
            buf.clear();
            self.body.face_edges(f, &mut buf);
            edges.extend(buf.iter().copied());
        }
        let verts: BTreeSet<u32> = edges.iter().flat_map(|&e| self.body.edge(e).v).collect();
        for &v in &verts {
            edges.extend(self.body.vertex(v).edges.iter().copied());
        }
        for &f in patch {
            self.set_surface(f, geo(SURFACE_LEN, salt + f as f64));
        }
        for &v in &verts {
            self.body.move_vertex(v, salt * 1e-3);
        }
        let mut faces = BTreeSet::new();
        for &e in &edges {
            self.body.set_curve(e, geo(CURVE_LEN, salt + e as f64), 2e-7);
            faces.extend(self.body.edge(e).faces);
        }
        for &f in &faces {
            self.body.retrim(f, &edges, salt);
            self.touched.entry(f).or_insert(false);
        }
    }
}

fn new_trim(salt: f64, e: u32) -> Geo {
    geo(TRIM_LEN, salt + e as f64 * 0.25)
}

// ---- table layout -------------------------------------------------------------------------

pub struct TableBody<F: Family> {
    pub faces: F::T<Face>,
    pub loops: F::T<Loop>,
    pub edges: F::T<Edge>,
    pub verts: F::T<Vertex>,
}

impl<F: Family> Clone for TableBody<F> {
    fn clone(&self) -> Self {
        Self { faces: self.faces.clone(), loops: self.loops.clone(), edges: self.edges.clone(), verts: self.verts.clone() }
    }
}

macro_rules! must {
    ($e:expr) => {
        match $e {
            Some(x) => x,
            None => panic!("dangling id"),
        }
    };
}

impl<F: Family> Body for TableBody<F> {
    fn name() -> String {
        format!("table/{}", F::NAME)
    }
    fn build(s: Source) -> Self {
        Self {
            faces: Table::from_vec(s.faces),
            loops: Table::from_vec(s.loops),
            edges: Table::from_vec(s.edges),
            verts: Table::from_vec(s.verts),
        }
    }
    fn n_faces(&self) -> usize {
        self.faces.len()
    }
    fn face_edges(&self, f: u32, out: &mut Vec<u32>) {
        for &l in &must!(self.faces.get(f as usize)).loops {
            out.extend(must!(self.loops.get(l as usize)).uses.iter().map(|u| u.edge));
        }
    }
    #[inline]
    fn edge(&self, e: u32) -> &Edge {
        must!(self.edges.get(e as usize))
    }
    #[inline]
    fn vertex(&self, v: u32) -> &Vertex {
        must!(self.verts.get(v as usize))
    }
    fn surface(&self, f: u32) -> &Geo {
        &must!(self.faces.get(f as usize)).surface
    }
    #[inline]
    fn face_tol(&self, f: u32) -> f64 {
        let face = must!(self.faces.get(f as usize));
        face.tol + face.surface[0]
    }
    fn cells(&self, f: u32) -> &FaceCells {
        &must!(self.faces.get(f as usize)).cells
    }
    fn trims(&self, f: u32) -> Vec<&[f64]> {
        let mut t = Vec::new();
        for &l in &must!(self.faces.get(f as usize)).loops {
            t.extend(must!(self.loops.get(l as usize)).uses.iter().map(|u| &*u.trim));
        }
        t
    }
    fn traverse(&self) -> f64 {
        let mut s = 0.0;
        for fi in 0..self.faces.len() {
            let face = must!(self.faces.get(fi));
            s += face.tol;
            for &l in &face.loops {
                for u in &must!(self.loops.get(l as usize)).uses {
                    let e = self.edge(u.edge);
                    s += e.tol;
                    for v in e.v {
                        s += self.vertex(v).p[0];
                    }
                }
            }
        }
        s
    }
    fn walk(&self, start: u32, steps: usize) -> u32 {
        let mut f = start;
        for k in 0..steps {
            let face = must!(self.faces.get(f as usize));
            let lp = must!(self.loops.get(face.loops[0] as usize));
            let e = self.edge(lp.uses[k % lp.uses.len()].edge);
            f = if e.faces[0] == f { e.faces[1] } else { e.faces[0] };
        }
        f
    }
    fn set_surface(&mut self, f: u32, g: Geo) {
        must!(self.faces.get_mut(f as usize)).surface = g;
    }
    fn move_vertex(&mut self, v: u32, dz: f64) {
        must!(self.verts.get_mut(v as usize)).p[2] += dz;
    }
    fn set_curve(&mut self, e: u32, g: Geo, tol: f64) {
        let edge = must!(self.edges.get_mut(e as usize));
        edge.curve = g;
        edge.tol = tol;
    }
    fn set_edge_tol(&mut self, e: u32, tol: f64) {
        must!(self.edges.get_mut(e as usize)).tol = tol;
    }
    fn retrim(&mut self, f: u32, edges: &BTreeSet<u32>, salt: f64) {
        let loops = must!(self.faces.get(f as usize)).loops.clone();
        for l in loops {
            for u in &mut must!(self.loops.get_mut(l as usize)).uses {
                if edges.contains(&u.edge) {
                    u.trim = new_trim(salt, u.edge);
                }
            }
        }
    }
    fn reset_cells(&mut self, f: u32) {
        must!(self.faces.get_mut(f as usize)).cells = Arc::default();
    }
}

// ---- per-face layout ----------------------------------------------------------------------

/// A face with everything it owns: one reference-counted unit.
#[derive(Clone, Debug)]
pub struct FaceRecord {
    pub loops: Vec<Vec<Use>>,
    pub surface: Geo,
    pub tol: f64,
    pub cells: FaceCells,
}

pub struct FaceBody<F: Family> {
    pub faces: F::T<Arc<FaceRecord>>,
    pub edges: F::T<Edge>,
    pub verts: F::T<Vertex>,
}

impl<F: Family> Clone for FaceBody<F> {
    fn clone(&self) -> Self {
        Self { faces: self.faces.clone(), edges: self.edges.clone(), verts: self.verts.clone() }
    }
}

impl<F: Family> FaceBody<F> {
    #[inline]
    fn rec(&self, f: u32) -> &FaceRecord {
        must!(self.faces.get(f as usize))
    }
    /// The face record for writing: copied once if another body shares it.
    fn rec_mut(&mut self, f: u32) -> &mut FaceRecord {
        Arc::make_mut(must!(self.faces.get_mut(f as usize)))
    }
    /// The face record's pointer (to test sharing).
    pub fn record_ptr(&self, f: u32) -> *const FaceRecord {
        Arc::as_ptr(must!(self.faces.get(f as usize)))
    }
}

impl<F: Family> Body for FaceBody<F> {
    fn name() -> String {
        format!("perface/{}", F::NAME)
    }
    fn build(s: Source) -> Self {
        let mut loops: Vec<Option<Loop>> = s.loops.into_iter().map(Some).collect();
        let faces = s
            .faces
            .into_iter()
            .map(|f| {
                let ls = f.loops.iter().map(|&l| must!(loops[l as usize].take()).uses).collect();
                Arc::new(FaceRecord { loops: ls, surface: f.surface, tol: f.tol, cells: FaceCells::default() })
            })
            .collect();
        Self { faces: Table::from_vec(faces), edges: Table::from_vec(s.edges), verts: Table::from_vec(s.verts) }
    }
    fn n_faces(&self) -> usize {
        self.faces.len()
    }
    fn face_edges(&self, f: u32, out: &mut Vec<u32>) {
        for l in &self.rec(f).loops {
            out.extend(l.iter().map(|u| u.edge));
        }
    }
    #[inline]
    fn edge(&self, e: u32) -> &Edge {
        must!(self.edges.get(e as usize))
    }
    #[inline]
    fn vertex(&self, v: u32) -> &Vertex {
        must!(self.verts.get(v as usize))
    }
    fn surface(&self, f: u32) -> &Geo {
        &self.rec(f).surface
    }
    #[inline]
    fn face_tol(&self, f: u32) -> f64 {
        let r = self.rec(f);
        r.tol + r.surface[0]
    }
    fn cells(&self, f: u32) -> &FaceCells {
        &self.rec(f).cells
    }
    fn trims(&self, f: u32) -> Vec<&[f64]> {
        self.rec(f).loops.iter().flat_map(|l| l.iter().map(|u| &*u.trim)).collect()
    }
    fn traverse(&self) -> f64 {
        let mut s = 0.0;
        for fi in 0..self.faces.len() {
            let r = self.rec(fi as u32);
            s += r.tol;
            for l in &r.loops {
                for u in l {
                    let e = self.edge(u.edge);
                    s += e.tol;
                    for v in e.v {
                        s += self.vertex(v).p[0];
                    }
                }
            }
        }
        s
    }
    fn walk(&self, start: u32, steps: usize) -> u32 {
        let mut f = start;
        for k in 0..steps {
            let l = &self.rec(f).loops[0];
            let e = self.edge(l[k % l.len()].edge);
            f = if e.faces[0] == f { e.faces[1] } else { e.faces[0] };
        }
        f
    }
    fn set_surface(&mut self, f: u32, g: Geo) {
        self.rec_mut(f).surface = g;
    }
    fn move_vertex(&mut self, v: u32, dz: f64) {
        must!(self.verts.get_mut(v as usize)).p[2] += dz;
    }
    fn set_curve(&mut self, e: u32, g: Geo, tol: f64) {
        let edge = must!(self.edges.get_mut(e as usize));
        edge.curve = g;
        edge.tol = tol;
    }
    fn set_edge_tol(&mut self, e: u32, tol: f64) {
        must!(self.edges.get_mut(e as usize)).tol = tol;
    }
    fn retrim(&mut self, f: u32, edges: &BTreeSet<u32>, salt: f64) {
        for l in &mut self.rec_mut(f).loops {
            for u in l {
                if edges.contains(&u.edge) {
                    u.trim = new_trim(salt, u.edge);
                }
            }
        }
    }
    fn reset_cells(&mut self, f: u32) {
        self.rec_mut(f).cells = FaceCells::default();
    }
}
