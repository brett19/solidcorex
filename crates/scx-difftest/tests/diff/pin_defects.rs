// Written for solidcorex. Apache-2.0.
//! The inputs on which the pinned solidcore is known to be wrong, one [`PinDefect`] per test.
//! On those inputs the pin is not compared; scx's result must pass the entry's check against
//! the true value instead. Each entry gives its evidence: the true value, scx's error and the
//! pin's.

use crate::harness::{Applies, PinDefect};
use scx_num::certified::Verdict;

type M = [[f64; 4]; 4];

/// `Affine::is_regular` on a linear part whose largest entry is of `f64::MAX`'s scale
/// (its correctly rounded `log2` is 1024), at a positive finite `ratio`.
///
/// The pin scales the matrix by `f64::powi(2, -floor(log2(big)))`: `log2` of `f64::MAX` is
/// `1024 - 1.6e-16`, which rounds to 1024, and `powi(2, -1024)` computes `1 / 2^1024 =
/// 1 / inf = 0`. The scaled matrix is zero and the pin answers `Undecided`. scx reads the
/// exponent from the bits (1023) and scales by the subnormal `2^-1023`.
///
/// The true value, for the pool's matrices (the identity but one off-diagonal entry `x`,
/// `|x|` at least `2^1023`): the linear part's singular values are 1 and those of
/// `[[1, x], [0, 1]]`, whose product is 1 and the larger of which exceeds `|x|`. Its
/// conditioning is below `1 / x^2 <= 2^-2046`, under every positive double, so the true
/// answer is `No` at every positive `ratio`. The pin answers `Undecided`. scx answers `No`
/// from `ratio` 1e-160 up (156 inputs), and `Undecided`, which is sound, below: it proves
/// `No` from the rows' lengths with an absolute margin of 1e-320 on their squares, which
/// decides once `ratio^2 * 4` (the largest scaled row's square is about 4, the smallest
/// underflows) passes `2e-320`. Below 1e-160 what it measures is inside that margin, so
/// `Undecided` is the honest answer there, not a miss: the check accepts it only there, and
/// `Yes` nowhere. The pin's `Undecided` is the same answer below 1e-160, so no defect shows
/// there; the check still holds scx to it.
pub fn tr_is_regular() -> PinDefect<(M, f64), Verdict> {
    PinDefect {
        reason: "the pin's scale powi(2, -1024) is 0",
        applies: Applies::Inputs(|(t, ratio)| {
            let t = &crate::facade::affine_part(*t);
            let finite = t.iter().flatten().all(|x| x.is_finite());
            let big = linear(t).fold(0.0_f64, |b, v| b.max(v.abs()));
            finite && ratio.is_finite() && *ratio > 0.0 && scx_num::fmath::log2(big) == 1024.0
        }),
        check: |(t, ratio), v| {
            // the identity but one off-diagonal entry: the only shape with a true value here
            let off: Vec<(usize, usize)> = (0..3)
                .flat_map(|i| (0..3).map(move |j| (i, j)))
                .filter(|&(i, j)| t[i][j] != if i == j { 1.0 } else { 0.0 })
                .collect();
            match off.as_slice() {
                [(i, j)] if i != j => match v {
                    Verdict::No => Ok(()),
                    Verdict::Undecided if *ratio < 1e-160 => Ok(()),
                    _ => Err(format!("the true answer is No; scx says {v:?}")),
                },
                _ => Err("no true value known for this matrix: extend the check".to_owned()),
            }
        },
    }
}

/// `outer * inner` (the pin: `inner.concat(&outer)`) where the outer map's translation is not
/// finite.
///
/// The pin's `smgu_4x4Mult` sums all four products into each entry, so each entry of the
/// linear part takes `0 * outer translation`: NaN for a translation infinite or NaN. The linear
/// part of a composition is the product of the linear parts, which no translation enters; scx
/// sums only those three products. The true value is that product: the check computes each
/// entry from the inputs, in the pin's order without the fourth term, and the translation with
/// it, and asks for scx's entries to be those bits (NaN where the product itself is NaN).
pub fn tr_concat() -> PinDefect<(M, M), scx_num::math::Affine> {
    PinDefect {
        reason: "the pin's linear part takes 0 * a translation not finite",
        applies: Applies::Disagreeing,
        check: |(inner, outer), r| {
            let (a, b) = (
                crate::facade::affine_part(*inner),
                crate::facade::affine_part(*outer),
            );
            let got: M = (*r).into();
            for i in 0..4 {
                for j in 0..3 {
                    let mut want = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
                    if i == 3 {
                        want += b[3][j];
                    }
                    let g = got[i][j];
                    if !(g == want || (g.is_nan() && want.is_nan())) {
                        return Err(format!("entry [{i}][{j}] is {g}, the product's {want}"));
                    }
                }
            }
            Ok(())
        },
    }
}

/// The linear part's nine entries.
fn linear(t: &M) -> impl Iterator<Item = f64> + '_ {
    t.iter().take(3).flat_map(|r| r.iter().take(3).copied())
}

type P = [f64; 2];
type Circle = Option<([f64; 2], f64, f64)>;

/// `three_point_circle_2d(s, e, t)` where the pin's plain-`f64` evaluation is ill-conditioned:
/// the forward error bound of its every step ([`PinError`], each rounding a relative `2^-53`
/// plus an absolute `2^-1074`, through the pin's own formulas), on the centre, the radius or
/// the sweep, passes a quarter of `Tol::DEFAULT`, or a decision it takes (the area against
/// `1e-12`, the chord and the radius against `1e-9`, the long or short arc) is within its
/// bound of the other side, or a step overflows.
///
/// The pin computes `t - s` and `e - s` (exact to a rounding each), then the centre
/// `s + (perp numerators) / d` in `f64`: the numerators and `d` cancel when the points are
/// nearly on one line or of very different sizes (`1e20` beside unit or subnormal points),
/// with a condition number up to `1e30` on the pool. The radius is the distance from that
/// centre, and the sweep `2 asin(chord / 2r)` multiplies the radius's error by
/// `1 / sqrt(1 - q^2)` as `q` nears 1. Its squares of differences overflow from about
/// `1e154`, where the circle is finite and the pin answers `None`.
///
/// The true value is computed in the check, exactly in integers ([`scx_testkit::exact`]): the
/// area and the decisions exactly, the centre `t + N / (2 a x b)` and the radius
/// `|s - centre|` from exact numerators each rounded once (within `2^-51`; whether each is
/// finite decided exactly, against `2^1024 - 2^970`), and the sweep as
/// `atan2` of the exact cross and dot products of `s - centre` and `e - centre` with the long
/// or short arc by the exact side of the chord `t` and the centre are on: a different route
/// from scx's (the inscribed angle at `t`, in expansions with unbounded exponents). scx must
/// match it within `Tol::DEFAULT`, on every input.
///
/// The worst input before scx was exact: `(1, 0)`, `(1e-13, 1)`, `(1e-6, 1e-6)`: true sweep
/// `3.1415886535859934`, the pin's off by `1.7e-10` relative and the old scx's (the pin's
/// method with a correctly rounded `hypot`) by `1.5e-11`; on the pool's ill-conditioned
/// inputs both were off by up to O(1), and agreed only where they ran the same steps.
pub fn three_point_circle_2d() -> PinDefect<(P, P, P), Circle> {
    PinDefect {
        reason: "the pin's f64 circumcentre and asin sweep are ill-conditioned here",
        applies: Applies::Inputs(|&(s, e, t)| pin_ill_conditioned(s, e, t)),
        check: |&(s, e, t), got| {
            let want = true_circle(s, e, t);
            let tol = crate::harness::Tol::DEFAULT;
            let agree = match (want, *got) {
                (None, None) => true,
                (Some((wc, wr, ws)), Some((gc, gr, gs))) => {
                    tol.floats_agree(wc[0], gc[0])
                        && tol.floats_agree(wc[1], gc[1])
                        && tol.floats_agree(wr, gr)
                        && tol.floats_agree(ws, gs)
                }
                _ => false,
            };
            if agree {
                Ok(())
            } else {
                Err(format!("true circle {want:?}, scx {got:?}"))
            }
        },
    }
}

/// A double the pin computed and a bound on its distance from the true value of the step.
#[derive(Clone, Copy, Debug)]
struct PinError {
    v: f64,
    e: f64,
}

/// `2^-53`, the relative rounding error, and `2^-1074`, the absolute one in the subnormals.
const U: f64 = 1.1102230246251565e-16;
const ETA: f64 = 4.9406564584124654e-324;

impl PinError {
    fn exact(v: f64) -> Self {
        PinError { v, e: 0.0 }
    }

    /// `v`, rounded once from a value within `e` of the truth.
    fn rounded(v: f64, e: f64) -> Self {
        let e = if v.is_finite() {
            e + U * v.abs() + ETA
        } else {
            f64::INFINITY
        };
        PinError { v, e }
    }

    fn add(self, o: Self) -> Self {
        Self::rounded(self.v + o.v, self.e + o.e)
    }

    fn sub(self, o: Self) -> Self {
        Self::rounded(self.v - o.v, self.e + o.e)
    }

    fn mul(self, o: Self) -> Self {
        Self::rounded(
            self.v * o.v,
            self.v.abs() * o.e + o.v.abs() * self.e + self.e * o.e,
        )
    }

    fn div(self, o: Self) -> Self {
        let q = self.v / o.v;
        let room = o.v.abs() - o.e;
        if room > 0.0 {
            Self::rounded(q, (self.e + q.abs() * o.e) / room)
        } else {
            PinError {
                v: q,
                e: f64::INFINITY,
            }
        }
    }

    fn hypot(x: Self, y: Self) -> Self {
        Self::rounded(scx_num::fmath::hypot(x.v, y.v), x.e + y.e)
    }

    /// Whether the true value may be on the other side of `at` (or on it).
    fn straddles(self, at: f64) -> bool {
        (self.v - at).abs() <= self.e + 8.0 * U * self.v.abs()
    }

    /// Whether the bound passes a quarter of `Tol::DEFAULT` on this value.
    fn loose(self) -> bool {
        let tol = crate::harness::Tol::DEFAULT;
        self.e.is_nan() || self.e > 0.25 * tol.abs.max(tol.rel * self.v.abs())
    }
}

/// Whether the pin's evaluation (its steps, one by one, as `scx_num::math` had them before
/// it was made exact) cannot be trusted to `Tol::DEFAULT / 4` on these points.
fn pin_ill_conditioned(s: P, e: P, t: P) -> bool {
    type E = PinError;
    if !s.iter().chain(&e).chain(&t).all(|v| v.is_finite()) {
        return false;
    }
    let [sx, sy] = s.map(E::exact);
    let [ex, ey] = e.map(E::exact);
    let [tx, ty] = t.map(E::exact);
    // circumcentre_2d(s, t, e)
    let (abx, aby) = (tx.sub(sx), ty.sub(sy));
    let (acx, acy) = (ex.sub(sx), ey.sub(sy));
    let ab2 = abx.mul(abx).add(aby.mul(aby));
    let ac2 = acx.mul(acx).add(acy.mul(acy));
    let d = E::exact(2.0).mul(abx.mul(acy).sub(aby.mul(acx)));
    if !d.v.is_finite() {
        return true;
    }
    let abs_d = E {
        v: d.v.abs(),
        e: d.e,
    };
    if d.v.abs() < 1e-12 && !abs_d.straddles(1e-12) {
        return false;
    }
    let cx = sx.add(acy.mul(ab2).sub(aby.mul(ac2)).div(d));
    let cy = sy.add(abx.mul(ac2).sub(acx.mul(ab2)).div(d));
    if !(cx.v.is_finite() && cy.v.is_finite()) || d.straddles(1e-12) || d.straddles(-1e-12) {
        return true;
    }
    // three_point_circle_2d
    let r = E::hypot(cx.sub(sx), cy.sub(sy));
    let (chx, chy) = (ex.sub(sx), ey.sub(sy));
    let len = E::hypot(chx, chy);
    if !(r.v.is_finite() && len.v.is_finite()) {
        return true;
    }
    if len.straddles(1e-9) || r.straddles(1e-9) {
        return true;
    }
    if len.v <= 1e-9 || r.v <= 1e-9 {
        return false;
    }
    let (nx, ny) = (E::exact(0.0).sub(chy.div(len)), chx.div(len));
    let half = E::exact(0.5);
    let (mx, my) = (sx.add(ex).mul(half), sy.add(ey).mul(half));
    let side_t = nx.mul(tx.sub(mx)).add(ny.mul(ty.sub(my)));
    let side_c = nx.mul(cx.sub(mx)).add(ny.mul(cy.sub(my)));
    if side_t.straddles(0.0) || side_c.straddles(0.0) {
        return true;
    }
    let q = len.div(E::exact(2.0).mul(r));
    let asin = |x: f64| scx_num::fmath::asin(x.clamp(-1.0, 1.0));
    let mv = 2.0 * asin(q.v);
    let me = 2.0 * (asin(q.v + q.e) - asin(q.v)).max(asin(q.v) - asin(q.v - q.e));
    let minor = E::rounded(mv, me);
    let long = side_t.v * side_c.v > 0.0;
    // TAU itself is 2.4e-16 from 2 pi
    let sweep = if long {
        E::rounded(core::f64::consts::TAU - minor.v, minor.e + 2.5e-16)
    } else {
        minor
    };
    cx.loose() || cy.loose() || r.loose() || sweep.loose()
}

