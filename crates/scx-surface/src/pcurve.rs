// Written for solidcorex. Apache-2.0.
//! The exact pcurve of a curve lying on a surface (research 63 D10, §4.10): the 2-D curve in
//! the surface's uv (D13) whose image is the curve, at the curve's own parameter.
//!
//! [`pcurve_of`] gives a closed form where one exists and refuses with a typed
//! [`PcurveError`] otherwise; it never fits or approximates (that is the writer's, BR9).
//! Every answer is checked by lifting: at sample parameters across the interval, the
//! surface's point at the pcurve's point lies within the tolerance of the curve's point.
//!
//! | curve on surface | pcurve |
//! |---|---|
//! | line, circle, ellipse, NURBS curve on a plane | [`Line2`], [`Circle2`], [`Ellipse2`], the NURBS curve through the plane's affine uv |
//! | line along a cylinder's or cone's ruling | [`Line2`]: `u` fixed, `v` the height, affine in `t` |
//! | anything else, or any curve on a NURBS surface | [`PcurveError::NoClosedForm`] |
//!
//! The period window (07 §3 rule 5): where `u` is periodic, the pcurve's `u` at the interval's
//! start lies in the surface's base window `[base, base + period)`. A face whose trims use
//! another window shifts the pcurve by whole periods.
//!
//! The pin (`eval/pcurve.rs`, `On::exact`) builds the same forms as degree-1 splines in
//! radians, between the interval's end points; here the kinds are kept (a line stays a
//! [`Line2`], a circle a [`Circle2`]) in the pin's NURBS uv.

use core::fmt;
use std::sync::Arc;

use scx_curve::{Circle2, Curve, Curve2, Ellipse2, Line2};
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
}

impl PcurveError {
    /// The error's kind: [`ErrorKind::Refused`] for the interval, the tolerance and a
    /// curve off the surface, [`ErrorKind::Unsupported`] for a pair with no closed form.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::EmptyInterval { .. } | Self::BadTolerance { .. } | Self::NotOnSurface { .. } => {
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
            Self::BadTolerance { .. } => "pcurve_tolerance_invalid",
            Self::NotOnSurface { .. } => "pcurve_not_on_surface",
            Self::NoClosedForm { .. } => "pcurve_no_closed_form",
            Self::Inexact { .. } => "pcurve_inexact",
        }
    }
}

impl fmt::Display for CurveKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Line => "line",
            Self::Circle => "circle",
            Self::Ellipse => "ellipse",
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
/// module's table); [`PcurveError::Inexact`] when the closed form fails the lifting check.
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
    let pc = closed_form(curve, surface, &points).ok_or(no_form)?;
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

/// The closed form for the pair, from the curve's sample points; `None` where the pair has
/// none here.
fn closed_form(curve: &Curve, surface: &Surface, points: &[Point3; SAMPLES]) -> Option<Curve2> {
    match surface {
        Surface::Plane(s) => on_plane(curve, s),
        Surface::Cylinder(s) => {
            let axes = Axes::new(s.frame(), s.sense());
            let from_step = |phi: f64| s.from_step(Point2::new(phi, 0.0)).x;
            ruling(curve, &axes, points, from_step, s.domain().u)
        }
        Surface::Cone(s) => {
            let axes = Axes::new(s.frame(), s.sense());
            let from_step = |phi: f64| s.from_step(Point2::new(phi, 0.0)).x;
            ruling(curve, &axes, points, from_step, s.domain().u)
        }
        Surface::Sphere(_) | Surface::Torus(_) | Surface::Nurbs(_) => None,
    }
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
    use scx_curve::{Circle, Ellipse, Line};
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
        // No closed form in this step: the circle about a cylinder's axis, a sphere's
        // equator, a curve on a NURBS surface.
        let ring = Curve::Circle(Circle::new(f, mm(1.0), Sense::Forward).unwrap());
        let e = pcurve_of(&ring, &cyl, span(0.0, 1.0), tol).unwrap_err();
        assert_eq!(
            e,
            PcurveError::NoClosedForm {
                curve: CurveKind::Circle,
                surface: SurfaceKind::Cylinder
            }
        );
        assert_eq!(e.kind(), ErrorKind::Unsupported);
        let sphere = Surface::Sphere(Sphere::new(f, mm(1.0), Sense::Forward).unwrap());
        let e = pcurve_of(&ring, &sphere, span(0.0, 1.0), tol).unwrap_err();
        assert_eq!(e.name(), "pcurve_no_closed_form");
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
