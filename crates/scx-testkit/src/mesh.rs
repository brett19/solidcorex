// Written for solidcorex. Apache-2.0.
//! The invariants of a triangle mesh as a test oracle (research 72 §6.4, T13): closed,
//! manifold, consistently oriented, free of degenerate and folded triangles, and enclosing
//! the volume it should, to a stated gap.
//!
//! A [`Mesh`] is plain data: vertex positions and triangles of vertex indices, wound so the
//! right-hand normal points out of the solid. `scx-tess` does not exist yet and a testkit
//! depends on no library type; a test adapts the library's mesh by a thin function. The
//! checks are written for clarity with maps and sets, so a library's faster bookkeeping and
//! this oracle share no bug.
//!
//! [`violations`] lists every invariant that fails; an empty list is a pass. The volume is
//! checked against a reference by [`volume_gap`] with a bound the caller states; [`chord_bound`]
//! gives the bound for a mesh whose vertices lie on the surface: chord times area.

use std::collections::{BTreeMap, BTreeSet};

/// A triangle mesh: positions and triangles of indices into them.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    /// Vertex positions.
    pub vertices: Vec<[f64; 3]>,
    /// Triangles, each wound counter-clockwise seen from outside the solid.
    pub triangles: Vec<[u32; 3]>,
}

/// The thresholds of the shape checks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    /// A triangle whose doubled area is below this fraction of its longest edge squared is
    /// degenerate (a sliver or a point). `0.0` flags only exactly zero area.
    pub min_area_ratio: f64,
    /// Two triangles sharing an edge fold over when their normals' dot product is below this.
    /// `-0.99` flags a dihedral turn within about 8 degrees of a full reversal; `-1.0` flags
    /// nothing.
    pub fold_cos: f64,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            min_area_ratio: 1e-9,
            fold_cos: -0.99,
        }
    }
}

/// One invariant a mesh fails.
#[derive(Clone, Debug, PartialEq)]
pub enum Violation {
    /// A triangle names a vertex that is not there.
    IndexOutOfRange {
        /// The triangle.
        triangle: usize,
    },
    /// A vertex has a coordinate that is not finite.
    NonFinite {
        /// The vertex.
        vertex: usize,
    },
    /// An edge is used by one triangle: the mesh is open.
    Boundary {
        /// The edge, as its two vertices in increasing order.
        edge: [u32; 2],
    },
    /// An edge is used by more than two triangles: not a manifold.
    NonManifoldEdge {
        /// The edge, as its two vertices in increasing order.
        edge: [u32; 2],
        /// How many uses.
        uses: usize,
    },
    /// Two triangles use an edge in the same direction: their windings disagree.
    Orientation {
        /// The edge, as its two vertices in increasing order.
        edge: [u32; 2],
    },
    /// The triangles around a vertex do not form one fan: a pinched vertex, or two cones
    /// meeting at a point.
    NonManifoldVertex {
        /// The vertex.
        vertex: u32,
    },
    /// A triangle of zero or sliver area, or with a repeated vertex.
    Degenerate {
        /// The triangle.
        triangle: usize,
    },
    /// Two triangles sharing an edge turn back over each other.
    Fold {
        /// The first triangle.
        a: usize,
        /// The second triangle.
        b: usize,
    },
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

impl Mesh {
    fn corners(&self, t: [u32; 3]) -> Option<[[f64; 3]; 3]> {
        let p = |i: u32| self.vertices.get(i as usize).copied();
        Some([p(t[0])?, p(t[1])?, p(t[2])?])
    }

    /// A triangle's normal, not normalized (twice its area in length); `None` if an index is
    /// out of range.
    pub fn normal(&self, t: usize) -> Option<[f64; 3]> {
        let [a, b, c] = self.corners(*self.triangles.get(t)?)?;
        Some(cross(sub(b, a), sub(c, a)))
    }

    /// The total surface area.
    pub fn area(&self) -> f64 {
        let mut s = Sum::default();
        for t in 0..self.triangles.len() {
            if let Some(n) = self.normal(t) {
                s.add(0.5 * dot(n, n).sqrt());
            }
        }
        s.total()
    }

    /// The signed volume the triangles enclose, `sum of (a . (b x c)) / 6` about the vertex
    /// mean (a shift that keeps far-from-origin meshes accurate), summed with compensation.
    /// Positive when the windings point outwards. Meaningful for a closed mesh only.
    pub fn signed_volume(&self) -> f64 {
        let n = self.vertices.len().max(1) as f64;
        let mut m = [0.0; 3];
        for v in &self.vertices {
            for (mk, vk) in m.iter_mut().zip(v) {
                *mk += vk / n;
            }
        }
        let mut s = Sum::default();
        for &t in &self.triangles {
            if let Some([a, b, c]) = self.corners(t) {
                s.add(dot(sub(a, m), cross(sub(b, m), sub(c, m))) / 6.0);
            }
        }
        s.total()
    }
}

/// Neumaier's compensated sum.
#[derive(Default)]
struct Sum {
    s: f64,
    c: f64,
}

impl Sum {
    fn add(&mut self, x: f64) {
        let t = self.s + x;
        self.c += if self.s.abs() >= x.abs() {
            (self.s - t) + x
        } else {
            (x - t) + self.s
        };
        self.s = t;
    }

