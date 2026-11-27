// Written for solidcorex. Apache-2.0.
//! Quadric recognition: the plane, cylinder, cone, sphere or torus a surface written as
//! another kind or as NURBS stands for, with a proved bound of its deviation (research 74
//! §5.3, SU7). One function, shared by the importer and HLR.
//!
//! - [`recognize`] takes a surface and the uv extent it is used over (a face's box). A cone
//!   whose angle is nearly zero or nearly right is a cylinder or a plane over that extent, in
//!   closed form. A NURBS surface is a quadric when a certificate proves every point of it
//!   over the extent within the accuracy of the analytic surface found; samples only refute.
//!   The other analytic kinds come back as they are.
//! - [`recognize_written`] takes a kind's sizes as a file writes them where the kind
//!   refuses them: a cone of angle zero (a cylinder) and a torus whose major radius is zero
//!   or not above its minor (a sphere, within the major radius).
//!
//! # The NURBS path
//!
//! 1. **Candidates.** Points, normals and principal curvatures at a 5 x 5 grid inside the
//!    extent propose each kind in turn, simplest first (plane, sphere, cylinder, cone,
//!    torus): the plane from the points' least-squares fit, the sphere's centre where the
//!    normal lines meet, the cylinder's axis square to every normal, the cone's axis square
//!    to the normals' spread and its apex where the tangent planes meet, the torus's tube
//!    from the principal curvature every sample shares and its core circle through the
//!    centres of curvature. A candidate further than `64` accuracies (or a millionth of the
//!    surface's size) from a sample is refuted. This is the whole cost of a reject.
//! 2. **Refinement.** Where a point of a 9 x 9 grid over the extent, edges included, is
//!    beyond the accuracy, Levenberg-Marquardt on the closed-form distances there; a point
//!    still beyond it refutes the candidate.
//! 3. **Certificate.** Over each Bezier patch of the surface restricted to the extent, in the
//!    candidate's frame, the implicit form `Q` of the kind in the patch's homogeneous
//!    coordinates is a polynomial in Bernstein form ([`scx_algo::bernstein::Bounded`], with a
//!    running error bound), and `Q / w^m` is a mean of its coefficients' quotients with
//!    positive weights, so no larger than their largest. The kind's distance is bounded by
//!    `|Q / w^m|` over a factor no smaller than its least over the piece (below). A piece
//!    whose bound is above the accuracy is halved; a corner (a surface point) beyond it
//!    refutes. Each piece is one unit of the budget.
//!
//! The bounds, for the distance `d` to the kind and `delta` its radial part:
//! - plane `z = 0`: `|z| = |Z / w|`, directly from the coefficients;
//! - sphere and cylinder of radius `r`: `Q = rho^2 - r^2 = delta (rho + r)` with
//!   `rho + r >= 2 r - delta`;
//! - cone `rho = s = r + k z`: `Q = rho^2 - s^2`, `delta = |rho - s|`, `rho + s >= 2 s_min -
//!   delta` with `s_min > 0` the least `s` over the piece (from `Z / w`), so the foot is on
//!   the cone's own nappe, and `d = delta cos(angle)`;
//! - torus `(R, r)`, `R > r`: `N = (|p|^2 + R^2 - r^2)^2 - 4 R^2 rho^2 = (d_t^2 - r^2)
//!   (d_t'^2 - r^2)` for `d_t` the distance to the core circle and `d_t'` to its opposite
//!   point, so `|N| >= delta (2 r - delta) ((2 R - r - delta)^2 - r^2)`.
//!
//! Each factor holds for `delta` up to a set `delta_max` (an eighth of the smallest size,
//! and no more than twice the accuracy's radial reach, so the factor loses little), so
//! `delta <= B / c` follows where that is under `delta_max` and a corner of the piece is
//! under it: `delta` is continuous over the piece and takes no value between. The torus's
//! second factor also takes `rho` no less than the piece's control points' least reach
//! along their mean direction. A piece with a corner within `1/64` of the accuracy is not
//! proved (`None`): the bound tends to the piece's largest distance as it is halved, but too
//! slowly there for the halvings to end.
//!
//! The deviation is one-sided: the largest distance from a point of the given surface over
//! the extent to the recognised one. It adds `16 eps` of the frame's size for the rounding of
//! the frame's axes.
//!
//! The normal of the result agrees with the given surface's at the middle of the extent.

// `!(x <= bound)` is the test that refutes a NaN too.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use scx_algo::bernstein::{Bernstein2, Bounded};
use scx_num::budget::{Budget, Stop};
use scx_num::extent2d::Extent2d;
use scx_num::extent3d::Extent3d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::matrix::Matrix;
use scx_num::units::{Accuracy, Length};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_nurbs_surface::{NurbsSurface, ParamAxis};

use crate::common::Axes;
use crate::cone::Cone;
use crate::cylinder::Cylinder;
use crate::error::SurfaceError;
use crate::nurbs_invert::{Hb, halved, patches, up};
use crate::plane::Plane;
use crate::sphere::Sphere;
use crate::surface::Surface;
use crate::torus::Torus;

const EPS: f64 = f64::EPSILON;
const ABOVE: [Approach; 2] = [Approach::Above, Approach::Above];
/// The candidates' grid, inside the extent: `GRID x GRID`.
const GRID: usize = 5;
/// The refinement's grid, over the extent with its edges: `DENSE x DENSE`.
const DENSE: usize = 9;
/// How far past the accuracy a first candidate may be before refinement, in accuracies.
const LOOSE: f64 = 64.0;
/// The same, as a fraction of the surface's size: the floor where the accuracy is tiny.
const LOOSE_REL: f64 = 1e-6;
/// Levenberg-Marquardt iterations at most.
const LM_STEPS: usize = 30;
/// A piece with a corner within this fraction of the accuracy is not proved: the bound,
/// which tends to the piece's largest distance as it is halved, would need too many halvings.
const NEAR: f64 = 1.0 / 64.0;
const DEFECT_BERNSTEIN: &str = "a recognition certificate's Bernstein form could not be built";

/// A recognised surface and the bound of its deviation.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Recognized {
    /// The analytic surface.
    pub surface: Surface,
    /// A proved upper bound of the distance from every point of the given surface, over
    /// the extent it was recognised over, to [`Recognized::surface`].
    pub deviation: Length,
}

