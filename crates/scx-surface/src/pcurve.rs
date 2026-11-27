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
//! | ellipse on a cylinder (a plane's oblique section, centre on the axis) | [`AngleWave2`]: `u` the azimuth's arc parameter, `v` a sinusoid in `t` |
//! | sphere's great circle through the poles (a meridian), on one side of them | [`AngleLine2`] in `v`, `u` fixed |
//! | torus's tube circle (a meridian) | [`AngleLine2`] in `v`, `u` fixed |
//! | meridian over an interval that crosses a pole | [`PcurveError::CrossesPole`]: split it there |
//! | any other curve on a cylinder, cone, sphere or torus (a tilted circle on a sphere, a cone's conic section) | [`OnChart`]: the curve through the surface's chart inverse, at its own parameter (73 SM1) |
//! | such a curve through a pole or a cone's apex inside the interval | [`PcurveError::CrossesPole`]: split it there |
//! | such a curve meeting the axis at an end, or whose angle spans a band half a period wide or more about the straight line between its ends' | [`PcurveError::NoClosedForm`] |
//! | any curve on a NURBS surface | [`PcurveError::NoClosedForm`] |
//!
//! A circle is tried as a parallel or a meridian when its axis and centre are within
//! `1e-6` (relative) of that configuration, an ellipse on a cylinder as a section when its
//! centre is within `1e-6` of the axis; the lifting check then decides, so a curve near
//! one but not in it is [`PcurveError::Inexact`] and one far from all is
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
//!
//! The pin refuses an ellipse on a cylinder in `eval/pcurve.rs`; its drop of the curve
//! onto the surface (WR-680, `cylinder_section.rs`) builds a rational spline in the
//! azimuth, not in the ellipse's parameter. Here the pcurve keeps the edge's parameter,
//! which needs the sinusoid in `v`: an [`AngleWave2`] (63 §4.14).

use core::fmt;
use std::sync::Arc;

use core::f64::consts::{FRAC_PI_2, PI, TAU};

use scx_curve::arcs::Arcs;
use scx_curve::{
    AngleAxis, AngleLine2, AngleWave2, Chart, Circle, Circle2, Curve, Curve2, Ellipse, Ellipse2,
    Hyperbola2, Line2, OnChart, Parabola2,
};
use scx_num::domain::ParamDomain;
use scx_num::error::ErrorKind;
use scx_num::extent1d::Extent1d;
use scx_num::fmath;
use scx_num::frame::{Frame2, Sense};
use scx_num::units::Resolution;
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

/// The samples [`on_chart`] unwraps the angle across, ends included.
const DENSE: usize = 64;

/// How many times [`on_chart`] may halve a step that turns too fast (near the axis).
const MAX_HALVINGS: usize = 40;

/// Golden-section steps for the nearest approach to the axis: `0.618^120` of the bracket.
const GOLDEN_STEPS: usize = 120;

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
    /// The curve passes the surface's axis at `at`, inside the interval (a sphere's pole,
    /// a cone's apex: a meridian, or any curve through the chart): there `u` jumps, so no one
    /// pcurve covers the interval. The caller splits the edge at `at`.
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
            Self::EmptyInterval { .. } | Self::NotOnSurface { .. } | Self::CrossesPole { .. } => {
                ErrorKind::Refused
            }
            Self::NoClosedForm { .. } | Self::Inexact { .. } => ErrorKind::Unsupported,
        }
    }

    /// The cause's stable name (62 D6).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::EmptyInterval { .. } => "pcurve_interval_empty",
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
/// `over` (the same parameter), within `res.approx()`, the distance a fitted entity may
/// stand from the one it stands for. A `Resolution` is above zero and finite, so the
/// tolerance needs no refusal.
///
/// # Errors
///
/// [`PcurveError::EmptyInterval`] for the interval;
/// [`PcurveError::NotOnSurface`] when a sample of the curve is more than `res.approx()` from the
/// surface, at the sample farthest from it; [`PcurveError::NoClosedForm`] for a pair with no closed form here (see the
/// module's table); [`PcurveError::CrossesPole`] for a meridian across a pole;
/// [`PcurveError::Inexact`] when the closed form fails the lifting check.
pub fn pcurve_of(
    curve: &Curve,
    surface: &Surface,
    over: Extent1d,
    res: Resolution,
) -> Result<Curve2, PcurveError> {
    let tol = res.approx().mm();
    if over.min() >= over.max() {
        return Err(PcurveError::EmptyInterval { at: over.min() });
    }
    let no_form = PcurveError::NoClosedForm {
        curve: CurveKind::of(curve),
        surface: SurfaceKind::of(surface),
    };
    let ts = samples(over);
    let points = ts.map(|t| curve.point(t));
    for (i, (&t, &p)) in ts.iter().zip(&points).enumerate() {
        let miss = distance(surface, p).ok_or(no_form)?;
        // NaN refuses (07 §6 rule 5).
        if miss.is_nan() || miss > tol {
            return Err(worst_miss(surface, &ts, &points, i, t, miss));
        }
    }
    let pc = match closed_form(curve, surface, &points, over)? {
        Some(pc) => pc,
        None => on_chart(curve, surface, over, tol)?.ok_or(no_form)?,
    };
    for (&t, &p) in ts.iter().zip(&points) {
        let miss = (surface.point(pc.point(t)) - p).length();
        if miss.is_nan() || miss > tol {
            return Err(PcurveError::Inexact { at: t, miss });
        }
    }
    Ok(pc)
}

