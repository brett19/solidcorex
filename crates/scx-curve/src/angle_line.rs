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
        if self.arcs.is_turn() {
            return ParamDomain::Unbounded;
        }
        let s = self.sense.sign();
        let ends = [
            (-core::f64::consts::FRAC_PI_2 - self.start) * s,
            (core::f64::consts::FRAC_PI_2 - self.start) * s,
        ];
        Extent1d::spanning(ends[0], ends[1]).map_or(ParamDomain::Unbounded, ParamDomain::Bounded)
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
        let s = self.sense.sign();
        let phi = l(self.start) + l(s) * t;
        // The angle falls as `t` grows when reversed, so `t`'s side is the angle's other one.
        let angle_side = match (self.sense, side) {
            (Sense::Forward, side) => side,
            (_, Approach::Above) => Approach::Below,
            (_, Approach::Below) => Approach::Above,
        };
        let (a, da) = self.arcs.parameter_jet::<T, D>(phi, angle_side);
        let b = l(self.other) + l(self.rate) * t;
        let mut sign = 1.0;
        let mut d = [Vec2::new(l(0.0), l(0.0)); D];
        for (n, (out, &da)) in d.iter_mut().zip(da.iter()).enumerate() {
            sign *= s;
            let db = l(if n == 0 { self.rate } else { 0.0 });
            let da = da * l(sign);
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
}
