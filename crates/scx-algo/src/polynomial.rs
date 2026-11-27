// Moved from solidcore `solidcore/crates/solidcore/src/geom/polynomial.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/src/SmPolynomial.cpp`,
// `source/SMLib/inc/SmPolynomial.h`.
// Changes: paths only (`acos`, `cbrt`, `sqrt` and the constants from `scx-num`). Results
// unchanged bit for bit.
// Then the cubic and quartic roots gained Newton polishing on the original polynomial.
// Then the cubic and quartic were rewritten (docs/research/71): the closed forms and their
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
//! with an absolute tolerance. The polynomial is first scaled by powers of two so that its
//! roots are near one in size, so the roots of `2^k p(2^j x)` are those of `p` times
//! `2^-j`, bit for bit, while the scaled roots stay normal. Where the coefficients' sizes
//! split the roots into groups far apart (the Newton polygon turns by 128 bits or more),
//! each group is solved from its own part of the coefficients, within the same contract.
//!
//! The roots are bracketed by the critical points (the derivative's roots, from the
//! stable quadratic formula for a cubic and recursively from the cubic for a quartic):
//! between two consecutive critical points `p` is monotone, so a change of sign there
//! holds exactly one root. Each sign is taken from plain Horner when the value clears its
//! error bound, else from compensated Horner. Each root is then found by Newton's method,
//! kept inside the bracket, and finished by compensated Newton steps. The starts come from
//! exact forms solved in plain arithmetic: a cubic with critical points `c1 < c2` is
//! `v1 + (v2 - v1)(3 t^2 - 2 t^3)` at `x = c1 + (c2 - c1) t`, and a quartic between two
//! critical points is that plus `c4 (c2 - c1)^4 t^2 (1 - t)^2`; a monotone cubic is odd
//! about its inflection point but for its value there. A quartic's outer roots start from
//! the quadratic left once its inner roots are divided out, else from a Taylor bound
//! beyond the outermost critical point.
//!
//! The contract, with `S(x) = Σ |c_i| |x|^i` and `η = 2^-90`:
//! - Every real root `r` of the given (double) coefficients, within the double range, has
//!   a returned root within half a unit in the last place of `r`, plus its pseudo-zero
//!   radius `ρ(r) = min_k (k! η S(r) / |p^(k)(r)|)^(1/k)`. For a simple root that is
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
use scx_num::vector3d::{cbrt, sqrt};

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

    /// Every root times `factor`, where the products are known to be normal or zero.
    #[inline]
    fn times(mut self, factor: f64) -> Self {
        let len = self.len;
        for x in self.values.iter_mut().take(len) {
            *x *= factor;
        }
        self
    }

    /// Every root times `2^j`, rounded once; a root past the range is dropped.
    #[cold]
    fn scaled(self, j: i32) -> Self {
        let mut out = Self::new();
        for &x in self.as_slice() {
            let y = scale(x, j);
            if y.is_finite() {
                out.push_distinct(y);
            }
        }
        out
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
    scaled_roots(coefficients, cubic)
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
    scaled_roots(coefficients, quartic_roots)
}

/// The real roots of `c` (finite, the leading coefficient nonzero), ascending and
/// distinct. Degrees one to four.
fn exact_roots(c: &[f64]) -> Roots {
    match *c {
        [c0, c1] => {
            let mut roots = Roots::new();
            let x = if c0 == 0.0 { 0.0 } else { -c0 / c1 };
            if x.is_finite() {
                roots.push(x);
            }
            roots
        }
        [c0, c1, c2] => scaled_roots(&[c0, c1, c2], quadratic_roots),
        [c0, c1, c2, c3] => scaled_roots(&[c0, c1, c2, c3], cubic),
        [c0, c1, c2, c3, c4] => scaled_roots(&[c0, c1, c2, c3, c4], quartic_roots),
        _ => Roots::new(),
    }
}

/// The roots of `c` by `solve` (on the polynomial scaled by [`balanced`], with a positive
/// leading coefficient), or, when the coefficients' sizes split the roots into two groups
/// far apart ([`split`]), the roots of each group's part. The split is looked for only
/// when an end coefficient is far below the largest after balancing, which it must be.
fn scaled_roots<const N: usize>(c: &[f64; N], solve: fn(&[f64; N]) -> Roots) -> Roots {
    // the same roots with a positive leading coefficient (negation is exact)
    let c = if c.last().is_some_and(|&v| v < 0.0) {
        c.map(|v| -v)
    } else {
        *c
    };
    let (b, j, ends) = balanced(&c);
    if ends <= -60
        && let Some(k) = split(&c)
    {
        let (low, high) = (c.get(..=k).unwrap_or(&[]), c.get(k..).unwrap_or(&[]));
        return merged(&exact_roots(low), &exact_roots(high));
    }
    let roots = solve(&b);
    // With both ends of `b` above `2^-60` (its largest coefficient below 2), every root is
    // within `[2^-63, 2^62]` in size (Cauchy's bounds), so a scale back by at most `2^950`
    // gives normal products: exact.
    if ends > -60 && j.abs() <= 950 {
        roots.times(pow2(j))
    } else {
        roots.scaled(j)
    }
}

