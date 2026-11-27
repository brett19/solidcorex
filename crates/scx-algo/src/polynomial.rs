// Moved from solidcore `solidcore/crates/solidcore/src/geom/polynomial.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/src/SmPolynomial.cpp`,
// `source/SMLib/inc/SmPolynomial.h`.
// Changes: paths only (`acos`, `cbrt`, `sqrt` and the constants from `scx-num`). Results
// unchanged bit for bit.
// Then the cubic and quartic roots gained Newton polishing on the original polynomial.
// Then the cubic and quartic were rewritten (docs/research/70): the closed forms and their
// absolute tolerances gave way to bracketing between critical points, so their roots,
// their count where the pin's were wrong, and their order differ from the pin's.
//! Real roots of quadratic, cubic and quartic polynomials.
//!
//! Upstream: `source/SMLib/src/SmPolynomial.cpp`, `source/SMLib/inc/SmPolynomial.h`.
//!
//! | Rust                  | Upstream                          |
//! |-----------------------|-----------------------------------|
//! | [`Roots`]             | `ULONG rlNumSolutions` + `double adSolutions[]` |
//! | [`solve_quadratic`]   | `SmPolynomial::SolveQuadraticEqn` |
//! | [`solve_cubic`]       | `SmPolynomial::SolveCubicEqn` (rewritten) |
//! | [`solve_quartic`]     | `SmPolynomial::SolveQuarticEqn` (rewritten) |
//!
//! Coefficients are ordered by ascending power, as upstream: `c[i]` multiplies
//! `x^i`, so `[C, B, A]` is `A x^2 + B x + C`.
//!
//! # The cubic and quartic
//!
//! Upstream solves both in closed form (Cardano, and Ferrari through the resolvent cubic),
//! deciding double roots, triple roots and the quartic's branches by comparing
//! intermediate values with `zero_tol`. Those values are not scaled to the polynomial, so
//! the decisions go wrong for small or clustered roots and near double roots: roots are
//! lost, invented, or off by far more than their conditioning. Here `zero_tol` only drops
//! the degree (a leading coefficient below it), as upstream; past that nothing is compared
//! with an absolute tolerance, and the roots of `2^k p(2^j x)` are those of `p` times
//! `2^-j`, bit for bit, barring overflow and underflow.
//!
//! The roots are bracketed by the critical points (the derivative's roots, from the
//! stable quadratic formula for a cubic and recursively from the cubic for a quartic):
//! between two consecutive critical points `p` is monotone, so a change of sign there
//! holds exactly one root. Each sign is taken from plain Horner when the value clears its
//! error bound, else from compensated Horner. Each root is then found by Newton's method
//! from a start where it converges monotonically (the inflection point between two
//! critical points; outside them, a Taylor bound that over-reaches the root), kept inside
//! the bracket, and finished by compensated Newton steps.
//!
//! The contract, with `S(x) = Σ |c_i| |x|^i` and `η = 2^-90`:
//! - Every real root `r` of the given (double) coefficients has a returned root within
//!   half a unit in the last place of `r`, plus its pseudo-zero radius
//!   `ρ(r) = min_k (k! η S(r) / |p^(k)(r)|)^(1/k)`. For a simple root that is
//!   `η S(r) / |p'(r)|` or smaller; for a root of multiplicity `m`, the `k = m` term.
//! - Roots closer together than their radii may be reported once.
//! - Every returned root is within that bound of a real root, or is a point where
//!   `|p(x)| <= η S(x)` (a pair of complex roots too close to the real axis to tell
//!   from a double root).
//! - The roots are distinct and ascending; at most the degree of them.
//!
//! `tests/polynomial_families.rs` checks this against exact Sturm sequences on Wilkinson,
//! Chebyshev, geometric, Mignotte, clustered and near-double families at several scales.
//!
//! Deviations:
//! - The out-params (`rlNumSolutions`, `adSolutions`) become a returned [`Roots`];
//!   the `SmStatus` return is dropped because every path returns `SM_SUCCESS`.
//!   Upstream's unused tail slots (zeroed or left uninitialised) are not exposed.
//! - [`solve_cubic`] and [`solve_quartic`] are not upstream's method (above). Their roots
//!   are ascending, not in upstream's order; roots closer than `zero_tol` are not merged
//!   (upstream's `sm_AddSolution`); a coefficient that is not finite gives no roots; a
//!   zero leading coefficient drops the degree even when `zero_tol` is zero or NaN.
//! - The `SM_DEBUG_CODE` self-checks (disabled upstream) are not ported.

