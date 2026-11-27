// Written for solidcorex. Apache-2.0.
//! The exact pcurve of a curve lying on a surface (research 63 D10, §4.11): the 2-D curve in
//! the surface's uv (D13) whose image is the curve, at the curve's own parameter.
//!
//! [`pcurve_of`] gives a closed form where one exists and refuses with a typed
//! [`PcurveError`] otherwise; it never fits or approximates (that is the writer's, BR9).
//! Every answer is checked by lifting: at sample parameters across the interval, the
//! surface's point at the pcurve's point lies within the tolerance of the curve's point.
//!
//! | curve on surface | pcurve |
//! |---|---|
//! | line, circle, ellipse, parabola, hyperbola, NURBS curve on a plane | [`Line2`], [`Circle2`], [`Ellipse2`], [`Parabola2`], [`Hyperbola2`], the NURBS curve through the plane's affine uv |
//! | line along a cylinder's or cone's ruling | [`Line2`]: `u` fixed, `v` the height, affine in `t` |
//! | circle about the axis of a cylinder, cone, sphere or torus (a parallel) | [`AngleLine2`] in `u`, `v` fixed |
//! | sphere's great circle through the poles (a meridian), on one side of them | [`AngleLine2`] in `v`, `u` fixed |
//! | torus's tube circle (a meridian) | [`AngleLine2`] in `v`, `u` fixed |
//! | meridian over an interval that crosses a pole | [`PcurveError::CrossesPole`]: split it there |
//! | anything else, or any curve on a NURBS surface | [`PcurveError::NoClosedForm`] |
//!
//! A circle is tried as a parallel or a meridian when its axis and centre are within
//! `1e-6` (relative) of that configuration; the lifting check then decides, so a circle
//! near one but not in it is [`PcurveError::Inexact`] and one far from both is
//! [`PcurveError::NoClosedForm`].
//!
//! The period window (07 §3 rule 5): where a direction is periodic, the pcurve's coordinate
//! at the interval's start lies in the surface's base window `[base, base + period)`. A face
//! whose trims use another window shifts the pcurve by whole periods.
//!
//! The pin (`eval/pcurve.rs`, `On::exact`) builds the same forms as degree-1 splines
//! between the interval's end points, in a uv whose angle directions are radians; there a
//! parallel or a meridian is affine in the circle's angle. Here the kinds are kept (a line
//! stays a [`Line2`], a circle a [`Circle2`]) in the NURBS uv, whose angle directions are
//! arc parameters (63 D13), so a parallel or a meridian is an [`AngleLine2`] at the
//! circle's own parameter. The pin also lets a sphere's meridian run past a pole (latitude
//! beyond `pi/2`); here that is [`PcurveError::CrossesPole`] (63 §4.11).

use core::fmt;
use std::sync::Arc;

use core::f64::consts::{FRAC_PI_2, PI, TAU};

use scx_curve::arcs::Arcs;
use scx_curve::{
    AngleAxis, AngleLine2, Circle, Circle2, Curve, Curve2, Ellipse2, Hyperbola2, Line2, Parabola2,
};
use scx_num::domain::ParamDomain;
use scx_num::error::ErrorKind;
use scx_num::extent1d::Extent1d;
use scx_num::fmath;
use scx_num::frame::{Frame2, Sense};
use scx_num::units::Length;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::NurbsCurve;

use crate::common::Axes;
use crate::{Cone, Plane, Surface};

/// The parameters the lifting check samples, ends included.
const SAMPLES: usize = 9;

/// How near (relative: an angle in radians, an offset over the circle's radius) a circle's
/// axis and centre must be to a parallel's or a meridian's for that closed form to be
/// tried. It only picks the candidate; the lifting check at the tolerance decides.
const NEAR: f64 = 1e-6;

/// Slack, in radians, for a meridian's end at a pole: the end's latitude is a rounded
/// `atan2`, so a half meridian from pole to pole may reach a few ulps past `pi/2`.
const POLE_SLACK: f64 = 1e-14;

/// A curve kind, as an error names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CurveKind {
    /// A line.
    Line,
    /// A circle.
    Circle,
    /// An ellipse.
    Ellipse,
    /// A parabola.
    Parabola,
    /// A hyperbola's branch.
    Hyperbola,
    /// A NURBS curve.
    Nurbs,
    /// A kind `scx-curve` gained after this crate was written (its `Curve` is
    /// non-exhaustive).
    Other,
}

impl CurveKind {
    /// The kind of `c`.
    #[must_use]
    pub const fn of(c: &Curve) -> Self {
        match c {
            Curve::Line(_) => Self::Line,
            Curve::Circle(_) => Self::Circle,
            Curve::Ellipse(_) => Self::Ellipse,
            Curve::Parabola(_) => Self::Parabola,
            Curve::Hyperbola(_) => Self::Hyperbola,
            Curve::Nurbs(_) => Self::Nurbs,
            _ => Self::Other,
        }
    }
}

/// A surface kind, as an error names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SurfaceKind {
    /// A plane.
    Plane,
    /// A cylinder.
    Cylinder,
    /// A cone.
    Cone,
    /// A sphere.
    Sphere,
    /// A torus.
    Torus,
    /// A NURBS surface.
    Nurbs,
}

impl SurfaceKind {
    /// The kind of `s`.
    #[must_use]
    pub const fn of(s: &Surface) -> Self {
        match s {
            Surface::Plane(_) => Self::Plane,
            Surface::Cylinder(_) => Self::Cylinder,
            Surface::Cone(_) => Self::Cone,
            Surface::Sphere(_) => Self::Sphere,
            Surface::Torus(_) => Self::Torus,
            Surface::Nurbs(_) => Self::Nurbs,
        }
    }
}

