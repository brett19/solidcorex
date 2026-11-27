// Written for solidcorex. Apache-2.0.
//! Which side of a face the other body's material is on, where a face of the other body
//! meets it (research 72 §3.5 BQ2, §14.27): the Boolean's seeds (BO4).
//!
//! **Along a curve** ([`sides_along`]). Face `f` and the other body's face `g` meet along a
//! piece of their intersection. At a point `p` of it, with the curve's direction `T`, the
//! face runs off the curve along `w = n_f x T` (its *left*, seen from its front) and `-w`
//! (its *right*). Walking a distance `s` along `f` in direction `w`, the point's signed
//! height over `g` (along `g`'s natural normal `n_g`, positive in front) is, to second
//! order,
//!
//! ```text
//! d(s) = a s + b s^2,   a = w . n_g,
//!                       b = (II_f(w, w) (n_f . n_g) - II_g(w_g, w_g)) / 2,
//! ```
//!
//! where `II` is each surface's second fundamental form about its natural normal and `w_g`
//! is `w` projected into `g`'s tangent plane. The side of `g`, and so the other body's
//! region, on the left is the sign of `d` for small positive `s`, and on the right for
//! small negative `s`:
//!
//! - **A crossing** ([`Contact::Transversal`]): the sign of `a` on the left, the other on the
//!   right. A sample decides when the surfaces part by more than coincidence before the
//!   model meets `g` again, `a^2 > 4 |b| c` (SS4's depth rule, read locally), and `|a|` is
//!   past what the point's distance from either surface can turn the normals by.
//! - **A tangency** ([`Contact::Tangent`], SS4's one tangent piece where the surfaces part by
//!   no more than the tolerance): the sign of `b` on both sides. A sample decides when `b`
//!   is clear of rounding against the two curvatures; when it is not, the surfaces agree to
//!   second order across the curve and no side is read.
//!
//! Samples are taken at the span's middle, then its quarters, then its eighths (at most 7,
//! as BO2's); the first that decides answers. None deciding is
//! [`BodyqUnsupported::NoClearSide`], so the caller falls back to `classify_point`.
//!
//! **At a touch point** ([`side_at_touch`]): an isolated point where the faces touch. Both
//! normals are parallel there; the face lies on one side of `g` round the point when the
//! quadratic form `d(t) = (II_f(t, t) (n_f . n_g) - II_g(t, t)) / 2` over `f`'s tangent
//! plane is definite, and on that side.

use scx_brep::{Face, FaceSide, RegionId, Stage};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::domain::ParamDomain;
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_ssi::analytic::Contact;
use scx_surface::Surface;

use crate::error::{BodyqDefect, BodyqError, BodyqRefusal, BodyqUnsupported};
use crate::geom::ABOVE;

/// Where two faces of two bodies meet along a curve: a piece of their intersection, as
/// stage 4 of the Boolean keeps it (`scx-boolean`'s `Section`).
#[derive(Debug, Clone, Copy)]
pub struct Along<'a> {
    /// The curve, lying on both faces within `coincidence`.
    pub curve: &'a Curve,
    /// The piece's span of the curve's parameter, inside both faces.
    pub span: Extent1d,
    /// How the surfaces meet along it, as `scx_ssi::analytic::intersect` decided.
    pub contact: Contact,
    /// The pair's coincidence: `coincident(resolution, accuracy of f, accuracy of g)`, which
    /// the caller has per pair (a face's accuracy is derived in time linear in its uses).
    pub coincidence: Length,
}

/// The other body's region on each side of a face, along a curve.
///
/// *Left* is the side the face runs off the curve along `n x T`, where `n` is the face's
/// natural normal (towards its [`FaceSide::Front`]) and `T` the curve's direction of
/// increasing parameter: the face lies to the left of a use running along `T` seen from its
/// front, as a loop's uses run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sides {
    /// The other body's region the face lies in on its left.
    pub left: RegionId,
    /// The other body's region the face lies in on its right.
    pub right: RegionId,
}

