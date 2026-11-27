// Written for solidcorex. Apache-2.0.
//! Exact plane predicates: the orientation of three points and whether a fourth lies inside
//! the circle through three, as signs that are exact for every finite double input.
//!
//! Each predicate is Shewchuk's adaptive one ("Adaptive Precision Floating-Point Arithmetic
//! and Fast Robust Geometric Predicates", 1997, `orient2dadapt` and `incircleadapt`):
//!
//! - **A.** The determinant in floats, its sign returned when it clears the static bound
//!   (`ccwerrboundA`, `iccerrboundA`). The bound assumes no product underflows, so a
//!   determinant whose magnitude sum is below [`FILTER_FLOOR`] skips this stage.
//! - **B.** The determinant of the rounded coordinate differences, exactly, in double
//!   expansions on the stack; its sign when it clears `ccwerrboundB` / `iccerrboundB`, or
//!   when every difference was exact.
//! - **C.** B plus the differences' first-order rounding errors, against `ccwerrboundC` /
//!   `iccerrboundC`.
//! - **D.** The determinant exactly, in the crate's own expansions, whose terms carry their
//!   own exponents, so no difference or product rounds, overflows or underflows.
//!
//! B and C hold only while no product in them underflows or overflows, so they run only
//! when every coordinate is zero or of magnitude within `[2^-200, 2^250]` (every
//! coordinate a model holds); otherwise D decides at once. Each stage returns only a sign
//! it has proved, so the result is the exact sign, a function of the input bits alone, the
//! same on every target.
//!
//! An input that is not finite is a caller's bug; the result is then unspecified (never a
//! panic).

use core::cmp::Ordering;

use crate::eft::{fast_two_sum, two_prod, two_sum};
use crate::expansion::Expansion as X;
use crate::vector2d::Point2;

/// Half the machine epsilon, `2^-53`.
const EPS: f64 = f64::EPSILON / 2.0;

/// Below this magnitude sum a product may have underflowed, so the static filter's bound
/// does not hold and the exact path decides.
pub const FILTER_FLOOR: f64 = 1e-250;

/// The sign of the orientation of `a`, `b`, `c`: `Greater` when `c` lies left of the
/// directed line from `a` to `b` (the three counter-clockwise), `Less` when right, `Equal`
/// when the three are collinear. Exact.
#[must_use]
pub fn orient2d(a: Point2, b: Point2, c: Point2) -> Ordering {
    let l = (a.x - c.x) * (b.y - c.y);
    let r = (a.y - c.y) * (b.x - c.x);
    let det = l - r;
    let sum = l.abs() + r.abs();
    let bound = (3.0 + 16.0 * EPS) * EPS * sum;
    if sum >= FILTER_FLOOR && sum.is_finite() {
        if det > bound {
            return Ordering::Greater;
        }
        if -det > bound {
            return Ordering::Less;
        }
    }
    if in_range(&[a, b, c])
        && let Some(s) = orient2d_adapt(a, b, c, sum)
    {
        return s;
    }
    orient2d_exact(a, b, c)
}

/// Stages B and C of [`orient2d`]; `None` when neither proves the sign.
fn orient2d_adapt(a: Point2, b: Point2, c: Point2, detsum: f64) -> Option<Ordering> {
    let (acx, bcx) = (a.x - c.x, b.x - c.x);
    let (acy, bcy) = (a.y - c.y, b.y - c.y);
    let (l1, l0) = two_prod(acx, bcy);
    let (r1, r0) = two_prod(acy, bcx);
    let bb = two_two_diff(l1, l0, r1, r0);
    let mut det = bb.estimate();
    let bound = CCW_B * detsum;
    if det >= bound || -det >= bound {
        return Some(sgn(det));
    }
    let tails = [
        diff_tail(a.x, c.x, acx),
        diff_tail(b.x, c.x, bcx),
        diff_tail(a.y, c.y, acy),
        diff_tail(b.y, c.y, bcy),
    ];
    if tails == [0.0; 4] {
        return Some(bb.sign());
    }
    let [acxt, bcxt, acyt, bcyt] = tails;
    let bound = CCW_C * detsum + RESULT * det.abs();
    det += (acx * bcyt + bcy * acxt) - (acy * bcxt + bcx * acyt);
    (det >= bound || -det >= bound).then(|| sgn(det))
}

