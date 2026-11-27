// Written for solidcorex. Apache-2.0.
//! [`AngleLine2`]: a straight line in angle and height, drawn in the NURBS-compatible uv
//! (research 63 D13, §4.11).
//!
//! In that uv an angle direction is the parameter of quarter arcs ([`Arcs`]), not the
//! angle, so a circle about a surface's axis (or a sphere's meridian, a torus's tube circle)
//! is not a straight line in uv. This kind is that curve, exactly: one coordinate is
//! `arcs.parameter(start + s t)` (`s = ±1`), the other `other + rate t`. The parameter `t`
//! is the 3-D circle's own angle (63 X11), so the pcurve and the edge share a parameter, as
//! STEP and OCCT expect of a pcurve.
//!
//! The pin has no such kind: its surfaces' pcurves are in a radians uv, where these are
//! degree-1 splines affine in the circle's angle (63 §4.11). OCCT's surfaces are also
//! parametrised in radians, so there a pcurve of this shape is a `Geom2d_Line`; this kind
//! is that line seen through the arc parameter map of the NURBS uv (63 D13).
//!
//! The map is `C^1` at an arc break and its second derivative jumps there, so a jet takes
//! the side the caller asks for, as a NURBS curve's does.
//!
//! [`AngleWave2`] is the same angle coordinate with a sinusoid in the other: `u` the arc
//! parameter of `start + s t`, `v = mean + c cos t + d sin t`. It is a plane's oblique
//! section of a cylinder (an ellipse) in the cylinder's uv, `t` the ellipse's own angle:
//! the ellipse projects along the axis onto the cylinder's circle with conjugate, hence
//! perpendicular, semi-diameters of length `r`, so its azimuth is `start + s t` exactly,
//! and its height is affine in `cos t` and `sin t` (63 §4.14). The pin's pcurve of the same
//! section (WR-680) is a rational curve in the azimuth, not in the edge's parameter.

use scx_num::domain::ParamDomain;
use scx_num::extent1d::Extent1d;
use scx_num::frame::Sense;
use scx_num::jet::CurveJet2;
use scx_num::trig::Trig;
use scx_num::vector2d::{Point2, Vec2};
use scx_nurbs::Approach;

use crate::arcs::Arcs;
use crate::error::{CurveError, Part};

/// Which uv coordinate is the angle's arc parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AngleAxis {
    /// `u` is the angle's parameter, `v` affine in `t`.
    U,
    /// `v` is the angle's parameter, `u` affine in `t`.
    V,
}

/// A line in angle and height: one coordinate the [`Arcs`] parameter of the angle
/// `start + s t`, the other `other + rate t`; see the [module](self) docs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AngleLine2 {
    arcs: Arcs,
    axis: AngleAxis,
    start: f64,
    sense: Sense,
    other: f64,
    rate: f64,
}

impl AngleLine2 {
    /// The line whose `axis` coordinate is `arcs.parameter(start + s t)`, `s` `+1` for
    /// [`Sense::Forward`] and `-1` reversed, and whose other coordinate is `other + rate t`.
    ///
    /// # Errors
    ///
    /// [`CurveError::NotFinite`] when `start` or `other` ([`Part::Origin`]) or `rate`
    /// ([`Part::Direction`]) is not finite.
    pub fn new(
        arcs: Arcs,
        axis: AngleAxis,
        start: f64,
        sense: Sense,
        other: f64,
        rate: f64,
    ) -> crate::Result<Self> {
        if !start.is_finite() || !other.is_finite() {
            return Err(CurveError::NotFinite(Part::Origin));
        }
        if !rate.is_finite() {
            return Err(CurveError::NotFinite(Part::Direction));
        }
        Ok(Self {
            arcs,
            axis,
            start,
            sense,
            other,
            rate,
        })
    }

    /// The angle direction.
    #[must_use]
    pub const fn arcs(&self) -> Arcs {
        self.arcs
    }

