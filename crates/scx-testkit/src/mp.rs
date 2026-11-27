// Written for solidcorex. Apache-2.0.
//! Correctly rounded references for the elementary functions, from interval arithmetic on
//! dyadic numbers of any precision (Ziv's strategy). This is more precise than [`crate::dd`]
//! (about 104 bits) and needed where that is not enough: at the hardest-to-round inputs
//! (CORE-MATH's worst cases, `scx-num`'s `tests/fmath_worst.rs`), where the true value
//! lies within `2^-120` relative or less of a halfway point between two doubles.
//!
//! Every value is an interval `[lo, hi]` of dyadics `m 2^e` with `p`-bit `m`, each end
//! rounded outward, so the interval holds the true value. Series are cut where the terms
//! fall below `2^-p` of the sum, and the bound of the rest is added to the interval
//! (`tail`). Constants (pi, `ln 2`) are computed once at [`MAX_BITS`] bits and rounded
//! outward to the precision asked for.
//!
//! [`correctly_rounded`] evaluates at 128 bits, then 256, and so on to 4096, until both
//! ends of the interval round to the same double (round to nearest, ties to even): every
//! value between them then rounds to it too, so it is the correctly rounded result. The
//! only inputs it cannot decide are those whose true value is exactly halfway between two
//! doubles; those are known for the algebraic functions and the powers `x^(n / 2^k)`, which are
//! decided exactly instead ([`cbrt`], [`hypot`], [`pow`]), and do not occur for the others
//! (Lindemann: `e^x`, `ln x`, `sin x`, ... of a nonzero rational are transcendental).
//!
//! No result depends on `fmath`.

use core::cmp::Ordering;
use std::sync::OnceLock;

use crate::exact::Int;
use crate::rat::Rat;

/// The precision of the cached constants, and the most any evaluation may ask for.
pub const MAX_BITS: u64 = 8192;

/// The precisions [`correctly_rounded`] tries, in order.
const LADDER: [u64; 6] = [128, 256, 512, 1024, 2048, 4096];

// ---------------------------------------------------------------- dyadics

/// A dyadic number `m 2^e`.
#[derive(Clone, Debug, Default)]
pub struct Dy {
    m: Int,
    e: i64,
}

impl Dy {
    /// `n` exactly.
    fn int(n: i64) -> Dy {
        Dy {
            m: Int::from(n),
            e: 0,
        }
    }

    /// The finite `x` exactly.
    pub fn of_f64(x: f64) -> Dy {
        let ([m], e) = Int::of_doubles([x]);
        Dy { m, e }
    }

    fn sign(&self) -> i32 {
        self.m.signum()
    }

    fn is_zero(&self) -> bool {
        self.m.is_zero()
    }

    fn neg(&self) -> Dy {
        Dy {
            m: self.m.neg(),
            e: self.e,
        }
    }

    fn abs(&self) -> Dy {
        Dy {
            m: self.m.abs(),
            e: self.e,
        }
    }

    /// `self 2^k`, exactly.
    fn scale(&self, k: i64) -> Dy {
        Dy {
            m: self.m.clone(),
            e: self.e + k,
        }
    }

    fn add(&self, o: &Dy) -> Dy {
        if self.is_zero() {
            return o.clone();
        }
        if o.is_zero() {
            return self.clone();
        }
        let e = self.e.min(o.e);
        let a = self.m.shl((self.e - e) as u64);
        let b = o.m.shl((o.e - e) as u64);
        Dy { m: a.add(&b), e }
    }

    fn sub(&self, o: &Dy) -> Dy {
        self.add(&o.neg())
    }

    fn mul(&self, o: &Dy) -> Dy {
        Dy {
            m: self.m.mul(&o.m),
            e: self.e + o.e,
        }
    }

    fn cmp(&self, o: &Dy) -> Ordering {
        self.sub(o).sign().cmp(&0)
    }

    /// `floor(log2 |self|)`; a large negative number for zero.
    fn mag(&self) -> i64 {
        if self.is_zero() {
            return i64::MIN / 4;
        }
        self.m.bit_len() as i64 - 1 + self.e
    }

    /// Rounded to at most `p` bits, down or `up`.
    fn round(&self, p: u64, up: bool) -> Dy {
        let n = self.m.bit_len();
        if n <= p {
            return self.clone();
        }
        let s = n - p;
        let q = self.m.shr(s);
        let exact = q.shl(s) == self.m;
        // `shr` rounds toward zero: down for a positive value, up for a negative one
        let away = !exact && (up != self.m.is_negative());
        let q = if away {
            let one = Int::from(if self.m.is_negative() { -1i64 } else { 1 });
            q.add(&one)
        } else {
            q
        };
        Dy {
            m: q,
            e: self.e + s as i64,
        }
    }

    /// `self / d` for a positive integer `d`, rounded to `p` bits down or `up`.
    fn div_int(&self, d: &Int, p: u64, up: bool) -> Dy {
        if self.is_zero() {
            return Dy::default();
        }
        let s = (p + d.bit_len() + 2).saturating_sub(self.m.bit_len());
        let n = self.m.shl(s);
        let Some((q, r)) = n.div_rem(d) else {
            return Dy::default();
        };
        let q = if r.is_zero() {
            q
        } else if up && !n.is_negative() {
            q.add(&Int::from(1i64))
        } else if !up && n.is_negative() {
            q.sub(&Int::from(1i64))
        } else {
            q
        };
        Dy {
            m: q,
            e: self.e - s as i64,
        }
        .round(p, up)
    }