/// The roots of a quadratic with a positive leading coefficient: its one critical point
/// separates them.
fn quadratic_roots(c: &[f64; 3]) -> Roots {
    let critical = -c[1] / (2.0 * c[2]);
    roots_between(&Poly::exact(*c), &[critical], true, &[], true)
}

/// The roots of a cubic with a positive leading coefficient.
fn cubic(c: &[f64; 4]) -> Roots {
    cubic_roots(&Poly::exact(*c), true).0
}

/// The roots of a quartic with a positive leading coefficient.
fn quartic_roots(c: &[f64; 5]) -> Roots {
    let [_, c1, c2, c3, c4] = *c;
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
    roots_between(&Poly::exact(*c), &critical, true, &inflections, true)
}

/// Two sets of roots, ascending and distinct, merged ascending and distinct.
fn merged(a: &[f64], b: &[f64]) -> Roots {
    let mut all = [0.0; 8];
    let mut n = 0;
    for (slot, &x) in all.iter_mut().zip(a.iter().chain(b)) {
        *slot = x;
        n += 1;
    }
    let all = all.get_mut(..n).unwrap_or(&mut []);
    all.sort_by(f64::total_cmp);
    let mut roots = Roots::new();
    for &x in all.iter() {
        // a signed zero is one root
        roots.push_distinct(x + 0.0);
    }
    roots
}

/// `floor(log2 |v|)` for `v` finite and nonzero, subnormals included.
#[inline]
fn exponent(v: f64) -> i32 {
    let bits = v.to_bits() & !(1 << 63);
    let field = (bits >> 52) as i32;
    if field != 0 {
        field - 1023
    } else {
        bits.ilog2() as i32 - 1074
    }
}

/// `2^k` for `k` in `-1074..=1023`.
fn pow2(k: i32) -> f64 {
    if k >= -1022 {
        f64::from_bits(((k + 1023) as u64) << 52)
    } else {
        f64::from_bits(1 << (k + 1074).max(0))
    }
}

/// `v 2^s`, rounded once: exact unless it is subnormal (or past the range, where it is
/// zero or infinite).
#[inline]
fn scale(v: f64, s: i32) -> f64 {
    let field = ((v.to_bits() >> 52) & 0x7ff) as i32;
    if v == 0.0 {
        return v;
    }
    if field != 0 && (1..=2046).contains(&(field + s)) {
        // add to the exponent field
        return f64::from_bits(v.to_bits().wrapping_add_signed(i64::from(s) << 52));
    }
    scale_far(v, s)
}

/// [`scale`] where the input or the result is not normal.
#[cold]
fn scale_far(v: f64, s: i32) -> f64 {
    // as far as the result stays normal in two exact steps, then the rest at once
    let exact = s.max(-1022 - exponent(v));
    let half = exact / 2;
    let x = v * pow2(half) * pow2(exact - half);
    x * pow2((s - exact).max(-1074))
}

/// `c` scaled by powers of two, `2^m p(2^j y)`, so that its roots `y` are near one in size
/// (the lowest and highest nonzero coefficients balanced) and its largest coefficient is
/// in `[1, 2)`; `j`, by which the roots scale back (`x = 2^j y`); and the lesser of the
/// scaled end coefficients' exponents. The scaled polynomial is the same for
/// `2^a p(2^b x)`. It is exact when the end exponents are at least `-1022`; an interior
/// coefficient may round into the subnormals, which changes `p` by at most
/// `2^-1075 |y|^i`, below `2^-100 S(y)` when the ends are at least `2^-975` (and they are
/// unless [`split`] applies).
fn balanced<const N: usize>(c: &[f64; N]) -> ([f64; N], i32, i32) {
    let n = N as i32 - 1;
    let lowest = c.iter().position(|&v| v != 0.0).unwrap_or(0);
    let span = n - lowest as i32;
    let mut e = [0; N];
    for (ei, &v) in e.iter_mut().zip(c) {
        if v != 0.0 {
            *ei = exponent(v);
        }
    }
    let (e_low, e_lead) = (
        e.get(lowest).copied().unwrap_or(0),
        e.last().copied().unwrap_or(0),
    );
    // round to nearest (ties down) of (e_low - e_lead) / span
    let j = if span > 0 {
        (2 * (e_low - e_lead) + span).div_euclid(2 * span)
    } else {
        0
    };
    let (mut top, mut low, mut least) = (i32::MIN, i32::MAX, i32::MAX);
    for ((&v, &ei), i) in c.iter().zip(&e).zip(0..) {
        if v != 0.0 {
            top = top.max(ei + j * i);
            low = low.min(ei + j * i);
            least = least.min(ei);
        }
    }
    let m = -top;
    let mut out = *c;
    if least >= -1022 && low + m >= -1022 {
        // every input and result normal: add to the exponent fields, exactly
        for (o, i) in out.iter_mut().zip(0..) {
            let shift = if *o == 0.0 {
                0
            } else {
                i64::from(m + j * i) << 52
            };
            *o = f64::from_bits(o.to_bits().wrapping_add_signed(shift));
        }
    } else {
        for (o, i) in out.iter_mut().zip(0..) {
            *o = scale(*o, m + j * i);
        }
    }
    let ends = (e_low + m + j * lowest as i32).min(e_lead + m + j * n);
    (out, j, ends)
}

