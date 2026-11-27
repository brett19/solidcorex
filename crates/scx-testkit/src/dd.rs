// Written for solidcorex. Apache-2.0.
//! Double-double references: about 104 bits, from the error-free transformations of
//! [`scx_num::eft`] alone. Slow and plain: series to 1e-34, Machin's formula for pi, `ln`
//! through `atanh`, `e^x` by squaring, `atan` by Newton's method from `fmath`'s result.
//! Moved here from `scx-num`'s `fmath` tests, which now use it from here.
//!
//! A double-double result is within about `2^-100` relative of the true value (the
//! functions say where they lose more), not correctly rounded: its `hi` is the value
//! rounded once, which can differ from the correct rounding only when the true value lies
//! within about `2^-100` relative of a halfway point.

use core::ops::{Add, Div, Mul, Neg, Sub};
use std::sync::OnceLock;

use scx_num::eft::{fast_two_sum, two_prod, two_sum};
use scx_num::fmath;

/// A double-double: `hi + lo` with `|lo| <= ulp(hi) / 2`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Dd {
    /// The value rounded to a double.
    pub hi: f64,
    /// The rest.
    pub lo: f64,
}

impl Dd {
    /// `x` exactly.
    pub const fn new(x: f64) -> Dd {
        Dd { hi: x, lo: 0.0 }
    }

    /// `p.0 + p.1` from an error-free transformation (`|p.1| <= ulp(p.0) / 2`).
    pub const fn of(p: (f64, f64)) -> Dd {
        Dd { hi: p.0, lo: p.1 }
    }

    /// `a - b` for doubles, exactly.
    pub fn diff(a: f64, b: f64) -> Dd {
        Dd::of(two_sum(a, -b))
    }

    /// `a * b` for doubles, exactly.
    pub fn prod(a: f64, b: f64) -> Dd {
        Dd::of(two_prod(a, b))
    }

    /// The square root (zero for a value not above zero).
    #[must_use]
    pub fn sqrt(self) -> Dd {
        if self.hi <= 0.0 {
            return Dd::new(0.0);
        }
        let s = self.hi.sqrt();
        let r = self - Dd::of(two_prod(s, s));
        Dd::of(fast_two_sum(s, r.hi / (2.0 * s)))
    }

    /// `|self|`.
    #[must_use]
    pub fn abs(self) -> Dd {
        if self.hi < 0.0 {
            -self
        } else {
            self
        }
    }

    /// `self 2^k`, exact whenever both parts stay normal.
    #[must_use]
    pub fn scale(self, k: i32) -> Dd {
        Dd {
            hi: scale(self.hi, k),
            lo: scale(self.lo, k),
        }
    }

    /// The nearest double to `hi + lo` (rounded once; see the module note).
    pub fn to_f64(self) -> f64 {
        self.hi + self.lo
    }
}

impl From<f64> for Dd {
    fn from(x: f64) -> Dd {
        Dd::new(x)
    }
}

impl Add for Dd {
    type Output = Dd;
    fn add(self, o: Dd) -> Dd {
        let (s, e) = two_sum(self.hi, o.hi);
        let (t, f) = two_sum(self.lo, o.lo);
        let (s, e) = fast_two_sum(s, e + t);
        Dd::of(fast_two_sum(s, e + f))
    }
}

impl Neg for Dd {
    type Output = Dd;
    fn neg(self) -> Dd {
        Dd {
            hi: -self.hi,
            lo: -self.lo,
        }
    }
}

impl Sub for Dd {
    type Output = Dd;
    fn sub(self, o: Dd) -> Dd {
        self + -o
    }
}

impl Mul for Dd {
    type Output = Dd;
    fn mul(self, o: Dd) -> Dd {
        let (p, e) = two_prod(self.hi, o.hi);
        Dd::of(fast_two_sum(p, e + (self.hi * o.lo + self.lo * o.hi)))
    }
}

impl Mul<f64> for Dd {
    type Output = Dd;
    fn mul(self, f: f64) -> Dd {
        self * Dd::new(f)
    }
}

impl Div for Dd {
    type Output = Dd;
    fn div(self, o: Dd) -> Dd {
        let q1 = self.hi / o.hi;
        let r = self - o * q1;
        let q2 = r.hi / o.hi;
        let r = r - o * q2;
        let q3 = r.hi / o.hi;
        Dd::of(fast_two_sum(q1, q2)) + Dd::new(q3)
    }
}

impl Div<f64> for Dd {
    type Output = Dd;
    fn div(self, f: f64) -> Dd {
        self / Dd::new(f)
    }
}

/// A series term `t` is spent once it is not above `bound`; a NaN term (from a NaN input)
/// counts as spent, so no loop runs on forever.
fn spent(t: f64, bound: f64) -> bool {
    t.partial_cmp(&bound) != Some(core::cmp::Ordering::Greater)
}