    /// `1 / self`, rounded to `p` bits down or `up`; zero for zero.
    fn recip(&self, p: u64, up: bool) -> Dy {
        if self.is_zero() {
            return Dy::default();
        }
        let one = Dy {
            m: Int::from(1i64),
            e: -self.e,
        };
        if self.m.is_negative() {
            // 1 / (-a) = -(1 / a), the direction reversed
            one.div_int(&self.m.abs(), p, !up).neg()
        } else {
            one.div_int(&self.m, p, up)
        }
    }

    /// `sqrt(self)` for `self >= 0`, rounded to `p` bits down or `up`.
    fn sqrt(&self, p: u64, up: bool) -> Dy {
        if self.sign() <= 0 {
            return Dy::default();
        }
        let mut s = (2 * p + 2).saturating_sub(self.m.bit_len());
        if (self.e - s as i64).rem_euclid(2) != 0 {
            s += 1;
        }
        let n = self.m.shl(s);
        let Some(r) = n.isqrt() else {
            return Dy::default();
        };
        let r = if up && r.mul(&r) != n {
            r.add(&Int::from(1i64))
        } else {
            r
        };
        Dy {
            m: r,
            e: (self.e - s as i64) / 2,
        }
        .round(p, up)
    }

    /// `floor(self)`.
    fn floor(&self) -> Int {
        if self.e >= 0 {
            return self.m.shl(self.e as u64);
        }
        let s = self.e.unsigned_abs();
        let q = self.m.shr(s);
        if self.m.is_negative() && q.shl(s) != self.m {
            q.sub(&Int::from(1i64))
        } else {
            q
        }
    }

    /// The nearest double, ties to even (infinite past the doubles, zero below).
    pub fn to_f64(&self) -> f64 {
        Rat::from_int(&self.m, self.e).to_f64()
    }
}

// ---------------------------------------------------------------- intervals

/// An interval `[lo, hi]` of dyadics, holding a true value.
#[derive(Clone, Debug, Default)]
pub struct Iv {
    lo: Dy,
    hi: Dy,
}

impl Iv {
    fn point(d: Dy) -> Iv {
        Iv {
            lo: d.clone(),
            hi: d,
        }
    }

    fn f64(x: f64) -> Iv {
        Iv::point(Dy::of_f64(x))
    }

    fn int(n: i64) -> Iv {
        Iv::point(Dy::int(n))
    }

    /// The ends.
    pub fn ends(&self) -> (&Dy, &Dy) {
        (&self.lo, &self.hi)
    }

    fn round(&self, p: u64) -> Iv {
        Iv {
            lo: self.lo.round(p, false),
            hi: self.hi.round(p, true),
        }
    }

    fn neg(&self) -> Iv {
        Iv {
            lo: self.hi.neg(),
            hi: self.lo.neg(),
        }
    }

    fn scale(&self, k: i64) -> Iv {
        Iv {
            lo: self.lo.scale(k),
            hi: self.hi.scale(k),
        }
    }

    fn add(&self, o: &Iv, p: u64) -> Iv {
        Iv {
            lo: self.lo.add(&o.lo).round(p, false),
            hi: self.hi.add(&o.hi).round(p, true),
        }
    }

    fn sub(&self, o: &Iv, p: u64) -> Iv {
        self.add(&o.neg(), p)
    }

    fn mul(&self, o: &Iv, p: u64) -> Iv {
        let c = [
            self.lo.mul(&o.lo),
            self.lo.mul(&o.hi),
            self.hi.mul(&o.lo),
            self.hi.mul(&o.hi),
        ];
        let mut lo = &c[0];
        let mut hi = &c[0];
        for x in &c[1..] {
            if x.cmp(lo) == Ordering::Less {
                lo = x;
            }
            if x.cmp(hi) == Ordering::Greater {
                hi = x;
            }
        }
        Iv {
            lo: lo.round(p, false),
            hi: hi.round(p, true),
        }
    }

    /// `1 / self`; `None` when the interval holds zero.
    fn recip(&self, p: u64) -> Option<Iv> {
        if self.lo.sign() * self.hi.sign() <= 0 {
            return None;
        }
        Some(Iv {
            lo: self.hi.recip(p, false),
            hi: self.lo.recip(p, true),
        })
    }

    fn div(&self, o: &Iv, p: u64) -> Option<Iv> {
        Some(self.mul(&o.recip(p)?, p))
    }

    /// `self / n` for a positive `n`.
    fn div_u(&self, n: u64, p: u64) -> Iv {
        let d = Int::from(n);
        Iv {
            lo: self.lo.div_int(&d, p, false),
            hi: self.hi.div_int(&d, p, true),
        }
    }