/// An interior index `k` where the Newton polygon of `c` (the upper hull of the points
/// `(i, log2 |c_i|)`) turns by at least 128 bits: every slope from a lower point into
/// `(k, log2 |c_k|)` exceeds every slope out of it to a higher point by that much. Then
/// `p` has `k` roots near those of `c_0 + ... + c_k x^k` and the rest near those of
/// `c_k + ... + c_n x^(n - k)`: at the roots of either part, the other part's terms are
/// below `2^-120` of the part's own `S`, far inside the contract's `η`. (Exponents are
/// `floor(log2)`; their error is a bit each, inside the margin.)
fn split(c: &[f64]) -> Option<usize> {
    let n = c.len().checked_sub(1)?;
    let e = |i: usize| match c.get(i) {
        Some(&v) if v != 0.0 => Some(i64::from(exponent(v))),
        _ => None,
    };
    (1..n).find(|&k| {
        let Some(ek) = e(k) else {
            return false;
        };
        let lower = (0..k).filter_map(|i| e(i).map(|ei| (i, ei)));
        let mut any = false;
        let all = lower.into_iter().all(|(i, ei)| {
            any = true;
            (k + 1..=n)
                .filter_map(|j| e(j).map(|ej| (j, ej)))
                .all(|(j, ej)| {
                    let (a, b) = ((k - i) as i64, (j - k) as i64);
                    (ek - ei) * b + (ek - ej) * a >= 128 * a * b
                })
        });
        any && all
    })
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
    /// Horner's rule in twice the working precision; `p'(x)` by plain Horner; and `S(x)`.
    fn compensated(&self, x: f64) -> (f64, f64, f64) {
        // In three passes, the same operations as one: the sums, then the products'
        // errors, then the errors' sum. Where a product's error is a library call (x86-64's
        // `fma`), nothing else is live across it.
        let ax = x.abs();
        let (Some((&lead, rest)), Some((&lo_lead, lo_rest))) =
            (self.hi.split_last(), self.lo.split_last())
        else {
            return (0.0, 0.0, 0.0);
        };
        let (mut s, mut d, mut size) = (lead, 0.0, lead.abs());
        // per coefficient below the lead: the product's factor (then its error) and the
        // sum's error
        let mut errs = [(0.0, 0.0); N];
        for (e, &a) in errs.iter_mut().zip(rest).rev() {
            d = d * x + s;
            size = size * ax + a.abs();
            let (sum, sum_err) = two_sum(s * x, a);
            *e = (s, sum_err);
            s = sum;
        }
        for (e, _) in errs.iter_mut().zip(rest) {
            e.0 = two_prod(e.0, x).1;
        }
        let mut err = lo_lead;
        for (&(prod_err, sum_err), &lo) in errs.iter().zip(lo_rest).rev() {
            err = err * x + (prod_err + sum_err + lo);
        }
        (s + err, d, size)
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

/// Newton steps per root at most, from near the root. A guard, not a budget.
const MAX_STEPS: usize = 100;

/// Plain Newton steps per root at most, from its start. A start can be far from the
/// root, past a group of `k` others around a point `z` (the coefficients' sizes put
/// groups up to `2^128` apart without a [`split`]): there `p ~ (x - z)^k` and each step
/// closes only `1 / k` of the way to `z` (at least `0.41` bits for `k <= 4`), and the
/// distance can span the doubles' range (`2^2098`); bisection of a bracket that wide
/// takes as many halvings. So a guard, again, that does not end a search still making
/// progress.
const PLAIN_STEPS: usize = 8192;

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
/// value cannot be told from zero), and the value (compensated when the sign needed it).
#[derive(Clone, Copy)]
struct Sample {
    x: f64,
    sign: i8,
    value: f64,
}

impl Sample {
    /// The Taylor coefficients of `c` here, the value first (as sampled).
    fn taylor<const N: usize>(&self, c: &[f64; N]) -> [f64; N] {
        let mut t = taylor(c, self.x);
        if let Some(t0) = t.first_mut() {
            *t0 = self.value;
        }
        t
    }
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
        value: 0.0,
    }; 4];
    for (slot, &x) in samples.iter_mut().zip(points) {
        *slot = classify(c, x, extremum);
    }
    let samples = samples.get(..points.len()).unwrap_or(&[]);
    let (Some(first), Some(last)) = (samples.first(), samples.last()) else {
        return Roots::new();
    };
    // A cubic with critical points `c1 < c2`, where it is `v1` and `v2`, is exactly
    // `v1 + (v2 - v1) s(t)` at `x = c1 + (c2 - c1) t`, with `s(t) = 3 t^2 - 2 t^3`; so its
    // roots are those of `s` in plain arithmetic, which start every search. Between a
    // quartic's critical points the same form is a close model.
    let value = |s: &Sample| s.value;
    let mut inner = [None; 4];
    // the model's inner root, for a cubic: with it, the outer two are a quadratic's
    let mut inner_t = None;
    for ((slot, s), next) in inner.iter_mut().zip(samples).zip(samples.iter().skip(1)) {
        if s.sign * next.sign < 0 {
            let (lo, hi) = (s.x, next.x);
            let start = if extremum {
                let (v1, v2) = (value(s), value(next));
                let h = next.x - s.x;
                // the model's root as a fraction `t` of `h` from the nearer end (`flip`:
                // `next`), so that a root close to that end keeps its precision
                let (t, flip) = smooth_inner(v1, v2);
                inner_t = Some((t, flip));
                // a quartic is the cubic form plus `c4 h^4 t^2 (1 - t)^2`, which is the
                // same from either end
                let t = match c.hi.get(4) {
                    Some(&c4) if N == 5 => {
                        let (near, far) = if flip { (v2, v1) } else { (v1, v2) };
                        quartic_inner(t, near, far, c4 * (h * h) * (h * h))
                    }
                    _ => t,
                };
                if flip { next.x - h * t } else { s.x + h * t }
            } else {
                inflections
                    .iter()
                    .copied()
                    .find(|&t| lo < t && t < hi)
                    .unwrap_or(lo + 0.5 * (hi - lo))
            };
            let start = if lo < start && start < hi {
                start
            } else {
                lo + 0.5 * (hi - lo)
            };
            *slot = Some(newton(c, lo, hi, s.sign < 0, start, finish, None));
        }
    }
    // the sign of p at minus infinity
    let left_sign: i8 = if N.is_multiple_of(2) { -1 } else { 1 };
    let left = first.sign != 0 && first.sign != left_sign;
    let right = last.sign < 0;
    // The outer roots: the cubic's from its form above (or, monotone, from its Taylor
    // form at the inflection point); the quartic's, when the inner roots leave a
    // quadratic, from that quadratic's roots (in plain arithmetic); else from the start
    // beyond the sample.
    let mut outer = [None, None];
    if left || right {
        let cubic = N == 4 && samples.iter().all(|s| s.sign != 0);
        let known = inner.iter().flatten().count();
        let simple = samples.iter().all(|s| s.sign != 0);
        let quadratic = if !cubic && simple && known + 3 == N {
            deflated_roots(&c.hi, inner.iter().flatten().copied())
        } else {
            None
        };
        let pick = |lo: f64, hi: f64, start: Option<f64>, fallback: &dyn Fn() -> f64| {
            let inside = |t: f64| lo < t && t < hi;
            match (start, quadratic) {
                (Some(t), _) if inside(t) => t,
                (_, Some((a, _))) if inside(a) => a,
                (_, Some((_, b))) if inside(b) => b,
                _ => fallback(),
            }
        };
        // each side's model start, taken only for a side that has a root
        let outer_model = |left: bool| match (cubic, samples) {
            (true, [s, next]) => {
                let (v1, v2) = (value(s), value(next));
                let w = v1 - v2;
                let h = next.x - s.x;
                // how far beyond its end each outer root is, in units of `h`: from the
                // inner root when there is one (from the nearer end, exact enough), else
                // by Newton's method
                let u = match inner_t {
                    Some((t, flip)) => {
                        let (from_s, from_next) = if flip { (1.0 - t, t) } else { (t, 1.0 - t) };
                        if left {
                            outer_beyond(v1 / w, from_s)
                        } else {
                            outer_beyond(-v2 / w, from_next)
                        }
                    }
                    None => smooth_outer(if left { v1 / w } else { -v2 / w }),
                };
                Some(if left { s.x - h * u } else { next.x + h * u })
            }
            (true, [s]) => monotone_start(s.x, &s.taylor(&c.hi)),
            _ => None,
        };
        if left {
            let beyond = || outer_start(first.x, &first.taylor(&c.hi), -1.0);
            let start = pick(f64::NEG_INFINITY, first.x, outer_model(true), &beyond);
            outer[0] = Some(newton(
                c,
                f64::NEG_INFINITY,
                first.x,
                first.sign > 0,
                start,
                finish,
                Some(&beyond),
            ));
        }
        if right {
            let beyond = || outer_start(last.x, &last.taylor(&c.hi), 1.0);
            let start = pick(last.x, f64::INFINITY, outer_model(false), &beyond);
            outer[1] = Some(newton(
                c,
                last.x,
                f64::INFINITY,
                true,
                start,
                finish,
                Some(&beyond),
            ));
        }
    }
    let mut roots = Roots::new();
    let [left_root, right_root] = outer;
    if let Some(x) = left_root {
        roots.push_distinct(x);
    }
    for (s, root) in samples.iter().zip(inner) {
        if s.sign == 0 {
            roots.push_distinct(s.x);
        }
        if let Some(x) = root {
            roots.push_distinct(x);
        }
    }
    if let Some(x) = right_root {
        roots.push_distinct(x);
    }
    roots
}

