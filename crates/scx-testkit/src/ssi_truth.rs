// Written for solidcorex. Apache-2.0.
//! Closed-form surface-surface intersection truths for step 5's general SSI (research 73 §9,
//! task SM0): each is the exact answer for a pair of surfaces in standard position, as
//! curves with exact points and parameters, the topology (loops, nodes, touches), and the
//! lengths and volumes where a closed form is known.
//!
//! A [`Truth`] holds the two [`Surf`]s (their signed distance and normal, so a residual is a
//! distance), the [`TruthCurve`]s (a pure function of one parameter, with its domain), the
//! [`Node`]s (points where the curve meets itself or another curve, with every parameter
//! that reaches them), isolated touch points, and whether the contact is a coincidence or a
//! tangency along curves. [`Truth::placed`] moves the whole answer by a [`Rigid`] motion.
//! [`Truth::hausdorff`] measures a candidate's polylines against the curves in both
//! directions.
//!
//! The cases (all constructors return `None` for arguments outside their stated ranges):
//!
//! - [`sphere_sphere`]: the circle on the radical plane, a touch, or nothing; [`tiny_loop`] is
//!   a plane at depth `delta` into a sphere.
//! - [`cylinders_perpendicular`] (axes meeting or skew by `e`): two loops, the Steinmetz pair
//!   of ellipses with two nodes (`r = R`, `e = 0`), a figure eight at `e = R - r`, one loop,
//!   a touch at `e = R + r`.
//! - [`cylinders_parallel`]: two lines, a tangent line, coincident or empty.
//! - [`sphere_cylinder`] and [`viviani`]: two loops, a figure eight when `rho = r + e`
//!   (Viviani's curve at `e = r`, `rho = 2r`), one loop, a touch.
//! - [`torus_plane_parallel`]: the spiric sections of a plane parallel to the axis (two tube
//!   circles, two ovals, a figure eight, one oval, a touch); [`torus_villarceau`] the two
//!   Villarceau circles of the bitangent plane through the centre.
//! - [`bicylinder_volume`], [`bicylinder_area`], [`t_union_volume`], [`sphere_lens_volume`]
//!   and [`sphere_union_area`] for the Boolean results, with [`ellip_k`] and [`ellip_e`].
//!
//! Every parameter is exact up to the rounding of `f64` and [`scx_num::fmath`]; the tests
//! check each against independent quadrature of the defining integrals and against the
//! surfaces' implicit equations.

#![allow(clippy::indexing_slicing)]

use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::fmt;
use std::sync::Arc;

use scx_num::fmath;

/// A point or vector.
pub type P3 = [f64; 3];

fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: P3, s: f64) -> P3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: P3) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: P3) -> P3 {
    scale(a, 1.0 / norm(a))
}

/// The distance between two points.
#[must_use]
pub fn dist(a: P3, b: P3) -> f64 {
    norm(sub(a, b))
}

/// Two unit vectors completing `u` (a unit vector) to a right-handed frame `(e1, e2, u)`.
fn perp_basis(u: P3) -> (P3, P3) {
    let k = if u[0].abs() <= u[1].abs() && u[0].abs() <= u[2].abs() {
        [1.0, 0.0, 0.0]
    } else if u[1].abs() <= u[2].abs() {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = unit(cross(k, u));
    (e1, cross(u, e1))
}

/// A surface by its signed distance and normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Surf {
    /// `n . (p - point) = 0`, `normal` a unit vector.
    Plane {
        /// A point on the plane.
        point: P3,
        /// The unit normal.
        normal: P3,
    },
    /// `|p - centre| = radius`.
    Sphere {
        /// The centre.
        centre: P3,
        /// The radius.
        radius: f64,
    },
    /// The points at `radius` from the line through `point` along the unit `axis`.
    Cylinder {
        /// A point on the axis.
        point: P3,
        /// The unit axis.
        axis: P3,
        /// The radius.
        radius: f64,
    },
    /// The points at `minor` from the circle of radius `major` about `axis` through `centre`.
    Torus {
        /// The centre.
        centre: P3,
        /// The unit axis.
        axis: P3,
        /// The radius of the tube's centre circle.
        major: f64,
        /// The tube's radius.
        minor: f64,
    },
}

impl Surf {
    /// The signed distance to the surface, positive outside (the side the normal points to),
    /// as the true Euclidean distance: zero exactly on the surface.
    #[must_use]
    pub fn distance(&self, p: P3) -> f64 {
        match *self {
            Surf::Plane { point, normal } => dot(sub(p, point), normal),
            Surf::Sphere { centre, radius } => norm(sub(p, centre)) - radius,
            Surf::Cylinder {
                point,
                axis,
                radius,
            } => {
                let v = sub(p, point);
                norm(sub(v, scale(axis, dot(v, axis)))) - radius
            }
            Surf::Torus {
                centre,
                axis,
                major,
                minor,
            } => {
                let v = sub(p, centre);
                let h = dot(v, axis);
                let rho = norm(sub(v, scale(axis, h)));
                let q = rho - major;
                (q * q + h * h).sqrt() - minor
            }
        }
    }

    /// The unit normal at a point on the surface (the gradient of [`Surf::distance`]); `None`
    /// on a cylinder's axis, a sphere's centre or a torus's tube circle.
    #[must_use]
    pub fn normal(&self, p: P3) -> Option<P3> {
        let radial = |v: P3| {
            let n = norm(v);
            (n > 0.0).then(|| scale(v, 1.0 / n))
        };
        match *self {
            Surf::Plane { normal, .. } => Some(normal),
            Surf::Sphere { centre, .. } => radial(sub(p, centre)),
            Surf::Cylinder { point, axis, .. } => {
                let v = sub(p, point);
                radial(sub(v, scale(axis, dot(v, axis))))
            }
            Surf::Torus {
                centre,
                axis,
                major,
                ..
            } => {
                let v = sub(p, centre);
                let h = dot(v, axis);
                let out = sub(v, scale(axis, h));
                let rho = norm(out);
                if rho == 0.0 {
                    return None;
                }
                let tube_centre = scale(out, major / rho);
                radial(sub(v, tube_centre))
            }
        }
    }

