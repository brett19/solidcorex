// Written for solidcorex. Apache-2.0.
//! The images of curves under a [`VolumeMap`] (research 77 §5.2, VM2; 75 §13.34).
//!
//! A line and a circle get an exact image by rule where the rule's kind and frame say so
//! (each a test within [`ALIGNED`](super::ALIGNED), never a sample); every other curve is
//! refitted by [`fit_within`] to the caller's distance, with the certified bound of every
//! span. The map's frame has the axis along its z.
//!
//! | map | curve (in the map's frame) | image |
//! |---|---|---|
//! | bend | line along the axis, `\|v0\| <= pi d` | the line along the axis at `v0 / d` |
//! | bend | line along `u` at `v0` | the line through the axis at `v0 / d` |
//! | bend | line along `v` at `u0 > 0` | the circle of radius `u0` about the axis (the angle is the parameter) |
//! | bend | circle in a plane `v = v0` | the circle in the plane through the axis at `v0 / d` |
//! | twist | line on the axis; a line or a circle in a plane square to it | itself, or turned |
//! | unbend | line along the axis | the line along the axis at `(r, d theta)` |
//! | unbend | line through the axis, one side of it | the line along `u` at `d theta` |
//! | unbend | circle about the axis in a plane square to it | the line along `v` at `u = r` |
//!
//! A rule's image is the same curve at the same parameter ([`CurveParam::Same`]) or the same
//! points at an affine parameter ([`CurveParam::Affine`]).

use std::f64::consts::PI;
use std::sync::Arc;

use scx_curve::{Circle, Curve, Line};
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::trig::Trig;
use scx_num::units::Length;
use scx_nurbs::fit::{CurveFn, Ended, FitAim, FitError, SpanBound, fit_within};

use super::{
    ALIGNED, Bend, Twist, VolumeMap, along_axis, dir_local, dir_world, norm, placed, sign, small,
    to_local, to_world, turned,
};

/// How a curve's image is parametrised: the image at `s` is the map of the source at `t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CurveParam {
    /// `s = t`: the pcurves of the source carry over.
    Same,
    /// `s = scale t + shift`: the same points at another parameter (a negative scale runs the
    /// other way).
    Affine {
        /// The parameter's scale.
        scale: f64,
        /// The parameter's shift.
        shift: f64,
    },
}

impl CurveParam {
    const fn parts(self) -> (f64, f64) {
        match self {
            Self::Same => (1.0, 0.0),
            Self::Affine { scale, shift } => (scale, shift),
        }
    }

    /// `self` first, then `then`.
    fn then(self, then: Self) -> Self {
        let ((k1, c1), (k2, c2)) = (self.parts(), then.parts());
        if matches!((self, then), (Self::Same, Self::Same)) {
            Self::Same
        } else {
            Self::Affine {
                scale: k2 * k1,
                shift: k2 * c1 + c2,
            }
        }
    }

    fn over(self, e: Extent1d) -> Option<Extent1d> {
        let (k, c) = self.parts();
        Extent1d::spanning(k * e.min() + c, k * e.max() + c)
    }
}

/// A curve refitted within a distance, with what proves it.
#[derive(Clone, Debug, PartialEq)]
pub struct Fit {
    /// The fit: a clamped B-spline on the source's span, so the same parameter.
    pub curve: Curve,
    /// The span.
    pub over: Extent1d,
    /// A bound of `|map(source(t)) - curve(t)|` over the span, within the aim.
    pub bound: f64,
    /// Every span of the fit's own bound, in order.
    pub spans: Vec<SpanBound>,
    /// How many fits the search made.
    pub fits: usize,
}

/// The image of a curve over a span.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum CurveImage {
    /// The image by rule: this curve, over this span of its parameter.
    Exact {
        /// The curve.
        curve: Curve,
        /// The span, in the image's parameter.
        over: Extent1d,
        /// The image's parameter against the source's.
        param: CurveParam,
    },
    /// No rule: the certified fit.
    Fitted(Fit),
}