use core::ops::Deref;

use scx_num::eft::{two_prod, two_sum};
use scx_num::math::EFF_ZERO_SQRT;
use scx_num::vector3d::sqrt;


/// Up to four real roots: [`solve_cubic`] and [`solve_quartic`] give them ascending,
/// [`solve_quadratic`] in upstream's order.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Roots {
    values: [f64; 4],
    len: usize,
}

impl Roots {
    /// No roots.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            values: [0.0; 4],
            len: 0,
        }
    }

    /// The roots as a slice.
    #[must_use]
    pub fn as_slice(&self) -> &[f64] {
        self.values.get(..self.len).unwrap_or(&[])
    }

    /// Appends a root (ignored once four roots are stored, which no solver
    /// reaches).
    fn push(&mut self, x: f64) {
        if let Some(slot) = self.values.get_mut(self.len) {
            *slot = x;
            self.len += 1;
        }
    }

    /// Every root times `2^j` (`j` within the normal exponents).
    fn scaled(mut self, j: i32) -> Self {
        if j != 0 {
            let factor = f64::from_bits(((1023 + j) as u64) << 52);
            let len = self.len;
            for x in self.values.iter_mut().take(len) {
                *x *= factor;
            }
        }
        self
    }

    /// Appends a root unless it equals the last one stored.
    fn push_distinct(&mut self, x: f64) {
        if self.as_slice().last() != Some(&x) {
            self.push(x);
        }
    }
}

impl Deref for Roots {
    type Target = [f64];

    fn deref(&self) -> &[f64] {
        self.as_slice()
    }
}

/// Solves `A x^2 + B x + C = 0` with `coefficients = [C, B, A]`.
///
/// Returns no roots for imaginary roots or a constant equation, one root for a
/// linear equation or a (near-)double root, and two otherwise (`sqrt(disc) - p`
/// first). `zero_tol` is the largest value treated as zero (typically `1e-8` to
/// `1e-12`). From Graphics Gems I.
#[must_use]
pub fn solve_quadratic(coefficients: &[f64; 3], zero_tol: f64) -> Roots {
    let [c0, c1, c2] = *coefficients;
    let mut roots = Roots::new();

    if c2.abs() < zero_tol {
        // No quadratic term: constant (no meaningful solution) or linear.
        if c1.abs() < zero_tol {
            // Independent of x.
        } else {
            roots.push(-c0 / c1);
        }
    } else if c0.abs() < zero_tol {
        // x * (Ax + B) = 0: x = 0, and -B/A unless that is also zero.
        roots.push(0.0);
        if c1.abs() < zero_tol {
            // Second root is also zero.
        } else {
            roots.push(-c1 / c2);
        }
    } else {
        let p = c1 / (2.0 * c2);
        let q = c0 / c2;

        let disc = p * p - q;

        if disc.abs() < zero_tol || (disc < 0.0 && disc.abs() < EFF_ZERO_SQRT) {
            // One 'grazing' solution.
            roots.push(-p);
        } else if disc < 0.0 {
            // Imaginary roots.
        } else {
            let sqrt_disc = sqrt(disc);
            roots.push(sqrt_disc - p);
            roots.push(-sqrt_disc - p);
        }
    }
    roots
}


