// Written for solidcorex. Apache-2.0.
//! A curve's point and derivatives over a [`Ranged`] scalar: `f64` for values, [`Ivl`] for
//! enclosures (research 63 §3.2, "the scalar-generic form"), and the `f64` jet, a point with
//! its first `K` derivatives apart (any `K` up to [`MAX_ORDER`]).
//!
//! The kernels are [`crate::basis`]'s (A2.2 and A2.3) and [`NurbsCurve`]'s (the point, and
//! the derivatives with the rational quotient rule) written over the scalar, in the same
//! arithmetic order, so in `f64` they give [`NurbsCurve::point`]'s and
//! [`NurbsCurve::derivs`]'s bits. Two things differ in an interval:
//!
//! - The knot difference a basis divides by is [`Ranged::gap`], an enclosure of the
//!   difference itself. The pin's `(U[i+j] - u) + (u - U[i])` would be twice as wide as `u`
//!   and hold zero once `u` is as wide as a span.
//! - An interval wider than a point meets several pieces: each is evaluated on the part of the
//!   interval inside it (the end pieces continue past the domain, as the curve does), and the
//!   results joined. At an interior knot both one-sided values are inside, so the enclosure
//!   holds whichever `side` asks for.
//!
//! A parameter with an end that is not finite gives [`Ranged::undefined`] (NaN, the whole
//! line), as an order of derivative above [`MAX_ORDER`] does.
//!
//! [`Ivl`]: scx_num::interval::Ivl

use scx_num::trig::Ranged;

use crate::basis::{FULL, SMALL};
use crate::{Approach, MAX_ORDER, NurbsCurve, NurbsError, Result};

impl<const D: usize> NurbsCurve<D> {
    /// `C(u)` and its first `K` derivatives `C^(k)(u)` on the piece `side` picks: the values
    /// of [`NurbsCurve::derivs`]`::<{K + 1}>`, bit for bit, with the point apart.
    ///
    /// Errors as [`NurbsCurve::derivs`].
    pub fn jet<const K: usize>(&self, u: f64, side: Approach) -> Result<([f64; D], [[f64; D]; K])> {
        if K > MAX_ORDER {
            return Err(NurbsError::DerivativeOrder {
                order: K,
                max: MAX_ORDER,
            });
        }
        if !u.is_finite() {
            return Err(NurbsError::ParameterNotFinite { parameter: u });
        }
        let (point, d) = self.jet_over::<f64, K>(u, side);
        if point
            .iter()
            .chain(d.iter().flatten())
            .all(|x| x.is_finite())
        {
            Ok((point, d))
        } else {
            Err(NurbsError::Degenerate { parameter: u })
        }
    }

    /// `C(u)` over a scalar: in `f64` the arithmetic of [`NurbsCurve::point`] (its bits where
    /// it succeeds, the non-finite values themselves where it fails); in an interval an
    /// enclosure of `C` over the interval.
    #[must_use]
    pub fn point_over<T: Ranged>(&self, u: T, side: Approach) -> [T; D] {
        let undefined = [T::undefined(); D];
        self.pieces(u, side, undefined, join, |u, span| {
            if self.degree() < SMALL {
                self.point_span::<T, SMALL>(u, span)
            } else {
                self.point_span::<T, FULL>(u, span)
            }
        })
    }

    /// `C(u)` and `C^(k)(u)` for `k` in `1..=K` over a scalar: in `f64` the arithmetic of
    /// [`NurbsCurve::derivs`]`::<{K + 1}>`; in an interval an enclosure of each over the
    /// interval.
    #[must_use]
    pub fn jet_over<T: Ranged, const K: usize>(
        &self,
        u: T,
        side: Approach,
    ) -> ([T; D], [[T; D]; K]) {
        let undefined = ([T::undefined(); D], [[T::undefined(); D]; K]);
        if K > MAX_ORDER {
            return undefined;
        }
        let both = |a: ([T; D], [[T; D]; K]), b: ([T; D], [[T; D]; K])| {
            let mut d = a.1;
            for (x, y) in d.iter_mut().zip(b.1) {
                *x = join(*x, y);
            }
            (join(a.0, b.0), d)
        };
        self.pieces(u, side, undefined, both, |u, span| {
            if self.degree() < SMALL {
                self.jet_span::<T, SMALL, K>(u, span)
            } else {
                self.jet_span::<T, FULL, K>(u, span)
            }
        })
    }

