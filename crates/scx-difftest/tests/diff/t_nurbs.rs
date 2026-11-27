// Written for solidcorex. Apache-2.0.
//! `scx-nurbs` against the pin's `nurbs::knots` (`basis_eval`), `nurbs::nurbs_crv`
//! (`basis_derivs`, `crv_eval`, `crv_derivs`, `crv_split`), `nurbs::crv_tool::p1`
//! (`Curve::insert_knot`, `Curve::elevate_degree`) and `nurbs::bezier` (`binomial`,
//! `eval_one_basis`, `split`), on seeded curves of every knot and weight kind from
//! `scx_testkit::sample` (3D; the pin's 2D marker is gone from scx).
//!
//! The results are compared as `Option`s (`None` for any error): the error types differ by
//! design (`NurbsError` against `SolidError`). The pin defects are in `pin_defects.rs`.

use crate::diff;
use crate::harness::{Gen, SplitMix64};
use crate::pin_defects;

use scx_num::hash::Rng;
use scx_testkit::data::CurveData;
use scx_testkit::sample::{self, Knots, Spec, Weights};

/// A curve as data: its knots and its homogeneous points `[w x, w y, w z, w]`.
pub type Hom = (Vec<f64>, Vec<[f64; 4]>);

/// A seeded curve (knots on `[0, 1]`, clamped) and a parameter for it: the ends, the knots,
/// a rounding error either side of each, points outside the domain and random ones.
#[derive(Clone, Debug)]
pub struct At {
    pub curve: CurveData<3>,
    pub u: f64,
}

/// The seeded curves: degrees 1-5 and 9, every knot and weight kind.
pub fn curves(rng: &mut SplitMix64) -> Vec<CurveData<3>> {
    let mut r = Rng::new(rng.next_u64());
    let mut out = Vec::new();
    for degree in [1, 2, 3, 4, 5, 9] {
        for knots in [
            Knots::Uniform,
            Knots::Random,
            Knots::Clustered,
            Knots::Multiple,
        ] {
            for weights in [Weights::Unit, Weights::Moderate, Weights::NearZero] {
                for count in [degree + 1, degree + 5] {
                    out.push(sample::curve(
                        &mut r,
                        Spec {
                            degree,
                            count,
                            knots,
                        },
                        weights,
                    ));
                }
            }
        }
    }
    out
}

impl Gen for At {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v = Vec::new();
        for curve in curves(rng) {
            let mut us = vec![0.0, 1.0, -0.25, 1.25, -1e-300, 1.0 + f64::EPSILON];
            let interior = curve.knots.iter().copied().filter(|&k| k > 0.0 && k < 1.0);
            for k in interior.take(3) {
                us.extend([k, k.next_down(), k.next_up()]);
            }
            us.extend([0.0_f64.next_up(), 1.0_f64.next_down()]);
            for _ in 0..3 {
                us.push(rng.uniform(0.0, 1.0));
            }
            v.extend(us.into_iter().map(|u| At {
                curve: curve.clone(),
                u,
            }));
        }
        v
    }
}

/// The adapters: each builds its side's curve from the same data.
pub mod x {
    use super::{At, Hom};
    use scx_nurbs::{Approach, KnotVector, NurbsCurve};

    pub fn side(above: bool) -> Approach {
        if above {
            Approach::Above
        } else {
            Approach::Below
        }
    }

    pub fn curve(a: &At) -> Option<NurbsCurve<3>> {
        let d = &a.curve;
        let knots = KnotVector::new(d.degree, d.knots.clone()).ok()?;
        NurbsCurve::new(knots, d.points.clone(), d.weights.clone()).ok()
    }

    /// Knots and homogeneous points `(w P, w)` (`w = 1` for a polynomial curve).
    pub fn data(c: &NurbsCurve<3>) -> Hom {
        let pts = match (c.weighted_points(), c.weights()) {
            (Some(p), Some(w)) => p
                .iter()
                .zip(w)
                .map(|(p, &w)| [p[0], p[1], p[2], w])
                .collect(),
            _ => c.points().iter().map(|p| [p[0], p[1], p[2], 1.0]).collect(),
        };
        (c.knots().to_vec(), pts)
    }

