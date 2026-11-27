// Written for solidcorex. Apache-2.0.
//! [`Surface`]: every surface kind as one value, dispatched by `match` (63 §4.3, 01 C10;
//! an enum, not a trait object, as `scx_curve::Curve` is, 63 A7).
//!
//! Each method is the kind's own call under one `match`, so a dispatched result has the
//! kind's bits. [`Surface::eval_many`] and [`Surface::eval_grid`] match once per batch,
//! outside the loop.
//!
//! The `Nurbs` arm holds its [`NurbsSurface`] behind an `Arc` (63 A7): cloning a surface is
//! a count, and the enum keeps the size of its largest analytic arm. Its calls are the
//! surface's own: [`NurbsSurface::point`], [`NurbsSurface::eval_many`] and
//! [`NurbsSurface::eval_grid`] on the pieces above `uv`, and [`NurbsSurface::jet`] on the
//! pieces `side` picks. These calls cannot fail here, so where the surface's call errs (a parameter that is not finite, a value that overflows,
//! a jet past [`scx_nurbs::MAX_ORDER`]) every coordinate is NaN, as an analytic kind's are at
//! an infinite parameter.
//!
//! The scalar-generic evaluation (each kind's `eval` over a [`Ring`](scx_num::ring::Ring))
//! is not on the enum yet: a NURBS surface has none, and an enum method that refused one arm
//! would change its signature when it arrives (63 §4.7).
//!
//! The enum is `#[non_exhaustive]` (01 C10): `Procedural` and the swept kinds join later,
//! so a match outside this crate keeps a wildcard arm. It is not `Copy`: the `Nurbs` arm
//! holds an `Arc`.

use std::sync::Arc;

use scx_curve::{Chart, ChartKind};
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Sense;
use scx_num::domain::{ParamDomain, UvDomain};
use scx_num::extent2d::Extent2d;
use scx_num::jet::SurfaceJet;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_nurbs::Approach;
use scx_nurbs_surface::NurbsSurface;

use crate::common::check_lengths;
use crate::error::SurfaceError;
use crate::invert::Inversion;
use crate::nurbs_invert;
use crate::to_nurbs::restrict;
use crate::{Cone, Cylinder, Plane, Sphere, Torus};

/// A surface in space: one of the kinds.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Surface {
    /// A plane, over every `uv`.
    Plane(Plane),
    /// A cylinder.
    Cylinder(Cylinder),
    /// A cone.
    Cone(Cone),
    /// A sphere.
    Sphere(Sphere),
    /// A torus.
    Torus(Torus),
    /// A NURBS surface, over its knots' domain and continuing its end pieces past it.
    Nurbs(Arc<NurbsSurface<3>>),
}

// The largest arm (a cone or torus: a frame, two sizes and a sense) plus the tag (63 §4.3);
// an arm that would grow past this goes behind an `Arc` (63 A7), as `Nurbs` does.
const _: () = assert!(size_of::<Surface>() <= 128);

/// Every method once: each analytic arm calls the kind's method of the same name, the
/// `Nurbs` arm the second expression.
macro_rules! dispatch {
    ($self:expr, $k:ident => $call:expr, $n:ident => $nurbs:expr) => {
        match $self {
            Self::Plane($k) => $call,
            Self::Cylinder($k) => $call,
            Self::Cone($k) => $call,
            Self::Sphere($k) => $call,
            Self::Torus($k) => $call,
            Self::Nurbs($n) => $nurbs,
        }
    };
}

/// The pieces above `uv` in both directions, for the calls that take no side.
const ABOVE: [Approach; 2] = [Approach::Above, Approach::Above];

const NAN3: Point3 = Point3::new(f64::NAN, f64::NAN, f64::NAN);

#[inline]
fn point3([x, y, z]: [f64; 3]) -> Point3 {
    Point3::new(x, y, z)
}