/// A kind's sizes as a file writes them, including those the kind refuses.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Written {
    /// A cone ([`Cone::new`]'s arguments); a half-angle of zero is a cylinder.
    Cone {
        /// The frame.
        frame: Frame,
        /// The radius at `v = 0`.
        radius: Length,
        /// The half-angle, radians, signed.
        half_angle: f64,
        /// The sense.
        sense: Sense,
    },
    /// A torus ([`Torus::new`]'s arguments); a major radius of zero or not above the
    /// minor is a sphere of the minor radius, within the major.
    Torus {
        /// The frame.
        frame: Frame,
        /// The major radius.
        major: Length,
        /// The minor radius.
        minor: Length,
        /// The sense.
        sense: Sense,
    },
}

/// The kind `written` stands for, within `accuracy`: a cone of half-angle zero is the
/// cylinder of its radius (deviation 0); a torus of major radius 0 is the sphere of its
/// minor radius (deviation 0), and one whose major radius is not above its minor (a spindle
/// or horn torus) is that sphere within the major radius, every point of it being a point
/// of the sphere moved by the major radius. Any other sizes give the kind itself, deviation
/// 0, through its constructor.
///
/// # Errors
///
/// The constructor's: sizes that make no surface, and [`SurfaceError::SelfIntersecting`] for
/// a spindle or horn torus whose major radius is above `accuracy`.
pub fn recognize_written(
    written: &Written,
    accuracy: Accuracy,
) -> Result<Recognized, SurfaceError> {
    let exact = |surface| Recognized {
        surface,
        deviation: Length::ZERO,
    };
    match *written {
        Written::Cone {
            frame,
            radius,
            half_angle,
            sense,
        } => {
            if half_angle == 0.0 {
                return Cylinder::new(frame, radius, sense).map(|c| exact(Surface::Cylinder(c)));
            }
            Cone::new(frame, radius, half_angle, sense).map(|c| exact(Surface::Cone(c)))
        }
        Written::Torus {
            frame,
            major,
            minor,
            sense,
        } => {
            let (big, small) = (major.mm(), minor.mm());
            let degenerate = big == 0.0 || (big > 0.0 && big.is_finite() && small >= big);
            if degenerate && big <= accuracy.length().mm() {
                let sphere = Sphere::new(frame, minor, sense)?;
                return Ok(Recognized {
                    surface: Surface::Sphere(sphere),
                    deviation: major,
                });
            }
            Torus::new(frame, major, minor, sense).map(|t| exact(Surface::Torus(t)))
        }
    }
}

/// The simplest analytic surface within `accuracy` of `surface` over `over`, and the proved
/// bound of its deviation there; `None` where none is proved (a sample beyond the accuracy
/// refutes a candidate, and a candidate the certificate cannot prove is dropped).
///
/// - A cone whose height range over `over` is within twice the accuracy is the plane square
///   to its axis at the middle height; one whose radius range is, the coaxial cylinder of
///   the middle radius; else the cone itself.
/// - A NURBS surface: the module's three steps.
/// - Any other analytic kind: itself, deviation 0.
///
/// Leaf query (research 74 R8): the NURBS certificate charges `budget` and polls `stop`.
///
/// # Errors
///
/// `Refused` for an `over` the surface's domain does not hold ([`Surface::restrict`]'s
/// errors), [`SurfaceError::Exhausted`] when the budget runs out, [`SurfaceError::Stopped`]
/// when `stop` is set, and [`SurfaceError::Defect`] where a Bernstein form cannot be built.
pub fn recognize(
    surface: &Surface,
    over: &Extent2d,
    accuracy: Accuracy,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Option<Recognized>, SurfaceError> {
    stop.check().map_err(|_| SurfaceError::Stopped)?;
    let restricted = surface.restrict(*over)?;
    let acc = accuracy.length().mm();
    match &restricted {
        Surface::Cone(c) => Ok(Some(cone(c, over, acc))),
        Surface::Nurbs(n) => nurbs(&restricted, n, over, acc, budget, stop),
        _ => Ok(Some(Recognized {
            surface: restricted,
            deviation: Length::ZERO,
        })),
    }
}

// ---------------------------------------------------------------- the cone, closed form

/// The cone over `over` as a plane, a cylinder or itself.
fn cone(c: &Cone, over: &Extent2d, acc: f64) -> Recognized {
    let v = over.y_interval();
    let (v0, v1) = (v.min(), v.max());
    let vm = 0.5 * (v0 + v1);
    let reach = v0.abs().max(v1.abs());
    let axes = Axes::new(c.frame(), c.sense());
    let size = axes.o.to_vec().max_dimension();
    let (r, k) = (c.radius().mm(), c.slope());
    // The plane at the middle height: every point's height `v` is within the half-range.
    let plane_dev = up((vm - v0).max(v1 - vm) * (1.0 + 2.0 * EPS) + 8.0 * EPS * (size + reach));
    if plane_dev <= acc {
        // The cone's normal is `(s D - k Z) cos`: against its slope along the axis.
        let z = *c.frame().z() * (-k.signum());
        let origin = axes.o + axes.z * vm;
        if let Some(frame) = Frame::new(origin, z, *c.frame().x()) {
            return Recognized {
                surface: Surface::Plane(Plane::new(frame)),
                deviation: Length::from_mm(plane_dev),
            };
        }
    }
    // The coaxial cylinder of the middle radius: every radius is within the half-range.
    let rc = r + k * vm;
    let cyl_dev = up(
        (k * (v0 - vm)).abs().max((k * (v1 - vm)).abs()) * (1.0 + 4.0 * EPS)
            + 4.0 * EPS * (r + k.abs() * reach),
    );
    if cyl_dev <= acc
        && let Ok(cyl) = Cylinder::new(*c.frame(), Length::from_mm(rc), c.sense())
    {
        return Recognized {
            surface: Surface::Cylinder(cyl),
            deviation: Length::from_mm(cyl_dev),
        };
    }
    Recognized {
        surface: Surface::Cone(*c),
        deviation: Length::ZERO,
    }
}

// ---------------------------------------------------------------- the NURBS path

/// A kind's parameters in the normalised coordinates `(p - centre) / size`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Plane,
    Sphere,
    Cylinder,
    Cone,
    Torus,
}