    fn placed(&self, m: &Rigid) -> Surf {
        match *self {
            Surf::Plane { point, normal } => Surf::Plane {
                point: m.point(point),
                normal: m.vector(normal),
            },
            Surf::Sphere { centre, radius } => Surf::Sphere {
                centre: m.point(centre),
                radius,
            },
            Surf::Cylinder {
                point,
                axis,
                radius,
            } => Surf::Cylinder {
                point: m.point(point),
                axis: m.vector(axis),
                radius,
            },
            Surf::Torus {
                centre,
                axis,
                major,
                minor,
            } => Surf::Torus {
                centre: m.point(centre),
                axis: m.vector(axis),
                major,
                minor,
            },
        }
    }
}

/// A rigid motion `p -> rot p + shift`, `rot` a rotation given by its rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rigid {
    /// The rows of the rotation matrix.
    pub rot: [P3; 3],
    /// The translation, applied after the rotation.
    pub shift: P3,
}

impl Rigid {
    /// A translation.
    #[must_use]
    pub fn translation(shift: P3) -> Rigid {
        Rigid {
            rot: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            shift,
        }
    }

    /// A turn of `angle` about the line through the origin along `axis` (any length), then a translation (Rodrigues' formula).
    #[must_use]
    pub fn turn(axis: P3, angle: f64, shift: P3) -> Rigid {
        let k = unit(axis);
        let (s, c) = fmath::sin_cos(angle);
        let t = 1.0 - c;
        Rigid {
            rot: [
                [
                    c + k[0] * k[0] * t,
                    k[0] * k[1] * t - k[2] * s,
                    k[0] * k[2] * t + k[1] * s,
                ],
                [
                    k[1] * k[0] * t + k[2] * s,
                    c + k[1] * k[1] * t,
                    k[1] * k[2] * t - k[0] * s,
                ],
                [
                    k[2] * k[0] * t - k[1] * s,
                    k[2] * k[1] * t + k[0] * s,
                    c + k[2] * k[2] * t,
                ],
            ],
            shift,
        }
    }

    /// The image of a vector.
    #[must_use]
    pub fn vector(&self, v: P3) -> P3 {
        [
            dot(self.rot[0], v),
            dot(self.rot[1], v),
            dot(self.rot[2], v),
        ]
    }

    /// The image of a point.
    #[must_use]
    pub fn point(&self, p: P3) -> P3 {
        add(self.vector(p), self.shift)
    }
}

/// Whether a curve is a loop or an infinite line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extent {
    /// Closed: the point at the domain's end is the point at its start, and the parameter is
    /// periodic with the domain's width.
    Loop,
    /// Unbounded in both directions; the domain is a window to sample.
    Line,
}

type Eval = Arc<dyn Fn(f64) -> P3 + Send + Sync>;

/// One curve of the intersection, a pure function of one parameter.
#[derive(Clone)]
pub struct TruthCurve {
    /// A loop or a line.
    pub extent: Extent,
    /// The parameter interval: one period of a loop, a window on a line.
    pub domain: (f64, f64),
    /// The length where it is known in closed form, or by a quadrature whose integrand is
    /// the exact speed and which converges geometrically (the tests check it against
    /// independent polyline quadrature); `None` for a line or where no such form was derived.
    pub length: Option<f64>,
    eval: Eval,
}

impl fmt::Debug for TruthCurve {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TruthCurve")
            .field("extent", &self.extent)
            .field("domain", &self.domain)
            .field("length", &self.length)
            .finish_non_exhaustive()
    }
}

impl TruthCurve {
    fn new(
        extent: Extent,
        domain: (f64, f64),
        length: Option<f64>,
        eval: impl Fn(f64) -> P3 + Send + Sync + 'static,
    ) -> TruthCurve {
        TruthCurve {
            extent,
            domain,
            length,
            eval: Arc::new(eval),
        }
    }

    /// The point at parameter `t` (any real: a loop is periodic).
    #[must_use]
    pub fn point(&self, t: f64) -> P3 {
        (self.eval)(t)
    }

    /// `n` points at equal parameter steps from the domain's start (a loop's end is not
    /// repeated; a line's window includes both ends when `n >= 2`).
    #[must_use]
    pub fn samples(&self, n: usize) -> Vec<P3> {
        let (a, b) = self.domain;
        let span = b - a;
        let steps = match self.extent {
            Extent::Loop => n,
            Extent::Line => n.saturating_sub(1).max(1),
        };
        (0..n)
            .map(|i| self.point(a + span * (i as f64) / (steps as f64)))
            .collect()
    }

