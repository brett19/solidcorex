// Written for solidcorex. Apache-2.0.
//! What [`tangent_all_along`](crate::tangent_all_along) and
//! [`continuity`](crate::continuity) share (research 74 §25): each face's normal field as a
//! closed form of a point, the curvature of its normal section across the edge, the edge's
//! curve as a Taylor model over a piece of its interval, and the adaptive proof over the
//! pieces.
//!
//! **The faces at a point of the edge.** At the edge curve's point `p = C(t)` each analytic
//! face's surface is reached along its own normal field: the unit normal `n0(p)` of the
//! level sets of its distance (the plane's axis; the radial direction of a cylinder; the
//! direction from a sphere's centre; a cone's slant normal; a torus's direction from its tube
//! circle) and the signed distance `delta0(p)`, so the foot is `p - delta0 n0` and the
//! surface's normal there is `n0`, exactly. Nothing is solved and no pcurve is read: the
//! same closed forms take a NURBS edge curve as well as an analytic one. A NURBS face has no
//! such field: `Unsupported`.
//!
//! **The section across the edge.** At a foot `p*` with oriented normal `n`, the direction
//! across the edge is `w = n x T` (normalised), `T` the curve's tangent. The surface's height
//! `z(a)` over the line `p* + a w`, along `n`, solves `F(p* + a w + z(a) n) = 0` for its
//! implicit polynomial `F`; with `z(0) = z'(0) = 0`, each coefficient `z_k` (`k = 2, 3, 4`)
//! is `-E_k / g`, `E_k` the `a^k` coefficient of `F` with `z_k` still 0 and `g` the
//! derivative of `F` along `n`: `F`'s `a^k` coefficient is linear in `z_k`. The section's
//! curvature, its arc-length derivative and that derivative's are `2 z2`, `6 z3` and
//! `24 z4 - 24 z2^3`.
//!
//! **One code, two rings.** Every formula is written over [`Num`]: an [`Ivl`] gives a
//! certified bracket at one parameter, a [`TmN`] an enclosure over a piece that keeps the
//! correlation of the quantities built from one curve (a constant angle cancels to rounding
//! over a whole piece, where intervals would not shrink with it).

use core::ops::{Add, Mul, Sub};

use scx_brep::{Brep, EdgeId, EdgeShape, Entity, FaceId};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::fmath;
use scx_num::interval::{Ivl, round_up};
use scx_num::taylor_model::TmN;
use scx_num::trig::Trig;
use scx_num::vector3d::Vec3;
use scx_nurbs::Approach;
use scx_surface::Surface;

use crate::error::{BodyqError, BodyqRefusal, BodyqUnsupported};

/// The Taylor models' coefficient count: degree 5 in a piece's parameter.
pub(crate) const N: usize = 6;

/// A Taylor model of the edge's parameter over one piece.
pub(crate) type Tm = TmN<N>;

/// A number the formulas run over: a range at one parameter ([`Ivl`]) or a function of the
/// parameter over a piece ([`TmN`]). Each operation encloses the exact one.
pub(crate) trait Num:
    Copy + Add<Output = Self> + Sub<Output = Self> + Mul<Output = Self>
{
    /// The constant `x`.
    fn of(x: Ivl) -> Self;
    /// `1 / self`, where its range excludes 0.
    fn inv(self) -> Option<Self>;
    /// The square root, where its range is positive.
    fn root(self) -> Option<Self>;

    /// The number `x`, exact.
    fn num(x: f64) -> Self {
        Self::of(Ivl::from(x))
    }
}

impl Num for Ivl {
    fn of(x: Ivl) -> Self {
        x
    }

    fn inv(self) -> Option<Self> {
        (self.lo > 0.0 || self.hi < 0.0).then(|| Ivl::from(1.0) / self)
    }

    fn root(self) -> Option<Self> {
        self.is_positive().then(|| self.sqrt())
    }
}

impl<const M: usize> Num for TmN<M> {
    fn of(x: Ivl) -> Self {
        Self::constant(x)
    }

    fn inv(self) -> Option<Self> {
        let r = TmN::range(&self);
        (r.lo > 0.0 || r.hi < 0.0).then(|| self.recip())
    }

    fn root(self) -> Option<Self> {
        TmN::range(&self).is_positive().then(|| self.sqrt_tm())
    }
}

/// `v`'s coordinates as constants.
fn lift<T: Num>(v: Vec3) -> Vec3<T> {
    Vec3::new(T::num(v.x), T::num(v.y), T::num(v.z))
}