#[inline]
fn vec3([x, y, z]: [f64; 3]) -> Vec3 {
    Vec3::new(x, y, z)
}

/// A NURBS surface's point on the pieces above `uv`, NaN where it errs.
///
/// The `Nurbs` helpers are never inlined: their bodies are large, and inlined beside the
/// analytic arms the match's frame and spills cost every arm (63 §4.7: a plane's point
/// took 13 ns through the enum against 5 ns direct while they were inlined). A NURBS
/// evaluation is over 100 ns, so the call is noise there.
#[inline(never)]
fn nurbs_point(n: &NurbsSurface<3>, uv: Point2) -> Point3 {
    n.point(uv.to_array(), ABOVE).map_or(NAN3, point3)
}

/// A NURBS surface's partials to total order `D` on the pieces `side` picks, NaN where it
/// errs.
#[inline(never)]
fn nurbs_jet<const D: usize>(
    n: &NurbsSurface<3>,
    uv: Point2,
    side: [Approach; 2],
) -> SurfaceJet<Vec3, D> {
    match n.jet::<D>(uv.to_array(), side) {
        Ok(j) => j.map(vec3),
        Err(_) => SurfaceJet::from_fn(|_, _| Vec3::splat(f64::NAN)),
    }
}

/// A NURBS surface's unit `S_u x S_v` on the pieces `side` picks; `None` where it errs or
/// the cross product is too short to point (a collapsed edge, a cusp).
#[inline(never)]
fn nurbs_normal(n: &NurbsSurface<3>, uv: Point2, side: [Approach; 2]) -> Option<UnitVec3> {
    let j = n.jet::<1>(uv.to_array(), side).ok()?;
    let (su, sv) = (vec3(j.get(1, 0)?), vec3(j.get(0, 1)?));
    su.cross(sv).normalized()
}

impl Surface {
    /// The closed-form inverse of the kind's uv (63 D13), as a [`Chart`]: `None` for a
    /// NURBS surface, which has none.
    #[must_use]
    pub fn chart(&self) -> Option<Chart> {
        let (frame, sense, kind) = match self {
            Self::Plane(p) => (p.frame(), Sense::Forward, ChartKind::Plane),
            Self::Cylinder(c) => (
                c.frame(),
                c.sense(),
                ChartKind::Cylinder {
                    radius: c.radius(),
                },
            ),
            Self::Cone(c) => (c.frame(), c.sense(), ChartKind::Cone),
            Self::Sphere(s) => (s.frame(), s.sense(), ChartKind::Sphere),
            Self::Torus(t) => (t.frame(), t.sense(), ChartKind::Torus { major: t.major() }),
            Self::Nurbs(_) => return None,
        };
        // Every surface's sizes passed the same checks the chart makes.
        Chart::new(*frame, sense, kind).ok()
    }

    /// The parameter's domain: the kind's, and [`ParamDomain::Bounded`] by its knots in
    /// each direction for a NURBS surface.
    #[inline]
    #[must_use]
    pub fn domain(&self) -> UvDomain {
        dispatch!(self, k => k.domain(), n => {
            let e = n.domain();
            UvDomain::new(
                ParamDomain::Bounded(e.x_interval()),
                ParamDomain::Bounded(e.y_interval()),
            )
        })
    }

    /// The exact NURBS image over `over`, in the same uv (63 §4.8): the analytic kind's
    /// `to_nurbs`, and for a NURBS surface its restriction to `over` (split at each end of
    /// `over` inside its domain; the surface itself, cloned, where `over` is its domain).
    ///
    /// # Errors
    ///
    /// The kind's (`EmptyExtent`, `OutsideDomain`, `Nurbs` on overflow); for a NURBS
    /// surface, `EmptyExtent`, `OutsideDomain` past its knots' domain, and `Nurbs` with
    /// the split's error.
    pub fn to_nurbs(&self, over: &Extent2d) -> Result<NurbsSurface<3>, SurfaceError> {
        dispatch!(self, k => k.to_nurbs(over), n => restrict(n, over))
    }