/// Solves `A x^3 + B x^2 + C x + D = 0` with `coefficients = [D, C, B, A]`.
///
/// Falls back to [`solve_quadratic`] when `|A| < zero_tol` (or `A` is zero). Otherwise
/// `zero_tol` is not read: the real roots are returned ascending and distinct, each within
/// the bound of the module's contract. A coefficient that is not finite gives no roots.
#[must_use]
pub fn solve_cubic(coefficients: &[f64; 4], zero_tol: f64) -> Roots {
    let [c0, c1, c2, c3] = *coefficients;
    if c3.abs() < zero_tol || c3 == 0.0 {
        return solve_quadratic(&[c0, c1, c2], zero_tol);
    }
    if !coefficients.iter().all(|c| c.is_finite()) {
        return Roots::new();
    }
    // the same roots with a positive leading coefficient (negation is exact)
    let c = if c3 < 0.0 {
        coefficients.map(|v| -v)
    } else {
        *coefficients
    };
    let (c, j) = balanced(&c);
    cubic_roots(&Poly::exact(c), true).0.scaled(j)
}

/// Solves `A x^4 + B x^3 + C x^2 + D x + E = 0` with
/// `coefficients = [E, D, C, B, A]`.
///
/// Falls back to [`solve_cubic`] when `|A| < zero_tol` (or `A` is zero). Otherwise
/// `zero_tol` is not read: the real roots are returned ascending and distinct, each within
/// the bound of the module's contract. A coefficient that is not finite gives no roots.
#[must_use]
pub fn solve_quartic(coefficients: &[f64; 5], zero_tol: f64) -> Roots {
    let [c0, c1, c2, c3, c4] = *coefficients;
    if c4.abs() < zero_tol || c4 == 0.0 {
        return solve_cubic(&[c0, c1, c2, c3], zero_tol);
    }
    if !coefficients.iter().all(|c| c.is_finite()) {
        return Roots::new();
    }
    let c = if c4 < 0.0 {
        coefficients.map(|v| -v)
    } else {
        *coefficients
    };
    let (c, j) = balanced(&c);
    let [_, c1, c2, c3, c4] = c;
    // The critical points are the derivative's roots, and the derivative's own critical
    // points are the inflection points, which start the inner searches. `3 c3` is kept
    // exactly (as a pair), so that compensated steps see the true derivative: a cluster
    // of roots has a cluster of critical points, which plain arithmetic cannot resolve.
    let (h3, l3) = two_prod(3.0, c3);
    let derivative = Poly {
        hi: [c1, 2.0 * c2, h3, 4.0 * c4],
        lo: [0.0, 0.0, l3, 0.0],
    };
    let (critical, inflections) = cubic_roots(&derivative, false);
    roots_between(&Poly::exact(c), &critical, true, &inflections, true).scaled(j)
}

/// `c` scaled exactly by powers of two, `2^m p(2^j y)`, so that its roots `y` are near one
/// in size (the lowest and highest nonzero coefficients balanced) and its largest
/// coefficient is in `[1, 2)`; and `j`, by which the roots scale back (`x = 2^j y`). No
/// product of coefficients then overflows or underflows short of extreme spreads, and the
/// scaled polynomial is the same for `2^a p(2^b x)`. Unscaled where a coefficient would
/// leave the normal range.
fn balanced<const N: usize>(c: &[f64; N]) -> ([f64; N], i32) {
    let exponent = |v: f64| ((v.to_bits() >> 52) & 0x7ff) as i32;
    let n = N as i32 - 1;
    let lowest = c.iter().position(|&v| v != 0.0).unwrap_or(0) as i32;
    let (e_low, e_lead) = (
        c.get(lowest as usize).map_or(0, |&v| exponent(v)),
        c.last().map_or(0, |&v| exponent(v)),
    );
    // round to nearest (ties down) of (e_low - e_lead) / (n - lowest)
    let span = n - lowest;
    let j = if span > 0 {
        (2 * (e_low - e_lead) + span).div_euclid(2 * span)
    } else {
        0
    };
    for j in [j, 0] {
        if !(-1022..=1023).contains(&j) {
            continue;
        }
        let top = c
            .iter()
            .zip(0..)
            .filter(|&(&v, _)| v != 0.0)
            .map(|(&v, i)| exponent(v) + j * i)
            .max()
            .unwrap_or(1023);
        let m = 1023 - top;
        let mut out = [0.0; N];
        let mut normal = true;
        for ((o, &v), i) in out.iter_mut().zip(c).zip(0..) {
            if v == 0.0 {
                continue;
            }
            let (e, shift) = (exponent(v), j * i + m);
            if e == 0 || !(1..=2046).contains(&(e + shift)) {
                normal = false;
                break;
            }
            // add to the exponent field: exact for a normal result
            *o = f64::from_bits(v.to_bits().wrapping_add_signed(i64::from(shift) << 52));
        }
        if normal {
            return (out, j);
        }
    }
    (*c, 0)
}

