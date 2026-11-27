// Derived from solidcore `solidcore/crates/solidcore/src/curve/line.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/src/SmLine.cpp`,
// `source/SMLib/inc/SmLine.h`.
// Changes: the analytic line only, as plain data: `Line { origin, direction }` replaces
// `SmLine`'s point, unit vector and scale, its STEP domain and its NURB twin (63 X4, research
// 63 §3.2). The parameter is the pin's STEP parameter (`point + t * vector * scale`, 63 D13),
// over every `t`: a bounded line is a restriction (CU5). A direction shorter than the pin's
// `SM_EFF_ZERO` is refused (`CurveError`) where `from_points` built the X axis over
// `[0, 0]`, and non-finite data is refused where the pin built it. `evaluate_step` becomes
// `point` and the fixed-order `jet::<D>`, one scalar-generic evaluator (`point_over`,
// `jet_over`). The 2-D line (`Line2`) is the same in the plane, where the pin used a 3-D
// line of dimension 2.

use scx_num::domain::ParamDomain;
use scx_num::jet::{CurveJet, CurveJet2};
use scx_num::math::EFF_ZERO;
use scx_num::ring::Ring;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;

use crate::error::{CurveError, Degenerate, Part};

/// The line `origin + t direction`, `t` over every real.
///
/// `direction` is the velocity, kept as given: `t = 0` is `origin`, `t = 1` is
/// `origin + direction`. A line made [`Line::through`] two points runs from the first at
/// `t = 0` to the second at `t = 1`, as the pin's `SmLine(StartPoint, EndPoint)` does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    origin: Point3,
    direction: Vec3,
}

/// The line `origin + t direction` in the plane, `t` over every real.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line2 {
    origin: Point2,
    direction: Vec2,
}

impl Line {
    /// The line through `origin` with velocity `direction`.
    ///
    /// # Errors
    ///
    /// [`CurveError::NotFinite`] for an infinite or NaN coordinate;
    /// [`Degenerate::Direction`] for a direction shorter than
    /// [`EFF_ZERO`](scx_num::math::EFF_ZERO).
    pub fn new(origin: Point3, direction: Vec3) -> crate::Result<Self> {
        if !origin.is_finite() {
            return Err(CurveError::NotFinite(Part::Origin));
        }
        if !direction.is_finite() {
            return Err(CurveError::NotFinite(Part::Direction));
        }
        direction_ok(direction.length())?;
        Ok(Self { origin, direction })
    }

    /// The line from `start` at `t = 0` to `end` at `t = 1`.
    ///
    /// # Errors
    ///
    /// As [`Line::new`] with the direction `end - start` (which overflows to non-finite for
    /// points too far apart).
    pub fn through(start: Point3, end: Point3) -> crate::Result<Self> {
        if !start.is_finite() {
            return Err(CurveError::NotFinite(Part::Origin));
        }
        Self::new(start, end - start)
    }

    /// The point at `t = 0`.
    #[must_use]
    pub const fn origin(&self) -> Point3 {
        self.origin
    }

    /// The velocity: the first derivative everywhere.
    #[must_use]
    pub const fn direction(&self) -> Vec3 {
        self.direction
    }

    /// Every real.
    #[must_use]
    pub const fn domain(&self) -> ParamDomain {
        ParamDomain::Unbounded
    }

    /// The point at `t`.
    #[must_use]
    pub fn point(&self, t: f64) -> Point3 {
        self.point_over(t)
    }

    /// The point and the first `D` derivatives at `t`. A line is smooth, so `side` changes
    /// nothing.
    #[must_use]
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet<D> {
        let _ = side;
        self.jet_over(t)
    }

    /// The point at `t` in the scalar `T`: `origin + direction t` (an enclosure in
    /// [`Ivl`](scx_num::interval::Ivl)).
    #[must_use]
    pub fn point_over<T: Ring>(&self, t: T) -> Point3<T> {
        self.origin.map(T::lift) + self.direction.map(T::lift) * t
    }

    /// The point and the first `D` derivatives at `t` in the scalar `T`: the direction,
    /// then zeros.
    #[must_use]
    pub fn jet_over<T: Ring, const D: usize>(&self, t: T) -> CurveJet<D, T> {
        let d1 = self.direction.map(T::lift);
        let zero = Vec3::splat(T::lift(0.0));
        CurveJet {
            point: self.point_over(t),
            d: core::array::from_fn(|k| if k == 0 { d1 } else { zero }),
        }
    }
}