/// Why [`VolumeMap::image_curve`] gives no image.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum CurveNot {
    /// The span is not finite.
    BadSpan,
    /// The curve meets the axis of a bend (or an unbend), where the image is not of the rule's
    /// kind.
    MeetsAxis,
    /// The curve crosses or lies on the unbend's cut at `pi`, where its image splits.
    OnCut,
    /// The curve lies outside the bend's domain `|v| <= pi d`.
    OutsideDomain,
    /// The fit's search ended with its bound past the aim.
    Unproven {
        /// The fit's bound, past the aim.
        bound: f64,
        /// Why the search stopped.
        ended: Ended,
    },
    /// The fitter refused or stopped.
    Fit(FitError),
}

impl From<FitError> for CurveNot {
    fn from(e: FitError) -> Self {
        Self::Fit(e)
    }
}

/// What a rule gives: the image, its span and its parameter, `None` when no rule applies.
type Step = Option<(Curve, Extent1d, CurveParam)>;

/// `map(curve(t))` over a span, for the fit.
struct Mapped<'a> {
    map: &'a VolumeMap,
    curve: &'a Curve,
    over: Extent1d,
    breaks: Vec<f64>,
}

impl CurveFn<3> for Mapped<'_> {
    fn domain(&self) -> Extent1d {
        self.over
    }

    fn breaks(&self) -> &[f64] {
        &self.breaks
    }

    fn eval<T: Trig>(&self, t: T) -> [T; 3] {
        let q = self.map.map(self.curve.point_over(t));
        [q.x, q.y, q.z]
    }
}