/// The true `three_point_circle_2d(s, e, t)`, in exact integers, each result rounded once
/// (within `2^-51`): see [`three_point_circle_2d`].
fn true_circle(s: P, e: P, t: P) -> Circle {
    use core::cmp::Ordering::{Greater, Less};
    use scx_testkit::exact::{ldexp, ratio, sqrt_ratio, Int};
    if !s.iter().chain(&e).chain(&t).all(|v| v.is_finite()) {
        return None;
    }
    let ([sx, sy, ex, ey, tx, ty], u) = Int::of_doubles([s[0], s[1], e[0], e[1], t[0], t[1]]);
    let ([thr_d, thr_l], v) = Int::of_doubles([1e-12, 1e-9]);
    let (ax, ay) = (sx.sub(&tx), sy.sub(&ty));
    let (bx, by) = (ex.sub(&tx), ey.sub(&ty));
    let (cx, cy) = (ex.sub(&sx), ey.sub(&sy));
    let cross = ax.mul(&by).sub(&ay.mul(&bx));
    let d = cross.add(&cross);
    // twice the area at least 1e-12
    if d.cmp_scaled(2 * u, &thr_d, v) == Less {
        return None;
    }
    let sq = |x: &Int, y: &Int| x.mul(x).add(&y.mul(y));
    let (a2, b2, c2) = (sq(&ax, &ay), sq(&bx, &by), sq(&cx, &cy));
    // the chord above 1e-9, and finite
    let thr_l2 = thr_l.mul(&thr_l);
    // A result is finite when below 2^1024 - 2^970 (halfway past the largest double), in
    // units of 2^w: decided exactly, as the rounding near it is not.
    let ([max, half], w) = Int::of_doubles([f64::MAX, ldexp(1.0, 970)]);
    let top = max.add(&half);
    let top2 = top.mul(&top);
    let clamp = |x: f64| x.clamp(-f64::MAX, f64::MAX);
    if c2.cmp_scaled(2 * u, &thr_l2, 2 * v) != Greater || c2.cmp_scaled(2 * u, &top2, 2 * w) != Less
    {
        return None;
    }
    // the centre's offset from t is N / d
    let nx = by.mul(&a2).sub(&ay.mul(&b2));
    let ny = ax.mul(&b2).sub(&bx.mul(&a2));
    // s - centre = (a d - N) / d, e - centre = (b d - N) / d
    let (px, py) = (ax.mul(&d).sub(&nx), ay.mul(&d).sub(&ny));
    let (qx, qy) = (bx.mul(&d).sub(&nx), by.mul(&d).sub(&ny));
    // the radius above 1e-9, and finite: |p|^2 > 1e-18 d^2
    let p2 = sq(&px, &py);
    if p2.cmp_scaled(2 * u, &thr_l2.mul(&d.mul(&d)), 2 * v) != Greater {
        return None;
    }
    if p2.cmp_scaled(2 * u, &top2.mul(&d.mul(&d)), 2 * w) != Less {
        return None;
    }
    let r = clamp(sqrt_ratio(&p2, &d.mul(&d), 2 * u));
    let (cnx, cny) = (tx.mul(&d).add(&nx), ty.mul(&d).add(&ny));
    let centre = [clamp(ratio(&cnx, &d, u)), clamp(ratio(&cny, &d, u))];
    if [&cnx, &cny]
        .iter()
        .any(|c| c.cmp_scaled(u, &top.mul(&d), w) != Less)
    {
        return None;
    }
    // p x q = d (d cross + (b - a) x N), exactly; p . q from rounded parts (no cancellation
    // that matters: its error moves the angle by a relative 2^-51).
    let bma = (bx.sub(&ax), by.sub(&ay));
    let pxq = d.mul(&d.mul(&cross).add(&bma.0.mul(&ny).sub(&bma.1.mul(&nx))));
    let f = |x: &Int| x.approx();
    let ((mpx, kpx), (mpy, kpy), (mqx, kqx), (mqy, kqy)) = (f(&px), f(&py), f(&qx), f(&qy));
    let top = [kpx, kpy, kqx, kqy].into_iter().max().unwrap_or(0);
    let g = |m: f64, k: i64| ldexp(m, k - top);
    let dot = g(mpx, kpx) * g(mqx, kqx) + g(mpy, kpy) * g(mqy, kqy);
    let (mc, kc) = pxq.approx();
    let crs = ldexp(mc.abs(), kc - 2 * top);
    let minor = scx_num::fmath::atan2(crs, dot);
    // long when t and the centre are on one side of the chord: n = (-cy, cx),
    // side_t ~ n . (2t - s - e), side_c ~ n . (2 N + (2t - s - e) d) / d
    let w = (tx.add(&tx).sub(&sx).sub(&ex), ty.add(&ty).sub(&sy).sub(&ey));
    let side = |x: &Int, y: &Int| cy.neg().mul(x).add(&cx.mul(y));
    let side_t = side(&w.0, &w.1).signum();
    let side_c = side(
        &nx.add(&nx).add(&w.0.mul(&d)),
        &ny.add(&ny).add(&w.1.mul(&d)),
    )
    .signum()
        * d.signum();
    let sweep = if side_t * side_c > 0 {
        core::f64::consts::TAU - minor
    } else {
        minor
    };
    Some((centre, r, sweep))
}

type V = [f64; 3];

/// `x² + y² + z²` as the pin sums it.
fn square3(a: &V) -> f64 {
    a[0] * a[0] + a[1] * a[1] + a[2] * a[2]
}

/// The direction of `a` (finite, not zero), computed with no overflow: `a` over its largest
/// component's size, then over that one's length. Within a few units of the last place of the
/// true unit vector.
fn direction<const N: usize>(a: [f64; N]) -> [f64; N] {
    let s = a.iter().fold(0.0_f64, |m, c| m.max(c.abs()));
    let w = a.map(|c| c / s);
    let n = w.iter().map(|c| c * c).sum::<f64>().sqrt();
    w.map(|c| c / n)
}

/// Whether `u` is the direction of `a` to a few units of the last place.
fn along<const N: usize>(u: [f64; N], a: [f64; N]) -> Result<(), String> {
    let want = direction(a);
    if u.iter()
        .zip(want)
        .all(|(x, w)| (x - w).abs() <= 8.0 * f64::EPSILON)
    {
        Ok(())
    } else {
        Err(format!("{u:?} is not the direction {want:?}"))
    }
}

/// `Vec3::normalized` (the pin's `unitize`) of a vector whose squared length is not finite.
///
/// The pin divides by the root of the squared length: for a component not finite that is NaN
/// and it answers `Ok` of a NaN vector; for finite components whose square overflows it divides
/// by infinity and answers `Ok` of a zero vector (or NaN), neither a unit vector. scx answers
/// `None` for a component not finite (no direction), and scales an overflowing vector by its
/// largest component first, so a finite vector has its direction. The check: `None` exactly
/// for the vectors with a component not finite, otherwise the true direction.
pub fn v3_normalized() -> PinDefect<(V,), Option<scx_num::vector3d::UnitVec3>> {
    PinDefect {
        reason: "the pin's unit of an overflowing or non-finite vector is not a unit vector",
        applies: Applies::Inputs(|(a,)| !square3(a).is_finite()),
        check: |(a,), r| match r {
            None if a.iter().any(|c| !c.is_finite()) => Ok(()),
            None => Err("a finite vector has a direction".to_owned()),
            Some(u) if a.iter().all(|c| c.is_finite()) => along(u.to_array(), *a),
            Some(u) => Err(format!("a vector not finite has no direction, not {u:?}")),
        },
    }
}

/// `Vec2::normalized` (the pin's `unitize`): as [`v3_normalized`], in the plane.
pub fn v2_normalized() -> PinDefect<([f64; 2],), Option<scx_num::vector2d::UnitVec2>> {
    PinDefect {
        reason: "the pin's unit of an overflowing or non-finite vector is not a unit vector",
        applies: Applies::Inputs(|(a,)| !(a[0] * a[0] + a[1] * a[1]).is_finite()),
        check: |(a,), r| match r {
            None if a.iter().any(|c| !c.is_finite()) => Ok(()),
            None => Err("a finite vector has a direction".to_owned()),
            Some(u) if a.iter().all(|c| c.is_finite()) => along(u.to_array(), *a),
            Some(u) => Err(format!("a vector not finite has no direction, not {u:?}")),
        },
    }
}

/// `make_unit_ortho_vectors(a, y_ref)`.
///
/// The pin unitizes as [`v3_normalized`] describes: an overflowing `a` becomes a zero vector,
/// so no axis crosses it and the pin fails (`NoPerpendicular`); a NaN `a` gives a NaN frame;
/// it crosses `a` with `y_ref` as given, so a `y_ref` too long overflows to a `z` whose unit
/// is zero or NaN, which the pin answers, and one too short passes for parallel. scx: an
/// error exactly when `a` has no direction; otherwise `x` the direction of `a`, `y` in the
/// plane of `a` and `y_ref` on its side when `y_ref` has a direction off `a`'s line, unit
/// and square axes and `z = x × y`.
pub fn v3_make_unit_ortho_vectors(
) -> PinDefect<(V, Option<V>), scx_num::Result<[scx_num::vector3d::UnitVec3; 3]>> {
    PinDefect {
        reason: "the pin's unit of an overflowing or non-finite vector is not a unit vector",
        applies: Applies::Disagreeing,
        check: |(a, b), r| {
            if dir3(*a).is_none() {
                return refused(r, "a vector with no direction");
            }
            let [x, y, z] = answer(r)?.map(|u| u.to_array());
            frame_is(*a, *b, x, y)?;
            near3("z", z, cross3(x, y), 8.0 * f64::EPSILON)
        },
    }
}

/// Within one unit of the last place of `w` (or both NaN).
fn ulp_close(v: f64, w: f64) -> bool {
    v == w || (v.is_nan() && w.is_nan()) || (v - w).abs() <= f64::EPSILON * w.abs()
}

/// `Extent2d::mid` of a box whose corner sum overflows the pin's guarded divide.
///
/// The pin forms `(min + max) / 2` with a divide that returns the plain sum when it would
/// overflow, so the mid of `[1e20, 1e20]` is `2e20`, outside the box. scx takes the midpoint
/// of each interval. The check: each coordinate is `min/2 + max/2` to an ulp.
pub fn e2_mid() -> PinDefect<([f64; 4],), Option<scx_num::vector2d::Point2>> {
    PinDefect {
        reason: "the pin's guarded divide answers the corner sum, not the mid, for large boxes",
        applies: Applies::Disagreeing,
        check: |(a,), p| {
            let Some(p) = p else {
                return Err(format!("no mid for the box {a:?}"));
            };
            let (x, y) = (a[0] * 0.5 + a[2] * 0.5, a[1] * 0.5 + a[3] * 0.5);
            if ulp_close(p.x, x) && ulp_close(p.y, y) {
                Ok(())
            } else {
                Err(format!("mid {p:?} is not ({x}, {y})"))
            }
        },
    }
}

// The truths below cover inputs on which scx and the pin disagree past the tolerance
// (`Applies::Disagreeing`): nearly all are vectors of `1e300` or `f64::MAX` scale, whose
// squares or cross products overflow. The pin unitizes them to zero or NaN vectors (see
// [`v3_normalized`]) and answers a frame, map or flag built on those; scx scales first and
// answers for the true direction. Each check recomputes the answer from its definition with
// the overflow-free [`direction`], and holds scx to a few units of the last place of it,
// tighter than the harness's agreement tolerance, so on these inputs scx is also nearer the
// truth than the pin.