/// The inner root of the model cubic, `t` in `(0, 1)` with
/// `s(t) = 3 t^2 - 2 t^3 = v1 / (v1 - v2)` (`v1`, `v2` of opposite signs), in plain
/// arithmetic: solved on the half nearer `v1`'s or `v2`'s end, where `s` is convex, from
/// below, and given as the fraction of the way from that end, in `(0, 1/2]`, with whether
/// it is `v2`'s.
fn smooth_inner(v1: f64, v2: f64) -> (f64, bool) {
    let w = v1 - v2;
    let (q, flip) = if v1.abs() <= v2.abs() {
        (v1 / w, false)
    } else {
        (-v2 / w, true)
    };
    // `3 t^2 - 2 t^3 = q` on `(0, 1/2]`: one fixed-point step from `sqrt(q / 3)`, then Newton
    let t = (q / 3.0).sqrt();
    let t = scalar_newton(((q + 2.0 * t * t * t) / 3.0).sqrt(), |t| {
        (t * t * (3.0 - 2.0 * t) - q, 6.0 * t * (1.0 - t))
    });
    (t, flip)
}

/// `u > 0` with `3 u^2 + 2 u^3 = q` (the model cubic's outer root beyond the end where
/// it is `q` of the way to the other end's value), given the inner root `t` measured from
/// the same end: the model's three roots in `-u`, `t` and the third sum to `3/2` with
/// product `-q / 2`, so `u^2 + (3/2 - t) u - q / (2 t) = 0`, solved without cancellation.
fn outer_beyond(q: f64, t: f64) -> f64 {
    let (sigma, p) = (1.5 - t, q / (2.0 * t));
    2.0 * p / (sigma + (sigma * sigma + 4.0 * p).sqrt())
}