/// [`orient2d`] by the exact path alone.
#[must_use]
pub fn orient2d_exact(a: Point2, b: Point2, c: Point2) -> Ordering {
    let (acx, acy) = (X::difference(a.x, c.x), X::difference(a.y, c.y));
    let (bcx, bcy) = (X::difference(b.x, c.x), X::difference(b.y, c.y));
    sign(&acx.mul(&bcy).sub(&acy.mul(&bcx)))
}

/// Whether `d` lies inside the circle through `a`, `b`, `c`, given counter-clockwise:
/// `Greater` inside, `Less` outside, `Equal` on it (or the three collinear). For `a`, `b`,
/// `c` clockwise the sign is reversed. Exact.
#[must_use]
pub fn incircle(a: Point2, b: Point2, c: Point2, d: Point2) -> Ordering {
    let (adx, ady) = (a.x - d.x, a.y - d.y);
    let (bdx, bdy) = (b.x - d.x, b.y - d.y);
    let (cdx, cdy) = (c.x - d.x, c.y - d.y);
    let (bdxcdy, cdxbdy) = (bdx * cdy, cdx * bdy);
    let (cdxady, adxcdy) = (cdx * ady, adx * cdy);
    let (adxbdy, bdxady) = (adx * bdy, bdx * ady);
    let alift = adx * adx + ady * ady;
    let blift = bdx * bdx + bdy * bdy;
    let clift = cdx * cdx + cdy * cdy;
    let det = alift * (bdxcdy - cdxbdy) + blift * (cdxady - adxcdy) + clift * (adxbdy - bdxady);
    let permanent = (bdxcdy.abs() + cdxbdy.abs()) * alift
        + (cdxady.abs() + adxcdy.abs()) * blift
        + (adxbdy.abs() + bdxady.abs()) * clift;
    let bound = (10.0 + 96.0 * EPS) * EPS * permanent;
    if permanent >= FILTER_FLOOR && permanent.is_finite() {
        if det > bound {
            return Ordering::Greater;
        }
        if -det > bound {
            return Ordering::Less;
        }
    }
    if in_range(&[a, b, c, d])
        && let Some(s) = incircle_adapt(a, b, c, d, permanent)
    {
        return s;
    }
    incircle_exact(a, b, c, d)
}

/// Stages B and C of [`incircle`]; `None` when neither proves the sign.
fn incircle_adapt(a: Point2, b: Point2, c: Point2, d: Point2, permanent: f64) -> Option<Ordering> {
    let (adx, ady) = (a.x - d.x, a.y - d.y);
    let (bdx, bdy) = (b.x - d.x, b.y - d.y);
    let (cdx, cdy) = (c.x - d.x, c.y - d.y);
    // `x * (p * q - r * s) * x + y * (...) * y` for one row, exactly.
    let row = |x: f64, y: f64, p: f64, q: f64, r: f64, s: f64| {
        let (l1, l0) = two_prod(p, q);
        let (r1, r0) = two_prod(r, s);
        let m = two_two_diff(l1, l0, r1, r0);
        let xx = m.scale::<8>(x).scale::<16>(x);
        let yy = m.scale::<8>(y).scale::<16>(y);
        xx.sum::<16, 32>(&yy)
    };
    let adet = row(adx, ady, bdx, cdy, cdx, bdy);
    let bdet = row(bdx, bdy, cdx, ady, adx, cdy);
    let cdet = row(cdx, cdy, adx, bdy, bdx, ady);
    let fin = adet.sum::<32, 64>(&bdet).sum::<32, 96>(&cdet);
    let mut det = fin.estimate();
    let bound = ICC_B * permanent;
    if det >= bound || -det >= bound {
        return Some(sgn(det));
    }
    let tails = [
        diff_tail(a.x, d.x, adx),
        diff_tail(a.y, d.y, ady),
        diff_tail(b.x, d.x, bdx),
        diff_tail(b.y, d.y, bdy),
        diff_tail(c.x, d.x, cdx),
        diff_tail(c.y, d.y, cdy),
    ];
    if tails == [0.0; 6] {
        return Some(fin.sign());
    }
    let [adxt, adyt, bdxt, bdyt, cdxt, cdyt] = tails;
    let bound = ICC_C * permanent + RESULT * det.abs();
    det += ((adx * adx + ady * ady) * ((bdx * cdyt + cdy * bdxt) - (bdy * cdxt + cdx * bdyt))
        + 2.0 * (adx * adxt + ady * adyt) * (bdx * cdy - bdy * cdx))
        + ((bdx * bdx + bdy * bdy) * ((cdx * adyt + ady * cdxt) - (cdy * adxt + adx * cdyt))
            + 2.0 * (bdx * bdxt + bdy * bdyt) * (cdx * ady - cdy * adx))
        + ((cdx * cdx + cdy * cdy) * ((adx * bdyt + bdy * adxt) - (ady * bdxt + bdx * adyt))
            + 2.0 * (cdx * cdxt + cdy * cdyt) * (adx * bdy - ady * bdx));
    (det >= bound || -det >= bound).then(|| sgn(det))
}