    fn total(&self) -> f64 {
        self.s + self.c
    }
}

/// Every invariant the mesh fails, in a fixed order (index and finiteness first, then edges,
/// vertices, triangle shapes, folds). Empty if none.
pub fn violations(mesh: &Mesh, limits: Limits) -> Vec<Violation> {
    let mut out = Vec::new();
    for (i, v) in mesh.vertices.iter().enumerate() {
        if v.iter().any(|x| !x.is_finite()) {
            out.push(Violation::NonFinite { vertex: i });
        }
    }
    let mut in_range = true;
    for (t, tri) in mesh.triangles.iter().enumerate() {
        if mesh.corners(*tri).is_none() {
            out.push(Violation::IndexOutOfRange { triangle: t });
            in_range = false;
        }
    }
    if !in_range || !out.is_empty() {
        return out;
    }

    // Edge uses: for each undirected edge, the triangles using it and whether along the
    // edge's increasing direction.
    let mut uses: BTreeMap<[u32; 2], Vec<(usize, bool)>> = BTreeMap::new();
    for (t, tri) in mesh.triangles.iter().enumerate() {
        for k in 0..3 {
            let (Some(&a), Some(&b)) = (tri.get(k), tri.get((k + 1) % 3)) else {
                continue;
            };
            if a == b {
                continue;
            }
            uses.entry([a.min(b), a.max(b)])
                .or_default()
                .push((t, a < b));
        }
    }
    for (&edge, u) in &uses {
        match u.as_slice() {
            [_] => out.push(Violation::Boundary { edge }),
            [(_, d0), (_, d1)] => {
                if d0 == d1 {
                    out.push(Violation::Orientation { edge });
                }
            }
            _ => out.push(Violation::NonManifoldEdge {
                edge,
                uses: u.len(),
            }),
        }
    }

    // Vertex fans: the edges opposite a vertex, over its triangles, must form one cycle.
    let mut link: BTreeMap<u32, Vec<[u32; 2]>> = BTreeMap::new();
    for tri in &mesh.triangles {
        for k in 0..3 {
            if let (Some(&v), Some(&a), Some(&b)) =
                (tri.get(k), tri.get((k + 1) % 3), tri.get((k + 2) % 3))
            {
                link.entry(v).or_default().push([a, b]);
            }
        }
    }
    for (&vertex, edges) in &link {
        if !is_one_cycle(edges) {
            out.push(Violation::NonManifoldVertex { vertex });
        }
    }

    // Shapes.
    let mut flat = BTreeSet::new();
    for (t, tri) in mesh.triangles.iter().enumerate() {
        let repeated =
            tri.first() == tri.get(1) || tri.get(1) == tri.get(2) || tri.first() == tri.get(2);
        let sliver = mesh.corners(*tri).is_none_or(|[a, b, c]| {
            let n = cross(sub(b, a), sub(c, a));
            let longest = [sub(b, a), sub(c, b), sub(a, c)]
                .iter()
                .map(|e| dot(*e, *e))
                .fold(0.0, f64::max);
            let twice_area = dot(n, n).sqrt();
            twice_area <= limits.min_area_ratio * longest
        });
        if repeated || sliver {
            out.push(Violation::Degenerate { triangle: t });
            flat.insert(t);
        }
    }
    // Folds: neighbours across a two-use edge whose normals oppose. A degenerate triangle has
    // no normal and is skipped, so a fold is reported only between sound triangles.
    for u in uses.values() {
        if let [(a, _), (b, _)] = u.as_slice()
            && !flat.contains(a)
            && !flat.contains(b)
            && let (Some(na), Some(nb)) = (mesh.normal(*a), mesh.normal(*b))
        {
            let c = dot(na, nb) / (dot(na, na).sqrt() * dot(nb, nb).sqrt());
            if c < limits.fold_cos {
                out.push(Violation::Fold { a: *a, b: *b });
            }
        }
    }
    out
}

/// Whether undirected edges form exactly one simple cycle: every vertex of degree two and one
/// connected piece.
fn is_one_cycle(edges: &[[u32; 2]]) -> bool {
    let mut adj: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for &[a, b] in edges {
        adj.entry(a).or_default().push(b);
        adj.entry(b).or_default().push(a);
    }
    if adj.values().any(|n| n.len() != 2) {
        return false;
    }
    let Some(&first) = adj.keys().next() else {
        return false;
    };
    let mut seen = BTreeSet::from([first]);
    let mut stack = vec![first];
    while let Some(v) = stack.pop() {
        for &w in adj.get(&v).into_iter().flatten() {
            if seen.insert(w) {
                stack.push(w);
            }
        }
    }
    seen.len() == adj.len()
}

/// The result of comparing a mesh's volume with a reference.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VolumeGap {
    /// The mesh's signed volume.
    pub mesh: f64,
    /// The reference volume.
    pub reference: f64,
    /// `mesh - reference`.
    pub gap: f64,
    /// The bound the caller stated.
    pub bound: f64,
}

impl VolumeGap {
    /// The gap is within the stated bound.
    pub fn holds(&self) -> bool {
        self.gap.abs() <= self.bound
    }
}

/// The mesh's signed volume against a reference volume (a closed form, T9) and a stated bound
/// on `|gap|`. A mesh wound inwards has a negative volume and fails against a positive
/// reference, so orientation to the outside is checked here too.
pub fn volume_gap(mesh: &Mesh, reference: f64, bound: f64) -> VolumeGap {
    let v = mesh.signed_volume();
    VolumeGap {
        mesh: v,
        reference,
        gap: v - reference,
        bound,
    }
}

/// The bound on the volume gap of a mesh whose vertices lie on the surface and whose
/// triangles stay within `chord` of it: `chord * area`, with `area` the surface's area. The
/// mesh differs from the solid only in the shell between surface and triangles, at most
/// `chord` thick over the surface's area.
pub fn chord_bound(chord: f64, area: f64) -> f64 {
    chord * area
}