/// [`PcurveError::NotOnSurface`] at the largest miss over the samples from `first` on, the
/// first sample off the surface (`t`, `miss`): a caller that tells a near miss from a far
/// one (BR5's writer, `NeedsFit` against `OffSurface`) must see the far one. A NaN miss is
/// the largest. Only the failure path pays for the rest of the samples.
fn worst_miss(
    surface: &Surface,
    ts: &[f64; SAMPLES],
    points: &[Point3; SAMPLES],
    first: usize,
    t: f64,
    miss: f64,
) -> PcurveError {
    let (mut at, mut worst) = (t, miss);
    for (&t, &p) in ts.iter().zip(points).skip(first + 1) {
        if worst.is_nan() {
            break;
        }
        let miss = distance(surface, p).unwrap_or(f64::NAN);
        if miss.is_nan() || miss > worst {
            (at, worst) = (t, miss);
        }
    }
    PcurveError::NotOnSurface { at, miss: worst }
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
                Curve::Ellipse(e) => {
                    turn(u).and_then(|a| section(e, &axes, s.radius().mm(), over, a, u))
                }
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

/// The curve through the surface's chart ([`OnChart`]), for a curve on an analytic surface
/// with no other closed form: `Ok(None)` for a NURBS surface, and where the curve's angle
/// spans a band half a period wide or more about the straight line between its ends' (the
/// branch rule `OnChart` lifts by, nearest to the band's centre line, would be unsafe there);
/// [`PcurveError::CrossesPole`] where the curve meets the axis (a pole, an apex) inside the
/// interval; [`PcurveError::NoClosedForm`] (as `None`) where it meets it at an end, where
/// the angle has no value.
///
/// The angle is unwrapped across `DENSE` samples, each step halved until it turns less than
/// an eighth of the period; the axis is found from the samples' distances to it, each local
/// least one near enough to reach the axis refined by golden section.
fn on_chart(
    curve: &Curve,
    surface: &Surface,
    over: Extent1d,
    tol: f64,
) -> Result<Option<Curve2>, PcurveError> {
    let Some(chart) = surface.chart() else {
        return Ok(None);
    };
    let axes = Axes::new(chart.frame(), chart.sense());
    let off_axis = |t: f64| {
        let (x, y, _) = axes.local(curve.point(t));
        fmath::hypot(x, y)
    };
    let (lo, hi) = (over.min(), over.max());
    let ts: [f64; DENSE] = core::array::from_fn(|i| match i {
        0 => lo,
        i if i == DENSE - 1 => hi,
        i => lo + (hi - lo) * (i as f64 / (DENSE - 1) as f64),
    });
    let points = ts.map(|t| curve.point(t));
    if !matches!(surface, Surface::Plane(_) | Surface::Cylinder(_)) {
        if off_axis(lo) <= tol || off_axis(hi) <= tol {
            return Ok(None);
        }
        if let Some(at) = axis_crossing(&ts, &points, &axes, off_axis, tol) {
            return Err(PcurveError::CrossesPole { at });
        }
    }
    let periods = chart.periods();
    let windows = [surface.domain().u, surface.domain().v];
    let first = chart.inverse(points[0]);
    let mut start = [first.x, first.y];
    for ((s, w), p) in start.iter_mut().zip(windows).zip(periods) {
        if p.is_some() {
            *s = in_window(*s, w);
        }
    }
    // Every coordinate (unwrapped where periodic) at each parameter the walk visits.
    let mut walk: Vec<(f64, [f64; 2])> = vec![(lo, start)];
    for (&t, &p) in ts.iter().zip(&points).skip(1) {
        let (t0, at0) = walk.last().copied().unwrap_or((lo, start));
        step(&chart, curve, periods, (t0, at0), (t, p), 0, &mut walk);
    }
    let end = walk.last().map_or(start, |w| w.1);
    // The reference line: the chord between the lifted ends, moved to the middle of the
    // band the curve's lifted values span about it. The nearest lift is the curve's own
    // while the band's half-width is under half a period; a quarter keeps a margin.
    let mut reference = [start, end];
    for (k, p) in periods.iter().enumerate() {
        let Some(p) = *p else { continue };
        let (a, b) = (coord(start, k), coord(end, k));
        let (low, high) = walk.iter().fold((0.0_f64, 0.0_f64), |(low, high), &(t, at)| {
            let d = coord(at, k) - (a + (b - a) * ((t - lo) / (hi - lo)));
            (low.min(d), high.max(d))
        });
        if high - low >= 0.5 * p {
            return Ok(None);
        }
        let mid = 0.5 * (low + high);
        for r in &mut reference {
            if let Some(c) = r.get_mut(k) {
                *c += mid;
            }
        }
    }
    let [r0, r1] = reference.map(|r| Point2::new(r[0], r[1]));
    Ok(OnChart::new(curve.clone(), chart, over, [r0, r1])
        .ok()
        .map(Curve2::from))
}

fn coord(at: [f64; 2], k: usize) -> f64 {
    at.get(k).copied().unwrap_or(f64::NAN)
}

/// Walk from `(t0, at0)` to the parameter `t1` (its point `p1`): push `t1`'s coordinates,
/// each periodic one the period's copy nearest the previous, halving the step (to a depth of
/// `MAX_HALVINGS`) while a periodic one turns by an eighth of its period or more.
fn step(
    chart: &Chart,
    curve: &Curve,
    periods: [Option<f64>; 2],
    (t0, at0): (f64, [f64; 2]),
    (t1, p1): (f64, Point3),
    depth: usize,
    walk: &mut Vec<(f64, [f64; 2])>,
) {
    let raw = chart.inverse(p1);
    let mut at1 = [raw.x, raw.y];
    let mut fast = false;
    for ((a1, &a0), p) in at1.iter_mut().zip(&at0).zip(periods) {
        let Some(p) = p else { continue };
        let d = *a1 - a0;
        let d = d - p * (d / p).round();
        *a1 = a0 + d;
        fast |= d.abs() >= 0.125 * p;
    }
    let mid = 0.5 * (t0 + t1);
    if fast && depth < MAX_HALVINGS && t0 < mid && mid < t1 {
        step(chart, curve, periods, (t0, at0), (mid, curve.point(mid)), depth + 1, walk);
        let m = walk.last().copied().unwrap_or((t0, at0));
        step(chart, curve, periods, m, (t1, p1), depth + 1, walk);
        return;
    }
    walk.push((t1, at1));
}

/// The parameter inside the interval where the curve meets the surface's axis, within
/// `tol`: each sample nearer the axis than both its neighbours and than the chord between
/// them (so the curve might reach the axis between them) is refined by golden section.
fn axis_crossing(
    ts: &[f64; DENSE],
    points: &[Point3; DENSE],
    axes: &Axes,
    off_axis: impl Fn(f64) -> f64,
    tol: f64,
) -> Option<f64> {
    let rho = points.map(|p| {
        let (x, y, _) = axes.local(p);
        fmath::hypot(x, y)
    });
    for i in 1..DENSE - 1 {
        let (Some(&[r0, r1, r2]), Some(&[t0, _, t2]), Some(&[p0, _, p2])) = (
            rho.get(i - 1..=i + 1).and_then(|s| <&[f64; 3]>::try_from(s).ok()),
            ts.get(i - 1..=i + 1).and_then(|s| <&[f64; 3]>::try_from(s).ok()),
            points.get(i - 1..=i + 1).and_then(|s| <&[Point3; 3]>::try_from(s).ok()),
        ) else {
            continue;
        };
        if r1 > r0 || r1 > r2 || r1 > (p2 - p0).length() {
            continue;
        }
        let (at, r) = golden_min(&off_axis, t0, t2);
        if r <= tol {
            return Some(at);
        }
    }
    None
}

/// The least of `f` over `[a, b]` by golden section, `(t, f(t))`, for `f` with one local
/// least there.
fn golden_min(f: &impl Fn(f64) -> f64, mut a: f64, mut b: f64) -> (f64, f64) {
    const G: f64 = 0.618_033_988_749_894_8;
    let mut c = b - G * (b - a);
    let mut d = a + G * (b - a);
    let (mut fc, mut fd) = (f(c), f(d));
    for _ in 0..GOLDEN_STEPS {
        if !(a < c && c < d && d < b) {
            break;
        }
        if fc <= fd {
            (b, d, fd) = (d, c, fc);
            c = b - G * (b - a);
            fc = f(c);
        } else {
            (a, c, fc) = (c, d, fd);
            d = a + G * (b - a);
            fd = f(d);
        }
    }
    if fc <= fd { (c, fc) } else { (d, fd) }
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

/// A plane's oblique section of a cylinder (an ellipse whose centre is on the axis): `u` the
/// arc parameter of the azimuth `alpha + s t`, `v = h + p cos t + q sin t` (63 §4.14).
/// `None` when the centre is not within `NEAR` of the axis.
///
/// The ellipse runs `O + a cos t X + sigma b sin t Y`. Along the axis it projects onto the
/// cylinder's circle, so `A = a X_h` and `B = sigma b Y_h` (the parts across the axis) are
/// conjugate semi-diameters of a circle of radius `r`: perpendicular and of length `r`.
/// Then the projection is `r (cos(alpha + s t), sin(alpha + s t))` with `alpha` the azimuth
/// of `A` and `s` the turn from `A` to `B`, and the height is `(O - o).z + (A_z) cos t +
/// (B_z) sin t` for the axial parts. Both hold for every `t`, so the pcurve keeps the
/// ellipse's parameter; the lifting check catches an ellipse near this but not in it.
fn section(
    e: &Ellipse,
    axes: &Axes,
    r: f64,
    over: Extent1d,
    arcs: Arcs,
    window: ParamDomain,
) -> Option<Curve2> {
    let d = e.frame().origin() - axes.o;
    let h = d.dot(axes.z);
    if (d - axes.z * h).length() > NEAR * r {
        return None;
    }
    let a = *e.frame().x() * e.semi_x().mm();
    let b = *e.frame().y() * (e.semi_y().mm() * e.sense().sign());
    let (ax, ay) = (a.dot(axes.x), a.dot(axes.y));
    let (bx, by) = (b.dot(axes.x), b.dot(axes.y));
    let alpha = fmath::atan2(ay, ax);
    let s = if ax * by - ay * bx >= 0.0 {
        Sense::Forward
    } else {
        Sense::Reversed
    };
    let start = place(alpha, s, over.min(), arcs, window);
    let wave = AngleWave2::new(arcs, start, s, h, a.dot(axes.z), b.dot(axes.z)).ok()?;
    Some(Curve2::AngleWave(wave))
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
    use scx_num::units::Length;
    use scx_nurbs::KnotVector;
    use scx_nurbs::Approach;
    use scx_testkit::sample;

    fn frame(rng: &mut Rng, scale: f64) -> Frame {
        let f = sample::frame(rng, scale);
        Frame::new(Point3::from(f.origin), Vec3::from(f.z), Vec3::from(f.x)).unwrap()
    }

    fn mm(x: f64) -> Length {
        Length::from_mm(x)
    }

    /// The resolution whose approximation tolerance is `approx`, which `pcurve_of` reads
    /// (twice it is exact).
    fn res(approx: f64) -> Resolution {
        Resolution::new(mm(approx * 2.0)).unwrap()
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
        let pc = pcurve_of(curve, surface, over, res(TOL * size))
            .unwrap_or_else(|e| panic!("{curve:?} on {surface:?}: {e}"));
        lifts(curve, surface, &pc, over, size);
        pc
    }

    /// A curve off the surface is reported at its farthest sample, not its first one past
    /// the tolerance (FU1): a line leaving a plane at its end, a tilted circle at its middle.
    #[test]
    fn not_on_surface_reports_the_largest_miss() {
        let plane = Surface::Plane(Plane::new(Frame::WORLD));
        let line = Line::new(Point3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 1e-3)).unwrap();
        let got = pcurve_of(&Curve::Line(line), &plane, span(0.0, 1.0), res(1e-4));
        let Err(PcurveError::NotOnSurface { at, miss }) = got else {
            panic!("{got:?}")
        };
        assert_eq!(at, 1.0);
        let end = line.point(1.0).z;
        assert!((miss - end).abs() <= 1e-15, "{miss} against {end}");
        let tilted = Frame::new(Point3::ORIGIN, Vec3::new(0.0, -0.01, 1.0), Vec3::X).unwrap();
        let c = Circle::new(tilted, mm(2.0), Sense::Forward).unwrap();
        let got = pcurve_of(&Curve::Circle(c), &plane, span(0.0, PI), res(1e-3));
        let Err(PcurveError::NotOnSurface { at, miss }) = got else {
            panic!("{got:?}")
        };
        assert!((at - FRAC_PI_2).abs() <= 1e-15, "{at}");
        let top = c.point(FRAC_PI_2).z.abs();
        assert!((miss - top).abs() <= 1e-15, "{miss} against {top}");
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
        let tol = res(TOL);
        let plane = Surface::Plane(Plane::new(f));
        let off = Line::new(o, x + z * 1e-3).unwrap();
        let e = pcurve_of(&Curve::Line(off), &plane, span(0.0, 1.0), tol).unwrap_err();
        assert_eq!(e.name(), "pcurve_not_on_surface");
        assert_eq!(e.kind(), ErrorKind::Refused);
        let on = Curve::Line(Line::new(o, x).unwrap());
        let e = pcurve_of(&on, &plane, span(1.0, 1.0), tol).unwrap_err();
        assert_eq!(e.name(), "pcurve_interval_empty");
        // A line skew to a cylinder's axis by 1e-3 stays within 1e-6 of it over `[-1, 1]`,
        // but it is not a ruling: the ruling's closed form misses it.
        let cyl = Surface::Cylinder(Cylinder::new(f, mm(1.0), Sense::Forward).unwrap());
        let skew = Line::new(o + x, z + *f.y() * 1e-3).unwrap();
        let e = pcurve_of(&Curve::Line(skew), &cyl, span(-1.0, 1.0), res(1e-6)).unwrap_err();
        assert_eq!(e.name(), "pcurve_inexact", "{e}");
        assert_eq!(e.kind(), ErrorKind::Unsupported);
        // Through the chart (no other closed form here): a plane's section of a cone (an
        // ellipse), one of its hyperbolas, a small circle of a sphere tilted off its axis.
        // A curve on a NURBS surface has none.
        let y = *f.y();
        let (r, m) = (1.0, 0.5);
        let cone = Cone::new(f, mm(r), 0.4, Sense::Forward).unwrap();
        let c = cone.slope() * m;
        let y0 = r * c / (1.0 - c * c);
        let up = (y + z * m) * (1.0 / fmath::hypot(1.0, m));
        let fe = Frame::new(o + (y + z * m) * y0, x.cross(up), x).unwrap();
        let (sa, sb) = (
            r / (1.0 - c * c).sqrt(),
            r / (1.0 - c * c) * fmath::hypot(1.0, m),
        );
        let on_cone = Ellipse::new(fe, mm(sa), mm(sb), Sense::Forward).unwrap();
        let e = pcurve_of(
            &Curve::Ellipse(on_cone),
            &Surface::Cone(cone),
            span(0.0, 6.0),
            tol,
        );
        assert!(matches!(e, Ok(Curve2::OnChart(_))), "{e:?}");
        // A hyperbola on a cone (its section by a plane along the axis), named as one.
        let k = fmath::tan(0.5);
        let cone = Surface::Cone(Cone::new(f, mm(0.0), 0.5, Sense::Forward).unwrap());
        let fh = Frame::new(o + y, y, z).unwrap();
        let h = Hyperbola::new(fh, mm(1.0 / k), mm(1.0), Sense::Forward).unwrap();
        let e = pcurve_of(&Curve::Hyperbola(h), &cone, span(-1.0, 1.0), res(1e-9));
        assert!(matches!(e, Ok(Curve2::OnChart(_))), "{e:?}");
        let sphere = Surface::Sphere(Sphere::new(f, mm(1.0), Sense::Forward).unwrap());
        let n = (z + x).normalized().map(Vec3::from).unwrap();
        let small = Frame::new(o + n * 0.6, n, y).unwrap();
        let small = Curve::Circle(Circle::new(small, mm(0.8), Sense::Forward).unwrap());
        let e = pcurve_of(&small, &sphere, span(0.0, 1.0), tol);
        assert!(matches!(e, Ok(Curve2::OnChart(_))), "{e:?}");
        let nurbs = Surface::Nurbs(Arc::new(
            Plane::new(f)
                .to_nurbs(&scx_num::extent2d::Extent2d::from_bounds(-2.0, -2.0, 2.0, 2.0).unwrap())
                .unwrap(),
        ));
        let e = pcurve_of(&on, &nurbs, span(0.0, 1.0), tol).unwrap_err();
        assert_eq!(
            e,
            PcurveError::NoClosedForm {
                curve: CurveKind::Line,
                surface: SurfaceKind::Nurbs
            }
        );
        assert!(e.to_string().contains("NURBS surface"), "{e}");
        assert_eq!(e.kind(), ErrorKind::Unsupported);
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

    /// A plane's section of a cylinder through each tilt and turn about the axis, each sense
    /// of ellipse and cylinder, the ellipse's z with and against the plane's normal and its
    /// x the major or the minor axis, over intervals across the seam: an angle wave at the
    /// ellipse's own parameter, `u` in the window at the interval's start.
    #[test]
    fn cylinder_sections_are_angle_waves() {
        let mut rng = Rng::new(0x9c_0007);
        let mut n = 0;
        for scale in [1.0, 1e3, 1e-3] {
            for _ in 0..4 {
                let f = frame(&mut rng, scale);
                let r = 0.8 * scale;
                for sense in [Sense::Forward, Sense::Reversed] {
                    let a = Axes::new(&f, sense);
                    let cyl = Surface::Cylinder(Cylinder::new(f, mm(r), sense).unwrap());
                    for (tilt, psi) in [(0.4, 0.0), (1.2, 2.5), (0.05, -1.0), (1e-9, 0.3)] {
                        let (st, ct) = (fmath::sin(tilt), fmath::cos(tilt));
                        let (sp, cp) = (fmath::sin(psi), fmath::cos(psi));
                        let normal = (a.x * cp + a.y * sp) * st + a.z * ct;
                        let w = a.z.cross(normal).normalized().map(Vec3::from).unwrap();
                        let steep = normal.cross(w);
                        let centre = a.o + a.z * (0.3 * scale);
                        let size = 1.0 + 4.0 * scale / ct;
                        for (zc, xc, sx, sy) in [
                            (normal, w, r, r / ct),
                            (-normal, w, r, r / ct),
                            (normal, steep, r / ct, r),
                            (-normal, -steep, r / ct, r),
                        ] {
                            for es in [Sense::Forward, Sense::Reversed] {
                                let fe = Frame::new(centre, zc, xc).unwrap();
                                let e = Ellipse::new(fe, mm(sx), mm(sy), es).unwrap();
                                for (lo, hi) in SPANS {
                                    let over = span(lo, hi);
                                    let pc = exact(&Curve::Ellipse(e), &cyl, over, size);
                                    let Curve2::AngleWave(wave) = pc else {
                                        panic!("{pc:?}");
                                    };
                                    let u0 = wave.point(lo).x;
                                    let period = cyl.domain().u.period().unwrap();
                                    let slack = 1e-15 * period;
                                    assert!(u0 >= -slack && u0 <= period + slack, "{wave:?}");
                                    n += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(n, 3 * 4 * 2 * 4 * 4 * 2 * SPANS.len());
    }

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
                                    pcurve_of(&c, &s, span(lo, hi), res(TOL * size)).unwrap_err();
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

    /// Curves with no other closed form go through the chart: small circles of a sphere at
    /// many tilts, senses and spans (long ones cross the seam). Each lifts back onto the curve at fine samples, starts in the base window,
    /// and its derivative is the chain rule's.
    #[test]
    fn other_curves_go_through_the_chart() {
        let mut rng = Rng::new(0x9c_00c1);
        for _ in 0..40 {
            let f = frame(&mut rng, 1.0);
            let (o, x, y, z) = (f.origin(), *f.x(), *f.y(), *f.z());
            let r = 0.5 + 2.0 * rng.next_f64();
            let sphere = Surface::Sphere(Sphere::new(f, mm(r), Sense::Forward).unwrap());
            // The circle's axis, tilted 0.3 to 1.2 off the sphere's; its centre's height
            // keeps it clear of both poles (the pole is `tilt` from the axis, the circle's
            // angular radius is acos(h)).
            let tilt = 0.3 + 0.9 * rng.next_f64();
            let n = (z * fmath::cos(tilt) + x * fmath::sin(tilt)).normalized().unwrap();
            let n = Vec3::from(n);
            let h = fmath::cos(tilt + 0.2 + 0.3 * rng.next_f64());
            let frame = Frame::new(o + n * (r * h), n, y).unwrap();
            let rho = r * (1.0 - h * h).sqrt();
            let sense = if rng.next_u64() & 1 == 0 {
                Sense::Forward
            } else {
                Sense::Reversed
            };
            let c = Curve::Circle(Circle::new(frame, mm(rho), sense).unwrap());
            let lo = 6.0 * rng.next_f64() - 3.0;
            let over = span(lo, lo + 0.5 + 5.5 * rng.next_f64());
            let pc = exact(&c, &sphere, over, r);
            let Curve2::OnChart(o) = &pc else {
                panic!("{pc:?}");
            };
            let u0 = pc.point(over.min()).x;
            assert!((0.0..1.0).contains(&u0), "{u0} {:?}", o.reference());
            for i in 0..=20 {
                let t = over.min() + (over.max() - over.min()) * (f64::from(i) / 20.0);
                let j = pc.jet::<1>(t, Approach::Above);
                let sj = sphere.jet::<1>(j.point, [Approach::Above; 2]);
                let [d] = j.d;
                let chain = sj.get(1, 0).unwrap() * d.x + sj.get(0, 1).unwrap() * d.y;
                let [want] = c.jet::<1>(t, Approach::Above).d;
                assert!((chain - want).length() <= 1e-11 * r, "{t}: {chain:?} {want:?}");
            }
        }
    }

    /// A small circle through a sphere's pole: across it, the edge is split there; ending
    /// at it, `u` has no value at the end; the same circle clear of the pole goes through.
    #[test]
    fn a_curve_through_a_pole_is_split_there() {
        let f = Frame::WORLD;
        let (o, x, z) = (f.origin(), *f.x(), *f.z());
        let sphere = Surface::Sphere(Sphere::new(f, mm(1.0), Sense::Forward).unwrap());
        // Axis `alpha` off z, angular radius `alpha`: the circle meets the north pole at
        // `t = 0`, where its radial direction `w` points at the pole.
        let alpha: f64 = 0.7;
        let n = Vec3::from((z * fmath::cos(alpha) + x * fmath::sin(alpha)).normalized().unwrap());
        let w = Vec3::from((z * fmath::sin(alpha) - x * fmath::cos(alpha)).normalized().unwrap());
        let frame = Frame::new(o + n * fmath::cos(alpha), n, w).unwrap();
        let c = Curve::Circle(Circle::new(frame, mm(fmath::sin(alpha)), Sense::Forward).unwrap());
        assert!((c.point(0.0) - (o + z)).length() < 1e-15);
        let e = pcurve_of(&c, &sphere, span(-1.0, 1.5), res(TOL)).unwrap_err();
        let PcurveError::CrossesPole { at } = e else {
            panic!("{e:?}");
        };
        assert!(at.abs() < 1e-7, "{at}");
        let e = pcurve_of(&c, &sphere, span(0.0, 1.5), res(TOL)).unwrap_err();
        assert_eq!(e.name(), "pcurve_no_closed_form", "{e}");
        let pc = exact(&c, &sphere, span(0.5, 5.5), 1.0);
        assert!(matches!(pc, Curve2::OnChart(_)), "{pc:?}");
    }
}