/// The sample fractions of a span, in order: the middle, the quarters, the eighths (BO2's).
const SAMPLES: [f64; 7] = [0.5, 0.25, 0.75, 0.125, 0.375, 0.625, 0.875];

/// How far below the two curvatures `b` must stay to be rounding: a relative bound well
/// above the jets' few ulps.
const ROUNDING: f64 = 1e-9;

/// The other body's region on each side of `face` along `along`, where `other` (a face of
/// the other body) meets it (see the module's note for the method). `face` may be a
/// draft's face (only its surface and uv box are read); `other` is a finished operand's.
///
/// Two units of work per sample (one inversion onto each surface); `stop` is checked before
/// each. The same inputs give the same answer and work.
///
/// # Errors
///
/// - [`BodyqError::Unsupported`] with [`BodyqUnsupported::NoClearSide`] when no sample
///   decides: a crossing too shallow at every sample, surfaces agreeing to second order
///   across the curve, or no normal or tangent at any sample.
/// - [`BodyqError::Exhausted`] and [`BodyqError::Cancelled`] from `budget` and `stop`.
/// - [`BodyqError::Defect`] when a face has no uv box or a NURBS surface's inversion fails
///   inside.
pub fn sides_along<S: Stage>(
    face: Face<'_, S>,
    other: Face<'_>,
    along: &Along<'_>,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Sides, BodyqError> {
    let f = Sheet::of(face)?;
    let g = Sheet::of(other)?;
    match sides_on(&f, &g, along, budget, stop)? {
        Some([left, right]) => Ok(Sides {
            left: other.shell(left).region().id(),
            right: other.shell(right).region().id(),
        }),
        None => Err(no_clear_side(face, other)),
    }
}

/// The other body's region that `face` lies in round `at`, an isolated point where `other`
/// (a face of the other body) touches it (see the module's note for the method).
///
/// Two units of work (one inversion onto each surface).
///
/// # Errors
///
/// - [`BodyqError::Refused`] for a point with a coordinate that is not finite.
/// - [`BodyqError::Unsupported`] with [`BodyqUnsupported::NoClearSide`] when the face does
///   not lie on one side round the point: the form is indefinite (the surfaces cross there)
///   or within rounding of singular, or a surface has no normal there.
/// - [`BodyqError::Exhausted`], [`BodyqError::Cancelled`] and [`BodyqError::Defect`] as
///   [`sides_along`].
pub fn side_at_touch<S: Stage>(
    face: Face<'_, S>,
    other: Face<'_>,
    at: Point3,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<RegionId, BodyqError> {
    if !(at.x.is_finite() && at.y.is_finite() && at.z.is_finite()) {
        return Err(BodyqError::Refused(BodyqRefusal::NotFinite));
    }
    let f = Sheet::of(face)?;
    let g = Sheet::of(other)?;
    match side_at(&f, &g, at, budget, stop)? {
        Some(side) => Ok(other.shell(side).region().id()),
        None => Err(no_clear_side(face, other)),
    }
}

fn no_clear_side<S: Stage>(face: Face<'_, S>, other: Face<'_>) -> BodyqError {
    BodyqError::Unsupported(BodyqUnsupported::NoClearSide {
        face: face.id(),
        other: other.id(),
    })
}

/// A surface and the uv window its feet are placed in (a face's uv box).
pub(crate) struct Sheet<'a> {
    pub(crate) surface: &'a Surface,
    pub(crate) window: Option<Extent2d>,
    /// The face, for errors.
    pub(crate) face: scx_brep::FaceId,
}

impl<'a> Sheet<'a> {
    fn of<S: Stage>(face: Face<'a, S>) -> Result<Self, BodyqError> {
        let window = face
            .uv_box()
            .ok_or_else(|| BodyqError::Defect(BodyqDefect::NoUvBox { face: face.id() }))?;
        Ok(Self {
            surface: face.surface(),
            window: Some(window),
            face: face.id(),
        })
    }

