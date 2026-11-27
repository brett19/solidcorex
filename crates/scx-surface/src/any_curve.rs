// Written for solidcorex. Apache-2.0.
//! [`AnyCurve`]: every curve, free or defined on a surface, as one value (01 §2, C1; 56
//! §7.1; 63 §4.2). Step 2's arms are [`AnyCurve::Free`] (a [`Curve`] of `scx-curve`) and
//! [`AnyCurve::OnSurface`] (a [`CurveOnSurface`]: a parameter-space [`Curve2`] on a
//! [`Surface`]); the intersection curve, the curve in a volume and the projected curve join
//! as arms when their steps build them.
//!
//! `C(t) = S(uv(t))`. Its point is the surface's point at the uv curve's point, so
//! [`CurveOnSurface::point`] is [`Surface::point`]`(`[`Curve2::point`]`(t))` bit for bit.
//! Its derivatives are the chain rule to any order ([`CurveOnSurface::jet`]), from the uv
//! curve's jet and the surface's partials, each taken on the side the curve approaches from.
//!
//! The pin's `SmCrvOnSurf` also holds a domain of interest, owner flags, a break cache and a
//! cached B-spline approximation. None is kept here (63 §4.9, A3): ownership is the `Arc`,
//! and a derived approximation is memoised by its caller if a profile asks.

use std::sync::Arc;

use scx_curve::{Curve, Curve2};
use scx_num::domain::ParamDomain;
use scx_num::jet::{CurveJet, CurveJet2, SurfaceJet};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;

use crate::surface::Surface;

/// A curve in space: a free curve, or a curve defined on a surface.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AnyCurve {
    /// A curve of its own kind (line, circle, ellipse, NURBS).
    Free(Curve),
    /// A parameter-space curve on a surface.
    OnSurface(CurveOnSurface),
}

// A free curve's size (63 §3.7) plus the tag; the surface is behind an `Arc`, so the
// on-surface arm is its uv curve and a pointer.
const _: () = assert!(size_of::<AnyCurve>() <= 112);

/// The curve `S(uv(t))` of a parameter-space curve `uv` on a surface `S`, over `uv`'s
/// parameter.
///
/// The surface is shared (`Arc`): the curves on one face hold one surface. Nothing is
/// checked when it is built: every `uv` curve on every surface has a point at every `t`
/// (a NURBS surface continues its end pieces past its knots, every analytic kind is
/// defined at every `uv`), as the pin's constructor checks only its nesting depth.
#[derive(Debug, Clone, PartialEq)]
pub struct CurveOnSurface {
    uv: Curve2,
    surface: Arc<Surface>,
}

/// How many points [`CurveOnSurface::eval_many`] passes at a time through its stack buffer
/// of uv points.
const CHUNK: usize = 64;

impl CurveOnSurface {
    /// The curve `uv` on `surface`.
    #[must_use]
    pub fn new(uv: Curve2, surface: impl Into<Arc<Surface>>) -> Self {
        Self {
            uv,
            surface: surface.into(),
        }
    }

    /// The parameter-space curve.
    #[must_use]
    pub const fn uv(&self) -> &Curve2 {
        &self.uv
    }

    /// The surface.
    #[must_use]
    pub fn surface(&self) -> &Arc<Surface> {
        &self.surface
    }

    /// The parameter's domain: the uv curve's.
    #[inline]
    #[must_use]
    pub fn domain(&self) -> ParamDomain {
        self.uv.domain()
    }

    /// `S(uv(t))`.
    #[inline]
    #[must_use]
    pub fn point(&self, t: f64) -> Point3 {
        self.surface.point(self.uv.point(t))
    }

    /// The point and the first `D` derivatives of `S(uv(t))` at `t`, from `side` where the
    /// curve or the surface is not smooth there.
    ///
    /// The uv curve's jet is taken from `side`. The surface's partials are taken, in each
    /// direction, from the side the curve moves to as `t` moves to `side`: the sign of the
    /// first nonzero derivative of that coordinate, turned for an odd order from below
    /// ([`approach`]). Where every derivative to order `D` of a coordinate is zero, no
    /// partial along it reaches the result and `side` is used. The pin takes `side` in both
    /// directions, which is the other piece where the curve runs backward across a knot
    /// line.
    ///
    /// The derivatives are the chain rule to order `D` ([`lift`]).
    #[must_use]
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet<D> {
        let c = self.uv.jet::<D>(t, side);
        let sides = [
            approach(c.d.map(|d| d.x), side),
            approach(c.d.map(|d| d.y), side),
        ];
        lift(&c, &self.surface.jet::<D>(c.point, sides))
    }