/// `v` normalised, where its length is positive.
fn unit<T: Num>(v: Vec3<T>) -> Option<Vec3<T>> {
    let inv = v.dot(v).root()?.inv()?;
    Some(v * inv)
}

// ---- a face's normal field ------------------------------------------------------------------

/// An analytic surface's kind and sizes, as its normal field reads them.
#[derive(Clone, Copy, Debug)]
enum Shape {
    Plane,
    Cylinder {
        r: f64,
    },
    /// `rho = r + k h`; `c = 1 / sqrt(1 + k^2)`, the slant normal's scale. `k` encloses the
    /// surface's stored slope (`tan` of the half-angle, to a few ulps).
    Cone {
        r: f64,
        k: Ivl,
        c: Ivl,
    },
    Sphere {
        r: f64,
    },
    Torus {
        major: f64,
        minor: f64,
    },
}

/// A face's surface as a normal field: `n0(p)`, `delta0(p)`, and its implicit polynomial.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Field {
    o: Vec3,
    x: Vec3,
    y: Vec3,
    /// The axis heights are measured along: the frame's z times the surface's sense (the
    /// plane's own z).
    z: Vec3,
    shape: Shape,
    /// The face's oriented normal is `sign * n0` (the natural normal's sign times the face's
    /// orientation across the edge, [`Faces::of`]).
    sign: f64,
}

/// The normal field at a point: `n0` and the signed distance along it.
pub(crate) struct Foot<T> {
    pub(crate) n0: Vec3<T>,
    pub(crate) delta: T,
}

impl Field {
    /// `surface`'s field, and the sign of its natural normal against `n0`; `None` for a
    /// NURBS surface (or a kind this does not know yet).
    fn of(surface: &Surface) -> Option<(Self, f64)> {
        let at = |frame: &scx_num::frame::Frame, s: f64, shape| {
            (
                Self {
                    o: frame.origin() - scx_num::vector3d::Point3::ORIGIN,
                    x: *frame.x(),
                    y: *frame.y(),
                    z: *frame.z() * s,
                    shape,
                    sign: 1.0,
                },
                s,
            )
        };
        Some(match surface {
            Surface::Plane(p) => at(p.frame(), 1.0, Shape::Plane),
            Surface::Cylinder(c) => at(
                c.frame(),
                c.sense().sign(),
                Shape::Cylinder { r: c.radius().mm() },
            ),
            Surface::Cone(c) => {
                let t = fmath::tan(c.half_angle());
                // tan rounds to within an ulp or two; four ulps out each way covers it
                let mut k = Ivl::outward_ulp(t, t);
                for _ in 0..3 {
                    k = Ivl::outward_ulp(k.lo, k.hi);
                }
                let c2 = Ivl::from(1.0) + k * k;
                let scale = Ivl::from(1.0) / c2.sqrt();
                at(
                    c.frame(),
                    c.sense().sign(),
                    Shape::Cone {
                        r: c.radius().mm(),
                        k,
                        c: scale,
                    },
                )
            }
            Surface::Sphere(s) => at(
                s.frame(),
                s.sense().sign(),
                Shape::Sphere { r: s.radius().mm() },
            ),
            Surface::Torus(t) => at(
                t.frame(),
                t.sense().sign(),
                Shape::Torus {
                    major: t.major().mm(),
                    minor: t.minor().mm(),
                },
            ),
            _ => return None,
        })
    }

    /// `n0` and `delta0` at `p` (a point's coordinates); `None` where the field has no
    /// direction (the axis of a cylinder or cone, a sphere's centre, a torus's tube circle)
    /// or the range is too wide to tell.
    pub(crate) fn foot<T: Num>(&self, p: Vec3<T>) -> Option<Foot<T>> {
        let d = p - lift(self.o);
        let radial = |d: Vec3<T>| {
            let (xl, yl) = (d.dot(lift(self.x)), d.dot(lift(self.y)));
            let rho = (xl * xl + yl * yl).root()?;
            let inv = rho.inv()?;
            Some((
                (lift::<T>(self.x) * xl + lift::<T>(self.y) * yl) * inv,
                rho,
                inv,
            ))
        };
        Some(match self.shape {
            Shape::Plane => Foot {
                n0: lift(self.z),
                delta: d.dot(lift(self.z)),
            },
            Shape::Cylinder { r } => {
                let (e, rho, _) = radial(d)?;
                Foot {
                    n0: e,
                    delta: rho - T::num(r),
                }
            }
            Shape::Cone { r, k, c } => {
                let (e, rho, _) = radial(d)?;
                let h = d.dot(lift(self.z));
                let (k, c) = (T::of(k), T::of(c));
                Foot {
                    n0: (e - lift::<T>(self.z) * k) * c,
                    delta: c * (rho - T::num(r) - k * h),
                }
            }
            Shape::Sphere { r } => {
                let l = d.dot(d).root()?;
                Foot {
                    n0: d * l.inv()?,
                    delta: l - T::num(r),
                }
            }
            Shape::Torus { major, minor } => {
                let (_, rho, inv) = radial(d)?;
                let h = d.dot(lift(self.z));
                let (xl, yl) = (d.dot(lift(self.x)), d.dot(lift(self.y)));
                let f = T::num(1.0) - T::num(major) * inv;
                let q =
                    (lift::<T>(self.x) * xl + lift::<T>(self.y) * yl) * f + lift::<T>(self.z) * h;
                let off = rho - T::num(major);
                let l = (off * off + h * h).root()?;
                Foot {
                    n0: q * l.inv()?,
                    delta: l - T::num(minor),
                }
            }
        })
    }