    /// The surface's local shape at its foot of `p`: one unit of work. `None` where it has
    /// no normal, or its partials do not span the tangent plane (a pole).
    fn local(
        &self,
        p: Point3,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Option<Local>, BodyqError> {
        budget.charge(1)?;
        let foot = self
            .surface
            .invert(p, None, self.window.as_ref(), budget, stop)
            .map_err(|e| BodyqError::of_surface(e, self.face))?
            .into_foot();
        Ok(Local::at(self.surface, foot.at))
    }
}

/// A surface's normal and second fundamental form at a point.
#[derive(Debug, Clone, Copy)]
struct Local {
    /// The natural unit normal.
    n: Vec3,
    su: Vec3,
    sv: Vec3,
    /// `S_uu . n`, `S_uv . n`, `S_vv . n`.
    l: f64,
    m: f64,
    nn: f64,
    /// The first fundamental form `E`, `F`, `G` and its determinant.
    e: f64,
    f: f64,
    g: f64,
    det: f64,
}

impl Local {
    /// The shape at `uv`. Where the partials do not span the tangent plane (a sphere's pole,
    /// a NURBS surface's collapsed edge), the form is read a step [`NUDGE`] into the domain
    /// (its own normal there, which the curvature of the small `u` circle needs), with the
    /// normal at `uv` kept: exact on a sphere (every direction is a principal one), and off
    /// by the step times the curvature's change elsewhere.
    fn at(s: &Surface, uv: Point2) -> Option<Self> {
        let n = s.normal(uv, ABOVE)?.as_vec();
        Self::form(s, uv).or_else(|| {
            let shape = Self::form(s, inward(s, uv))?;
            Some(Self { n, ..shape })
        })
    }

    fn form(s: &Surface, uv: Point2) -> Option<Self> {
        let n = s.normal(uv, ABOVE)?.as_vec();
        let j = s.jet::<2>(uv, ABOVE);
        let (su, sv) = (j.get(1, 0)?, j.get(0, 1)?);
        let (suu, suv, svv) = (j.get(2, 0)?, j.get(1, 1)?, j.get(0, 2)?);
        let (e, f, g) = (su.dot(su), su.dot(sv), sv.dot(sv));
        let det = e * g - f * f;
        // Partials too near parallel to solve for a tangent's coordinates.
        if det.is_nan() || det <= 1e-24 * (e * g) || !det.is_finite() {
            return None;
        }
        Some(Self {
            n,
            su,
            sv,
            l: suu.dot(n),
            m: suv.dot(n),
            nn: svv.dot(n),
            e,
            f,
            g,
            det,
        })
    }