    /// The nearest point of the curve to `p`: its parameter and distance. Sampled at 512
    /// parameters; the six lowest local minima of the sampled distance (a crossing makes two
    /// close ones) are each narrowed by 30 golden-section steps on the distance itself (not
    /// its square: a point on the curve gives a corner, found to rounding), and the best one
    /// is refined to the parameter's last digits. About 800 evaluations.
    #[must_use]
    pub fn nearest(&self, p: P3) -> (f64, f64) {
        const N: usize = 512;
        let (a, b) = self.domain;
        let h = (b - a) / (N as f64);
        let ds: Vec<f64> = (0..N)
            .map(|i| dist(self.point(a + h * (i as f64)), p))
            .collect();
        let at = |i: isize| -> f64 {
            let n = N as isize;
            match self.extent {
                Extent::Loop => ds[i.rem_euclid(n) as usize],
                Extent::Line => ds[i.clamp(0, n - 1) as usize],
            }
        };
        let mut minima: Vec<usize> = (0..N)
            .filter(|&i| {
                let i = i as isize;
                at(i) <= at(i - 1) && at(i) <= at(i + 1)
            })
            .collect();
        minima.sort_by(|&x, &y| ds[x].total_cmp(&ds[y]));
        minima.truncate(6);
        let f = |t: f64| dist(self.point(t), p);
        // Golden-section on `[lo, hi]` for `steps` steps: the narrowed bracket and its best.
        let golden = |mut lo: f64, mut hi: f64, steps: usize| -> (f64, f64, f64, f64) {
            let g = 0.5 * (5.0_f64.sqrt() - 1.0);
            let (mut x1, mut x2) = (hi - g * (hi - lo), lo + g * (hi - lo));
            let (mut f1, mut f2) = (f(x1), f(x2));
            for _ in 0..steps {
                if f1 < f2 {
                    hi = x2;
                    x2 = x1;
                    f2 = f1;
                    x1 = hi - g * (hi - lo);
                    f1 = f(x1);
                } else {
                    lo = x1;
                    x1 = x2;
                    f1 = f2;
                    x2 = lo + g * (hi - lo);
                    f2 = f(x2);
                }
            }
            (lo, hi, if f1 < f2 { x1 } else { x2 }, f1.min(f2))
        };
        let mut best: Option<(f64, f64, f64, f64)> = None;
        for i in minima {
            let c = a + h * (i as f64);
            let r = golden(c - h, c + h, 30);
            if best.is_none_or(|b| r.3 < b.3) {
                best = Some(r);
            }
        }
        match best {
            Some((lo, hi, _, _)) => {
                let (_, _, t, d) = golden(lo, hi, 60);
                (t, d)
            }
            None => (a, f64::INFINITY),
        }
    }

    fn placed(&self, m: &Rigid) -> TruthCurve {
        let (inner, m) = (Arc::clone(&self.eval), *m);
        TruthCurve {
            extent: self.extent,
            domain: self.domain,
            length: self.length,
            eval: Arc::new(move |t| m.point(inner(t))),
        }
    }
}

/// A point where the curves meet themselves or each other, with every parameter reaching it.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// The point.
    pub point: P3,
    /// `(curve index, parameter)` for each visit, parameters inside the curve's domain.
    pub at: Vec<(usize, f64)>,
}

/// The two Hausdorff one-sided distances between a candidate and the truth's curves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hausdorff {
    /// The farthest a candidate vertex lies from the truth's curves.
    pub to_truth: f64,
    /// The farthest a truth sample lies from the candidate's polylines (so a coarse
    /// candidate shows its chord sag here).
    pub from_truth: f64,
}

/// The exact intersection of two surfaces.
#[derive(Clone, Debug)]
pub struct Truth {
    /// A name for failure messages.
    pub name: String,
    /// The first surface.
    pub a: Surf,
    /// The second surface.
    pub b: Surf,
    /// The curves: loops (a figure eight is one loop with a node visited twice) and lines.
    pub curves: Vec<TruthCurve>,
    /// The nodes. At every node the two surfaces are tangent (their normals are parallel).
    pub nodes: Vec<Node>,
    /// Isolated points of contact.
    pub touches: Vec<P3>,
    /// The surfaces touch along their curves without crossing there (a tangent line or circle).
    pub tangent: bool,
    /// The surfaces are the same surface.
    pub coincident: bool,
}

impl Truth {
    fn new(name: &str, a: Surf, b: Surf) -> Truth {
        Truth {
            name: name.to_string(),
            a,
            b,
            curves: Vec::new(),
            nodes: Vec::new(),
            touches: Vec::new(),
            tangent: false,
            coincident: false,
        }
    }

    /// No contact at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.coincident && self.curves.is_empty() && self.touches.is_empty()
    }

    /// The number of closed curves.
    #[must_use]
    pub fn loops(&self) -> usize {
        self.curves
            .iter()
            .filter(|c| c.extent == Extent::Loop)
            .count()
    }

    /// The number of lines.
    #[must_use]
    pub fn lines(&self) -> usize {
        self.curves.len() - self.loops()
    }

    /// The larger of the two surfaces' distances at `p`: the residual of a claimed point.
    #[must_use]
    pub fn residual(&self, p: P3) -> f64 {
        self.a.distance(p).abs().max(self.b.distance(p).abs())
    }

    /// The nearest curve to `p`: `(curve index, parameter, distance)`; `None` with no curves.
    #[must_use]
    pub fn nearest(&self, p: P3) -> Option<(usize, f64, f64)> {
        let mut best: Option<(usize, f64, f64)> = None;
        for (i, c) in self.curves.iter().enumerate() {
            let (t, d) = c.nearest(p);
            if best.is_none_or(|b| d < b.2) {
                best = Some((i, t, d));
            }
        }
        best
    }

    /// The farthest a vertex of the candidate polylines lies from the curves.
    #[must_use]
    pub fn to_truth(&self, candidate: &[Vec<P3>]) -> f64 {
        candidate
            .iter()
            .flatten()
            .filter_map(|&p| self.nearest(p).map(|b| b.2))
            .fold(0.0, f64::max)
    }

    /// The farthest one of `n` samples per curve lies from the candidate polylines' chords
    /// (so a coarse candidate shows its chord sag here).
    #[must_use]
    pub fn from_truth(&self, candidate: &[Vec<P3>], n: usize) -> f64 {
        self.curves
            .iter()
            .flat_map(|c| c.samples(n))
            .map(|p| {
                candidate
                    .iter()
                    .flat_map(|pl| pl.windows(2).map(|w| point_segment(p, w[0], w[1])))
                    .fold(f64::INFINITY, f64::min)
            })
            .fold(0.0, f64::max)
    }

    /// The Hausdorff distances between the candidate polylines and the curves, the truth
    /// sampled at `n` parameters per curve. A loop in the candidate is a polyline whose last
    /// point repeats its first.
    #[must_use]
    pub fn hausdorff(&self, candidate: &[Vec<P3>], n: usize) -> Hausdorff {
        Hausdorff {
            to_truth: self.to_truth(candidate),
            from_truth: self.from_truth(candidate, n),
        }
    }

    /// The whole answer moved by a rigid motion: surfaces, curves, nodes and touches.
    #[must_use]
    pub fn placed(&self, m: &Rigid) -> Truth {
        Truth {
            name: self.name.clone(),
            a: self.a.placed(m),
            b: self.b.placed(m),
            curves: self.curves.iter().map(|c| c.placed(m)).collect(),
            nodes: self
                .nodes
                .iter()
                .map(|n| Node {
                    point: m.point(n.point),
                    at: n.at.clone(),
                })
                .collect(),
            touches: self.touches.iter().map(|&p| m.point(p)).collect(),
            tangent: self.tangent,
            coincident: self.coincident,
        }
    }
}