    /// The implicit polynomial `F` at a point given by its series: zero on the surface, its
    /// gradient a positive or negative multiple of `n0` there.
    fn implicit<T: Num>(&self, q: Vec3<Ser<T>>) -> Ser<T> {
        let c = |x: f64| Ser::constant(T::num(x));
        let dot = |a: Vec3<Ser<T>>, v: Vec3| a.x * c(v.x) + a.y * c(v.y) + a.z * c(v.z);
        let d = Vec3::new(q.x - c(self.o.x), q.y - c(self.o.y), q.z - c(self.o.z));
        let planar = |d: Vec3<Ser<T>>| {
            let (xl, yl) = (dot(d, self.x), dot(d, self.y));
            xl * xl + yl * yl
        };
        match self.shape {
            Shape::Plane => dot(d, self.z),
            Shape::Cylinder { r } => planar(d) - Ser::constant(T::num(r) * T::num(r)),
            Shape::Cone { r, k, .. } => {
                let s = Ser::constant(T::num(r)) + Ser::constant(T::of(k)) * dot(d, self.z);
                planar(d) - s * s
            }
            Shape::Sphere { r } => {
                d.x * d.x + d.y * d.y + d.z * d.z - Ser::constant(T::num(r) * T::num(r))
            }
            Shape::Torus { major, minor } => {
                let dd = d.x * d.x + d.y * d.y + d.z * d.z;
                let rr = Ser::constant(T::num(major)) * Ser::constant(T::num(major))
                    - Ser::constant(T::num(minor)) * Ser::constant(T::num(minor));
                let s = dd + rr;
                let four_r2 = Ser::constant(T::num(4.0) * T::num(major) * T::num(major));
                s * s - four_r2 * planar(d)
            }
        }
    }

    /// The normal section's curvature `kappa`, its arc-length derivative and that
    /// derivative's at the foot `foot` with oriented normal `n`, across the unit direction `w`
    /// (square to `n`).
    pub(crate) fn bends<T: Num>(&self, foot: Vec3<T>, n: Vec3<T>, w: Vec3<T>) -> Option<[T; 3]> {
        let zero = T::num(0.0);
        // g: F's derivative along n at the foot
        let along = |dir: Vec3<T>, z: [T; 3]| {
            let comp = |p: T, d: T, nk: T| Ser {
                c: [p, d, z[0] * nk, z[1] * nk, z[2] * nk],
            };
            Vec3::new(
                comp(foot.x, dir.x, n.x),
                comp(foot.y, dir.y, n.y),
                comp(foot.z, dir.z, n.z),
            )
        };
        let [_, g, ..] = self.implicit(along(n, [zero; 3])).c;
        let ginv = g.inv()?;
        let mut z = [zero; 3];
        for k in 0..3 {
            let e = self.implicit(along(w, z)).c;
            let ek = e.get(k + 2).copied().unwrap_or(zero);
            if let Some(zk) = z.get_mut(k) {
                *zk = zero - ek * ginv;
            }
        }
        let [z2, z3, z4] = z;
        Some([
            T::num(2.0) * z2,
            T::num(6.0) * z3,
            T::num(24.0) * z4 - T::num(24.0) * z2 * z2 * z2,
        ])
    }
}

// ---- power series in the across-edge parameter ----------------------------------------------

/// A power series in `a` to `a^4`, its coefficients numbers of `T`.
#[derive(Clone, Copy)]
struct Ser<T> {
    c: [T; 5],
}

impl<T: Num> Ser<T> {
    fn constant(x: T) -> Self {
        let z = T::num(0.0);
        Self { c: [x, z, z, z, z] }
    }
}