    /// Which coordinate is the angle's parameter.
    #[must_use]
    pub const fn axis(&self) -> AngleAxis {
        self.axis
    }

    /// The angle at `t = 0`.
    #[must_use]
    pub const fn start(&self) -> f64 {
        self.start
    }

    /// Whether the angle grows with `t` ([`Sense::Forward`]) or falls.
    #[must_use]
    pub const fn sense(&self) -> Sense {
        self.sense
    }

    /// The other coordinate at `t = 0`.
    #[must_use]
    pub const fn other(&self) -> f64 {
        self.other
    }

    /// The other coordinate's rate in `t`.
    #[must_use]
    pub const fn rate(&self) -> f64 {
        self.rate
    }

    /// The angle at `t`.
    #[must_use]
    pub fn angle(&self, t: f64) -> f64 {
        self.start + self.sense.sign() * t
    }

    /// Every `t` on a turn; on the half turn the `t` whose angle is in `[-pi/2, pi/2]`.
    #[must_use]
    pub fn domain(&self) -> ParamDomain {
        angle_domain(self.arcs, self.start, self.sense)
    }

    /// The point at `t`, on the arc above the angle at a break.
    #[must_use]
    pub fn point(&self, t: f64) -> Point2 {
        self.jet_over::<f64, 0>(t, Approach::Above).point
    }

    /// The point and the first `D` derivatives at `t`, from `side` at an arc break.
    #[must_use]
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet2<D> {
        self.jet_over(t, side)
    }

    /// The point at `t` in the scalar `T`.
    #[must_use]
    pub fn point_over<T: Trig>(&self, t: T) -> Point2<T> {
        self.jet_over::<T, 0>(t, Approach::Above).point
    }

    /// [`Self::point`] at each of `ts`, into `out`, zipped.
    pub(crate) fn fill(&self, ts: &[f64], out: &mut [Point2]) {
        for (p, &t) in out.iter_mut().zip(ts) {
            *p = self.point(t);
        }
    }

    /// The point and the first `D` derivatives at `t` in the scalar `T`, from `side` at an
    /// arc break: the angle's derivatives are `s^n` times the arc map's (the chain rule
    /// through `start + s t`), the other coordinate's `rate` and then zero.
    #[must_use]
    pub fn jet_over<T: Trig, const D: usize>(&self, t: T, side: Approach) -> CurveJet2<D, T> {
        let l = T::lift;
        let (a, da) = angle_jet::<T, D>(self.arcs, self.start, self.sense, t, side);
        let b = l(self.other) + l(self.rate) * t;
        let mut d = [Vec2::new(l(0.0), l(0.0)); D];
        for (n, (out, &da)) in d.iter_mut().zip(da.iter()).enumerate() {
            let db = l(if n == 0 { self.rate } else { 0.0 });
            *out = match self.axis {
                AngleAxis::U => Vec2::new(da, db),
                AngleAxis::V => Vec2::new(db, da),
            };
        }
        let point = match self.axis {
            AngleAxis::U => Point2::new(a, b),
            AngleAxis::V => Point2::new(b, a),
        };
        CurveJet2 { point, d }
    }

    /// The same line run the other way, `C'(t) = C(-t)`: the sense and the rate reversed.
    /// Exact: `start + (-s)(t)` is `start + s(-t)` to the bit.
    #[must_use]
    pub fn reversed(&self) -> Self {
        Self {
            sense: self.sense.reversed(),
            rate: -self.rate,
            ..*self
        }
    }
}

/// The domain of an angle coordinate `start + s t` on `arcs`: every `t` on a turn; on the
/// half turn the `t` whose angle is in `[-pi/2, pi/2]`.
fn angle_domain(arcs: Arcs, start: f64, sense: Sense) -> ParamDomain {
    if arcs.is_turn() {
        return ParamDomain::Unbounded;
    }
    let s = sense.sign();
    let ends = [
        (-core::f64::consts::FRAC_PI_2 - start) * s,
        (core::f64::consts::FRAC_PI_2 - start) * s,
    ];
    Extent1d::spanning(ends[0], ends[1]).map_or(ParamDomain::Unbounded, ParamDomain::Bounded)
}