fn point_segment(p: P3, a: P3, b: P3) -> f64 {
    let ab = sub(b, a);
    let len2 = dot(ab, ab);
    let t = if len2 == 0.0 {
        0.0
    } else {
        (dot(sub(p, a), ab) / len2).clamp(0.0, 1.0)
    };
    dist(p, add(a, scale(ab, t)))
}

/// The trapezoid rule on `n` equal steps of a periodic function over one period (geometric
/// convergence for an analytic integrand).
fn periodic_trapezoid(f: impl Fn(f64) -> f64, a: f64, b: f64, n: u32) -> f64 {
    let h = (b - a) / f64::from(n);
    (0..n).map(|j| f(a + h * f64::from(j))).sum::<f64>() * h
}

// --- elliptic integrals -------------------------------------------------------------------

/// The complete elliptic integral of the first kind `K(m)`, parameter `m = k^2` in `[0, 1)`
/// (arithmetic-geometric mean); infinite at `1`, `NaN` outside `[0, 1]`.
#[must_use]
pub fn ellip_k(m: f64) -> f64 {
    if !(0.0..=1.0).contains(&m) {
        return f64::NAN;
    }
    if m == 1.0 {
        return f64::INFINITY;
    }
    let (mut a, mut b) = (1.0, (1.0 - m).sqrt());
    for _ in 0..64 {
        if (a - b).abs() <= 4e-16 * a {
            break;
        }
        (a, b) = (0.5 * (a + b), (a * b).sqrt());
    }
    PI / (2.0 * a)
}

/// `(K, S)` for `0 <= m < 1`, where `E = K (1 - S)` and `S = sum 2^(n-1) c_n^2` is the AGM
/// sum; `K - E = K S` and `E - (1 - m) K = K (m - S)` need no cancellation.
fn ellip_k_s(m: f64) -> (f64, f64) {
    let (mut a, mut b) = (1.0, (1.0 - m).sqrt());
    let mut sum = 0.5 * m;
    let mut pow = 0.5;
    for _ in 0..64 {
        if (a - b).abs() <= 4e-16 * a {
            break;
        }
        let c = 0.5 * (a - b);
        (a, b) = (0.5 * (a + b), (a * b).sqrt());
        pow *= 2.0;
        sum += pow * c * c;
    }
    (PI / (2.0 * a), sum)
}

/// The complete elliptic integral of the second kind `E(m)`, parameter `m = k^2` in `[0, 1]`
/// (the AGM sum `E = K (1 - sum 2^(n-1) c_n^2)`); `NaN` outside `[0, 1]`.
#[must_use]
pub fn ellip_e(m: f64) -> f64 {
    if !(0.0..=1.0).contains(&m) {
        return f64::NAN;
    }
    if m == 1.0 {
        return 1.0;
    }
    let (k, s) = ellip_k_s(m);
    k * (1.0 - s)
}

// --- Boolean volumes and areas ------------------------------------------------------------

/// The volume of the intersection of two cylinders of radii `r <= big` whose axes meet at
/// right angles: `(8/3) R [(R^2 + r^2) E(k) - (R^2 - r^2) K(k)]`, `k = r / R`, and
/// `16 R^3 / 3` at `r = R` (Steinmetz). `None` for `r > big` or a non-positive radius.
#[must_use]
pub fn bicylinder_volume(r: f64, big: f64) -> Option<f64> {
    if !(r > 0.0 && r <= big) {
        return None;
    }
    if r == big {
        return Some(16.0 * big * big * big / 3.0);
    }
    // (R^2 + r^2) E - (R^2 - r^2) K = r^2 K (2 - S - S / m): no cancellation for small m.
    let m = (r / big) * (r / big);
    let (k, s) = ellip_k_s(m);
    Some(8.0 / 3.0 * big * r * r * k * (2.0 - s - s / m))
}

/// The surface area of that intersection: `8 r R E(k) + 8 R^2 (E(k) - (1 - k^2) K(k))`
/// (the `r`-cylinder's part, then the `R`-cylinder's), `16 R^2` at `r = R`.
#[must_use]
pub fn bicylinder_area(r: f64, big: f64) -> Option<f64> {
    if !(r > 0.0 && r <= big) {
        return None;
    }
    if r == big {
        return Some(16.0 * big * big);
    }
    let m = (r / big) * (r / big);
    let (k, s) = ellip_k_s(m);
    Some(8.0 * r * big * k * (1.0 - s) + 8.0 * big * big * k * (m - s))
}

/// The volume of a pipe of radius `pipe_r` and length `pipe_len` (axis through the middle of
/// both ends) with a branch of radius `branch_r <= pipe_r` and length `branch_len` from the
/// pipe's axis out at right angles, united: `pi R^2 L + pi r^2 l - V / 2` with `V` the
/// bicylinder volume (the branch is inside the pipe over half the bicylinder). It needs
/// `pipe_len >= 2 branch_r` and `branch_len >= pipe_r`; otherwise `None`. The pin's `tpipe`
/// is `(2, 20, 1.5, 10)`.
#[must_use]
pub fn t_union_volume(pipe_r: f64, pipe_len: f64, branch_r: f64, branch_len: f64) -> Option<f64> {
    if pipe_len < 2.0 * branch_r || branch_len < pipe_r {
        return None;
    }
    let v = bicylinder_volume(branch_r, pipe_r)?;
    Some(PI * pipe_r * pipe_r * pipe_len + PI * branch_r * branch_r * branch_len - 0.5 * v)
}