/// `2^k` for any `k`: a normal power of two for `k` in `[-1022, 1023]`, a subnormal one
/// down to `-1074`, zero below and infinity above.
pub fn pow2(k: i32) -> f64 {
    if k > 1023 {
        f64::INFINITY
    } else if k >= -1022 {
        f64::from_bits(((k + 1023) as u64) << 52)
    } else if k >= -1074 {
        f64::from_bits(1u64 << (k + 1074))
    } else {
        0.0
    }
}

/// `x 2^k` for finite `x` and any `k`, rounded once (ties to even): exact whenever the
/// result is normal, correctly rounded when it is subnormal, infinite when it overflows.
pub fn scale(x: f64, k: i32) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    // x = m 2^e with |m| in [1, 2), exactly, the significand read from the bits.
    let e = exponent(x);
    let b = if e < -1022 {
        (x * pow2(64)).to_bits()
    } else {
        x.to_bits()
    };
    let m = f64::from_bits((b & 0x800f_ffff_ffff_ffff) | 0x3ff0_0000_0000_0000);
    let t = e.saturating_add(k);
    if t > 1023 {
        f64::INFINITY.copysign(x)
    } else if t >= -1022 {
        m * pow2(t)
    } else if t >= -1080 {
        // m 2^-1022 is exact; the second product is the one rounding.
        m * pow2(-1022) * pow2(t + 1022)
    } else {
        0.0f64.copysign(x)
    }
}

/// `floor(log2 |x|)` for a finite nonzero `x`.
pub fn exponent(x: f64) -> i32 {
    let f = ((x.to_bits() >> 52) & 0x7ff) as i32;
    if f == 0 {
        exponent(x * pow2(64)) - 64
    } else {
        f - 1023
    }
}

/// `|c - r 2^k|` in ulps of `r 2^k` (the ulp of its binade, floored at the subnormal
/// spacing); an infinite `c` counts as `2^1024`.
pub fn ulp_err(c: f64, r: Dd, k: i32) -> f64 {
    if c.is_nan() {
        return f64::INFINITY;
    }
    if r.hi == 0.0 {
        return if c == 0.0 { 0.0 } else { f64::INFINITY };
    }
    let mut e = exponent(r.hi) + k;
    // hi a power of two and lo against it: the binade below.
    if r.hi.to_bits().trailing_zeros() >= 52 && r.lo != 0.0 && (r.lo < 0.0) != (r.hi < 0.0) {
        e -= 1;
    }
    if (c < 0.0) != (r.hi < 0.0) && c != 0.0 {
        return f64::INFINITY;
    }
    if e >= 1024 {
        return if c.is_infinite() { 0.0 } else { f64::INFINITY };
    }
    let u = (e - 52).max(-1074);
    let rs = r.abs().scale(k - u);
    let cs = if c.is_infinite() {
        scale(1.0, 1024 - u)
    } else {
        scale(c.abs(), -u)
    };
    ((cs - rs.hi) - rs.lo).abs()
}

// ---------------------------------------------------------------- constants

/// Which series [`series`] sums.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Series {
    Atanh,
    Atan,
}

/// `atanh(s)` or `atan(s)` by their series, `|s| <= 1/3`.
fn series(s: Dd, which: Series) -> Dd {
    let s2 = s * s;
    let (mut term, mut sum, mut n) = (s, s, 1.0);
    loop {
        term = term * s2;
        n += 2.0;
        let mut t = term / n;
        if which == Series::Atan && (n as i64 / 2) % 2 == 1 {
            t = -t;
        }
        sum = sum + t;
        if spent(t.hi.abs(), sum.hi.abs() * 1e-34) {
            return sum;
        }
    }
}

/// The constants the references use, in double-double.
#[derive(Clone, Copy, Debug)]
pub struct Consts {
    /// `ln 2`.
    pub ln2: Dd,
    /// `ln 10`.
    pub ln10: Dd,
    /// `pi`.
    pub pi: Dd,
    /// `pi / 2`.
    pub pio2: Dd,
}

/// The constants, computed once.
pub fn consts() -> &'static Consts {
    static CONSTS: OnceLock<Consts> = OnceLock::new();
    CONSTS.get_or_init(|| {
        let ln2 = series(Dd::new(1.0) / 3.0, Series::Atanh) * 2.0;
        // ln 10 = 3 ln 2 + ln(5/4), ln(5/4) = 2 atanh(1/9).
        let ln10 = ln2 * 3.0 + series(Dd::new(1.0) / 9.0, Series::Atanh) * 2.0;
        // Machin: pi/4 = 4 atan(1/5) - atan(1/239).
        let a5 = series(Dd::new(1.0) / 5.0, Series::Atan);
        let a239 = series(Dd::new(1.0) / 239.0, Series::Atan);
        let pi = a5 * 16.0 - a239 * 4.0;
        Consts {
            ln2,
            ln10,
            pi,
            pio2: pi * 0.5,
        }
    })
}

// ---------------------------------------------------------------- functions