    /// `eval` on every piece `u` meets, each with `u` clipped to it, joined; on the one piece
    /// `side` picks for a point.
    fn pieces<T: Ranged, R: Copy>(
        &self,
        u: T,
        side: Approach,
        undefined: R,
        join: impl Fn(R, R) -> R,
        eval: impl Fn(T, usize) -> Option<R>,
    ) -> R {
        let (lo, hi) = u.ends();
        if !(lo.is_finite() && hi.is_finite() && lo <= hi) {
            return undefined;
        }
        let kv = self.knot_vector();
        if lo == hi {
            return eval(u, kv.span_of(lo, side)).unwrap_or(undefined);
        }
        let (low, high) = kv.ends();
        let knots = kv.knots();
        let mut all: Option<R> = None;
        for span in kv.span_of(lo, side)..=kv.span_of(hi, side) {
            let (Some(&a), Some(&b)) = (knots.get(span), knots.get(span + 1)) else {
                return undefined;
            };
            if a >= b {
                continue;
            }
            let a = if a <= low { f64::NEG_INFINITY } else { a };
            let b = if b >= high { f64::INFINITY } else { b };
            let Some(r) = eval(u.clip(a, b), span) else {
                return undefined;
            };
            all = Some(all.map_or(r, |acc| join(acc, r)));
        }
        all.unwrap_or(undefined)
    }

    /// `C(u)` on the piece `span`, with scratch width `N > p`: [`NurbsCurve::point`]'s
    /// `point_n` over `T`.
    fn point_span<T: Ranged, const N: usize>(&self, u: T, span: usize) -> Option<[T; D]> {
        let p = self.degree();
        let zero = T::lift(0.0);
        let mut n = [zero; N];
        values_over(self.knot_vector().window(span)?, u, &mut n);
        let local = span - p..=span;
        let mut c = [zero; D];
        let (Some(weighted), Some(weights)) = (self.weighted_points(), self.weights()) else {
            for (&alpha, q) in n.iter().zip(self.points().get(local)?) {
                c = blend(c, alpha, q);
            }
            return Some(c);
        };
        let mut w = zero;
        let pairs = weighted.get(local.clone())?.iter().zip(weights.get(local)?);
        for (&alpha, (q, &wq)) in n.iter().zip(pairs) {
            c = blend(c, alpha, q);
            w = w + alpha * T::lift(wq);
        }
        Some(c.map(|x| x / w))
    }