/// Why a curve has no exact pcurve here.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum PcurveError {
    /// The interval is a single parameter: no pcurve has an empty domain.
    EmptyInterval {
        /// The parameter.
        at: f64,
    },
    /// The tolerance is negative, infinite or NaN.
    BadTolerance {
        /// The value given, in mm.
        value: f64,
    },
    /// The curve leaves the surface: at `at` it is `miss` from it, more than the tolerance.
    NotOnSurface {
        /// The curve's parameter.
        at: f64,
        /// The distance from the surface, in mm.
        miss: f64,
    },
    /// The curve lies on the surface, but the pair has no closed-form pcurve here (a circle
    /// on a cylinder that is not about its axis, any curve on a NURBS surface): the writer
    /// fits one (63 D10, BR9).
    NoClosedForm {
        /// The curve's kind.
        curve: CurveKind,
        /// The surface's kind.
        surface: SurfaceKind,
    },
    /// The curve lies on the surface near a configuration with a closed form, but not in
    /// it: the closed form misses the curve at `at` by `miss`, more than the tolerance
    /// (a circle tilted off a plane by less than the tolerance over its radius).
    Inexact {
        /// The curve's parameter.
        at: f64,
        /// The distance between the curve and the lifted closed form, in mm.
        miss: f64,
    },
    /// A sphere's meridian passes a pole at `at`, inside the interval: there `u` jumps by
    /// half a turn, so no one pcurve covers the interval. The caller splits the edge at `at`.
    CrossesPole {
        /// The curve's parameter at the pole.
        at: f64,
    },
}

impl PcurveError {
    /// The error's kind: [`ErrorKind::Refused`] for the interval, the tolerance, a curve
    /// off the surface and a meridian across a pole, [`ErrorKind::Unsupported`] for a pair
    /// with no closed form.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::EmptyInterval { .. }
            | Self::BadTolerance { .. }
            | Self::NotOnSurface { .. }
            | Self::CrossesPole { .. } => ErrorKind::Refused,
            Self::NoClosedForm { .. } | Self::Inexact { .. } => ErrorKind::Unsupported,
        }
    }

    /// The cause's stable name (62 D6).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::EmptyInterval { .. } => "pcurve_interval_empty",
            Self::BadTolerance { .. } => "pcurve_tolerance_invalid",
            Self::NotOnSurface { .. } => "pcurve_not_on_surface",
            Self::NoClosedForm { .. } => "pcurve_no_closed_form",
            Self::Inexact { .. } => "pcurve_inexact",
            Self::CrossesPole { .. } => "pcurve_crosses_pole",
        }
    }
}

impl fmt::Display for CurveKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Line => "line",
            Self::Circle => "circle",
            Self::Ellipse => "ellipse",
            Self::Parabola => "parabola",
            Self::Hyperbola => "hyperbola",
            Self::Nurbs => "NURBS curve",
            Self::Other => "curve",
        })
    }
}

impl fmt::Display for SurfaceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Plane => "plane",
            Self::Cylinder => "cylinder",
            Self::Cone => "cone",
            Self::Sphere => "sphere",
            Self::Torus => "torus",
            Self::Nurbs => "NURBS surface",
        })
    }
}

impl fmt::Display for PcurveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::EmptyInterval { at } => write!(f, "the interval is the single parameter {at}"),
            Self::BadTolerance { value } => write!(f, "the tolerance {value} is not usable"),
            Self::NotOnSurface { at, miss } => {
                write!(f, "the curve leaves the surface: {miss} from it at {at}")
            }
            Self::NoClosedForm { curve, surface } => {
                write!(f, "no exact pcurve of this {curve} on a {surface}")
            }
            Self::Inexact { at, miss } => write!(
                f,
                "the closed form misses the curve by {miss} at {at}: the curve is near a \
                 configuration with an exact pcurve but not in it"
            ),
            Self::CrossesPole { at } => {
                write!(
                    f,
                    "the meridian crosses a pole at {at}: split the curve there"
                )
            }
        }
    }
}

impl std::error::Error for PcurveError {}

/// The exact pcurve of `curve` on `surface` over `over`: the 2-D curve `p` in the
/// surface's uv with `surface.point(p.point(t))` on `curve.point(t)` for every `t` in
/// `over` (the same parameter), within `tol`.
///
/// # Errors
///
/// [`PcurveError::EmptyInterval`] or [`PcurveError::BadTolerance`] for the arguments;
/// [`PcurveError::NotOnSurface`] when a sample of the curve is more than `tol` from the
/// surface; [`PcurveError::NoClosedForm`] for a pair with no closed form here (see the
/// module's table); [`PcurveError::CrossesPole`] for a meridian across a pole;
/// [`PcurveError::Inexact`] when the closed form fails the lifting check.
pub fn pcurve_of(
    curve: &Curve,
    surface: &Surface,
    over: Extent1d,
    tol: Length,
) -> Result<Curve2, PcurveError> {
    let tol = tol.mm();
    if !(tol.is_finite() && tol >= 0.0) {
        return Err(PcurveError::BadTolerance { value: tol });
    }
    if over.min() >= over.max() {
        return Err(PcurveError::EmptyInterval { at: over.min() });
    }
    let no_form = PcurveError::NoClosedForm {
        curve: CurveKind::of(curve),
        surface: SurfaceKind::of(surface),
    };
    let ts = samples(over);
    let points = ts.map(|t| curve.point(t));
    for (&t, &p) in ts.iter().zip(&points) {
        let miss = distance(surface, p).ok_or(no_form)?;
        // NaN refuses (07 §6 rule 5).
        if miss.is_nan() || miss > tol {
            return Err(PcurveError::NotOnSurface { at: t, miss });
        }
    }
    let pc = closed_form(curve, surface, &points, over)?.ok_or(no_form)?;
    for (&t, &p) in ts.iter().zip(&points) {
        let miss = (surface.point(pc.point(t)) - p).length();
        if miss.is_nan() || miss > tol {
            return Err(PcurveError::Inexact { at: t, miss });
        }
    }
    Ok(pc)
}

/// `SAMPLES` parameters evenly across `over`, its ends exact.
fn samples(over: Extent1d) -> [f64; SAMPLES] {
    let (lo, hi) = (over.min(), over.max());
    let last = (SAMPLES - 1) as f64;
    core::array::from_fn(|i| match i {
        0 => lo,
        i if i == SAMPLES - 1 => hi,
        i => lo + (hi - lo) * (i as f64 / last),
    })
}