    /// The point at each of `ts`, into `out`: `out[i]` is [`CurveOnSurface::point`]`(ts[i])`,
    /// bit for bit. The uv points go through a stack buffer, a chunk at a time, into the
    /// surface's [`Surface::eval_many`].
    ///
    /// # Panics
    ///
    /// If `ts` and `out` differ in length.
    pub fn eval_many(&self, ts: &[f64], out: &mut [Point3]) {
        assert_eq!(
            ts.len(),
            out.len(),
            "eval_many: ts and out differ in length"
        );
        let mut uv = [Point2::default(); CHUNK];
        for (ts, out) in ts.chunks(CHUNK).zip(out.chunks_mut(CHUNK)) {
            let Some(uv) = uv.get_mut(..ts.len()) else {
                continue;
            };
            self.uv.eval_many(ts, uv);
            self.surface.eval_many(uv, out);
        }
    }
}

/// The side of a coordinate's knot the curve moves to as `t` moves to `side`, from that
/// coordinate's derivatives `d[k]` (order `k + 1`): its first nonzero derivative's sign,
/// turned for an odd order when `t` moves down; `side` where every one is zero.
fn approach<const D: usize>(d: [f64; D], side: Approach) -> Approach {
    let Some((k, x)) = d.into_iter().enumerate().find(|&(_, x)| x != 0.0) else {
        return side;
    };
    let up = x > 0.0;
    let odd = k % 2 == 0;
    if up == (odd && side == Approach::Below) {
        Approach::Below
    } else {
        Approach::Above
    }
}

/// `n!` as a double (exact to `n = 22`, far past any jet order).
#[inline]
fn factorial(n: usize) -> f64 {
    (1..=n).fold(1.0, |f, k| f * k as f64)
}

/// The truncated power series `p q`: index `n - 1` is the coefficient of `h^n`, neither has
/// a constant term.
fn mul<const D: usize>(p: &[f64; D], q: &[f64; D]) -> [f64; D] {
    let mut r = [0.0; D];
    for (n, r) in r.iter_mut().enumerate() {
        // h^(n+1) = h^(i+1) h^(j+1) with i + j = n - 1.
        *r = p
            .iter()
            .take(n)
            .zip(q.iter().take(n).rev())
            .fold(0.0, |s, (a, b)| s + a * b);
    }
    r
}

/// The Taylor coefficients `x^(k) / k!` (index `k - 1`) of derivatives `x^(k)`.
fn taylor<const D: usize>(mut d: [f64; D]) -> [f64; D] {
    for (k, x) in d.iter_mut().enumerate() {
        *x /= factorial(k + 1);
    }
    d
}

/// `[a, a^2, ..., a^D]`, truncated.
fn powers<const D: usize>(a: [f64; D]) -> [[f64; D]; D] {
    let mut out = [a; D];
    let mut prev = a;
    for o in out.iter_mut().skip(1) {
        prev = mul(&prev, &a);
        *o = prev;
    }
    out
}

/// The jet of `S(uv(t))` from the uv curve's jet `c` and the surface's partials `s` at
/// `c.point`: the chain rule to order `D` (Faa di Bruno). To order 3 it is written out
/// ([`lift3`]), as the pin's `smsurf_LiftSecondDerivative` and `smsurf_LiftThirdDerivative`
/// are; past 3 it is summed by Taylor series ([`lift_taylor`]), where the pin stops.
#[inline]
pub(crate) fn lift<const D: usize>(c: &CurveJet2<D>, s: &SurfaceJet<Vec3, D>) -> CurveJet<D> {
    let p = s.point();
    CurveJet {
        point: Point3::new(p.x, p.y, p.z),
        d: if D <= 3 {
            lift3(c, s)
        } else {
            lift_taylor(c, s)
        },
    }
}