    /// `II(t, t)` for a tangent vector `t` (its part along the normal is ignored).
    fn second(&self, t: Vec3) -> f64 {
        let (tu, tv) = (t.dot(self.su), t.dot(self.sv));
        let alpha = (self.g * tu - self.f * tv) / self.det;
        let beta = (self.e * tv - self.f * tu) / self.det;
        alpha * alpha * self.l + 2.0 * alpha * beta * self.m + beta * beta * self.nn
    }
}

/// How far into the domain a pole's form is read, in uv (a quarter turn per unit on the
/// analytic kinds, D13): `1e-5` keeps the solve's rounding (about `1e-16 / NUDGE`) well
/// below [`ROUNDING`].
const NUDGE: f64 = 1e-5;

/// `uv` stepped [`NUDGE`] towards the middle of each bounded direction of the surface's
/// domain, and up in each other direction.
fn inward(s: &Surface, uv: Point2) -> Point2 {
    let step = |d: ParamDomain, x: f64| match d {
        ParamDomain::Bounded(e) if x > e.mid() => x - NUDGE,
        _ => x + NUDGE,
    };
    let d = s.domain();
    Point2::new(step(d.u, uv.x), step(d.v, uv.y))
}

/// The side of `g` in front of a positive height.
const fn side_of(positive: bool) -> FaceSide {
    if positive {
        FaceSide::Front
    } else {
        FaceSide::Back
    }
}

/// The side of `g` that `f` lies on, left then right, along `along`; `None` when no sample
/// decides.
pub(crate) fn sides_on(
    f: &Sheet<'_>,
    g: &Sheet<'_>,
    along: &Along<'_>,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Option<[FaceSide; 2]>, BodyqError> {
    let c = along.coincidence.mm().max(0.0);
    let (t0, len) = (along.span.min(), along.span.length());
    for k in SAMPLES {
        stop.check()?;
        let t = t0 + len * k;
        let jet = along.curve.jet::<1>(t, Approach::Above);
        let p = jet.point;
        let [dir] = jet.d;
        let (Some(lf), Some(lg)) = (f.local(p, budget, stop)?, g.local(p, budget, stop)?) else {
            continue;
        };
        let Some(w) = lf.n.cross(dir).normalized() else {
            continue;
        };
        let w = w.as_vec();
        let a = w.dot(lg.n);
        let kf = lf.second(w);
        let wg = w - lg.n * a;
        let kg = lg.second(wg);
        let b = 0.5 * (kf * lf.n.dot(lg.n) - kg);
        let curvature = kf.abs() + kg.abs();
        match along.contact {
            Contact::Transversal => {
                // The point lies within `c` of both surfaces, which turns each normal by up
                // to its curvature times `c`.
                let clear = a.abs() > curvature * c && a * a > 4.0 * b.abs() * c;
                if clear {
                    let left = side_of(a > 0.0);
                    return Ok(Some([left, left.other()]));
                }
            }
            Contact::Tangent => {
                if b.abs() > ROUNDING * curvature {
                    let both = side_of(b > 0.0);
                    return Ok(Some([both, both]));
                }
            }
        }
    }
    Ok(None)
}

/// The side of `g` that `f` lies on round the touch point `at`; `None` when it does not lie
/// on one side clearly.
pub(crate) fn side_at(
    f: &Sheet<'_>,
    g: &Sheet<'_>,
    at: Point3,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Option<FaceSide>, BodyqError> {
    stop.check()?;
    let (Some(lf), Some(lg)) = (f.local(at, budget, stop)?, g.local(at, budget, stop)?) else {
        return Ok(None);
    };
    // An orthonormal basis of f's tangent plane.
    let Some(e1) = lf.su.normalized() else {
        return Ok(None);
    };
    let e1 = e1.as_vec();
    let e2 = lf.n.cross(e1);
    let dot = lf.n.dot(lg.n);
    let q = |t: Vec3| {
        let tg = t - lg.n * t.dot(lg.n);
        let (kf, kg) = (lf.second(t), lg.second(tg));
        (0.5 * (kf * dot - kg), kf.abs() + kg.abs())
    };
    let (q11, s1) = q(e1);
    let (q22, s2) = q(e2);
    let (qd, _) = q(e1 + e2);
    // The polarisation of the form: q(e1 + e2) = q11 + 2 q12 + q22.
    let q12 = 0.5 * (qd - q11 - q22);
    // The eigenvalues of [[q11, q12], [q12, q22]].
    let mean = 0.5 * (q11 + q22);
    let half = (0.25 * (q11 - q22) * (q11 - q22) + q12 * q12).sqrt();
    let (lo, hi) = (mean - half, mean + half);
    let floor = ROUNDING * (s1 + s2);
    if lo > floor {
        Ok(Some(FaceSide::Front))
    } else if hi < -floor {
        Ok(Some(FaceSide::Back))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
impl<'a> Sheet<'a> {
    /// A whole surface, for the tests.
    pub(crate) const fn whole(surface: &'a Surface) -> Self {
        Self {
            surface,
            window: None,
            face: scx_brep::FaceId::from_raw(0),
        }
    }
}

#[cfg(test)]
#[path = "side_tests.rs"]
mod tests;
