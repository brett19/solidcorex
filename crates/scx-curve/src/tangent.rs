// Derived from solidcore `solidcore/crates/solidcore/src/nurbs/b_spline_curve/p1.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/src/SmBSplineCurve.cpp`.
// Changes: `Evaluate`'s `bNonZeroTangents` flag is dropped (63 X5, A2): derivatives stay the
// true ones, and the limiting direction is its own call. The pin's floor for a vanishing first
// derivative (`1.1 SM_EFF_ZERO (1 + max|P|)`) is kept, and applied to each derivative in turn
// rather than to the second alone. The direction's sign follows the side, `(-1)^(k-1)` for the
// k-th derivative from below, where the pin negated the second derivative past the middle of
// the domain.

//! The curve's direction of travel at a parameter, defined where the first derivative
//! vanishes (63 X5, A2).

use scx_num::math::EFF_ZERO;
use scx_num::vector2d::{UnitVec2, Vec2};
use scx_num::vector3d::{UnitVec3, Vec3};
use scx_nurbs::{Approach, MAX_ORDER, NurbsCurve};

use crate::curve::{Curve, Curve2};

/// The derivatives a NURBS curve's limiting direction reads: as many as a jet takes.
const ORDERS: usize = MAX_ORDER + 1;

impl Curve {
    /// The unit direction of travel at `t`, approached from `side`. It is defined at poles
    /// and cusps where the limit exists: where the first derivative vanishes, the first
    /// derivative `C^(k)` that does not, its sign `(-1)^(k-1)` from below (the curve
    /// near `t` runs as `C^(k) h^k / k!`).
    ///
    /// A derivative vanishes when it is shorter than `1.1 EFF_ZERO (1 + max|C(t)|)`, the
    /// pin's floor for a degenerate tangent. An analytic kind's first derivative never does.
    /// `None` where every derivative vanishes (a NURBS curve collapsed to a point) or `t` is
    /// not finite.
    #[must_use]
    pub fn tangent_dir(&self, t: f64, side: Approach) -> Option<UnitVec3> {
        let v = match self {
            Self::Nurbs(n) => {
                let [x, y, z] = nurbs_dir(n, t, side)?;
                Vec3::new(x, y, z)
            }
            _ => self.jet::<1>(t, side).d[0],
        };
        (v / v.max_dimension()).normalized()
    }
}

impl Curve2 {
    /// The unit direction of travel at `t`, approached from `side`; see
    /// [`Curve::tangent_dir`].
    #[must_use]
    pub fn tangent_dir(&self, t: f64, side: Approach) -> Option<UnitVec2> {
        let v = match self {
            Self::Nurbs(n) => {
                let [x, y] = nurbs_dir(n, t, side)?;
                Vec2::new(x, y)
            }
            _ => self.jet::<1>(t, side).d[0],
        };
        (v / v.max_dimension()).normalized()
    }
}