    /// `S(u, v)` (on the pieces above `uv` for a NURBS surface, which is continuous).
    #[inline]
    #[must_use]
    pub fn point(&self, uv: Point2) -> Point3 {
        dispatch!(self, k => k.point(uv), n => nurbs_point(n, uv))
    }

    /// The partials `S^(i,j)` for every `i + j <= D` at `uv`, on the piece `side` picks in
    /// each direction where the surface is not smooth there (an analytic kind's arc break,
    /// a NURBS surface's knot).
    #[inline]
    #[must_use]
    pub fn jet<const D: usize>(&self, uv: Point2, side: [Approach; 2]) -> SurfaceJet<Vec3, D> {
        dispatch!(self, k => k.jet(uv, side), n => nurbs_jet(n, uv, side))
    }

    /// The unit normal along `S_u x S_v`, from the pieces `side` picks (a NURBS surface's
    /// normal may differ across a knot; every analytic kind's is the same on both sides).
    ///
    /// `None` where there is none: a cone's apex, and a NURBS surface's point where the
    /// partials err or their cross product is too short to point. The analytic kinds that
    /// have a normal everywhere always return `Some`.
    #[inline]
    #[must_use]
    pub fn normal(&self, uv: Point2, side: [Approach; 2]) -> Option<UnitVec3> {
        match self {
            Self::Plane(k) => Some(k.normal(uv)),
            Self::Cylinder(k) => Some(k.normal(uv)),
            Self::Cone(k) => k.normal(uv),
            Self::Sphere(k) => Some(k.normal(uv)),
            Self::Torus(k) => Some(k.normal(uv)),
            Self::Nurbs(n) => nurbs_normal(n, uv, side),
        }
    }

    /// The point at each of `uv`, into `out`: `out[i]` is [`Surface::point`]`(uv[i])`, bit
    /// for bit (a NaN is a NaN; Rust leaves its sign and payload open). The kind is matched
    /// once; an analytic kind reads its frame once, and a NURBS surface reuses its basis
    /// along a row or column ([`NurbsSurface::eval_many`]).
    ///
    /// # Panics
    ///
    /// If `uv` and `out` differ in length.
    pub fn eval_many(&self, uv: &[Point2], out: &mut [Point3]) {
        check_lengths(uv, out);
        match self {
            Self::Plane(k) => k.eval_many(uv, out),
            Self::Cylinder(k) => k.eval_many(uv, out),
            Self::Cone(k) => k.eval_many(uv, out),
            Self::Sphere(k) => k.eval_many(uv, out),
            Self::Torus(k) => k.eval_many(uv, out),
            Self::Nurbs(n) => nurbs_many(n, uv, out),
        }
    }

    /// The points of the grid `us × vs`, into `out` row by row: `out[i * vs.len() + j]` is
    /// [`Surface::point`]`((us[i], vs[j]))`, bit for bit. A NURBS surface computes each
    /// basis once ([`NurbsSurface::eval_grid`]); an analytic kind evaluates each point.
    ///
    /// # Panics
    ///
    /// If `out.len()` is not `us.len() * vs.len()`.
    pub fn eval_grid(&self, us: &[f64], vs: &[f64], out: &mut [Point3]) {
        assert_eq!(
            us.len().checked_mul(vs.len()),
            Some(out.len()),
            "eval_grid: out is not us.len() * vs.len() long"
        );
        dispatch!(
            self,
            k => grid_by_point(us, vs, out, |uv| k.point(uv)),
            n => nurbs_grid(n, us, vs, out)
        );
    }