    pub fn point(a: &At, above: bool) -> Option<[f64; 3]> {
        curve(a)?.point(a.u, side(above)).ok()
    }

    pub fn derivs(a: &At, above: bool) -> Option<Vec<[f64; 3]>> {
        Some(curve(a)?.derivs::<6>(a.u, side(above)).ok()?.to_vec())
    }

    pub fn basis(a: &At, above: bool) -> Option<(usize, Vec<f64>)> {
        let k = KnotVector::new(a.curve.degree, a.curve.knots.clone()).ok()?;
        let b = k.basis(a.u, side(above)).ok()?;
        Some((b.span(), b.values().to_vec()))
    }

    pub fn basis_derivs(a: &At, above: bool) -> Option<(usize, Vec<Vec<f64>>)> {
        let k = KnotVector::new(a.curve.degree, a.curve.knots.clone()).ok()?;
        let b = k.basis_derivs::<4>(a.u, side(above)).ok()?;
        let rows = (0..4)
            .map(|r| b.row(r).map(<[f64]>::to_vec))
            .collect::<Option<_>>()?;
        Some((b.span(), rows))
    }

    pub fn insert(a: &At, times: usize) -> Option<Hom> {
        Some(data(&curve(a)?.insert_knot(a.u, times).ok()?))
    }

    pub fn split(a: &At) -> Option<[Hom; 2]> {
        let (l, r) = curve(a)?.split(a.u).ok()?;
        Some([data(&l), data(&r)])
    }

    pub fn elevate(a: &At, times: usize) -> Option<Hom> {
        Some(data(&curve(a)?.elevate(times).ok()?))
    }

    pub fn bezier_split(a: &At) -> Option<[Vec<[f64; 3]>; 2]> {
        let (l, r) = scx_nurbs::bezier::split(&a.curve.points, a.u).ok()?;
        Some([l, r])
    }
}

pub mod s {
    use super::{At, Hom};
    use solidcore::math::Vec3;
    use solidcore::nurbs::crv_basic::Curve;
    use solidcore::nurbs::knots::{KnotVector, Side};
    use solidcore::nurbs::math::{CPoint, NOW};
    use solidcore::nurbs::nurbs_crv;

    /// The pin's `Left` is the span starting at `u` (scx `Above`).
    pub fn side(above: bool) -> Side {
        if above { Side::Left } else { Side::Right }
    }

    fn v3(v: Vec3) -> [f64; 3] {
        [v.x, v.y, v.z]
    }

    pub fn cpts(a: &At) -> Vec<CPoint> {
        let d = &a.curve;
        match &d.weights {
            None => d
                .points
                .iter()
                .map(|p| CPoint::new(p[0], p[1], p[2], NOW))
                .collect(),
            Some(w) => d
                .points
                .iter()
                .zip(w)
                .map(|(p, &w)| CPoint::new(w * p[0], w * p[1], w * p[2], w))
                .collect(),
        }
    }

    pub fn curve(a: &At) -> Option<Curve> {
        Curve::new(cpts(a), a.curve.degree, a.curve.knots.clone()).ok()
    }

    pub fn data(c: &Curve) -> Hom {
        let pts = c
            .cpts()
            .iter()
            .map(|q| [q.x, q.y, q.z, if q.w == NOW { 1.0 } else { q.w }])
            .collect();
        (c.knots().to_vec(), pts)
    }

    pub fn point(a: &At, above: bool) -> Option<[f64; 3]> {
        nurbs_crv::crv_eval(&curve(a)?, a.u, side(above))
            .ok()
            .map(v3)
    }

    pub fn derivs(a: &At, above: bool) -> Option<Vec<[f64; 3]>> {
        let d = nurbs_crv::crv_derivs(&curve(a)?, a.u, side(above), 5).ok()?;
        Some(d.into_iter().map(v3).collect())
    }

