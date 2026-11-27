// Written for solidcorex. Apache-2.0.
//! The inputs on which the pinned solidcore is known to be wrong, one [`PinDefect`] per test.
//! On those inputs the pin is not compared; scx's result must pass the entry's check against
//! the true value instead. Each entry gives its evidence: the true value, scx's error and the
//! pin's.

use crate::harness::{Applies, PinDefect};
use scx_num::certified::Verdict;

type M = [[f64; 4]; 4];

/// `Transform::is_regular` on a linear part whose largest entry is of `f64::MAX`'s scale
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
            let finite = t.iter().flatten().all(|x| x.is_finite());
            let affine = t[0][3] == 0.0 && t[1][3] == 0.0 && t[2][3] == 0.0 && t[3][3] == 1.0;
            let big = linear(t).fold(0.0_f64, |b, v| b.max(v.abs()));
            finite
                && affine
                && ratio.is_finite()
                && *ratio > 0.0
                && scx_num::fmath::log2(big) == 1024.0
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
pub fn e2_mid() -> PinDefect<([f64; 4],), scx_num::vector2d::Point2> {
    PinDefect {
        reason: "the pin's guarded divide answers the corner sum, not the mid, for large boxes",
        applies: Applies::Disagreeing,
        check: |(a,), p| {
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

type Map = scx_num::math::Transform;
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
            t.transform_vector(vec(e)).to_array(),
            lin(e),
            lin_tol,
        )?;
    }
    let o = scx_num::vector3d::Point3::ORIGIN;
    near3(
        "the translation",
        t.transform_point(o).to_array(),
        shift,
        tol,
    )
}

/// `Transform::reflection(origin, normal)`: `p' = p - 2 ((p - origin)·n) n`.
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

/// `Transform::rotation(point, axis, angle)`: Rodrigues' formula in vector form,
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

/// `Transform::into_plane_frame(origin, x, y)`: the unit axes `x̂`, `n = unit(x̂ × ŷ)` and
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
                    t.transform_vector(vec(u)).to_array(),
                    e,
                    tol,
                )?;
            }
            let o = scx_num::vector3d::Point3::ORIGIN;
            let tt = (tol + 1e-14) * biggest(p);
            near3(
                "the translation",
                t.transform_point(o).to_array(),
                shift,
                tt,
            )
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

/// `Transform::orthonormal_frame(x, y)`, `make_unit_ortho_vectors`'s first two axes.
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

/// `Transform::axis_squared_frame(x, y, noise)`: each axis with its coordinates below `noise`
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
/// inverse map (`Transform::inverse`, held to the pin by `tr_inverse`), unit. `Ok(None)` when
/// that image's length is in the band about [`NO_LENGTH`] where either answer is right.
fn own_axis(t: &M, axis: V) -> Result<Option<V>, scx_num::math::AxisRefusal> {
    use scx_num::math::AxisRefusal as Ax;
    if !(t.iter().all(|r| finite(r)) && finite(&axis)) {
        return Err(Ax::NotFinite);
    }
    let map = Map { m: *t };
    let inverse = map.inverse().map_err(|_| Ax::FlatMap)?;
    let a = dir3(axis).ok_or(Ax::ZeroAxis)?;
    let own = inverse.transform_vector(vec(a)).to_array();
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
            let want = if !(t.iter().all(|r| finite(r)) && finite(axis)) {
                Err(Ax::NotFinite)
            } else if let Some(a) = dir3(*axis) {
                let map = Map { m: *t };
                let image = dir3(*n)
                    .filter(|_| finite(o))
                    .map(|n| map.transform_vector(vec(n)).to_array());
                if image.is_some_and(|i| (0.5 * NO_LENGTH..2.0 * NO_LENGTH).contains(&len3(i))) {
                    return Ok(());
                }
                match image.and_then(dir3).map(|p| len3(cross3(p, a))) {
                    None => Ok(None),
                    Some(c) if (c - 1e-9).abs() < 1e-15 => return Ok(()),
                    Some(c) if c > 1e-9 => Ok(None),
                    Some(_) => {
                        let at = scx_num::vector3d::Point3::from_array(*o);
                        Ok(Some((map.transform_point(at).to_array(), a)))
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
        applies: |(t,)| t.0.replace(' ', "").contains("=();"),
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
        applies: |(t,)| typed_arities(t.0).iter().any(|&n| n > 1),
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
        applies: |(t,)| typed_arities(t.0).contains(&0),
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
        applies: |(t,)| {
            t.0.split('.').skip(1).step_by(2).any(|tok| {
                tok.contains('-')
                    && tok.bytes().all(|c| {
                        c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_' || c == b'-'
                    })
            })
        },
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
        applies: |(t,)| t.0.contains(r"\PA\"),
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
        applies: |(t,)| {
            t.0.split('"')
                .nth(1)
                .is_some_and(|h| h.bytes().any(|c| c.is_ascii_lowercase()))
        },
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