/// The distance from `p` to the surface, in closed form; `None` for a NURBS surface.
fn distance(surface: &Surface, p: Point3) -> Option<f64> {
    let local = |a: Axes| {
        let d = p - a.o;
        let (x, y, h) = (d.dot(a.x), d.dot(a.y), d.dot(a.z));
        (fmath::hypot(x, y), h)
    };
    Some(match surface {
        Surface::Plane(s) => (p - s.frame().origin()).dot(*s.frame().z()).abs(),
        Surface::Cylinder(s) => {
            let (rho, _) = local(Axes::new(s.frame(), s.sense()));
            (rho - s.radius().mm()).abs()
        }
        Surface::Cone(s) => {
            let (rho, h) = local(Axes::new(s.frame(), s.sense()));
            cone_distance(s, rho, h)
        }
        Surface::Sphere(s) => ((p - s.frame().origin()).length() - s.radius().mm()).abs(),
        Surface::Torus(s) => {
            let (rho, h) = local(Axes::new(s.frame(), s.sense()));
            (fmath::hypot(rho - s.major().mm(), h) - s.minor().mm()).abs()
        }
        Surface::Nurbs(_) => return None,
    })
}

/// The distance from the point at radius `rho` and height `h` to the cone's nappe: in the
/// half-plane of `(rho, h)`, the nappe is the ray from the apex through `(r, 0)`.
fn cone_distance(c: &Cone, rho: f64, h: f64) -> f64 {
    let (r, k) = (c.radius().mm(), c.slope());
    let apex_h = -r / k;
    let (dr, dh) = (rho, h - apex_h);
    // The ray's direction, from the apex toward `(r, 0)`: `(k, 1)` when the apex is below
    // (`k > 0`), `(-k, -1)` when above.
    let n = fmath::hypot(k, 1.0);
    let (gr, gh) = (k.abs() / n, k.signum() / n);
    let along = dr * gr + dh * gh;
    if along > 0.0 {
        (dr * gh - dh * gr).abs()
    } else {
        fmath::hypot(dr, dh)
    }
}

/// The closed form for the pair, from the curve's sample points; `Ok(None)` where the pair
/// has none here.
fn closed_form(
    curve: &Curve,
    surface: &Surface,
    points: &[Point3; SAMPLES],
    over: Extent1d,
) -> Result<Option<Curve2>, PcurveError> {
    // Every angle direction here is a turn whose period is the domain's (the cylinder's
    // `2 pi r`, the others' 1) or the sphere's half turn (63 D13).
    let turn = |d: ParamDomain| d.period().map(Arcs::turn);
    Ok(match surface {
        Surface::Plane(s) => on_plane(curve, s),
        Surface::Cylinder(s) => {
            let axes = Axes::new(s.frame(), s.sense());
            let from_step = |phi: f64| s.from_step(Point2::new(phi, 0.0)).x;
            let u = s.domain().u;
            match curve {
                Curve::Circle(c) => turn(u).and_then(|a| parallel(c, &axes, over, a, u, |h, _| h)),
                _ => ruling(curve, &axes, points, from_step, u),
            }
        }
        Surface::Cone(s) => {
            let axes = Axes::new(s.frame(), s.sense());
            let from_step = |phi: f64| s.from_step(Point2::new(phi, 0.0)).x;
            let u = s.domain().u;
            match curve {
                Curve::Circle(c) => turn(u).and_then(|a| parallel(c, &axes, over, a, u, |h, _| h)),
                _ => ruling(curve, &axes, points, from_step, u),
            }
        }
        Surface::Sphere(s) => {
            let (Curve::Circle(c), Some(ua)) = (curve, turn(s.domain().u)) else {
                return Ok(None);
            };
            let axes = Axes::new(s.frame(), s.sense());
            let u = s.domain().u;
            let lat = |h: f64, rho: f64| s.from_step(Point2::new(0.0, fmath::atan2(h, rho))).y;
            match parallel(c, &axes, over, ua, u, lat) {
                Some(pc) => Some(pc),
                None => sphere_meridian(c, &axes, over, ua, u)?,
            }
        }
        Surface::Torus(s) => {
            let (u, v) = (s.domain().u, s.domain().v);
            let (Curve::Circle(c), Some(ua), Some(va)) = (curve, turn(u), turn(v)) else {
                return Ok(None);
            };
            let axes = Axes::new(s.frame(), s.sense());
            let major = s.major().mm();
            let tube = |h: f64, rho: f64| {
                in_window(
                    s.from_step(Point2::new(0.0, fmath::atan2(h, rho - major)))
                        .y,
                    v,
                )
            };
            parallel(c, &axes, over, ua, u, tube)
                .or_else(|| tube_circle(c, &axes, major, over, (ua, u), (va, v)))
        }
        Surface::Nurbs(_) => None,
    })
}

/// The circle's angle in the plane of the orthonormal `e1` and `e2`: `(alpha, s)` with the
/// circle's radial direction at `t` equal to `cos(alpha + s t) e1 + sin(alpha + s t) e2`
/// (for a circle in that plane), `s` [`Sense::Forward`] for `+1`.
///
/// The circle runs `cos t Fx + sigma sin t Fy` with `Fy = Fz x Fx`; with `Fz = ±(e1 x e2)`,
/// `Fy` is `Fx` turned a quarter forward or back in the plane, so `s` is the sign of
/// `sigma (Fz . (e1 x e2))`.
fn in_plane(c: &Circle, e1: Vec3, e2: Vec3) -> (f64, Sense) {
    let fx = *c.frame().x();
    let alpha = fmath::atan2(fx.dot(e2), fx.dot(e1));
    let with = c.frame().z().dot(e1.cross(e2)) >= 0.0;
    let s = match (with, c.sense()) {
        (true, Sense::Forward) | (false, Sense::Reversed) => Sense::Forward,
        _ => Sense::Reversed,
    };
    (alpha, s)
}

/// `start` moved by whole turns so the coordinate `arcs.parameter(start + s t0)` at the
/// interval's start lies in the window (07 §3 rule 5); unchanged for a direction that is
/// not periodic.
fn place(start: f64, s: Sense, t0: f64, arcs: Arcs, window: ParamDomain) -> f64 {
    let ParamDomain::Periodic { period, base, .. } = window else {
        return start;
    };
    let at = |a: f64| arcs.parameter(a + s.sign() * t0);
    let u0 = at(start);
    let a = start + ((in_window(u0, window) - u0) / period).round() * TAU;
    // How far the coordinate at `t0` is outside the half-open window (the top itself a
    // least step outside). At the seam the rounded parameter can be just outside on either
    // side; the turn nearest the window is kept.
    let outside = |a: f64| {
        let u = at(a);
        if (base..base + period).contains(&u) {
            0.0
        } else {
            (base - u).max(u - (base + period)).max(f64::MIN_POSITIVE)
        }
    };
    [a, a + TAU, a - TAU]
        .into_iter()
        .min_by(|x, y| outside(*x).total_cmp(&outside(*y)))
        .unwrap_or(a)
}