/// A polynomial in ascending powers whose coefficients are `hi[i] + lo[i]` exactly; `lo`
/// holds what rounding a derivative's coefficients drops. Plain evaluation reads `hi`
/// (its error bound allows for `lo`), compensated evaluation both.
#[derive(Clone, Copy)]
struct Poly<const N: usize> {
    hi: [f64; N],
    lo: [f64; N],
}

impl<const N: usize> Poly<N> {
    const fn exact(hi: [f64; N]) -> Self {
        Self { hi, lo: [0.0; N] }
    }

    /// `p(x)` by compensated Horner (Graillat, Langlois and Louvet, 2009), as accurate as
    /// Horner's rule in twice the working precision, and `p'(x)` by plain Horner.
    fn compensated(&self, x: f64) -> (f64, f64) {
        compensated(self.hi.iter().copied().zip(self.lo).rev(), x)
    }

    /// `p'(x)` by compensated Horner on the derivative's coefficients, each exact as a
    /// pair, and `p''(x)` by plain Horner.
    fn compensated_slope(&self, x: f64) -> (f64, f64) {
        let terms = self.hi.iter().copied().zip(self.lo).enumerate().skip(1);
        compensated(
            terms.rev().map(|(i, (h, l))| {
                let k = i as f64;
                let (kh, kl) = two_prod(k, h);
                (kh, kl + k * l)
            }),
            x,
        )
    }
}

/// Compensated Horner over coefficients given as exact pairs `(hi, lo)`, highest power
/// first: the value, and the derivative of the `hi` part by plain Horner.
fn compensated(mut terms: impl Iterator<Item = (f64, f64)>, x: f64) -> (f64, f64) {
    let Some((lead, lead_lo)) = terms.next() else {
        return (0.0, 0.0);
    };
    let (mut s, mut err, mut d) = (lead, lead_lo, 0.0);
    for (a, a_lo) in terms {
        d = d * x + s;
        let (prod, prod_err) = two_prod(s, x);
        let (sum, sum_err) = two_sum(prod, a);
        s = sum;
        err = err * x + (prod_err + sum_err + a_lo);
    }
    (s + err, d)
}

/// The unit roundoff, `2^-53`.
const U: f64 = f64::EPSILON / 2.0;

/// Higham's `γ_k = k u / (1 - k u)`. Horner's rule on a polynomial of degree `n` errs by at
/// most `γ_2n` times `S(x) = Σ |c_i| |x|^i`; one more `u` covers the dropped `lo` parts.
/// Compensated Horner errs by at most `u |p(x)| + γ_2n^2 S(x)`.
const fn gamma<const N: usize>() -> f64 {
    let ku = (2 * N - 1) as f64 * U;
    ku / (1.0 - ku)
}

/// `2^-30`: a plain Newton step this small (relative) leaves the iterate within about
/// `2^-60` of the root (unless the root is ill-conditioned, when the value reaches the
/// error bound first).
const PLAIN_STEP: f64 = 9.313225746154785e-10;

/// `2^-51`: a compensated step of at most two units in the last place, after which the
/// remaining error (the square of the step, times `p'' / 2p'`) is far below one.
const FINE_STEP: f64 = 4.440892098500626e-16;

/// Newton steps per root at most. Every search starts where Newton's method converges
/// monotonically, so this is a guard, not a budget.
const MAX_STEPS: usize = 100;