    /// `sqrt(self)`; `None` when the interval reaches below zero.
    fn sqrt(&self, p: u64) -> Option<Iv> {
        if self.lo.sign() < 0 {
            return None;
        }
        Some(Iv {
            lo: self.lo.sqrt(p, false),
            hi: self.hi.sqrt(p, true),
        })
    }

    /// The largest magnitude in the interval.
    fn mag_hi(&self) -> Dy {
        let (a, b) = (self.lo.abs(), self.hi.abs());
        if a.cmp(&b) == Ordering::Greater { a } else { b }
    }

    /// Widened by `t >= 0` on each side.
    fn tail(&self, t: &Dy, p: u64) -> Iv {
        Iv {
            lo: self.lo.sub(t).round(p, false),
            hi: self.hi.add(t).round(p, true),
        }
    }

    /// The double nearest each end, when they agree.
    fn rounded(&self) -> Option<f64> {
        let (a, b) = (self.lo.to_f64(), self.hi.to_f64());
        (a.to_bits() == b.to_bits()).then_some(a)
    }
}

// ---------------------------------------------------------------- constants

struct Consts {
    pi: Iv,
    ln2: Iv,
}

/// `atan(1 / n)` for `n >= 2`: the alternating series, its first left-out term as the
/// bound of the rest.
fn atan_inv(n: u64, p: u64) -> Iv {
    let n2 = n * n;
    let mut pw = Iv::int(1).div_u(n, p);
    let mut sum = pw.clone();
    let mut k = 1u64;
    loop {
        pw = pw.div_u(n2, p);
        let term = pw.div_u(2 * k + 1, p);
        if term.mag_hi().mag() < -(p as i64) - 8 {
            return sum.tail(&term.mag_hi(), p);
        }
        sum = if k % 2 == 1 {
            sum.sub(&term, p)
        } else {
            sum.add(&term, p)
        };
        k += 1;
    }
}

/// `atanh(1 / n)` for `n >= 3`: terms falling by `1 / n^2 <= 1/9`, so the rest is at most
/// twice the first left out.
fn atanh_inv(n: u64, p: u64) -> Iv {
    let n2 = n * n;
    let mut pw = Iv::int(1).div_u(n, p);
    let mut sum = pw.clone();
    let mut k = 1u64;
    loop {
        pw = pw.div_u(n2, p);
        let term = pw.div_u(2 * k + 1, p);
        if term.mag_hi().mag() < -(p as i64) - 8 {
            return sum.tail(&term.mag_hi().scale(1), p);
        }
        sum = sum.add(&term, p);
        k += 1;
    }
}

fn consts() -> &'static Consts {
    static C: OnceLock<Consts> = OnceLock::new();
    C.get_or_init(|| {
        let p = MAX_BITS + 64;
        // Machin: pi = 16 atan(1/5) - 4 atan(1/239); ln 2 = 2 atanh(1/3).
        let pi = atan_inv(5, p).scale(4).sub(&atan_inv(239, p).scale(2), p);
        let ln2 = atanh_inv(3, p).scale(1);
        Consts { pi, ln2 }
    })
}

fn pi(p: u64) -> Iv {
    consts().pi.round(p)
}

fn ln2(p: u64) -> Iv {
    consts().ln2.round(p)
}

// ---------------------------------------------------------------- functions on intervals

/// The sum of `t_0 = first`, `t_n = t_(n-1) * step(n)`, cut at the first term below
/// `2^-(p + 8)` of `2^scale_log2` (the sum's size), with `rest` times that term added as
/// the bound of the rest (1 for an alternating series of falling terms, 2 for terms
/// falling by half or more).
fn series(
    first: Iv,
    p: u64,
    scale_log2: i64,
    rest: i64,
    mut step: impl FnMut(&Iv, u64) -> Iv,
) -> Iv {
    let mut t = first.clone();
    let mut sum = first;
    let mut n = 1u64;
    loop {
        t = step(&t, n);
        let m = t.mag_hi();
        if m.is_zero() {
            return sum;
        }
        if m.mag() < scale_log2 - p as i64 - 8 {
            return sum.tail(&m.scale(rest - 1), p);
        }
        sum = sum.add(&t, p);
        n += 1;
        if n > 100_000 {
            // not converging: an interval too wide; the caller's precision rises
            return sum.tail(&Dy::int(1).scale(scale_log2 + 4), p);
        }
    }
}

/// `e^x` for `|x| < 1e6` (outside, a stand-in past the doubles' range).
fn exp_iv(x: &Iv, p: u64) -> Iv {
    let xa = x.lo.to_f64();
    if xa > 1e6 {
        return Iv::point(Dy::int(1).scale(2_000_000));
    }
    if x.hi.to_f64() < -1e6 {
        return Iv {
            lo: Dy::default(),
            hi: Dy::int(1).scale(-2_000_000),
        };
    }
    let w = p + 40;
    let k = (xa / core::f64::consts::LN_2).round() as i64;
    let r = x.sub(&ln2(w + 64).mul(&Iv::int(k), w + 64), w);
    // e^r = (e^(r / 2^h))^(2^h); the Taylor series at |r / 2^h| < 2^-11 falls by half or
    // more per term
    let h = 12;
    let r = r.scale(-h);
    let mut e = series(Iv::int(1), w, 0, 2, |t, n| t.mul(&r, w).div_u(n, w));
    for _ in 0..h {
        e = e.mul(&e, w);
    }
    e.scale(k)
}