    pub fn basis(a: &At, above: bool) -> Option<(usize, Vec<f64>)> {
        let k = &a.curve.knots;
        let kv = KnotVector::new(k, k.len() - 1).ok()?;
        let b = kv.basis_eval(a.curve.degree, a.u, side(above)).ok()?;
        Some((b.span, b.values))
    }

    pub fn basis_derivs(a: &At, above: bool) -> Option<(usize, Vec<Vec<f64>>)> {
        let k = &a.curve.knots;
        let kv = KnotVector::new(k, k.len() - 1).ok()?;
        let b = nurbs_crv::basis_derivs(&kv, a.curve.degree, a.u, side(above), 3).ok()?;
        Some((b.span, b.values))
    }

    pub fn insert(a: &At, times: usize) -> Option<Hom> {
        Some(data(&curve(a)?.insert_knot(a.u, times).ok()?))
    }

    pub fn split(a: &At) -> Option<[Hom; 2]> {
        let (l, r) = nurbs_crv::crv_split(&curve(a)?, a.u).ok()?;
        Some([data(&l), data(&r)])
    }

    pub fn elevate(a: &At, times: usize) -> Option<Hom> {
        Some(data(&curve(a)?.elevate_degree(times).ok()?))
    }

    pub fn bezier_split(a: &At) -> Option<[Vec<[f64; 3]>; 2]> {
        let pw: Vec<CPoint> = a
            .curve
            .points
            .iter()
            .map(|p| CPoint::new(p[0], p[1], p[2], NOW))
            .collect();
        let (l, r) = solidcore::nurbs::bezier::split(&pw, pw.len().checked_sub(1)?, a.u).ok()?;
        let e = |v: Vec<CPoint>| v.into_iter().map(|q| [q.x, q.y, q.z]).collect();
        Some([e(l), e(r)])
    }
}

diff!(
    nurbs_point,
    pin_defect = pin_defects::nurbs_point,
    |a: At, above: bool| x::point(&a, above),
    s::point(&a, above)
);
diff!(
    nurbs_derivs,
    pin_defect = pin_defects::nurbs_derivs,
    |a: At, above: bool| x::derivs(&a, above),
    s::derivs(&a, above)
);
diff!(
    nurbs_basis,
    |a: At, above: bool| x::basis(&a, above),
    s::basis(&a, above)
);
diff!(
    nurbs_basis_derivs,
    |a: At, above: bool| x::basis_derivs(&a, above),
    s::basis_derivs(&a, above)
);
diff!(
    nurbs_insert_knot,
    pin_defect = pin_defects::nurbs_insert_knot,
    |a: At, times: usize| x::insert(&a, times % 4),
    s::insert(&a, times % 4)
);
diff!(
    nurbs_split,
    pin_defect = pin_defects::nurbs_split,
    |a: At| x::split(&a),
    s::split(&a)
);
diff!(
    nurbs_elevate,
    pin_defect = pin_defects::nurbs_elevate,
    |a: At, times: usize| x::elevate(&a, times % 4),
    s::elevate(&a, times % 4)
);
diff!(
    nurbs_bezier_split,
    pin_defect = pin_defects::nurbs_bezier_split,
    |a: At| x::bezier_split(&a),
    s::bezier_split(&a)
);
diff!(
    nurbs_binomial,
    pin_defect = pin_defects::nurbs_binomial,
    |n: usize, k: usize| scx_nurbs::bezier::binomial(n % 40, k % 40),
    solidcore::nurbs::bezier::binomial(n % 40, k % 40).ok()
);
diff!(
    nurbs_bernstein,
    pin_defect = pin_defects::nurbs_bernstein,
    |n: usize, i: usize, u: f64| scx_nurbs::bezier::bernstein(n % 17, i % 17, u).ok(),
    solidcore::nurbs::bezier::eval_one_basis(n % 17, i % 17, u).ok()
);