/// `t` in `(0, 1)` with `v1 + (v2 - v1) s(t) + k t^2 (1 - t)^2 = 0`, by Newton's method in
/// plain arithmetic from `t`: a quartic between its critical points, exactly, where
/// `k = c4 h^4`. The cubic form's root `t` otherwise.
fn quartic_inner(t: f64, v1: f64, v2: f64, k: f64) -> f64 {
    let w = v2 - v1;
    let r = scalar_newton(t, |t| {
        let m = t * (1.0 - t);
        (
            v1 + w * t * t * (3.0 - 2.0 * t) + k * m * m,
            6.0 * w * m + 2.0 * k * m * (1.0 - 2.0 * t),
        )
    });
    if r > 0.0 && r < 1.0 { r } else { t }
}

/// `u > 0` with `3 u^2 + 2 u^3 = q` (`q > 0`), in plain arithmetic: how far the model
/// cubic's outer root lies beyond a critical point, in units of their distance.
fn smooth_outer(q: f64) -> f64 {
    let u = (q / 3.0).sqrt().min(cbrt_estimate(0.5 * q));
    scalar_newton(u, |u| (u * u * (3.0 + 2.0 * u) - q, 6.0 * u * (1.0 + u)))
}

/// The root of a monotone cubic from its Taylor form `taylor` at the inflection point `x`,
/// `v + t1 d + t3 d^3` (no `d^2` term): `d = k u` with `k = sqrt(t1 / t3)` and
/// `u + u^3 = -v / (t1 k)`, in plain arithmetic.
fn monotone_start<const N: usize>(x: f64, taylor: &[f64; N]) -> Option<f64> {
    let (&v, &t1, &t3) = (taylor.first()?, taylor.get(1)?, taylor.get(3)?);
    if !(t1 > 0.0 && t3 > 0.0) {
        return Some(x + cbrt_estimate((v / t3).abs()).copysign(-v));
    }
    let k = (t1 / t3).sqrt();
    let q = (v / (t1 * k)).abs();
    let u = scalar_newton(q.min(cbrt_estimate(q)), |u| {
        (u * (1.0 + u * u) - q, 1.0 + 3.0 * u * u)
    });
    Some(x + (k * u).copysign(-v))
}