/// [`incircle`] by the exact path alone.
#[must_use]
pub fn incircle_exact(a: Point2, b: Point2, c: Point2, d: Point2) -> Ordering {
    let diff = |p: Point2| (X::difference(p.x, d.x), X::difference(p.y, d.y));
    let ((adx, ady), (bdx, bdy), (cdx, cdy)) = (diff(a), diff(b), diff(c));
    let lift = |x: &X, y: &X| x.mul(x).add(&y.mul(y));
    let cross = |px: &X, py: &X, qx: &X, qy: &X| px.mul(qy).sub(&qx.mul(py));
    let det = lift(&adx, &ady)
        .mul(&cross(&bdx, &bdy, &cdx, &cdy))
        .add(&lift(&bdx, &bdy).mul(&cross(&cdx, &cdy, &adx, &ady)))
        .add(&lift(&cdx, &cdy).mul(&cross(&adx, &ady, &bdx, &bdy)));
    sign(&det)
}

fn sign(x: &X) -> Ordering {
    x.sign().cmp(&0)
}

/// The sign of a double, zero of either sign `Equal`.
fn sgn(v: f64) -> Ordering {
    if v > 0.0 {
        Ordering::Greater
    } else if v < 0.0 {
        Ordering::Less
    } else {
        Ordering::Equal
    }
}

const CCW_B: f64 = (2.0 + 12.0 * EPS) * EPS;
const CCW_C: f64 = (9.0 + 64.0 * EPS) * EPS * EPS;
const ICC_B: f64 = (4.0 + 48.0 * EPS) * EPS;
const ICC_C: f64 = (44.0 + 576.0 * EPS) * EPS * EPS;
const RESULT: f64 = (3.0 + 8.0 * EPS) * EPS;

/// Whether every coordinate is zero or within `[2^-200, 2^250]`: then each value stages B
/// and C form is a multiple of the least coordinate ulp to the fourth (at least `2^-1008`,
/// normal) and below `2^1010`, so no product or sum in them underflows or overflows.
fn in_range(points: &[Point2]) -> bool {
    const LO: f64 = 6.223_015_277_861_142e-61; // 2^-200
    const HI: f64 = 1.809_251_394_333_065_6e75; // 2^250
    points.iter().all(|p| {
        [p.x, p.y]
            .iter()
            .all(|&v| v == 0.0 || (LO..=HI).contains(&v.abs()))
    })
}

/// The rounding error of `x = fl(a - b)`.
fn diff_tail(a: f64, b: f64, x: f64) -> f64 {
    let bv = a - x;
    let av = x + bv;
    (a - av) + (bv - b)
}

/// `(a1 + a0) - (b1 + b0)` exactly, as four terms (Shewchuk's `Two_Two_Diff`).
fn two_two_diff(a1: f64, a0: f64, b1: f64, b0: f64) -> Dx<4> {
    let (i, x0) = two_sum(a0, -b0);
    let (j, z) = two_sum(a1, i);
    let (i, x1) = two_sum(z, -b1);
    let (x3, x2) = two_sum(j, i);
    Dx {
        t: [x0, x1, x2, x3],
        n: 4,
    }
}

/// A double expansion of at most `N` terms on the stack: nonoverlapping, in increasing
/// magnitude, zeros dropped (but for a lone zero), as Shewchuk's routines keep them.
#[derive(Clone, Copy)]
struct Dx<const N: usize> {
    t: [f64; N],
    n: usize,
}

impl<const N: usize> Dx<N> {
    fn terms(&self) -> &[f64] {
        &self.t[..self.n]
    }

    fn estimate(&self) -> f64 {
        self.terms().iter().sum()
    }

    /// The exact sign: the largest term's.
    fn sign(&self) -> Ordering {
        self.terms()
            .iter()
            .rev()
            .find(|&&v| v != 0.0)
            .map_or(Ordering::Equal, |&v| sgn(v))
    }

    fn new<const M: usize>() -> Dx<M> {
        Dx { t: [0.0; M], n: 0 }
    }