impl<T: Num> Add for Ser<T> {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        let mut c = self.c;
        for (x, y) in c.iter_mut().zip(o.c) {
            *x = *x + y;
        }
        Self { c }
    }
}

impl<T: Num> Sub for Ser<T> {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        let mut c = self.c;
        for (x, y) in c.iter_mut().zip(o.c) {
            *x = *x - y;
        }
        Self { c }
    }
}

impl<T: Num> Mul for Ser<T> {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        let c = core::array::from_fn(|k| {
            self.c
                .iter()
                .take(k + 1)
                .zip(o.c.iter().take(k + 1).rev())
                .fold(T::num(0.0), |s, (&x, &y)| s + x * y)
        });
        Self { c }
    }
}

// ---- the edge and its two faces --------------------------------------------------------------

/// The two faces across an edge.
pub(crate) enum Faces {
    /// One surface on both sides with one oriented normal field (a seam, or two faces of one
    /// surface): every drop across the edge is exactly 0.
    Same,
    /// Two fields, each signed to its face's orientation across the edge.
    Two(Box<[Field; 2]>),
}

/// An edge to run along: its curve, its interval and where to cut it first (a NURBS curve's
/// interior knots), and its two faces.
pub(crate) struct Edge<'a> {
    pub(crate) curve: &'a Curve,
    pub(crate) lo: f64,
    pub(crate) hi: f64,
    pub(crate) faces: Faces,
}

impl<'a> Edge<'a> {
    /// `edge` of `body`.
    ///
    /// The faces' normals are oriented coherently across the edge: face a's natural normal
    /// as it is, face b's as it is where its use runs the other way along the edge from face
    /// a's (two faces of an oriented sheet), turned where both run the same way.
    pub(crate) fn of(body: &'a Brep, edge: EdgeId) -> Result<Self, BodyqError> {
        let at = Entity::Edge(edge);
        let e = body
            .edge(edge)
            .ok_or(BodyqError::Refused(BodyqRefusal::NoEntity { entity: at }))?;
        let EdgeShape::Curve { curve, interval } = e.shape() else {
            return Err(BodyqError::Refused(BodyqRefusal::Collapsed { edge }));
        };
        let fins: Vec<_> = e.fins().collect();
        let [fa, fb] = match fins.as_slice() {
            [a, b] => [*a, *b],
            [] | [_] => return Err(BodyqError::Refused(BodyqRefusal::OneSided { edge })),
            _ => {
                return Err(BodyqError::Unsupported(BodyqUnsupported::NonManifold {
                    at,
                }));
            }
        };
        let surface = |f: FaceId| {
            body.face(f)
                .map(scx_brep::Face::surface)
                .ok_or(BodyqError::Refused(BodyqRefusal::NoEntity {
                    entity: Entity::Face(f),
                }))
        };
        let (sa, sb) = (surface(fa.face)?, surface(fb.face)?);
        let turn = if fa.sense == fb.sense { -1.0 } else { 1.0 };
        let faces = if sa == sb && turn > 0.0 {
            Faces::Same
        } else {
            let field = |s: &Surface, f: FaceId| {
                Field::of(s).ok_or(BodyqError::Unsupported(BodyqUnsupported::Nurbs {
                    at: Entity::Face(f),
                }))
            };
            let (mut a, ka) = field(sa, fa.face)?;
            let (mut b, kb) = field(sb, fb.face)?;
            a.sign = ka;
            b.sign = kb * turn;
            Faces::Two(Box::new([a, b]))
        };
        Ok(Self {
            curve,
            lo: interval.min(),
            hi: interval.max(),
            faces,
        })
    }

    /// The first pieces: the interval cut at the curve's interior knots, so that no piece's
    /// Taylor model straddles a knot.
    pub(crate) fn pieces(&self) -> Vec<(f64, f64)> {
        let mut cuts = vec![self.lo];
        if let Curve::Nurbs(n) = self.curve {
            for &k in n.knots() {
                if k > self.lo && k < self.hi && cuts.last().is_some_and(|&l| k > l) {
                    cuts.push(k);
                }
            }
        }
        cuts.push(self.hi);
        cuts.windows(2)
            .filter_map(|w| match w {
                [a, b] => Some((*a, *b)),
                _ => None,
            })
            .collect()
    }