/// The volume common to two spheres of radii `r1`, `r2` with centres `d` apart.
#[must_use]
pub fn sphere_lens_volume(r1: f64, r2: f64, d: f64) -> f64 {
    if d >= r1 + r2 {
        0.0
    } else if d <= (r1 - r2).abs() {
        4.0 / 3.0 * PI * fmath::powi(r1.min(r2), 3)
    } else {
        let s = r1 + r2 - d;
        PI * s * s * (d * d + 2.0 * d * (r1 + r2) - 3.0 * (r1 - r2) * (r1 - r2)) / (12.0 * d)
    }
}

/// The surface area of the union of two spheres: each sphere less the cap inside the other
/// (`2 pi r h`, `h = r - a` for the signed distance `a` from the centre to the radical plane,
/// toward the other centre); a contained sphere leaves the larger alone.
#[must_use]
pub fn sphere_union_area(r1: f64, r2: f64, d: f64) -> f64 {
    let sph = |r: f64| 4.0 * PI * r * r;
    if d >= r1 + r2 {
        sph(r1) + sph(r2)
    } else if d <= (r1 - r2).abs() {
        sph(r1.max(r2))
    } else {
        let a = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
        let (h1, h2) = (r1 - a, r2 - (d - a));
        sph(r1) + sph(r2) - 2.0 * PI * (r1 * h1 + r2 * h2)
    }
}

// --- the truths ---------------------------------------------------------------------------

/// Two spheres. Centres `c1`, `c2`, radii `r1`, `r2`; `d = |c2 - c1|`, `a = (d^2 + r1^2 -
/// r2^2) / (2 d)` from `c1` along the axis and the circle's radius `sqrt((r1 - a)(r1 + a))`
/// when `|r1 - r2| < d < r1 + r2`. Equality (compared on `d^2`, exactly) is a touch, `d = 0`
/// with equal radii is a coincidence, anything else is empty. The circle's parameter `t` is
/// the angle from a fixed unit vector perpendicular to the axis (`e1` of the frame
/// completed from the axis, as [`sphere_sphere_frame`] gives it), `t` in `[0, 2 pi)`.
#[must_use]
pub fn sphere_sphere(c1: P3, r1: f64, c2: P3, r2: f64) -> Option<Truth> {
    if !(r1 > 0.0 && r2 > 0.0) {
        return None;
    }
    let mut t = Truth::new(
        "sphere_sphere",
        Surf::Sphere {
            centre: c1,
            radius: r1,
        },
        Surf::Sphere {
            centre: c2,
            radius: r2,
        },
    );
    let v = sub(c2, c1);
    let d2 = dot(v, v);
    if d2 == 0.0 {
        if r1 == r2 {
            t.coincident = true;
        }
        return Some(t);
    }
    let (sum, diff) = ((r1 + r2) * (r1 + r2), (r1 - r2) * (r1 - r2));
    if d2 > sum || d2 < diff {
        return Some(t);
    }
    let d = d2.sqrt();
    let u = scale(v, 1.0 / d);
    let a = (d2 + r1 * r1 - r2 * r2) / (2.0 * d);
    let centre = add(c1, scale(u, a));
    if d2 == sum || d2 == diff {
        t.touches.push(centre);
        return Some(t);
    }
    let rho = ((r1 - a) * (r1 + a)).sqrt();
    let (e1, e2) = perp_basis(u);
    t.curves.push(TruthCurve::new(
        Extent::Loop,
        (0.0, TAU),
        Some(TAU * rho),
        move |s| {
            let (sn, cs) = fmath::sin_cos(s);
            add(centre, add(scale(e1, rho * cs), scale(e2, rho * sn)))
        },
    ));
    Some(t)
}

/// The circle of [`sphere_sphere`] as `(centre, radius, e1, e2)`: the point at `t` is
/// `centre + radius (cos t e1 + sin t e2)`; `None` when there is no circle.
#[must_use]
pub fn sphere_sphere_frame(c1: P3, r1: f64, c2: P3, r2: f64) -> Option<(P3, f64, P3, P3)> {
    let v = sub(c2, c1);
    let d2 = dot(v, v);
    let (sum, diff) = ((r1 + r2) * (r1 + r2), (r1 - r2) * (r1 - r2));
    if !(r1 > 0.0 && r2 > 0.0) || d2 >= sum || d2 <= diff {
        return None;
    }
    let d = d2.sqrt();
    let u = scale(v, 1.0 / d);
    let a = (d2 + r1 * r1 - r2 * r2) / (2.0 * d);
    let (e1, e2) = perp_basis(u);
    Some((add(c1, scale(u, a)), ((r1 - a) * (r1 + a)).sqrt(), e1, e2))
}

/// A sphere of radius `big` at the origin and the plane `z = big - delta`, `delta` the depth
/// of the cap cut off: the circle of radius `sqrt(delta (2 big - delta))` at height `big -
/// delta` (`t` the angle from `x`), a touch at the pole for `delta = 0` or `2 big`, else
/// empty. The corpus's `c7_tiny_loop_*` is `delta = 1e-7`.
#[must_use]
pub fn tiny_loop(big: f64, delta: f64) -> Option<Truth> {
    if !(big > 0.0 && delta.is_finite()) {
        return None;
    }
    let h = big - delta;
    let mut t = Truth::new(
        "tiny_loop",
        Surf::Sphere {
            centre: [0.0; 3],
            radius: big,
        },
        Surf::Plane {
            point: [0.0, 0.0, h],
            normal: [0.0, 0.0, 1.0],
        },
    );
    if delta < 0.0 || delta > 2.0 * big {
        return Some(t);
    }
    if delta == 0.0 || delta == 2.0 * big {
        t.touches.push([0.0, 0.0, h]);
        return Some(t);
    }
    let rho = (delta * (2.0 * big - delta)).sqrt();
    t.curves.push(TruthCurve::new(
        Extent::Loop,
        (0.0, TAU),
        Some(TAU * rho),
        move |s| {
            let (sn, cs) = fmath::sin_cos(s);
            [rho * cs, rho * sn, h]
        },
    ));
    Some(t)
}