    /// The nearest point of the surface to `p`, placed against `domain` (`None`: the
    /// surface's own), as [`Inversion::Inside`] or [`Inversion::Outside`] it; the foot is the
    /// nearest point over the whole surface either way (63 §4.2, X8).
    ///
    /// An analytic kind's foot is its closed form (each kind's `invert`), which takes no
    /// budget and cannot fail. A NURBS surface's is a certified search over its Bezier
    /// patches ([`crate::nurbs_invert`]): the global nearest to a gap of `1e-12` of the
    /// control net's size, charged to `budget` and polled against `stop`; `near` is a warm
    /// start for it (the analytic arms ignore it), which changes only the work, not the
    /// foot's place beyond that gap.
    ///
    /// # Errors
    ///
    /// For a NURBS surface only: [`SurfaceError::Exhausted`] when `budget` runs out,
    /// [`SurfaceError::Stopped`] when `stop` is set, and [`SurfaceError::Defect`] where the
    /// search fails inside. None of these returns a partial foot.
    pub fn invert(
        &self,
        p: Point3,
        near: Option<Point2>,
        domain: Option<&Extent2d>,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Inversion<Point2>, SurfaceError> {
        dispatch!(
            self,
            k => Ok(k.invert(p, domain)),
            n => nurbs_invert::invert(n, p, near, domain, budget, stop)
        )
    }
}

/// How many points the `Nurbs` arm of [`Surface::eval_many`] passes at a time: its `uv`
/// and points go through buffers on the stack (`Point2` and `Point3` have no layout promise,
/// and `unsafe` is forbidden), and a basis is recomputed only at a chunk's first point.
const CHUNK: usize = 64;

/// [`NurbsSurface::eval_many`] into `out`, a chunk at a time. A chunk errs at its first bad
/// point, so then each of its points is taken alone, NaN only where its own call errs.
#[inline(never)]
fn nurbs_many(n: &NurbsSurface<3>, uv: &[Point2], out: &mut [Point3]) {
    let mut at = [[0.0; 2]; CHUNK];
    let mut ps = [[0.0; 3]; CHUNK];
    for (uv, out) in uv.chunks(CHUNK).zip(out.chunks_mut(CHUNK)) {
        // Both buffers hold `CHUNK`, so the prefixes always exist.
        let bufs = at.get_mut(..uv.len()).zip(ps.get_mut(..uv.len()));
        let done = bufs.is_some_and(|(at, ps)| {
            for (a, p) in at.iter_mut().zip(uv) {
                *a = p.to_array();
            }
            let ok = n.eval_many(at, ABOVE, ps).is_ok();
            if ok {
                for (o, &p) in out.iter_mut().zip(ps.iter()) {
                    *o = point3(p);
                }
            }
            ok
        });
        if !done {
            for (o, &p) in out.iter_mut().zip(uv) {
                *o = nurbs_point(n, p);
            }
        }
    }
}

/// [`NurbsSurface::eval_grid`] into `out`, through one buffer of the grid's size; where the
/// grid errs, each point is taken alone, NaN only where its own call errs.
#[inline(never)]
fn nurbs_grid(n: &NurbsSurface<3>, us: &[f64], vs: &[f64], out: &mut [Point3]) {
    let mut ps = vec![[0.0; 3]; out.len()];
    if n.eval_grid(us, vs, ABOVE, &mut ps).is_ok() {
        for (o, p) in out.iter_mut().zip(ps) {
            *o = point3(p);
        }
    } else {
        grid_by_point(us, vs, out, |uv| nurbs_point(n, uv));
    }
}

/// `out[i * vs.len() + j] = f((us[i], vs[j]))`.
#[inline]
fn grid_by_point(us: &[f64], vs: &[f64], out: &mut [Point3], f: impl Fn(Point2) -> Point3) {
    for (&u, row) in us.iter().zip(out.chunks_mut(vs.len().max(1))) {
        for (o, &v) in row.iter_mut().zip(vs) {
            *o = f(Point2::new(u, v));
        }
    }
}

macro_rules! from_kind {
    ($($kind:ident),*) => {$(
        impl From<$kind> for Surface {
            fn from(k: $kind) -> Self {
                Self::$kind(k)
            }
        }
    )*};
}

from_kind!(Plane, Cylinder, Cone, Sphere, Torus);

impl From<Arc<NurbsSurface<3>>> for Surface {
    fn from(n: Arc<NurbsSurface<3>>) -> Self {
        Self::Nurbs(n)
    }
}

impl From<NurbsSurface<3>> for Surface {
    fn from(n: NurbsSurface<3>) -> Self {
        Self::Nurbs(Arc::new(n))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use scx_num::frame::{Frame, Sense};
    use scx_num::hash::Rng;
    use scx_num::units::Length;
    use scx_nurbs::KnotVector;
    use scx_nurbs_surface::{Net, ParamAxis};
    use scx_testkit::sample::{self, Knots, Spec, Weights};

    const SIDES: [[Approach; 2]; 4] = [
        [Approach::Below, Approach::Below],
        [Approach::Below, Approach::Above],
        [Approach::Above, Approach::Below],
        [Approach::Above, Approach::Above],
    ];

    fn signed(rng: &mut Rng, scale: f64) -> f64 {
        scale * rng.next_signed_unit()
    }

    fn sense(i: usize) -> Sense {
        if i.is_multiple_of(2) {
            Sense::Forward
        } else {
            Sense::Reversed
        }
    }

    /// 60 seeded placements at three scales and both senses, each as every analytic kind.
    pub(crate) fn analytic(rng: &mut Rng) -> Vec<Surface> {
        let mut out = Vec::new();
        for (i, scale) in [1.0, 1e-3, 1e4].into_iter().cycle().take(60).enumerate() {
            let p = Point3::from(core::array::from_fn(|_| signed(rng, scale)));
            let z = Vec3::from(core::array::from_fn(|_| signed(rng, 1.0)));
            let x = Vec3::from(core::array::from_fn(|_| signed(rng, 1.0)));
            let Some(f) = Frame::new(p, z, x) else {
                continue;
            };
            let r = scale * (0.25 + rng.next_f64());
            let (len, s) = (Length::from_mm, sense(i));
            let angle = 1.4 * signed(rng, 1.0);
            out.push(Plane::new(f).into());
            out.push(Cylinder::new(f, len(r), s).unwrap().into());
            if let Ok(c) = Cone::new(f, len(r * rng.next_f64()), angle, s) {
                out.push(c.into());
            }
            out.push(Sphere::new(f, len(r), s).unwrap().into());
            let minor = r * (0.05 + 0.9 * rng.next_f64());
            out.push(Torus::new(f, len(r), len(minor), s).unwrap().into());
        }
        out
    }

    /// NURBS surfaces of every knot and weight kind, degrees 1..=4 and 9, without the picks
    /// whose interior knot repeats past the degree, which are not surfaces.
    pub(crate) fn nurbs(rng: &mut Rng) -> Vec<Surface> {
        let mut out = Vec::new();
        for (pu, pv) in [(1, 1), (1, 3), (2, 2), (3, 4), (9, 2)] {
            for knots in [Knots::Uniform, Knots::Clustered, Knots::Multiple] {
                for weights in [Weights::Unit, Weights::Moderate, Weights::NearZero] {
                    let spec = |degree| Spec {
                        degree,
                        count: degree + 3,
                        knots,
                    };
                    let d = sample::surface::<3>(rng, (spec(pu), spec(pv)), weights);
                    let built = (|| {
                        let u = KnotVector::new(d.degrees.0, d.knots_u.clone()).ok()?;
                        let v = KnotVector::new(d.degrees.1, d.knots_v.clone()).ok()?;
                        let (rows, cols) = (u.point_count(), v.point_count());
                        let points = Net::new(rows, cols, d.points.clone()).ok()?;
                        let w = match d.weights.clone() {
                            Some(w) => Some(Net::new(rows, cols, w).ok()?),
                            None => None,
                        };
                        NurbsSurface::new(u, v, points, w).ok()
                    })();
                    out.extend(built.map(Surface::from));
                }
            }
        }
        assert!(out.len() > 30, "{}", out.len());
        out
    }

    fn surfaces(seed: u64) -> Vec<Surface> {
        let mut rng = Rng::new(seed);
        let mut out = analytic(&mut rng);
        out.extend(nurbs(&mut rng));
        out
    }

    /// Ordinary parameters, the analytic breaks and a NURBS surface's knots, both domain
    /// ends and past them, large values, signed zero, infinities and NaN, paired up.
    fn params(rng: &mut Rng, s: &Surface) -> Vec<Point2> {
        let mut t = vec![
            0.0,
            -0.0,
            0.25,
            0.5,
            1.0,
            -1.0,
            1e-300,
            1e6,
            -3.75e5,
            f64::INFINITY,
            f64::NAN,
        ];
        if let Surface::Nurbs(n) = s {
            t.extend(n.knots(ParamAxis::U).iter().take(6));
            t.extend(n.knots(ParamAxis::V).iter().rev().take(6));
        }
        while t.len() < 33 {
            t.push(1.5 * rng.next_signed_unit());
        }
        let mut uv: Vec<Point2> = t.iter().map(|&u| Point2::new(u, u)).collect();
        // Each `u` with another list entry's `v` (7 is prime to the length, 33 or more).
        for (i, &u) in t.iter().enumerate() {
            uv.push(Point2::new(u, t[(7 * i + 3) % t.len()]));
        }
        uv
    }

    /// The bits, with every NaN one pattern: Rust leaves a NaN's sign and payload
    /// unspecified (an optimised loop and a single call may differ), so no caller can key on
    /// them. Signed zeros stay apart.
    fn bits(x: f64) -> u64 {
        if x.is_nan() {
            f64::NAN.to_bits()
        } else {
            x.to_bits()
        }
    }

    fn bits3(p: [f64; 3]) -> [u64; 3] {
        p.map(bits)
    }

    fn jbits<const D: usize>(j: &SurfaceJet<Vec3, D>) -> Vec<[u64; 3]> {
        j.iter().map(|(_, v)| bits3(v.to_array())).collect()
    }

    fn nbits(n: Option<UnitVec3>) -> Option<[u64; 3]> {
        n.map(|n| bits3(n.as_vec().to_array()))
    }

    /// The kind's own calls, for comparison with the dispatch.
    struct Direct {
        domain: UvDomain,
        point: Point3,
        jet: SurfaceJet<Vec3, 4>,
        jet0: SurfaceJet<Vec3, 0>,
        jet1: SurfaceJet<Vec3, 1>,
        normal: Option<UnitVec3>,
    }

    fn direct(s: &Surface, uv: Point2, side: [Approach; 2]) -> Direct {
        macro_rules! of {
            ($k:expr, $normal:expr) => {
                Direct {
                    domain: $k.domain(),
                    point: $k.point(uv),
                    jet: $k.jet::<4>(uv, side),
                    jet0: $k.jet::<0>(uv, side),
                    jet1: $k.jet::<1>(uv, side),
                    normal: $normal,
                }
            };
        }
        match s {
            Surface::Plane(k) => of!(k, Some(k.normal(uv))),
            Surface::Cylinder(k) => of!(k, Some(k.normal(uv))),
            Surface::Cone(k) => of!(k, k.normal(uv)),
            Surface::Sphere(k) => of!(k, Some(k.normal(uv))),
            Surface::Torus(k) => of!(k, Some(k.normal(uv))),
            Surface::Nurbs(n) => {
                let nan = |_, _| Vec3::splat(f64::NAN);
                let e = n.domain();
                let jet1 = n.jet::<1>(uv.to_array(), side);
                Direct {
                    domain: UvDomain::new(
                        ParamDomain::Bounded(e.x_interval()),
                        ParamDomain::Bounded(e.y_interval()),
                    ),
                    point: n.point(uv.to_array(), ABOVE).map_or(NAN3, Point3::from),
                    jet: n
                        .jet::<4>(uv.to_array(), side)
                        .map_or_else(|_| SurfaceJet::from_fn(nan), |j| j.map(Vec3::from)),
                    jet0: n
                        .jet::<0>(uv.to_array(), side)
                        .map_or_else(|_| SurfaceJet::from_fn(nan), |j| j.map(Vec3::from)),
                    jet1: jet1.map_or_else(|_| SurfaceJet::from_fn(nan), |j| j.map(Vec3::from)),
                    normal: jet1.ok().and_then(|j| {
                        let [su, sv] = [j.get(1, 0).unwrap(), j.get(0, 1).unwrap()];
                        Vec3::from(su).cross(Vec3::from(sv)).normalized()
                    }),
                }
            }
        }
    }

    #[test]
    fn dispatch_has_the_bits_of_the_kind() {
        let mut rng = Rng::new(31);
        let (mut n, mut nurbs, mut none) = (0, 0, 0);
        for s in surfaces(30) {
            for uv in params(&mut rng, &s) {
                for side in SIDES {
                    let want = direct(&s, uv, side);
                    assert_eq!(s.domain(), want.domain);
                    let p = s.point(uv);
                    assert_eq!(bits3(p.to_array()), bits3(want.point.to_array()), "{uv:?}");
                    assert_eq!(jbits(&s.jet::<4>(uv, side)), jbits(&want.jet), "{uv:?}");
                    assert_eq!(jbits(&s.jet::<0>(uv, side)), jbits(&want.jet0));
                    assert_eq!(jbits(&s.jet::<1>(uv, side)), jbits(&want.jet1));
                    let normal = s.normal(uv, side);
                    assert_eq!(nbits(normal), nbits(want.normal), "{uv:?}");
                    n += 1;
                    nurbs += usize::from(matches!(s, Surface::Nurbs(_)));
                    none += usize::from(normal.is_none());
                }
            }
        }
        assert!(
            n > 30_000 && nurbs > 5_000 && none > 0,
            "{n} {nurbs} {none}"
        );
    }

    #[test]
    fn eval_many_has_the_bits_of_point() {
        let mut rng = Rng::new(33);
        for s in surfaces(32) {
            let uv = params(&mut rng, &s);
            let mut out = vec![Point3::ORIGIN; uv.len()];
            s.eval_many(&uv, &mut out);
            for (p, &q) in out.iter().zip(&uv) {
                assert_eq!(bits3(p.to_array()), bits3(s.point(q).to_array()), "{q:?}");
            }
            // Finite parameters only: the NURBS arm's batch path, with no fallback.
            let finite: Vec<Point2> = uv
                .iter()
                .copied()
                .filter(|p| p.x.abs() < 2.0 && p.y.abs() < 2.0)
                .collect();
            let mut out = vec![Point3::ORIGIN; finite.len()];
            s.eval_many(&finite, &mut out);
            for (p, &q) in out.iter().zip(&finite) {
                assert_eq!(bits3(p.to_array()), bits3(s.point(q).to_array()), "{q:?}");
            }
            s.eval_many(&[], &mut []);
            // Grids: from the same parameters (an infinity and a NaN among them, so a NURBS
            // grid errs and takes the per-point path), and from finite ones (its batch path).
            let us: Vec<f64> = uv.iter().map(|p| p.x).take(20).collect();
            let vs: Vec<f64> = uv.iter().map(|p| p.y).skip(20).take(13).collect();
            let finite =
                |t: &[f64]| -> Vec<f64> { t.iter().copied().filter(|x| x.abs() < 2.0).collect() };
            for (us, vs) in [(us.clone(), vs.clone()), (finite(&us), finite(&vs))] {
                let mut out = vec![Point3::ORIGIN; us.len() * vs.len()];
                s.eval_grid(&us, &vs, &mut out);
                for (k, p) in out.iter().enumerate() {
                    let q = Point2::new(us[k / vs.len()], vs[k % vs.len()]);
                    assert_eq!(bits3(p.to_array()), bits3(s.point(q).to_array()), "{q:?}");
                }
            }
            s.eval_grid(&[], &[0.5], &mut []);
            s.eval_grid(&[0.5], &[], &mut []);
        }
    }

    #[test]
    #[should_panic(expected = "differ in length")]
    fn eval_many_refuses_slices_of_different_lengths() {
        let s = Surface::from(Plane::new(Frame::WORLD));
        s.eval_many(&[Point2::ORIGIN; 2], &mut [Point3::ORIGIN]);
    }

    #[test]
    #[should_panic(expected = "us.len() * vs.len()")]
    fn eval_grid_refuses_an_output_of_the_wrong_length() {
        let s = Surface::from(Plane::new(Frame::WORLD));
        s.eval_grid(&[0.0, 1.0], &[0.0, 1.0], &mut [Point3::ORIGIN; 3]);
    }

    #[test]
    #[should_panic(expected = "differ in length")]
    fn a_kind_refuses_slices_of_different_lengths() {
        let s = Sphere::new(Frame::WORLD, Length::from_mm(1.0), Sense::Forward).unwrap();
        s.eval_many(&[Point2::ORIGIN], &mut []);
    }

    #[test]
    fn every_kind_converts_into_its_arm() {
        let f = Frame::WORLD;
        let (r, s) = (Length::from_mm(2.0), Sense::Forward);
        let pl = Plane::new(f);
        let cy = Cylinder::new(f, r, s).unwrap();
        let co = Cone::new(f, r, 0.5, s).unwrap();
        let sp = Sphere::new(f, r, s).unwrap();
        let to = Torus::new(f, r, Length::from_mm(1.0), s).unwrap();
        assert_eq!(Surface::from(pl), Surface::Plane(pl));
        assert_eq!(Surface::from(cy), Surface::Cylinder(cy));
        assert_eq!(Surface::from(co), Surface::Cone(co));
        assert_eq!(Surface::from(sp), Surface::Sphere(sp));
        assert_eq!(Surface::from(to), Surface::Torus(to));
        let Surface::Nurbs(n) = nurbs(&mut Rng::new(34)).remove(0) else {
            unreachable!()
        };
        let by_value = Surface::from((*n).clone());
        let shared = Surface::from(Arc::clone(&n));
        assert_eq!(by_value, shared);
        let Surface::Nurbs(m) = &shared else {
            unreachable!()
        };
        assert!(Arc::ptr_eq(m, &n));
        assert_ne!(Surface::from(cy), Surface::from(sp));
    }

    /// A C0 knot line: the normal differs by side, and the enum follows `side`.
    #[test]
    fn a_nurbs_normal_follows_the_side_at_a_crease() {
        // Two flat bilinear patches meeting at u = 0.5 at a right angle.
        let u = KnotVector::new(1, vec![0.0, 0.0, 0.5, 1.0, 1.0]).unwrap();
        let v = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
        let pts = vec![
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
        ];
        let s = Surface::from(NurbsSurface::new(u, v, Net::new(3, 2, pts).unwrap(), None).unwrap());
        let at = Point2::new(0.5, 0.5);
        let below = s.normal(at, [Approach::Below, Approach::Above]).unwrap();
        let above = s.normal(at, [Approach::Above, Approach::Above]).unwrap();
        assert_eq!(below.as_vec().to_array(), [0.0, 0.0, 1.0]);
        assert_eq!(above.as_vec().to_array(), [-1.0, 0.0, 0.0]);
    }
}
