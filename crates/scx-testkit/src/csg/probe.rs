// Written for solidcorex. Apache-2.0.
//! Points on purpose (research 72 §3.4): points on each face, edge and vertex of a
//! recipe, points where two leaves' surfaces meet, and points just off them.
//!
//! Each leaf's faces, edges and vertices are sampled at seeded parameters in
//! `[0.02, 0.98]`, with the seams and poles of the round faces sampled on purpose too. A
//! sample is kept only when it lies on the result's boundary: among the points `10`
//! coincidences off it along its directions, the tree has a certain `In` and a certain
//! `Out` (at width one coincidence). So a leaf's face inside another leaf, or the shared
//! face of a coplanar union, gives none. Meets are found by walking each face's parameter
//! lines and bisecting where another leaf's `f` changes sign.
//!
//! A kept point gives a [`Probe`] at offset `0` and, for each direction (each normal
//! there, and their normalised sum at an edge or vertex; at a meet also their normalised
//! difference, since a meet's normals are each leaf's own and the solid may lie in any of
//! the four quarters they bound), probes at `+-k` coincidences for `k` in `{1/2, 2, 10}`.
//! A probe's offset is along its direction; which side that is, the caller asks
//! [`Csg::side`].

use super::{Csg, Leaf, Primitive, Side};
use scx_num::fmath;
use scx_num::hash::{Rng, mix};

/// What to sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbeSpec {
    /// The seed: equal specs give equal probes.
    pub seed: u64,
    /// Seeded samples per face and per edge.
    pub per_face: usize,
    /// The coincidence distance the offsets are multiples of.
    pub coincidence: f64,
    /// The half-size of the window sampled on an unbounded face, about the leaf's origin.
    pub extent: f64,
}

/// The kind of feature a probe was sampled from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FeatureKind {
    /// A face (a seam or pole of a round face included).
    Face,
    /// An edge between two faces of one leaf.
    Edge,
    /// A vertex of one leaf (an apex included).
    Vertex,
    /// Where the leaf's face meets leaf `other`'s surface.
    Meet {
        /// The other leaf.
        other: usize,
    },
}

/// The feature a probe was sampled from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Feature {
    /// The leaf, as [`Csg::leaves`] numbers them.
    pub leaf: usize,
    /// What on it.
    pub kind: FeatureKind,
}

/// One point on purpose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Probe {
    /// The point.
    pub point: [f64; 3],
    /// What it was sampled from.
    pub feature: Feature,
    /// The unit direction it is offset along (for an offset of `0`, the first one).
    pub dir: [f64; 3],
    /// The signed offset along `dir` from the feature point, a multiple of the coincidence
    /// (`0` for the point on the feature).
    pub offset: f64,
}

/// A sample in local coordinates with its local normals (covectors).
struct Local {
    q: [f64; 3],
    normals: Vec<[f64; 3]>,
    kind: FeatureKind,
}

type Patch<'a> = Box<dyn Fn(f64, f64) -> ([f64; 3], [f64; 3]) + 'a>;

/// The offsets, in coincidences.
const KS: [f64; 3] = [0.5, 2.0, 10.0];
/// Steps along a parameter line when looking for meets.
const WALK: usize = 32;

fn param(rng: &mut Rng) -> f64 {
    0.02 + 0.96 * rng.next_f64()
}

fn turn(u: f64) -> (f64, f64) {
    let (s, c) = fmath::sin_cos(core::f64::consts::TAU * u);
    (c, s)
}

fn add(a: [f64; 3], s: f64, d: [f64; 3]) -> [f64; 3] {
    [a[0] + s * d[0], a[1] + s * d[1], a[2] + s * d[2]]
}

fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (l > 1e-12 && l.is_finite()).then(|| v.map(|c| c / l))
}

const ZP: [f64; 3] = [0.0, 0.0, 1.0];
const ZM: [f64; 3] = [0.0, 0.0, -1.0];

fn axis(k: usize, s: f64) -> [f64; 3] {
    core::array::from_fn(|i| if i == k { s } else { 0.0 })
}