    /// The curve's point and derivative over `[lo, hi]` as Taylor models in
    /// `s = (t - m) / delta`, `m` the middle; `None` if the jets are not finite.
    pub(crate) fn piece(&self, lo: f64, hi: f64) -> Option<(Vec3<Tm>, Vec3<Tm>)> {
        let m = 0.5 * (lo + hi);
        let delta = round_up((hi - m).max(m - lo));
        let at = self.curve.jet_over::<Ivl, 7>(Ivl::from(m), Approach::Above);
        let over = self
            .curve
            .jet_over::<Ivl, 7>(Ivl::new(lo, hi), Approach::Above);
        let ds = Ivl::from(delta);
        // delta^k / k!, k = 0..=N
        let mut scale = [Ivl::from(1.0); N + 1];
        for k in 1..=N {
            let prev = scale.get(k - 1).copied().unwrap_or(Ivl::whole());
            if let Some(s) = scale.get_mut(k) {
                *s = prev * ds / Ivl::from(k as f64);
            }
        }
        let power = if N.is_multiple_of(2) {
            Ivl::new(0.0, 1.0)
        } else {
            Ivl::new(-1.0, 1.0)
        };
        let p0 = Vec3::new(at.point.x, at.point.y, at.point.z);
        // derivative k of C at m (k = 0 the point), and over the piece
        let der = |k: usize| -> Option<Vec3<Ivl>> {
            if k == 0 {
                Some(p0)
            } else {
                at.d.get(k - 1).copied()
            }
        };
        let der_over = |k: usize| over.d.get(k - 1).copied();
        let model = |shift: usize| -> Option<Vec3<Tm>> {
            let mut cs: [Vec3<Ivl>; N] =
                [Vec3::new(Ivl::from(0.0), Ivl::from(0.0), Ivl::from(0.0)); N];
            for (k, c) in cs.iter_mut().enumerate() {
                let s = scale.get(k).copied()?;
                *c = der(k + shift)? * s;
            }
            let r = der_over(N + shift)? * (scale.get(N).copied()? * power);
            let axis = |f: fn(&Vec3<Ivl>) -> Ivl, r: Ivl| {
                let coeffs: [Ivl; N] = cs.map(|v| f(&v));
                Tm::from_poly(&coeffs, r)
            };
            Some(Vec3::new(
                axis(|v| v.x, r.x),
                axis(|v| v.y, r.y),
                axis(|v| v.z, r.z),
            ))
        };
        let c = model(0)?;
        let dc = model(1)?;
        let finite = |v: &Vec3<Tm>| {
            [v.x, v.y, v.z]
                .iter()
                .all(|t| t.rem.is_finite() && t.c.iter().all(|x| x.is_finite()))
        };
        (finite(&c) && finite(&dc)).then_some((c, dc))
    }

    /// The curve's point and derivative at `t`, as brackets.
    pub(crate) fn point(&self, t: f64) -> (Vec3<Ivl>, Vec3<Ivl>) {
        let j = self.curve.jet_over::<Ivl, 1>(Ivl::from(t), Approach::Above);
        let [d] = j.d;
        (Vec3::new(j.point.x, j.point.y, j.point.z), d)
    }
}

// ---- what the queries measure ---------------------------------------------------------------

/// The two faces' oriented normals at the curve's point `p`.
pub(crate) fn normals<T: Num>(f: &[Field; 2], p: Vec3<T>) -> Option<[Vec3<T>; 2]> {
    let [a, b] = f;
    let na = a.foot(p)?.n0 * T::num(a.sign);
    let nb = b.foot(p)?.n0 * T::num(b.sign);
    Some([na, nb])
}

/// `|na - nb|^2` and `|na + nb|^2`: the angle `phi` between the normals is
/// `2 asin(sqrt(D) / 2) = 2 acos(sqrt(S) / 2)`, the first well conditioned near 0 and the
/// second near pi.
pub(crate) fn chord<T: Num>(n: [Vec3<T>; 2]) -> (T, T) {
    let [a, b] = n;
    let (d, s) = (a - b, a + b);
    (d.dot(d), s.dot(s))
}

/// The squared distance between the two feet of `p`.
pub(crate) fn gap2<T: Num>(f: &[Field; 2], p: Vec3<T>) -> Option<T> {
    let [a, b] = f;
    let (fa, fb) = (a.foot(p)?, b.foot(p)?);
    let g = fb.n0 * fb.delta - fa.n0 * fa.delta;
    Some(g.dot(g))
}

/// Face a's section values minus face b's, each across its own `w = n x T`.
pub(crate) fn jumps<T: Num>(f: &[Field; 2], p: Vec3<T>, dp: Vec3<T>) -> Option<[T; 3]> {
    let tangent = unit(dp)?;
    let side = |field: &Field| -> Option<[T; 3]> {
        let ft = field.foot(p)?;
        let n = ft.n0 * T::num(field.sign);
        let foot = p - ft.n0 * ft.delta;
        let w = unit(n.cross(tangent))?;
        field.bends(foot, n, w)
    };
    let [a, b] = f;
    let [ka, ka1, ka2] = side(a)?;
    let [kb, kb1, kb2] = side(b)?;
    Some([ka - kb, ka1 - kb1, ka2 - kb2])
}