/// A circle about the axis (a parallel): `u` the arc parameter of the azimuth, `v` fixed at
/// `v_of(h, rho)` for the circle's height `h` along the axis and its radius `rho`. `None`
/// when the circle's axis or centre is not within `NEAR` of the surface's axis.
fn parallel(
    c: &Circle,
    axes: &Axes,
    over: Extent1d,
    arcs: Arcs,
    window: ParamDomain,
    v_of: impl Fn(f64, f64) -> f64,
) -> Option<Curve2> {
    let rho = c.radius().mm();
    let d = c.frame().origin() - axes.o;
    let h = d.dot(axes.z);
    let off_axis = (d - axes.z * h).length();
    if c.frame().z().cross(axes.z).length() > NEAR || off_axis > NEAR * rho {
        return None;
    }
    let (alpha, s) = in_plane(c, axes.x, axes.y);
    let start = place(alpha, s, over.min(), arcs, window);
    let line = AngleLine2::new(arcs, AngleAxis::U, start, s, v_of(h, rho), 0.0).ok()?;
    Some(Curve2::AngleLine(line))
}

/// `u` of the azimuth of the horizontal unit vector `d`, in the window.
fn azimuth(axes: &Axes, d: Vec3, arcs: Arcs, window: ParamDomain) -> f64 {
    in_window(
        arcs.parameter(fmath::atan2(d.dot(axes.y), d.dot(axes.x))),
        window,
    )
}

/// One half-plane of a sphere's meridian: the horizontal direction `d` of the half-plane,
/// the latitude `start + s t`, the latitude at the interval's start (in `(-pi, pi]`), and
/// whether the interval stays in `[-pi/2, pi/2]`.
struct Half {
    d: Vec3,
    start: f64,
    s: Sense,
    lat0: f64,
    fits: bool,
}

/// A sphere's great circle through the poles (a meridian): `v` the latitude's arc
/// parameter, `u` fixed at the azimuth of the half of the circle the interval runs on.
/// `Ok(None)` when the circle is not within `NEAR` of a meridian;
/// [`PcurveError::CrossesPole`] when the interval passes a pole.
fn sphere_meridian(
    c: &Circle,
    axes: &Axes,
    over: Extent1d,
    ua: Arcs,
    u: ParamDomain,
) -> Result<Option<Curve2>, PcurveError> {
    let rho = c.radius().mm();
    let fz = *c.frame().z();
    let off_centre = (c.frame().origin() - axes.o).length();
    if fz.dot(axes.z).abs() > NEAR || off_centre > NEAR * rho {
        return Ok(None);
    }
    let Some(d) = axes.z.cross(fz).normalized() else {
        return Ok(None);
    };
    let (t0, t1) = (over.min(), over.max());
    let half = |d: Vec3| {
        let (alpha, s) = in_plane(c, d, axes.z);
        let sign = s.sign();
        // The latitude at `t0`, reduced by whole turns into `(-pi, pi]`.
        let raw = alpha + sign * t0;
        let lat0 = raw - TAU * ((raw - PI) / TAU).ceil();
        let start = lat0 - sign * t0;
        let lat1 = start + sign * t1;
        let fits =
            lat0.min(lat1) >= -FRAC_PI_2 - POLE_SLACK && lat0.max(lat1) <= FRAC_PI_2 + POLE_SLACK;
        Half {
            d,
            start,
            s,
            lat0,
            fits,
        }
    };
    let halves = [half(*d), half(-*d)];
    if let Some(h) = halves.iter().find(|h| h.fits) {
        let other = azimuth(axes, h.d, ua, u);
        let line = AngleLine2::new(Arcs::half(), AngleAxis::V, h.start, h.s, other, 0.0);
        return Ok(line.ok().map(Curve2::AngleLine));
    }
    // The interval starts on one half and leaves it through the pole it moves toward.
    let h = halves
        .iter()
        .find(|h| h.lat0.abs() <= FRAC_PI_2 + POLE_SLACK)
        .unwrap_or(&halves[0]);
    let sign = h.s.sign();
    Err(PcurveError::CrossesPole {
        at: (FRAC_PI_2 * sign - h.start) * sign,
    })
}

/// A torus's tube circle (a meridian): `v` the tube angle's arc parameter, `u` fixed at the
/// azimuth of the tube's centre. `None` when the circle is not within `NEAR` of one.
fn tube_circle(
    c: &Circle,
    axes: &Axes,
    major: f64,
    over: Extent1d,
    (ua, u): (Arcs, ParamDomain),
    (va, v): (Arcs, ParamDomain),
) -> Option<Curve2> {
    let rho = c.radius().mm();
    let to_centre = c.frame().origin() - axes.o;
    let h = to_centre.dot(axes.z);
    let (dist, d) = (to_centre - axes.z * h).length_and_unit()?;
    let d = *d;
    let fz = *c.frame().z();
    let near = h.abs() <= NEAR * rho
        && (dist - major).abs() <= NEAR * major
        && fz.dot(d).abs() <= NEAR
        && fz.dot(axes.z).abs() <= NEAR;
    if !near {
        return None;
    }
    let (alpha, s) = in_plane(c, d, axes.z);
    let start = place(alpha, s, over.min(), va, v);
    let line = AngleLine2::new(va, AngleAxis::V, start, s, azimuth(axes, d, ua, u), 0.0).ok()?;
    Some(Curve2::AngleLine(line))
}