/// `ln(x)` for a finite positive `x`: `e ln 2 + 2 atanh((m - 1)/(m + 1))`.
pub fn ln(x: f64) -> Dd {
    let mut e = exponent(x);
    let mut m = scale(x, -e);
    if m > core::f64::consts::SQRT_2 {
        m *= 0.5;
        e += 1;
    }
    // m - 1 is exact (Sterbenz); m + 1 as a double-double.
    let s = Dd::new(m - 1.0) / Dd::of(two_sum(m, 1.0));
    consts().ln2 * f64::from(e) + series(s, Series::Atanh) * 2.0
}

/// `e^z` as `(d, n)`, meaning `d 2^n`.
pub fn exp(z: Dd) -> (Dd, i32) {
    let ln2 = consts().ln2;
    let n = (z.hi / ln2.hi).round();
    let r = (z - ln2 * n).scale(-10);
    // expm1(r) by its series, then (1 + t)^1024 as t <- 2t + t^2, ten times.
    let (mut term, mut t, mut i) = (r, r, 1.0);
    while r.hi != 0.0 && r.hi.is_finite() {
        i += 1.0;
        term = term * r / i;
        t = t + term;
        if spent(term.hi.abs(), t.hi.abs() * 1e-34) {
            break;
        }
    }
    for _ in 0..10 {
        t = t * 2.0 + t * t;
    }
    (Dd::new(1.0) + t, n as i32)
}

/// `(sin x, cos x)` for `|x| < 2^20`. The reduction by `pi/2` costs `|x| 2^-106`
/// absolute: about `2^-102` for an angle in a turn or two.
pub fn sin_cos(x: Dd) -> (Dd, Dd) {
    let pio2 = consts().pio2;
    let n = (x.hi / pio2.hi).round();
    let y = x - pio2 * n;
    let y2 = y * y;
    let (mut ts, mut s, mut i) = (y, y, 1.0);
    while y.hi != 0.0 && y.hi.is_finite() {
        ts = ts * y2 / (-(i + 1.0) * (i + 2.0));
        i += 2.0;
        s = s + ts;
        if spent(ts.hi.abs(), s.hi.abs() * 1e-34) {
            break;
        }
    }
    let (mut tc, mut c, mut i) = (Dd::new(1.0), Dd::new(1.0), 0.0);
    loop {
        tc = tc * y2 / (-(i + 1.0) * (i + 2.0));
        i += 2.0;
        c = c + tc;
        if spent(tc.hi.abs(), 1e-34) {
            break;
        }
    }
    match (n as i64).rem_euclid(4) {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

/// `atan(t)` by Newton's method on `tan y = t` from `fmath`'s result, for `|t| <= 1`
/// (above, `pi/2 - atan(1/t)`: Newton's step is ill-conditioned next to `pi/2`).
pub fn atan(t: Dd) -> Dd {
    if t.hi.abs() > 1.0 {
        let r = atan(Dd::new(1.0) / t);
        let pio2 = consts().pio2;
        let h = if t.hi > 0.0 { pio2 } else { -pio2 };
        return h - r;
    }
    let mut y = Dd::new(fmath::atan(t.hi));
    for _ in 0..2 {
        let (s, c) = sin_cos(y);
        y = y + (t * c - s) * c;
    }
    y
}

/// `atan2(y, x)`: the angle of `(x, y)` in `(-pi, pi]`, zero at the origin.
pub fn atan2(y: Dd, x: Dd) -> Dd {
    let k = consts();
    if x.hi == 0.0 && y.hi == 0.0 {
        return Dd::new(0.0);
    }
    if y.hi.abs() <= x.hi.abs() {
        let a = atan(y / x);
        if x.hi > 0.0 {
            a
        } else if y.hi < 0.0 {
            a - k.pi
        } else {
            a + k.pi
        }
    } else {
        let a = atan(x / y);
        if y.hi > 0.0 {
            k.pio2 - a
        } else {
            -k.pio2 - a
        }
    }
}

/// `(sinh x, cosh x)` by their series, for `|x|` below about 1.
pub fn sinh_cosh_series(x: f64) -> (Dd, Dd) {
    let x = Dd::new(x);
    let x2 = x * x;
    let (mut ts, mut s, mut tc, mut c, mut i) = (x, x, Dd::new(1.0), Dd::new(1.0), 0.0);
    loop {
        tc = tc * x2 / ((i + 1.0) * (i + 2.0));
        ts = ts * x2 / ((i + 2.0) * (i + 3.0));
        i += 2.0;
        c = c + tc;
        s = s + ts;
        if spent(tc.hi, 1e-34) {
            return (s, c);
        }
    }
}

/// `a . b` for vectors of double-doubles.
pub fn dot<const N: usize>(a: &[Dd; N], b: &[Dd; N]) -> Dd {
    a.iter().zip(b).fold(Dd::new(0.0), |s, (x, y)| s + *x * *y)
}

/// A vector of doubles as double-doubles.
pub fn lift<const N: usize>(v: [f64; N]) -> [Dd; N] {
    v.map(Dd::new)
}

/// A vector of double-doubles, each rounded once.
pub fn round<const N: usize>(v: [Dd; N]) -> [f64; N] {
    v.map(Dd::to_f64)
}