/// Newton's method on a scalar function `f` (value and slope) from `t`, until the step is
/// below `2^-30` of `t`, in at most eight steps.
fn scalar_newton(mut t: f64, f: impl Fn(f64) -> (f64, f64)) -> f64 {
    for _ in 0..8 {
        let (v, d) = f(t);
        let step = v / d;
        t -= step;
        // a NaN step stops too
        if step.is_nan() || step.abs() <= PLAIN_STEP * t.abs() {
            break;
        }
    }
    t
}

/// The roots of `c` divided by `x - r` for each of `found` (synthetic division in plain
/// arithmetic), when that leaves a quadratic with real roots: starts for its other roots.
fn deflated_roots<const N: usize>(
    c: &[f64; N],
    found: impl Iterator<Item = f64>,
) -> Option<(f64, f64)> {
    let mut q = *c;
    let mut degree = N - 1;
    for r in found {
        // the quotient's coefficients replace the lowest (zeros above stay zero); the
        // remainder is dropped
        let mut acc = 0.0;
        for v in q.iter_mut().rev() {
            let next = *v + r * acc;
            *v = acc;
            acc = next;
        }
        degree -= 1;
    }
    let (Some(&q0), Some(&q1), Some(&q2), 2) = (q.first(), q.get(1), q.get(2), degree) else {
        return None;
    };
    let disc = q1 * q1 - 4.0 * q2 * q0;
    if disc.is_nan() || disc < 0.0 {
        return None;
    }
    let t = -0.5 * (q1 + disc.sqrt().copysign(q1));
    Some((t / q2, q0 / t))
}