/// `sinc`-like factor of the oval parametrisation: `sin(phi0 w) / w`, `phi0` at `w = 0`.
fn sinc0(phi0: f64, w: f64) -> f64 {
    if w == 0.0 {
        phi0
    } else {
        fmath::sin(phi0 * w) / w
    }
}

/// An oval's parametrisation by `th` in `[0, 2 pi]` for `phi = phi0 cos th` sweeping
/// `[-phi0, phi0]` and back: `(phi, f)` with `cos phi - cos phi0 = f sin^2(th) / 2`, `f > 0`
/// and analytic, so that `sqrt(K (cos phi - cos phi0)) = sin(th) sqrt(K f / 2)` (signed) is
/// analytic in `th` through the oval's ends, where the plain square root is not.
fn oval(phi0: f64, th: f64) -> (f64, f64) {
    let (s, c) = fmath::sin_cos(0.5 * th);
    (
        phi0 * fmath::cos(th),
        sinc0(phi0, c * c) * sinc0(phi0, s * s),
    )
}

/// Two perpendicular cylinders: `x^2 + y^2 = r^2` (axis `z`) and `(y - e)^2 + z^2 = big^2`
/// (axis `x`, offset `e` along `y`), `0 < r <= big`, `e >= 0`; `t` is the angle on the first
/// cylinder, `x = r cos t`, `y = r sin t`, `z^2 = big^2 - (r sin t - e)^2`.
///
/// - `e < big - r`: two loops `z = +/- sqrt(...)`, `t` in `[0, 2 pi)`; for `e = 0` with
///   `r < big` each has its length, by quadrature of the exact speed.
/// - `r = big`, `e = 0` (Steinmetz): two ellipses `z = +/- x`, length `4 sqrt(2) big E(1/2)`
///   each, crossing at `(0, +/- big, 0)` (`t = pi/2` and `3 pi/2` on both).
/// - `e = big - r` (`r < big`): one figure eight, `t` in `[0, 4 pi)`, node at `(0, -r, 0)`
///   for `t = 3 pi/2` and `7 pi/2`.
/// - `big - r < e < big + r`: one loop (`t = pi/2 + phi`, `phi = phi0 cos th`,
///   `cos phi0 = (e - big)/r`; the parameter is `th` in `[0, 2 pi)`, `z` positive on the
///   first half).
/// - `e = big + r`: a touch at `(0, r, 0)`; beyond, empty.
#[must_use]
pub fn cylinders_perpendicular(r: f64, big: f64, e: f64) -> Option<Truth> {
    if !(r > 0.0 && r <= big && e >= 0.0 && e.is_finite()) {
        return None;
    }
    let mut t = Truth::new(
        "cylinders_perpendicular",
        Surf::Cylinder {
            point: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            radius: r,
        },
        Surf::Cylinder {
            point: [0.0, e, 0.0],
            axis: [1.0, 0.0, 0.0],
            radius: big,
        },
    );
    if e > big + r {
        return Some(t);
    }
    if e == big + r {
        t.touches.push([0.0, r, 0.0]);
        return Some(t);
    }
    if r == big && e == 0.0 {
        let len = 4.0 * 2.0_f64.sqrt() * big * ellip_e(0.5);
        for s in [1.0, -1.0] {
            t.curves.push(TruthCurve::new(
                Extent::Loop,
                (0.0, TAU),
                Some(len),
                move |u| {
                    let (sn, cs) = fmath::sin_cos(u);
                    [big * cs, big * sn, s * big * cs]
                },
            ));
        }
        t.nodes = vec![
            Node {
                point: [0.0, big, 0.0],
                at: vec![(0, FRAC_PI_2), (1, FRAC_PI_2)],
            },
            Node {
                point: [0.0, -big, 0.0],
                at: vec![(0, 1.5 * PI), (1, 1.5 * PI)],
            },
        ];
        return Some(t);
    }
    if e < big - r {
        let length = (e == 0.0).then(|| {
            // x' = -r sin t, y' = r cos t, z z' = -r^2 sin t cos t.
            periodic_trapezoid(
                |u| {
                    let (s, c) = fmath::sin_cos(u);
                    (r * r + fmath::powi(r * r * s * c, 2) / (big * big - r * r * s * s)).sqrt()
                },
                0.0,
                TAU,
                4096,
            )
        });
        for s in [1.0, -1.0] {
            t.curves.push(TruthCurve::new(
                Extent::Loop,
                (0.0, TAU),
                length,
                move |u| {
                    let (sn, cs) = fmath::sin_cos(u);
                    let w = r * sn - e;
                    [r * cs, r * sn, s * (big * big - w * w).sqrt()]
                },
            ));
        }
    } else if e == big - r {
        t.curves.push(TruthCurve::new(
            Extent::Loop,
            (0.0, 2.0 * TAU),
            None,
            move |u| {
                let (sn, cs) = fmath::sin_cos(u);
                let z =
                    fmath::cos(0.5 * u - 0.25 * PI) * (2.0 * r * (2.0 * big - r - r * sn)).sqrt();
                [r * cs, r * sn, z]
            },
        ));
        t.nodes.push(Node {
            point: [0.0, -r, 0.0],
            at: vec![(0, 1.5 * PI), (0, 3.5 * PI)],
        });
    } else {
        let phi0 = fmath::acos((e - big) / r);
        t.curves
            .push(TruthCurve::new(Extent::Loop, (0.0, TAU), None, move |th| {
                let (phi, f) = oval(phi0, th);
                let (sp, cp) = fmath::sin_cos(phi);
                let k = r * (big + e - r * cp);
                [-r * sp, r * cp, fmath::sin(th) * (0.5 * k * f).sqrt()]
            }));
    }
    Some(t)
}