/// The angle coordinate `arcs.parameter(start + s t)` and its first `D` derivatives in `t`,
/// from `side` at an arc break: the arc map's derivatives in the angle times `s^n` (the
/// chain rule through `start + s t`).
fn angle_jet<T: Trig, const D: usize>(
    arcs: Arcs,
    start: f64,
    sense: Sense,
    t: T,
    side: Approach,
) -> (T, [T; D]) {
    let l = T::lift;
    let s = sense.sign();
    let phi = l(start) + l(s) * t;
    // The angle falls as `t` grows when reversed, so `t`'s side is the angle's other one.
    let angle_side = match (sense, side) {
        (Sense::Forward, side) => side,
        (_, Approach::Above) => Approach::Below,
        (_, Approach::Below) => Approach::Above,
    };
    let (a, mut da) = arcs.parameter_jet::<T, D>(phi, angle_side);
    let mut sign = 1.0;
    for x in &mut da {
        sign *= s;
        *x = *x * l(sign);
    }
    (a, da)
}

/// A plane's section of a cylinder in its uv: `u` the [`Arcs`] parameter of the angle
/// `start + s t`, `v = mean + c cos t + d sin t`; see the [module](self) docs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AngleWave2 {
    arcs: Arcs,
    start: f64,
    sense: Sense,
    mean: f64,
    amp_cos: f64,
    amp_sin: f64,
}

impl AngleWave2 {
    /// The curve whose `u` is `arcs.parameter(start + s t)` (`s` `+1` for
    /// [`Sense::Forward`], `-1` reversed) and whose `v` is
    /// `mean + amp_cos cos t + amp_sin sin t`.
    ///
    /// # Errors
    ///
    /// [`CurveError::NotFinite`] when `start` or `mean` ([`Part::Origin`]) or an amplitude
    /// ([`Part::Direction`]) is not finite.
    pub fn new(
        arcs: Arcs,
        start: f64,
        sense: Sense,
        mean: f64,
        amp_cos: f64,
        amp_sin: f64,
    ) -> crate::Result<Self> {
        if !start.is_finite() || !mean.is_finite() {
            return Err(CurveError::NotFinite(Part::Origin));
        }
        if !amp_cos.is_finite() || !amp_sin.is_finite() {
            return Err(CurveError::NotFinite(Part::Direction));
        }
        Ok(Self {
            arcs,
            start,
            sense,
            mean,
            amp_cos,
            amp_sin,
        })
    }

    /// The angle direction.
    #[must_use]
    pub const fn arcs(&self) -> Arcs {
        self.arcs
    }

    /// The angle at `t = 0`.
    #[must_use]
    pub const fn start(&self) -> f64 {
        self.start
    }

    /// Whether the angle grows with `t` ([`Sense::Forward`]) or falls.
    #[must_use]
    pub const fn sense(&self) -> Sense {
        self.sense
    }

    /// `v`'s mean, the height where the plane meets the axis.
    #[must_use]
    pub const fn mean(&self) -> f64 {
        self.mean
    }

    /// `v`'s coefficients of `cos t` and `sin t`.
    #[must_use]
    pub const fn amplitudes(&self) -> (f64, f64) {
        (self.amp_cos, self.amp_sin)
    }

    /// The angle at `t`.
    #[must_use]
    pub fn angle(&self, t: f64) -> f64 {
        self.start + self.sense.sign() * t
    }

    /// As [`AngleLine2::domain`]: every `t` on a turn.
    #[must_use]
    pub fn domain(&self) -> ParamDomain {
        angle_domain(self.arcs, self.start, self.sense)
    }

    /// The point at `t`, on the arc above the angle at a break.
    #[must_use]
    pub fn point(&self, t: f64) -> Point2 {
        self.jet_over::<f64, 0>(t, Approach::Above).point
    }

