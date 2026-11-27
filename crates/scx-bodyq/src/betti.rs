// Written for solidcorex. Apache-2.0.
//! [`betti`]: the Betti numbers of one shell, exactly, from its cells alone (research 74
//! §2.3, BW1). No geometry is read except which faces are a loopless torus.
//!
//! **What is counted.** The shell is the cell complex of its faces, wire edges and lone
//! vertices. Its cells are counted shell by shell: a vertex or edge that faces of two shells
//! both use is one cell in each shell's complex, so two bodies touching at a vertex or along
//! an edge (each its own shell) keep their own numbers, and two faces of one shell meeting
//! at a vertex share that vertex's one cell. A face is a sphere with one hole per loop
//! (`2 - L` for its Euler characteristic), except a face with no loop on a torus, which is
//! the closed torus (`0`). An edge collapsed to a point is no 1-cell (its vertex is). A
//! seam is an edge like any other.
//!
//! **The numbers** (rational homology). `b0` counts the components connected through shared
//! vertices, `chi = V - E + sum(face chi)`, `b2` is the dimension of the 2-cycles (the
//! combinations of faces, each face once and oriented by its own loops, whose boundary on
//! every edge cancels: the nullity of the face-by-edge boundary matrix, found by exact
//! elimination over the integers), and `b1 = b0 + b2 - chi` (Euler-Poincare). A closed
//! orientable surface of genus `g` gives `(1, 2g, 1)`; a disc sheet `(1, 0, 0)`; `n` closed
//! lumps pinched together at vertices or along edges (one shell, a D5 contact the body
//! keeps as one shell) give `b2 = n`. A face with both sides in the shell is one cell. The
//! face side's flip does not matter: it changes a column's sign, not the nullity.
//!
//! **Pure and deterministic.** The body is `&Brep`, no budget is drawn (the work is a
//! sparse elimination whose rows are almost all two-term, so close to linear in the
//! shell's uses), and every set is visited in id order.

use std::collections::{BTreeMap, BTreeSet};

use scx_brep::{Brep, EdgeId, EdgeShape, FaceId, ShellId, VertexId};
use scx_num::frame::Sense;
use scx_surface::Surface;

use crate::error::{BodyqDefect, BodyqError, BodyqRefusal};

/// The Betti numbers of a shell's cell complex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Betti {
    /// Connected components.
    pub b0: u32,
    /// Independent loops that bound nothing: `2 g` for a closed orientable surface of genus
    /// `g`.
    pub b1: u32,
    /// Independent 2-cycles (voids): one per closed lump.
    pub b2: u32,
}

/// A union-find over indices; a missing index is its own root.
#[derive(Default)]
struct Sets(BTreeMap<usize, usize>);

impl Sets {
    fn find(&mut self, i: usize) -> usize {
        let mut root = i;
        while let Some(&p) = self.0.get(&root) {
            if p == root {
                break;
            }
            root = p;
        }
        let mut at = i;
        while let Some(&p) = self.0.get(&at) {
            if p == root {
                break;
            }
            self.0.insert(at, root);
            at = p;
        }
        root
    }

    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        self.0.insert(hi, lo);
        self.0.entry(lo).or_insert(lo);
    }
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

type Row = BTreeMap<usize, i128>;

/// The rank of the matrix whose rows are sparse (column to coefficient), by elimination in
/// exact integers, rows taken shortest first. `None` when an entry would overflow.
fn rank(rows: Vec<Row>) -> Option<usize> {
    let mut order: Vec<(usize, usize)> =
        rows.iter().enumerate().map(|(i, r)| (r.len(), i)).collect();
    order.sort_unstable();
    let mut rows: BTreeMap<usize, Row> = rows.into_iter().enumerate().collect();
    let mut by_col: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (&r, row) in &rows {
        for &c in row.keys() {
            by_col.entry(c).or_default().insert(r);
        }
    }
    let mut rank = 0;
    for (_, p) in order {
        let Some(pivot) = rows.remove(&p) else {
            continue;
        };
        // The pivot: the column of the smallest coefficient, lowest column.
        let Some((&c, &a)) = pivot.iter().min_by_key(|&(&c, &v)| (v.abs(), c)) else {
            continue;
        };
        rank += 1;
        for k in pivot.keys() {
            if let Some(set) = by_col.get_mut(k) {
                set.remove(&p);
            }
        }
        let others: Vec<usize> = by_col.get(&c).into_iter().flatten().copied().collect();
        for r in others {
            let Some(row) = rows.get(&r) else { continue };
            let b = *row.get(&c)?;
            let g = gcd(a, b);
            let (ma, mb) = (a / g, b / g);
            let mut next: Row = BTreeMap::new();
            for (&k, &v) in row {
                next.insert(k, v.checked_mul(ma)?);
            }
            for (&k, &v) in &pivot {
                let e = next.entry(k).or_insert(0);
                *e = e.checked_sub(v.checked_mul(mb)?)?;
            }
            next.retain(|_, v| *v != 0);
            let g = next.values().fold(0, |g, &v| gcd(g, v));
            if g > 1 {
                for v in next.values_mut() {
                    *v /= g;
                }
            }
            for k in row.keys() {
                if !next.contains_key(k)
                    && let Some(set) = by_col.get_mut(k)
                {
                    set.remove(&r);
                }
            }
            for &k in next.keys() {
                by_col.entry(k).or_default().insert(r);
            }
            rows.insert(r, next);
        }
    }
    Some(rank)
}