/// The faces as patches over `[0, 1]^2`, for samples and meets, with whether `u` is
/// periodic (a seam at `u = 0`) and whether `v` is.
fn patches(p: &Primitive, extent: f64) -> Vec<(Patch<'_>, bool, bool)> {
    let mut out: Vec<(Patch<'_>, bool, bool)> = Vec::new();
    let plane = |z: f64, n: [f64; 3]| -> Patch<'_> {
        Box::new(move |u, v| ([extent * (2.0 * u - 1.0), extent * (2.0 * v - 1.0), z], n))
    };
    match p {
        Primitive::HalfSpace => out.push((plane(0.0, ZP), false, false)),
        Primitive::Slab { height } => {
            out.push((plane(0.0, ZM), false, false));
            out.push((plane(*height, ZP), false, false));
        }
        Primitive::Block { size } => {
            for k in 0..3 {
                for s in [0.0, 1.0] {
                    let size = *size;
                    out.push((
                        Box::new(move |u, v| {
                            let (i, j) = ((k + 1) % 3, (k + 2) % 3);
                            let q = core::array::from_fn(|m| {
                                let g = size.get(m).copied().unwrap_or(0.0);
                                if m == k {
                                    s * g
                                } else if m == i {
                                    u * g
                                } else if m == j {
                                    v * g
                                } else {
                                    0.0
                                }
                            });
                            (q, axis(k, 2.0 * s - 1.0))
                        }),
                        false,
                        false,
                    ));
                }
            }
        }
        Primitive::Cylinder { radius, height } => {
            let (r, h) = (*radius, *height);
            out.push((
                Box::new(move |u, v| {
                    let (c, s) = turn(u);
                    ([r * c, r * s, v * h], [c, s, 0.0])
                }),
                true,
                false,
            ));
            for (z, n) in [(0.0, ZM), (h, ZP)] {
                out.push((disc(r, z, n), true, false));
            }
        }
        Primitive::Frustum {
            radius0,
            radius1,
            height,
        } => {
            let (r0, r1, h) = (*radius0, *radius1, *height);
            out.push((
                Box::new(move |u, v| {
                    let (c, s) = turn(u);
                    let r = r0 + (r1 - r0) * v;
                    let n = unit([c, s, (r0 - r1) / h]).unwrap_or([c, s, 0.0]);
                    ([r * c, r * s, v * h], n)
                }),
                true,
                false,
            ));
            for (r, z, n) in [(r0, 0.0, ZM), (r1, h, ZP)] {
                if r > 0.0 {
                    out.push((disc(r, z, n), true, false));
                }
            }
        }
        Primitive::Sphere { radius } => {
            let r = *radius;
            out.push((
                Box::new(move |u, v| {
                    let (c, s) = turn(u);
                    let (sp, cp) = fmath::sin_cos(core::f64::consts::PI * (v - 0.5));
                    let n = [cp * c, cp * s, sp];
                    (n.map(|x| r * x), n)
                }),
                true,
                false,
            ));
        }
        Primitive::Torus { major, minor } => {
            let (rr, r) = (*major, *minor);
            out.push((
                Box::new(move |u, v| {
                    let (c, s) = turn(u);
                    let (cv, sv) = turn(v);
                    let n = [cv * c, cv * s, sv];
                    ([(rr + r * cv) * c, (rr + r * cv) * s, r * sv], n)
                }),
                true,
                true,
            ));
        }
        Primitive::Extrusion { profile, height } => {
            let h = *height;
            for piece in profile.pieces() {
                let sign = outward(profile, piece.at(0.5));
                out.push((
                    Box::new(move |u, v| {
                        let (p, n) = piece.at(u);
                        ([p[0], p[1], v * h], [sign * n[0], sign * n[1], 0.0])
                    }),
                    false,
                    false,
                ));
            }
        }
    }
    out
}

/// `1` when a piece's left normal points out of the profile, else `-1`.
fn outward(profile: &super::Profile, (p, n): ([f64; 2], [f64; 2])) -> f64 {
    let e = 1e-6 * profile.scale();
    if profile
        .signed_distance([p[0] + e * n[0], p[1] + e * n[1]])
        .hi
        > 0.0
    {
        1.0
    } else {
        -1.0
    }
}

fn disc<'a>(r: f64, z: f64, n: [f64; 3]) -> Patch<'a> {
    Box::new(move |u, v| {
        let (c, s) = turn(u);
        let rho = r * v.sqrt();
        ([rho * c, rho * s, z], n)
    })
}