/// A curve on a plane through the plane's affine uv, `((p - O) . X, (p - O) . Y)`: exact
/// for every kind, the frame orthonormal.
fn on_plane(curve: &Curve, plane: &Plane) -> Option<Curve2> {
    let f = plane.frame();
    let (o, x, y, z) = (f.origin(), *f.x(), *f.y(), *f.z());
    let at = |p: Point3| {
        let d = p - o;
        Point2::new(d.dot(x), d.dot(y))
    };
    let along = |v: Vec3| Vec2::new(v.dot(x), v.dot(y));
    Some(match curve {
        Curve::Line(l) => Curve2::Line(Line2::new(at(l.origin()), along(l.direction())).ok()?),
        Curve::Circle(c) => {
            let (frame, sense) = conic_frame(c.frame(), c.sense(), at, along, z)?;
            Curve2::Circle(Circle2::new(frame, c.radius(), sense).ok()?)
        }
        Curve::Ellipse(e) => {
            let (frame, sense) = conic_frame(e.frame(), e.sense(), at, along, z)?;
            Curve2::Ellipse(Ellipse2::new(frame, e.semi_x(), e.semi_y(), sense).ok()?)
        }
        Curve::Parabola(c) => {
            let (frame, sense) = conic_frame(c.frame(), c.sense(), at, along, z)?;
            Curve2::Parabola(Parabola2::new(frame, c.focal(), sense).ok()?)
        }
        Curve::Hyperbola(h) => {
            let (frame, sense) = conic_frame(h.frame(), h.sense(), at, along, z)?;
            Curve2::Hyperbola(Hyperbola2::new(frame, h.semi_x(), h.semi_y(), sense).ok()?)
        }
        Curve::Nurbs(n) => {
            let points = n.points().iter().map(|p| {
                let q = at(Point3::new(p[0], p[1], p[2]));
                [q.x, q.y]
            });
            let weights = n.weights().map(<[f64]>::to_vec);
            let n2 = NurbsCurve::new(n.knot_vector().clone(), points.collect(), weights).ok()?;
            Curve2::Nurbs(Arc::new(n2))
        }
        _ => return None,
    })
}

/// A conic's frame in the plane's uv: its centre and x axis mapped, run the other way when
/// its z is against the plane's.
fn conic_frame(
    frame: &scx_num::frame::Frame,
    sense: Sense,
    at: impl Fn(Point3) -> Point2,
    along: impl Fn(Vec3) -> Vec2,
    z: Vec3,
) -> Option<(Frame2, Sense)> {
    let frame2 = Frame2::new(at(frame.origin()), along(*frame.x()))?;
    let flip = frame.z().dot(z) < 0.0;
    Some((frame2, if flip { sense.reversed() } else { sense }))
}

/// A line along a ruling of a cylinder or cone: `u` fixed where the line is, `v` its height
/// along the effective axis, `v0 + t (D . Z)`. `from_step` maps an angle to `u`.
fn ruling(
    curve: &Curve,
    axes: &Axes,
    points: &[Point3; SAMPLES],
    from_step: impl Fn(f64) -> f64,
    window: ParamDomain,
) -> Option<Curve2> {
    let Curve::Line(l) = curve else {
        return None;
    };
    // The azimuth from the sample farthest from the axis (a cone's ruling may reach the apex).
    let radial = |p: Point3| {
        let d = p - axes.o;
        (d.dot(axes.x), d.dot(axes.y))
    };
    let (x, y) = points
        .iter()
        .map(|&p| radial(p))
        .max_by(|a, b| fmath::hypot(a.0, a.1).total_cmp(&fmath::hypot(b.0, b.1)))?;
    let u = in_window(from_step(fmath::atan2(y, x)), window);
    let rate = l.direction().dot(axes.z);
    let v0 = (l.origin() - axes.o).dot(axes.z);
    Line2::new(Point2::new(u, v0), Vec2::new(0.0, rate))
        .ok()
        .map(Curve2::Line)
}