/// The roots of a cubic with positive leading coefficient, ascending and distinct, and the
/// points that separated them: its critical points (where the derivative's discriminant is
/// positive) or else its inflection point. With `finish`, each root ends in compensated
/// steps (to half a unit in the last place); without it, only an ill-conditioned one does.
fn cubic_roots(c: &Poly<4>, finish: bool) -> (Roots, Roots) {
    let [_, c1, c2, c3] = c.hi;
    let three_c3 = 3.0 * c3;
    let inflection = -c2 / three_c3;
    // p'(x) = 3 c3 x^2 + 2 c2 x + c1, whose discriminant over four is `disc`
    let disc = derivative_discriminant(c1, c2, c3);
    let mut samples = Roots::new();
    let extremum = disc > 0.0;
    if extremum {
        // the stable quadratic formula: no cancellation in either root
        let q = -(c2 + disc.sqrt().copysign(c2));
        let (r1, r2) = (q / three_c3, c1 / q);
        samples.push(r1.min(r2));
        if r1 != r2 {
            samples.push(r1.max(r2));
        }
    } else {
        // monotone: the inflection point tells which side the one root is on
        samples.push(inflection);
    }
    let roots = roots_between(c, &samples, extremum, &[inflection], finish);
    (roots, samples)
}

/// `c2^2 - 3 c3 c1`, in twice the working precision when it cancels badly (a near-triple
/// root), so that its sign is right.
fn derivative_discriminant(c1: f64, c2: f64, c3: f64) -> f64 {
    let (a, b) = (c2 * c2, c3 * c1);
    let disc = a - 3.0 * b;
    // `2^-26`: far above the `3u` relative error of the plain form
    if disc.abs() > 1.4901161193847656e-8 * (a + 3.0 * b.abs()) {
        return disc;
    }
    let (ah, al) = two_prod(c2, c2);
    let (bh, bl) = two_prod(c3, c1);
    let (s1, e1) = two_sum(ah, -bh);
    let (s2, e2) = two_sum(s1, -bh);
    let (s3, e3) = two_sum(s2, -bh);
    s3 + ((e1 + e2 + e3) + (al - 3.0 * bl))
}

/// A sample point of a polynomial: where it is, the sign of the value there (zero when the
/// value cannot be told from zero), and the Taylor coefficients there (the value first,
/// compensated when the sign needed it).
#[derive(Clone, Copy)]
struct Sample<const N: usize> {
    x: f64,
    sign: i8,
    taylor: [f64; N],
}

/// The real roots of `c` (positive leading coefficient), ascending, given the points that
/// separate them: `p` is monotone between consecutive `points` and beyond the outer ones.
/// `extremum` says the points are critical points (so a value near zero may hide a pair of
/// roots); `inflections` are candidate starts for the inner searches.
fn roots_between<const N: usize>(
    c: &Poly<N>,
    points: &[f64],
    extremum: bool,
    inflections: &[f64],
    finish: bool,
) -> Roots {
    let mut samples = [Sample {
        x: 0.0,
        sign: 0,
        taylor: [0.0; N],
    }; 4];
    for (slot, &x) in samples.iter_mut().zip(points) {
        *slot = classify(c, x, extremum);
    }
    let samples = samples.get(..points.len()).unwrap_or(&[]);
    let mut roots = Roots::new();
    let (Some(first), Some(last)) = (samples.first(), samples.last()) else {
        return roots;
    };
    // the sign of p at minus infinity
    let left_sign: i8 = if N.is_multiple_of(2) { -1 } else { 1 };
    if first.sign != 0 && first.sign != left_sign {
        let start = outer_start(first, -1.0);
        roots.push_distinct(newton(
            c,
            f64::NEG_INFINITY,
            first.x,
            first.sign > 0,
            start,
            finish,
        ));
    }
    for (i, s) in samples.iter().enumerate() {
        if s.sign == 0 {
            roots.push_distinct(s.x);
        }
        let Some(next) = samples.get(i + 1) else {
            continue;
        };
        if s.sign * next.sign < 0 {
            let (lo, hi) = (s.x, next.x);
            let start = inflections
                .iter()
                .copied()
                .find(|&t| lo < t && t < hi)
                .unwrap_or(lo + 0.5 * (hi - lo));
            roots.push_distinct(newton(c, lo, hi, s.sign < 0, start, finish));
        }
    }
    if last.sign < 0 {
        let start = outer_start(last, 1.0);
        roots.push_distinct(newton(c, last.x, f64::INFINITY, true, start, finish));
    }
    roots
}