const KINDS: [Kind; 5] = [
    Kind::Plane,
    Kind::Sphere,
    Kind::Cylinder,
    Kind::Cone,
    Kind::Torus,
];

/// A sample: its point (normalised), unit normal and principal curvatures (normalised).
#[derive(Debug, Clone, Copy)]
struct Sample {
    q: Vec3,
    n: Vec3,
    curv: Option<[f64; 2]>,
}

/// The normalisation: `q = (p - centre) / size`.
#[derive(Debug, Clone, Copy)]
struct Norm {
    centre: Point3,
    size: f64,
}

impl Norm {
    fn to(self, p: Point3) -> Vec3 {
        (p - self.centre) * (1.0 / self.size)
    }

    fn from(self, q: Vec3) -> Point3 {
        self.centre + q * self.size
    }
}

fn nurbs(
    whole: &Surface,
    n: &NurbsSurface<3>,
    over: &Extent2d,
    acc: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Option<Recognized>, SurfaceError> {
    let Some(bbox) = Extent3d::hull(
        n.points()
            .points()
            .iter()
            .map(|a| Point3::new(a[0], a[1], a[2])),
    ) else {
        return Ok(None);
    };
    let (lo, hi) = (bbox.min(), bbox.max());
    let size = (hi - lo).length();
    if !(size > 0.0 && size.is_finite()) {
        return Ok(None);
    }
    let norm = Norm {
        centre: lo.midpoint(hi),
        size,
    };
    let tick = |budget: &mut Budget, work: u64| -> Result<(), SurfaceError> {
        budget.charge(work).map_err(SurfaceError::Exhausted)?;
        stop.check().map_err(|_| SurfaceError::Stopped)
    };
    tick(budget, (GRID * GRID) as u64)?;
    let samples = grid_samples(whole, over, norm);
    if samples.len() < GRID * GRID / 2 {
        return Ok(None);
    }
    let acc_n = acc / size;
    let loose = (LOOSE * acc_n).max(LOOSE_REL);
    let mut dense: Option<Vec<Vec3>> = None;
    for kind in KINDS {
        let Some(mut theta) = candidate(kind, &samples) else {
            continue;
        };
        if max_residual(kind, &theta, samples.iter().map(|s| s.q)) > loose {
            continue;
        }
        let pts = if let Some(p) = &dense {
            p
        } else {
            tick(budget, (DENSE * DENSE) as u64)?;
            dense.insert(dense_points(whole, over, norm))
        };
        // A candidate already within the accuracy at every point goes to the certificate
        // as it is; refined where it is not, or where the certificate does not prove it.
        let mut refined = false;
        loop {
            if max_residual(kind, &theta, pts.iter().copied()) <= acc_n
                && let Some(built) = build(kind, &theta, norm)
                && let Some(deviation) = certify(n, &built, acc, budget, stop)?
            {
                let mid = Point2::new(over.x_interval().mid(), over.y_interval().mid());
                let surface = oriented(built.surface, &built.local, whole, mid);
                return Ok(Some(Recognized {
                    surface,
                    deviation: Length::from_mm(deviation),
                }));
            }
            if refined {
                break;
            }
            tick(budget, LM_STEPS as u64)?;
            refine(kind, &mut theta, pts);
            refined = true;
        }
    }
    Ok(None)
}

/// The grid's parameters: `count` values over `[lo, hi]`, inside (`inside`) or with the ends.
fn ticks(lo: f64, hi: f64, count: usize, inside: bool) -> impl Iterator<Item = f64> {
    (0..count).map(move |i| {
        let t = if inside {
            (i as f64 + 0.5) / count as f64
        } else {
            i as f64 / (count - 1) as f64
        };
        lo + (hi - lo) * t
    })
}

fn grid_samples(s: &Surface, over: &Extent2d, norm: Norm) -> Vec<Sample> {
    let (u, v) = (over.x_interval(), over.y_interval());
    let mut out = Vec::with_capacity(GRID * GRID);
    for x in ticks(u.min(), u.max(), GRID, true) {
        for y in ticks(v.min(), v.max(), GRID, true) {
            let j = s.jet::<2>(Point2::new(x, y), ABOVE);
            let (Some(p), Some(su), Some(sv), Some(suu), Some(suv), Some(svv)) = (
                j.get(0, 0),
                j.get(1, 0),
                j.get(0, 1),
                j.get(2, 0),
                j.get(1, 1),
                j.get(0, 2),
            ) else {
                continue;
            };
            let Some(n) = su.cross(sv).normalized() else {
                continue;
            };
            let n = n.as_vec();
            let (e, f, g) = (su.dot(su), su.dot(sv), sv.dot(sv));
            let (l, m, nn) = (suu.dot(n), suv.dot(n), svv.dot(n));
            let det = e * g - f * f;
            let curv = (det > 0.0).then(|| {
                let gauss = (l * nn - m * m) / det;
                let mean = (e * nn - 2.0 * f * m + g * l) / (2.0 * det);
                let disc = (mean * mean - gauss).max(0.0).sqrt();
                [(mean - disc) * norm.size, (mean + disc) * norm.size]
            });
            let q = norm.to(Point3::new(p.x, p.y, p.z));
            if q.is_finite() && n.is_finite() {
                out.push(Sample { q, n, curv });
            }
        }
    }
    out
}

fn dense_points(s: &Surface, over: &Extent2d, norm: Norm) -> Vec<Vec3> {
    let (u, v) = (over.x_interval(), over.y_interval());
    let mut out = Vec::with_capacity(DENSE * DENSE);
    for x in ticks(u.min(), u.max(), DENSE, false) {
        for y in ticks(v.min(), v.max(), DENSE, false) {
            let q = norm.to(s.point(Point2::new(x, y)));
            if q.is_finite() {
                out.push(q);
            }
        }
    }
    out
}

// ---------------------------------------------------------------- candidates

/// The parameters, by kind:
/// - plane `[p, n]`, sphere `[c, r]`, cylinder `[q, a, r]`, cone `[o, a, r0, k]` (radius
///   `r0 + k z` at height `z` above `o`), torus `[c, a, R, r]`.
type Theta = Vec<f64>;

fn v3(t: &[f64], i: usize) -> Vec3 {
    let g = |k: usize| t.get(i + k).copied().unwrap_or(f64::NAN);
    Vec3::new(g(0), g(1), g(2))
}

fn s1(t: &[f64], i: usize) -> f64 {
    t.get(i).copied().unwrap_or(f64::NAN)
}

/// The signed distance of `q` from the kind (from the cone's line, for the cone).
fn residual(kind: Kind, t: &[f64], q: Vec3) -> f64 {
    match kind {
        Kind::Plane => {
            let n = v3(t, 3);
            (q - v3(t, 0)).dot(n) / n.length()
        }
        Kind::Sphere => (q - v3(t, 0)).length() - s1(t, 3),
        Kind::Cylinder => {
            let a = v3(t, 3);
            (q - v3(t, 0)).cross(a).length() / a.length() - s1(t, 6)
        }
        Kind::Cone => {
            let a = v3(t, 3);
            let al = a.length();
            let w = q - v3(t, 0);
            let (z, rho) = (w.dot(a) / al, w.cross(a).length() / al);
            let k = s1(t, 7);
            (rho - s1(t, 6) - k * z) / (1.0 + k * k).sqrt()
        }
        Kind::Torus => {
            let a = v3(t, 3);
            let al = a.length();
            let w = q - v3(t, 0);
            let (z, rho) = (w.dot(a) / al, w.cross(a).length() / al);
            let big = s1(t, 6);
            ((rho - big) * (rho - big) + z * z).sqrt() - s1(t, 7)
        }
    }
}

fn max_residual(kind: Kind, t: &[f64], qs: impl Iterator<Item = Vec3>) -> f64 {
    let mut m = 0.0_f64;
    for q in qs {
        let r = residual(kind, t, q).abs();
        if !r.is_finite() {
            return f64::INFINITY;
        }
        m = m.max(r);
    }
    m
}

fn mean(xs: impl Iterator<Item = f64>) -> f64 {
    let (mut s, mut c) = (0.0, 0.0);
    for x in xs {
        s += x;
        c += 1.0;
    }
    s / c
}

fn mean3(xs: impl Iterator<Item = Vec3>) -> Vec3 {
    let (mut s, mut c) = (Vec3::new(0.0, 0.0, 0.0), 0.0);
    for x in xs {
        s += x;
        c += 1.0;
    }
    s * (1.0 / c)
}

/// The symmetric 3 x 3 `sum f(x) f(x)^T`'s eigenvectors, smallest eigenvalue first.
fn eigen(rows: impl Iterator<Item = Vec3>) -> Option<([f64; 3], [Vec3; 3])> {
    let mut m = [0.0; 9];
    for v in rows {
        let a = [v.x, v.y, v.z];
        for (i, ai) in a.iter().enumerate() {
            for (j, aj) in a.iter().enumerate() {
                if let Some(slot) = m.get_mut(3 * i + j) {
                    *slot += ai * aj;
                }
            }
        }
    }
    let mut mat = Matrix::from_row_major(3, 3, m.to_vec()).ok()?;
    mat.compute_3x3_eigen_vectors().ok()
}

/// `x` with `A x = b` for the 3 x 3 `A`, `None` where it is near singular.
fn solve3(a: [f64; 9], b: Vec3) -> Option<Vec3> {
    let m = Matrix::from_row_major(3, 3, a.to_vec()).ok()?;
    let s = m.solve_linear_system(&[b.x, b.y, b.z]).ok()?;
    if s.near_singular {
        return None;
    }
    let v = Vec3::new(
        s.x.first().copied()?,
        s.x.get(1).copied()?,
        s.x.get(2).copied()?,
    );
    v.is_finite().then_some(v)
}

/// `sum (I - n n^T)` and `sum (I - n n^T) q` over the samples: where the normal lines meet.
fn normal_lines(samples: &[Sample]) -> ([f64; 9], Vec3) {
    let mut a = [0.0; 9];
    let mut b = Vec3::new(0.0, 0.0, 0.0);
    for s in samples {
        let n = [s.n.x, s.n.y, s.n.z];
        for (i, ni) in n.iter().enumerate() {
            for (j, nj) in n.iter().enumerate() {
                if let Some(slot) = a.get_mut(3 * i + j) {
                    *slot += if i == j { 1.0 } else { 0.0 } - ni * nj;
                }
            }
        }
        b += s.q - s.n * s.n.dot(s.q);
    }
    (a, b)
}

fn candidate(kind: Kind, samples: &[Sample]) -> Option<Theta> {
    let qs = || samples.iter().map(|s| s.q);
    match kind {
        Kind::Plane => {
            let c = mean3(qs());
            let (_, vecs) = eigen(qs().map(|q| q - c))?;
            let n = *vecs.first()?;
            Some(vec![c.x, c.y, c.z, n.x, n.y, n.z])
        }
        Kind::Sphere => {
            let (a, b) = normal_lines(samples);
            let c = solve3(a, b)?;
            let r = mean(qs().map(|q| (q - c).length()));
            Some(vec![c.x, c.y, c.z, r])
        }
        Kind::Cylinder => {
            let (_, vecs) = eigen(samples.iter().map(|s| s.n))?;
            let ax = *vecs.first()?;
            let (mut a, mut b) = normal_lines(samples);
            let count = samples.len() as f64;
            let axv = [ax.x, ax.y, ax.z];
            for (i, ai) in axv.iter().enumerate() {
                for (j, aj) in axv.iter().enumerate() {
                    if let Some(slot) = a.get_mut(3 * i + j) {
                        *slot += count * ai * aj;
                    }
                }
            }
            b += ax * (count * ax.dot(mean3(qs())));
            let q0 = solve3(a, b)?;
            let r = mean(qs().map(|q| (q - q0).cross(ax).length()));
            Some(vec![q0.x, q0.y, q0.z, ax.x, ax.y, ax.z, r])
        }
        Kind::Cone => {
            let nbar = mean3(samples.iter().map(|s| s.n));
            let (_, vecs) = eigen(samples.iter().map(|s| s.n - nbar))?;
            let mut ax = *vecs.first()?;
            // The tangent planes meet at the apex: `n . x = n . q`.
            let mut a = [0.0; 9];
            let mut b = Vec3::new(0.0, 0.0, 0.0);
            for s in samples {
                let n = [s.n.x, s.n.y, s.n.z];
                for (i, ni) in n.iter().enumerate() {
                    for (j, nj) in n.iter().enumerate() {
                        if let Some(slot) = a.get_mut(3 * i + j) {
                            *slot += ni * nj;
                        }
                    }
                }
                b += s.n * s.n.dot(s.q);
            }
            let apex = solve3(a, b)?;
            if mean(qs().map(|q| (q - apex).dot(ax))) < 0.0 {
                ax *= -1.0;
            }
            let zbar = mean(qs().map(|q| (q - apex).dot(ax)));
            let rbar = mean(qs().map(|q| (q - apex).cross(ax).length()));
            if !(zbar > 0.0) {
                return None;
            }
            let k = rbar / zbar;
            let o = apex + ax * zbar;
            Some(vec![o.x, o.y, o.z, ax.x, ax.y, ax.z, rbar, k])
        }
        Kind::Torus => torus_candidate(samples),
    }
}

/// The torus from the tube's curvature, which every sample shares, and the core circle
/// through three centres of curvature.
fn torus_candidate(samples: &[Sample]) -> Option<Theta> {
    let middle = samples.get(samples.len() / 2)?;
    let shared = |kappa: f64| {
        kappa.abs() > 0.0
            && samples.iter().all(|s| {
                s.curv
                    .is_some_and(|c| c.iter().any(|&x| (x - kappa).abs() <= 0.05 * kappa.abs()))
            })
    };
    let kappa = middle.curv?.into_iter().find(|&k| shared(k))?;
    let centre = |s: &Sample| s.q + s.n * (1.0 / kappa);
    // Three centres far apart: the first and last samples and the middle one.
    let (a, b, c) = (
        centre(samples.first()?),
        centre(middle),
        centre(samples.last()?),
    );
    let (ab, ac) = (b - a, c - a);
    let normal = ab.cross(ac);
    let nn = normal.length_squared();
    if !(nn > 0.0) {
        return None;
    }
    // The circumcentre of `a, b, c`.
    let o = a
        + (normal.cross(ab) * ac.length_squared() + ac.cross(normal) * ab.length_squared())
            * (0.5 / nn);
    let ax = normal * (1.0 / nn.sqrt());
    let big = (a - o).length();
    let small = 1.0 / kappa.abs();
    (big > small).then(|| vec![o.x, o.y, o.z, ax.x, ax.y, ax.z, big, small])
}

// ---------------------------------------------------------------- refinement

/// Levenberg-Marquardt on the residuals at `pts`, forward differences.
fn refine(kind: Kind, theta: &mut Theta, pts: &[Vec3]) {
    let m = theta.len();
    let cost = |t: &[f64]| -> f64 {
        pts.iter()
            .map(|&q| {
                let r = residual(kind, t, q);
                r * r
            })
            .sum()
    };
    let mut best = cost(theta);
    if !best.is_finite() {
        return;
    }
    let mut lambda = 1e-3;
    for _ in 0..LM_STEPS {
        let base: Vec<f64> = pts.iter().map(|&q| residual(kind, theta, q)).collect();
        let mut jac = vec![0.0; pts.len() * m];
        for j in 0..m {
            let Some(&tj) = theta.get(j) else { continue };
            let h = 1e-7 * tj.abs().max(1.0);
            let mut t = theta.clone();
            if let Some(slot) = t.get_mut(j) {
                *slot = tj + h;
            }
            for (i, (&q, &r0)) in pts.iter().zip(&base).enumerate() {
                if let Some(slot) = jac.get_mut(i * m + j) {
                    *slot = (residual(kind, &t, q) - r0) / h;
                }
            }
        }
        let mut jtj = vec![0.0; m * m];
        let mut jtr = vec![0.0; m];
        for (i, &r) in base.iter().enumerate() {
            let row = jac.get(i * m..(i + 1) * m).unwrap_or(&[]);
            for (a, &ja) in row.iter().enumerate() {
                if let Some(slot) = jtr.get_mut(a) {
                    *slot -= ja * r;
                }
                for (b, &jb) in row.iter().enumerate() {
                    if let Some(slot) = jtj.get_mut(a * m + b) {
                        *slot += ja * jb;
                    }
                }
            }
        }
        let mut improved = false;
        for _ in 0..8 {
            let mut a = jtj.clone();
            for d in 0..m {
                if let Some(slot) = a.get_mut(d * m + d) {
                    *slot += lambda * *slot + 1e-15;
                }
            }
            let Some(step) = Matrix::from_row_major(m, m, a)
                .ok()
                .and_then(|mat| mat.solve_linear_system(&jtr).ok())
            else {
                lambda *= 10.0;
                continue;
            };
            let trial: Theta = theta.iter().zip(&step.x).map(|(t, s)| t + s).collect();
            let c = cost(&trial);
            if c < best {
                let small = best - c <= 1e-15 * best;
                *theta = trial;
                best = c;
                lambda = (lambda * 0.1).max(1e-12);
                improved = !small;
                break;
            }
            lambda *= 10.0;
        }
        if !improved {
            break;
        }
    }
}

// ---------------------------------------------------------------- the analytic surface

/// The kind in its frame, as the certificate reads it (world units).
#[derive(Debug, Clone, Copy)]
enum Local {
    Plane,
    Sphere { r: f64 },
    Cylinder { r: f64 },
    Cone { r: f64, k: f64 },
    Torus { big: f64, small: f64 },
}

struct Built {
    surface: Surface,
    local: Local,
    origin: Point3,
    axes: [Vec3; 3],
}

/// A unit vector square to `z`.
fn hint(z: Vec3) -> Vec3 {
    let (x, y, w) = (z.x.abs(), z.y.abs(), z.z.abs());
    if x <= y && x <= w {
        Vec3::new(1.0, 0.0, 0.0)
    } else if y <= w {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        Vec3::new(0.0, 0.0, 1.0)
    }
}

fn build(kind: Kind, t: &[f64], norm: Norm) -> Option<Built> {
    let len = |x: f64| Length::from_mm(x * norm.size);
    let frame = |o: Vec3, z: Vec3| Frame::new(norm.from(o), z, hint(z));
    let (surface, local, frame) = match kind {
        Kind::Plane => {
            let f = frame(v3(t, 0), v3(t, 3))?;
            (Surface::Plane(Plane::new(f)), Local::Plane, f)
        }
        Kind::Sphere => {
            let f = frame(v3(t, 0), Vec3::new(0.0, 0.0, 1.0))?;
            let s = Sphere::new(f, len(s1(t, 3)), Sense::Forward).ok()?;
            let r = s.radius().mm();
            (Surface::Sphere(s), Local::Sphere { r }, f)
        }
        Kind::Cylinder => {
            let f = frame(v3(t, 0), v3(t, 3))?;
            let c = Cylinder::new(f, len(s1(t, 6)), Sense::Forward).ok()?;
            let r = c.radius().mm();
            (Surface::Cylinder(c), Local::Cylinder { r }, f)
        }
        Kind::Cone => {
            let (mut a, mut k) = (v3(t, 3), s1(t, 7));
            if k < 0.0 {
                a *= -1.0;
                k = -k;
            }
            let f = frame(v3(t, 0), a)?;
            let c = Cone::new(f, len(s1(t, 6)), fmath::atan(k), Sense::Forward).ok()?;
            let (r, k) = (c.radius().mm(), c.slope());
            (Surface::Cone(c), Local::Cone { r, k }, f)
        }
        Kind::Torus => {
            let f = frame(v3(t, 0), v3(t, 3))?;
            let s = Torus::new(f, len(s1(t, 6)), len(s1(t, 7)), Sense::Forward).ok()?;
            let (big, small) = (s.major().mm(), s.minor().mm());
            (Surface::Torus(s), Local::Torus { big, small }, f)
        }
    };
    Some(Built {
        surface,
        local,
        origin: frame.origin(),
        axes: [frame.x().as_vec(), frame.y().as_vec(), frame.z().as_vec()],
    })
}

/// `built`'s surface, reversed where its normal at the foot of the given surface's middle
/// point opposes the given normal there.
fn oriented(built: Surface, local: &Local, given: &Surface, mid: Point2) -> Surface {
    let Some(n) = given.normal(mid, ABOVE) else {
        return built;
    };
    let p = given.point(mid);
    // The kind's forward normal at `p`'s foot: away from the axis, centre or core circle.
    let forward = match (local, &built) {
        (Local::Plane, Surface::Plane(k)) => k.frame().z().as_vec(),
        (Local::Sphere { .. }, Surface::Sphere(k)) => p - k.frame().origin(),
        (Local::Cylinder { .. }, Surface::Cylinder(k)) => {
            let z = k.frame().z().as_vec();
            let w = p - k.frame().origin();
            w - z * w.dot(z)
        }
        (Local::Cone { k: slope, .. }, Surface::Cone(k)) => {
            let z = k.frame().z().as_vec();
            let w = p - k.frame().origin();
            let radial = w - z * w.dot(z);
            radial
                .normalized()
                .map_or(z * -1.0, |r| r.as_vec() - z * *slope)
        }
        (Local::Torus { big, .. }, Surface::Torus(k)) => {
            let z = k.frame().z().as_vec();
            let w = p - k.frame().origin();
            let radial = w - z * w.dot(z);
            let core = radial.normalized().map_or(z * 0.0, |r| r.as_vec() * *big);
            w - core
        }
        _ => return built,
    };
    if forward.dot(n.as_vec()) < 0.0 {
        match built {
            // The plane's normal is its frame's z: the frame turned over, x kept.
            Surface::Plane(k) => {
                let f = k.frame();
                Frame::new(f.origin(), f.z().as_vec() * -1.0, f.x().as_vec())
                    .map_or(Surface::Plane(k), |f| Surface::Plane(Plane::new(f)))
            }
            other => other.reversed(ParamAxis::U).0,
        }
    } else {
        built
    }
}

// ---------------------------------------------------------------- the certificate

/// The bound of the deviation of `n` from `built`, proved, or `None` (refuted or not
/// proved).
fn certify(
    n: &NurbsSurface<3>,
    built: &Built,
    acc: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Option<f64>, SurfaceError> {
    let mut worst = 0.0_f64;
    let mut reach = 0.0_f64;
    for patch in patches(n) {
        let net = local_net(&patch.net, built.origin, built.axes);
        for (h, _) in &net {
            reach = reach.max((h[0].abs() + h[1].abs() + h[2].abs()) / h[3]);
        }
        let mut stack = vec![(patch.lo, patch.hi, net)];
        while let Some((lo, hi, net)) = stack.pop() {
            budget.charge(1).map_err(SurfaceError::Exhausted)?;
            stop.check().map_err(|_| SurfaceError::Stopped)?;
            match bound(&built.local, &net, patch.degrees, acc)? {
                Verdict::Proved(b) => worst = worst.max(b),
                Verdict::Refuted | Verdict::Unprovable => return Ok(None),
                Verdict::Open => {
                    let m = lo.midpoint(hi);
                    let su = lo.x < m.x && m.x < hi.x;
                    let sv = lo.y < m.y && m.y < hi.y;
                    if !(su || sv) {
                        return Ok(None);
                    }
                    let mut parts = vec![(lo, hi, net)];
                    if su {
                        parts = parts
                            .into_iter()
                            .flat_map(|(lo, hi, net)| {
                                let (a, b) = halved(&net, patch.degrees, true);
                                [
                                    (lo, Point2::new(m.x, hi.y), a),
                                    (Point2::new(m.x, lo.y), hi, b),
                                ]
                            })
                            .collect();
                    }
                    if sv {
                        parts = parts
                            .into_iter()
                            .flat_map(|(lo, hi, net)| {
                                let (a, b) = halved(&net, patch.degrees, false);
                                [
                                    (lo, Point2::new(hi.x, m.y), a),
                                    (Point2::new(lo.x, m.y), hi, b),
                                ]
                            })
                            .collect();
                    }
                    // Last pushed is first taken: the lowest corner first.
                    stack.extend(parts.into_iter().rev());
                }
            }
        }
    }
    let sizes = match built.local {
        Local::Plane => 0.0,
        Local::Sphere { r } | Local::Cylinder { r } | Local::Cone { r, .. } => r,
        Local::Torus { big, small } => big + small,
    };
    let total = up(worst + 16.0 * EPS * (reach + sizes));
    Ok((total <= acc).then_some(total))
}

/// The net in the frame `(origin, axes)`: `(X, Y, Z, w)` with `X = (w P - w O) . x` and so
/// on, each with its error bound (the inputs' carried, and the roundings of the shift and
/// the dot product).
fn local_net(net: &[Hb], origin: Point3, axes: [Vec3; 3]) -> Vec<Hb> {
    let o = [origin.x, origin.y, origin.z];
    net.iter()
        .map(|(h, e)| {
            let (w, ew) = (h[3], e[3]);
            let mut d = [0.0; 3];
            let mut ed = [0.0; 3];
            for k in 0..3 {
                let (hk, ok) = (
                    h.get(k).copied().unwrap_or(f64::NAN),
                    o.get(k).copied().unwrap_or(f64::NAN),
                );
                let ek = e.get(k).copied().unwrap_or(f64::NAN);
                if let (Some(dk), Some(edk)) = (d.get_mut(k), ed.get_mut(k)) {
                    *dk = hk - ok * w;
                    *edk = up(ek + ok.abs() * ew + 2.0 * EPS * (hk.abs() + (ok * w).abs()));
                }
            }
            let mut out = ([0.0, 0.0, 0.0, w], [0.0, 0.0, 0.0, ew]);
            for (j, a) in axes.iter().enumerate() {
                let av = [a.x, a.y, a.z];
                let mut v = 0.0;
                let mut size = 0.0;
                let mut err = 0.0;
                for ((dk, edk), ak) in d.iter().zip(&ed).zip(&av) {
                    v += dk * ak;
                    size += (dk * ak).abs();
                    err += ak.abs() * edk;
                }
                if let (Some(slot), Some(se)) = (out.0.get_mut(j), out.1.get_mut(j)) {
                    *slot = v;
                    *se = up(err + 4.0 * EPS * size);
                }
            }
            out
        })
        .collect()
}

enum Verdict {
    /// The piece is within this bound, at most the accuracy.
    Proved(f64),
    /// A corner is beyond the accuracy.
    Refuted,
    /// A corner is too far for the bound's factor to hold: not provable here.
    Unprovable,
    /// Neither yet: halve.
    Open,
}

fn form(net: &[Hb], (du, dv): (usize, usize), k: usize) -> Result<Bounded, SurfaceError> {
    let c = net
        .iter()
        .map(|(h, _)| h.get(k).copied().unwrap_or(f64::NAN))
        .collect();
    let err = net.iter().fold(0.0, |m: f64, (_, e)| {
        m.max(e.get(k).copied().unwrap_or(f64::NAN))
    });
    Bernstein2::new(du, dv, c)
        .map(|f| Bounded { f, err })
        .map_err(|_| SurfaceError::Defect {
            reason: DEFECT_BERNSTEIN,
        })
}

/// `max_k (|N_k| + err_N) / (D_k - err_D)`, rounded up: a bound of `|N / D|` for forms of
/// one degree whose `D` coefficients are all positive; `None` where one is not.
fn max_ratio(num: &Bounded, den: &Bounded) -> Option<f64> {
    if !(num.is_finite() && den.is_finite()) {
        return None;
    }
    let mut m = 0.0_f64;
    for (&nk, &dk) in num.f.coefficients().iter().zip(den.f.coefficients()) {
        let lo = (dk - den.err) * (1.0 - 2.0 * EPS);
        if !(lo > 0.0) {
            return None;
        }
        m = m.max(up(nk.abs() + num.err) / lo);
    }
    m.is_finite().then(|| up(m))
}

/// The least and largest of `Z / w` over the piece: of its coefficients' quotients, each at
/// the end of its error that widens it.
fn ratio_range(net: &[Hb], k: usize) -> Option<(f64, f64)> {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for (h, e) in net {
        let (z, ez) = (h.get(k).copied()?, e.get(k).copied()?);
        let (wl, wh) = (h[3] - e[3], h[3] + e[3]);
        if !(wl > 0.0) {
            return None;
        }
        let (nl, nh) = (z - ez, z + ez);
        let l = if nl >= 0.0 { nl / wh } else { nl / wl };
        let u = if nh >= 0.0 { nh / wl } else { nh / wh };
        lo = lo.min(l - up(l.abs() * 2.0 * EPS));
        hi = hi.max(u + up(u.abs() * 2.0 * EPS));
    }
    (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
}

/// A lower bound of `rho = |(x, y)|` over the piece (0 where none is found): the piece is
/// inside its control points' hull, so `rho >= e . p` for the unit `e` along their mean,
/// at the least of the points.
fn radial_floor(net: &[Hb]) -> f64 {
    let pts: Vec<(f64, f64, f64)> = net
        .iter()
        .map(|(h, e)| {
            let wl = h[3] - e[3];
            let slack = if wl > 0.0 {
                (e[0] + e[1]) / wl + 4.0 * EPS * (h[0].abs() + h[1].abs()) / wl
            } else {
                f64::INFINITY
            };
            (h[0] / h[3], h[1] / h[3], slack)
        })
        .collect();
    let (mx, my) = pts
        .iter()
        .fold((0.0, 0.0), |(a, b), &(x, y, _)| (a + x, b + y));
    let len = (mx * mx + my * my).sqrt();
    if !(len > 0.0) {
        return 0.0;
    }
    let (ex, ey) = (mx / len, my / len);
    let lo = pts.iter().fold(f64::INFINITY, |m: f64, &(x, y, sl)| {
        m.min(ex * x + ey * y - sl - 4.0 * EPS * (x.abs() + y.abs()))
    });
    if lo.is_finite() { lo.max(0.0) } else { 0.0 }
}

/// The piece's homogeneous forms in the kind's frame.
struct Forms {
    x: Bounded,
    y: Bounded,
    z: Bounded,
    w: Bounded,
}

fn sq(f: &Bounded) -> Result<Bounded, SurfaceError> {
    f.mul(f).map_err(|_| SurfaceError::Defect {
        reason: DEFECT_BERNSTEIN,
    })
}

fn defect<T, E>(r: Result<T, E>) -> Result<T, SurfaceError> {
    r.map_err(|_| SurfaceError::Defect {
        reason: DEFECT_BERNSTEIN,
    })
}

/// `f` with its error raised by `extra`.
fn widen(mut f: Bounded, extra: f64) -> Bounded {
    f.err = up(f.err + extra);
    f
}

/// The distance of the local point `(x, y, z)` from the kind, in floating point, and its
/// radial part `delta` (the quantity whose branch the bound needs).
fn distance(local: &Local, p: Vec3) -> (f64, f64) {
    let rho = (p.x * p.x + p.y * p.y).sqrt();
    match *local {
        Local::Plane => (p.z.abs(), 0.0),
        Local::Sphere { r } => {
            let d = (p.length() - r).abs();
            (d, d)
        }
        Local::Cylinder { r } => {
            let d = (rho - r).abs();
            (d, d)
        }
        Local::Cone { r, k } => {
            let s = r + k * p.z;
            let c = 1.0 / (1.0 + k * k).sqrt();
            // The foot on the cone's line lies on the nappe where `s + k^2 rho >= 0`.
            if s + k * k * rho >= 0.0 {
                ((rho - s).abs() * c, (rho - s).abs())
            } else {
                let apex = -r / k;
                let d = (rho * rho + (p.z - apex) * (p.z - apex)).sqrt();
                (d, f64::INFINITY)
            }
        }
        Local::Torus { big, small } => {
            let d = ((rho - big) * (rho - big) + p.z * p.z).sqrt();
            ((d - small).abs(), (d - small).abs())
        }
    }
}

fn bound(
    local: &Local,
    net: &[Hb],
    degrees: (usize, usize),
    acc: f64,
) -> Result<Verdict, SurfaceError> {
    let (du, dv) = degrees;
    let at = |i: usize, j: usize| {
        net.get(i * (dv + 1) + j)
            .map_or(Vec3::splat(f64::NAN), |(h, _)| {
                Vec3::new(h[0] / h[3], h[1] / h[3], h[2] / h[3])
            })
    };
    let corners = [at(0, 0), at(du, 0), at(0, dv), at(du, dv)];
    let scale = corners
        .iter()
        .fold(0.0_f64, |m, c| m.max(c.max_dimension()));
    let margin = 64.0 * EPS * scale;
    let mut first_delta = f64::INFINITY;
    let mut seen = 0.0_f64;
    for (i, c) in corners.iter().enumerate() {
        let (d, delta) = distance(local, *c);
        if !(d <= acc + margin) {
            return Ok(Verdict::Refuted);
        }
        seen = seen.max(d);
        if i == 0 {
            first_delta = delta;
        }
    }
    let f = Forms {
        x: form(net, degrees, 0)?,
        y: form(net, degrees, 1)?,
        z: form(net, degrees, 2)?,
        w: form(net, degrees, 3)?,
    };
    // The radial part's bound, the distance's share of it, and the factor's reach
    // `delta_max`: no more than twice the accuracy's radial reach, so the factor loses at
    // most that much.
    let (bd, share, delta_max) = match *local {
        Local::Plane => (max_ratio(&f.z, &f.w), 1.0, f64::INFINITY),
        Local::Sphere { r } | Local::Cylinder { r } => {
            let mut q = defect(sq(&f.x)?.add(&sq(&f.y)?))?;
            if matches!(local, Local::Sphere { .. }) {
                q = defect(q.add(&sq(&f.z)?))?;
            }
            let w2 = sq(&f.w)?;
            let r2 = r * r;
            let q = widen(
                defect(q.add_scaled(&w2, -r2))?,
                2.0 * EPS * r2 * w2.f.max_abs_coefficient(),
            );
            let dm = (r / 8.0).min(2.0 * acc);
            (
                max_ratio(&q, &w2).map(|b| up(b / ((2.0 * r - dm) * (1.0 - 2.0 * EPS)))),
                1.0,
                dm,
            )
        }
        Local::Cone { r, k } => {
            let Some((zl, zh)) = ratio_range(net, 2) else {
                return Ok(Verdict::Open);
            };
            let s_min = (r + (k * zl).min(k * zh)) * (1.0 - 4.0 * EPS)
                - 4.0 * EPS * (k * zl).abs().max((k * zh).abs());
            if !(s_min > 0.0) {
                return Ok(Verdict::Open);
            }
            let w2 = sq(&f.w)?;
            let g = defect(f.w.scale(r).add_scaled(&f.z, k))?;
            let q = defect(defect(sq(&f.x)?.add(&sq(&f.y)?))?.sub(&sq(&g)?))?;
            let c = 1.0 / (1.0 + k * k).sqrt();
            let dm = (s_min / 8.0).min(2.0 * acc / c);
            (
                max_ratio(&q, &w2).map(|b| up(b / ((2.0 * s_min - dm) * (1.0 - 2.0 * EPS)))),
                c * (1.0 + 4.0 * EPS),
                dm,
            )
        }
        Local::Torus { big, small } => {
            let p = defect(sq(&f.x)?.add(&sq(&f.y)?))?;
            let w2 = sq(&f.w)?;
            let shift = big * big - small * small;
            let s = defect(defect(p.add(&sq(&f.z)?))?.add_scaled(&w2, shift))?;
            let s = widen(
                s,
                4.0 * EPS * (big * big + small * small) * w2.f.max_abs_coefficient(),
            );
            let pw = defect(p.mul(&w2))?;
            let four = 4.0 * big * big;
            let n = defect(sq(&s)?.add_scaled(&pw, -four))?;
            let n = widen(n, 4.0 * EPS * four * pw.f.max_abs_coefficient());
            let w4 = sq(&w2)?;
            let dm = (small.min(big - small) / 8.0).min(2.0 * acc);
            // `d_t'^2 >= (rho + R)^2`, with `rho` at least `R - r - delta` and at least the
            // piece's least reach from the axis along its control points' mean direction.
            let far = (2.0 * big - small - dm).max(radial_floor(net) + big);
            let lead = (2.0 * small - dm) * (far * far - small * small);
            (
                max_ratio(&n, &w4).map(|b| up(b / (lead * (1.0 - 8.0 * EPS)))),
                1.0,
                dm,
            )
        }
    };
    // The bound holds only where `delta` starts below its reach.
    if !(first_delta < delta_max) {
        return Ok(Verdict::Unprovable);
    }
    if let Some(bd) = bd
        && bd < delta_max
    {
        let b = up(bd * share);
        if b <= acc {
            return Ok(Verdict::Proved(b));
        }
    }
    // A corner this close to the accuracy leaves the bound too little room to close.
    Ok(if seen >= acc * (1.0 - NEAR) {
        Verdict::Unprovable
    } else {
        Verdict::Open
    })
}