    /// The point and the first `D` derivatives at `t`, from `side` at an arc break.
    #[must_use]
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet2<D> {
        self.jet_over(t, side)
    }

    /// The point at `t` in the scalar `T`.
    #[must_use]
    pub fn point_over<T: Trig>(&self, t: T) -> Point2<T> {
        self.jet_over::<T, 0>(t, Approach::Above).point
    }

    /// [`Self::point`] at each of `ts`, into `out`, zipped.
    pub(crate) fn fill(&self, ts: &[f64], out: &mut [Point2]) {
        for (p, &t) in out.iter_mut().zip(ts) {
            *p = self.point(t);
        }
    }

    /// The point and the first `D` derivatives at `t` in the scalar `T`, from `side` at an
    /// arc break (`v` is smooth): `u` as [`AngleLine2`]'s angle coordinate, `v`'s `n`-th
    /// derivative `c cos(t + n pi/2) + d sin(t + n pi/2)`.
    #[must_use]
    pub fn jet_over<T: Trig, const D: usize>(&self, t: T, side: Approach) -> CurveJet2<D, T> {
        let l = T::lift;
        let (u, du) = angle_jet::<T, D>(self.arcs, self.start, self.sense, t, side);
        let (sin, cos) = t.sin_cos();
        let (c, d) = (l(self.amp_cos), l(self.amp_sin));
        let v = l(self.mean) + c * cos + d * sin;
        let mut out = [Vec2::new(l(0.0), l(0.0)); D];
        // (cos, sin) of t + n pi/2: each derivative turns the pair a quarter forward.
        let (mut cn, mut sn) = (cos, sin);
        for (o, &du) in out.iter_mut().zip(du.iter()) {
            (cn, sn) = (l(0.0) - sn, cn);
            *o = Vec2::new(du, c * cn + d * sn);
        }
        CurveJet2 {
            point: Point2::new(u, v),
            d: out,
        }
    }