/// The sign of `p` at `x`: plainly when the value clears Horner's error bound, else by
/// compensated Horner. An `extremum` is first moved onto the derivative's root by
/// compensated Newton steps, and the test then also allows for the distance still left to
/// it. A value within the bound is zero: a double root, or a pair of roots or of complex
/// roots too close to tell apart.
fn classify<const N: usize>(c: &Poly<N>, x: f64, extremum: bool) -> Sample<N> {
    let g = gamma::<N>();
    let taylor = taylor(&c.hi, x);
    let value = taylor.first().copied().unwrap_or(0.0);
    let (size, _) = magnitudes(&c.hi, x);
    if value.abs() > g * size {
        return Sample {
            x,
            sign: sign_of(value),
            taylor,
        };
    }
    let mut x = x;
    if extremum {
        for _ in 0..MAX_STEPS {
            let (slope, curvature) = c.compensated_slope(x);
            let next = x - slope / curvature;
            if !next.is_finite() || next == x {
                break;
            }
            let step = (next - x).abs();
            x = next;
            if step <= FINE_STEP * x.abs() {
                break;
            }
        }
    }
    let mut taylor = self::taylor(&c.hi, x);
    let (size, slope_size) = magnitudes(&c.hi, x);
    let (value, _) = c.compensated(x);
    let mut tol = 2.0 * (U * value.abs() + g * g * size);
    if extremum {
        // the critical point is within `delta` of `x`; `p` there differs from `p(x)` by at
        // most the Taylor terms' magnitudes at that distance
        let (slope, _) = c.compensated_slope(x);
        let slope = slope.abs() + 2.0 * (U * slope.abs() + g * g * slope_size);
        let half_curvature = taylor.get(2).copied().unwrap_or(0.0).abs();
        let delta = slope / (2.0 * half_curvature);
        tol += slope * delta;
        let mut power = delta;
        for &t in taylor.iter().skip(2) {
            power *= delta;
            tol += t.abs() * power;
        }
    }
    if let Some(t0) = taylor.first_mut() {
        *t0 = value;
    }
    Sample {
        x,
        // a NaN bound (an infinite distance times a zero term) is no bound: zero
        sign: if value.abs() > tol { sign_of(value) } else { 0 },
        taylor,
    }
}

/// `-1`, `0` or `1`.
fn sign_of(v: f64) -> i8 {
    if v > 0.0 {
        1
    } else if v < 0.0 {
        -1
    } else {
        0
    }
}

/// The Taylor coefficients of `c` at `x` (`p^(k)(x) / k!`), by repeated synthetic division.
fn taylor<const N: usize>(c: &[f64; N], x: f64) -> [f64; N] {
    let mut t = *c;
    for k in 0..N {
        let mut acc = 0.0;
        for v in t.iter_mut().skip(k).rev() {
            acc = acc * x + *v;
            *v = acc;
        }
    }
    t
}

/// `S(x) = Σ |c_i| |x|^i` and `Σ i |c_i| |x|^(i-1)`: the scales of Horner's errors in
/// `p(x)` and `p'(x)`.
fn magnitudes<const N: usize>(c: &[f64; N], x: f64) -> (f64, f64) {
    let ax = x.abs();
    let (mut s, mut d) = (0.0, 0.0);
    for &a in c.iter().rev() {
        d = d * ax + s;
        s = s * ax + a.abs();
    }
    (s, d)
}

/// `p(x)`, `p'(x)` and `S(x)` by plain Horner.
fn horner<const N: usize>(c: &[f64; N], x: f64) -> (f64, f64, f64) {
    let ax = x.abs();
    let (mut p, mut d, mut s) = (0.0, 0.0, 0.0);
    for &a in c.iter().rev() {
        d = d * x + p;
        p = p * x + a;
        s = s * ax + a.abs();
    }
    (p, d, s)
}