/// The chain rule to order `D <= 3`, written out: with `S_ij` the partials and `u_k`, `v_k`
/// the uv curve's `k`-th derivatives,
///
/// - `C' = S_10 u_1 + S_01 v_1`;
/// - `C'' = S_10 u_2 + S_01 v_2 + S_20 u_1^2 + 2 S_11 u_1 v_1 + S_02 v_1^2`;
/// - `C''' = S_10 u_3 + S_01 v_3 + 3 (S_20 u_1 u_2 + S_11 (u_1 v_2 + u_2 v_1) + S_02 v_1 v_2)`
///   `+ S_30 u_1^3 + 3 S_21 u_1^2 v_1 + 3 S_12 u_1 v_1^2 + S_03 v_1^3`.
///
/// Each product is grouped and each sum taken in the pin's order (its
/// `lift_second_derivative` and `lift_third_derivative`), so from the same partials the
/// result is the pin's bit for bit. Orders past `D` read as zero and fall away once `D` is
/// known.
#[inline]
fn lift3<const D: usize>(c: &CurveJet2<D>, s: &SurfaceJet<Vec3, D>) -> [Vec3; D] {
    let x = |k: usize| c.d.get(k).copied().unwrap_or_default();
    let g = |i, j| s.get(i, j).unwrap_or_default();
    let (a, b, e) = (x(0), x(1), x(2));
    let (u1, v1, u2, v2, u3, v3) = (a.x, a.y, b.x, b.y, e.x, e.y);
    let d1 = g(1, 0) * u1 + g(0, 1) * v1;
    let d2 = g(1, 0) * u2
        + g(0, 1) * v2
        + g(2, 0) * (u1 * u1)
        + g(1, 1) * (2.0 * u1 * v1)
        + g(0, 2) * (v1 * v1);
    let d3 = g(1, 0) * u3
        + g(0, 1) * v3
        + g(2, 0) * (3.0 * u1 * u2)
        + g(1, 1) * (3.0 * (u1 * v2 + u2 * v1))
        + g(0, 2) * (3.0 * v1 * v2)
        + g(3, 0) * (u1 * u1 * u1)
        + g(2, 1) * (3.0 * u1 * u1 * v1)
        + g(1, 2) * (3.0 * u1 * v1 * v1)
        + g(0, 3) * (v1 * v1 * v1);
    let all = [d1, d2, d3];
    core::array::from_fn(|k| all.get(k).copied().unwrap_or_default())
}

/// The chain rule to any order `D` by Taylor series. With `du(h) = sum_k u^(k) h^k / k!` and
/// `dv` the same, the composite's Taylor coefficients are
/// `sum_(i+j<=D) S^(i,j) / (i! j!) [du^i dv^j]_n`, and its `n`-th derivative is `n!` times
/// the `n`-th.
fn lift_taylor<const D: usize>(c: &CurveJet2<D>, s: &SurfaceJet<Vec3, D>) -> [Vec3; D] {
    let pu = powers(taylor(c.d.map(|d| d.x)));
    let pv = powers(taylor(c.d.map(|d| d.y)));
    let mut sum = [Vec3::default(); D];
    for i in 0..=D {
        for j in 0..=D - i {
            let (a, b) = (
                i.checked_sub(1).and_then(|i| pu.get(i)),
                j.checked_sub(1).and_then(|j| pv.get(j)),
            );
            let series = match (a, b) {
                (None, None) => continue,
                (None, Some(b)) => *b,
                (Some(a), None) => *a,
                (Some(a), Some(b)) => mul(a, b),
            };
            let Some(sij) = s.get(i, j) else { continue };
            let sij = sij / (factorial(i) * factorial(j));
            // The series starts at h^(i+j).
            for (t, x) in sum.iter_mut().zip(series).skip(i + j - 1) {
                *t += sij * x;
            }
        }
    }
    for (k, t) in sum.iter_mut().enumerate() {
        *t *= factorial(k + 1);
    }
    sum
}

impl AnyCurve {
    /// The parameter's domain.
    #[inline]
    #[must_use]
    pub fn domain(&self) -> ParamDomain {
        match self {
            Self::Free(c) => c.domain(),
            Self::OnSurface(c) => c.domain(),
        }
    }