    /// The same curve run the other way, `C'(t) = C(-t)`: the sense and the sine's
    /// amplitude reversed. Exact: `start + (-s) t` is `start + s (-t)` and
    /// `sin(-t) = -sin t` to the bit (the sine is odd in `fmath`).
    #[must_use]
    pub fn reversed(&self) -> Self {
        Self {
            sense: self.sense.reversed(),
            amp_sin: -self.amp_sin,
            ..*self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::f64::consts::{FRAC_PI_2, PI};
    use scx_num::interval::Ivl;

    fn lines() -> Vec<AngleLine2> {
        let mut out = Vec::new();
        for arcs in [Arcs::turn(1.0), Arcs::turn(2.0 * PI * 3.0), Arcs::half()] {
            for axis in [AngleAxis::U, AngleAxis::V] {
                for sense in [Sense::Forward, Sense::Reversed] {
                    out.push(AngleLine2::new(arcs, axis, 0.3, sense, 1.5, -0.25).unwrap());
                }
            }
        }
        out
    }

    #[test]
    fn the_angle_coordinate_is_the_arc_parameter() {
        for c in lines() {
            for i in -20..=20 {
                let t = f64::from(i) * 0.07;
                if let ParamDomain::Bounded(e) = c.domain()
                    && !e.contains_value(t, 0.0)
                {
                    continue;
                }
                let p = c.point(t);
                let (a, b) = match c.axis() {
                    AngleAxis::U => (p.x, p.y),
                    AngleAxis::V => (p.y, p.x),
                };
                let want = c.arcs().parameter(c.angle(t));
                assert!((a - want).abs() < 1e-14 * (1.0 + want.abs()), "{c:?} {t}");
                assert!((c.arcs().angle(a) - c.angle(t)).abs() < 1e-14 * (1.0 + t.abs()));
                assert_eq!(b, 1.5 - 0.25 * t);
            }
        }
    }

    #[test]
    fn derivatives_match_differences_on_both_sides() {
        let h = 1e-6;
        for c in lines() {
            for t in [0.11, -0.4, FRAC_PI_2 - 0.35, 0.9] {
                let j: CurveJet2<3> = c.jet(t, Approach::Above);
                for n in 0..3 {
                    let lo: CurveJet2<3> = c.jet(t - h, Approach::Above);
                    let hi: CurveJet2<3> = c.jet(t + h, Approach::Above);
                    let (fl, fh) = if n == 0 {
                        (
                            lo.point - Point2::new(0.0, 0.0),
                            hi.point - Point2::new(0.0, 0.0),
                        )
                    } else {
                        (lo.d[n - 1], hi.d[n - 1])
                    };
                    let fd = (fh - fl) * (0.5 / h);
                    let scale = 1.0 + j.d[n].length();
                    assert!((fd - j.d[n]).length() < 1e-5 * scale, "{c:?} {t} {n}");
                }
            }
        }
    }

    #[test]
    fn a_break_has_one_tangent_and_two_curvatures() {
        // Angle pi/2 is a break of the turn: reached at t = pi/2 - 0.3, forward.
        let c =
            AngleLine2::new(Arcs::turn(1.0), AngleAxis::U, 0.3, Sense::Forward, 0.0, 0.0).unwrap();
        let t = FRAC_PI_2 - 0.3;
        let below: CurveJet2<2> = c.jet(t, Approach::Below);
        let above: CurveJet2<2> = c.jet(t, Approach::Above);
        assert!((below.point - above.point).length() < 1e-15);
        assert!((below.d[0] - above.d[0]).length() < 1e-14);
        assert!(below.d[1].x * above.d[1].x < 0.0, "{below:?} {above:?}");
        // Reversed, the side in t is the other side in the angle.
        let r = c.reversed();
        let rb: CurveJet2<2> = r.jet(-t, Approach::Below);
        assert!((rb.d[1].x - above.d[1].x).abs() < 1e-12, "{rb:?} {above:?}");
    }

    #[test]
    fn reversed_runs_back() {
        for c in lines() {
            let r = c.reversed();
            for t in [0.0, 0.2, -0.7] {
                assert_eq!(r.point(-t), c.point(t));
            }
        }
    }

    #[test]
    fn intervals_enclose_the_values() {
        for c in lines() {
            for (lo, hi) in [(0.1, 0.2), (-0.5, 1.4), (1.0, 1.0)] {
                let e = c.point_over(Ivl::new(lo, hi));
                for i in 0..=16 {
                    let t = lo + (hi - lo) * f64::from(i) / 16.0;
                    let p = c.point(t);
                    assert!(
                        e.x.lo <= p.x && p.x <= e.x.hi && e.y.lo <= p.y && p.y <= e.y.hi,
                        "{c:?} {t} {e:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn non_finite_data_is_refused() {
        let a = Arcs::turn(1.0);
        let f = Sense::Forward;
        assert!(AngleLine2::new(a, AngleAxis::U, f64::NAN, f, 0.0, 0.0).is_err());
        assert!(AngleLine2::new(a, AngleAxis::U, 0.0, f, f64::INFINITY, 0.0).is_err());
        assert!(AngleLine2::new(a, AngleAxis::U, 0.0, f, 0.0, f64::NAN).is_err());
    }

    fn waves() -> Vec<AngleWave2> {
        let mut out = Vec::new();
        for arcs in [Arcs::turn(1.0), Arcs::turn(2.0 * PI * 3.0)] {
            for sense in [Sense::Forward, Sense::Reversed] {
                out.push(AngleWave2::new(arcs, 0.3, sense, 1.5, -0.25, 0.75).unwrap());
            }
        }
        out
    }

    #[test]
    fn a_wave_is_the_arc_parameter_and_a_sinusoid() {
        for c in waves() {
            for i in -40..=40 {
                let t = f64::from(i) * 0.17;
                let p = c.point(t);
                let want = c.arcs().parameter(c.angle(t));
                assert!((p.x - want).abs() < 1e-14 * (1.0 + want.abs()), "{c:?} {t}");
                let v = 1.5 - 0.25 * scx_num::fmath::cos(t) + 0.75 * scx_num::fmath::sin(t);
                assert!((p.y - v).abs() < 1e-15 * 4.0, "{c:?} {t}");
            }
        }
    }

    #[test]
    fn a_wave_s_derivatives_match_differences() {
        let h = 1e-6;
        for c in waves() {
            for t in [0.11, -0.4, FRAC_PI_2 - 0.35, 0.9, 4.0] {
                let j: CurveJet2<3> = c.jet(t, Approach::Above);
                for n in 0..3 {
                    let lo: CurveJet2<3> = c.jet(t - h, Approach::Above);
                    let hi: CurveJet2<3> = c.jet(t + h, Approach::Above);
                    let (fl, fh) = if n == 0 {
                        (
                            lo.point - Point2::new(0.0, 0.0),
                            hi.point - Point2::new(0.0, 0.0),
                        )
                    } else {
                        (lo.d[n - 1], hi.d[n - 1])
                    };
                    let fd = (fh - fl) * (0.5 / h);
                    let scale = 1.0 + j.d[n].length();
                    assert!((fd - j.d[n]).length() < 1e-5 * scale, "{c:?} {t} {n}");
                }
            }
        }
    }

    #[test]
    fn a_wave_shares_the_line_s_angle_coordinate() {
        // The helper both kinds use: the u of a wave is the u of the line, to the bit.
        for sense in [Sense::Forward, Sense::Reversed] {
            let a = Arcs::turn(2.0);
            let w = AngleWave2::new(a, 0.3, sense, 0.0, 1.0, 1.0).unwrap();
            let l = AngleLine2::new(a, AngleAxis::U, 0.3, sense, 0.0, 0.0).unwrap();
            for t in [0.0, 1.27, -2.5, FRAC_PI_2 - 0.3] {
                for side in [Approach::Below, Approach::Above] {
                    let (jw, jl): (CurveJet2<2>, CurveJet2<2>) = (w.jet(t, side), l.jet(t, side));
                    assert_eq!(jw.point.x, jl.point.x);
                    assert_eq!(jw.d[0].x, jl.d[0].x);
                    assert_eq!(jw.d[1].x, jl.d[1].x);
                }
            }
        }
    }

    #[test]
    fn a_wave_reversed_runs_back() {
        for c in waves() {
            let r = c.reversed();
            for t in [0.0, 0.2, -0.7, 3.9] {
                assert_eq!(r.point(-t), c.point(t));
            }
            assert_eq!(r.reversed(), c);
        }
    }

    #[test]
    fn a_wave_s_intervals_enclose_the_values() {
        for c in waves() {
            for (lo, hi) in [(0.1, 0.2), (-0.5, 1.4), (1.0, 1.0), (-4.0, 4.0)] {
                let e = c.point_over(Ivl::new(lo, hi));
                for i in 0..=16 {
                    let t = lo + (hi - lo) * f64::from(i) / 16.0;
                    let p = c.point(t);
                    assert!(
                        e.x.lo <= p.x && p.x <= e.x.hi && e.y.lo <= p.y && p.y <= e.y.hi,
                        "{c:?} {t} {e:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_wave_refuses_non_finite_data() {
        let a = Arcs::turn(1.0);
        let f = Sense::Forward;
        assert!(AngleWave2::new(a, f64::NAN, f, 0.0, 0.0, 0.0).is_err());
        assert!(AngleWave2::new(a, 0.0, f, f64::INFINITY, 0.0, 0.0).is_err());
        assert!(AngleWave2::new(a, 0.0, f, 0.0, f64::NAN, 0.0).is_err());
        assert!(AngleWave2::new(a, 0.0, f, 0.0, 0.0, f64::NEG_INFINITY).is_err());
    }
}