/// `ln x` for a finite `x > 0`.
fn ln_iv(x: f64, p: u64) -> Iv {
    let (s, e) = ln_parts(x, p);
    s.add(&ln2(p + 64).mul(&Iv::int(e), p), p)
}

/// `(ln m, e)` with `x = m 2^e`, `m` in `[sqrt(1/2), sqrt(2))`.
fn ln_parts(x: f64, p: u64) -> (Iv, i64) {
    let d = Dy::of_f64(x);
    let mut e = d.mag();
    // m in [1, 2); above sqrt(2) = 1.41421356..., take the binade above
    if x / crate::dd::pow2(e as i32) > core::f64::consts::SQRT_2 {
        e += 1;
    }
    let m = Iv::point(d.scale(-e));
    let w = p + 16;
    let one = Iv::int(1);
    let num = m.sub(&one, w);
    if num.lo.is_zero() && num.hi.is_zero() {
        return (Iv::int(0), e);
    }
    let Some(z) = num.div(&m.add(&one, w), w) else {
        return (Iv::int(0), e);
    };
    // ln m = 2 atanh z, |z| <= 0.172: terms fall by z^2 < 1/32
    let z2 = z.mul(&z, w);
    let zm = z.mag_hi().mag();
    let mut k = 0u64;
    let mut pw = z.clone();
    let s = series(z.clone(), w, zm, 2, |_, n| {
        pw = pw.mul(&z2, w);
        k = n;
        pw.div_u(2 * k + 1, w)
    });
    (s.scale(1), e)
}

/// `(sin r, cos r)` for `|r| <= 0.8` by their Taylor series (alternating, falling terms).
fn sin_cos_small(r: &Iv, p: u64) -> (Iv, Iv) {
    let w = p + 16;
    let r2 = r.mul(r, w);
    let rm = r.mag_hi().mag();
    let s = series(r.clone(), w, rm, 1, |t, n| {
        t.mul(&r2, w).div_u((2 * n) * (2 * n + 1), w).neg()
    });
    let c = series(Iv::int(1), w, -1, 1, |t, n| {
        t.mul(&r2, w).div_u((2 * n - 1) * (2 * n), w).neg()
    });
    (s, c)
}

/// `(sin x, cos x)` for a finite `x`, reduced by `x - k pi/2` with pi to `p` bits past
/// `x`'s size.
fn sin_cos_iv(x: f64, p: u64) -> Option<(Iv, Iv)> {
    if x.abs() < 0.78 {
        return Some(sin_cos_small(&Iv::f64(x), p));
    }
    let d = Dy::of_f64(x);
    let w = p + d.mag().max(0) as u64 + 64;
    if w + 64 > MAX_BITS {
        return None;
    }
    let half_pi = pi(w + 64).scale(-1);
    let q = Iv::point(d.clone()).div(&half_pi, w)?;
    let k = q.lo.add(&Dy::int(1).scale(-1)).floor();
    let r = Iv::point(d).sub(
        &half_pi.mul(&Iv::point(Dy { m: k.clone(), e: 0 }), w + 64),
        w,
    );
    let quadrant = {
        let low = k.low_u64() & 3;
        if k.is_negative() { (4 - low) & 3 } else { low }
    };
    let (s, c) = sin_cos_small(&r, p);
    Some(match quadrant {
        0 => (s, c),
        1 => (c, s.neg()),
        2 => (s.neg(), c.neg()),
        _ => (c.neg(), s),
    })
}

/// `atan t` for an interval of `t > 0`.
fn atan_pos(t: &Iv, p: u64) -> Option<Iv> {
    let w = p + 24;
    if t.hi.cmp(&Dy::int(1)) == Ordering::Greater {
        // atan t = pi/2 - atan(1/t)
        let a = atan_small(&t.recip(w)?, w)?;
        return Some(pi(w).scale(-1).sub(&a, w));
    }
    atan_small(t, w)
}

/// `atan t` for `0 < t <= 1` (and a little past): three halvings
/// `t -> t / (1 + sqrt(1 + t^2))` bring `t` below `tan(pi/32) < 0.1`, where the alternating
/// series falls by `t^2 < 1/100` per term.
fn atan_small(t: &Iv, w: u64) -> Option<Iv> {
    let one = Iv::int(1);
    let mut t = t.clone();
    for _ in 0..3 {
        let d = one.add(&one.add(&t.mul(&t, w), w).sqrt(w)?, w);
        t = t.div(&d, w)?;
    }
    let t2 = t.mul(&t, w);
    let tm = t.mag_hi().mag();
    let mut pw = t.clone();
    let s = series(t.clone(), w, tm, 1, |_, n| {
        pw = pw.mul(&t2, w).neg();
        pw.div_u(2 * n + 1, w)
    });
    Some(s.scale(3))
}