impl VolumeMap {
    /// The image of `curve` over `over`: exact by rule for a line or a circle where a rule
    /// names it (the table of the module), else a cubic B-spline within `aim` mm of the map's
    /// image at every parameter, its bound proved ([`fit_within`]). A chain is exact only when
    /// every step is; otherwise the whole chain is fitted in one, not step by step.
    ///
    /// The span is the part of the curve the caller uses (a line or a circle has no end of
    /// its own), and decides a half-plane's side (the unbend picks the nappe the span's
    /// middle is on, as [`VolumeMap::image`]'s `near`).
    ///
    /// # Errors
    ///
    /// [`CurveNot::BadSpan`] for a span not finite; [`CurveNot::MeetsAxis`],
    /// [`CurveNot::OnCut`] and [`CurveNot::OutsideDomain`] where a rule finds the image broken;
    /// [`CurveNot::Unproven`] when the fit ends past `aim` (a curve across the unbend's cut
    /// or axis never gets within); [`CurveNot::Fit`] for the fitter's refusals, the budget and
    /// the stop flag.
    pub fn image_curve(
        &self,
        curve: &Curve,
        over: Extent1d,
        aim: f64,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<CurveImage, CurveNot> {
        if !(over.min().is_finite() && over.max().is_finite()) {
            return Err(CurveNot::BadSpan);
        }
        if let Some((curve, over, param)) = self.exact_curve(curve, over)? {
            return Ok(CurveImage::Exact { curve, over, param });
        }
        let breaks = match curve {
            Curve::Nurbs(n) => n
                .knots()
                .iter()
                .copied()
                .filter(|&k| over.min() < k && k < over.max())
                .collect(),
            _ => Vec::new(),
        };
        let source = Mapped {
            map: self,
            curve,
            over,
            breaks,
        };
        let aim = FitAim {
            within: aim,
            degree: 3,
            start: None,
            end: None,
        };
        let fitted = fit_within(&source, &aim, budget, stop)?;
        if fitted.ended != Ended::Within {
            return Err(CurveNot::Unproven {
                bound: fitted.bound,
                ended: fitted.ended,
            });
        }
        Ok(CurveImage::Fitted(Fit {
            curve: Curve::Nurbs(Arc::new(fitted.curve)),
            over,
            bound: fitted.bound,
            spans: fitted.spans,
            fits: fitted.fits,
        }))
    }

    /// The rule's image of `curve` over `over`, or `None`: every step of a chain by rule.
    fn exact_curve(&self, curve: &Curve, over: Extent1d) -> Result<Step, CurveNot> {
        match self {
            Self::Bend(b) => bend_curve(b, curve, over),
            Self::Unbend(b) => unbend_curve(b, curve, over),
            Self::Twist(t) => twist_curve(t, curve, over),
            Self::Chain(maps) => {
                let mut at = (curve.clone(), over, CurveParam::Same);
                for m in maps.iter() {
                    let Some((c, e, p)) = m.exact_curve(&at.0, at.1)? else {
                        return Ok(None);
                    };
                    at = (c, e, at.2.then(p));
                }
                Ok(Some(at))
            }
        }
    }
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The line through the local point `o` with the local velocity `d`, in the map's frame.
fn line_at(map: &Frame, o: [f64; 3], d: [f64; 3]) -> Option<Curve> {
    Line::new(to_world(map, o), dir_world(map, d))
        .ok()
        .map(Curve::Line)
}

/// The circle about the local point `o`, its local x and y axes `x` and `y` (orthonormal), the
/// radius and sense of `src`.
fn circle_at(
    map: &Frame,
    o: [f64; 3],
    x: [f64; 3],
    y: [f64; 3],
    radius: Length,
    sense: Sense,
) -> Option<Curve> {
    let z = cross(x, y);
    let frame = Frame::new(to_world(map, o), dir_world(map, z), dir_world(map, x))?;
    Circle::new(frame, radius, sense).ok().map(Curve::Circle)
}

fn done(curve: Option<Curve>, over: Extent1d, param: CurveParam) -> Result<Step, CurveNot> {
    let Some(image) = param.over(over) else {
        return Err(CurveNot::BadSpan);
    };
    Ok(curve.map(|c| (c, image, param)))
}

/// A line's local origin, velocity, the velocity's unit, and the scale positions are judged at.
fn line_local(map: &Frame, l: &Line) -> ([f64; 3], [f64; 3], [f64; 3], f64) {
    let o = to_local(map, l.origin());
    let scale = 1.0 + norm(o);
    let d = dir_local(map, l.direction());
    let len = norm(d);
    (o, d, d.map(|c| c / len), scale)
}

fn bend_curve(b: &Bend, curve: &Curve, over: Extent1d) -> Result<Step, CurveNot> {
    let (m, d) = (&b.frame, b.neutral);
    match curve {
        Curve::Line(l) => {
            let (o, dir, n, scale) = line_local(m, l);
            let [ou, ov, ow] = o;
            if along_axis(n, 2) {
                if ov.abs() > PI * d {
                    return Err(CurveNot::OutsideDomain);
                }
                let (s, c) = fmath::sin_cos(ov / d);
                let image = line_at(m, [ou * c, ou * s, ow], dir);
                return done(image, over, CurveParam::Same);
            }
            if along_axis(n, 0) {
                if ov.abs() > PI * d {
                    return Err(CurveNot::OutsideDomain);
                }
                let (s, c) = fmath::sin_cos(ov / d);
                let image = line_at(m, [ou * c, ou * s, ow], [dir[0] * c, dir[0] * s, 0.0]);
                return done(image, over, CurveParam::Same);
            }
            if along_axis(n, 1) {
                if ou <= ALIGNED * scale {
                    return Err(CurveNot::MeetsAxis);
                }
                let speed = dir[1];
                let (shift, scale) = (ov / d, speed / d);
                let image = circle_at(
                    m,
                    [0.0, 0.0, ow],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    Length::from_mm(ou),
                    Sense::Forward,
                );
                return done(image, over, CurveParam::Affine { scale, shift });
            }
            Ok(None)
        }
        Curve::Circle(c) => {
            let (o, z, _) = placed(m, c.frame(), c.radius().mm());
            if !along_axis(z, 1) {
                return Ok(None);
            }
            let [ou, ov, ow] = o;
            if ov.abs() > PI * d {
                return Err(CurveNot::OutsideDomain);
            }
            let (s, co) = fmath::sin_cos(ov / d);
            // A vector of the plane `v = v0`, `(a, _, w)`, goes to `(a cos, a sin, w)`.
            let r = |v: [f64; 3]| [v[0] * co, v[0] * s, v[2]];
            let (x, y) = (dir_local(m, *c.frame().x()), dir_local(m, *c.frame().y()));
            let image = circle_at(m, r([ou, 0.0, ow]), r(x), r(y), c.radius(), c.sense());
            done(image, over, CurveParam::Same)
        }
        _ => Ok(None),
    }
}

fn twist_curve(t: &Twist, curve: &Curve, over: Extent1d) -> Result<Step, CurveNot> {
    let m = &t.frame;
    match curve {
        Curve::Line(l) => {
            let (o, dir, n, scale) = line_local(m, l);
            let [ou, ov, ow] = o;
            if along_axis(n, 2) {
                // On the axis the twist only slides the line along itself.
                return if small(ou, scale) && small(ov, scale) {
                    done(Some(curve.clone()), over, CurveParam::Same)
                } else {
                    Ok(None)
                };
            }
            if small(n[2], 1.0) {
                let (s, c) = fmath::sin_cos(ow * t.rate());
                let image = line_at(m, turned(o, s, c), turned(dir, s, c));
                return done(image, over, CurveParam::Same);
            }
            Ok(None)
        }
        Curve::Circle(c) => {
            let (o, z, _) = placed(m, c.frame(), c.radius().mm());
            if !along_axis(z, 2) {
                return Ok(None);
            }
            let (s, co) = fmath::sin_cos(o[2] * t.rate());
            let (x, y) = (dir_local(m, *c.frame().x()), dir_local(m, *c.frame().y()));
            let image = circle_at(
                m,
                turned(o, s, co),
                turned(x, s, co),
                turned(y, s, co),
                c.radius(),
                c.sense(),
            );
            done(image, over, CurveParam::Same)
        }
        _ => Ok(None),
    }
}

/// The angle `theta` into `(-pi, pi]`.
fn wrapped(theta: f64) -> f64 {
    let t = theta + PI;
    t - 2.0 * PI * (t / (2.0 * PI)).floor() - PI
}

fn unbend_curve(b: &Bend, curve: &Curve, over: Extent1d) -> Result<Step, CurveNot> {
    let (m, d) = (&b.frame, b.neutral);
    match curve {
        Curve::Line(l) => {
            let (o, dir, n, scale) = line_local(m, l);
            let [ox, oy, ow] = o;
            if along_axis(n, 2) {
                let r = fmath::hypot(ox, oy);
                if small(r, scale) {
                    return Err(CurveNot::MeetsAxis);
                }
                let theta = fmath::atan2(oy, ox);
                if PI - theta.abs() <= ALIGNED * PI {
                    return Err(CurveNot::OnCut);
                }
                return done(line_at(m, [r, d * theta, ow], dir), over, CurveParam::Same);
            }
            // A line square to the axis and through it, over one side of it.
            let len = norm(dir);
            if small(n[2], 1.0) && small(ox * n[1] - oy * n[0], scale) {
                let s0 = ox * n[0] + oy * n[1];
                let (sa, sb) = (s0 + len * over.min(), s0 + len * over.max());
                if sa * sb <= 0.0 || small(sa.min(sb).abs(), scale) {
                    return Err(CurveNot::MeetsAxis);
                }
                let side = sign(sa);
                let theta = fmath::atan2(side * n[1], side * n[0]);
                if PI - theta.abs() <= ALIGNED * PI {
                    return Err(CurveNot::OnCut);
                }
                let image = line_at(m, [0.0, d * theta, ow], [1.0, 0.0, 0.0]);
                return done(
                    image,
                    over,
                    CurveParam::Affine {
                        scale: side * len,
                        shift: side * s0,
                    },
                );
            }
            Ok(None)
        }
        Curve::Circle(c) => {
            let rho = c.radius().mm();
            let (o, z, scale) = placed(m, c.frame(), rho);
            let [ox, oy, ow] = o;
            if !(along_axis(z, 2) && small(ox, scale) && small(oy, scale)) {
                return Ok(None);
            }
            let x = dir_local(m, *c.frame().x());
            // `angle(t) = phi0 + eps t`: the circle runs about its own z, which is the map's
            // z or its opposite.
            let eps = sign(z[2]) * c.sense().sign();
            let tm = over.mid();
            let theta_m = wrapped(fmath::atan2(x[1], x[0]) + eps * tm);
            let half = 0.5 * over.length() * (1.0 + ALIGNED);
            if theta_m.abs() + half >= PI {
                return Err(CurveNot::OnCut);
            }
            let image = line_at(m, [rho, d * (theta_m - eps * tm), ow], [0.0, d * eps, 0.0]);
            done(image, over, CurveParam::Same)
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests;