/// The seeded samples of one primitive, in local coordinates.
fn locals(p: &Primitive, rng: &mut Rng, spec: &ProbeSpec) -> Vec<Local> {
    let n = spec.per_face.max(1);
    let mut out = Vec::new();
    let face = |q, nn| Local {
        q,
        normals: vec![nn],
        kind: FeatureKind::Face,
    };
    for (patch, pu, pv) in patches(p, spec.extent) {
        for _ in 0..n {
            let (q, nn) = patch(param(rng), param(rng));
            out.push(face(q, nn));
        }
        if pu {
            let (q, nn) = patch(0.0, param(rng));
            out.push(face(q, nn));
        }
        if pv {
            let (q, nn) = patch(param(rng), 0.0);
            out.push(face(q, nn));
        }
    }
    let mut edge = |f: &dyn Fn(f64) -> ([f64; 3], [f64; 3], [f64; 3])| {
        for _ in 0..n {
            let (q, a, b) = f(param(rng));
            out.push(Local {
                q,
                normals: vec![a, b],
                kind: FeatureKind::Edge,
            });
        }
    };
    let mut vertices: Vec<([f64; 3], Vec<[f64; 3]>)> = Vec::new();
    let mut extra: Vec<([f64; 3], [f64; 3])> = Vec::new();
    match p {
        Primitive::HalfSpace | Primitive::Slab { .. } | Primitive::Torus { .. } => {}
        Primitive::Block { size } => {
            let g = |m: usize| size.get(m).copied().unwrap_or(0.0);
            for k in 0..3 {
                let (i, j) = ((k + 1) % 3, (k + 2) % 3);
                for (si, sj) in [(0.0, 0.0), (0.0, 1.0), (1.0, 0.0), (1.0, 1.0)] {
                    edge(&|t| {
                        let q = core::array::from_fn(|m| {
                            if m == k {
                                t * g(k)
                            } else if m == i {
                                si * g(i)
                            } else {
                                sj * g(j)
                            }
                        });
                        (q, axis(i, 2.0 * si - 1.0), axis(j, 2.0 * sj - 1.0))
                    });
                }
            }
            for c in 0..8 {
                let s = |m: usize| f64::from((c >> m) & 1);
                vertices.push((
                    [s(0) * g(0), s(1) * g(1), s(2) * g(2)],
                    (0..3).map(|m| axis(m, 2.0 * s(m) - 1.0)).collect(),
                ));
            }
        }
        Primitive::Cylinder { radius, height } => {
            for (z, nz) in [(0.0, ZM), (*height, ZP)] {
                edge(&|t| {
                    let (c, s) = turn(t);
                    ([radius * c, radius * s, z], [c, s, 0.0], nz)
                });
                extra.push(([0.0, 0.0, z], nz));
            }
        }
        Primitive::Frustum {
            radius0,
            radius1,
            height,
        } => {
            let slant = (radius0 - radius1) / height;
            for (r, z, nz) in [(*radius0, 0.0, ZM), (*radius1, *height, ZP)] {
                if r > 0.0 {
                    edge(&|t| {
                        let (c, s) = turn(t);
                        let side = unit([c, s, slant]).unwrap_or([c, s, 0.0]);
                        ([r * c, r * s, z], side, nz)
                    });
                    extra.push(([0.0, 0.0, z], nz));
                } else {
                    vertices.push(([0.0, 0.0, z], vec![nz]));
                }
            }
        }
        Primitive::Sphere { radius } => {
            extra.push(([0.0, 0.0, *radius], ZP));
            extra.push(([0.0, 0.0, -radius], ZM));
        }
        Primitive::Extrusion { profile, height } => {
            let h = *height;
            for piece in profile.pieces() {
                let sign = outward(profile, piece.at(0.5));
                for (z, nz) in [(0.0, ZM), (h, ZP)] {
                    edge(&|t| {
                        let (p, nn) = piece.at(t);
                        ([p[0], p[1], z], [sign * nn[0], sign * nn[1], 0.0], nz)
                    });
                }
                if let Some(n1) = piece.corner() {
                    let (p, n0) = piece.at(1.0);
                    let (a, b) = (
                        [sign * n0[0], sign * n0[1], 0.0],
                        [sign * n1[0], sign * n1[1], 0.0],
                    );
                    edge(&|t| ([p[0], p[1], t * h], a, b));
                    for (z, nz) in [(0.0, ZM), (h, ZP)] {
                        vertices.push(([p[0], p[1], z], vec![a, b, nz]));
                    }
                }
            }
            // The caps: seeded points of the profile's box that fall inside it.
            let (lo, hi) = profile.bounds();
            for (z, nz) in [(0.0, ZM), (h, ZP)] {
                let mut found = 0;
                for _ in 0..20 * n {
                    if found == n {
                        break;
                    }
                    let x = lo[0] + (hi[0] - lo[0]) * rng.next_f64();
                    let y = lo[1] + (hi[1] - lo[1]) * rng.next_f64();
                    if profile.inside([x, y]) {
                        extra.push(([x, y, z], nz));
                        found += 1;
                    }
                }
            }
        }
    }
    out.extend(extra.into_iter().map(|(q, nn)| face(q, nn)));
    out.extend(vertices.into_iter().map(|(q, normals)| Local {
        q,
        normals,
        kind: FeatureKind::Vertex,
    }));
    out
}

/// The directions a probe is offset along: each normal and, when there are several, their
/// normalised sum (and, for a meet, their difference).
fn directions(normals: &[[f64; 3]], meet: bool) -> Vec<[f64; 3]> {
    let mut d = normals.to_vec();
    if normals.len() > 1 {
        let s = normals.iter().fold([0.0; 3], |a, n| add(a, 1.0, *n));
        d.extend(unit(s));
    }
    // A meet's two normals are each leaf's own, not the tree's outward: the solid may lie
    // in any of the four quarters they bound, so the difference goes in as well.
    if let (true, [a, b]) = (meet, normals) {
        d.extend(unit(add(*a, -1.0, *b)));
    }
    d
}