/// `(sinh a, cosh a)` for `a >= 0`.
fn sinh_cosh_iv(a: f64, p: u64) -> Option<(Iv, Iv)> {
    let w = p + 16;
    if a < 1.0 {
        let x = Iv::f64(a);
        let x2 = x.mul(&x, w);
        let am = Dy::of_f64(a).mag();
        // terms fall by a^2 / ((2n)(2n + 1)) <= 1/2
        let s = series(x.clone(), w, am, 2, |t, n| {
            t.mul(&x2, w).div_u((2 * n) * (2 * n + 1), w)
        });
        let c = series(Iv::int(1), w, 0, 2, |t, n| {
            t.mul(&x2, w).div_u((2 * n - 1) * (2 * n), w)
        });
        return Some((s, c));
    }
    let e = exp_iv(&Iv::f64(a), w);
    let r = e.recip(w)?;
    Some((e.sub(&r, w).scale(-1), e.add(&r, w).scale(-1)))
}

// ---------------------------------------------------------------- the references

/// The double the interval `f(p)` rounds to, at the first precision of the ladder where
/// both its ends round to the same double; `None` when none does (or `f` gives none).
pub fn correctly_rounded(f: impl Fn(u64) -> Option<Iv>) -> Option<f64> {
    LADDER.iter().find_map(|&p| f(p)?.rounded())
}

fn finite(xs: &[f64]) -> bool {
    xs.iter().all(|x| x.is_finite())
}

/// `e^x`, correctly rounded; `None` for `x` not finite.
pub fn exp(x: f64) -> Option<f64> {
    if !finite(&[x]) {
        return None;
    }
    correctly_rounded(|p| Some(exp_iv(&Iv::f64(x), p)))
}

/// `2^x`, correctly rounded; `None` for `x` not finite.
pub fn exp2(x: f64) -> Option<f64> {
    if !finite(&[x]) {
        return None;
    }
    if x.abs() > 1e5 {
        return Some(if x > 0.0 { f64::INFINITY } else { 0.0 });
    }
    // x = k + f with k an integer, both exact
    let k = x.round();
    let f = x - k;
    correctly_rounded(|p| {
        let w = p + 16;
        Some(exp_iv(&Iv::f64(f).mul(&ln2(w + 8), w), p).scale(k as i64))
    })
}

/// `ln x`, correctly rounded; `None` for `x` not finite or not above zero.
pub fn ln(x: f64) -> Option<f64> {
    if !finite(&[x]) || x <= 0.0 {
        return None;
    }
    correctly_rounded(|p| Some(ln_iv(x, p)))
}

/// `log2 x`, correctly rounded; `None` for `x` not finite or not above zero.
pub fn log2(x: f64) -> Option<f64> {
    if !finite(&[x]) || x <= 0.0 {
        return None;
    }
    correctly_rounded(|p| {
        let (s, e) = ln_parts(x, p);
        Some(s.div(&ln2(p + 16), p)?.add(&Iv::int(e), p))
    })
}

/// `log10 x`, correctly rounded; `None` for `x` not finite or not above zero.
pub fn log10(x: f64) -> Option<f64> {
    if !finite(&[x]) || x <= 0.0 {
        return None;
    }
    correctly_rounded(|p| ln_iv(x, p + 8).div(&ln_iv(10.0, p + 8), p))
}

/// `sin x`, correctly rounded; `None` for `x` not finite or zero.
pub fn sin(x: f64) -> Option<f64> {
    if !finite(&[x]) || x == 0.0 {
        return None;
    }
    correctly_rounded(|p| Some(sin_cos_iv(x, p)?.0))
}

/// `cos x`, correctly rounded; `None` for `x` not finite.
pub fn cos(x: f64) -> Option<f64> {
    if !finite(&[x]) {
        return None;
    }
    correctly_rounded(|p| Some(sin_cos_iv(x, p)?.1))
}

/// `tan x`, correctly rounded; `None` for `x` not finite or zero.
pub fn tan(x: f64) -> Option<f64> {
    if !finite(&[x]) || x == 0.0 {
        return None;
    }
    correctly_rounded(|p| {
        let (s, c) = sin_cos_iv(x, p + 8)?;
        s.div(&c, p)
    })
}

/// `atan x`, correctly rounded; `None` for `x` not finite or zero.
pub fn atan(x: f64) -> Option<f64> {
    if !finite(&[x]) || x == 0.0 {
        return None;
    }
    let a = correctly_rounded(|p| atan_pos(&Iv::f64(x.abs()), p))?;
    Some(a.copysign(x))
}

/// `asin x`, correctly rounded; `None` for `x` not finite, zero or outside `[-1, 1]`.
pub fn asin(x: f64) -> Option<f64> {
    if !finite(&[x]) || x == 0.0 || x.abs() > 1.0 {
        return None;
    }
    let a = x.abs();
    let r = correctly_rounded(|p| {
        if a == 1.0 {
            return Some(pi(p).scale(-1));
        }
        let w = p + 16;
        // 1 - a^2 = (1 - a)(1 + a), each factor exact
        let d = Iv::point(
            Dy::int(1)
                .sub(&Dy::of_f64(a))
                .mul(&Dy::int(1).add(&Dy::of_f64(a))),
        );
        atan_pos(&Iv::f64(a).div(&d.sqrt(w)?, w)?, p)
    })?;
    Some(r.copysign(x))
}