/// How far [`angle_of`] moves its bracket out: the inverse functions' and the square root's
/// rounding, a few ulps of an angle up to pi, with a wide margin.
const PAD: f64 = 1e-14;

/// The angle bracket from the ranges of `D` and `S` ([`chord`]): each read through its
/// inverse, intersected, and moved out by the inverse functions' rounding.
pub(crate) fn angle_of(d: Ivl, s: Ivl) -> Ivl {
    let half = |x: f64| (0.25 * x).clamp(0.0, 1.0).sqrt();
    let (a_lo, a_hi) = (2.0 * fmath::asin(half(d.lo)), 2.0 * fmath::asin(half(d.hi)));
    let (b_lo, b_hi) = (2.0 * fmath::acos(half(s.hi)), 2.0 * fmath::acos(half(s.lo)));
    let (lo, hi) = (a_lo.max(b_lo), a_hi.min(b_hi));
    // the two readings meet unless rounding put them apart: then take both
    let (lo, hi) = if lo <= hi {
        (lo, hi)
    } else {
        (a_lo.min(b_lo), a_hi.max(b_hi))
    };
    Ivl::new((lo - PAD).max(0.0), (hi + PAD).min(core::f64::consts::PI))
}

/// The thresholds `(2 sin(theta / 2))^2` and `(2 cos(theta / 2))^2` an angle `theta` sets on
/// `D` and `S`, as brackets.
pub(crate) fn thresholds(theta: f64) -> (Ivl, Ivl) {
    let half = Ivl::from(theta) * Ivl::from(0.5);
    let (s, c) = half.sin_cos();
    let two = Ivl::from(2.0);
    ((two * s) * (two * s), (two * c) * (two * c))
}

// ---- the proof over the pieces --------------------------------------------------------------

/// What one piece's enclosure says.
pub(crate) enum OnPiece {
    /// The claim holds over the whole piece; the range of the measured quantity.
    Holds(Ivl),
    /// It cannot tell.
    Open,
}

/// What a bracket at one parameter says.
pub(crate) enum AtPoint {
    /// The claim holds there.
    Holds,
    /// The claim fails there, certified: the quantity's bracket.
    Fails(Ivl),
    /// The bracket straddles the claim's threshold.
    Open(Ivl),
}

/// How the proof over an edge ended.
pub(crate) enum Proof {
    /// Proved over every piece: the hull of the quantity's ranges.
    Holds(Ivl),
    /// A certified witness: the parameter and the quantity's bracket there.
    Fails(f64, Ivl),
    /// Neither: the parameter where the bracket straddled the threshold (or a piece could no
    /// longer be split), and the bracket.
    Open(f64, Ivl),
}

/// Runs the claim over `pieces`, depth first and left to right: a piece whose enclosure
/// decides is done; otherwise its middle is bracketed, and a failing bracket ends the proof
/// with a witness, a straddling one marks the place undecided (no piece holding it can
/// decide), and a holding one splits the piece in two. A piece too short to split is
/// undecided at its middle. Each enclosure and each bracket charges one unit to `budget`.
///
/// The ranking among undecided places is by the bracket's upper end, then by parameter, so
/// the same edge gives the same place.
pub(crate) fn prove(
    pieces: &[(f64, f64)],
    budget: &mut Budget,
    stop: &Stop,
    mut on_piece: impl FnMut(f64, f64) -> OnPiece,
    mut at_point: impl FnMut(f64) -> AtPoint,
) -> Result<Proof, BodyqError> {
    let mut stack: Vec<(f64, f64)> = pieces.iter().rev().copied().collect();
    let mut hull: Option<Ivl> = None;
    let mut open: Option<(f64, Ivl)> = None;
    let note = |t: f64, b: Ivl, open: &mut Option<(f64, Ivl)>| {
        if open.is_none_or(|(_, w)| b.hi > w.hi) {
            *open = Some((t, b));
        }
    };
    while let Some((lo, hi)) = stack.pop() {
        stop.check()?;
        budget.charge(1)?;
        if let OnPiece::Holds(r) = on_piece(lo, hi) {
            hull = Some(hull.map_or(r, |h| h.hull(r)));
            continue;
        }
        let m = 0.5 * (lo + hi);
        budget.charge(1)?;
        match at_point(m) {
            AtPoint::Fails(b) => return Ok(Proof::Fails(m, b)),
            AtPoint::Open(b) => note(m, b, &mut open),
            AtPoint::Holds if lo < m && m < hi => {
                stack.push((m, hi));
                stack.push((lo, m));
            }
            AtPoint::Holds => note(m, Ivl::whole(), &mut open),
        }
    }
    Ok(match (open, hull) {
        (Some((t, b)), _) => Proof::Open(t, b),
        (None, Some(h)) => Proof::Holds(h),
        (None, None) => Proof::Holds(Ivl::from(0.0)),
    })
}