/// `u` moved by whole periods into the window `[base, base + period)` of a periodic
/// direction; unchanged otherwise.
fn in_window(u: f64, window: ParamDomain) -> f64 {
    let ParamDomain::Periodic { period, base, .. } = window else {
        return u;
    };
    let k = ((u - base) / period).floor();
    let mut w = u - k * period;
    if w >= base + period {
        w -= period;
    }
    if w < base {
        w += period;
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cylinder, Sphere, Torus};
    use core::f64::consts::FRAC_PI_2;
    use scx_curve::{Circle, Ellipse, Hyperbola, Line, Parabola};
    use scx_num::frame::Frame;
    use scx_num::hash::Rng;
    use scx_num::math::PI;
    use scx_nurbs::KnotVector;
    use scx_testkit::sample;

    fn frame(rng: &mut Rng, scale: f64) -> Frame {
        let f = sample::frame(rng, scale);
        Frame::new(Point3::from(f.origin), Vec3::from(f.z), Vec3::from(f.x)).unwrap()
    }

    fn mm(x: f64) -> Length {
        Length::from_mm(x)
    }

    fn span(lo: f64, hi: f64) -> Extent1d {
        Extent1d::new(lo, hi).unwrap()
    }

    const TOL: f64 = 1e-9;

    /// The truth check: a fine sampling of `over` mapped through the pcurve and then the
    /// surface lands back on the curve at the same parameter, to rounding of the sizes.
    fn lifts(curve: &Curve, surface: &Surface, pc: &Curve2, over: Extent1d, size: f64) {
        for i in 0..=100 {
            let t = over.min() + (over.max() - over.min()) * (f64::from(i) / 100.0);
            let got = surface.point(pc.point(t));
            let want = curve.point(t);
            let miss = (got - want).length();
            assert!(
                miss <= 1e-12 * size,
                "{curve:?} on {surface:?} at {t}: {miss}"
            );
        }
    }

    fn exact(curve: &Curve, surface: &Surface, over: Extent1d, size: f64) -> Curve2 {
        let pc = pcurve_of(curve, surface, over, mm(TOL * size))
            .unwrap_or_else(|e| panic!("{curve:?} on {surface:?}: {e}"));
        lifts(curve, surface, &pc, over, size);
        pc
    }

    /// Lines, circles, ellipses and a rational NURBS curve in a plane give the same kinds.
    #[test]
    fn plane_curves_keep_their_kinds() {
        let mut rng = Rng::new(0x9c_0001);
        for scale in [1.0, 1e3, 1e-3] {
            for _ in 0..20 {
                let f = frame(&mut rng, scale);
                let plane = Surface::Plane(Plane::new(f));
                let (o, x, y, z) = (f.origin(), *f.x(), *f.y(), *f.z());
                let size = 1.0 + 4.0 * scale;
                let at = |u: f64, v: f64| o + x * (u * scale) + y * (v * scale);
                let line = Line::through(at(0.3, -0.2), at(1.1, 0.7)).unwrap();
                let pc = exact(&Curve::Line(line), &plane, span(-2.0, 3.0), size);
                assert!(matches!(pc, Curve2::Line(_)));
                // Circles and ellipses with their z with and against the plane's.
                for (zc, sense) in [
                    (z, Sense::Forward),
                    (-z, Sense::Reversed),
                    (-z, Sense::Forward),
                ] {
                    let fc = Frame::new(at(0.5, 0.25), zc, x * 0.6 + y * 0.8).unwrap();
                    let c = Circle::new(fc, mm(0.7 * scale), sense).unwrap();
                    let pc = exact(&Curve::Circle(c), &plane, span(-1.0, 7.0), size);
                    assert!(matches!(pc, Curve2::Circle(_)));
                    let e = Ellipse::new(fc, mm(0.9 * scale), mm(0.4 * scale), sense).unwrap();
                    let pc = exact(&Curve::Ellipse(e), &plane, span(0.5, 2.0), size);
                    assert!(matches!(pc, Curve2::Ellipse(_)));
                    let p = Parabola::new(fc, mm(0.3 * scale), sense).unwrap();
                    let pc = exact(&Curve::Parabola(p), &plane, span(-2.0, 1.5), size * 4.0);
                    assert!(matches!(pc, Curve2::Parabola(_)));
                    let h = Hyperbola::new(fc, mm(0.5 * scale), mm(0.2 * scale), sense).unwrap();
                    let pc = exact(&Curve::Hyperbola(h), &plane, span(-1.5, 2.0), size * 4.0);
                    assert!(matches!(pc, Curve2::Hyperbola(_)));
                }
                // A rational quarter circle.
                let w = core::f64::consts::FRAC_1_SQRT_2;
                let pts = [at(1.0, 0.0), at(1.0, 1.0), at(0.0, 1.0)].map(|p| [p.x, p.y, p.z]);
                let knots = KnotVector::new(2, vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]).unwrap();
                let n = NurbsCurve::new(knots, pts.to_vec(), Some(vec![1.0, w, 1.0])).unwrap();
                let pc = exact(&Curve::Nurbs(Arc::new(n)), &plane, span(0.0, 1.0), size);
                assert!(matches!(pc, Curve2::Nurbs(_)));
            }
        }
    }

    /// A ruling of a cylinder or cone is a line of fixed `u` in the window, `v` affine,
    /// including the rulings at and just below the seam, and a cone's that reaches its apex.
    #[test]
    fn rulings_are_lines_of_fixed_u() {
        let mut rng = Rng::new(0x9c_0002);
        let phis = [0.0, 1.0, PI, -0.3, 2.0 * PI - 1e-14, -1e-15];
        for scale in [1.0, 1e3, 1e-3] {
            for _ in 0..10 {
                let f = frame(&mut rng, scale);
                let r = 0.8 * scale;
                for sense in [Sense::Forward, Sense::Reversed] {
                    let cyl = Cylinder::new(f, mm(r), sense).unwrap();
                    let cone = Cone::new(f, mm(r), 0.4, sense).unwrap();
                    let narrowing = Cone::new(f, mm(r), -0.3, sense).unwrap();
                    let axes = |s: Sense| Axes::new(&f, s);
                    let a = axes(sense);
                    for phi in phis {
                        let d = a.x * fmath::cos(phi) + a.y * fmath::sin(phi);
                        let foot = a.o + d * r;
                        let size = 1.0 + 4.0 * scale;
                        let s = Surface::Cylinder(cyl);
                        let line = Line::new(foot + a.z * (0.2 * scale), a.z * scale).unwrap();
                        let pc = exact(&Curve::Line(line), &s, span(-1.0, 2.0), size);
                        check_window(&pc, &s);
                        for c in [cone, narrowing] {
                            let k = c.slope();
                            let dir = d * k + a.z;
                            let line = Line::new(foot, dir * scale).unwrap();
                            let s = Surface::Cone(c);
                            let apex = -0.8 / k;
                            let over = if k > 0.0 {
                                span(apex, 1.0)
                            } else {
                                span(-1.0, apex)
                            };
                            let pc = exact(&Curve::Line(line), &s, over, size);
                            check_window(&pc, &s);
                        }
                    }
                }
            }
        }
    }

    fn check_window(pc: &Curve2, s: &Surface) {
        let Curve2::Line(l) = pc else {
            panic!("{pc:?}");
        };
        let p = s.domain().u.period().unwrap();
        assert!((0.0..p).contains(&l.origin().x), "{pc:?}");
        assert_eq!(l.direction().x, 0.0);
    }

    /// The refusals, each typed.
    #[test]
    fn refusals_are_typed() {
        let mut rng = Rng::new(0x9c_0003);
        let f = frame(&mut rng, 1.0);
        let (o, x, z) = (f.origin(), *f.x(), *f.z());
        let tol = mm(TOL);
        let plane = Surface::Plane(Plane::new(f));
        let off = Line::new(o, x + z * 1e-3).unwrap();
        let e = pcurve_of(&Curve::Line(off), &plane, span(0.0, 1.0), tol).unwrap_err();
        assert_eq!(e.name(), "pcurve_not_on_surface");
        assert_eq!(e.kind(), ErrorKind::Refused);
        let on = Curve::Line(Line::new(o, x).unwrap());
        let e = pcurve_of(&on, &plane, span(1.0, 1.0), tol).unwrap_err();
        assert_eq!(e.name(), "pcurve_interval_empty");
        let e = pcurve_of(&on, &plane, span(0.0, 1.0), mm(f64::NAN)).unwrap_err();
        assert_eq!(e.name(), "pcurve_tolerance_invalid");
        // A line skew to a cylinder's axis by 1e-3 stays within 1e-6 of it over `[-1, 1]`,
        // but it is not a ruling: the ruling's closed form misses it.
        let cyl = Surface::Cylinder(Cylinder::new(f, mm(1.0), Sense::Forward).unwrap());
        let skew = Line::new(o + x, z + *f.y() * 1e-3).unwrap();
        let e = pcurve_of(&Curve::Line(skew), &cyl, span(-1.0, 1.0), mm(1e-6)).unwrap_err();
        assert_eq!(e.name(), "pcurve_inexact", "{e}");
        assert_eq!(e.kind(), ErrorKind::Unsupported);
        // No closed form here: an oblique section of a cylinder (an ellipse; the SS5
        // section gap, 63 §4.11), a small circle of a sphere tilted off its axis, a curve on
        // a NURBS surface.
        let (y, tilt) = (*f.y(), 0.4_f64);
        let major = x * fmath::cos(tilt) - z * fmath::sin(tilt);
        let fe = Frame::new(o, major.cross(y), major).unwrap();
        let oblique = Ellipse::new(fe, mm(1.0 / fmath::cos(tilt)), mm(1.0), Sense::Forward);
        let e = pcurve_of(&Curve::Ellipse(oblique.unwrap()), &cyl, span(0.0, 1.0), tol);
        assert_eq!(
            e.unwrap_err(),
            PcurveError::NoClosedForm {
                curve: CurveKind::Ellipse,
                surface: SurfaceKind::Cylinder
            }
        );
        // A hyperbola on a cone (its section by a plane along the axis), named as one.
        let k = fmath::tan(0.5);
        let cone = Surface::Cone(Cone::new(f, mm(0.0), 0.5, Sense::Forward).unwrap());
        let fh = Frame::new(o + y, y, z).unwrap();
        let h = Hyperbola::new(fh, mm(1.0 / k), mm(1.0), Sense::Forward).unwrap();
        let e = pcurve_of(&Curve::Hyperbola(h), &cone, span(-1.0, 1.0), mm(1e-9)).unwrap_err();
        assert_eq!(
            e,
            PcurveError::NoClosedForm {
                curve: CurveKind::Hyperbola,
                surface: SurfaceKind::Cone
            }
        );
        assert!(e.to_string().contains("hyperbola"), "{e}");
        let sphere = Surface::Sphere(Sphere::new(f, mm(1.0), Sense::Forward).unwrap());
        let n = (z + x).normalized().map(Vec3::from).unwrap();
        let small = Frame::new(o + n * 0.6, n, y).unwrap();
        let small = Curve::Circle(Circle::new(small, mm(0.8), Sense::Forward).unwrap());
        let e = pcurve_of(&small, &sphere, span(0.0, 1.0), tol).unwrap_err();
        assert_eq!(e.name(), "pcurve_no_closed_form", "{e}");
        assert_eq!(e.kind(), ErrorKind::Unsupported);
        let nurbs = Surface::Nurbs(Arc::new(
            Plane::new(f)
                .to_nurbs(&scx_num::extent2d::Extent2d::from_bounds(-2.0, -2.0, 2.0, 2.0).unwrap())
                .unwrap(),
        ));
        let e = pcurve_of(&on, &nurbs, span(0.0, 1.0), tol).unwrap_err();
        assert_eq!(e.name(), "pcurve_no_closed_form");
        // A line on a sphere or torus leaves it; a line across a cylinder too.
        let torus = Surface::Torus(Torus::new(f, mm(3.0), mm(1.0), Sense::Forward).unwrap());
        for s in [&sphere, &torus, &cyl] {
            let e = pcurve_of(&on, s, span(-0.5, 0.5), tol).unwrap_err();
            assert_eq!(e.name(), "pcurve_not_on_surface", "{s:?}");
        }
        // A cone's ruling continued past the apex onto the other nappe leaves the domain.
        let cone = Cone::new(f, mm(1.0), 0.4, Sense::Forward).unwrap();
        let k = cone.slope();
        let line = Line::new(o + x, x * k + z).unwrap();
        let e = pcurve_of(
            &Curve::Line(line),
            &Surface::Cone(cone),
            span(-5.0, 1.0),
            tol,
        );
        assert_eq!(e.unwrap_err().name(), "pcurve_not_on_surface");
    }

    /// The pcurve is an angle line in `axis` with the other coordinate fixed, and at `t0`
    /// each periodic coordinate lies in the surface's window.
    fn check_angle_line(pc: &Curve2, axis: AngleAxis, s: &Surface, t0: f64) {
        let Curve2::AngleLine(a) = pc else {
            panic!("{pc:?}");
        };
        assert_eq!(a.axis(), axis);
        assert_eq!(a.rate(), 0.0);
        let p = pc.point(t0);
        for (c, window) in [(p.x, s.domain().u), (p.y, s.domain().v)] {
            // At the seam the rounded parameter may sit a rounding outside.
            if let ParamDomain::Periodic { period, base, .. } = window {
                let slack = 1e-15 * period;
                assert!(
                    c >= base - slack && c <= base + period + slack,
                    "{pc:?} {c} at {t0}"
                );
            }
        }
    }

    /// Intervals across the seam, past a turn, negative and short.
    const SPANS: [(f64, f64); 5] = [
        (0.0, 6.0),
        (-1.0, 2.0),
        (5.5, 13.0),
        (-20.0, -19.5),
        (3.0, 3.1),
    ];

    /// A circle about the axis of each surface of revolution, each sense of circle and
    /// surface, its z with and against the axis, is an angle line in `u` with `v` fixed.
    #[test]
    fn parallels_are_angle_lines_in_u() {
        let mut rng = Rng::new(0x9c_0004);
        for scale in [1.0, 1e3, 1e-3] {
            for _ in 0..6 {
                let f = frame(&mut rng, scale);
                let size = 1.0 + 8.0 * scale;
                for sense in [Sense::Forward, Sense::Reversed] {
                    let a = Axes::new(&f, sense);
                    let r = 0.8 * scale;
                    let cone = Cone::new(f, mm(r), 0.4, sense).unwrap();
                    let cases: [(Surface, f64, f64); 6] = [
                        (
                            Surface::Cylinder(Cylinder::new(f, mm(r), sense).unwrap()),
                            0.3,
                            r,
                        ),
                        (Surface::Cone(cone), 0.5, r + cone.slope() * 0.5 * scale),
                        (
                            Surface::Sphere(Sphere::new(f, mm(r), sense).unwrap()),
                            0.3,
                            (0.64_f64 - 0.09).sqrt() * scale,
                        ),
                        (
                            Surface::Sphere(Sphere::new(f, mm(r), sense).unwrap()),
                            0.0,
                            r,
                        ),
                        (
                            Surface::Torus(Torus::new(f, mm(3.0 * scale), mm(r), sense).unwrap()),
                            0.5,
                            (3.0 + (0.64_f64 - 0.25).sqrt()) * scale,
                        ),
                        (
                            Surface::Torus(Torus::new(f, mm(3.0 * scale), mm(r), sense).unwrap()),
                            -0.5,
                            (3.0 - (0.64_f64 - 0.25).sqrt()) * scale,
                        ),
                    ];
                    for (s, h, rho) in cases {
                        let centre = a.o + a.z * (h * scale);
                        for zc in [a.z, -a.z] {
                            for phase in [0.0, 1.0, -2.5, PI] {
                                let xc = a.x * fmath::cos(phase) + a.y * fmath::sin(phase);
                                for cs in [Sense::Forward, Sense::Reversed] {
                                    let fc = Frame::new(centre, zc, xc).unwrap();
                                    let c = Curve::Circle(Circle::new(fc, mm(rho), cs).unwrap());
                                    for (lo, hi) in SPANS {
                                        let pc = exact(&c, &s, span(lo, hi), size);
                                        check_angle_line(&pc, AngleAxis::U, &s, lo);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// A sphere's great circle through the poles is an angle line in `v` with `u` fixed on
    /// the half it runs on, pole to pole included; across a pole it is refused at the pole.
    #[test]
    fn sphere_meridians_are_angle_lines_in_v() {
        let mut rng = Rng::new(0x9c_0005);
        for scale in [1.0, 1e3, 1e-3] {
            for _ in 0..6 {
                let f = frame(&mut rng, scale);
                let size = 1.0 + 2.0 * scale;
                for sense in [Sense::Forward, Sense::Reversed] {
                    let a = Axes::new(&f, sense);
                    let r = 0.8 * scale;
                    let s = Surface::Sphere(Sphere::new(f, mm(r), sense).unwrap());
                    for az in [0.0, 1.0, PI, -0.3, 2.0 * PI - 1e-13] {
                        let d = a.x * fmath::cos(az) + a.y * fmath::sin(az);
                        // `x = d`: latitude `+t` (forward, z = d x Z) on `d`'s half.
                        for (zc, cs) in [
                            (d.cross(a.z), Sense::Forward),
                            (-d.cross(a.z), Sense::Reversed),
                        ] {
                            let fc = Frame::new(a.o, zc, d).unwrap();
                            let c = Curve::Circle(Circle::new(fc, mm(r), cs).unwrap());
                            for (lo, hi) in [
                                (-FRAC_PI_2, FRAC_PI_2),
                                (-1.0, 0.5),
                                (2.0, 4.0),
                                (0.0, FRAC_PI_2),
                            ] {
                                let pc = exact(&c, &s, span(lo, hi), size);
                                check_angle_line(&pc, AngleAxis::V, &s, lo);
                            }
                            for (lo, hi, pole) in [
                                (0.3, 2.0, FRAC_PI_2),
                                (-2.0, 0.0, -FRAC_PI_2),
                                (1.0, 5.0, FRAC_PI_2),
                            ] {
                                let e =
                                    pcurve_of(&c, &s, span(lo, hi), mm(TOL * size)).unwrap_err();
                                let PcurveError::CrossesPole { at } = e else {
                                    panic!("{e}");
                                };
                                assert!((at - pole).abs() < 1e-12, "{at} {pole}");
                                assert_eq!(e.kind(), ErrorKind::Refused);
                            }
                            // Run the other way, the latitude falls: a different sense in v.
                            let pc = exact(&c, &s, span(-0.5, 0.5), size);
                            let Curve2::AngleLine(l) = pc else { panic!() };
                            assert_eq!(l.sense(), Sense::Forward);
                        }
                    }
                }
            }
        }
    }

    /// A torus's tube circle is an angle line in `v` with `u` fixed, over any interval.
    #[test]
    fn tube_circles_are_angle_lines_in_v() {
        let mut rng = Rng::new(0x9c_0006);
        for scale in [1.0, 1e3, 1e-3] {
            for _ in 0..6 {
                let f = frame(&mut rng, scale);
                let size = 1.0 + 8.0 * scale;
                for sense in [Sense::Forward, Sense::Reversed] {
                    let a = Axes::new(&f, sense);
                    let (big, r) = (3.0 * scale, 0.8 * scale);
                    let s = Surface::Torus(Torus::new(f, mm(big), mm(r), sense).unwrap());
                    for az in [0.0, 1.0, PI, -0.3] {
                        let d = a.x * fmath::cos(az) + a.y * fmath::sin(az);
                        for zc in [d.cross(a.z), -d.cross(a.z)] {
                            for phase in [0.0, 2.0, -1.0] {
                                let xc = d * fmath::cos(phase) + a.z * fmath::sin(phase);
                                for cs in [Sense::Forward, Sense::Reversed] {
                                    let fc = Frame::new(a.o + d * big, zc, xc).unwrap();
                                    let c = Curve::Circle(Circle::new(fc, mm(r), cs).unwrap());
                                    for (lo, hi) in SPANS {
                                        let pc = exact(&c, &s, span(lo, hi), size);
                                        check_angle_line(&pc, AngleAxis::V, &s, lo);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn window_moves_by_whole_periods() {
        let w = ParamDomain::periodic(1.0, 0.0).unwrap();
        assert_eq!(in_window(0.25, w), 0.25);
        assert_eq!(in_window(1.25, w), 0.25);
        assert_eq!(in_window(-0.75, w), 0.25);
        // Just below a whole period rounds onto the window's top: it wraps to its base.
        assert_eq!(in_window(-1e-17, w), 0.0);
        assert!((0.0..1.0).contains(&in_window(-1e-300, w)));
        assert_eq!(in_window(3.0, ParamDomain::Unbounded), 3.0);
    }
}