/// `acos x`, correctly rounded; `None` for `x` not finite or outside `[-1, 1]`.
pub fn acos(x: f64) -> Option<f64> {
    if !finite(&[x]) || x.abs() > 1.0 {
        return None;
    }
    if x == 1.0 {
        return Some(0.0);
    }
    correctly_rounded(|p| {
        if x == -1.0 {
            return Some(pi(p));
        }
        let w = p + 16;
        // acos x = 2 atan(sqrt((1 - x) / (1 + x))), both exact
        let q = Iv::point(Dy::int(1).sub(&Dy::of_f64(x)))
            .div(&Iv::point(Dy::int(1).add(&Dy::of_f64(x))), w)?;
        Some(atan_pos(&q.sqrt(w)?, w)?.scale(1).round(p))
    })
}

/// `atan2(y, x)`, the angle of `(x, y)`, correctly rounded; `None` for an argument not
/// finite or `y` zero (whose result's sign is C99's, not a value).
pub fn atan2(y: f64, x: f64) -> Option<f64> {
    if !finite(&[x, y]) || y == 0.0 {
        return None;
    }
    let r = correctly_rounded(|p| {
        let w = p + 16;
        if x == 0.0 {
            return Some(pi(p).scale(-1));
        }
        let (ay, ax) = (Iv::f64(y.abs()), Iv::f64(x.abs()));
        let t = if y.abs() <= x.abs() {
            atan_pos(&ay.div(&ax, w)?, w)?
        } else {
            pi(w).scale(-1).sub(&atan_pos(&ax.div(&ay, w)?, w)?, w)
        };
        Some(if x < 0.0 {
            pi(w).sub(&t, p)
        } else {
            t.round(p)
        })
    })?;
    Some(r.copysign(y))
}

/// `sinh x`, correctly rounded; `None` for `x` not finite or zero.
pub fn sinh(x: f64) -> Option<f64> {
    if !finite(&[x]) || x == 0.0 {
        return None;
    }
    let r = correctly_rounded(|p| Some(sinh_cosh_iv(x.abs(), p)?.0))?;
    Some(r.copysign(x))
}

/// `cosh x`, correctly rounded; `None` for `x` not finite.
pub fn cosh(x: f64) -> Option<f64> {
    if !finite(&[x]) {
        return None;
    }
    correctly_rounded(|p| Some(sinh_cosh_iv(x.abs(), p)?.1))
}

/// `tanh x`, correctly rounded; `None` for `x` not finite or zero.
pub fn tanh(x: f64) -> Option<f64> {
    if !finite(&[x]) || x == 0.0 {
        return None;
    }
    if x.abs() > 40.0 {
        // 1 - tanh a = 2 / (e^(2a) + 1) < 2^-115: below a quarter ulp of 1
        return Some(1.0f64.copysign(x));
    }
    let r = correctly_rounded(|p| {
        let (s, c) = sinh_cosh_iv(x.abs(), p + 8)?;
        s.div(&c, p)
    })?;
    Some(r.copysign(x))
}