type Map = scx_num::math::Affine;
const BASIS: [V; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
/// The length below which a vector has no direction (`EFF_ZERO`), and around it the band
/// where scx's rounded test and this one may fall either way.
const NO_LENGTH: f64 = 1e-12;

fn finite(a: &[f64]) -> bool {
    a.iter().all(|c| c.is_finite())
}
fn biggest(a: &[f64]) -> f64 {
    a.iter().fold(0.0_f64, |m, c| m.max(c.abs()))
}
/// The length of `a`, with no overflow.
fn len3(a: V) -> f64 {
    let s = biggest(&a);
    if s == 0.0 {
        0.0
    } else {
        s * a.map(|c| c / s).iter().map(|c| c * c).sum::<f64>().sqrt()
    }
}
/// The direction of `a`, if it has one: finite and at least [`NO_LENGTH`] long.
fn dir3(a: V) -> Option<V> {
    (finite(&a) && len3(a) >= NO_LENGTH).then(|| direction(a))
}
fn dot3(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
/// `p · u` for a unit `u`, at an eighth of `p` when a partial sum overflows.
fn far_dot3(p: V, u: V) -> f64 {
    let d = dot3(p, u);
    if d.is_finite() {
        d
    } else {
        dot3(p.map(|c| c * 0.125), u) * 8.0
    }
}
fn cross3(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn add3(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub3(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale3(a: V, s: f64) -> V {
    a.map(|c| c * s)
}
fn vec(a: V) -> scx_num::vector3d::Vec3 {
    scx_num::vector3d::Vec3::from_array(a)
}
/// `Err` unless `u` is `w` to `tol` in every component.
fn near3(what: &str, u: V, w: V, tol: f64) -> Result<(), String> {
    if (0..3).all(|i| (u[i] - w[i]).abs() <= tol) {
        Ok(())
    } else {
        Err(format!("{what} {u:?} is not {w:?} (to {tol:e})"))
    }
}
/// `Ok` when scx refused.
fn refused<R: std::fmt::Debug, E>(r: &Result<R, E>, why: &str) -> Result<(), String> {
    match r {
        Err(_) => Ok(()),
        Ok(v) => Err(format!("{why}: no answer, not {v:?}")),
    }
}
fn answer<R, E: std::fmt::Debug>(r: &Result<R, E>) -> Result<&R, String> {
    r.as_ref()
        .map_err(|e| format!("a true answer exists, not {e:?}"))
}
/// The map's linear part on the unit axes and its translation, against `lin` and `shift`.
fn map_is(t: &Map, lin: impl Fn(V) -> V, lin_tol: f64, shift: V, tol: f64) -> Result<(), String> {
    for e in BASIS {
        near3(
            "the map of an axis",
            t.apply_vec(vec(e)).to_array(),
            lin(e),
            lin_tol,
        )?;
    }
    let o = scx_num::vector3d::Point3::ORIGIN;
    near3("the translation", t.apply(o).to_array(), shift, tol)
}

/// `Affine::reflection(origin, normal)`: `p' = p - 2 ((p - origin)·n) n`.
pub fn tr_reflection() -> PinDefect<(V, V), scx_num::Result<Map>> {
    PinDefect {
        reason: "the pin's unit of an overflowing normal is zero, so its mirror is the identity",
        applies: Applies::Disagreeing,
        check: |(p, n), r| {
            let Some(d) = dir3(*n).filter(|_| finite(p)) else {
                return refused(r, "a mirror with no normal");
            };
            let shift = scale3(scale3(d, far_dot3(*p, d)), 2.0);
            if !finite(&shift) {
                return refused(r, "a mirror too far out to represent");
            }
            // Within rounding of `f64::MAX` scx's unit may overflow where this one does not.
            if r.is_err() && biggest(&shift) > (1.0 - 1e-14) * f64::MAX {
                return Ok(());
            }
            let lin = |e: V| sub3(e, scale3(d, 2.0 * dot3(e, d)));
            map_is(
                answer(r)?,
                lin,
                8.0 * f64::EPSILON,
                shift,
                1e-14 * biggest(p),
            )
        },
    }
}

/// `Affine::rotation(point, axis, angle)`: Rodrigues' formula in vector form,
/// `R v = c v + s (k × v) + (1 - c)(k·v) k`, with `std`'s sine and cosine, about `point`.
pub fn tr_rotation() -> PinDefect<(V, V, f64), scx_num::Result<Map>> {
    PinDefect {
        reason: "the pin's unit of an overflowing axis is zero, so its turn is a scaling",
        applies: Applies::Disagreeing,
        check: |(p, a, ang), r| {
            let Some(k) = dir3(*a).filter(|_| finite(p) && ang.is_finite()) else {
                return refused(r, "a turn with no axis");
            };
            let (s, c) = scx_num::fmath::sin_cos(*ang);
            let rot = |v: V| {
                let along = scale3(k, (1.0 - c) * dot3(k, v));
                add3(add3(scale3(v, c), scale3(cross3(k, v), s)), along)
            };
            let near = sub3(*p, rot(*p));
            let shift = if finite(&near) {
                near
            } else {
                let q = scale3(*p, 0.125);
                scale3(sub3(q, rot(q)), 8.0)
            };
            if !finite(&shift) {
                return refused(r, "a turn too far out to represent");
            }
            map_is(
                answer(r)?,
                rot,
                16.0 * f64::EPSILON,
                shift,
                1e-14 * biggest(p),
            )
        },
    }
}

/// `Affine::into_plane_frame(origin, x, y)`: the unit axes `x̂`, `n = unit(x̂ × ŷ)` and
/// `n × x̂` go to X, Z and Y, and `origin` to the origin.
pub fn tr_into_plane_frame() -> PinDefect<(V, V, V), scx_num::Result<Map>> {
    PinDefect {
        reason: "the pin's unit of an overflowing axis is zero, so it finds no plane",
        applies: Applies::Disagreeing,
        check: |(p, x, y), r| {
            let (Some(xh), Some(yh)) = (dir3(*x), dir3(*y)) else {
                return refused(r, "an axis with no direction");
            };
            if !finite(p) {
                return refused(r, "an origin not finite");
            }
            let c = cross3(xh, yh);
            let lc = len3(c);
            if lc < 0.5 * NO_LENGTH {
                return refused(r, "axes along one line");
            } else if lc < 2.0 * NO_LENGTH {
                return Ok(());
            }
            let n = direction(c);
            let yv = cross3(n, xh);
            let shift = [-far_dot3(*p, xh), -far_dot3(*p, yv), -far_dot3(*p, n)];
            if !finite(&shift) {
                return refused(r, "a plane too far out to represent");
            }
            let tol = 16.0 * f64::EPSILON / lc;
            let t = answer(r)?;
            for (u, e) in [(xh, BASIS[0]), (yv, BASIS[1]), (n, BASIS[2])] {
                near3(
                    "the map of a frame axis",
                    t.apply_vec(vec(u)).to_array(),
                    e,
                    tol,
                )?;
            }
            let o = scx_num::vector3d::Point3::ORIGIN;
            let tt = (tol + 1e-14) * biggest(p);
            near3("the translation", t.apply(o).to_array(), shift, tt)
        },
    }
}

/// A frame `x`, `y` for `a` and its `y` reference `b`: `x` the direction of `a`, `y` unit and
/// square to it, and in the plane of `a` and `b` on `b`'s side when `b` has a direction not
/// along `a`.
fn frame_is(a: V, b: Option<V>, x: V, y: V) -> Result<(), String> {
    along(x, a)?;
    let tight = 8.0 * f64::EPSILON;
    if (len3(y) - 1.0).abs() > tight || dot3(x, y).abs() > tight {
        return Err(format!("{x:?} {y:?} are not unit and square"));
    }
    let Some(b) = b.and_then(dir3) else {
        return Ok(());
    };
    let xt = direction(a);
    let lc = len3(cross3(xt, b));
    if lc < 2.0 * NO_LENGTH {
        return Ok(());
    }
    let want = direction(sub3(b, scale3(xt, dot3(b, xt))));
    near3("y", y, want, 16.0 * f64::EPSILON / lc)
}

/// `math::orthonormal_frame(x, y)`, `make_unit_ortho_vectors`'s first two axes.
pub fn tr_orthonormal_frame() -> PinDefect<(V, V), scx_num::Result<[scx_num::vector3d::Vec3; 2]>> {
    PinDefect {
        reason: "the pin's unit of an overflowing vector is zero, so it finds no perpendicular",
        applies: Applies::Disagreeing,
        check: |(a, b), r| {
            if dir3(*a).is_none() {
                return refused(r, "a vector with no direction");
            }
            let [x, y] = answer(r)?.map(|v| v.to_array());
            frame_is(*a, Some(*b), x, y)
        },
    }
}

/// `math::axis_squared_frame(x, y, noise)`: each axis with its coordinates below `noise`
/// dropped, made unit; none when an axis has no direction once cleaned or the two are along
/// one line.
pub fn tr_axis_squared_frame() -> PinDefect<(V, V, f64), Option<[scx_num::vector3d::Vec3; 2]>> {
    PinDefect {
        reason: "the pin's unit of an overflowing axis is zero, so it finds no frame",
        applies: Applies::Disagreeing,
        check: |(x, y, noise), r| {
            let clean = |v: V| v.map(|c| if c.abs() < *noise { 0.0 } else { c });
            let ok = finite(x) && finite(y) && noise.is_finite() && *noise >= 0.0;
            let (Some(xc), Some(yc)) = (dir3(clean(*x)), dir3(clean(*y))) else {
                return r.map_or(Ok(()), |f| {
                    Err(format!("an axis has no direction, not {f:?}"))
                });
            };
            let lc = len3(cross3(xc, yc));
            if !ok || lc < 0.5 * NO_LENGTH {
                return r.map_or(Ok(()), |f| Err(format!("no frame, not {f:?}")));
            } else if lc < 2.0 * NO_LENGTH {
                return Ok(());
            }
            let Some([u, v]) = r else {
                return Err("a frame exists".to_owned());
            };
            near3("x", u.to_array(), xc, 8.0 * f64::EPSILON)?;
            near3("y", v.to_array(), yc, 8.0 * f64::EPSILON)
        },
    }
}

/// `axis` as a placed body's own frame sees it, by its definition: the unit axis through the
/// inverse map (`Affine::inverse`, held to the pin by `tr_inverse`), unit. `Ok(None)` when
/// that image's length is in the band about [`NO_LENGTH`] where either answer is right.
fn own_axis(t: &M, axis: V) -> Result<Option<V>, scx_num::math::AxisRefusal> {
    use scx_num::math::AxisRefusal as Ax;
    let t = &crate::facade::affine_part(*t);
    if !(t.iter().all(|r| finite(r)) && finite(&axis)) {
        return Err(Ax::NotFinite);
    }
    let map = Map::try_from(*t).unwrap();
    let inverse = map.inverse().ok_or(Ax::FlatMap)?;
    let a = dir3(axis).ok_or(Ax::ZeroAxis)?;
    let own = inverse.apply_vec(vec(a)).to_array();
    if !finite(&own) {
        return Err(Ax::NotFinite);
    }
    let l = len3(own);
    if (0.5 * NO_LENGTH..2.0 * NO_LENGTH).contains(&l) {
        return Ok(None);
    }
    dir3(own).map(Some).ok_or(Ax::ZeroAxis)
}

/// `axis_along_own_axes(t, axis)`: the own axis has a component of size `1 - 1e-9` or more.
pub fn axis_along_own_axes() -> PinDefect<(M, V), Result<bool, scx_num::math::AxisRefusal>> {
    PinDefect {
        reason: "the pin's unit of an overflowing axis is zero, so it is along no own axis",
        applies: Applies::Disagreeing,
        check: |(t, axis), r| match (own_axis(t, *axis), r) {
            (Err(e), Err(f)) if e == *f => Ok(()),
            (Ok(None), _) => Ok(()),
            (Ok(Some(own)), Ok(b)) => {
                let top = biggest(&own);
                if (top - (1.0 - 1e-9)).abs() < 1e-12 || *b == (top >= 1.0 - 1e-9) {
                    Ok(())
                } else {
                    Err(format!("own axis {own:?}: along an own axis is {}", !b))
                }
            }
            (want, got) => Err(format!("{got:?}, not {want:?}")),
        },
    }
}

/// `box_extent_along(min, max, t, axis)`: `Σ |dᵢ| |maxᵢ - minᵢ|` for the own axis `d`.
pub fn box_extent_along() -> PinDefect<(V, V, M, V), Result<f64, scx_num::math::AxisRefusal>> {
    PinDefect {
        reason: "the pin's unit of an overflowing axis is zero, and its box ends overflow",
        applies: Applies::Disagreeing,
        check: |(min, max, t, axis), r| {
            use scx_num::math::AxisRefusal as Ax;
            let want = own_axis(t, *axis).and_then(|d| {
                let Some(d) = d else { return Ok(None) };
                let size = sub3(*max, *min);
                let e: f64 = (0..3).map(|i| (size[i] * d[i]).abs()).sum();
                if finite(min) && finite(max) && e.is_finite() {
                    Ok(Some(e))
                } else {
                    Err(Ax::NotFinite)
                }
            });
            match (want, r) {
                (Err(e), Err(f)) if e == *f => Ok(()),
                (Ok(None), _) => Ok(()),
                (Ok(Some(e)), Ok(g)) if (e - g).abs() <= 1e-14 * e + f64::MIN_POSITIVE => Ok(()),
                (want, got) => Err(format!("{got:?}, not {want:?}")),
            }
        },
    }
}

/// An answer of the own-axis queries.
type AxisAnswer = Result<Option<f64>, scx_num::math::AxisRefusal>;

/// `plane_offset_square_to(t, origin, normal, axis)`: the placed unit normal against the
/// unit axis, and the placed origin's offset along it.
pub fn plane_offset_square_to() -> PinDefect<(M, V, V, V), AxisAnswer> {
    PinDefect {
        reason: "the pin's unit of an overflowing normal is zero, so it is square to any axis",
        applies: Applies::Disagreeing,
        check: |(t, o, n, axis), r| {
            use scx_num::math::AxisRefusal as Ax;
            let t = &crate::facade::affine_part(*t);
            let want = if !(t.iter().all(|r| finite(r)) && finite(axis)) {
                Err(Ax::NotFinite)
            } else if let Some(a) = dir3(*axis) {
                let map = Map::try_from(*t).unwrap();
                let image = dir3(*n)
                    .filter(|_| finite(o))
                    .map(|n| map.apply_vec(vec(n)).to_array());
                if image.is_some_and(|i| (0.5 * NO_LENGTH..2.0 * NO_LENGTH).contains(&len3(i))) {
                    return Ok(());
                }
                match image.and_then(dir3).map(|p| len3(cross3(p, a))) {
                    None => Ok(None),
                    Some(c) if (c - 1e-9).abs() < 1e-15 => return Ok(()),
                    Some(c) if c > 1e-9 => Ok(None),
                    Some(_) => {
                        let at = scx_num::vector3d::Point3::from_array(*o);
                        Ok(Some((map.apply(at).to_array(), a)))
                    }
                }
            } else {
                Err(Ax::ZeroAxis)
            };
            match (want, r) {
                (Err(e), Err(f)) if e == *f => Ok(()),
                (Ok(None), Ok(None)) => Ok(()),
                (Ok(Some((q, a))), Ok(Some(g))) => {
                    let e = far_dot3(q, a);
                    let tol = 1e-14 * biggest(&q);
                    if (e - g).abs() <= tol || (!e.is_finite() && !g.is_finite()) {
                        Ok(())
                    } else {
                        Err(format!("offset {g}, not {e}"))
                    }
                }
                (want, got) => Err(format!("{got:?}, not {want:?}")),
            }
        },
    }
}

type Quad = (
    scx_num::vector3d::Vec3,
    [scx_num::vector3d::Point3; 4],
    [scx_num::vector3d::Point3; 4],
);

/// `offset_quad(corners, lift)`: the unit normal of the edges' units, the corners moved
/// `lift` along and against it.
pub fn offset_quad() -> PinDefect<(V, V, V, V, f64), Option<Quad>> {
    PinDefect {
        reason: "the pin's unit of an overflowing cross product is NaN, so its normal is too",
        applies: Applies::Disagreeing,
        check: |(a, b, c, d, lift), r| {
            let corners = [*a, *b, *c, *d];
            let none = |why: &str| {
                r.as_ref()
                    .map_or(Ok(()), |q| Err(format!("{why}, not {q:?}")))
            };
            if !(corners.iter().all(|p| finite(p)) && lift.is_finite()) {
                return none("numbers not finite");
            }
            let (Some(du), Some(dv)) = (dir3(sub3(*b, *a)), dir3(sub3(*d, *a))) else {
                return none("an edge with no direction");
            };
            let cr = cross3(du, dv);
            let lc = len3(cr);
            if lc < 0.5 * NO_LENGTH {
                return none("edges along one line");
            } else if lc < 2.0 * NO_LENGTH {
                return Ok(());
            }
            let n = direction(cr);
            let tol = 16.0 * f64::EPSILON / lc;
            let Some((normal, up, down)) = r else {
                return Err("a normal exists".to_owned());
            };
            near3("the normal", normal.to_array(), n, tol)?;
            for (k, p) in corners.iter().enumerate() {
                let off = scale3(n, *lift);
                let tt = tol * lift.abs() + 2.0 * f64::EPSILON * biggest(p);
                near3("a corner moved up", up[k].to_array(), add3(*p, off), tt)?;
                near3("a corner moved down", down[k].to_array(), sub3(*p, off), tt)?;
            }
            Ok(())
        },
    }
}

/// `certified::unit(a)`: `None` when the square of `a` is not finite (its error bound is for
/// the plain quotient) or `a` has no direction, else the direction.
pub fn c_unit() -> PinDefect<(V,), Option<scx_num::vector3d::Vec3>> {
    PinDefect {
        reason: "the pin's unit of an overflowing vector is zero, not a unit vector",
        applies: Applies::Disagreeing,
        check: |(a,), r| match r {
            None if !square3(a).is_finite() || dir3(*a).is_none() => Ok(()),
            None => Err("a vector of finite square has its unit".to_owned()),
            Some(u) if square3(a).is_finite() => along(u.to_array(), *a),
            Some(u) => Err(format!(
                "no certified unit for a square not finite, not {u:?}"
            )),
        },
    }
}

/// `transform::Transform::mirror_simple(point, normal)`: the transform followed by the
/// mirror in the plane (`x - 2 ((x - point)·n) n`), refused (unchanged) when the plane has
/// no normal or the mirrored transform is too large to represent.
pub fn tf_mirror_simple(
) -> PinDefect<(crate::facade::TfR, V, V), (scx_num::Result<()>, scx_num::transform::Transform)> {
    PinDefect {
        reason: "the pin's unit of an overflowing normal fails, so it leaves the transform as is",
        applies: Applies::Disagreeing,
        check: |(tf, p, n), (r, after)| {
            use scx_num::vector3d::{Point3, Vec3};
            let before = scx_num::transform::Transform::new(
                Point3::from_array(tf.0),
                Vec3::from_array(tf.1),
                Vec3::from_array(tf.2),
                tf.3.map(Vec3::from_array),
            );
            let m = *before.matrix();
            let rows: [V; 4] = std::array::from_fn(|i| [m[i][0], m[i][1], m[i][2]]);
            let want = dir3(*n).filter(|_| finite(p)).and_then(|d| {
                // `2 (v·d) d` as `((v·d) d) 2`: a row near `f64::MAX` overflows on `2 (v·d)`.
                let lin = |v: V| sub3(v, scale3(scale3(d, far_dot3(v, d)), 2.0));
                let x = rows[3];
                let point = sub3(x, scale3(scale3(d, far_dot3(sub3(x, *p), d)), 2.0));
                let out = [lin(rows[0]), lin(rows[1]), lin(rows[2]), point];
                out.iter().all(|r| finite(r)).then_some(out)
            });
            let Some(out) = want else {
                return match r {
                    Err(_)
                        if after.matrix().map(|r| r.map(f64::to_bits))
                            == m.map(|r| r.map(f64::to_bits)) =>
                    {
                        Ok(())
                    }
                    Err(_) => Err("a refused mirror changed the transform".to_owned()),
                    Ok(()) => Err(format!("no mirror to represent, not {after:?}")),
                };
            };
            answer(r)?;
            let got = *after.matrix();
            for (i, w) in out.iter().enumerate() {
                let tol = if i == 3 {
                    1e-14 * (biggest(&rows[3]) + biggest(p))
                } else {
                    16.0 * f64::EPSILON * biggest(&rows[i])
                };
                near3("a mirrored row", [got[i][0], got[i][1], got[i][2]], *w, tol)?;
            }
            Ok(())
        },
    }
}

/// Apart: one ends before the other starts (a NaN bound compares as not apart).
fn apart(a: &[f64; 2], b: &[f64; 2]) -> bool {
    a[1] < b[0] || b[1] < a[0]
}
/// Not inverted: `lo > hi` does not hold (a NaN bound compares as not inverted).
fn not_inverted(lo: f64, hi: f64) -> bool {
    lo.partial_cmp(&hi) != Some(std::cmp::Ordering::Greater)
}
type Periodic = Option<scx_num::Result<Vec<scx_num::extent1d::Extent1d>>>;
/// `Extent1d::intersect_periodic` at period 0 of intervals that are apart (`b` wholly above
/// or below `a`): the plain intersection, which has none. The pin's error names its inner
/// `intersect` as the failing routine; scx names the routine called. The check holds scx to
/// a `Disjoint` failure of `intersect_periodic` that carries the two intervals. With a NaN
/// bound, the pin's overlap is NaN or not by operand order; scx's is the overlap of the
/// bounds that are not NaN (as `e1_union_intersect` checks), which the check recomputes.
pub fn e1_intersect_periodic() -> PinDefect<([f64; 2], [f64; 2], f64, bool), Periodic> {
    PinDefect {
        reason: "the pin names its inner intersect as the failing routine, and takes the \
                 overlap with a NaN bound by operand order (scx skips the NaN)",
        applies: Applies::Inputs(|(a, b, p, _)| {
            let ordered = |e: &[f64; 2]| not_inverted(e[0], e[1]);
            let nan = a.iter().chain(b).any(|x| x.is_nan());
            *p == 0.0 && ordered(a) && ordered(b) && (nan || apart(a, b))
        }),
        check: |(a, b, ..), r| match r {
            Some(Err(scx_num::NumError::Failed {
                operation: "geom::extent1d::Extent1d::intersect_periodic",
                failure: scx_num::error::NumFailure::Disjoint { a: x, b: y },
                ..
            })) if apart(a, b)
                && x.map(f64::to_bits) == a.map(f64::to_bits)
                && y.map(f64::to_bits) == b.map(f64::to_bits) =>
            {
                Ok(())
            }
            // With a NaN bound: the overlap of the bounds that are not NaN, one interval.
            Some(Ok(v)) if !apart(a, b) && v.len() == 1 => {
                let w = [a[0].max(b[0]), a[1].min(b[1])];
                let got = [v[0].min(), v[0].max()];
                if got.map(f64::to_bits) == w.map(f64::to_bits) {
                    Ok(())
                } else {
                    Err(format!("the overlap is {w:?}, not {got:?}"))
                }
            }
            _ => Err(format!("{a:?} and {b:?}: not {r:?}")),
        },
    }
}

/// A box with a NaN bound has no defined extent along that axis, and a line through a point
/// or along a direction that is not finite is no line, so no clip is defined: scx refuses
/// them (`InvalidInput`). The pin clips on, its slab extremes NaN or not by operand order
/// and its parameters `inf - inf`, and answers a miss, a touch or a crossing.
fn nan_line(a: &[f64; 6], p: &V, d: &V) -> bool {
    let undefined = a.iter().any(|x| x.is_nan()) || !p.iter().chain(d).all(|x| x.is_finite());
    undefined && (0..3).all(|i| not_inverted(a[i], a[i + 3]))
}
fn refused_line<T: std::fmt::Debug>(r: &Option<scx_num::Result<T>>) -> Result<(), String> {
    match r {
        Some(Err(scx_num::NumError::InvalidInput { .. })) => Ok(()),
        _ => Err(format!("an undefined box or line is refused, not {r:?}")),
    }
}
pub fn e3_intersect_line(
) -> PinDefect<([f64; 6], V, V), Option<scx_num::Result<scx_num::extent3d::LineHit>>> {
    PinDefect {
        reason: "the pin clips an undefined line or box",
        applies: Applies::Inputs(|(a, p, d)| nan_line(a, p, d)),
        check: |_, r| refused_line(r),
    }
}
pub fn e3_line_hit_count() -> PinDefect<([f64; 6], V, V), Option<scx_num::Result<u32>>> {
    PinDefect {
        reason: "the pin clips an undefined line or box",
        applies: Applies::Inputs(|(a, p, d)| nan_line(a, p, d)),
        check: |_, r| refused_line(r),
    }
}

// ---- scx-p21 against solidcore-record's Part 21 reader (`t_p21`) ---------------------------
//
// Each predicate reads the DATA section's text (the pools have no strings or comments that
// could hide the construct). The clauses are those of ISO 10303-21:2016.

use crate::t_p21::{
    Text, XRead, EMPTY_COMPLEX, ENUM_DASH, LOWER_HEX, PAGE_LATIN, TYPED_MANY, TYPED_NONE,
};
use scx_p21::{P21Error, Param, Refusal, Syntax};

/// scx's refusal cause, through any `Within` chain.
fn syntax_cause(r: &XRead) -> Option<&Syntax> {
    match r.0.as_ref().err().map(P21Error::innermost) {
        Some(P21Error::Refused(Refusal::Syntax { cause, .. })) => Some(cause),
        _ => None,
    }
}

/// `#1`'s first parameter, as scx read it.
fn first_param(r: &XRead) -> Result<&Param, String> {
    let parsed = r.0.as_ref().map_err(|e| format!("scx refused: {e}"))?;
    parsed
        .exchange
        .get(1)
        .and_then(|e| e.params().first())
        .ok_or_else(|| "scx read no #1 with a parameter".to_owned())
}

/// The number of values in each typed parameter `KEYWORD(...)` inside a record's
/// parameters (a keyword directly before `(` at depth 1 or more).
fn typed_arities(body: &str) -> Vec<usize> {
    let b = body.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0usize;
    for (i, &c) in b.iter().enumerate() {
        match c {
            b'(' => {
                let keyword = i > 0 && b[i - 1].is_ascii_alphabetic();
                if keyword && depth >= 1 {
                    // count the values up to the matching parenthesis
                    let (mut d, mut commas, mut empty) = (0usize, 0usize, true);
                    for &k in &b[i + 1..] {
                        match k {
                            b'(' => d += 1,
                            b')' if d == 0 => break,
                            b')' => d -= 1,
                            b',' if d == 0 => commas += 1,
                            _ => {}
                        }
                        empty = false;
                    }
                    out.push(if empty { 0 } else { commas + 1 });
                }
                depth += 1;
            }
            b')' => depth = depth.saturating_sub(1),
            b';' => depth = 0,
            _ => {}
        }
    }
    out
}

/// `#n=();`: a complex instance with no records.
///
/// §5.5 `SUBSUPER_RECORD = "(" SIMPLE_RECORD_LIST ")"`, `SIMPLE_RECORD_LIST = SIMPLE_RECORD
/// { SIMPLE_RECORD }`: at least one record. The true reading is a refusal; scx refuses with
/// `Syntax::ExpectedKeyword` at the `)`. The pin accepts an instance with no records.
pub fn p21_empty_complex() -> PinDefect<(Text<EMPTY_COMPLEX>,), XRead> {
    PinDefect {
        reason: "the pin reads an empty complex instance",
        applies: Applies::Inputs(|(t,)| t.0.replace(' ', "").contains("=();")),
        check: |_, r| match syntax_cause(r) {
            Some(Syntax::ExpectedKeyword) => Ok(()),
            _ => Err(format!("§5.5 needs a record; scx: {:?}", r.0)),
        },
    }
}

/// `B(1,2)` as a parameter: a typed parameter with more than one value.
///
/// §5.5 `TYPED_PARAMETER = KEYWORD "(" PARAMETER ")"`: exactly one parameter (a list is one).
/// The true reading is a refusal; scx refuses with `Syntax::TypedArity`. The pin accepts
/// and keeps every value.
pub fn p21_typed_many() -> PinDefect<(Text<TYPED_MANY>,), XRead> {
    PinDefect {
        reason: "the pin reads a typed parameter of several values",
        applies: Applies::Inputs(|(t,)| typed_arities(t.0).iter().any(|&n| n > 1)),
        check: |_, r| match syntax_cause(r) {
            Some(Syntax::TypedArity(_)) => Ok(()),
            _ => Err(format!("§5.5 allows one value; scx: {:?}", r.0)),
        },
    }
}

/// `B()` as a parameter: a typed parameter with no value.
///
/// The same clause as [`p21_typed_many`]: one parameter, so none is a syntax error. scx
/// refuses with `Syntax::TypedArity`; the pin accepts a typed value with no values.
pub fn p21_typed_none() -> PinDefect<(Text<TYPED_NONE>,), XRead> {
    PinDefect {
        reason: "the pin reads a typed parameter of no value",
        applies: Applies::Inputs(|(t,)| typed_arities(t.0).contains(&0)),
        check: |_, r| match syntax_cause(r) {
            Some(Syntax::TypedArity(_)) => Ok(()),
            _ => Err(format!("§5.5 needs one value; scx: {:?}", r.0)),
        },
    }
}

/// `.A-B.`: an enumeration value with a `-`.
///
/// §5.4 `ENUMERATION = "." UPPER { UPPER | DIGIT } "."` (with `_` among the upper-case
/// characters, §5.3), and §6.4.5: no `-`. The true reading is a refusal; scx refuses with
/// `Syntax::BadEnumeration`. The pin reads `A-B`.
pub fn p21_enum_dash() -> PinDefect<(Text<ENUM_DASH>,), XRead> {
    PinDefect {
        reason: "the pin reads an enumeration with a dash",
        applies: Applies::Inputs(|(t,)| {
            t.0.split('.').skip(1).step_by(2).any(|tok| {
                tok.contains('-')
                    && tok.bytes().all(|c| {
                        c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_' || c == b'-'
                    })
            })
        }),
        check: |_, r| match syntax_cause(r) {
            Some(Syntax::BadEnumeration) => Ok(()),
            _ => Err(format!("§6.4.5 has no '-'; scx: {:?}", r.0)),
        },
    }
}

/// `'\PA\\S\a'`: a string with a `\PA\` page directive.
///
/// §6.4.3.3: `\PA\` is a directive, not text; it selects ISO 8859-1 for the `\S\`
/// directives that follow, and `\S\c` is the character at `c`'s code plus 0x80 in that
/// part: `\S\a` is 0x61 + 0x80 = 0xE1, `á`. The pin does not read the page directive and
/// keeps the string's text raw (`\PA\\S\a`, and `\PA\abc` for `abc`); scx reads `á` and
/// `abc`. The check decodes the string independently: Latin-1 is
/// the first 256 code points, so the character is `char::from(c + 0x80)`.
pub fn p21_page_latin() -> PinDefect<(Text<PAGE_LATIN>,), XRead> {
    PinDefect {
        reason: "the pin keeps a \\PA\\ string raw",
        applies: Applies::Inputs(|(t,)| t.0.contains(r"\PA\")),
        check: |(t,), r| {
            let raw = t.0.split('\'').nth(1).ok_or("no string in the input")?;
            let mut truth = String::new();
            let mut rest = raw;
            while !rest.is_empty() {
                if let Some(r) = rest.strip_prefix(r"\PA\") {
                    rest = r;
                } else if let Some(r) = rest.strip_prefix(r"\S\") {
                    let c = r.bytes().next().ok_or("\\S\\ at the end")?;
                    truth.push(char::from(c + 0x80));
                    rest = &r[1..];
                } else {
                    let c = rest.chars().next().unwrap();
                    truth.push(c);
                    rest = &rest[c.len_utf8()..];
                }
            }
            match first_param(r)? {
                Param::Str(s) if *s == truth => Ok(()),
                p => Err(format!("§6.4.3.3 gives {truth:?}; scx: {p:?}")),
            }
        },
    }
}

/// `"0f"`: a binary with lower-case hexadecimal digits.
///
/// §5.4 `BINARY = """" ("0" | "1" | "2" | "3") { HEX } """"`, `HEX` being `0`-`9` and
/// `A`-`F` (§5.3, §6.4.6): lower case is outside the grammar. Both readers accept it
/// (leniency kept for real files); scx normalises the digits to upper case, so the value
/// it holds (and writes back) is valid, while the pin keeps `0f`. The check: scx holds the
/// input's digits upper-cased.
pub fn p21_lower_hex() -> PinDefect<(Text<LOWER_HEX>,), XRead> {
    PinDefect {
        reason: "the pin keeps lower-case hex in a binary",
        applies: Applies::Inputs(|(t,)| {
            t.0.split('"')
                .nth(1)
                .is_some_and(|h| h.bytes().any(|c| c.is_ascii_lowercase()))
        }),
        check: |(t,), r| {
            let hex = t.0.split('"').nth(1).ok_or("no binary in the input")?;
            match first_param(r)? {
                Param::Binary(b) if *b == hex.to_ascii_uppercase() => Ok(()),
                p => Err(format!(
                    "the digits upper-cased are {:?}; scx: {p:?}",
                    hex.to_ascii_uppercase()
                )),
            }
        },
    }
}

// ---- scx-nurbs (t_nurbs) ----------------------------------------------------------------

use crate::t_nurbs::{s as pin_nurbs, At, Hom};
use scx_testkit::nurbs as tk;
use scx_testkit::Rat;

/// The two pieces of a split Bezier curve.
type Pieces = Option<[Vec<[f64; 3]>; 2]>;

fn tk_side(above: bool) -> tk::Side {
    if above {
        tk::Side::Above
    } else {
        tk::Side::Below
    }
}

/// The largest coordinate of the control points, at least 1: the scale of every point the
/// curve passes through.
fn hull_scale(a: &At) -> f64 {
    a.curve
        .points
        .iter()
        .flatten()
        .fold(1.0_f64, |m, x| m.max(x.abs()))
}

/// The larger of two differences, a NaN difference counting as infinite.
fn worst(m: f64, d: f64) -> f64 {
    if d.is_nan() {
        f64::INFINITY
    } else {
        m.max(d)
    }
}

/// The largest coordinate difference of two point lists of the same length.
fn max_diff(a: &[[f64; 3]], b: &[[f64; 3]]) -> f64 {
    a.iter()
        .zip(b)
        .flat_map(|(p, q)| p.iter().zip(q).map(|(x, y)| (x - y).abs()))
        .fold(0.0, worst)
}

/// `scx` is within `tol` of `truth`, and no further from it than the pin's `pin` (when the
/// pin answered) by more than `tol / 8`.
fn closest(
    what: &str,
    scx: &[[f64; 3]],
    pin: Option<&[[f64; 3]]>,
    truth: &[[f64; 3]],
    tol: f64,
) -> Result<(), String> {
    if scx.len() != truth.len() {
        return Err(format!(
            "{what}: {} values, the truth has {}",
            scx.len(),
            truth.len()
        ));
    }
    let e = max_diff(scx, truth);
    if e > tol {
        return Err(format!("{what}: {e:e} from the truth (tolerance {tol:e})"));
    }
    if let Some(p) = pin.filter(|p| p.len() == truth.len()) {
        let ep = max_diff(p, truth);
        if e > ep + tol / 8.0 {
            return Err(format!("{what}: {e:e} from the truth, the pin {ep:e}"));
        }
    }
    Ok(())
}

/// `NurbsCurve::point` where scx and the pin disagree: at the ends of the domain.
///
/// The pin's `crv_eval` refuses `u = U[p]` and `u = U[n+1]` (`basis_eval` errs there, and the
/// upstream caller evaluated the end control points itself); scx's span search is exact and
/// the ends evaluate. The check: scx's point is within `4 eps` of the hull's scale of the
/// exact point (`scx_testkit::nurbs::Curve`, rationals), and no further than the pin's.
pub fn nurbs_point() -> PinDefect<(At, bool), Option<[f64; 3]>> {
    PinDefect {
        reason: "the pin's crv_eval refuses the ends of the domain",
        applies: Applies::Disagreeing,
        check: |(a, above), r| {
            let truth = a.curve.exact().map_err(|e| format!("{e:?}"))?;
            let want = truth
                .point(a.u, tk_side(*above))
                .map_err(|e| format!("{e:?}"))?
                .rounded;
            let got = r.ok_or("scx refused")?;
            let pin = pin_nurbs::point(a, *above);
            let tol = 4.0 * f64::EPSILON * hull_scale(a);
            closest(
                "point",
                &[got],
                pin.as_ref().map(std::slice::from_ref),
                &[want],
                tol,
            )
        },
    }
}

/// `NurbsCurve::derivs` (orders 0 to 5) where scx and the pin disagree past `Tol::DEFAULT`:
/// a component far smaller than the others in a high derivative of a rational curve, where
/// both carry the cancellation of the quotient rule and land a few ulps of the largest
/// component apart (2 inputs). The check: each order of scx's is within `1e-12 * 10^k`
/// of the largest component of the exact derivatives, and no further than the pin's.
pub fn nurbs_derivs() -> PinDefect<(At, bool), Option<Vec<[f64; 3]>>> {
    PinDefect {
        reason: "the pin and scx round a cancelling high derivative differently",
        applies: Applies::Disagreeing,
        check: |(a, above), r| {
            let truth = a.curve.exact().map_err(|e| format!("{e:?}"))?;
            let want = truth
                .derivs(a.u, tk_side(*above), 5)
                .map_err(|e| format!("{e:?}"))?;
            let got = r.as_ref().ok_or("scx refused")?;
            let pin = pin_nurbs::derivs(a, *above);
            for (k, w) in want.iter().enumerate() {
                let scale = w.rounded.iter().fold(hull_scale(a), |m, x| m.max(x.abs()));
                let tol = 1e-12 * [1.0, 1e1, 1e2, 1e3, 1e4, 1e5][k] * scale;
                let p = pin
                    .as_ref()
                    .and_then(|p| p.get(k))
                    .map(std::slice::from_ref);
                closest(&format!("order {k}"), &got[k..=k], p, &[w.rounded], tol)?;
            }
            Ok(())
        },
    }
}

/// `NurbsCurve::elevate` where scx and the pin disagree: rational curves with knots a few
/// ulps apart (`Knots::Clustered`). A5.9 removes each inserted knot by extrapolating across
/// the neighbouring spans, multiplying the rounding by their length ratio (about 1e16), and
/// the elevated weights come out zero or negative. The pin returns that curve, 2 to 1e32 from
/// the exact elevation; scx refuses with `NurbsError::Unstable` (its polynomial answers,
/// equally inaccurate, are the pin's bit for bit; `scx-nurbs`' ignored truth test
/// `elevation_of_near_coincident_knots` keeps the gap visible).
///
/// The check: where scx answers, its curve is the exact elevation (knots equal, homogeneous
/// points within `1e-12` of the hull's scale); where it refuses, the pin's answer is more
/// than `1e-6` of the scale from the exact one.
pub fn nurbs_elevate() -> PinDefect<(At, usize), Option<Hom>> {
    PinDefect {
        reason: "the pin's A5.9 returns non-positive weights across near-coincident knots",
        applies: Applies::Disagreeing,
        check: |(a, times), r| {
            let t = times % 4;
            let want = a
                .curve
                .exact()
                .and_then(|c| c.elevate(t))
                .map_err(|e| format!("{e:?}"))?
                .rounded();
            let hom: Vec<[f64; 4]> = match &want.weights {
                None => want
                    .points
                    .iter()
                    .map(|p| [p[0], p[1], p[2], 1.0])
                    .collect(),
                Some(w) => want
                    .points
                    .iter()
                    .zip(w)
                    .map(|(p, &w)| [w * p[0], w * p[1], w * p[2], w])
                    .collect(),
            };
            let scale = hull_scale(a);
            let off = |(k, p): &Hom| {
                if *k != want.knots || p.len() != hom.len() {
                    return f64::INFINITY;
                }
                p.iter()
                    .zip(&hom)
                    .flat_map(|(x, y)| x.iter().zip(y).map(|(a, b)| (a - b).abs()))
                    .fold(0.0, worst)
            };
            match r {
                Some(x) if off(x) <= 1e-12 * scale => Ok(()),
                Some(x) => Err(format!("scx is {:e} from the exact elevation", off(x))),
                None => match pin_nurbs::elevate(a, t) {
                    Some(p) if off(&p) > 1e-6 * scale => Ok(()),
                    Some(p) => Err(format!(
                        "scx refused; the pin is {:e} from the exact elevation",
                        off(&p)
                    )),
                    None => Err("both refused".to_owned()),
                },
            }
        },
    }
}

/// `x^n` by repeated multiplication.
fn ipow(x: f64, n: usize) -> f64 {
    (0..n).fold(1.0, |a, _| a * x)
}

/// `C(n, k)` exactly, in integers.
fn binomial_exact(n: usize, k: usize) -> u128 {
    if k > n {
        return 0;
    }
    (1..=k as u128).fold(1, |c, i| c * (n as u128 - k as u128 + i) / i)
}

/// `bezier::binomial(n, k)` for `n` from 18 (the pin's table has 18 rows) or `k > n`.
///
/// The pin reads an 18-row Pascal triangle and errs outside it, which limits its Bezier
/// elevation to degree 17; scx computes the coefficient exactly in integers (`None` only
/// above `2^53`, past every degree it supports). `C(n, k)` is `0` for `k > n`. The check:
/// scx's value is the exact integer.
pub fn nurbs_binomial() -> PinDefect<(usize, usize), Option<f64>> {
    PinDefect {
        reason: "the pin's 18-row Pascal table",
        applies: Applies::Inputs(|(n, k)| n % 40 >= 18 || k % 40 > n % 40),
        check: |(n, k), r| {
            let want = binomial_exact(n % 40, k % 40);
            match r {
                Some(v) if *v == want as f64 => Ok(()),
                r => Err(format!("C({}, {}) is {want}; scx {r:?}", n % 40, k % 40)),
            }
        },
    }
}

/// `Rat` of a finite double.
fn rat(x: f64) -> Result<Rat, String> {
    Rat::from_f64(x).ok_or_else(|| format!("{x} is not finite"))
}

/// `bezier::bernstein(n, i, u)` for `i > n`, a finite `u` outside `[0, 1]`, or a NaN `u`.
///
/// The pin errs for the first two (`NL_IND_ERR`, `NL_PAR_ERR`); `B[i, n]` is `0` for `i > n`,
/// and the polynomial `C(n, i) u^i (1 - u)^(n - i)` is defined for every `u` (a Bezier curve
/// extended past its ends). For a NaN `u` the pin returns NaN as a value; scx refuses every
/// non-finite `u` with `ParameterNotFinite`, as it does for infinities, where the pin refuses
/// too. The check: for a non-finite `u`, that scx refused; otherwise scx's value against the exact rational
/// value, to `4 n eps (|u| + |1 - u|)^n`, the forward error bound of A1.2's recursion.
pub fn nurbs_bernstein() -> PinDefect<(usize, usize, f64), Option<f64>> {
    PinDefect {
        reason: "the pin refuses i > n and u outside [0, 1], and returns NaN for a NaN u",
        applies: Applies::Inputs(|(n, i, u)| {
            i % 17 > n % 17 || u.is_nan() || (u.is_finite() && !(0.0..=1.0).contains(u))
        }),
        check: |(n, i, u), r| {
            let (n, i) = (n % 17, i % 17);
            if !u.is_finite() {
                return match r {
                    None => Ok(()),
                    Some(v) => Err(format!("scx answered {v} for the parameter {u}")),
                };
            }
            let got = r.ok_or("scx refused")?;
            if i > n {
                return if got == 0.0 {
                    Ok(())
                } else {
                    Err(format!("B[{i}, {n}] is 0; scx {got}"))
                };
            }
            let bound = 4.0 * n as f64 * f64::EPSILON * ipow(u.abs() + (1.0 - u).abs(), n);
            let (ur, vr) = (rat(*u)?, &Rat::one() - &rat(*u)?);
            let mut want = Rat::from(binomial_exact(n, i) as i64);
            for _ in 0..i {
                want = &want * &ur;
            }
            for _ in i..n {
                want = &want * &vr;
            }
            let w = want.to_f64();
            if !bound.is_finite() || (got - w).abs() <= bound.max(f64::MIN_POSITIVE) {
                Ok(())
            } else {
                Err(format!(
                    "B[{i}, {n}]({u}) is {w:e}; scx {got:e}, bound {bound:e}"
                ))
            }
        },
    }
}

/// `bezier::split(points, u)` for `u` outside `[0, 1]`.
///
/// The pin errs (`NL_PAR_ERR`); scx's de Casteljau extends the curve, which is the
/// subdivision of the polynomial at `u` (the left piece runs from `0` to `u`). The check: the
/// pieces against an exact de Casteljau in rationals, to `4 n eps (|u| + |1 - u|)^n` of the
/// points' scale.
pub fn nurbs_bezier_split() -> PinDefect<(At,), Pieces> {
    PinDefect {
        reason: "the pin's Bezier split refuses u outside [0, 1]",
        applies: Applies::Inputs(|(a,)| !(0.0..=1.0).contains(&a.u)),
        check: |(a,), r| {
            let [l, rt] = r.as_ref().ok_or("scx refused")?;
            let n = a.curve.points.len() - 1;
            let (ur, vr) = (rat(a.u)?, &Rat::one() - &rat(a.u)?);
            let mut work: Vec<[Rat; 3]> = a
                .curve
                .points
                .iter()
                .map(|p| p.map(|x| rat(x).unwrap()))
                .collect();
            let mut left = vec![work[0].clone()];
            let mut right = vec![work[n].clone()];
            for k in 1..=n {
                for i in 0..=n - k {
                    let next: [Rat; 3] =
                        std::array::from_fn(|c| &(&vr * &work[i][c]) + &(&ur * &work[i + 1][c]));
                    work[i] = next;
                }
                left.push(work[0].clone());
                right.push(work[n - k].clone());
            }
            right.reverse();
            let round = |v: &[[Rat; 3]]| {
                v.iter()
                    .map(|p| p.clone().map(|x| x.to_f64()))
                    .collect::<Vec<_>>()
            };
            let tol = 4.0
                * n.max(1) as f64
                * f64::EPSILON
                * ipow(a.u.abs() + (1.0 - a.u).abs(), n)
                * hull_scale(a);
            closest("left", l, None, &round(&left), tol)?;
            closest("right", rt, None, &round(&right), tol)
        },
    }
}

/// `global_solve::solve` of `ProjectedMaximize` started with no best answer
/// (`t_gsolve::gsolve_projected_maximize`).
///
/// The pin resets "no best yet" (`SM_BIG_DOUBLE`, 1e20) to 0 for `Maximize` and
/// `DirectedMaximize` only, so a `ProjectedMaximize` starts with a best of 1e20 squared: every
/// solution is "far from the best" and the pin returns none. The true answer is the largest
/// projected distance over the leaf pairs (brute force over every pair); scx starts the
/// maximising operations at 0 and returns it first, with its square as the best.
pub fn gsolve_projected_maximize() -> PinDefect<(u64, u64), crate::t_gsolve::Res> {
    use crate::t_gsolve::{holds_the_best, scenario, Case};
    PinDefect {
        reason: "the pin starts ProjectedMaximize at the 1e20 sentinel",
        applies: Applies::Disagreeing,
        check: |&(a, b), r| holds_the_best(&scenario(a, b, false), Case::ProjectedMaximize, r),
    }
}

/// `global_solve::solve` of `Rayfire` (`t_gsolve::gsolve_rayfire`).
///
/// The pin's `FindBestSubdivisionNode` orders the halves of the node it splits by reading
/// the children of the second tree's node, whatever tree it splits. Splitting the ray tree
/// (the first) once the second is down to a leaf, that is a null child and the pin fails.
/// scx reads the children of the tree it splits. The scenario's ray tree has every leaf at
/// the ray's start, so the true answer is the smallest ray parameter of the pieces the ray
/// passes within the radius of (brute force over every pair), or no solution when it
/// passes none.
pub fn gsolve_rayfire() -> PinDefect<(u64, u64), crate::t_gsolve::Res> {
    use crate::t_gsolve::{holds_the_best, scenario, Case};
    PinDefect {
        reason: "the pin reads tree 1's children for the rayfire order",
        applies: Applies::Disagreeing,
        check: |&(a, b), r| holds_the_best(&scenario(a, b, true), Case::Rayfire, r),
    }
}

/// `Solution::object_for_parameter` on objects of mixed kinds
/// (`t_gsolve::sol_object_for_parameter`).
///
/// The pin's `GetObjectIndexForParameterIndex` reads the kind of object 0 on every pass of
/// its loop (it indexes by `object_index`, set only on the way out, not by the loop's `ii`),
/// so past a first object of another kind it counts the wrong parameters; and when no object
/// holds the parameter (vertices only) it falls out of the loop answering object 0, axis 0.
/// The truth is the object reached by counting each object's own parameters; scx counts them.
pub fn object_for_parameter() -> PinDefect<(usize, usize, usize), Option<(usize, usize)>> {
    use crate::t_gsolve::{object_for_parameter_case, object_for_parameter_truth};
    PinDefect {
        reason: "the pin reads object 0's kind for every object",
        applies: Applies::Disagreeing,
        check: |&a, r| {
            let (counts, index) = object_for_parameter_case(a);
            let want = object_for_parameter_truth(&counts, index);
            if *r == want {
                Ok(())
            } else {
                Err(format!("{r:?}, the truth is {want:?}"))
            }
        },
    }
}

// ---- scx-algo's pseudo box against solidcore's `SmPseudoBox` (`t_pbox`) --------------------

use crate::t_pbox::{BoxR, ProjR};
type XV3 = scx_num::vector3d::Vec3;
type XIv = scx_num::extent1d::Extent1d;
type BoxParts = ([XV3; 3], [XIv; 3]);
type BasisR = Option<([XV3; 3], bool)>;
type LineR = Option<scx_num::Result<(&'static str, Vec<f64>)>>;
type HullR = Option<(Option<BoxParts>, BoxParts)>;

/// `BoxBasis::new` (the pin's `SetBasis` then `InvertAbsolute`'s determinant test) of
/// vectors with a component not finite.
///
/// Such vectors span no box: a NaN component has no value, and an infinite one has no
/// direction once unitised (`inf / inf`). The pin's `SetBasis` compares the determinant
/// with `<`, which NaN passes, and its `InvertAbsolute` divides on, so it keeps a basis of
/// NaN vectors. scx refuses the basis (`None`). The check: `None` for every such input.
pub fn pb_basis() -> PinDefect<(V, V, V), BasisR> {
    PinDefect {
        reason: "the pin keeps a basis of vectors that are not finite",
        applies: Applies::Inputs(|(a, b, c)| !(finite(a) && finite(b) && finite(c))),
        check: |_, r| match r {
            None => Ok(()),
            Some(b) => Err(format!("vectors not finite span no box, not {b:?}")),
        },
    }
}

/// `PseudoBox::intersect_line` of a line through a point, or along a direction, that is not
/// finite: no line, so no clip is defined. scx refuses it (`InvalidInput`); the pin clips
/// its NaN parameters on and answers a miss or a hit (as [`e3_intersect_line`] for boxes
/// on the axes). The check: refused, or no box at all.
pub fn pb_intersect_line() -> PinDefect<(BoxR, V, V), LineR> {
    PinDefect {
        reason: "the pin clips an undefined line",
        applies: Applies::Inputs(|(_, p, v)| !(finite(p) && finite(v))),
        check: |(b, _, _), r| match r {
            None if crate::t_pbox::xbox(b).is_none() => Ok(()),
            _ => refused_line(r),
        },
    }
}

/// The corners of `r` on its unit basis vectors.
fn pb_corners(r: &BoxR) -> Vec<V> {
    let u = r.b.map(direction);
    let mut out = Vec::with_capacity(8);
    for k in 0..8 {
        let mut c = [0.0; 3];
        for (i, ui) in u.iter().enumerate() {
            c = add3(c, scale3(*ui, r.i[i][(k >> i) & 1]));
        }
        out.push(c);
    }
    out
}

/// Whether the perspective image of `r` from `p.e` onto the plane through `p.p` along `p.n`
/// is undefined, with slack around each boundary: the eye on the view plane (every ray
/// meets the plane at the eye or nowhere), the box reaching the plane through the eye
/// parallel to the view plane (its image is unbounded), or a basis vector at the plane's
/// point seen along a ray parallel to the plane (its image is at infinity).
fn pb_perspective_undefined(r: &BoxR, p: &ProjR) -> bool {
    const SLACK: f64 = 1e-9;
    let n = direction(p.n);
    let near = |d: V| far_dot3(d, n).abs() <= SLACK * (1.0 + len3(d));
    if near(sub3(p.p, p.e)) {
        return true;
    }
    let sides: Vec<f64> = pb_corners(r)
        .into_iter()
        .map(|c| {
            let d = sub3(c, p.e);
            let s = far_dot3(d, n);
            if s.abs() <= SLACK * (1.0 + len3(d)) {
                0.0
            } else {
                s.signum()
            }
        })
        .collect();
    let one_side = sides.iter().all(|&s| s > 0.0) || sides.iter().all(|&s| s < 0.0);
    !one_side
        || r.b
            .iter()
            .any(|b| near(add3(sub3(p.p, p.e), direction(*b))))
}

/// `PseudoBox::project_to_plane` in perspective where the image is undefined
/// ([`pb_perspective_undefined`]).
///
/// The pin divides by the ray's distance along the normal and answers a box of NaN or of
/// numbers past any scale (the true image is unbounded); for a box across the eye's plane it
/// answers a bounded box, though the image of the corners behind the eye folds through
/// infinity and no bounded box holds it. scx refuses. The check: scx refused (`Some(None)`),
/// and the projection is perspective and undefined for this box.
pub fn pb_project() -> PinDefect<(BoxR, ProjR), Option<Option<BoxParts>>> {
    PinDefect {
        reason: "the pin answers a box for an unbounded perspective image",
        applies: Applies::Disagreeing,
        check: |(b, p), r| {
            if p.kind != 1 || !pb_perspective_undefined(b, p) {
                return Err(format!("a defined projection has an image, not {r:?}"));
            }
            match r {
                Some(None) => Ok(()),
                _ => Err(format!("an unbounded image is refused, not {r:?}")),
            }
        },
    }
}

/// `PseudoBox::are_project_disjoint` where either box's perspective image is undefined
/// ([`pb_perspective_undefined`]): nothing shows the images apart, so scx answers `false`
/// (not proven disjoint); the pin compares its boxes of NaN or of a folded image
/// ([`pb_project`]) and finds them apart. The check: `false`, and an undefined image.
pub fn pb_project_disjoint() -> PinDefect<(BoxR, BoxR, ProjR), Option<bool>> {
    PinDefect {
        reason: "the pin separates unbounded perspective images",
        applies: Applies::Disagreeing,
        check: |(a, b, p), r| {
            if p.kind != 1 || !(pb_perspective_undefined(a, p) || pb_perspective_undefined(b, p)) {
                return Err(format!("both images are defined: {r:?}"));
            }
            match r {
                Some(false) => Ok(()),
                _ => Err(format!(
                    "undefined images are not proven disjoint, not {r:?}"
                )),
            }
        },
    }
}

/// `true` coordinates of `q` in the basis `u` (Cramer's rule through cross then dot, a
/// different rounding from scx's six-term determinant) and the tolerance on each: a few
/// thousand units of the last place of `|q| / |det|`.
fn pb_true_coords(u: [V; 3], q: V) -> ([f64; 3], f64) {
    let det = dot3(u[0], cross3(u[1], u[2]));
    let c = [
        far_dot3(q, cross3(u[1], u[2])) / det,
        far_dot3(q, cross3(u[2], u[0])) / det,
        far_dot3(q, cross3(u[0], u[1])) / det,
    ];
    (c, 1e-12 * (1.0 + len3(q) / det.abs()))
}

/// `PseudoBox::bounding` (the pin's `Init` then `AddPoint3d`) of points far from the origin.
///
/// The pin's hull starts at `±1e20` (`HULL_START`, as `e1_hull`), so a coordinate past it on
/// one side is lost and the box is short of its points; scx starts empty. The check: the
/// basis is the unit vectors of the input, each interval is the hull of the points' true
/// coordinates (NaN ones, from overflow, skipped) to the tolerance of [`pb_true_coords`],
/// and it is `None` exactly when an axis has no coordinate. A point not finite has no true
/// coordinates (the pin's and scx's are what `inf · 0` and `inf - inf` round to): with one
/// in the list, the intervals need only hold the finite points' hull. The unbounded box is
/// `±SM_INFINITE_PARAMETER` on that basis.
pub fn pb_bounding() -> PinDefect<(BoxR, List<V>), HullR> {
    PinDefect {
        reason: "the pin's hull starts at 1e20 and loses coordinates past it",
        applies: Applies::Disagreeing,
        check: |(b, pts), r| {
            let Some((got, (ub, ui))) = r else {
                return Err("a basis of independent vectors has a box".to_owned());
            };
            for i in 0..3 {
                along(ub[i].to_array(), b.b[i])?;
                if *ui.get(i).ok_or("three intervals")? != XIv::UNBOUNDED {
                    return Err(format!("unbounded box {ui:?}"));
                }
            }
            let u = ub.map(|v| v.to_array());
            let mut hull = [[f64::INFINITY, f64::NEG_INFINITY]; 3];
            let mut tol = [0.0_f64; 3];
            let all_finite = pts.0.iter().all(|q| finite(q));
            for &q in pts.0.iter().filter(|q| finite(*q)) {
                let (c, t) = pb_true_coords(u, q);
                for i in 0..3 {
                    if !c[i].is_nan() {
                        hull[i] = [hull[i][0].min(c[i]), hull[i][1].max(c[i])];
                        tol[i] = tol[i].max(t);
                    }
                }
            }
            let want: Option<Vec<XIv>> =
                hull.iter().map(|h| XIv::from_bounds(h[0], h[1])).collect();
            match (got, want) {
                (None, None) => Ok(()),
                (Some(_), None) if !all_finite => Ok(()),
                (Some((gb, gi)), Some(w)) => {
                    if gb != ub {
                        return Err(format!("basis {gb:?} is not {ub:?}"));
                    }
                    for i in 0..3 {
                        let (g, w) = (gi[i], w[i]);
                        let close = |a: f64, b: f64| a == b || (a - b).abs() <= tol[i];
                        let holds = all_finite
                            || (g.min() <= w.min() + tol[i] && g.max() >= w.max() - tol[i]);
                        let equal = close(g.min(), w.min()) && close(g.max(), w.max());
                        if !(holds && (equal || !all_finite)) {
                            return Err(format!(
                                "interval {i} {g:?} is not {w:?} (to {:e})",
                                tol[i]
                            ));
                        }
                    }
                    Ok(())
                }
                (g, w) => Err(format!("box {g:?}, but the points' hull is {w:?}")),
            }
        },
    }
}

/// `PseudoBox::expanded` (the pin's `ExpandAbsolute`) by NaN, then `union` with the
/// expanded box: a move by no number moves nothing (`NAN_MOVE`, as `e1_expanded`). The pin
/// moves every bound by NaN and its union of the NaN corners is NaN; scx leaves the box as
/// it was. The check: the expanded box is the box, and the union is the box's own hull in
/// the second basis.
pub fn pb_expand_union() -> PinDefect<(BoxR, BoxR, f64), HullR> {
    PinDefect {
        reason: "the pin moves a box by NaN",
        applies: Applies::Inputs(|(_, _, d)| d.is_nan()),
        check: |(a, q, _), r| {
            use crate::t_pbox::{xbasis, xbox, xparts};
            let want = xbox(a)
                .zip(xbasis(&q.b))
                .map(|(a, basis)| (a.union(&a, basis).map(|b| xparts(&b)), xparts(&a)));
            if *r == want {
                Ok(())
            } else {
                Err(format!("a move by NaN leaves the box: {r:?}, not {want:?}"))
            }
        },
    }
}

// ---- scx-algo's geom and sort against solidcore's geom_utility (t_gutil) -------------------
//
// Most of these share one cause: the pin keeps upstream's arithmetic, so where a square
// overflows (components past `1e154`), a direction has no length (below `SM_EFF_ZERO`), or
// an input is not finite, it divides on and answers NaN, an infinity, or a silent 0. scx
// answers `None` there, measures an overflowing direction scaled, and refuses an answer
// that is not finite. Each check computes the truth from the inputs, with every length
// taken scaled ([`len3`], [`dir3`], [`far_dot3`]) and every difference of points taken at
// half size so it cannot overflow. scx must agree with it to `GU_REL` of the size its
// rounding is bounded by; it may refuse only where the question has no answer (no
// direction, input not finite, the answer past `f64`'s range), where its own products pass
// `f64`'s range, or in the band of rounding around its own thresholds.

use crate::harness::List;
use crate::t_gutil::{FitOut, Pts};
type XP3 = scx_num::vector3d::Point3;
type V4 = (V, V, V, V);
type Two = Option<(f64, f64)>;
type SegR = ((f64, f64), (f64, f64));
type V5 = (V, V, V, V, V);
type Three = Option<(f64, f64, f64)>;
type V6 = (V, V, V, V, V, V);

/// Agreement with the truth, relative to the size that bounds the rounding.
const GU_REL: f64 = 1e-12;
/// `f64::EPSILON / 2`.
const GU_U: f64 = 1.1102230246251565e-16;

/// Eight steps of `2^-1074`: an answer below `f64::MIN_POSITIVE` is rounded absolutely.
const GU_SUBNORMAL: f64 = 8.0 * 5e-324;

fn gu_close(what: &str, x: f64, t: f64, scale: f64) -> Result<(), String> {
    let tol = GU_REL * (t.abs() + scale.abs()) + GU_SUBNORMAL;
    if (x - t).abs() <= tol {
        Ok(())
    } else {
        Err(format!("{what} {x:e} is not the true {t:e} (to {tol:e})"))
    }
}
fn half(a: V) -> V {
    scale3(a, 0.5)
}
/// `(q - p) / 2`, which cannot overflow.
fn half_gap(q: V, p: V) -> V {
    sub3(half(q), half(p))
}
/// The unit normal the test gives both sides, as an array.
fn gu_unit(n: V) -> Option<V> {
    crate::t_gutil::unit(n).map(|u| [u.x, u.y, u.z])
}
/// `x / |v|`, with `|v|` past `f64`'s range taken scaled.
fn over_len(x: f64, v: V) -> f64 {
    let s = biggest(&v);
    x / s / len3(v.map(|c| c / s))
}
/// A length is in the band where `|v|² < SM_EFF_ZERO_SQ` may round either way.
fn near_no_length(len: f64) -> bool {
    (len - NO_LENGTH).abs() <= 1e-9 * NO_LENGTH
}
/// Whether the products scx forms of these points and directions may pass `f64`'s range.
fn products_overflow(points: &[V], dirs: &[V]) -> bool {
    let mut m = points.iter().map(|p| biggest(p)).sum::<f64>() + 1.0;
    for d in dirs {
        m *= 1.0 + biggest(d);
    }
    !(m * 4.0).is_finite()
}
/// `x` against what the truth allows: a refusal only where `may_refuse`, an answer only
/// where the truth has one.
fn judge<X: std::fmt::Debug, T>(
    x: &Option<X>,
    may_refuse: bool,
    truth: Option<T>,
    agree: impl Fn(&X, &T) -> Result<(), String>,
) -> Result<(), String> {
    match (x, truth) {
        (None, _) if may_refuse => Ok(()),
        (None, Some(_)) => Err("a true answer exists".into()),
        (None, None) => Ok(()),
        (Some(x), Some(t)) => agree(x, &t),
        (Some(x), None) => Err(format!("no true answer, not {x:?}")),
    }
}

/// The true line parameter of `q` on the line `(p, v)` and the size its rounding is bounded
/// by (`|q - p| / |v|`); `None` for no direction or no finite answer.
fn gu_line_param(p: V, v: V, q: V) -> Option<(f64, f64)> {
    if !(finite(&p) && finite(&v) && finite(&q)) {
        return None;
    }
    let u = dir3(v)?;
    let dh = half_gap(q, p);
    let t = over_len(2.0 * far_dot3(dh, u), v);
    t.is_finite().then_some((t, over_len(2.0 * len3(dh), v)))
}
/// What scx may refuse for the line `(p, v)` and point `q`, past the true refusals.
fn gu_line_may_refuse(p: V, v: V, q: V) -> bool {
    !(finite(&p) && finite(&v) && finite(&q))
        || len3(v) < NO_LENGTH
        || near_no_length(len3(v))
        || !finite(&sub3(q, p))
        || products_overflow(&[p, q], &[v])
        || gu_line_param(p, v, q).is_none()
}

/// `line_closest_point` where `v` has no direction, overflows when squared, or the
/// parameter is past `f64`'s range.
///
/// For `|v|² < SM_EFF_ZERO_SQ` the pin answers 0 (upstream's "line beginning"), and NaN
/// for input not finite; for `|v|²` past `f64`'s range it divides by infinity and answers
/// 0 or NaN where the true parameter is a small number (`[2, 1e300, 0]` gives `-1.9e-300`).
/// scx refuses the first and measures the second scaled. The check: the truth
/// `2 ((q - p)/2 · v̂) / |v|`, or a refusal where [`gu_line_may_refuse`].
pub fn gu_line_closest_point() -> PinDefect<(V, V, V), Option<f64>> {
    PinDefect {
        reason: "the pin answers 0 or NaN for a direction with no length or an overflowing one",
        applies: Applies::Disagreeing,
        check: |&(p, v, q), r| {
            judge(
                r,
                gu_line_may_refuse(p, v, q),
                gu_line_param(p, v, q),
                |x, &(t, s)| gu_close("the parameter", *x, t, s),
            )
        },
    }
}

/// `line_point_distance` on the inputs of [`gu_line_closest_point`], and distances whose
/// squares overflow (the pin answers infinity for points `1e300` apart). The check: the
/// truth parameter and the distance `2 |(q - p)/2 - ((q - p)/2 · v̂) v̂|`, the distance to
/// the rounding of the points' coordinates.
pub fn gu_line_point_distance() -> PinDefect<(V, V, V), Option<(f64, f64)>> {
    PinDefect {
        reason: "the pin answers 0, NaN or infinity for a direction with no length or an \
                 overflowing one",
        applies: Applies::Disagreeing,
        check: |&(p, v, q), r| {
            let truth = gu_line_param(p, v, q).and_then(|(t, s)| {
                let u = dir3(v)?;
                let dh = half_gap(q, p);
                let dist = 2.0 * len3(sub3(dh, scale3(u, far_dot3(dh, u))));
                dist.is_finite().then_some((t, s, dist))
            });
            judge(
                r,
                gu_line_may_refuse(p, v, q),
                truth,
                |&(xd, xt), &(t, s, d)| {
                    gu_close("the parameter", xt, t, s)?;
                    // the foot is `p + v t` for an `f64` `t`, rounded at `2^-1074` when subnormal
                    let foot = 8.0 * (biggest(&p) + biggest(&q)) + len3(v) * 5e-324 / GU_REL;
                    gu_close("the distance", xd, d, foot)
                },
            )
        },
    }
}

/// `point_project_to_plane` with a normal of no length, an overflowing one, or a projection
/// past `f64`'s range.
///
/// The pin divides by `|n|²`: NaN or the point unmoved for a normal of no length, the
/// point unmoved or NaN for one whose square overflows (`[2e-308, 1e300, 0]`, along `y`,
/// leaves `y` at `1e-5` where the plane is at `y = 1`). The check: the truth
/// `q - 2 ((q - p)/2 · n̂) n̂`, or a refusal where the normal has no direction, an input
/// is not finite, the products overflow, or the truth is not finite.
pub fn gu_point_project_to_plane() -> PinDefect<(V, V, V), Option<XP3>> {
    PinDefect {
        reason: "the pin moves a point by NaN or 0 along a normal with no length or an \
                 overflowing one",
        applies: Applies::Disagreeing,
        check: |&(q, p, n), r| {
            let ok_in = finite(&q) && finite(&p) && finite(&n);
            let truth = dir3(n).filter(|_| ok_in).and_then(|u| {
                let k = 2.0 * far_dot3(half_gap(q, p), u);
                let res = sub3(q, scale3(u, k));
                finite(&res).then_some((res, k))
            });
            let may = !ok_in
                || len3(n) < NO_LENGTH
                || near_no_length(len3(n))
                || products_overflow(&[p, q], &[n])
                || truth.is_none();
            judge(r, may, truth, |x, &(t, k)| {
                let s = 8.0 * (biggest(&q) + biggest(&p)) + k.abs();
                let x = [x.x, x.y, x.z];
                (0..3).try_for_each(|i| gu_close("the projection", x[i], t[i], s))
            })
        },
    }
}

/// The true closest-point parameters of two lines and the sizes their rounding is bounded
/// by, from `t1 = ((q - p) × ŵ)·(v̂ × ŵ) / (|v̂ × ŵ|² |v|)` and its twin.
fn gu_line_line_truth(p: V, v: V, q: V, w: V) -> Option<(f64, f64, f64, f64)> {
    if !(finite(&p) && finite(&v) && finite(&q) && finite(&w)) {
        return None;
    }
    let (u1, u2) = (dir3(v)?, dir3(w)?);
    let (l1, l2) = (len3(v), len3(w));
    let c = cross3(u1, u2);
    let c2 = dot3(c, c);
    if l1 * l2 * c2.sqrt() < NO_LENGTH {
        return None; // parallel
    }
    // `(q - p) / 8`, so the triple products cannot overflow
    let dh = sub3(scale3(q, 0.125), scale3(p, 0.125));
    let t1 = 8.0 * over_len(dot3(cross3(dh, u2), c) / c2, v);
    let t2 = 8.0 * over_len(dot3(cross3(dh, u1), c) / c2, w);
    // a size past range is held at `f64::MAX`, a tighter bound
    let size = |l| (8.0 * (over_len(len3(dh), l) / c2)).min(f64::MAX);
    (t1.is_finite() && t2.is_finite()).then_some((t1, t2, size(v), size(w)))
}

/// `line_line_closest_point` where the pin's unit of an overflowing direction fails, or it
/// answers a NaN or an infinity.
///
/// The pin unitises `(v1 × v2) × v1`, which fails once its square overflows, so lines
/// along `[1e300, 1e-13, 1e-12]` get no answer; and for input not finite or past range it
/// answers NaN or infinity. The check: the truth by Cramer's rule on unit directions, or a
/// refusal where a direction has none, the lines are parallel (in the band of scx's
/// `|v × w|² < SM_EFF_ZERO_SQ`), the products overflow, or the truth is not finite.
pub fn gu_line_line_closest_point() -> PinDefect<V4, Two> {
    PinDefect {
        reason: "the pin fails on overflowing directions and answers NaN or infinity past range",
        applies: Applies::Disagreeing,
        check: |&(p, v, q, w), r| {
            let truth = gu_line_line_truth(p, v, q, w);
            let par = len3(v) * len3(w) * len3(cross3(direction(v), direction(w)));
            let may = !(finite(&p) && finite(&v) && finite(&q) && finite(&w))
                || len3(v) < NO_LENGTH * (1.0 + 1e-9)
                || len3(w) < NO_LENGTH * (1.0 + 1e-9)
                || par < NO_LENGTH * (1.0 + 1e-6)
                || products_overflow(&[p, q], &[v, w])
                || truth.is_none();
            judge(r, may, truth, |&(x1, x2), &(t1, t2, s1, s2)| {
                gu_close("t1", x1, t1, s1)?;
                gu_close("t2", x2, t2, s2)
            })
        },
    }
}

/// `line_plane_intersect` where the pin answers NaN or an infinity: input not finite, or
/// sums past `f64`'s range. The check: the truth `2 ((q - p)/2 · n̂) / (|v| v̂ · n̂)`, or a
/// refusal where a direction has none, the line is parallel (in the band of scx's
/// `|v · n| < SM_EFF_ZERO max |v_i| + SM_EFF_ZERO_SQ`), the products overflow, or the
/// truth is not finite.
pub fn gu_line_plane_intersect() -> PinDefect<(V, V, V, V), Option<f64>> {
    PinDefect {
        reason: "the pin answers NaN or infinity for input not finite or past range",
        applies: Applies::Disagreeing,
        check: |&(p, v, q, n), r| {
            let ok_in = finite(&p) && finite(&v) && finite(&q) && finite(&n);
            let truth = (|| {
                let (uv, un) = (dir3(v)?, dir3(n)?);
                let (lv, ln) = (len3(v), len3(n));
                let cos = dot3(uv, un);
                let m = lv * ln * cos.abs();
                if !ok_in || m < GU_REL * biggest(&v) + 1e-24 {
                    return None;
                }
                let dh = half_gap(q, p);
                let t = over_len(2.0 * far_dot3(dh, un) / cos, v);
                let s = (t.abs() + over_len(2.0 * len3(dh), v)) / cos.abs();
                t.is_finite().then_some((t, s))
            })();
            let m = len3(v) * len3(n) * dot3(direction(v), direction(n)).abs();
            let thr = GU_REL * biggest(&v) + 1e-24;
            let may = !ok_in
                || len3(v) < NO_LENGTH * (1.0 + 1e-9)
                || len3(n) < NO_LENGTH * (1.0 + 1e-9)
                || m < thr + 8.0 * GU_U * len3(v) * len3(n)
                || products_overflow(&[p, q], &[v, n])
                || truth.is_none();
            judge(r, may, truth, |x, &(t, s)| {
                gu_close("the parameter", *x, t, s)
            })
        },
    }
}

/// `line_plane_intersect_uv` on the inputs of [`gu_line_plane_intersect`]. The check: the
/// truth by Cramer's rule on unit vectors, `t = -(n̂·(p - q)) / (|v| v̂·n̂)`,
/// `u = ((p - q) × ŝv)·v̂ / (|su| v̂·n̂)`, `v = (ŝu × (p - q))·v̂ / (|sv| v̂·n̂)` with
/// `n̂ = ŝu × ŝv`, or a refusal as there (and for tangents with a parallel cross product).
pub fn gu_line_plane_intersect_uv() -> PinDefect<V5, Three> {
    PinDefect {
        reason: "the pin answers NaN or infinity for input not finite or past range",
        applies: Applies::Disagreeing,
        check: |&(p, v, q, a, b), r| {
            let ok_in = [p, v, q, a, b].iter().all(|x| finite(x));
            let (lv, l1, l2) = (len3(v), len3(a), len3(b));
            let nn = cross3(direction(a), direction(b));
            let ln = l1 * l2 * len3(nn);
            let g = dot3(direction(v), nn);
            let m = lv * l1 * l2 * g.abs();
            let thr = GU_REL * biggest(&v) + 1e-24;
            let truth = (|| {
                let (uv, u1, u2) = (dir3(v)?, dir3(a)?, dir3(b)?);
                if !ok_in || ln < NO_LENGTH || m < thr {
                    return None;
                }
                let dh = half_gap(p, q);
                let t = over_len(-2.0 * dot3(nn, dh) / g, v);
                let u = over_len(2.0 * dot3(cross3(dh, u2), uv) / g, a);
                let w = over_len(2.0 * dot3(cross3(u1, dh), uv) / g, b);
                let d = 2.0 * len3(dh);
                let sizes = [
                    (t.abs() + over_len(d, v)) / g.abs(),
                    (u.abs() + over_len(d, a)) / g.abs(),
                    (w.abs() + over_len(d, b)) / g.abs(),
                ];
                (t.is_finite() && u.is_finite() && w.is_finite()).then_some(([t, u, w], sizes))
            })();
            let may = !ok_in
                || [lv, l1, l2].iter().any(|&l| l < NO_LENGTH * (1.0 + 1e-9))
                || ln < NO_LENGTH * (1.0 + 1e-6)
                || m < thr + 8.0 * GU_U * lv * l1 * l2
                || products_overflow(&[p, q], &[v, a, b])
                || truth.is_none();
            judge(r, may, truth, |&(xt, xu, xv), (t, s)| {
                gu_close("t", xt, t[0], s[0])?;
                gu_close("u", xu, t[1], s[1])?;
                gu_close("v", xv, t[2], s[2])
            })
        },
    }
}

/// The common point of three planes, scaled by an eighth so the sums cannot overflow, with
/// the planes' unit normals and `|n1 · (n2 × n3)|`; `None` for input not finite or a point
/// past `f64`'s range.
fn gu_three_planes_truth(planes: [(V, V); 3]) -> Option<(V, f64)> {
    let [(p1, n1), (p2, n2), (p3, n3)] = planes;
    if !planes.iter().all(|(p, n)| finite(p) && finite(n)) {
        return None;
    }
    let det = dot3(n1, cross3(n2, n3));
    let r = [p1, p2, p3]
        .iter()
        .zip([n1, n2, n3])
        .map(|(p, n)| far_dot3(scale3(*p, 0.125), n) / det)
        .collect::<Vec<_>>();
    let x8 = add3(
        add3(scale3(cross3(n2, n3), r[0]), scale3(cross3(n3, n1), r[1])),
        scale3(cross3(n1, n2), r[2]),
    );
    (finite(&x8) && biggest(&x8) < f64::MAX / 8.0 * (1.0 - 1e-12))
        .then(|| (scale3(x8, 8.0), det.abs()))
}
/// Each plane passes through `x`: `|n̂·x - n̂·p| <= GU_REL (|x| + |p|) / |det|`.
fn gu_on_planes(x: V, planes: &[(V, V)], det: f64) -> Result<(), String> {
    for &(p, n) in planes {
        let res = far_dot3(x, n) - far_dot3(p, n);
        let tol = GU_REL * (len3(x) + len3(p)) / det.min(1.0);
        if res.is_nan() || res.abs() > tol {
            return Err(format!(
                "{x:?} is {res:e} off the plane through {p:?} (to {tol:e})"
            ));
        }
    }
    Ok(())
}

/// `intersect_three_planes` where the pin answers a point not finite, or rounds an
/// ill-conditioned one differently.
///
/// For a plane point not finite, or a point past `f64`'s range, the pin answers NaN or
/// infinite coordinates; scx refuses. Where the normals are nearly dependent the pin's
/// six-term determinant and scx's triple product round apart (`1e-12` relative for a
/// determinant near `1e-5`). The check: scx's point lies on all three planes to the
/// rounding of the input, magnified by `1 / |det|`; a refusal only for input not finite,
/// a point past range, or a determinant in the band of `SM_EFF_ZERO`.
pub fn gu_intersect_three_planes() -> PinDefect<V6, Option<Option<XP3>>> {
    PinDefect {
        reason: "the pin answers points not finite, and rounds ill-conditioned ones apart",
        applies: Applies::Disagreeing,
        check: |&(a, m, b, n, c, o), r| {
            let (Some(m), Some(n), Some(o)) = (gu_unit(m), gu_unit(n), gu_unit(o)) else {
                return match r {
                    None => Ok(()),
                    Some(x) => Err(format!("a normal has no direction, not {x:?}")),
                };
            };
            let planes = [(a, m), (b, n), (c, o)];
            let det = dot3(m, cross3(n, o)).abs();
            let truth = gu_three_planes_truth(planes).filter(|_| det >= 1e-12);
            let may = truth.is_none() || (det - 1e-12).abs() <= 8.0 * GU_U;
            let x = r.as_ref().ok_or("the normals have directions")?;
            judge(x, may, truth, |x, &(_, det)| {
                gu_on_planes(x.to_array(), &planes, det)
            })
        },
    }
}

type TwoPlanes = Option<Option<(&'static str, Vec<f64>)>>;

/// `intersect_two_planes` where the pin calls antiparallel planes apart coincident, or
/// answers a line point not finite.
///
/// For antiparallel normals the pin's two signed distances `(p2 - p1)·n1` and
/// `(p1 - p2)·n2` are equal, so planes any distance apart pass its `< scaled zero` test
/// on the side they face away. For a point not finite or past range it answers NaN or
/// infinity. The check: parallel normals (`|n1 × n2|² < SM_EFF_ZERO_SQ`, with a band) are
/// "Coincident" exactly when both absolute distances are below
/// `SM_EFF_ZERO (1 + max |coordinate|)` (with a band), else "Parallel"; a line's point lies
/// on both planes and the plane through their midpoint normal to both, and its direction
/// is `n̂1 × n̂2` normalised; a refusal only as [`gu_intersect_three_planes`].
pub fn gu_intersect_two_planes() -> PinDefect<(V, V, V, V), TwoPlanes> {
    PinDefect {
        reason: "the pin calls antiparallel planes on opposite sides coincident",
        applies: Applies::Disagreeing,
        check: |&(a, m, b, n), r| {
            let (Some(m), Some(n)) = (gu_unit(m), gu_unit(n)) else {
                return match r {
                    None => Ok(()),
                    Some(x) => Err(format!("a normal has no direction, not {x:?}")),
                };
            };
            let x = r.as_ref().ok_or("the normals have directions")?;
            let c = cross3(m, n);
            let c_len = len3(c);
            let parallel_band = (c_len - NO_LENGTH).abs() <= 1e-6 * NO_LENGTH;
            let ok_in = finite(&a) && finite(&b);
            if c_len < NO_LENGTH || parallel_band {
                let sz = 1e-12 * (1.0 + biggest(&a).max(biggest(&b)));
                let dh = half_gap(b, a);
                let d1 = 2.0 * far_dot3(dh, m).abs();
                let d2 = 2.0 * far_dot3(dh, n).abs();
                let near = |d: f64| (d - sz).abs() <= 1e-9 * sz;
                let want = if d1 < sz && d2 < sz {
                    "Coincident"
                } else {
                    "Parallel"
                };
                match x {
                    Some((label, _)) if *label == want => return Ok(()),
                    Some((label, _)) if (near(d1) || near(d2)) && *label != "Line" => return Ok(()),
                    None if !ok_in => return Ok(()),
                    Some(("Line", _)) if parallel_band => {}
                    _ => return Err(format!("parallel planes are {want}, not {x:?}")),
                }
            }
            let Some(u) = dir3(c) else {
                return Err(format!("the planes cross, not {x:?}"));
            };
            let mid = add3(half(a), half(b));
            let planes = [(a, m), (b, n), (mid, u)];
            let truth = gu_three_planes_truth(planes);
            let may = !ok_in || truth.is_none() || products_overflow(&[a, b], &[]);
            judge(x, may, truth, |(label, v), &(_, det)| {
                if *label != "Line" || v.len() != 6 {
                    return Err(format!("the planes cross along a line, not {label}"));
                }
                gu_on_planes([v[0], v[1], v[2]], &planes, det)?;
                near3("the direction", [v[3], v[4], v[5]], u, 1e-12)
            })
        },
    }
}

/// `segment_point_distance` and `segment_point_angle_to_perp` (in degrees) where the pin
/// answers NaN, an infinity or a silent 0.
///
/// Past range the pin's `|v|²`, `|q - foot|²` and the angle's squares overflow: it answers
/// NaN parameters, infinite distances and a 0 parameter for segments `1e300` long (so the
/// distance is to the wrong end). Where the angle is a few `1e-14` degrees from square the
/// pin's `angle · 180/π - 90` rounds at `90`'s ulp; scx takes `|angle - π/2|` in radians,
/// exactly, before converting. The check: the true parameter (clamped), the distance to
/// the true foot, and the angle `asin(|ĝ · v̂|)` of the gap `g` from the nearest end, each
/// to the rounding of its inputs; NaN parameters only where there is no answer.
pub fn gu_segment_point() -> PinDefect<(V, V, V), SegR> {
    PinDefect {
        reason: "the pin answers NaN, infinity or 0 past range, and rounds near-square angles",
        applies: Applies::Disagreeing,
        check: |&(a, b, q), &((xd, xt), (xang, xt2))| {
            let ok_in = finite(&a) && finite(&b) && finite(&q);
            let vh = half_gap(b, a);
            let v = scale3(vh, 2.0);
            let lv = 2.0 * len3(vh);
            if !ok_in {
                return if xt.is_nan() && xt2.is_nan() {
                    Ok(())
                } else {
                    Err(format!("no segment, so no parameter, not {xt:e}"))
                };
            }
            let tl = if lv < NO_LENGTH {
                if near_no_length(lv) {
                    return Ok(());
                }
                Some((0.0, 0.0))
            } else {
                gu_line_param(a, v, q).filter(|_| finite(&v))
            };
            let Some((tl, s)) = tl else {
                // the parameter is past range, or the segment's length overflows
                return if xt.is_nan() || !finite(&v) {
                    Ok(())
                } else {
                    Err(format!("no finite parameter, not {xt:e}"))
                };
            };
            if xt.is_nan() && products_overflow(&[a, b, q], &[v]) {
                return Ok(());
            }
            let t = tl.clamp(0.0, 1.0);
            gu_close("the parameter", xt, t, s)?;
            gu_close("the angle's parameter", xt2, t, s)?;
            // the foot and the gap, at half size
            let foot_h = add3(half(a), scale3(vh, t));
            let gh = sub3(half(q), foot_h);
            let dist = 2.0 * len3(gh);
            let coords = biggest(&a) / 2.0 + biggest(&b) / 2.0 + biggest(&q) / 2.0;
            if dist.is_finite() {
                gu_close("the distance", xd, dist, 16.0 * coords)?;
            } else if xd.is_finite() {
                return Err(format!("the distance is past range, not {xd:e}"));
            }
            let ang = if (0.0..=1.0).contains(&tl) || dist < NO_LENGTH {
                0.0
            } else {
                // atan2, not asin, so the truth is good next to square too
                let (ug, uv) = (direction(gh), direction(v));
                let (cos, sin) = (dot3(ug, uv).abs(), len3(cross3(ug, uv)));
                scx_num::math::rad_to_deg(scx_num::fmath::atan2(cos, sin))
            };
            // the gap is rounded at the coordinates' ulps (the half gap and coordinates
            // are both at half size)
            let slack_rad = 4.0 * GU_U * (1.0 + 2.0 * coords / len3(gh).max(f64::MIN_POSITIVE));
            let tol = GU_REL * ang + scx_num::math::rad_to_deg(slack_rad);
            let near_end = tl.abs() <= GU_REL * s || (tl - 1.0).abs() <= GU_REL * s;
            if (xang - ang).abs() <= tol || (near_end && xang.abs() <= tol) {
                Ok(())
            } else {
                Err(format!(
                    "the angle {xang:e} is not the true {ang:e} (to {tol:e})"
                ))
            }
        },
    }
}

/// `polygon_area` of no points. Upstream reads `sPoints[-1]` and the pin fails; no points
/// enclose no area. The check: 0 (when the normal has a direction).
pub fn gu_polygon_area() -> PinDefect<(List<V>, V), Option<f64>> {
    PinDefect {
        reason: "the pin fails on a polygon of no points",
        applies: Applies::Inputs(|(pts, _)| pts.0.is_empty()),
        check: |(_, n), r| {
            let want = gu_unit(*n).map(|_| 0.0);
            if r.map(f64::to_bits) == want.map(f64::to_bits) {
                Ok(())
            } else {
                Err(format!("no points enclose no area: {want:?}, not {r:?}"))
            }
        },
    }
}

/// `triangle_decomposition` of sides past `1e154`, whose squares overflow: the pin answers
/// NaN; scx takes the sides over the longest. The check: the foot `q` of the apex from the
/// right end and the height from the scaled sides, `q = (b² + r² - l²) / 2b`,
/// `h = √(r² - q²)`, each to the rounding of the squares (`s² / b` and `s² / h` for the
/// longest side `s`); a refusal only for sides not finite, or in the band of the
/// triangle's own tests.
pub fn gu_triangle_decomposition() -> PinDefect<(f64, f64, f64), Option<(f64, f64)>> {
    PinDefect {
        reason: "the pin answers NaN for sides whose squares overflow",
        applies: Applies::Disagreeing,
        check: |&(b, l, r), x| {
            let ok_in = b.is_finite() && l.is_finite() && r.is_finite();
            let s = b.abs().max(l.abs()).max(r.abs());
            let (bs, ls, rs) = (b / s, l / s, r / s);
            let qs = (bs * bs + rs * rs - ls * ls) / (2.0 * bs);
            let least = b + 1e-12 * (1.0 + b);
            let legs = l + r;
            let triangle = ok_in && b.abs() >= 1e-12 && legs >= least && qs.abs() <= rs;
            let band = (b.abs() - 1e-12).abs() <= 1e-9
                || (legs - least).abs() <= 1e-9 * least.abs()
                || (qs.abs() - rs).abs() <= 1e-9;
            let truth = triangle.then(|| {
                let h = (rs * rs - qs * qs).max(0.0).sqrt();
                ((bs - qs) * s, h * s)
            });
            judge(x, !ok_in || band, truth, |&(xa, xh), &(a, h)| {
                gu_close("the foot", xa, a, s * (s / b.abs()))?;
                gu_close(
                    "the height",
                    xh,
                    h,
                    s * (s / h.max(f64::MIN_POSITIVE)).min(1e12),
                )
            })
        },
    }
}

/// `parallelogram_cross_vectors` with sides past `1e154`, parallel sides, or distances not
/// finite or past range.
///
/// The pin's unitise fails on sides whose square overflows (no answer for
/// `[1e300, 1e300, -1]`); it divides by a zero sine for parallel sides and answers
/// infinities and NaN, and does likewise for distances not finite. The check: the true
/// diagonals `â (2h / s) + b̂ (h / s)` and `b̂ (h / s) - â (2h / s)` with `s = |â × b̂|`, to
/// the rounding of the sine (relative `ε / s`); a refusal only for sides of no direction,
/// sides parallel to rounding (`s` below `1e-15`), or diagonals past range.
pub fn gu_parallelogram_cross_vectors() -> PinDefect<(V, V, f64), Option<(XV3, XV3)>> {
    PinDefect {
        reason: "the pin fails on overflowing sides and divides by a zero sine",
        applies: Applies::Disagreeing,
        check: |&(a, b, h), r| {
            let truth = (|| {
                let (ua, ub) = (dir3(a)?, dir3(b)?);
                let s = len3(cross3(ua, ub));
                if s == 0.0 || !h.is_finite() {
                    return None;
                }
                let (lb, ls) = (2.0 * h / s, h / s);
                let d1 = add3(scale3(ua, lb), scale3(ub, ls));
                let d2 = sub3(scale3(ub, ls), scale3(ua, lb));
                (finite(&d1) && finite(&d2)).then_some((d1, d2, s, lb.abs()))
            })();
            let may = truth.is_none_or(|(_, _, s, _)| s < 1e-15)
                || [a, b].iter().any(|v| near_no_length(len3(*v)));
            judge(r, may, truth, |(x1, x2), (d1, d2, s, lb)| {
                // unit sides are rounded at `ε`, so the sine is good to `ε` absolute
                let tol = lb * (GU_REL + 8.0 * GU_U / s);
                near3("the first diagonal", x1.to_array(), *d1, tol)?;
                near3("the second diagonal", x2.to_array(), *d2, tol)
            })
        },
    }
}

/// `best_fit_plane` where the pin's gap misses the points below the plane.
///
/// `smgu_BestFitPlaneToPoints` keeps the largest signed distance `n · (p - c)`, so a set
/// bent to one side of its plane passes for planar however far it bends below; scx keeps
/// the largest absolute one. The check: scx's gap is the largest `|n · (p - c)|` over the
/// points for scx's own point and normal, the set is "Planar" exactly when that gap is
/// within the tolerance, and everything else (point, normal, centroid gap) is the pin's
/// to the bit, since the defect is in the gap alone.
pub fn gu_best_fit_plane() -> PinDefect<(Pts, f64), Option<FitOut>> {
    PinDefect {
        reason: "the pin's plane gap is the largest signed distance, not the largest distance",
        applies: Applies::Disagreeing,
        check: |(pts, tol), r| {
            let (kind, c, n, gap, cgap) = r.ok_or("the pin fits these points")?;
            let n = n.ok_or("a plane fit has a normal")?;
            let c = c.to_array();
            let want = pts
                .0
                .iter()
                .map(|&p| far_dot3(sub3(p, c), n).abs())
                .fold(0.0_f64, f64::max);
            if !ulp_close(gap, want) && (gap - want).abs() > 4.0 * GU_U * biggest(&c) {
                return Err(format!(
                    "the gap {gap:e} is not the largest distance {want:e}"
                ));
            }
            let kind_want = if gap <= *tol { "Planar" } else { "Scattered" };
            if kind != kind_want {
                return Err(format!(
                    "a gap of {gap:e} at {tol:e} is {kind_want}, not {kind}"
                ));
            }
            let sp: Vec<solidcore::geom::vector3d::Vec3> = pts
                .0
                .iter()
                .map(|q| solidcore::geom::vector3d::Vec3::new(q[0], q[1], q[2]))
                .collect();
            let s = solidcore::geom::geom_utility::p2::best_fit_plane_to_points(&sp, *tol)
                .map_err(|e| format!("the pin fits these points: {e:?}"))?;
            let same = |x: f64, y: f64| x.to_bits() == y.to_bits();
            let sn = s.normal;
            if same(c[0], s.point.x)
                && same(c[1], s.point.y)
                && same(c[2], s.point.z)
                && same(n[0], sn.x)
                && same(n[1], sn.y)
                && same(n[2], sn.z)
                && same(cgap, s.max_centroid_gap)
            {
                Ok(())
            } else {
                Err("the point, normal or centroid gap moved too".into())
            }
        },
    }
}

/// `sort` of values with a NaN. The pin's shell sort compares with `<`, which a NaN fails
/// both ways, so the NaN stops nowhere and splits the run around it unsorted
/// (`[2, 1.1e-11, NaN]` stays put; `[-1e-12, 5e-6, NaN, 1.2e6, …]` keeps `7.3e-12` after
/// `NaN`). scx sorts by `f64::total_cmp`, NaNs at the ends by sign. The check: the output
/// is the input reordered (as bits) and runs in the total order.
pub fn gu_sort() -> PinDefect<(List<f64>,), Vec<f64>> {
    PinDefect {
        reason: "the pin's shell sort leaves a NaN anywhere and the run around it unsorted",
        applies: Applies::Inputs(|(v,)| v.0.iter().any(|x| x.is_nan())),
        check: |(v,), r| {
            let mut a: Vec<u64> = v.0.iter().map(|x| x.to_bits()).collect();
            let mut b: Vec<u64> = r.iter().map(|x| x.to_bits()).collect();
            a.sort_unstable();
            b.sort_unstable();
            if a != b {
                return Err(format!("{r:?} is not {:?} reordered", v.0));
            }
            if r.windows(2).all(|w| w[0].total_cmp(&w[1]).is_le()) {
                Ok(())
            } else {
                Err(format!("{r:?} is not in the total order"))
            }
        },
    }
}

// ---- point sets ----------------------------------------------------------------------

use crate::t_pset::{SetOut, XBox};

/// Whether the pin's `unitize_or_z` loses `v`: its squared length is past `f64::MAX`
/// (the pin divides by an infinite length and gets the zero vector) or under
/// `SM_EFF_ZERO_SQ` (it answers `Z`).
/// Only vectors scx makes unit: it refuses the others as well.
fn ps_unitize_lost(v: &V) -> bool {
    let l = v.iter().map(|c| c * c).sum::<f64>();
    crate::t_gutil::unit(*v).is_some() && (!l.is_finite() || l < 1e-24)
}

/// The set's fit is [`gu_best_fit_plane`]'s, whose pin defect (a signed gap) makes a set
/// bent to one side of its plane `Planar` in the pin and `Scattered` in scx; the type
/// picks the tight box's hull-edge alignment, so the boxes, and through a `Linear`/
/// `Planar` frame the loop's answers, follow. The check: scx's fit passes
/// [`gu_best_fit_plane`]'s truth check, and the pin's fit has the other type.
fn ps_fit_moved(pts: &Pts, tol: f64) -> Result<&'static str, String> {
    use crate::t_gutil::{stype, xdir, xtype};
    let xp: Vec<scx_num::vector3d::Point3> = pts
        .0
        .iter()
        .map(|q| scx_num::vector3d::Point3::new(q[0], q[1], q[2]))
        .collect();
    let tol = tol.abs();
    let f = scx_algo::geom::fit::best_fit_plane(&xp, tol).map_err(|e| format!("{e:?}"))?;
    let out: FitOut = (
        xtype(f.set_type),
        f.point,
        xdir(f.direction),
        f.max_plane_gap,
        f.max_centroid_gap,
    );
    (gu_best_fit_plane().check)(&(pts.clone(), tol), &Some(out))?;
    let sp: Vec<solidcore::geom::vector3d::Vec3> = pts
        .0
        .iter()
        .map(|q| solidcore::geom::vector3d::Vec3::new(q[0], q[1], q[2]))
        .collect();
    let s = solidcore::geom::geom_utility::p2::best_fit_plane_to_points(&sp, tol)
        .map_err(|e| format!("the pin fits these points: {e:?}"))?;
    if stype(s.set_type) == out.0 {
        return Err(format!(
            "both fits are {}: the disagreement is not the fit's",
            out.0
        ));
    }
    Ok(out.0)
}

pub fn ps_set() -> PinDefect<(Pts, f64), Option<SetOut>> {
    PinDefect {
        reason: "the pin's set takes its type from the signed-gap plane fit",
        applies: Applies::Disagreeing,
        check: |(pts, tol), r| {
            let kind = ps_fit_moved(pts, *tol)?;
            let got = r.as_ref().ok_or("scx builds this set")?.0;
            if got == kind {
                Ok(())
            } else {
                Err(format!("the set is {got}, its fit {kind}"))
            }
        },
    }
}

pub fn ps_tight_about() -> PinDefect<(Pts, f64, V), Option<Option<XBox>>> {
    PinDefect {
        reason: "the pin's set takes its type, which picks the alignment, from the signed-gap plane fit; and its unitize_or_z loses a z whose squared length is past f64::MAX or under SM_EFF_ZERO_SQ",
        applies: Applies::Disagreeing,
        check: |(pts, tol, z), r| {
            if ps_unitize_lost(z) {
                let b = r.as_ref().ok_or("scx takes this z")?;
                // no points: no box either side
                return match b {
                    Some(_) => ps_box_along(b, *z),
                    None => Err("the pin and scx both have no box".into()),
                };
            }
            ps_fit_moved(pts, *tol).map(|_| ())
        },
    }
}

/// The box's third vector is `z`'s direction (the check for a `z` the pin loses).
fn ps_box_along(b: &Option<XBox>, z: V) -> Result<(), String> {
    let (v, _) = b.as_ref().ok_or("scx boxes these points")?;
    along(v[2].to_array(), z)
}

pub fn ps_with_normal() -> PinDefect<(Pts, f64, V), Option<SetOut>> {
    PinDefect {
        reason: "the pin's unitize_or_z loses a normal whose squared length is past f64::MAX or under SM_EFF_ZERO_SQ, and fits the points instead",
        applies: Applies::Inputs(|(_, _, n)| ps_unitize_lost(n)),
        check: |(_, _, n), r| {
            let (kind, _, normal, _, tight) = r.as_ref().ok_or("scx builds this set")?;
            if *kind != "Planar" {
                return Err(format!("a set with a given normal is {kind}"));
            }
            along(normal.ok_or("a set has a normal")?, *n)?;
            match tight {
                Some(_) => ps_box_along(tight, *n),
                None => Ok(()),
            }
        },
    }
}