    /// Appends `v`; the callers' `M` bounds make room for every term.
    fn push(&mut self, v: f64) {
        if let Some(slot) = self.t.get_mut(self.n) {
            *slot = v;
            self.n += 1;
        }
    }

    /// Appends `v` unless it is zero.
    fn push_nonzero(&mut self, v: f64) {
        if v != 0.0 {
            self.push(v);
        }
    }

    /// `self * b` (`scale_expansion_zeroelim`); `M >= 2 N`.
    fn scale<const M: usize>(&self, b: f64) -> Dx<M> {
        let mut h = Self::new::<M>();
        let e = self.terms();
        let Some((&first, rest)) = e.split_first() else {
            return h;
        };
        let (mut q, hh) = two_prod(first, b);
        h.push_nonzero(hh);
        for &ei in rest {
            let (p1, p0) = two_prod(ei, b);
            let (sum, hh) = two_sum(q, p0);
            h.push_nonzero(hh);
            let (nq, hh) = fast_two_sum(p1, sum);
            h.push_nonzero(hh);
            q = nq;
        }
        if q != 0.0 || h.n == 0 {
            h.push(q);
        }
        h
    }

    /// `self + f` (`fast_expansion_sum_zeroelim`); `M >= N + K`.
    fn sum<const K: usize, const M: usize>(&self, f: &Dx<K>) -> Dx<M> {
        let mut h = Self::new::<M>();
        let (e, f) = (self.terms(), f.terms());
        let (mut ei, mut fi) = (0, 0);
        // The next term of smaller magnitude.
        let take = |ei: &mut usize, fi: &mut usize| -> f64 {
            match (e.get(*ei), f.get(*fi)) {
                (Some(&en), Some(&fn_)) if (fn_ > en) == (fn_ > -en) => {
                    *ei += 1;
                    en
                }
                (Some(&en), None) => {
                    *ei += 1;
                    en
                }
                (_, Some(&fn_)) => {
                    *fi += 1;
                    fn_
                }
                (None, None) => 0.0,
            }
        };
        if e.is_empty() && f.is_empty() {
            return h;
        }
        let mut q = take(&mut ei, &mut fi);
        let mut first = true;
        while ei < e.len() || fi < f.len() {
            let both = ei < e.len() && fi < f.len();
            let next = take(&mut ei, &mut fi);
            let (nq, hh) = if first && both {
                fast_two_sum(next, q)
            } else {
                two_sum(q, next)
            };
            first = false;
            h.push_nonzero(hh);
            q = nq;
        }
        if q != 0.0 || h.n == 0 {
            h.push(q);
        }
        h
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]
    use super::*;
    use crate::hash::Rng;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    #[test]
    fn orientation_signs() {
        let (a, b) = (p(0.0, 0.0), p(1.0, 0.0));
        assert_eq!(orient2d(a, b, p(0.5, 1.0)), Ordering::Greater);
        assert_eq!(orient2d(a, b, p(0.5, -1.0)), Ordering::Less);
        assert_eq!(orient2d(a, b, p(7.0, 0.0)), Ordering::Equal);
    }

    #[test]
    fn orientation_is_exact_on_the_classic_near_collinear_grid() {
        // Shewchuk's figure: points near (0.5, 0.5) on a grid of ulps, against the line
        // through (12, 12) and (24, 24). The float determinant gets many of these wrong.
        let (b, c) = (p(12.0, 12.0), p(24.0, 24.0));
        let ulp = 0.5_f64.next_up() - 0.5;
        for i in 0..64 {
            for j in 0..64 {
                let a = p(0.5 + f64::from(i) * ulp, 0.5 + f64::from(j) * ulp);
                // Exactly the sign of a.y - a.x: b and c lie on y = x, b before c.
                let truth = j.cmp(&i);
                assert_eq!(orient2d(a, b, c), truth, "{i} {j}");
                assert_eq!(orient2d_exact(a, b, c), truth);
            }
        }
    }

    #[test]
    fn orientation_holds_at_extreme_exponents() {
        let tiny = 5e-324;
        assert_eq!(
            orient2d(p(0.0, 0.0), p(tiny, 0.0), p(0.0, tiny)),
            Ordering::Greater
        );
        assert_eq!(
            orient2d(p(0.0, 0.0), p(tiny, tiny), p(2.0 * tiny, 2.0 * tiny)),
            Ordering::Equal
        );
        let big = 1e300;
        assert_eq!(
            orient2d(p(-big, -big), p(big, -big), p(0.0, big)),
            Ordering::Greater
        );
        assert_eq!(
            orient2d(p(-big, 0.0), p(big, 0.0), p(0.0, -tiny)),
            Ordering::Less
        );
    }