/// The Betti numbers of `shell`'s cell complex (see the module doc for what is counted).
///
/// # Errors
///
/// [`BodyqRefusal::NoShell`] when `shell` is not in `body`; [`BodyqDefect::Betti`] when the
/// cells give a negative `b1`, which no complex whose faces are spheres with holes (or a
/// closed torus) can, or when the elimination overflows.
pub fn betti(body: &Brep, shell: ShellId) -> Result<Betti, BodyqError> {
    let s = body
        .shell(shell)
        .ok_or(BodyqError::Refused(BodyqRefusal::NoShell { shell }))?;
    let defect = BodyqError::Defect(BodyqDefect::Betti { shell });
    let mut vertices: BTreeSet<VertexId> = BTreeSet::new();
    let mut edges: BTreeSet<EdgeId> = BTreeSet::new();
    let mut faces: BTreeMap<FaceId, i64> = BTreeMap::new();
    // Per edge, each face's signed count of uses.
    let mut cols: BTreeMap<EdgeId, BTreeMap<FaceId, i128>> = BTreeMap::new();
    let mut joins: Vec<(FaceId, VertexId)> = Vec::new();
    for (f, _) in s.faces() {
        if faces.contains_key(&f.id()) {
            continue;
        }
        let loops = i64::try_from(f.loops().len()).map_err(|_| defect)?;
        let torus = loops == 0 && matches!(f.surface(), Surface::Torus(_));
        faces.insert(f.id(), if torus { 0 } else { 2 - loops });
        for u in f.uses() {
            let e = u.edge();
            vertices.insert(u.start().id());
            vertices.insert(u.end().id());
            joins.push((f.id(), u.start().id()));
            if !matches!(e.geometry().shape, EdgeShape::Collapsed) {
                edges.insert(e.id());
                let sign = if u.sense() == Sense::Forward { 1 } else { -1 };
                *cols.entry(e.id()).or_default().entry(f.id()).or_insert(0) += sign;
            }
        }
    }
    let mut wires: Vec<[VertexId; 2]> = Vec::new();
    for e in s.wires() {
        vertices.extend(e.ends());
        wires.push(e.ends());
        if !matches!(e.geometry().shape, EdgeShape::Collapsed) {
            edges.insert(e.id());
        }
    }
    vertices.extend(s.lone_vertices().map(scx_brep::Vertex::id));
    let vi: BTreeMap<VertexId, usize> = vertices.iter().enumerate().map(|(i, &v)| (v, i)).collect();
    let fi: BTreeMap<FaceId, usize> = faces.keys().enumerate().map(|(i, &f)| (f, i)).collect();
    // Components: vertices, then faces.
    let mut sets = Sets::default();
    for (f, v) in &joins {
        if let (Some(&a), Some(&b)) = (fi.get(f), vi.get(v)) {
            sets.join(vi.len() + a, b);
        }
    }
    for [a, b] in &wires {
        if let (Some(&a), Some(&b)) = (vi.get(a), vi.get(b)) {
            sets.join(a, b);
        }
    }
    let roots: BTreeSet<usize> = (0..vi.len() + fi.len()).map(|i| sets.find(i)).collect();
    let rows: Vec<BTreeMap<usize, i128>> = cols
        .values()
        .map(|c| {
            c.iter()
                .filter(|&(_, &v)| v != 0)
                .filter_map(|(f, &v)| fi.get(f).map(|&i| (i, v)))
                .collect()
        })
        .collect();
    let b2 = fi.len() - rank(rows).ok_or(defect)?;
    let chi = i64::try_from(vi.len()).map_err(|_| defect)?
        - i64::try_from(edges.len()).map_err(|_| defect)?
        + faces.values().sum::<i64>();
    let b0 = i64::try_from(roots.len()).map_err(|_| defect)?;
    let b2 = i64::try_from(b2).map_err(|_| defect)?;
    let b1 = u32::try_from(b0 + b2 - chi).map_err(|_| defect)?;
    Ok(Betti {
        b0: u32::try_from(b0).map_err(|_| defect)?,
        b1,
        b2: u32::try_from(b2).map_err(|_| defect)?,
    })
}