/// A start beyond the outermost sample `s` (in direction `dir`), at or past the root, so
/// that Newton's method (the polynomial is convex there) converges monotonically: each
/// Taylor term that pushes `p` towards zero would alone reach it within
/// `(|p(s)| / |t_k|)^(1/k)`, and the root is nearer than the nearest of those.
fn outer_start<const N: usize>(s: &Sample<N>, dir: f64) -> f64 {
    let value = s.taylor.first().copied().unwrap_or(0.0);
    let mut distance = f64::INFINITY;
    let mut dir_k = 1.0;
    for (k, &t) in s.taylor.iter().enumerate().skip(1) {
        dir_k *= dir;
        if t * dir_k * value < 0.0 {
            let r = (value / t).abs();
            let d = match k {
                1 => r,
                2 => r.sqrt(),
                3 => cbrt_estimate(r),
                _ => r.sqrt().sqrt(),
            };
            distance = distance.min(d);
        }
    }
    if !distance.is_finite() {
        // unreachable: the leading term always pushes towards zero beyond the samples
        distance = s.x.abs().max(1.0);
    }
    s.x + dir * distance
}

/// A cube root to within a few percent (Kahan's bit trick), for a start point.
fn cbrt_estimate(r: f64) -> f64 {
    f64::from_bits(r.to_bits() / 3 + 0x2a9f_7893_0000_0000)
}