    /// `C(u)` and `C^(k)(u)`, `k` in `1..=K`, on the piece `span`, with scratch width `N > p`:
    /// [`NurbsCurve::derivs`]'s `derivs_n` over `T`, the point's row kept apart.
    fn jet_span<T: Ranged, const N: usize, const K: usize>(
        &self,
        u: T,
        span: usize,
    ) -> Option<([T; D], [[T; D]; K])> {
        let p = self.degree();
        let zero = T::lift(0.0);
        let mut n0 = [zero; N];
        let mut nd = [[zero; N]; K];
        derivs_over(self.knot_vector().window(span)?, u, &mut n0, &mut nd)?;
        let local = span - p..=span;
        let points = self.points().get(local.clone())?;
        let mut c0 = [zero; D];
        let mut c = [[zero; D]; K];
        let Some(weights) = self.weights() else {
            // C^(k) = sum_i N^(k)[i] P[i].
            for (&b, q) in n0.iter().zip(points) {
                c0 = blend(c0, b, q);
            }
            for (ck, row) in c.iter_mut().zip(&nd) {
                for (&b, q) in row.iter().zip(points) {
                    *ck = blend(*ck, b, q);
                }
            }
            return Some((c0, c));
        };
        let weights = weights.get(local)?;
        // d[k] = sum_j w[j] N^(k)[j], the derivatives of the weight function.
        let mut d0 = zero;
        for (&b, &w) in n0.iter().zip(weights) {
            d0 = d0 + T::lift(w) * b;
        }
        let mut d = [zero; K];
        for (dk, row) in d.iter_mut().zip(&nd) {
            for (&b, &w) in row.iter().zip(weights) {
                *dk = *dk + T::lift(w) * b;
            }
        }
        // binomial[k - 1][j - 1] = C(k, j), exact.
        let mut binomial = [[0.0; K]; K];
        let mut prev = [0.0; K];
        for row in &mut binomial {
            let mut left = 1.0;
            for (v, &pv) in row.iter_mut().zip(&prev) {
                *v = left + pv;
                left = pv;
            }
            prev = *row;
        }
        // R^(k)[i] = (w[i] N^(k)[i] - sum_{j=1..=k} C(k, j) d[j] R^(k-j)[i]) / d[0], then
        // C^(k) = sum_i R^(k)[i] P[i].
        for (i, (&w, q)) in weights.iter().zip(points).enumerate() {
            let r0 = T::lift(w) * *n0.get(i)? / d0;
            let mut col = [zero; K];
            for (k, (row, brow)) in nd.iter().zip(&binomial).enumerate() {
                let (done, rest) = col.split_at_mut(k);
                let mut v = T::lift(w) * *row.get(i)?;
                let earlier = done.iter().rev().chain(core::iter::once(&r0));
                for ((&b, &dj), &rk) in brow.iter().zip(&d).zip(earlier) {
                    v = v - T::lift(b) * dj * rk;
                }
                *rest.first_mut()? = v / d0;
            }
            c0 = blend(c0, r0, q);
            for (ck, &rk) in c.iter_mut().zip(&col) {
                *ck = blend(*ck, rk, q);
            }
        }
        Some((c0, c))
    }
}

/// `c + alpha q`, coordinate by coordinate ([`crate::point::Control::blend`]).
fn blend<T: Ranged, const D: usize>(mut c: [T; D], alpha: T, q: &[f64; D]) -> [T; D] {
    for (x, &a) in c.iter_mut().zip(q) {
        *x = *x + alpha * T::lift(a);
    }
    c
}

fn join<T: Ranged, const D: usize>(mut a: [T; D], b: [T; D]) -> [T; D] {
    for (x, y) in a.iter_mut().zip(b) {
        *x = x.join(y);
    }
    a
}

/// [`crate::basis::values_into`] over `T`, its knot differences [`Ranged::gap`].
fn values_over<T: Ranged, const N: usize>((below, above): (&[f64], &[f64]), u: T, n: &mut [T; N]) {
    let zero = T::lift(0.0);
    // left[j-1] = u - U[span+1-j], right[j-1] = U[span+j] - u, for j in 1..=p. The knots
    // themselves are read from the window where `gap` needs them, not copied beside these,
    // so an `f64` instance, whose `gap` does not read them, keeps no second pair of arrays.
    let mut left = [zero; N];
    let mut right = [zero; N];
    for (l, &k) in left.iter_mut().zip(below.iter().rev()) {
        *l = u - T::lift(k);
    }
    for (r, &k) in right.iter_mut().zip(above) {
        *r = T::lift(k) - u;
    }
    *n = [zero; N];
    if let Some(first) = n.first_mut() {
        *first = T::lift(1.0);
    }
    let p = above.len();
    for j in 1..=p {
        let mut saved = zero;
        let mut pairs = right.iter().zip(left.iter().take(j).rev()).enumerate();
        for value in n.iter_mut().take(j + 1) {
            if let Some((i, (&rr, &ll))) = pairs.next() {
                // rr = U[span + 1 + i] - u, ll = u - U[span + 1 - j + i]. The knots are read
                // without a failure path, so where `gap` ignores them (`f64`) they go.
                let high = above.get(i).copied().unwrap_or(f64::NAN);
                let low = below.get(p + i - j).copied().unwrap_or(f64::NAN);
                let delta = T::gap(rr, ll, high, low);
                let temp = *value;
                *value = saved + temp * (rr / delta);
                saved = temp * (ll / delta);
            } else {
                *value = saved;
            }
        }
    }
}

