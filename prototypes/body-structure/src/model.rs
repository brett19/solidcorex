//! The synthetic body: entity records and a generator.
//!
//! The body is a closed quad mesh on a torus (W x H faces): every edge has two faces, every
//! vertex four edges, every face one loop of four edge uses. Geometry is immutable and held by
//! `Arc`, so copying a record never copies geometry. Sizes (in f64): a surface 100 (a 5x5
//! rational patch), a curve 24 (six rational control points), a trim 16. These are assumptions
//! for a NURBS-heavy body; analytic faces are smaller.

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, OnceLock};

/// Immutable geometry coefficients.
pub type Geo = Arc<[f64]>;

pub const SURFACE_LEN: usize = 100;
pub const CURVE_LEN: usize = 24;
pub const TRIM_LEN: usize = 16;
pub const TREE_LEN: usize = 256;

/// Geometry of `len` coefficients derived from `seed`.
pub fn geo(len: usize, seed: f64) -> Geo {
    (0..len).map(|i| seed + i as f64 * 0.001).collect()
}

#[derive(Clone, Debug)]
pub struct Vertex {
    pub p: [f64; 3],
    pub tol: f64,
    /// Edges meeting here (heap, like a vertex's use list).
    pub edges: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct Edge {
    pub v: [u32; 2],
    /// The two faces on either side.
    pub faces: [u32; 2],
    pub curve: Geo,
    pub t: [f64; 2],
    pub tol: f64,
}

/// One use of an edge by a loop, with its trim (the edge's curve in the face's parameters).
#[derive(Clone, Debug)]
pub struct Use {
    pub edge: u32,
    pub forward: bool,
    pub trim: Geo,
}

#[derive(Clone, Debug)]
pub struct Loop {
    pub face: u32,
    pub uses: Vec<Use>,
}

/// Fill-once cells of a face: computed on first read, then fixed for the life of the face.
#[derive(Clone, Default, Debug)]
pub struct FaceCells {
    pub bbox: OnceLock<[f64; 6]>,
    pub tree: OnceLock<Geo>,
}

/// Count of cell computations (to show a carried cell is not recomputed).
pub static CELL_COMPUTES: AtomicU64 = AtomicU64::new(0);

impl FaceCells {
    /// The box, filled from `surface` on first read.
    pub fn bbox(&self, surface: &[f64]) -> [f64; 6] {
        *self.bbox.get_or_init(|| {
            CELL_COMPUTES.fetch_add(1, Relaxed);
            let mut b = [f64::MAX, f64::MAX, f64::MAX, f64::MIN, f64::MIN, f64::MIN];
            for c in surface.chunks_exact(4) {
                for k in 0..3 {
                    b[k] = b[k].min(c[k]);
                    b[k + 3] = b[k + 3].max(c[k]);
                }
            }
            b
        })
    }
    /// The tree, filled from `surface` and the loop trims on first read.
    pub fn tree(&self, surface: &[f64], trims: &[&[f64]]) -> Geo {
        self.tree
            .get_or_init(|| {
                CELL_COMPUTES.fetch_add(1, Relaxed);
                let t: f64 = trims.iter().map(|t| t.iter().sum::<f64>()).sum();
                (0..TREE_LEN).map(|i| surface[i % surface.len()] + t + i as f64).collect()
            })
            .clone()
    }
}

/// Face of the table form: loops by id, cells behind their own `Arc` so a table copy shares them.
#[derive(Clone, Debug)]
pub struct Face {
    pub loops: Vec<u32>,
    pub surface: Geo,
    pub tol: f64,
    pub cells: Arc<FaceCells>,
}

/// The body as plain vectors, the input to every representation's builder.
#[derive(Clone, Debug)]
pub struct Source {
    pub w: usize,
    pub h: usize,
    pub faces: Vec<Face>,
    pub loops: Vec<Loop>,
    pub edges: Vec<Edge>,
    pub verts: Vec<Vertex>,
}

impl Source {
    /// A torus quad mesh of `w * h` faces.
    pub fn torus(w: usize, h: usize) -> Self {
        let n = w * h;
        let vid = |i: usize, j: usize| ((j % h) * w + (i % w)) as u32;
        let fid = |i: usize, j: usize| ((j % h) * w + (i % w)) as u32;
        // edge 2*vid(i,j) runs +i from (i,j); edge 2*vid(i,j)+1 runs +j from (i,j)
        let eh = |i: usize, j: usize| 2 * vid(i, j);
        let ev = |i: usize, j: usize| 2 * vid(i, j) + 1;
        let mut verts = Vec::with_capacity(n);
        let mut edges = Vec::with_capacity(2 * n);
        for j in 0..h {
            for i in 0..w {
                verts.push(Vertex {
                    p: [i as f64, j as f64, 0.0],
                    tol: 1e-7,
                    edges: vec![eh(i, j), eh(i + w - 1, j), ev(i, j), ev(i, j + h - 1)],
                });
                let s = (j * w + i) as f64;
                edges.push(Edge {
                    v: [vid(i, j), vid(i + 1, j)],
                    faces: [fid(i, j), fid(i, j + h - 1)],
                    curve: geo(CURVE_LEN, s),
                    t: [0.0, 1.0],
                    tol: 1e-7,
                });
                edges.push(Edge {
                    v: [vid(i, j), vid(i, j + 1)],
                    faces: [fid(i, j), fid(i + w - 1, j)],
                    curve: geo(CURVE_LEN, s + 0.5),
                    t: [0.0, 1.0],
                    tol: 1e-7,
                });
            }
        }
        let mut faces = Vec::with_capacity(n);
        let mut loops = Vec::with_capacity(n);
        for j in 0..h {
            for i in 0..w {
                let f = fid(i, j);
                let s = f as f64;
                let uses = [(eh(i, j), true), (ev(i + 1, j), true), (eh(i, j + 1), false), (ev(i, j), false)]
                    .into_iter()
                    .enumerate()
                    .map(|(k, (edge, forward))| Use { edge, forward, trim: geo(TRIM_LEN, s + k as f64) })
                    .collect();
                loops.push(Loop { face: f, uses });
                faces.push(Face { loops: vec![f], surface: geo(SURFACE_LEN, s), tol: 1e-7, cells: Arc::default() });
            }
        }
        Self { w, h, faces, loops, edges, verts }
    }

    /// A square patch of about `k` faces starting at face (w/3, h/3): an edit's footprint.
    pub fn patch(&self, k: usize) -> Vec<u32> {
        let side = ((k as f64).sqrt().ceil() as usize).max(1);
        let mut out = Vec::with_capacity(k);
        'outer: for dj in 0..side {
            for di in 0..side {
                if out.len() == k {
                    break 'outer;
                }
                let i = (self.w / 3 + di) % self.w;
                let j = (self.h / 3 + dj) % self.h;
                out.push((j * self.w + i) as u32);
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// `k` faces spread over the body (deterministic): the worst case for chunked sharing.
    pub fn scattered(&self, k: usize) -> Vec<u32> {
        let n = self.faces.len();
        let mut out: Vec<u32> = (0..k).map(|i| ((i * n) / k + (i * 7) % (n / k).max(1)) as u32 % n as u32).collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// A small deterministic generator (for read benchmarks).
pub struct Lcg(pub u64);
impl Lcg {
    #[inline]
    pub fn next(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n
    }
}