/// Two parallel cylinders, axes along `z` through the origin (radius `r1`) and `(e, 0)`
/// (radius `r2`), `e >= 0`: two lines at `x = (e^2 + r1^2 - r2^2) / (2 e)` and
/// `y = +/- sqrt(r1^2 - x^2)`; one tangent line (`tangent`) when `e = r1 + r2` or `|r1 - r2|`
/// (compared on `e^2`); coincident at `e = 0` with equal radii; else empty. The parameter is
/// `z`, with `half` the half-width of the sampling window.
#[must_use]
pub fn cylinders_parallel(r1: f64, r2: f64, e: f64, half: f64) -> Option<Truth> {
    if !(r1 > 0.0 && r2 > 0.0 && e >= 0.0 && half > 0.0) {
        return None;
    }
    let mut t = Truth::new(
        "cylinders_parallel",
        Surf::Cylinder {
            point: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            radius: r1,
        },
        Surf::Cylinder {
            point: [e, 0.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            radius: r2,
        },
    );
    if e == 0.0 {
        t.coincident = r1 == r2;
        return Some(t);
    }
    let (e2, sum2, diff2) = (e * e, (r1 + r2) * (r1 + r2), (r1 - r2) * (r1 - r2));
    if e2 > sum2 || e2 < diff2 {
        return Some(t);
    }
    let x = (e2 + r1 * r1 - r2 * r2) / (2.0 * e);
    let line = |y: f64| TruthCurve::new(Extent::Line, (-half, half), None, move |z| [x, y, z]);
    if e2 == sum2 || e2 == diff2 {
        t.tangent = true;
        t.curves.push(line(0.0));
    } else {
        let y = ((r1 - x) * (r1 + x)).sqrt();
        t.curves.push(line(y));
        t.curves.push(line(-y));
    }
    Some(t)
}

/// A cylinder (axis `z` through `(e, 0)`, radius `r`) and a sphere (centre the origin, radius
/// `rho`), `r > 0`, `e >= 0`: `x = e + r cos t`, `y = r sin t`, `z^2 = rho^2 - r^2 - e^2 - 2 e
/// r cos t`, whose extremes are `rho^2 - (r + e)^2` at `t = 0` and `rho^2 - (r - e)^2` at
/// `t = pi`.
///
/// - `rho > r + e`: two loops `z = +/- sqrt(..)`, `t` in `[0, 2 pi)`.
/// - `rho = r + e` (`e > 0`): one figure eight `z = 2 sqrt(e r) sin(t/2)`, `t` in `[0, 4
///   pi)`, a node at `(e + r, 0, 0)` for `t = 0` and `2 pi`; its length is `8 sqrt(2) r E(1/2)` for `e = r`
///   ([`viviani`]), `None` otherwise.
/// - `|r - e| < rho < r + e`: one loop (`t = pi + phi`, `phi = phi0 cos th`, `cos phi0 = (r^2 +
///   e^2 - rho^2) / (2 e r)`, the parameter `th` in `[0, 2 pi)`, `z` positive on the first
///   half).
/// - `rho = |r - e|`: a touch at `(e - r, 0, 0)`; `rho` smaller: empty.
/// - `e = 0` (sphere on the axis): two circles for `rho > r`, a tangent circle at `rho = r`.
#[must_use]
pub fn sphere_cylinder(rho: f64, r: f64, e: f64) -> Option<Truth> {
    if !(rho > 0.0 && r > 0.0 && e >= 0.0 && e.is_finite()) {
        return None;
    }
    let mut t = Truth::new(
        "sphere_cylinder",
        Surf::Sphere {
            centre: [0.0; 3],
            radius: rho,
        },
        Surf::Cylinder {
            point: [e, 0.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            radius: r,
        },
    );
    if e == 0.0 {
        if rho < r {
            return Some(t);
        }
        t.tangent = rho == r;
        let signs: &[f64] = if rho == r { &[1.0] } else { &[1.0, -1.0] };
        for &s in signs {
            let z = s * (rho * rho - r * r).sqrt();
            t.curves.push(TruthCurve::new(
                Extent::Loop,
                (0.0, TAU),
                Some(TAU * r),
                move |u| {
                    let (sn, cs) = fmath::sin_cos(u);
                    [r * cs, r * sn, z]
                },
            ));
        }
        return Some(t);
    }
    let (far, near) = (r + e, (r - e).abs());
    if rho < near {
        return Some(t);
    }
    if rho == near {
        t.touches.push([e - r, 0.0, 0.0]);
        return Some(t);
    }
    if rho > far {
        for s in [1.0, -1.0] {
            t.curves
                .push(TruthCurve::new(Extent::Loop, (0.0, TAU), None, move |u| {
                    let (sn, cs) = fmath::sin_cos(u);
                    let g = rho * rho - r * r - e * e - 2.0 * e * r * cs;
                    [e + r * cs, r * sn, s * g.sqrt()]
                }));
        }
    } else if rho == far {
        let length = (e == r).then(|| 8.0 * 2.0_f64.sqrt() * r * ellip_e(0.5));
        let amp = 2.0 * (e * r).sqrt();
        t.curves.push(TruthCurve::new(
            Extent::Loop,
            (0.0, 2.0 * TAU),
            length,
            move |u| {
                let (sn, cs) = fmath::sin_cos(u);
                [e + r * cs, r * sn, amp * fmath::sin(0.5 * u)]
            },
        ));
        t.nodes.push(Node {
            point: [e + r, 0.0, 0.0],
            at: vec![(0, 0.0), (0, TAU)],
        });
    } else {
        let phi0 = fmath::acos((r * r + e * e - rho * rho) / (2.0 * e * r));
        t.curves
            .push(TruthCurve::new(Extent::Loop, (0.0, TAU), None, move |th| {
                let (phi, f) = oval(phi0, th);
                let (sp, cp) = fmath::sin_cos(phi);
                [e - r * cp, -r * sp, fmath::sin(th) * (e * r * f).sqrt()]
            }));
    }
    Some(t)
}

/// Viviani's curve of `a`: the sphere of radius `2a` at the origin and the cylinder of radius
/// `a` about the axis through `(a, 0)`, `x = a (1 + cos t)`, `y = a sin t`, `z = 2 a sin(t/2)`
/// for `t` in `[0, 4 pi)`: one figure eight of length `8 sqrt(2) a E(1/2)` with a node at
/// `(2a, 0, 0)` for `t = 0` and `2 pi`.
#[must_use]
pub fn viviani(a: f64) -> Option<Truth> {
    let mut t = sphere_cylinder(2.0 * a, a, a)?;
    t.name = "viviani".to_string();
    Some(t)
}

/// A torus (axis `z`, centre the origin, radii `big > small > 0`) and the plane `y = d`
/// parallel to its axis, in the spiric sections. With `a = |d|`, `rho = big + small cos phi`
/// the distance from the axis and `x = +/- sqrt(rho^2 - a^2)`, `z = small sin phi`:
///
/// - `a = 0`: the two tube circles, `x = +/- (big + small cos phi)` (parameter `phi` in
///   `[0, 2 pi)`, length `2 pi small`);
/// - `0 < a < big - small`: two ovals (`phi` in `[0, 2 pi)`, the first with `x > 0`);
/// - `a = big - small`: a figure eight, one loop `x = cos(phi/2) sqrt(2 small (rho + a))`,
///   `phi` in `[0, 4 pi)`, node at `(0, d, 0)` for `phi = pi` and `3 pi`;
/// - `big - small < a < big + small`: one oval (`phi = phi0 cos th`, `cos phi0 = (a - big) /
///   small`, the parameter `th` in `[0, 2 pi)`, `x` positive on the first half);
/// - `a = big + small`: a touch at `(0, d, 0)`; beyond, empty.
#[must_use]
pub fn torus_plane_parallel(big: f64, small: f64, d: f64) -> Option<Truth> {
    if !(small > 0.0 && small < big && d.is_finite()) {
        return None;
    }
    let mut t = Truth::new(
        "torus_plane_parallel",
        Surf::Torus {
            centre: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            major: big,
            minor: small,
        },
        Surf::Plane {
            point: [0.0, d, 0.0],
            normal: [0.0, 1.0, 0.0],
        },
    );
    let a = d.abs();
    if a > big + small {
        return Some(t);
    }
    if a == big + small {
        t.touches.push([0.0, d, 0.0]);
        return Some(t);
    }
    if a == 0.0 {
        for s in [1.0, -1.0] {
            t.curves.push(TruthCurve::new(
                Extent::Loop,
                (0.0, TAU),
                Some(TAU * small),
                move |u| {
                    let (sn, cs) = fmath::sin_cos(u);
                    [s * (big + small * cs), d, small * sn]
                },
            ));
        }
    } else if a < big - small {
        for s in [1.0, -1.0] {
            t.curves
                .push(TruthCurve::new(Extent::Loop, (0.0, TAU), None, move |u| {
                    let (sn, cs) = fmath::sin_cos(u);
                    let rho = big + small * cs;
                    [s * ((rho - a) * (rho + a)).sqrt(), d, small * sn]
                }));
        }
    } else if a == big - small {
        t.curves.push(TruthCurve::new(
            Extent::Loop,
            (0.0, 2.0 * TAU),
            None,
            move |u| {
                let (sn, cs) = fmath::sin_cos(u);
                let rho = big + small * cs;
                let x = fmath::cos(0.5 * u) * (2.0 * small * (rho + a)).sqrt();
                [x, d, small * sn]
            },
        ));
        t.nodes.push(Node {
            point: [0.0, d, 0.0],
            at: vec![(0, PI), (0, 3.0 * PI)],
        });
    } else {
        let phi0 = fmath::acos((a - big) / small);
        t.curves
            .push(TruthCurve::new(Extent::Loop, (0.0, TAU), None, move |th| {
                let (phi, f) = oval(phi0, th);
                let (sp, cp) = fmath::sin_cos(phi);
                let rho = big + small * cp;
                [
                    fmath::sin(th) * (0.5 * small * f * (rho + a)).sqrt(),
                    d,
                    small * sp,
                ]
            }));
    }
    Some(t)
}

/// A torus (axis `z`, centre the origin, radii `big > small > 0`) and its bitangent plane
/// through the centre, spanned by `x` and `w = (0, cos al, sin al)`, `sin al = small / big`:
/// two Villarceau circles of radius `big`, centred at `(+/- small, 0, 0)`, `p = centre + big
/// (cos t x + sin t w)`, `t` in `[0, 2 pi)`, length `2 pi big` each. They cross at the two
/// tangent points `+/- (0, big cos^2 al, big cos al sin al)`.
#[must_use]
pub fn torus_villarceau(big: f64, small: f64) -> Option<Truth> {
    if !(small > 0.0 && small < big) {
        return None;
    }
    let sin_al = small / big;
    let cos_al = ((big - small) * (big + small)).sqrt() / big;
    let w = [0.0, cos_al, sin_al];
    let mut t = Truth::new(
        "torus_villarceau",
        Surf::Torus {
            centre: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            major: big,
            minor: small,
        },
        Surf::Plane {
            point: [0.0; 3],
            normal: unit(cross([1.0, 0.0, 0.0], w)),
        },
    );
    for s in [1.0, -1.0] {
        t.curves.push(TruthCurve::new(
            Extent::Loop,
            (0.0, TAU),
            Some(TAU * big),
            move |u| {
                let (sn, cs) = fmath::sin_cos(u);
                [s * small + big * cs, big * sn * w[1], big * sn * w[2]]
            },
        ));
    }
    let hi = [0.0, big * cos_al * cos_al, big * cos_al * sin_al];
    let lo = scale(hi, -1.0);
    // Circle 0 has its centre at +small: x = small + big cos t = 0 at cos t = -sin al.
    // Circle 1 has it at -small: cos t = +sin al.
    let th = |c: f64, s: f64| fmath::atan2(s * cos_al, c);
    t.nodes = vec![
        Node {
            point: hi,
            at: vec![(0, th(-sin_al, 1.0)), (1, th(sin_al, 1.0))],
        },
        Node {
            point: lo,
            at: vec![(0, th(-sin_al, -1.0) + TAU), (1, th(sin_al, -1.0) + TAU)],
        },
    ];
    Some(t)
}