    /// The point at `t`.
    #[inline]
    #[must_use]
    pub fn point(&self, t: f64) -> Point3 {
        match self {
            Self::Free(c) => c.point(t),
            Self::OnSurface(c) => c.point(t),
        }
    }

    /// The point and the first `D` derivatives at `t`, from `side` where the curve is not
    /// smooth there.
    #[inline]
    #[must_use]
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet<D> {
        match self {
            Self::Free(c) => c.jet(t, side),
            Self::OnSurface(c) => c.jet(t, side),
        }
    }

    /// The point at each of `ts`, into `out`: `out[i]` is [`AnyCurve::point`]`(ts[i])`, bit
    /// for bit. The arm is matched once.
    ///
    /// # Panics
    ///
    /// If `ts` and `out` differ in length.
    pub fn eval_many(&self, ts: &[f64], out: &mut [Point3]) {
        match self {
            Self::Free(c) => c.eval_many(ts, out),
            Self::OnSurface(c) => c.eval_many(ts, out),
        }
    }
}

impl From<Curve> for AnyCurve {
    fn from(c: Curve) -> Self {
        Self::Free(c)
    }
}

impl From<CurveOnSurface> for AnyCurve {
    fn from(c: CurveOnSurface) -> Self {
        Self::OnSurface(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scx_num::hash::Rng;
    use scx_num::vector2d::Vec2;
    use scx_testkit::rat::Rat;

    /// A dyadic in `[-2, 2]` with 8 fraction bits: exact in `f64` and in `Rat`.
    fn dyadic(rng: &mut Rng) -> f64 {
        (rng.next_signed_unit() * 512.0).round() / 256.0
    }

    fn rat(x: f64) -> Rat {
        Rat::from_f64(x).unwrap()
    }

    fn fact(n: usize) -> Rat {
        Rat::from(i64::try_from((1..=n).product::<usize>()).unwrap())
    }

    /// The full product of two polynomials (index `n` is the coefficient of `h^n`), cut to
    /// degree `d`.
    fn poly_mul(p: &[Rat], q: &[Rat], d: usize) -> Vec<Rat> {
        let mut r = vec![Rat::zero(); d + 1];
        for (i, a) in p.iter().enumerate() {
            for (j, b) in q.iter().enumerate() {
                if i + j <= d {
                    r[i + j] = r[i + j].clone() + a.clone() * b.clone();
                }
            }
        }
        r
    }

    /// The composite's derivatives to order `D` in exact rationals: the polynomial
    /// `sum S^(i,j) / (i! j!) du^i dv^j` with `du = sum u^(k) h^k / k!`, read off as
    /// `n!` times its `h^n` coefficient, one coordinate of `S` at a time.
    fn exact<const D: usize>(c: &CurveJet2<D>, s: &SurfaceJet<Vec3, D>) -> Vec<[f64; 3]> {
        let series = |f: fn(Vec2) -> f64| {
            let mut p = vec![Rat::zero(); D + 1];
            for (k, d) in c.d.iter().enumerate() {
                p[k + 1] = rat(f(*d)).checked_div(&fact(k + 1)).unwrap();
            }
            p
        };
        let (du, dv) = (series(|d| d.x), series(|d| d.y));
        let pow = |p: &[Rat], n: usize| {
            let mut r = vec![Rat::zero(); D + 1];
            r[0] = Rat::one();
            for _ in 0..n {
                r = poly_mul(&r, p, D);
            }
            r
        };
        let mut out = vec![[Rat::zero(), Rat::zero(), Rat::zero()]; D + 1];
        for i in 0..=D {
            for j in 0..=D - i {
                let term = poly_mul(&pow(&du, i), &pow(&dv, j), D);
                let sij = s.get(i, j).unwrap();
                let w = fact(i) * fact(j);
                for (n, t) in term.iter().enumerate() {
                    for (o, x) in out[n].iter_mut().zip([sij.x, sij.y, sij.z]) {
                        *o = o.clone() + rat(x).checked_div(&w).unwrap() * t.clone();
                    }
                }
            }
        }
        out.iter()
            .enumerate()
            .skip(1)
            .map(|(n, o)| o.clone().map(|x| (x * fact(n)).to_f64()))
            .collect()
    }

    fn random<const D: usize>(rng: &mut Rng) -> (CurveJet2<D>, SurfaceJet<Vec3, D>) {
        let c = CurveJet2 {
            point: Point2::new(dyadic(rng), dyadic(rng)),
            d: core::array::from_fn(|_| Vec2::new(dyadic(rng), dyadic(rng))),
        };
        let s = SurfaceJet::from_fn(|_, _| Vec3::new(dyadic(rng), dyadic(rng), dyadic(rng)));
        (c, s)
    }

    fn check<const D: usize>(rng: &mut Rng) {
        for _ in 0..50 {
            let (c, s) = random::<D>(rng);
            assert_eq!(lift(&c, &s).point.to_array(), s.point().to_array());
            for got in [lift(&c, &s).d, lift_taylor(&c, &s)] {
                let want = exact(&c, &s);
                // The same sums with every input made positive bound the rounding.
                let abs = |v: Vec3| Vec3::new(v.x.abs(), v.y.abs(), v.z.abs());
                let ca = CurveJet2 {
                    point: c.point,
                    d: c.d.map(|d| Vec2::new(d.x.abs(), d.y.abs())),
                };
                let bound = exact(&ca, &s.map(abs));
                for k in 0..D {
                    let g = got[k].to_array();
                    for a in 0..3 {
                        let tol = 1e-14 * (1.0 + bound[k][a]);
                        assert!(
                            (g[a] - want[k][a]).abs() <= tol,
                            "D = {D}, order {}: {} vs {}",
                            k + 1,
                            g[a],
                            want[k][a]
                        );
                    }
                }
            }
        }
    }

    /// The chain rule to every order against the composite polynomial in exact rationals.
    #[test]
    fn lift_is_the_exact_composite() {
        let mut rng = Rng::new(0x05c0_ffee);
        check::<1>(&mut rng);
        check::<2>(&mut rng);
        check::<3>(&mut rng);
        check::<4>(&mut rng);
        check::<6>(&mut rng);
    }

    /// The two paths to order 3: the written-out one and the Taylor one agree, order 1 bit
    /// for bit; and a lower order is the first entries of order 3's, bit for bit.
    #[test]
    fn the_written_out_lift_is_the_taylor_lift() {
        let mut rng = Rng::new(17);
        for _ in 0..200 {
            let (c, s) = random::<3>(&mut rng);
            let (got, taylor) = (lift(&c, &s).d, lift_taylor(&c, &s));
            assert_eq!(got[0].to_array(), taylor[0].to_array());
            for (x, y) in got.iter().zip(&taylor) {
                assert!(
                    (*x - *y).length() <= 1e-12 * (1.0 + y.length()),
                    "{x:?} vs {y:?}"
                );
            }
            let c2 = CurveJet2 {
                point: c.point,
                d: [c.d[0], c.d[1]],
            };
            let s2 = SurfaceJet::<Vec3, 2>::from_fn(|i, j| s.get(i, j).unwrap());
            let low = lift(&c2, &s2).d;
            assert_eq!(
                low.map(Vec3::to_array),
                [got[0], got[1]].map(Vec3::to_array)
            );
        }
    }

    #[test]
    fn approach_is_the_first_nonzero_derivatives_side() {
        use Approach::{Above, Below};
        // Moving up in t: the first nonzero derivative's sign.
        assert_eq!(approach([1.0, -5.0], Above), Above);
        assert_eq!(approach([-1.0, 5.0], Above), Below);
        assert_eq!(approach([0.0, -5.0], Above), Below);
        assert_eq!(approach([0.0, 0.0, 2.0], Above), Above);
        // Moving down: turned for an odd order only.
        assert_eq!(approach([1.0, 5.0], Below), Below);
        assert_eq!(approach([0.0, 5.0], Below), Above);
        assert_eq!(approach([0.0, -5.0], Below), Below);
        assert_eq!(approach([0.0, 0.0, 2.0], Below), Below);
        // All zero (or no derivatives): `side`.
        assert_eq!(approach([0.0; 3], Below), Below);
        assert_eq!(approach([0.0; 3], Above), Above);
        assert_eq!(approach::<0>([], Below), Below);
    }
}