/// The sign of `p` at `x`: plainly when the value clears Horner's error bound, else by
/// compensated Horner. An `extremum` is first moved onto the derivative's root by
/// compensated Newton steps, and the test then also allows for the distance still left to
/// it. A value within the bound is zero: a double root, or a pair of roots or of complex
/// roots too close to tell apart.
fn classify<const N: usize>(c: &Poly<N>, x: f64, extremum: bool) -> Sample {
    let g = gamma::<N>();
    let (value, _, size) = horner(&c.hi, x);
    if value.abs() > g * size {
        return Sample {
            x,
            sign: sign_of(value),
            value,
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
    let taylor = taylor(&c.hi, x);
    let (size, slope_size) = magnitudes(&c.hi, x);
    let (value, _, _) = c.compensated(x);
    let mut tol = 2.0 * (U * value.abs() + g * g * size);
    if extremum {
        // The derivative (degree `m`) has a root within `delta` of `x`: for each `j`, one
        // within `(C(m, j) |p'(x)| / |p'^(j)(x) / j!|)^(1/j)`, where `p'^(j)(x) / j!` is
        // `(j + 1) t_(j+1)`, taken at its least after the Taylor coefficient's error
        // (`2 γ` of the same sums over `|c_i|` at `|x|`). `p` there differs from `p(x)` by
        // at most the Taylor terms' magnitudes at that distance.
        let (slope, _) = c.compensated_slope(x);
        let slope = slope.abs() + 2.0 * (U * slope.abs() + g * g * slope_size);
        let scales = self::taylor(&c.hi.map(f64::abs), x.abs());
        let m = N.saturating_sub(2);
        let (mut delta, mut binom) = (f64::INFINITY, 1.0);
        for j in 1..=m {
            binom = binom * (m + 1 - j) as f64 / j as f64;
            let (Some(&t), Some(&e)) = (taylor.get(j + 1), scales.get(j + 1)) else {
                break;
            };
            let least = (j + 1) as f64 * (t.abs() - 2.0 * g * e);
            if least > 0.0 {
                let r = binom * slope / least;
                let d = match j {
                    1 => r,
                    2 => r.sqrt(),
                    _ => cbrt(r),
                };
                delta = delta.min(d * (1.0 + 8.0 * U));
            }
        }
        tol += slope * delta;
        let mut power = delta;
        for (&t, &e) in taylor.iter().zip(&scales).skip(2) {
            power *= delta;
            tol += (t.abs() + 2.0 * g * e) * power;
        }
    }
    Sample {
        x,
        // a NaN bound (an infinite distance times a zero term) is no bound: zero
        sign: if value.abs() > tol { sign_of(value) } else { 0 },
        value,
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

/// A start beyond the outermost sample `x` (in direction `dir`), where the Taylor
/// coefficients are `taylor`, at or past the root, so that Newton's method (the polynomial
/// is convex there) converges monotonically: each Taylor term that pushes `p` towards zero
/// would alone reach it within `(|p(x)| / |t_k|)^(1/k)`, and the root is nearer than the
/// nearest of those.
fn outer_start<const N: usize>(x: f64, taylor: &[f64; N], dir: f64) -> f64 {
    let value = taylor.first().copied().unwrap_or(0.0);
    let mut distance = f64::INFINITY;
    let mut dir_k = 1.0;
    for (k, &t) in taylor.iter().enumerate().skip(1) {
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
        distance = x.abs().max(1.0);
    }
    x + dir * distance
}

/// A cube root to within a few percent (Kahan's bit trick), for a start point.
fn cbrt_estimate(r: f64) -> f64 {
    f64::from_bits(r.to_bits() / 3 + 0x2a9f_7893_0000_0000)
}

/// The root of `c` in `(lo, hi)` (one end may be infinite), where `p` rises through the
/// root if `rising`, from `start`: Newton's method, kept inside a bracket (see
/// [`Bracket::step`]). Plain steps until the step is small; then, with `finish`,
/// compensated ones ([`polish`]). A plain value within its error bound also goes on to
/// compensated steps, unless the root is well conditioned and `finish` is not asked.
///
/// An outer search (one end infinite) takes `beyond`, a start past the root (as a rule):
/// when `start` turns out short of the root (its sign sure) and its Newton step lands
/// further out than `beyond`, the search goes on from `beyond` instead. From short of an
/// outer root, near the critical point where the slope vanishes, a Newton step can land
/// far past the root, where the steps back are each only a fraction `1 / n` of the way.
fn newton<const N: usize>(
    c: &Poly<N>,
    lo: f64,
    hi: f64,
    rising: bool,
    start: f64,
    finish: bool,
    mut beyond: Option<&dyn Fn() -> f64>,
) -> f64 {
    let g = gamma::<N>();
    let mut bracket = Bracket {
        lo,
        hi,
        a: lo,
        b: hi,
        rising,
    };
    let mut x = start;
    for _ in 0..PLAIN_STEPS {
        let (p, d, size) = horner(&c.hi, x);
        if p.abs() <= g * size {
            // The sign is noise. Without `finish`, a root this well conditioned (the
            // noise's reach, `γ S / |p'|`, below a plain step) is close enough already.
            if !finish && g * size <= PLAIN_STEP * x.abs() * d.abs() {
                return x;
            }
            return polish(c, bracket, x);
        }
        let Some((mut next, mut step)) = bracket.step(x, p, d) else {
            break;
        };
        if let Some(beyond) = beyond.take() {
            // short of the root (`a`, rightwards, or `b`, leftwards, moved to `x`), and
            // stepping further than `x` is from the sample, the search's finite end
            let (short, end) = if hi.is_infinite() {
                (bracket.a == x, lo)
            } else {
                (bracket.b == x, hi)
            };
            if short && step > (x - end).abs() {
                let far = beyond();
                if (hi.is_infinite() && x < far && far < next)
                    || (lo.is_infinite() && next < far && far < x)
                {
                    (next, step) = (far, (far - x).abs());
                }
            }
        }
        x = next;
        if step <= PLAIN_STEP * x.abs() {
            break;
        }
    }
    if finish { polish(c, bracket, x) } else { x }
}

/// Compensated Newton steps from `x` within `bracket`, until the step is within two units
/// in the last place or the value is within its error bound. Not inlined: the products'
/// calls (x86-64's `fma`) would otherwise spill the plain steps' registers too.
#[inline(never)]
fn polish<const N: usize>(c: &Poly<N>, mut bracket: Bracket, mut x: f64) -> f64 {
    let g = gamma::<N>();
    for _ in 0..MAX_STEPS {
        let (p, d, size) = c.compensated(x);
        if p.abs() <= 2.0 * (U * p.abs() + g * g * size) {
            break;
        }
        let Some((next, step)) = bracket.step(x, p, d) else {
            break;
        };
        x = next;
        if step <= FINE_STEP * x.abs() {
            break;
        }
    }
    x
}

/// A Newton search's bracket: the root is in `(a, b)`, within the search's `(lo, hi)`
/// (one end may be infinite), and `p` rises through it if `rising`.
#[derive(Clone, Copy)]
struct Bracket {
    lo: f64,
    hi: f64,
    a: f64,
    b: f64,
    rising: bool,
}

impl Bracket {
    /// The bracket shrunk to `x` (where `p` is `p`, its sign sure) and the next point with
    /// the step's length: Newton's from `x`, or, when that leaves the bracket, its midpoint
    /// (or, with an infinite end, twice the distance from the finite one). `None` when the
    /// step does not move `x`.
    fn step(&mut self, x: f64, p: f64, d: f64) -> Option<(f64, f64)> {
        if (p > 0.0) == self.rising {
            self.b = x;
        } else {
            self.a = x;
        }
        let mut next = x - p / d;
        if next == x {
            return None;
        }
        if !(next > self.a && next < self.b) {
            next = if self.a.is_finite() && self.b.is_finite() {
                self.a + 0.5 * (self.b - self.a)
            } else if self.a.is_finite() {
                self.lo + 2.0 * (x - self.lo)
            } else {
                self.hi + 2.0 * (x - self.hi)
            };
        }
        Some((next, (next - x).abs()))
    }
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

    #[test]
    fn extreme_coefficients() {
        // subnormal coefficients: (x + 1)(x^2 + 1) times the least subnormal
        let tiny = f64::from_bits(1);
        assert_eq!(solve_cubic(&[tiny; 4], 0.0).as_slice(), &[-1.0]);
        // roots far apart: one near -2.5 / MAX (subnormal), two near ±sqrt(MAX / 1234567),
        // each from its own part of the coefficients
        let r = solve_cubic(&[-2.5, -f64::MAX, 2e-5, 1_234_567.0], 0.0);
        assert_eq!(r.len(), 3);
        assert_eq!(r[1], -2.5 / f64::MAX);
        let big = (f64::MAX / 1_234_567.0).sqrt();
        assert!(close(&[r[0], r[2]], &[-big, big], 1e-6 * big));
        // x^4 = 2^1074: roots ±2^268.5, past the range of products of the coefficients
        let r = solve_quartic(&[-1.0, 0.0, 0.0, 0.0, tiny], 0.0);
        let x = f64::from_bits((1023 + 268) << 52) * core::f64::consts::SQRT_2;
        assert!(close(&r, &[-x, x], 1e-15 * x), "{r:?}");
        // roots past the double range are dropped: of 1 - MAX x^2 + 2^-1074 x^4, the
        // pair near ±2^1049
        let r = solve_quartic(&[1.0, 0.0, -f64::MAX, 0.0, tiny], 0.0);
        let x = 1.0 / f64::MAX.sqrt();
        assert!(close(&r, &[-x, x], 1e-15 * x), "{r:?}");
    }

    /// `r` is `expected`, root for root, to `1e-15` relative.
    fn assert_roots(r: &Roots, expected: &[f64]) {
        assert_eq!(r.len(), expected.len(), "{r:?}");
        for (&x, &e) in r.iter().zip(expected) {
            assert!((x - e).abs() <= 1e-15 * e.abs(), "{r:?}");
        }
    }

    // Groups of roots far apart (by less than a split): once found every root's
    // search ran out of steps first, returning a point short of the root or none.
    #[test]
    fn roots_in_groups_far_apart() {
        // from short of the outer roots, by the critical points, Newton's first step
        // lands far past them
        let r = solve_cubic(
            &[
                9.038942794798868e26,
                -3.7147613634085717e-13,
                -1.3042067502162137e48,
                -2.2972721273366368e42,
            ],
            0.0,
        );
        assert_roots(
            &r,
            &[
                -567_719.746_692_899_4,
                -2.632_604_365_318_986_4e-11,
                2.632_604_365_318_986e-11,
            ],
        );
        // the derivative's roots near zero, from the model's root nearer its end
        let r = solve_quartic(
            &[
                -2.4201697088465876e-19,
                11_597_494_302_572.11,
                -4.8463532864414455e-60,
                -1.3132552959826555e18,
                -1.3478078370677313e-14,
            ],
            0.0,
        );
        assert_roots(
            &r,
            &[
                -9.743638965920781e31,
                -0.002_971_717_374_894_825_7,
                2.08680396446484e-32,
                0.002_971_717_374_894_825_7,
            ],
        );
        // from the critical point by -1.3e49, past three roots' group by zero: each
        // step closes only a third of the way
        let r = solve_quartic(
            &[
                -3.1712278131862046e48,
                1.9694060740748343e-30,
                -2.840282542925966e-57,
                60_371_202_422_694.2,
                3.5742165121151144e-36,
            ],
            0.0,
        );
        assert_roots(&r, &[-1.6890751362728257e49, 374_512_115_718.136_17]);
        let r = solve_quartic(
            &[
                -3.2635846068652265e43,
                2.9636090803364664e-10,
                -2.6838777828702883e-49,
                2.0237998505865052e38,
                169_835_459.563_933_2,
            ],
            0.0,
        );
        assert_roots(&r, &[-1.1916238551023333e30, 54.430_514_422_102_76]);
    }
}