// ---- the claims ------------------------------------------------------------------------------

/// What is claimed of the edge at every parameter, by order of contact.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Claim {
    /// The feet are closer than `sqrt` of the bound: `|gap|^2 < gap2`.
    Gap(Ivl),
    /// The normals are closer than an angle: the thresholds on `D` and `S` ([`thresholds`]).
    Angle(Ivl, Ivl),
    /// Section value `k` (0 curvature, 1 its rate, 2 its acceleration) jumps by less than
    /// the bound.
    Bend(usize, f64),
}

impl Claim {
    /// The quantities the claim reads at the curve's point `p` with derivative `dp`.
    fn measure<T: Num>(self, f: &[Field; 2], p: Vec3<T>, dp: Vec3<T>) -> Option<[T; 2]> {
        let zero = T::num(0.0);
        match self {
            Self::Gap(_) => Some([gap2(f, p)?, zero]),
            Self::Angle(..) => {
                let (d, s) = chord(normals(f, p)?);
                Some([d, s])
            }
            Self::Bend(k, _) => Some([*jumps(f, p, dp)?.get(k)?, zero]),
        }
    }

    /// What the quantities' ranges say: holds, fails (certified), or open.
    fn judge(self, [a, b]: [Ivl; 2]) -> AtPoint {
        let bracket = self.bracket([a, b]);
        match self {
            Self::Gap(t) if a.hi < t.lo => AtPoint::Holds,
            Self::Gap(t) if a.lo > t.hi => AtPoint::Fails(bracket),
            Self::Angle(td, ts) if a.hi < td.lo || b.lo > ts.hi => AtPoint::Holds,
            Self::Angle(td, ts) if a.lo > td.hi || b.hi < ts.lo => AtPoint::Fails(bracket),
            Self::Bend(_, t) if a.mag() < t => AtPoint::Holds,
            Self::Bend(_, t) if a.mig() > t => AtPoint::Fails(bracket),
            _ => AtPoint::Open(bracket),
        }
    }

    /// The measured drop as a bracket: the gap's length, the angle, or the jump's size.
    fn bracket(self, [a, b]: [Ivl; 2]) -> Ivl {
        match self {
            Self::Gap(_) => {
                let lo = a.lo.max(0.0).sqrt();
                let hi = a.hi.max(0.0).sqrt();
                Ivl::outward_ulp(lo, hi)
            }
            Self::Angle(..) => angle_of(a, b),
            Self::Bend(..) => Ivl::new(a.mig(), a.mag()),
        }
    }

    /// The claim's verdict at `t`.
    pub(crate) fn at(self, edge: &Edge<'_>, f: &[Field; 2], t: f64) -> AtPoint {
        let (p, dp) = edge.point(t);
        match self.measure(f, p, dp) {
            Some(q) => self.judge(q),
            None => AtPoint::Open(Ivl::whole()),
        }
    }

    /// The drop's bracket at `t` (whole where it has none).
    pub(crate) fn drop_at(self, edge: &Edge<'_>, f: &[Field; 2], t: f64) -> Ivl {
        let (p, dp) = edge.point(t);
        self.measure(f, p, dp)
            .map_or(Ivl::whole(), |q| self.bracket(q))
    }

    /// The claim's verdict over the piece `[lo, hi]`, from its Taylor models.
    pub(crate) fn over(self, edge: &Edge<'_>, f: &[Field; 2], lo: f64, hi: f64) -> OnPiece {
        let Some((p, dp)) = edge.piece(lo, hi) else {
            return OnPiece::Open;
        };
        let Some([a, b]) = self.measure(f, p, dp) else {
            return OnPiece::Open;
        };
        let q = [a.range(), b.range()];
        match self.judge(q) {
            AtPoint::Holds => OnPiece::Holds(self.bracket(q)),
            _ => OnPiece::Open,
        }
    }