/// The first derivative of `c` at `t` longer than the floor, signed for `side`. The first
/// derivative alone is computed unless it vanishes.
fn nurbs_dir<const N: usize>(c: &NurbsCurve<N>, t: f64, side: Approach) -> Option<[f64; N]> {
    let [p, d1] = c.derivs::<2>(t, side).ok()?;
    let max = p.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
    let floor = 1.1 * EFF_ZERO * (1.0 + max);
    let long = |d: &[f64; N]| d.iter().map(|x| x * x).sum::<f64>() >= floor * floor;
    if long(&d1) {
        return Some(d1);
    }
    let all = c.derivs::<ORDERS>(t, side).ok()?;
    let (k, d) = all.iter().enumerate().skip(2).find(|(_, d)| long(d))?;
    let flip = side == Approach::Below && k % 2 == 0;
    Some(if flip { d.map(|x| -x) } else { *d })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use scx_num::frame::{Frame, Sense};
    use scx_num::units::Length;
    use scx_num::vector3d::Point3;
    use scx_nurbs::KnotVector;

    use super::*;
    use crate::{Circle, Line};

    fn nurbs(degree: usize, knots: Vec<f64>, points: Vec<[f64; 3]>) -> Curve {
        let k = KnotVector::new(degree, knots).unwrap();
        Curve::Nurbs(Arc::new(NurbsCurve::new(k, points, None).unwrap()))
    }

    fn near(a: UnitVec3, b: [f64; 3]) -> bool {
        let a = a.as_vec().to_array();
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-14)
    }

    #[test]
    fn an_analytic_kind_runs_along_its_first_derivative() {
        let l = Curve::Line(Line::new(Point3::ORIGIN, Vec3::new(3e-12, 0.0, 4e-12)).unwrap());
        for side in [Approach::Below, Approach::Above] {
            assert!(near(l.tangent_dir(5.0, side).unwrap(), [0.6, 0.0, 0.8]));
        }
        let f = Frame::new(Point3::ORIGIN, Vec3::Z, Vec3::X).unwrap();
        let c = Curve::Circle(Circle::new(f, Length::from_mm(2.0), Sense::Reversed).unwrap());
        assert!(near(
            c.tangent_dir(0.0, Approach::Below).unwrap(),
            [0.0, -1.0, 0.0]
        ));
        assert!(c.tangent_dir(f64::NAN, Approach::Above).is_none());
    }

    #[test]
    fn a_nurbs_cusp_runs_along_the_first_derivative_that_does_not_vanish() {
        // A smooth cubic: its first derivative.
        let s = nurbs(
            3,
            vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
            vec![[0.0; 3], [1.0, 1.0, 0.0], [2.0, 1.0, 0.0], [3.0, 0.0, 0.0]],
        );
        let r = 1.0 / 2.0_f64.sqrt();
        assert!(near(
            s.tangent_dir(0.0, Approach::Above).unwrap(),
            [r, r, 0.0]
        ));
        // Doubled start point: `C'(0) = 0`, `C''(0)` along x, from either side of 0 the
        // curve runs as `C'' h^2 / 2`.
        let q = nurbs(
            3,
            vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
            vec![[0.0; 3], [0.0; 3], [2.0, 0.0, 0.0], [3.0, 1.0, 0.0]],
        );
        let up = q.tangent_dir(0.0, Approach::Above).unwrap();
        let down = q.tangent_dir(0.0, Approach::Below).unwrap();
        assert!(near(up, [1.0, 0.0, 0.0]), "{up:?}");
        assert!(near(down, [-1.0, 0.0, 0.0]), "{down:?}");
        // Tripled: `C'''` along x, which keeps its sign from below.
        let t = nurbs(
            3,
            vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
            vec![[0.0; 3], [0.0; 3], [0.0; 3], [3.0, 0.0, 0.0]],
        );
        for side in [Approach::Below, Approach::Above] {
            assert!(near(t.tangent_dir(0.0, side).unwrap(), [1.0, 0.0, 0.0]));
        }
        // Collapsed to a point: no direction.
        let p = nurbs(1, vec![0.0, 0.0, 1.0, 1.0], vec![[1.0, 2.0, 3.0]; 2]);
        assert!(p.tangent_dir(0.5, Approach::Above).is_none());
    }

    #[test]
    fn the_2d_kinds_agree_with_the_3d_kinds() {
        let k = KnotVector::new(2, vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]).unwrap();
        let c2 = Curve2::Nurbs(Arc::new(
            NurbsCurve::new(k, vec![[0.0, 0.0], [0.0, 0.0], [1.0, 2.0]], None).unwrap(),
        ));
        let c3 = nurbs(
            2,
            vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            vec![[0.0; 3], [0.0; 3], [1.0, 2.0, 0.0]],
        );
        for side in [Approach::Below, Approach::Above] {
            let a = c2.tangent_dir(0.0, side).unwrap().as_vec();
            let b = c3.tangent_dir(0.0, side).unwrap().as_vec();
            assert_eq!([a.x, a.y, 0.0], b.to_array());
        }
    }
}