impl Csg {
    /// The points on purpose (module notes), in a fixed order: leaf by leaf, faces, then
    /// edges, then extra face spots (seams, poles, cap centres), vertices and meets.
    pub fn probes(&self, spec: &ProbeSpec) -> Vec<Probe> {
        let c = spec.coincidence;
        let leaves = self.leaves();
        let mut out = Vec::new();
        for (i, leaf) in leaves.iter().enumerate() {
            let mut rng = Rng::new(mix(&[spec.seed, i as u64]));
            let mut found: Vec<(Feature, [f64; 3], Vec<[f64; 3]>)> = Vec::new();
            for l in locals(&leaf.primitive, &mut rng, spec) {
                let normals: Vec<_> = l
                    .normals
                    .iter()
                    .map(|n| leaf.placement.normal(*n))
                    .collect();
                let feature = Feature {
                    leaf: i,
                    kind: l.kind,
                };
                found.push((feature, leaf.placement.world(l.q), normals));
            }
            for (j, other) in leaves.iter().enumerate() {
                if j != i {
                    meets(leaf, other, &mut rng, spec, &mut |p, n| {
                        let normals = vec![n, gradient(other, p)];
                        let kind = FeatureKind::Meet { other: j };
                        found.push((Feature { leaf: i, kind }, p, normals));
                    });
                }
            }
            for (feature, p, normals) in found {
                let dirs = directions(&normals, matches!(feature.kind, FeatureKind::Meet { .. }));
                let sides: Vec<Side> = dirs
                    .iter()
                    .flat_map(|d| [-10.0, 10.0].map(|k| self.side(add(p, k * c, *d), c)))
                    .collect();
                if !(sides.contains(&Side::In) && sides.contains(&Side::Out)) {
                    continue;
                }
                let first = dirs.first().copied().unwrap_or([0.0; 3]);
                out.push(Probe {
                    point: p,
                    feature,
                    dir: first,
                    offset: 0.0,
                });
                for d in &dirs {
                    for k in KS {
                        for s in [-1.0, 1.0] {
                            let offset = s * k * c;
                            out.push(Probe {
                                point: add(p, offset, *d),
                                feature,
                                dir: *d,
                                offset,
                            });
                        }
                    }
                }
            }
        }
        out
    }
}

/// The unit gradient of a leaf's `f` at `p` by central differences (zero when flat).
fn gradient(leaf: &Leaf, p: [f64; 3]) -> [f64; 3] {
    let h = 1e-7 * p.iter().fold(1.0, |m: f64, c| m.max(c.abs()));
    let g = core::array::from_fn(|k| {
        let e = axis(k, h);
        (leaf.value(add(p, 1.0, e)) - leaf.value(add(p, -1.0, e))) / (2.0 * h)
    });
    unit(g).unwrap_or([0.0; 3])
}

/// The points of `leaf`'s faces where `other`'s `f` changes sign, along seeded parameter
/// lines in both directions, each bisected to the last bit of the parameter.
fn meets(
    leaf: &Leaf,
    other: &Leaf,
    rng: &mut Rng,
    spec: &ProbeSpec,
    found: &mut dyn FnMut([f64; 3], [f64; 3]),
) {
    let lines = (spec.per_face / 4).max(2);
    #[allow(clippy::cast_precision_loss)] // small counts
    let step = 1.0 / WALK as f64;
    for (patch, _, _) in patches(&leaf.primitive, spec.extent) {
        for along_u in [true, false] {
            for _ in 0..lines {
                let fixed = param(rng);
                let at = |t: f64| {
                    let (q, n) = if along_u {
                        patch(t, fixed)
                    } else {
                        patch(fixed, t)
                    };
                    let p = leaf.placement.world(q);
                    (p, other.value(p), n)
                };
                #[allow(clippy::cast_precision_loss)] // small counts
                for k in 0..WALK {
                    let (mut a, mut b) = (k as f64 * step, (k + 1) as f64 * step);
                    let (fa, fb) = (at(a).1, at(b).1);
                    if !(fa < 0.0 && fb > 0.0 || fa > 0.0 && fb < 0.0) {
                        continue;
                    }
                    for _ in 0..60 {
                        let m = 0.5 * (a + b);
                        if m <= a || m >= b {
                            break;
                        }
                        if (at(m).1 < 0.0) == (fa < 0.0) {
                            a = m;
                        } else {
                            b = m;
                        }
                    }
                    let (p, _, n) = at(0.5 * (a + b));
                    found(p, leaf.placement.normal(n));
                }
            }
        }
    }
}