    #[test]
    fn incircle_signs_and_cocircular_points() {
        let (a, b, c) = (p(1.0, 0.0), p(0.0, 1.0), p(-1.0, 0.0));
        assert_eq!(incircle(a, b, c, p(0.0, 0.0)), Ordering::Greater);
        assert_eq!(incircle(a, b, c, p(0.0, -1.0)), Ordering::Equal);
        assert_eq!(
            incircle(a, b, c, p(0.0, (-1.0_f64).next_down())),
            Ordering::Less
        );
        assert_eq!(incircle(c, b, a, p(0.0, 0.0)), Ordering::Less);
        // A unit square's corners, shifted far off: exactly cocircular.
        let o = 1e6 + 0.125;
        let sq = [p(o, o), p(o + 1.0, o), p(o + 1.0, o + 1.0), p(o, o + 1.0)];
        assert_eq!(incircle(sq[0], sq[1], sq[2], sq[3]), Ordering::Equal);
        let next = p(o, (o + 1.0).next_up());
        assert_eq!(incircle(sq[0], sq[1], sq[2], next), Ordering::Less);
        let prev = p(o, (o + 1.0).next_down());
        assert_eq!(incircle(sq[0], sq[1], sq[2], prev), Ordering::Greater);
    }

    #[test]
    fn the_filter_agrees_with_the_exact_path() {
        let mut rng = Rng::new(0x5eed_7e55);
        for k in 0..20_000 {
            // Some well spread, some on a lattice of near-degenerate points.
            let q = |rng: &mut Rng| {
                if k % 2 == 0 {
                    p(rng.next_signed_unit(), rng.next_signed_unit())
                } else {
                    let s = 1.0 / 1024.0;
                    p(
                        (rng.next_f64() * 8.0).floor() * s,
                        (rng.next_f64() * 8.0).floor() * s,
                    )
                }
            };
            let (a, b, c, d) = (q(&mut rng), q(&mut rng), q(&mut rng), q(&mut rng));
            assert_eq!(orient2d(a, b, c), orient2d_exact(a, b, c));
            assert_eq!(incircle(a, b, c, d), incircle_exact(a, b, c, d));
        }
    }

    #[test]
    fn the_adaptive_stages_agree_with_the_exact_path_near_degeneracy() {
        // Points rounded onto a circle and a line about the origin: the float determinants
        // are noise and the differences inexact, so stages B and C decide some.
        let mut rng = Rng::new(0xc1c1e);
        let (cx, cy, r) = (0.3, -0.7, 50.0);
        let on_circle = |t: f64| p(cx + r * crate::fmath::cos(t), cy + r * crate::fmath::sin(t));
        for _ in 0..20_000 {
            let mut t = || rng.next_f64() * core::f64::consts::TAU;
            let (a, b, c, d) = (
                on_circle(t()),
                on_circle(t()),
                on_circle(t()),
                on_circle(t()),
            );
            assert_eq!(incircle(a, b, c, d), incircle_exact(a, b, c, d));
            let (u, v) = (rng.next_signed_unit(), rng.next_signed_unit());
            let on_line = |s: f64| p(cx + s * 3.0, cy - s * 7.0);
            let (a, b, c) = (on_line(u), on_line(v), on_line(u * v));
            assert_eq!(orient2d(a, b, c), orient2d_exact(a, b, c));
            let tiny = p(a.x * 1e-70, a.y * 1e-70);
            assert_eq!(orient2d(tiny, b, c), orient2d_exact(tiny, b, c));
        }
    }

    #[test]
    fn the_predicates_are_invariant_under_cyclic_order_and_flip_sign_on_a_swap() {
        let mut rng = Rng::new(17);
        for _ in 0..2_000 {
            let s = 1.0 / 16.0;
            let mut q = || {
                p(
                    (rng.next_f64() * 4.0).floor() * s,
                    (rng.next_f64() * 4.0).floor() * s,
                )
            };
            let (a, b, c, d) = (q(), q(), q(), q());
            assert_eq!(orient2d(a, b, c), orient2d(b, c, a));
            assert_eq!(orient2d(a, b, c), orient2d(b, a, c).reverse());
            assert_eq!(incircle(a, b, c, d), incircle(b, c, a, d));
            assert_eq!(incircle(a, b, c, d), incircle(b, a, c, d).reverse());
        }
    }
}