    /// Runs the claim along the whole edge ([`prove`]).
    pub(crate) fn along(
        self,
        edge: &Edge<'_>,
        f: &[Field; 2],
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Proof, BodyqError> {
        prove(
            &edge.pieces(),
            budget,
            stop,
            |lo, hi| self.over(edge, f, lo, hi),
            |t| self.at(edge, f, t),
        )
    }
}

/// `x` as a tolerance: finite and not negative.
pub(crate) fn tolerance(x: f64) -> Result<f64, BodyqError> {
    if x.is_finite() && x >= 0.0 {
        Ok(x)
    } else {
        Err(BodyqError::Refused(BodyqRefusal::Tolerance))
    }
}

/// `theta` as an angle claim: finite, not negative and below pi.
pub(crate) fn angle_claim(theta: f64) -> Result<Claim, BodyqError> {
    let theta = tolerance(theta)?;
    if theta >= core::f64::consts::PI {
        return Err(BodyqError::Refused(BodyqRefusal::Tolerance));
    }
    let (d, s) = thresholds(theta);
    Ok(Claim::Angle(d, s))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    //! The section curvatures of single surfaces against their closed forms (the pin's
    //! in-module curvature tests).

    use scx_num::frame::{Frame, Sense};
    use scx_num::units::Length;
    use scx_surface::{Cylinder, Sphere, Surface, Torus};

    use super::*;

    /// The bends of `s` at the point `p` of it, across `w`, as midpoints, with the bracket
    /// widths.
    fn bends_at(s: &Surface, p: [f64; 3], w: [f64; 3]) -> ([f64; 3], f64) {
        let (field, sign) = Field::of(s).unwrap();
        let p = Vec3::new(Ivl::from(p[0]), Ivl::from(p[1]), Ivl::from(p[2]));
        let ft = field.foot(p).unwrap();
        let n = ft.n0 * Ivl::from(sign);
        let foot = p - ft.n0 * ft.delta;
        let w = Vec3::new(Ivl::from(w[0]), Ivl::from(w[1]), Ivl::from(w[2]));
        let b = field.bends(foot, n, w).unwrap();
        let width = b.iter().map(|x| x.hi - x.lo).fold(0.0, f64::max);
        (b.map(Ivl::mid), width)
    }

    fn close(got: f64, want: f64) {
        assert!((got - want).abs() < 1e-12, "{got} vs {want}");
    }

    #[test]
    fn a_sphere_bends_by_its_inverse_radius_both_ways() {
        let s = Surface::Sphere(
            Sphere::new(Frame::WORLD, Length::from_mm(2.0), Sense::Forward).unwrap(),
        );
        for w in [[0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
            let (b, width) = bends_at(&s, [2.0, 0.0, 0.0], w);
            close(b[0], -0.5);
            close(b[1], 0.0);
            close(b[2], 0.0);
            assert!(width < 1e-12);
        }
    }

    #[test]
    fn a_cylinder_bends_round_and_not_along() {
        let c = Surface::Cylinder(
            Cylinder::new(Frame::WORLD, Length::from_mm(3.0), Sense::Forward).unwrap(),
        );
        let (round, _) = bends_at(&c, [3.0, 0.0, 1.0], [0.0, 1.0, 0.0]);
        close(round[0], -1.0 / 3.0);
        close(round[1], 0.0);
        close(round[2], 0.0);
        let (along, _) = bends_at(&c, [3.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
        for x in along {
            close(x, 0.0);
        }
        // turned inside out, it bends the other way
        let inner = Surface::Cylinder(
            Cylinder::new(Frame::WORLD, Length::from_mm(3.0), Sense::Reversed).unwrap(),
        );
        let (round, _) = bends_at(&inner, [3.0, 0.0, 1.0], [0.0, 1.0, 0.0]);
        close(round[0], 1.0 / 3.0);
    }

    #[test]
    fn a_saddle_bends_both_ways() {
        // the torus's inner equator: round the tube one way, along the equator the other
        let t = Surface::Torus(
            Torus::new(
                Frame::WORLD,
                Length::from_mm(5.0),
                Length::from_mm(2.0),
                Sense::Forward,
            )
            .unwrap(),
        );
        let p = [3.0, 0.0, 0.0];
        let (tube, _) = bends_at(&t, p, [0.0, 0.0, 1.0]);
        close(tube[0], -0.5);
        let (equator, _) = bends_at(&t, p, [0.0, 1.0, 0.0]);
        close(equator[0], 1.0 / 3.0);
        // a circle's section has no rate of curvature
        close(equator[1], 0.0);
        close(tube[1], 0.0);
        close(tube[2], 0.0);
        close(equator[2], 0.0);
    }
}