/// The root of `c` in `(lo, hi)` (one end may be infinite), where `p` rises through the
/// root if `rising`, from `start`: Newton's method, kept inside a bracket that shrinks at
/// each point whose sign is sure (bisecting, or doubling the distance from the finite end,
/// when a step leaves it). Plain steps until the step is small; compensated steps from
/// there with `finish`, and always once the plain value is within its error bound, until
/// the step is within two units in the last place or the compensated value is within its
/// own bound.
fn newton<const N: usize>(
    c: &Poly<N>,
    lo: f64,
    hi: f64,
    rising: bool,
    start: f64,
    finish: bool,
) -> f64 {
    let g = gamma::<N>();
    let (mut a, mut b, mut x) = (lo, hi, start);
    let mut fine = false;
    for _ in 0..MAX_STEPS {
        let (p, d) = if fine {
            let (p, d) = c.compensated(x);
            let (size, _) = magnitudes(&c.hi, x);
            if p.abs() <= 2.0 * (U * p.abs() + g * g * size) {
                break;
            }
            (p, d)
        } else {
            let (p, d, size) = horner(&c.hi, x);
            if p.abs() <= g * size {
                // the sign is noise: on in compensated arithmetic, from the same point
                fine = true;
                continue;
            }
            (p, d)
        };
        if (p > 0.0) == rising {
            b = x;
        } else {
            a = x;
        }
        let mut next = x - p / d;
        if next == x {
            break;
        }
        if !(next > a && next < b) {
            next = if a.is_finite() && b.is_finite() {
                a + 0.5 * (b - a)
            } else if a.is_finite() {
                lo + 2.0 * (x - lo)
            } else {
                hi + 2.0 * (x - hi)
            };
        }
        let step = (next - x).abs();
        x = next;
        if fine {
            if step <= FINE_STEP * x.abs() {
                break;
            }
        } else if step <= PLAIN_STEP * x.abs() {
            if !finish {
                break;
            }
            fine = true;
        }
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use scx_num::math::EFF_ZERO;

    fn sorted(r: &Roots) -> Vec<f64> {
        let mut v = r.to_vec();
        v.sort_by(f64::total_cmp);
        v
    }

    fn close(a: &[f64], b: &[f64], tol: f64) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < tol)
    }

    // prog_test my_test_polynomial (smCurveTest.cpp).
    #[test]
    fn quadratic_upstream_cases() {
        let r = solve_quadratic(&[2.0, 3.0, 1.0], EFF_ZERO);
        assert_eq!(r.len(), 2);
        assert!((r[0] + r[1] + 3.0).abs() < EFF_ZERO);
        assert_eq!(solve_quadratic(&[2.0, 7.0, 6.0], EFF_ZERO).len(), 2);
        let r = solve_quadratic(&[1.0, 2.0, 1.0], EFF_ZERO);
        assert_eq!(r.len(), 1);
        assert!((r[0] + 1.0).abs() < EFF_ZERO);
        assert!(solve_quadratic(&[1.0, -1.0, 1.0], EFF_ZERO).is_empty());
    }

    #[test]
    fn quadratic_degenerate() {
        assert!(solve_quadratic(&[1.0, 0.0, 0.0], EFF_ZERO).is_empty());
        assert_eq!(
            solve_quadratic(&[2.0, 4.0, 0.0], EFF_ZERO).as_slice(),
            &[-0.5]
        );
        assert_eq!(
            solve_quadratic(&[0.0, 0.0, 3.0], EFF_ZERO).as_slice(),
            &[0.0]
        );
        assert_eq!(
            solve_quadratic(&[0.0, 2.0, 1.0], EFF_ZERO).as_slice(),
            &[0.0, -2.0]
        );
        // Slightly negative discriminant above the tolerance but below sqrt(eff zero).
        assert_eq!(solve_quadratic(&[1.0 + 1e-8, 2.0, 1.0], EFF_ZERO).len(), 1);
    }

    #[test]
    fn cubic_upstream_cases() {
        let r = solve_cubic(&[6.0, 11.0, 6.0, 1.0], EFF_ZERO);
        assert!(close(&sorted(&r), &[-3.0, -2.0, -1.0], 1e-9));
        let r = solve_cubic(&[1.0, 3.0, 3.0, 1.0], EFF_ZERO);
        assert!(close(&r, &[-1.0], 1e-9));
        let r = solve_cubic(&[2.0, 5.0, 4.0, 1.0], EFF_ZERO);
        assert!(close(&sorted(&r), &[-2.0, -1.0], 1e-9));
        let r = solve_cubic(&[1.0, 0.0, 0.0, 1.0], EFF_ZERO);
        assert!(close(&r, &[-1.0], 1e-12));
    }

    #[test]
    fn cubic_falls_back_to_quadratic() {
        let r = solve_cubic(&[2.0, 3.0, 1.0, 0.0], EFF_ZERO);
        assert!(close(&sorted(&r), &[-2.0, -1.0], 1e-12));
    }

    #[test]
    fn quartic_upstream_cases() {
        let r = solve_quartic(&[-24.0, -38.0, -13.0, 2.0, 1.0], EFF_ZERO);
        assert!(close(&sorted(&r), &[-3.0, -2.0, -1.0, 4.0], 1e-9));
        let r = solve_quartic(&[1.0, 4.0, 6.0, 4.0, 1.0], EFF_ZERO);
        assert!(close(&r, &[-1.0], 1e-9));
        let r = solve_quartic(&[2.0, 7.0, 9.0, 5.0, 1.0], EFF_ZERO);
        assert_eq!(r.len(), 2);
        let r = solve_quartic(&[1.0, 0.0, -2.0, 0.0, 1.0], EFF_ZERO);
        assert!(close(&sorted(&r), &[-1.0, 1.0], 1e-9));
    }

    #[test]
    fn quartic_degenerate() {
        // Falls back to the cubic, duplicates removed.
        let r = solve_quartic(&[6.0, 11.0, 6.0, 1.0, 0.0], EFF_ZERO);
        assert_eq!(r.len(), 3);
        // x^4 + 1 = 0 has no real roots.
        assert!(solve_quartic(&[1.0, 0.0, 0.0, 0.0, 1.0], EFF_ZERO).is_empty());
        // x^4 - x = 0: r == 0 branch, roots 0 and 1.
        let r = solve_quartic(&[0.0, -1.0, 0.0, 0.0, 1.0], EFF_ZERO);
        assert!(close(&sorted(&r), &[0.0, 1.0], 1e-9));
        // Tolerance above one: resolvent cubic has no root, port uses z = 0.
        let _ = solve_quartic(&[1.0, 2.0, 3.0, 4.0, 5.0], 2.0);
    }
}