impl Line2 {
    /// The line through `origin` with velocity `direction`.
    ///
    /// # Errors
    ///
    /// As [`Line::new`].
    pub fn new(origin: Point2, direction: Vec2) -> crate::Result<Self> {
        if !(origin.x.is_finite() && origin.y.is_finite()) {
            return Err(CurveError::NotFinite(Part::Origin));
        }
        if !(direction.x.is_finite() && direction.y.is_finite()) {
            return Err(CurveError::NotFinite(Part::Direction));
        }
        direction_ok(direction.length())?;
        Ok(Self { origin, direction })
    }

    /// The line from `start` at `t = 0` to `end` at `t = 1`.
    ///
    /// # Errors
    ///
    /// As [`Line::through`].
    pub fn through(start: Point2, end: Point2) -> crate::Result<Self> {
        if !(start.x.is_finite() && start.y.is_finite()) {
            return Err(CurveError::NotFinite(Part::Origin));
        }
        Self::new(start, end - start)
    }

    /// The point at `t = 0`.
    #[must_use]
    pub const fn origin(&self) -> Point2 {
        self.origin
    }

    /// The velocity: the first derivative everywhere.
    #[must_use]
    pub const fn direction(&self) -> Vec2 {
        self.direction
    }

    /// Every real.
    #[must_use]
    pub const fn domain(&self) -> ParamDomain {
        ParamDomain::Unbounded
    }

    /// The point at `t`.
    #[must_use]
    pub fn point(&self, t: f64) -> Point2 {
        self.point_over(t)
    }

    /// The point and the first `D` derivatives at `t`; `side` changes nothing.
    #[must_use]
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet2<D> {
        let _ = side;
        self.jet_over(t)
    }

    /// The point at `t` in the scalar `T`.
    #[must_use]
    pub fn point_over<T: Ring>(&self, t: T) -> Point2<T> {
        self.origin.map(T::lift) + self.direction.map(T::lift) * t
    }

    /// The point and the first `D` derivatives at `t` in the scalar `T`.
    #[must_use]
    pub fn jet_over<T: Ring, const D: usize>(&self, t: T) -> CurveJet2<D, T> {
        let d1 = self.direction.map(T::lift);
        let zero = Vec2::new(T::lift(0.0), T::lift(0.0));
        CurveJet2 {
            point: self.point_over(t),
            d: core::array::from_fn(|k| if k == 0 { d1 } else { zero }),
        }
    }
}