/// [`crate::basis::derivs_into`] over `T` with `K + 1` rows, row 0 in `n0` and row `k` in
/// `nd[k - 1]`, its knot differences [`Ranged::gap`].
fn derivs_over<T: Ranged, const N: usize, const K: usize>(
    (below, above): (&[f64], &[f64]),
    u: T,
    n0: &mut [T; N],
    nd: &mut [[T; N]; K],
) -> Option<()> {
    let p = above.len();
    let zero = T::lift(0.0);
    // As in `values_over`, the knots `gap` takes are read from the window, not copied.
    let mut left = [zero; N];
    let mut right = [zero; N];
    for (l, &k) in left.iter_mut().skip(1).zip(below.iter().rev()) {
        *l = u - T::lift(k);
    }
    for (r, &k) in right.iter_mut().skip(1).zip(above) {
        *r = T::lift(k) - u;
    }

    // ndu[r][j] (r <= j): the basis functions of degree j. ndu[j][r] (j > r): the knot
    // differences they divide by.
    let mut ndu = [[zero; N]; N];
    *ndu.first_mut()?.first_mut()? = T::lift(1.0);
    for j in 1..=p {
        let mut saved = zero;
        for r in 0..j {
            // left[j - r] = u - U[span + 1 - (j - r)], right[r + 1] = U[span + 1 + r] - u.
            let (rr, high) = (*right.get(r + 1)?, *above.get(r)?);
            let (ll, low) = (*left.get(j - r)?, *below.get(p - (j - r))?);
            let den = T::gap(rr, ll, high, low);
            *ndu.get_mut(j)?.get_mut(r)? = den;
            let prev = *ndu.get(r)?.get(j - 1)?;
            *ndu.get_mut(r)?.get_mut(j)? = saved + prev * (rr / den);
            saved = prev * (ll / den);
        }
        *ndu.get_mut(j)?.get_mut(j)? = saved;
    }
    let at = |r: usize, c: usize| ndu.get(r).and_then(|row| row.get(c)).copied();

    *n0 = [zero; N];
    *nd = [[zero; N]; K];
    for (j, v) in n0.iter_mut().enumerate().take(p + 1) {
        *v = at(j, p)?;
    }

    let order = p.min(K);
    for r in 0..=p {
        let mut rows = [[zero; N]; 2];
        let [first, second] = &mut rows;
        let (mut s1, mut s2) = (first, second);
        *s1.first_mut()? = T::lift(1.0);
        for k in 1..=order {
            let mut d = zero;
            let pk = p - k;
            if r >= k {
                let rk = r - k;
                let v = *s1.first()? / at(pk + 1, rk)?;
                *s2.first_mut()? = v;
                d = v * at(rk, pk)?;
            }
            let j1 = if r + 1 >= k { 1 } else { k - r };
            let j2 = if r <= pk + 1 { k - 1 } else { p - r };
            for j in j1..=j2 {
                let rkj = r + j - k;
                let v = (*s1.get(j)? - *s1.get(j - 1)?) / at(pk + 1, rkj)?;
                *s2.get_mut(j)? = v;
                d = d + v * at(rkj, pk)?;
            }
            if r <= pk {
                let v = -*s1.get(k - 1)? / at(pk + 1, r)?;
                *s2.get_mut(k)? = v;
                d = d + v * at(r, pk)?;
            }
            *nd.get_mut(k - 1)?.get_mut(r)? = d;
            (s1, s2) = (s2, s1);
        }
    }

    // Multiply through by p! / (p - k)!.
    let mut factor = p;
    for (k, row) in nd.iter_mut().enumerate().take(order) {
        for v in row.iter_mut().take(p + 1) {
            *v = *v * T::lift(factor as f64);
        }
        factor *= p - (k + 1);
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use scx_num::hash::Rng;
    use scx_num::interval::Ivl;
    use scx_testkit::Rat;
    use scx_testkit::data::CurveData;
    use scx_testkit::nurbs as tk;
    use scx_testkit::sample::{self, Knots, Spec, Weights};

    use crate::{Approach, KnotVector, NurbsCurve, NurbsError};

    const SIDES: [(Approach, tk::Side); 2] = [
        (Approach::Below, tk::Side::Below),
        (Approach::Above, tk::Side::Above),
    ];

    fn ours(d: &CurveData<3>) -> NurbsCurve<3> {
        let knots = KnotVector::new(d.degree, d.knots.clone()).unwrap();
        NurbsCurve::new(knots, d.points.clone(), d.weights.clone()).unwrap()
    }

    /// Random curves of every knot and weight kind, degrees 1..=5 and 9 (the wide kernel),
    /// without the picks whose interior knot repeats `p + 1` times, which are not curves.
    fn curves(seed: u64) -> Vec<CurveData<3>> {
        let mut rng = Rng::new(seed);
        let mut out = Vec::new();
        for degree in [1, 2, 3, 4, 5, 9] {
            for knots in [
                Knots::Uniform,
                Knots::Random,
                Knots::Clustered,
                Knots::Multiple,
            ] {
                for weights in [Weights::Unit, Weights::Moderate, Weights::NearZero] {
                    let spec = Spec {
                        degree,
                        count: degree + 4,
                        knots,
                    };
                    out.push(sample::curve(&mut rng, spec, weights));
                }
            }
        }
        out.retain(|d| {
            !d.knots
                .iter()
                .filter(|&&k| k > 0.0 && k < 1.0)
                .any(|&k| d.knots.iter().filter(|&&x| x == k).count() > d.degree)
        });
        out
    }

    /// Random parameters across and past the domain, and every knot.
    fn params(rng: &mut Rng, d: &CurveData<3>) -> Vec<f64> {
        let mut us: Vec<f64> = (0..6)
            .map(|_| sample::parameter(rng, &d.knots, (-0.1, 1.1)))
            .collect();
        us.extend(&d.knots);
        us
    }

    fn bits<const D: usize>(v: &[f64; D]) -> [u64; D] {
        v.map(f64::to_bits)
    }

    #[test]
    fn f64_point_is_the_curves_bit_for_bit() {
        let mut rng = Rng::new(11);
        for data in curves(1) {
            let c = ours(&data);
            for u in params(&mut rng, &data) {
                for (side, _) in SIDES {
                    let got = c.point_over(u, side);
                    match c.point(u, side) {
                        Ok(want) => assert_eq!(bits(&got), bits(&want), "{c} at {u} {side}"),
                        Err(_) => assert!(got.iter().any(|x| !x.is_finite())),
                    }
                }
            }
        }
    }

    /// `jet::<K>` against `derivs::<{K + 1}>`, values and errors.
    macro_rules! same_as_derivs {
        ($c:expr, $u:expr, $side:expr, $($k:literal),*) => {$({
            let got = $c.jet::<$k>($u, $side);
            match ($c.derivs::<{ $k + 1 }>($u, $side), got) {
                (Ok(want), Ok((p, d))) => {
                    assert_eq!(bits(&p), bits(&want[0]), "{} at {} order {}", $c, $u, $k);
                    for (g, w) in d.iter().zip(&want[1..]) {
                        assert_eq!(bits(g), bits(w), "{} at {} order {}", $c, $u, $k);
                    }
                }
                (Err(want), Err(got)) => assert_eq!(format!("{want:?}"), format!("{got:?}")),
                (want, got) => panic!("{} at {}: {want:?} vs {got:?}", $c, $u),
            }
        })*};
    }

    #[test]
    fn f64_jet_is_the_derivatives_bit_for_bit() {
        let mut rng = Rng::new(12);
        for data in curves(2) {
            let c = ours(&data);
            for u in params(&mut rng, &data) {
                for (side, _) in SIDES {
                    same_as_derivs!(c, u, side, 0, 1, 2, 3, 4, 5, 9, 31);
                }
            }
            // The over-order and non-finite failures are the derivatives' too.
            same_as_derivs!(c, f64::NAN, Approach::Above, 2, 32);
            same_as_derivs!(c, f64::INFINITY, Approach::Below, 3);
        }
    }

    fn holds(i: Ivl, x: &Rat) -> bool {
        let lo = Rat::from_f64(i.lo).is_none_or(|lo| lo <= *x);
        let hi = Rat::from_f64(i.hi).is_none_or(|hi| *x <= hi);
        lo && hi && !(i.lo.is_nan() || i.hi.is_nan())
    }

    /// Intervals narrow and wide, inside, across knots and past the ends, with the
    /// parameters inside each the exact curve is checked at.
    fn intervals(rng: &mut Rng, d: &CurveData<3>) -> Vec<(Ivl, Vec<f64>)> {
        let mut out = Vec::new();
        for width in [0.0, 1e-9, 1e-3, 0.2, 0.7, 1.6] {
            let a = sample::parameter(rng, &d.knots, (-0.3, 1.1));
            let i = Ivl::new(a, a + width);
            let mut ts = vec![i.lo, i.hi];
            ts.extend(
                (0..2)
                    .map(|_| i.lo + width * rng.next_f64())
                    .filter(|&t| t <= i.hi),
            );
            ts.extend(d.knots.iter().filter(|&&k| i.lo <= k && k <= i.hi));
            ts.sort_by(f64::total_cmp);
            ts.dedup();
            out.push((i, ts));
        }
        out
    }

    #[test]
    fn intervals_hold_the_exact_curve() {
        let mut rng = Rng::new(13);
        // The wide kernel only polynomial: its exact rational derivatives take seconds.
        for data in curves(3)
            .into_iter()
            .filter(|d| d.degree < 9 || d.weights.is_none())
        {
            let c = ours(&data);
            let exact = data.exact().unwrap();
            for (i, ts) in intervals(&mut rng, &data) {
                let point = i.lo == i.hi;
                let got = SIDES.map(|(side, _)| {
                    let (q, d) = c.jet_over::<Ivl, 2>(i, side);
                    [c.point_over(i, side), q, d[0], d[1]]
                });
                for &t in &ts {
                    for (_, tside) in SIDES {
                        // Inside a wide interval both one-sided values are held, at its
                        // ends the one from inside; a point interval holds its side's.
                        let outside = (t == i.lo && tside == tk::Side::Below
                            || t == i.hi && tside == tk::Side::Above)
                            && !point;
                        if outside {
                            continue;
                        }
                        let want = exact.derivs(t, tside, 2).unwrap();
                        for (rows, (side, s)) in got.iter().zip(SIDES) {
                            if point && s != tside {
                                continue;
                            }
                            for (k, row) in rows.iter().enumerate() {
                                let w = &want[k.saturating_sub(1)];
                                for (x, e) in row.iter().zip(&w.exact) {
                                    assert!(
                                        holds(*x, e),
                                        "{c} over {i:?} {side} at {t} row {k}: {x:?} vs {e:?}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn point_intervals_are_tight() {
        let mut rng = Rng::new(14);
        for data in curves(4).into_iter().filter(|d| d.weights.is_none()) {
            let c = ours(&data);
            for u in params(&mut rng, &data) {
                for (side, _) in SIDES {
                    let (Ok(want), got) = (c.point(u, side), c.point_over(Ivl::from(u), side))
                    else {
                        continue;
                    };
                    for (g, w) in got.iter().zip(want) {
                        assert!(g.lo <= w && w <= g.hi, "{c} at {u}");
                        assert!(g.hi - g.lo <= 1e-9 * (1.0 + w.abs()), "{c} at {u}: {g:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn undefined_parameters_and_orders() {
        let c = ours(&curves(5)[0]);
        for u in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(c.point_over(u, Approach::Above).iter().all(|x| x.is_nan()));
        }
        for i in [
            Ivl::new(0.0, f64::INFINITY),
            Ivl::new(f64::NAN, 0.5),
            Ivl::new(0.6, 0.4),
        ] {
            assert_eq!(c.point_over(i, Approach::Above), [Ivl::whole(); 3]);
            assert_eq!(
                c.jet_over::<Ivl, 1>(i, Approach::Above).1,
                [[Ivl::whole(); 3]]
            );
        }
        let (p, d) = c.jet_over::<f64, 32>(0.5, Approach::Above);
        assert!(p.iter().chain(d.iter().flatten()).all(|x| x.is_nan()));
        assert!(matches!(
            c.jet::<32>(0.5, Approach::Above),
            Err(NurbsError::DerivativeOrder { order: 32, .. })
        ));
    }
}