/// The double nearest the positive true value `t`, ties to even, found by bisection over
/// the doubles with `side(q)`, the exact sign of `t - q` at a halfway point `q`.
pub fn round_by_side(side: impl Fn(&Dy) -> Ordering) -> f64 {
    // the halfway point above the positive double `c` (above the largest, the overflow
    // threshold)
    let upper = |c: f64| {
        let dc = Dy::of_f64(c);
        if c == f64::MAX {
            dc.add(&dc.sub(&Dy::of_f64(c.next_down())).scale(-1))
        } else {
            dc.add(&Dy::of_f64(c.next_up())).scale(-1)
        }
    };
    // the least double `c` with `t <= upper(c)`: then `t` is in `(upper(c-), upper(c)]`
    let (mut lo, mut hi) = (0u64, f64::INFINITY.to_bits());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if side(&upper(f64::from_bits(mid))) == Ordering::Greater {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let c = f64::from_bits(lo);
    if c.is_finite() && side(&upper(c)) == Ordering::Equal && lo & 1 == 1 {
        // halfway: to the even neighbour (the one above the largest double is infinity)
        return c.next_up();
    }
    c
}

/// `cbrt x`, correctly rounded, decided exactly (`x` against the cubes of the halfway
/// points); `None` for `x` not finite or zero.
pub fn cbrt(x: f64) -> Option<f64> {
    if !finite(&[x]) || x == 0.0 {
        return None;
    }
    let a = Dy::of_f64(x.abs());
    Some(round_by_side(|q| a.cmp(&q.mul(q).mul(q))).copysign(x))
}

/// `sqrt(x^2 + y^2)`, correctly rounded, decided exactly (`x^2 + y^2` against the squares
/// of the halfway points); `None` for an argument not finite or both zero.
pub fn hypot(x: f64, y: f64) -> Option<f64> {
    if !finite(&[x, y]) || (x == 0.0 && y == 0.0) {
        return None;
    }
    let (dx, dy) = (Dy::of_f64(x), Dy::of_f64(y));
    let s = dx.mul(&dx).add(&dy.mul(&dy));
    Some(round_by_side(|q| s.cmp(&q.mul(q))))
}

/// `x^y`, correctly rounded; `None` for an argument not finite or zero, or `x < 0` with
/// `y` not an integer (C99's special values, checked elsewhere). The powers that may be
/// exact or halfway are computed exactly ([`exact_dyadic_pow`]).
pub fn pow(x: f64, y: f64) -> Option<f64> {
    if !finite(&[x, y]) || x == 0.0 || y == 0.0 {
        return None;
    }
    let integer = y == y.trunc();
    if x < 0.0 && !integer {
        return None;
    }
    let odd = integer && y.abs() < 9.0e15 && (y as i64) % 2 != 0;
    let sign = if x < 0.0 && odd { -1.0 } else { 1.0 };
    let a = x.abs();
    if let Some(r) = exact_dyadic_pow(a, y) {
        return Some(sign * r);
    }
    let r = correctly_rounded(|p| {
        let w = p + 32;
        let l = ln_iv(a, w);
        Some(exp_iv(&l.mul(&Iv::f64(y), w), p))
    })?;
    Some(sign * r)
}

/// `a^y` for `a > 0`, exactly, when `y = n / 2^k` and `a` is a perfect `2^k`-th power
/// (`a^(1/2^k)` found by `k` exact square roots) whose `n`-th power [`exact_pow`] takes.
/// These are the only `x^y` that can be exactly halfway between two doubles (or exact),
/// so the ones Ziv's strategy cannot decide; `3^34`, `194701.5625^2.5`.
fn exact_dyadic_pow(a: f64, y: f64) -> Option<f64> {
    let mut a = a;
    let mut y = y;
    while y != y.trunc() {
        // a = m 2^e: a square only if m is and e is even
        let d = Dy::of_f64(a);
        let tz = d.m.trailing_zeros()?;
        let (m, e) = (d.m.shr(tz), d.e + tz as i64);
        if e % 2 != 0 {
            return None;
        }
        let r = m.isqrt()?;
        if r.mul(&r) != m {
            return None;
        }
        // r has at most 27 bits: exact as a double
        a = Rat::from_int(&r, e / 2).to_f64();
        y *= 2.0;
    }
    exact_pow(a, y)
}

/// `a^n` for `a > 0` and an integer `n`, exactly, when that is cheap: `a` a power of two,
/// or `|n|` times the bits of `a`'s odd part at most 4096.
fn exact_pow(a: f64, n: f64) -> Option<f64> {
    let d = Dy::of_f64(a);
    let tz = d.m.trailing_zeros()?;
    let odd = d.m.shr(tz);
    let e = d.e + tz as i64;
    if odd.is_one() {
        let k = e as f64 * n;
        if k.abs() > 1e5 {
            return Some(if k > 0.0 { f64::INFINITY } else { 0.0 });
        }
        return Some(Rat::from_int(&Int::from(1i64), k as i64).to_f64());
    }
    if n.abs() * odd.bit_len() as f64 > 4096.0 {
        return None;
    }
    let k = n.abs() as u64;
    let mut acc = Int::from(1i64);
    let mut base = odd;
    let mut bits = k;
    while bits > 0 {
        if bits & 1 == 1 {
            acc = acc.mul(&base);
        }
        base = base.mul(&base);
        bits >>= 1;
    }
    let v = Rat::from_int(&acc, e * k as i64);
    Some(if n < 0.0 {
        v.recip()?.to_f64()
    } else {
        v.to_f64()
    })
}

// ---------------------------------------------------------------- input

/// A double written as C99 hex (`0x1.8p+3`, `-0x.fffffffffffffp-1022`, `0xd.71bfp-4`), in
/// decimal (`1`, `-0`, `-1000.0`, `1e-300`), or as `inf`, `nan` or `snan`, each with an optional sign,
/// rounded to the nearest double as C's `strtod` does (CORE-MATH's files hold a few hex
/// values below the subnormals); `None` for anything else.
pub fn parse_f64(s: &str) -> Option<f64> {
    let s = s.trim();
    let (neg, body) = match s.as_bytes().first() {
        Some(b'-') => (true, s.get(1..)?),
        Some(b'+') => (false, s.get(1..)?),
        _ => (false, s),
    };
    let sign = |v: f64| if neg { -v } else { v };
    match body {
        "inf" | "infinity" => return Some(sign(f64::INFINITY)),
        "nan" | "snan" => return Some(f64::NAN),
        _ => {}
    }
    let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) else {
        // decimal: Rust's parse rounds correctly, as `strtod` does
        return body.parse::<f64>().ok().filter(|v| !v.is_nan()).map(sign);
    };
    let (mant, exp) = match hex.find(['p', 'P']) {
        Some(i) => (hex.get(..i)?, hex.get(i + 1..)?.parse::<i64>().ok()?),
        None => (hex, 0),
    };
    let (int, frac) = mant.split_once('.').unwrap_or((mant, ""));
    if int.is_empty() && frac.is_empty() {
        return None;
    }
    let mut m = Int::default();
    for ch in int.chars().chain(frac.chars()) {
        let d = ch.to_digit(16)?;
        m = m.shl(4).add(&Int::from(u64::from(d)));
    }
    Some(sign(
        Rat::from_int(&m, exp - 4 * frac.len() as i64).to_f64(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::f64::consts as c;

    #[test]
    fn constants_round_to_std() {
        assert_eq!(pi(256).rounded(), Some(c::PI));
        assert_eq!(ln2(256).rounded(), Some(c::LN_2));
        assert_eq!(exp(1.0), Some(c::E));
        assert_eq!(ln(10.0), Some(c::LN_10));
        assert_eq!(log2(10.0), Some(c::LOG2_10));
        assert_eq!(log10(2.0), Some(c::LOG10_2));
        assert_eq!(atan(1.0), Some(c::FRAC_PI_4));
        assert_eq!(asin(1.0), Some(c::FRAC_PI_2));
        assert_eq!(acos(-1.0), Some(c::PI));
        assert_eq!(atan2(1.0, -1.0), Some(3.0 * c::FRAC_PI_4));
        assert_eq!(exp2(0.5), Some(c::SQRT_2));
        assert_eq!(pow(2.0, 0.5), Some(c::SQRT_2));
    }

    #[test]
    fn exact_cases() {
        assert_eq!(log2(1024.0), Some(10.0));
        assert_eq!(log10(1e22), Some(22.0));
        assert_eq!(ln(1.0), Some(0.0));
        assert_eq!(exp2(-1074.0), Some(5e-324));
        assert_eq!(exp2(-1075.0), Some(0.0));
        assert_eq!(cbrt(27.0), Some(3.0));
        assert_eq!(hypot(3.0, 4.0), Some(5.0));
        // 3^34 is odd and of 54 bits: halfway, to even
        let r = pow(3.0, 34.0).map(f64::to_bits);
        assert_eq!(r, Some(16677181699666568f64.to_bits()));
        assert_eq!(pow(2.0, -1075.0), Some(0.0));
        assert_eq!(pow(-2.0, 3.0), Some(-8.0));
        assert_eq!(pow(4.0, 0.5), Some(2.0));
        // 194701.5625 = (1765 / 4)^2, so its 2.5th power is exactly halfway between two doubles
        let want = Rat::from_int(&Int::from(3115225u64 * 3115225 * 1765), -10).to_f64();
        assert_eq!(pow(194701.5625, 2.5), Some(want));
    }

    #[test]
    fn known_hard_values() {
        // sin(1e22), Ng's example of a reduction needing pi to 1100+ bits
        assert_eq!(sin(1e22), Some(-0.8522008497671888));
        assert_eq!(cos(1e22), Some(0.523214785395139));
        // the largest double
        assert!(sin(f64::MAX).is_some());
        assert_eq!(exp(709.8), Some(f64::INFINITY));
        assert_eq!(exp(-746.0), Some(0.0));
    }

    #[test]
    fn parses_hex() {
        assert_eq!(parse_f64("0x1.8p+3"), Some(12.0));
        assert_eq!(
            parse_f64("-0x.fffffffffffffp-1022"),
            Some(-(f64::MIN_POSITIVE - 5e-324))
        );
        assert_eq!(parse_f64("0x1p-1074"), Some(5e-324));
        assert_eq!(
            parse_f64("0xd.71bf68b38193p-4"),
            Some(f64::from_bits(0x3feae37ed1670326))
        );
        assert_eq!(parse_f64("+1"), Some(1.0));
        assert_eq!(parse_f64("-1000.0"), Some(-1000.0));
        assert_eq!(parse_f64("0.1"), Some(0.1));
        assert!(parse_f64("-0").is_some_and(|v| v == 0.0 && v.is_sign_negative()));
        assert_eq!(parse_f64("-inf"), Some(f64::NEG_INFINITY));
        assert!(parse_f64("snan").is_some_and(f64::is_nan));
        // rounded, ties to even
        assert_eq!(parse_f64("0x1.00000000000008p+0"), Some(1.0));
        assert_eq!(
            parse_f64("0x1.00000000000018p+0"),
            Some(1.0 + 2.0 * f64::EPSILON)
        );
        assert_eq!(parse_f64("0x1p-1080"), Some(0.0));
        assert_eq!(parse_f64("0x1.8p+1"), Some(3.0));
        assert_eq!(parse_f64("0x"), None);
        assert_eq!(parse_f64("0x1.2.3"), None);
    }

    /// Against the double-double references where those are sure (the dd value's `hi`
    /// well inside a rounding interval).
    #[test]
    fn agrees_with_double_double() {
        let mut r = scx_num::hash::Rng::new(77);
        for _ in 0..300 {
            let x = 20.0 * r.next_signed_unit();
            let (d, n) = crate::dd::exp(crate::dd::Dd::new(x));
            assert_eq!(exp(x), Some(crate::dd::scale(d.to_f64(), n)), "exp {x}");
            let s = crate::dd::sin_cos(crate::dd::Dd::new(x));
            assert_eq!(sin(x), Some(s.0.to_f64()), "sin {x}");
            assert_eq!(cos(x), Some(s.1.to_f64()), "cos {x}");
            let a = x.abs();
            assert_eq!(ln(a), Some(crate::dd::ln(a).to_f64()), "ln {a}");
        }
    }
}