/// The pin's degenerate-line test (`|end - start| < SM_EFF_ZERO`); a NaN length cannot
/// reach here (the coordinates are finite) and an overflowed one is long enough.
fn direction_ok(length: f64) -> crate::Result<()> {
    if length < EFF_ZERO {
        return Err(CurveError::Degenerate(Degenerate::Direction { length }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::truth::{U, holds, near};
    use scx_num::hash::Rng;
    use scx_num::interval::Ivl;
    use scx_testkit::analytic;

    fn lines(seed: u64) -> Vec<Line> {
        let mut rng = Rng::new(seed);
        let mut out = Vec::new();
        for scale in [1.0, 1e-3, 1e4] {
            for _ in 0..40 {
                let o = core::array::from_fn(|_| scale * rng.next_signed_unit());
                let d = core::array::from_fn(|_| scale * rng.next_signed_unit());
                out.push(Line::new(Point3::from(o), Vec3::from(d)).unwrap());
            }
        }
        out
    }

    fn params(rng: &mut Rng) -> Vec<f64> {
        let mut t = vec![0.0, -0.0, 1.0, -1.0, 0.5, 1e-300, 1e6, -3.75e5];
        t.extend((0..20).map(|_| 10.0 * rng.next_signed_unit()));
        t
    }

    fn truth(l: &Line) -> analytic::Line {
        analytic::Line {
            origin: l.origin().to_array(),
            direction: l.direction().to_array(),
        }
    }

    #[test]
    fn points_are_within_two_roundings_of_the_exact_point() {
        let mut rng = Rng::new(7);
        let mut n = 0;
        for l in lines(1) {
            let r = truth(&l);
            for t in params(&mut rng) {
                let p = l.point(t).to_array();
                let want = r.point_exact(t).unwrap();
                let derivs = r.derivs(t, 0);
                for i in 0..3 {
                    let scale =
                        l.origin().to_array()[i].abs() + (l.direction().to_array()[i] * t).abs();
                    assert!(near(p[i], derivs[0][i], scale, 2.0), "{l:?} {t}");
                    assert!((p[i] - want.rounded[i]).abs() <= 2.0 * U * scale + f64::MIN_POSITIVE);
                }
                n += 1;
            }
        }
        assert!(n > 3000);
    }

    #[test]
    fn jets_are_the_direction_then_zeros() {
        let mut rng = Rng::new(8);
        for l in lines(2) {
            let r = truth(&l);
            for t in params(&mut rng) {
                for side in [Approach::Below, Approach::Above] {
                    let j = l.jet::<4>(t, side);
                    assert_eq!(j.point, l.point(t));
                    let want = r.derivs(t, 4);
                    for k in 0..4 {
                        let got = j.d[k].to_array();
                        for i in 0..3 {
                            assert_eq!(Dd::new(got[i]), want[k + 1][i]);
                        }
                    }
                }
                let j0 = l.jet::<0>(t, Approach::Above);
                assert_eq!(j0.point, l.point(t));
            }
        }
    }

    use scx_testkit::dd::Dd;

    #[test]
    fn interval_evaluation_holds_the_truth() {
        let mut rng = Rng::new(9);
        for l in lines(3) {
            let r = truth(&l);
            for t in params(&mut rng) {
                let p = l.point_over(Ivl::from(t)).to_array();
                let want = r.derivs(t, 0);
                for i in 0..3 {
                    assert!(holds(p[i], want[0][i]), "{l:?} {t}");
                }
                // A wide parameter interval holds every point over it.
                let (lo, hi) = (t, t + 0.5);
                let w = l.point_over(Ivl::new(lo, hi)).to_array();
                for s in 0..=8 {
                    let ts = lo + (hi - lo) * f64::from(s) / 8.0;
                    let want = r.derivs(ts, 0);
                    for i in 0..3 {
                        assert!(holds(w[i], want[0][i]));
                    }
                }
                let j = l.jet_over::<Ivl, 2>(Ivl::from(t));
                assert_eq!(j.d[1].to_array(), [Ivl::from(0.0); 3]);
            }
        }
    }

    #[test]
    fn construction_refuses_what_the_pin_built_degenerate() {
        let o = Point3::new(1.0, 2.0, 3.0);
        let err = |r: crate::Result<Line>| r.unwrap_err().name();
        assert_eq!(err(Line::new(o, Vec3::ZERO)), "degenerate_direction");
        assert_eq!(
            err(Line::new(o, Vec3::new(0.0, 9e-13, 0.0))),
            "degenerate_direction"
        );
        assert!(Line::new(o, Vec3::new(0.0, 1e-12, 0.0)).is_ok());
        assert_eq!(
            Line::new(Point3::new(f64::NAN, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0)),
            Err(CurveError::NotFinite(Part::Origin))
        );
        assert_eq!(
            Line::new(o, Vec3::new(f64::INFINITY, 0.0, 0.0)),
            Err(CurveError::NotFinite(Part::Direction))
        );
        assert_eq!(
            Line::through(o, Point3::new(f64::MAX, 0.0, 0.0)).map(|l| l.direction().x),
            Ok(f64::MAX - 1.0)
        );
        assert_eq!(
            Line::through(
                Point3::new(-f64::MAX, 0.0, 0.0),
                Point3::new(f64::MAX, 0.0, 0.0)
            ),
            Err(CurveError::NotFinite(Part::Direction))
        );
        assert_eq!(err(Line::through(o, o)), "degenerate_direction");
        let l = Line::through(o, Point3::new(4.0, 6.0, 8.0)).unwrap();
        assert_eq!(l.point(0.0), o);
        assert_eq!(l.point(1.0), Point3::new(4.0, 6.0, 8.0));
        assert_eq!(l.domain(), ParamDomain::Unbounded);
        assert!(l.point(f64::NAN).x.is_nan());
    }

    #[test]
    fn the_plane_line_is_the_space_line_at_z_zero() {
        let mut rng = Rng::new(10);
        for _ in 0..50 {
            let o = Point2::new(rng.next_signed_unit(), rng.next_signed_unit());
            let d = Vec2::new(rng.next_signed_unit(), rng.next_signed_unit());
            let (Ok(l2), Ok(l3)) = (Line2::new(o, d), Line::new(o.into(), d.into())) else {
                continue;
            };
            for t in params(&mut rng) {
                let (j2, j3) = (
                    l2.jet::<3>(t, Approach::Below),
                    l3.jet::<3>(t, Approach::Below),
                );
                assert_eq!(Point3::from(j2.point), j3.point);
                for k in 0..3 {
                    assert_eq!(Vec3::from(j2.d[k]), j3.d[k]);
                }
                let i = l2.point_over(Ivl::from(t));
                assert!(i.x.lo <= j2.point.x && j2.point.x <= i.x.hi);
            }
        }
        assert_eq!(
            Line2::new(Point2::ORIGIN, Vec2::ZERO).unwrap_err().name(),
            "degenerate_direction"
        );
        assert_eq!(
            Line2::through(Point2::new(f64::NAN, 0.0), Point2::ORIGIN),
            Err(CurveError::NotFinite(Part::Origin))
        );
    }
}
