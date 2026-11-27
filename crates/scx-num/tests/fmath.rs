// Written for solidcorex. Apache-2.0.
//! `fmath` against references computed in double-double (about 104 bits, by
//! `scx_testkit::dd`), at
//! fixed-seed random inputs over each function's domain, and at hard cases whose
//! references were computed once at 90 digits with Python's `decimal` (the table at the
//! end: huge trigonometric arguments, the edges of overflow and underflow, subnormals,
//! arguments next to 1, the boundaries between each function's methods). Every result
//! must lie within [`BOUND`] ulp of its reference. C99's special values are checked bit
//! for bit.
//!
//! The references use nothing of `fmath` but its results as starting points that
//! Newton's method then corrects (`atan`, `cbrt`), and the error-free transformations of
//! [`scx_num::eft`]. They are slow and plain: series to 1e-34, Machin's formula for pi,
//! `ln` through `atanh`, `e^x` by squaring.

#![allow(missing_docs, clippy::pedantic, clippy::indexing_slicing)]

use core::ops::{Add, Div, Mul, Neg, Sub};

use scx_num::eft::{two_prod, two_sum};
use scx_num::fmath;
use scx_num::hash::Rng;
use scx_testkit::dd::{self, consts, exponent, pow2, scale, ulp_err, Consts as K, Dd};

/// The largest error allowed, in ulps of the reference. `fmath` measures at or below
/// 0.5 ulp (correctly rounded) on every input checked; the margin covers only the
/// reference's own error.
const BOUND: f64 = 0.501;

/// Random inputs per function.
const CASES: usize = 4000;

/// The reference for `f` at `(x, y)`, as `(d, n)` meaning `d 2^n`.
fn reference(k: &K, f: &str, x: f64, y: f64) -> (Dd, i32) {
    let plain = |d: Dd| (d, 0);
    match f {
        "exp" => dd::exp(Dd::new(x)),
        "exp2" => dd::exp(k.ln2.mul(x)),
        "ln" => plain(dd::ln(x)),
        "log2" => plain(dd::ln(x).div(k.ln2)),
        "log10" => plain(dd::ln(x).div(k.ln10)),
        "sin" => plain(dd::sin_cos(Dd::new(x)).0),
        "cos" => plain(dd::sin_cos(Dd::new(x)).1),
        "tan" => {
            let (s, c) = dd::sin_cos(Dd::new(x));
            plain(s.div(c))
        }
        "atan" => plain(dd::atan(Dd::new(x))),
        "asin" if x.abs() == 1.0 => plain(k.pio2.mul(x)),
        "acos" if x == -1.0 => plain(k.pi),
        "asin" => {
            let c = Dd::of(two_sum(1.0, -x)).mul(Dd::of(two_sum(1.0, x))).sqrt();
            plain(dd::atan(Dd::new(x).div(c)))
        }
        "acos" => {
            let t = Dd::of(two_sum(1.0, -x)).div(Dd::of(two_sum(1.0, x))).sqrt();
            plain(dd::atan(t).mul(2.0))
        }
        "atan2" => {
            // atan2(x, y): x is the ordinate.
            let a = dd::atan(Dd::new(x).div(Dd::new(y)).abs());
            let a = if y > 0.0 { a } else { k.pi.sub(a) };
            plain(if x < 0.0 { a.neg() } else { a })
        }
        "sinh" | "cosh" | "tanh" => {
            let a = x.abs();
            let sg = |d: Dd| if x < 0.0 && f != "cosh" { d.neg() } else { d };
            if a < 0.5 {
                let (s, c) = dd::sinh_cosh_series(a);
                return plain(sg(match f {
                    "sinh" => s,
                    "cosh" => c,
                    _ => s.div(c),
                }));
            }
            if a > 40.0 && f != "tanh" {
                let (d, n) = dd::exp(Dd::new(a));
                return (sg(d), n - 1);
            }
            if a > 40.0 {
                // 1 - 2 e^(-2a).
                let (d, n) = dd::exp(Dd::new(-2.0 * a));
                return plain(sg(Dd::new(1.0).sub(d.scale(n + 1))));
            }
            let (d, n) = dd::exp(Dd::new(a));
            let e = d.scale(n);
            let r = Dd::new(1.0).div(e);
            plain(sg(match f {
                "sinh" => e.sub(r).mul(0.5),
                "cosh" => e.add(r).mul(0.5),
                _ => e.sub(r).div(e.add(r)),
            }))
        }
        "cbrt" => {
            let e = exponent(x);
            let q = e.div_euclid(3);
            let a = scale(x.abs(), -3 * q);
            let mut r = Dd::new(fmath::cbrt(a));
            for _ in 0..2 {
                let res = r.mul(r).mul(r).sub(Dd::new(a));
                r = r.sub(res.div(r.mul(r).mul(3.0)));
            }
            (if x < 0.0 { r.neg() } else { r }, q)
        }
        "hypot" => {
            let e = exponent(x.abs().max(y.abs()));
            let (xs, ys) = (scale(x, -e), scale(y, -e));
            let s = Dd::of(two_prod(xs, xs)).add(Dd::of(two_prod(ys, ys)));
            (s.sqrt(), e)
        }
        "powf" | "powi" => {
            let (d, n) = dd::exp(dd::ln(x.abs()).mul(y));
            let odd = y == y.trunc() && y.abs() < 9.0e15 && (y as i64) & 1 == 1;
            (if x < 0.0 && odd { d.neg() } else { d }, n)
        }
        _ => unreachable!("{f}"),
    }
}

fn call(f: &str, x: f64, y: f64) -> f64 {
    match f {
        "exp" => fmath::exp(x),
        "exp2" => fmath::exp2(x),
        "ln" => fmath::ln(x),
        "log2" => fmath::log2(x),
        "log10" => fmath::log10(x),
        "sin" => fmath::sin(x),
        "cos" => fmath::cos(x),
        "tan" => fmath::tan(x),
        "sin_cos.sin" => fmath::sin_cos(x).0,
        "sin_cos.cos" => fmath::sin_cos(x).1,
        "atan" => fmath::atan(x),
        "asin" => fmath::asin(x),
        "acos" => fmath::acos(x),
        "atan2" => fmath::atan2(x, y),
        "sinh" => fmath::sinh(x),
        "cosh" => fmath::cosh(x),
        "tanh" => fmath::tanh(x),
        "cbrt" => fmath::cbrt(x),
        "hypot" => fmath::hypot(x, y),
        "powf" => fmath::powf(x, y),
        "powi" => fmath::powi(x, y as i32),
        _ => unreachable!("{f}"),
    }
}

// ---------------------------------------------------------------- inputs

/// A normal double with exponent in `[lo, hi]`, a random mantissa and a random sign.
fn logu(rng: &mut Rng, lo: i32, hi: i32) -> f64 {
    let span = (hi - lo + 1) as u64;
    let e = lo + (rng.next_u64() % span) as i32;
    let b = rng.next_u64();
    let m = b & 0x000f_ffff_ffff_ffff;
    f64::from_bits((((e + 1023) as u64) << 52) | m | (b & (1 << 63)))
}

/// A random finite double of any sign and size, subnormals included.
fn any_finite(rng: &mut Rng) -> f64 {
    loop {
        let x = f64::from_bits(rng.next_u64());
        if x.is_finite() && x != 0.0 {
            return x;
        }
    }
}

/// Input `i` of `f` (both arguments).
fn input(f: &str, rng: &mut Rng, i: usize) -> (f64, f64) {
    let half = i % 2 == 0;
    let u = rng.next_signed_unit();
    let x = match f {
        "exp" => {
            if half {
                logu(rng, -60, 9).clamp(-745.0, 709.7)
            } else {
                727.0 * u - 18.0
            }
        }
        "exp2" => {
            if half {
                logu(rng, -60, 10).clamp(-1075.0, 1023.9)
            } else {
                1049.0 * u - 26.0
            }
        }
        "ln" | "log2" | "log10" => {
            if half {
                any_finite(rng).abs()
            } else {
                let k = 1 + (rng.next_u64() % 52) as i32;
                1.0 + u * pow2(-k)
            }
        }
        "sin" | "cos" | "tan" | "sin_cos.sin" | "sin_cos.cos" => {
            if half {
                logu(rng, -30, 17)
            } else {
                10.0 * u
            }
        }
        "atan" => logu(rng, -60, 60),
        "asin" | "acos" => {
            if half {
                u
            } else {
                let k = 1 + (rng.next_u64() % 52) as i32;
                (1.0 - pow2(-k) * rng.next_f64()).copysign(u)
            }
        }
        "sinh" | "cosh" => {
            if half {
                logu(rng, -30, 5)
            } else {
                710.0 * u
            }
        }
        "tanh" => {
            if half {
                logu(rng, -30, 5)
            } else {
                25.0 * u
            }
        }
        "cbrt" => any_finite(rng),
        "hypot" => {
            let x = logu(rng, -1022, 1023);
            let d = (rng.next_u64() % 121) as i32 - 60;
            let y = scale(logu(rng, 0, 0), (exponent(x) + d).clamp(-1022, 1023));
            return (x, y);
        }
        "atan2" => return (logu(rng, -100, 100), logu(rng, -100, 100)),
        "powf" => {
            if half {
                let x = logu(rng, -20, 20).abs();
                let l = fmath::ln(x).abs().max(1e-3);
                return (x, u * 740.0 / l);
            }
            let n = (rng.next_u64() % 121) as f64 - 60.0;
            return (-logu(rng, -10, 10).abs(), n);
        }
        "powi" => {
            let x = if half {
                logu(rng, -8, 8)
            } else {
                1.0 + u * pow2(-30)
            };
            let l = fmath::ln(x.abs()).abs().max(1e-12);
            let cap = (740.0 / l).min(2_147_483_647.0);
            return (x, (rng.next_signed_unit() * cap).trunc());
        }
        _ => unreachable!("{f}"),
    };
    (x, 0.0)
}

const FUNCTIONS: [&str; 21] = [
    "exp",
    "exp2",
    "ln",
    "log2",
    "log10",
    "sin",
    "cos",
    "tan",
    "sin_cos.sin",
    "sin_cos.cos",
    "atan",
    "asin",
    "acos",
    "atan2",
    "sinh",
    "cosh",
    "tanh",
    "cbrt",
    "hypot",
    "powf",
    "powi",
];

fn ref_name(f: &str) -> &str {
    match f {
        "sin_cos.sin" => "sin",
        "sin_cos.cos" => "cos",
        _ => f,
    }
}

#[test]
fn random_inputs_within_bound() {
    let k = consts();
    let mut failures = Vec::new();
    for (j, f) in FUNCTIONS.iter().enumerate() {
        let mut rng = Rng::new(Rng::DEFAULT_SEED ^ j as u64);
        let mut worst = (0.0, 0.0, 0.0);
        for i in 0..CASES {
            let (x, y) = input(f, &mut rng, i);
            let (d, n) = reference(k, ref_name(f), x, y);
            let err = ulp_err(call(f, x, y), d, n);
            if err > worst.0 {
                worst = (err, x, y);
            }
        }
        if worst.0 > BOUND {
            failures.push(format!(
                "{f}: {} ulp at ({:e}, {:e})",
                worst.0, worst.1, worst.2
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn hard_cases_within_bound() {
    let mut failures = Vec::new();
    for &(f, x, y, hi, lo, n) in HARD {
        let (x, y) = (f64::from_bits(x), f64::from_bits(y));
        let r = Dd {
            hi: f64::from_bits(hi),
            lo: f64::from_bits(lo),
        };
        let fs: &[&str] = match f {
            "sin" => &["sin", "sin_cos.sin"],
            "cos" => &["cos", "sin_cos.cos"],
            _ => &[f],
        };
        for g in fs {
            let c = call(g, x, y);
            let err = ulp_err(c, r, n);
            if err > BOUND {
                failures.push(format!("{g}({x:e}, {y:e}) = {c:e}: {err} ulp"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The references themselves, at a few exactly known values.
#[test]
fn references_at_known_values() {
    let k = consts();
    let exact = |f: &str, x: f64, y: f64, v: f64| {
        let (d, n) = reference(k, f, x, y);
        assert!(ulp_err(v, d, n) < 1e-6, "{f}({x}, {y}) reference");
        assert_eq!(call(f, x, y), v, "{f}({x}, {y})");
    };
    exact("exp", 0.0, 0.0, 1.0);
    exact("exp2", 10.0, 0.0, 1024.0);
    exact("exp2", -1074.0, 0.0, 5e-324);
    exact("ln", 1.0, 0.0, 0.0);
    exact("log2", 0.125, 0.0, -3.0);
    exact("log10", 1e22, 0.0, 22.0);
    exact("cbrt", -27.0, 0.0, -3.0);
    exact("cbrt", 0.125, 0.0, 0.5);
    exact("hypot", 3.0, 4.0, 5.0);
    exact("powi", 2.0, 1023.0, pow2(1023));
    exact("powi", -3.0, 3.0, -27.0);
    exact("powf", 2.0, -3.0, 0.125);
    exact("powf", 4.0, 0.5, 2.0);
    exact("powf", 10.0, 22.0, 1e22);
    // pi/4, pi, and pi/2 rounded.
    let (d, _) = reference(k, "atan", 1.0, 0.0);
    assert!(ulp_err(core::f64::consts::FRAC_PI_4, d, 0) <= 0.5);
    assert!(ulp_err(core::f64::consts::PI, k.pi, 0) <= 0.5);
    assert!(ulp_err(core::f64::consts::LN_2, k.ln2, 0) <= 0.5);
    assert!(ulp_err(core::f64::consts::LN_10, k.ln10, 0) <= 0.5);
}

/// C99 Annex F's special values, bit for bit; every NaN result is [`f64::NAN`].
#[test]
fn special_values() {
    let (inf, nan, mz) = (f64::INFINITY, f64::NAN, -0.0);
    let pi = core::f64::consts::PI;
    let pio2 = core::f64::consts::FRAC_PI_2;
    let pio4 = core::f64::consts::FRAC_PI_4;
    let odd_nan = f64::from_bits(0xfff8_0000_0000_1234);
    let cases: &[(&str, f64, f64, f64)] = &[
        ("exp", inf, 0.0, inf),
        ("exp", -inf, 0.0, 0.0),
        ("exp", 710.0, 0.0, inf),
        ("exp", -746.0, 0.0, 0.0),
        ("exp", mz, 0.0, 1.0),
        ("exp", odd_nan, 0.0, nan),
        ("exp2", inf, 0.0, inf),
        ("exp2", -inf, 0.0, 0.0),
        ("exp2", 1024.0, 0.0, inf),
        ("ln", 0.0, 0.0, -inf),
        ("ln", mz, 0.0, -inf),
        ("ln", -1.0, 0.0, nan),
        ("ln", inf, 0.0, inf),
        ("ln", odd_nan, 0.0, nan),
        ("log2", 0.0, 0.0, -inf),
        ("log2", -inf, 0.0, nan),
        ("log10", inf, 0.0, inf),
        ("log10", mz, 0.0, -inf),
        ("sin", mz, 0.0, mz),
        ("sin", inf, 0.0, nan),
        ("cos", -inf, 0.0, nan),
        ("cos", mz, 0.0, 1.0),
        ("tan", mz, 0.0, mz),
        ("tan", inf, 0.0, nan),
        ("sin_cos.sin", nan, 0.0, nan),
        ("sin_cos.cos", inf, 0.0, nan),
        ("atan", inf, 0.0, pio2),
        ("atan", -inf, 0.0, -pio2),
        ("atan", mz, 0.0, mz),
        ("atan", odd_nan, 0.0, nan),
        ("asin", mz, 0.0, mz),
        ("asin", 1.0, 0.0, pio2),
        ("asin", 1.5, 0.0, nan),
        ("acos", 1.0, 0.0, 0.0),
        ("acos", -1.0, 0.0, pi),
        ("acos", -1.5, 0.0, nan),
        ("atan2", 0.0, mz, pi),
        ("atan2", mz, mz, -pi),
        ("atan2", 0.0, 0.0, 0.0),
        ("atan2", mz, 0.0, mz),
        ("atan2", 0.0, -1.0, pi),
        ("atan2", mz, -1.0, -pi),
        ("atan2", 1.0, 0.0, pio2),
        ("atan2", -1.0, mz, -pio2),
        ("atan2", 1.0, -inf, pi),
        ("atan2", -1.0, inf, mz),
        ("atan2", inf, -inf, 3.0 * pio4),
        ("atan2", -inf, inf, -pio4),
        ("atan2", inf, 1.0, pio2),
        ("atan2", nan, 1.0, nan),
        ("sinh", inf, 0.0, inf),
        ("sinh", -inf, 0.0, -inf),
        ("sinh", mz, 0.0, mz),
        ("sinh", 711.0, 0.0, inf),
        ("cosh", -inf, 0.0, inf),
        ("cosh", mz, 0.0, 1.0),
        ("tanh", inf, 0.0, 1.0),
        ("tanh", -inf, 0.0, -1.0),
        ("tanh", mz, 0.0, mz),
        ("tanh", odd_nan, 0.0, nan),
        ("cbrt", mz, 0.0, mz),
        ("cbrt", -inf, 0.0, -inf),
        ("cbrt", odd_nan, 0.0, nan),
        ("hypot", inf, nan, inf),
        ("hypot", nan, -inf, inf),
        ("hypot", nan, 1.0, nan),
        ("hypot", mz, 0.0, 0.0),
        ("hypot", f64::MAX, f64::MAX, inf),
        ("powf", nan, 0.0, 1.0),
        ("powf", 1.0, nan, 1.0),
        ("powf", -1.0, inf, 1.0),
        ("powf", -1.0, -inf, 1.0),
        ("powf", 0.5, inf, 0.0),
        ("powf", 0.5, -inf, inf),
        ("powf", 2.0, inf, inf),
        ("powf", -2.0, -inf, 0.0),
        ("powf", 0.0, -1.0, inf),
        ("powf", mz, -1.0, -inf),
        ("powf", mz, -2.0, inf),
        ("powf", mz, 3.0, mz),
        ("powf", mz, 0.5, 0.0),
        ("powf", -inf, 3.0, -inf),
        ("powf", -inf, -3.0, mz),
        ("powf", -inf, 2.0, inf),
        ("powf", inf, -0.5, 0.0),
        ("powf", -8.0, 1.0 / 3.0, nan),
        ("powf", nan, 1.0, nan),
        ("powf", 2.0, 1024.0, inf),
        ("powf", -2.0, 1025.0, -inf),
        ("powi", nan, 0.0, 1.0),
        ("powi", nan, 2.0, nan),
        ("powi", odd_nan, 1.0, nan),
        ("powi", 0.0, -1.0, inf),
        ("powi", mz, -3.0, -inf),
        ("powi", mz, -2.0, inf),
        ("powi", mz, 3.0, mz),
        ("powi", -inf, 3.0, -inf),
        ("powi", -inf, -3.0, mz),
        ("powi", 2.0, 1024.0, inf),
        ("powi", -2.0, 1025.0, -inf),
        ("powi", 2.0, -1075.0, 0.0),
    ];
    let mut failures = Vec::new();
    for &(f, x, y, want) in cases {
        let got = call(f, x, y);
        if got.to_bits() != want.to_bits() {
            failures.push(format!(
                "{f}({x:e}, {y:e}) = {got:e} ({:#x}), want {want:e}",
                got.to_bits()
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// `(f, x, y, hi, lo, n)`: the reference `(hi + lo) 2^n` for `f(x, y)` (bits), from
/// Python's `decimal` at 90 digits. `atan2` rows hold `(y, x)` in that order, as
/// `fmath::atan2` takes them; `powi` rows hold the exponent as a double.
#[rustfmt::skip]
const HARD: &[(&str, u64, u64, u64, u64, i32)] = &[
    ("sin", 0x4480f0cf064dd592, 0x0000000000000000, 0xbffb453ab76bf397, 0xbc6f453790772648, -1),
    ("sin", 0x7e37e43c8800759c, 0x0000000000000000, 0xbffa2c16b010e385, 0xbc9b900a1f54ecd2, -1),
    ("sin", 0x7fefffffffffffff, 0x0000000000000000, 0x3ff452fc98b34e97, 0xbc927bb193d960df, -8),
    ("sin", 0x7fe0000000000000, 0x0000000000000000, 0x3ff205248cbdb760, 0xbc7a5a336baf7435, -1),
    ("sin", 0x7506ac5b262ca1ff, 0x0000000000000000, 0x4000000000000000, 0xb852b089ea1e692b, -1),
    ("sin", 0x4076300000000000, 0x0000000000000000, 0xbfff9bd0307d1de3, 0x3c9894874d2528d2, -16),
    ("sin", 0x40f9639000000000, 0x0000000000000000, 0xbff40efdf1eb8de7, 0x3c9f636eaa54141c, -16),
    ("sin", 0x3ff921fb54442d18, 0x0000000000000000, 0x4000000000000000, 0xb93377ce858a5d48, -1),
    ("sin", 0x400921fb54442d18, 0x0000000000000000, 0x3ff1a62633145c07, 0xbc7f1976b7ed8fbd, -53),
    ("sin", 0x4012d97c7f3321d2, 0x0000000000000000, 0xc000000000000000, 0x3965e6c8563ba8f1, -1),
    ("sin", 0x401921fb54442d18, 0x0000000000000000, 0xbff1a62633145c07, 0x3c7f1976b7ed8fbf, -52),
    ("sin", 0x4130000000000000, 0x0000000000000000, 0x3ff526ccb2fc8656, 0xbc9312bfebafe1df, -2),
    ("sin", 0x412fffff00000000, 0x0000000000000000, 0xbff4cb305757fa66, 0x3c79051c2ac3d9a0, -3),
    ("sin", 0x4130000100000000, 0x0000000000000000, 0x3fff20cc1ff62d21, 0x3c9469f59e66396f, -1),
    ("sin", 0x40f86a0000000000, 0x0000000000000000, 0x3ff24daa9c527e96, 0x3c8c767d8e3e1ca8, -5),
    ("sin", 0x430c6bf526340000, 0x0000000000000000, 0x3ffb76f88136ceba, 0xbc8b5acbdcf56c2a, -1),
    ("sin", 0x4330000000000000, 0x0000000000000000, 0x3ffbf996908bb506, 0x3c7aa7c4af1e96c1, -1),
    ("sin", 0x4340000000000001, 0x0000000000000000, 0xbff04b40dd7d9a6a, 0x3c873d9f9d9ac7f8, -3),
    ("sin", 0x3fe921fb54442d18, 0x0000000000000000, 0x3ff6a09e667f3bcc, 0x3c97a7fb8d4bd43f, -1),
    ("sin", 0x4002d97c7f3321d2, 0x0000000000000000, 0x3ff6a09e667f3bcd, 0x3c83267a12a5e3d6, -1),
    ("sin", 0x3e45798ee2308c3a, 0x0000000000000000, 0x3ff5798ee2308c3a, 0xbc79ca58cce0be35, -27),
    ("sin", 0x3e40000000000000, 0x0000000000000000, 0x4000000000000000, 0xbc75555555555555, -28),
    ("sin", 0x3e4000001ad7f29b, 0x0000000000000000, 0x3ff000001ad7f29b, 0xbc655555c0b52075, -27),
    ("sin", 0xc480f0cf064dd592, 0x0000000000000000, 0x3ffb453ab76bf397, 0x3c6f453790772648, -1),
    ("sin", 0x01a56e1fc2f8f359, 0x0000000000000000, 0x3ff56e1fc2f8f359, 0x0000000000000000, -997),
    ("sin", 0x401c000000000000, 0x0000000000000000, 0x3ff50608c26d0a08, 0x3c90eea221047ebc, -1),
    ("sin", 0x4036000000000000, 0x0000000000000000, 0xbff220a29f6eb9f4, 0x3c8504b8bc40d505, -7),
    ("sin", 0x41155cc000000000, 0x0000000000000000, 0x3fffbfeebead6bfe, 0x3c8d101d074ffd54, -1),
    ("cos", 0x4480f0cf064dd592, 0x0000000000000000, 0x3ff0be2cef01c8f4, 0xbc9b2d1bc8018c4f, -1),
    ("cos", 0x7e37e43c8800759c, 0x0000000000000000, 0xbff2699022adc4c1, 0x3c8edd5594b5c574, -1),
    ("cos", 0x7fefffffffffffff, 0x0000000000000000, 0xbfffffe62ecfab75, 0xbc8e038d934070f1, -1),
    ("cos", 0x7fe0000000000000, 0x0000000000000000, 0xbffa719f26c232bf, 0x3c97a77829eb1138, -1),
    ("cos", 0x7506ac5b262ca1ff, 0x0000000000000000, 0xbff14ae72e6ba22f, 0x3c673eef1477d90e, -61),
    ("cos", 0x4076300000000000, 0x0000000000000000, 0xbfffffffffc18e4c, 0x3c7862265016699c, -1),
    ("cos", 0x40f9639000000000, 0x0000000000000000, 0x3fffffffffe6da77, 0x3c77035e9b2fcdec, -1),
    ("cos", 0x3ff921fb54442d18, 0x0000000000000000, 0x3ff1a62633145c07, 0xbc7f1976b7ed8fbc, -54),
    ("cos", 0x400921fb54442d18, 0x0000000000000000, 0xc000000000000000, 0x395377ce858a5d48, -1),
    ("cos", 0x4012d97c7f3321d2, 0x0000000000000000, 0xbffa79394c9e8a0a, 0xbc9456737b06ea19, -53),
    ("cos", 0x401921fb54442d18, 0x0000000000000000, 0x4000000000000000, 0xb97377ce858a5d48, -1),
    ("cos", 0x4130000000000000, 0x0000000000000000, 0x3ffe33ada92fe2ae, 0xbc7420bfaf737b2d, -1),
    ("cos", 0x412fffff00000000, 0x0000000000000000, 0x3fff932ec65b1d71, 0x3c75759be4908756, -1),
    ("cos", 0x4130000100000000, 0x0000000000000000, 0x3ffdacfb27d45b34, 0x3c9530f241e80f53, -3),
    ("cos", 0x40f86a0000000000, 0x0000000000000000, 0xbffffac3841b3da7, 0xbc8b085bf2c1a978, -1),
    ("cos", 0x430c6bf526340000, 0x0000000000000000, 0xbff06c154609d33f, 0x3c9ea7fabcd955c1, -1),
    ("cos", 0x4330000000000000, 0x0000000000000000, 0xbfff1300d681503f, 0xbc6f31ed7798da5d, -2),
    ("cos", 0x4340000000000001, 0x0000000000000000, 0x3fffbd5b0b6def11, 0xbc83fd5f6387a372, -1),
    ("cos", 0x3fe921fb54442d18, 0x0000000000000000, 0x3ff6a09e667f3bcd, 0xbc8ec4c7696139d5, -1),
    ("cos", 0x4002d97c7f3321d2, 0x0000000000000000, 0xbff6a09e667f3bcc, 0x3c54da530b7ba971, -1),
    ("cos", 0x3e45798ee2308c3a, 0x0000000000000000, 0x4000000000000000, 0xbc9cd2b297d889bc, -1),
    ("cos", 0x3e40000000000000, 0x0000000000000000, 0x4000000000000000, 0xbc90000000000000, -1),
    ("cos", 0x3e4000001ad7f29b, 0x0000000000000000, 0x4000000000000000, 0xbc90000035afe563, -1),
    ("cos", 0xc480f0cf064dd592, 0x0000000000000000, 0x3ff0be2cef01c8f4, 0xbc9b2d1bc8018c4f, -1),
    ("cos", 0x01a56e1fc2f8f359, 0x0000000000000000, 0x3ff0000000000000, 0x0000000000000000, 0),
    ("cos", 0x401c000000000000, 0x0000000000000000, 0x3ff81ff79ed92017, 0x3c957deb462d4ceb, -1),
    ("cos", 0x4036000000000000, 0x0000000000000000, 0xbfffffadd8d4acda, 0xbc3f6b2aa0db6ecd, -1),
    ("cos", 0x41155cc000000000, 0x0000000000000000, 0x3ffff4458c9b1e96, 0xbc5b69dc18381c60, -4),
    ("tan", 0x4480f0cf064dd592, 0x0000000000000000, 0xbffa0f79c1b6b257, 0xbc9d27810f5737dd, 0),
    ("tan", 0x7e37e43c8800759c, 0x0000000000000000, 0x3ff6be411f37ac77, 0xbc95a67ce3109efb, 0),
    ("tan", 0x7fefffffffffffff, 0x0000000000000000, 0xbff4530cfe729484, 0x3c91c97823bf87a5, -8),
    ("tan", 0x7fe0000000000000, 0x0000000000000000, 0xbff5ce6b4c0d02a3, 0xbc94efc20fe21559, -1),
    ("tan", 0x7506ac5b262ca1ff, 0x0000000000000000, 0xbffd9ba9a7975636, 0x3c9714cf36c65449, 60),
    ("tan", 0x4076300000000000, 0x0000000000000000, 0x3fff9bd030bacc16, 0x3c77ccc31be7b4ad, -16),
    ("tan", 0x40f9639000000000, 0x0000000000000000, 0xbff40efdf1fb5124, 0xbc6dc170232909b4, -16),
    ("tan", 0x3ff921fb54442d18, 0x0000000000000000, 0x3ffd02967c31cdb5, 0xbc7f3c72fe49aa2a, 53),
    ("tan", 0x400921fb54442d18, 0x0000000000000000, 0xbff1a62633145c07, 0x3c7f1976b7ed8fba, -53),
    ("tan", 0x4012d97c7f3321d2, 0x0000000000000000, 0x3ff3570efd768923, 0x3c90209780490e4d, 52),
    ("tan", 0x401921fb54442d18, 0x0000000000000000, 0xbff1a62633145c07, 0x3c7f1976b7ed8fb5, -52),
    ("tan", 0x4130000000000000, 0x0000000000000000, 0x3ff6692e5779206f, 0x3c9d2e6e7ad9a795, -2),
    ("tan", 0x412fffff00000000, 0x0000000000000000, 0xbff512d9bf52026a, 0x3c9586a3fb76f0b3, -3),
    ("tan", 0x4130000100000000, 0x0000000000000000, 0x3ff0c878130c5486, 0xbc686170042fd743, 2),
    ("tan", 0x40f86a0000000000, 0x0000000000000000, 0xbff250a9d503313d, 0x3c9c133e111bd9d8, -5),
    ("tan", 0x430c6bf526340000, 0x0000000000000000, 0xbffac23600a95be4, 0xbc8f75e34b730016, 0),
    ("tan", 0x4330000000000000, 0x0000000000000000, 0xbffccef2838da5ca, 0xbc9d2fe978fa2e54, 0),
    ("tan", 0x4340000000000001, 0x0000000000000000, 0xbff06d7752283c69, 0x3c53d364f700fd9c, -3),
    ("tan", 0x3fe921fb54442d18, 0x0000000000000000, 0x3fffffffffffffff, 0x3c9cb3b399d747f3, -1),
    ("tan", 0x4002d97c7f3321d2, 0x0000000000000000, 0xbff0000000000001, 0x3c861b1acd85d7d4, 0),
    ("tan", 0x3e45798ee2308c3a, 0x0000000000000000, 0x3ff5798ee2308c3a, 0x3c89ca58cce0be35, -27),
    ("tan", 0x3e40000000000000, 0x0000000000000000, 0x3ff0000000000000, 0x3c75555555555555, -27),
    ("tan", 0x3e4000001ad7f29b, 0x0000000000000000, 0x3ff000001ad7f29b, 0x3c755555c0b52076, -27),
    ("tan", 0xc480f0cf064dd592, 0x0000000000000000, 0x3ffa0f79c1b6b257, 0x3c9d27810f5737dd, 0),
    ("tan", 0x01a56e1fc2f8f359, 0x0000000000000000, 0x3ff56e1fc2f8f359, 0x0000000000000000, -997),
    ("tan", 0x401c000000000000, 0x0000000000000000, 0x3ffbe2e6e13eea79, 0xbc99f741d00a75bd, -1),
    ("tan", 0x4036000000000000, 0x0000000000000000, 0x3ff220d129b73ef8, 0xbc69d695f2680431, -7),
    ("tan", 0x41155cc000000000, 0x0000000000000000, 0x3fffcb95fc3bc9d7, 0x3c7c22024bd207d7, 2),
    ("exp", 0x40862e42fefa39ef, 0x0000000000000000, 0x3fffffffffffff2a, 0x3c7b0e263400d160, 1023),
    ("exp", 0x40862e3d70a3d70a, 0x0000000000000000, 0x3fffe9ce5c4c52b4, 0x3c9a8a120488d827, 1023),
    ("exp", 0xc0874910d52d3051, 0x0000000000000000, 0x3ff00000000001bf, 0xbc8cf9c6a6b90c7b, -1075),
    ("exp", 0xc087490a3d70a3d7, 0x0000000000000000, 0x3ff00d34e97025f0, 0xbc97ebfdcb68108d, -1075),
    ("exp", 0xc086232bdd7abcd2, 0x0000000000000000, 0x3ff000000000007c, 0xbc90c013e41c0952, -1022),
    ("exp", 0xc086233333333333, 0x0000000000000000, 0x3fffe2b68d3dbf12, 0xbc9688b5868bea36, -1023),
    ("exp", 0xc086800000000000, 0x0000000000000000, 0x3ff32769b92a3d7c, 0x3c9c7048665b0daa, -1039),
    ("exp", 0x01a56e1fc2f8f359, 0x0000000000000000, 0x3ff0000000000000, 0x0000000000000000, 0),
    ("exp", 0x81a56e1fc2f8f359, 0x0000000000000000, 0x3ff0000000000000, 0x0000000000000000, 0),
    ("exp", 0x3c90000000000000, 0x0000000000000000, 0x3ff0000000000000, 0x3c90000000000000, 0),
    ("exp", 0x3fe0000000000000, 0x0000000000000000, 0x3ffa61298e1e069c, 0xbc8b4690082a4906, 0),
    ("exp", 0x3ff0000000000000, 0x0000000000000000, 0x3ff5bf0a8b145769, 0x3c94d57ee2b1013a, 1),
    ("exp", 0x40562e42feba39ef, 0x0000000000000000, 0x3fffffffdffffff5, 0x3c90d8768b722976, 127),
    ("exp", 0xbf662e42f78c1446, 0x0000000000000000, 0x3fffe9d96b318cda, 0x3c6d3869867e7b76, -1),
    ("exp", 0x3ddb7cdfd9d7bdbb, 0x0000000000000000, 0x3ff000000006df38, 0xbc63112d8e5e6d4c, 0),
    ("exp", 0x4085e00000000000, 0x0000000000000000, 0x3ffd945df4f8ec8e, 0x3c8183392684a46e, 1009),
    ("exp", 0xc087200000000000, 0x0000000000000000, 0x3ff531fc8b1a3c7b, 0x3c704cceed14f65d, -1068),
    ("exp2", 0x408fffffffffffff, 0x0000000000000000, 0x3ffffffffffffd3a, 0x3c8bd0105c706c87, 1023),
    ("exp2", 0x408ff80000000000, 0x0000000000000000, 0x3ff0000000000000, 0x2dce8e37d89efb10, 1023),
    ("exp2", 0xc08ff00000000000, 0x0000000000000000, 0x3ff0000000000000, 0x2db83096e0d3316d, -1022),
    ("exp2", 0xc08ff40000000000, 0x0000000000000000, 0x3ff6a09e667f3bcd, 0xbc9bdd3413b26456, -1023),
    ("exp2", 0xc090c80000000000, 0x0000000000000000, 0x3ff0000000000000, 0x2db83096e0d3316d, -1074),
    ("exp2", 0xc090ca0000000000, 0x0000000000000000, 0x3ff6a09e667f3bcd, 0xbc9bdd3413b26456, -1075),
    ("exp2", 0xc090cc0000000000, 0x0000000000000000, 0x4000000000000000, 0xadde8e37d89efb10, -1076),
    ("exp2", 0x3fe0000000000000, 0x0000000000000000, 0x3ff6a09e667f3bcd, 0xbc9bdd3413b26456, 0),
    ("exp2", 0x01a56e1fc2f8f359, 0x0000000000000000, 0x3ff0000000000000, 0x0000000000000000, 0),
    ("exp2", 0x3c30000000000000, 0x0000000000000000, 0x3ff0000000000000, 0x3c262e42fefa39ef, 0),
    ("exp2", 0x3fd5555555555555, 0x0000000000000000, 0x3ff428a2f98d728b, 0xbc883ec99bccda4d, 0),
    ("exp2", 0xc090410000000000, 0x0000000000000000, 0x3ffae89f995ad3ad, 0x3c97a1cd345dcc81, -1041),
    ("exp2", 0x408f40cccccccccd, 0x0000000000000000, 0x3ff125fbee2506b0, 0x3c8c612ebbb95c3e, 1000),
    ("ln", 0x3ff0000000000001, 0x0000000000000000, 0x3fffffffffffffff, 0x3965555555555554, -53),
    ("ln", 0x3fefffffffffffff, 0x0000000000000000, 0xbff0000000000000, 0xbc90000000000000, -53),
    ("ln", 0x3ff0000000000002, 0x0000000000000000, 0x3ffffffffffffffe, 0x3985555555555553, -52),
    ("ln", 0x3feffffffffffffe, 0x0000000000000000, 0xbff0000000000001, 0x3c9fffffffffffff, -52),
    ("ln", 0x0000000000000001, 0x0000000000000000, 0xbff74385446d71c3, 0xbc98e569fa8ee781, 9),
    ("ln", 0x0010000000000000, 0x0000000000000000, 0xbff6232bdd7abcd2, 0xbc8eef3fec1be37f, 9),
    ("ln", 0x000fffffffffffff, 0x0000000000000000, 0xbff6232bdd7abcd2, 0xbc8f2f3fec1be37f, 9),
    ("ln", 0x7fefffffffffffff, 0x0000000000000000, 0x3ff62e42fefa39ef, 0x3c8a9c9e3b39803f, 9),
    ("ln", 0x4000000000000000, 0x0000000000000000, 0x3ff62e42fefa39ef, 0x3c8abc9e3b39803f, -1),
    ("ln", 0x4024000000000000, 0x0000000000000000, 0x3ff26bb1bbb55516, 0xbc9f48ad494ea3e9, 1),
    ("ln", 0x4480f0cf064dd592, 0x0000000000000000, 0x3ff95414621954fe, 0xbc9b03ee44cc2161, 5),
    ("ln", 0x01a56e1fc2f8f359, 0x0000000000000000, 0xbff5963447f87fb5, 0xbc8aa670d35324e6, 9),
    ("ln", 0x3fe6a09e667f3bcd, 0x0000000000000000, 0xbff62e42fefa39ee, 0x3c5716fdfdbc882e, -2),
    ("ln", 0x3ff6a09e667f3bcd, 0x0000000000000000, 0x3ff62e42fefa39f0, 0x3c9c2e0e1b1548c2, -2),
    ("ln", 0x3ff68f5c28f5c28f, 0x0000000000000000, 0x3ff5fd5fabe64084, 0xbc62752864b08ed4, -2),
    ("ln", 0x3ff6b851eb851eb8, 0x0000000000000000, 0x3ff6712984ec8f15, 0xbc93acc7bbe0b21b, -2),
    ("ln", 0x3ff0000100000000, 0x0000000000000000, 0x3fffffff00000aab, 0xbc95755553bbbbd1, -21),
    ("ln", 0x3feff7ced916872b, 0x0000000000000000, 0xbff064670d979b73, 0xbc7e46a88405b99b, -10),
    ("ln", 0x7e37e43c8800759c, 0x0000000000000000, 0x3ff5963447f87fb5, 0x3c8abccc0710fcd4, 9),
    ("ln", 0x4008000000000000, 0x0000000000000000, 0x3ff193ea7aad030b, 0xbc9a256f99caabeb, 0),
    ("log2", 0x3ff0000000000001, 0x0000000000000000, 0x3ff71547652b82fd, 0x3c97b365759f7d4f, -52),
    ("log2", 0x3fefffffffffffff, 0x0000000000000000, 0xbff71547652b82fe, 0xbc9cf33ba5220647, -53),
    ("log2", 0x3ff0000000000002, 0x0000000000000000, 0x3ff71547652b82fd, 0xbc96772954b788a8, -51),
    ("log2", 0x3feffffffffffffe, 0x0000000000000000, 0xbff71547652b82ff, 0x3c87eef9eb64ed72, -52),
    ("log2", 0x0000000000000001, 0x0000000000000000, 0xbff0c80000000000, 0x0000000000000000, 10),
    ("log2", 0x0010000000000000, 0x0000000000000000, 0xbffff00000000000, 0x0000000000000000, 9),
    ("log2", 0x000fffffffffffff, 0x0000000000000000, 0xbffff00000000000, 0xbc271547652b82ff, 9),
    ("log2", 0x7fefffffffffffff, 0x0000000000000000, 0x4000000000000000, 0xbc171547652b82fe, 9),
    ("log2", 0x4000000000000000, 0x0000000000000000, 0x3ff0000000000000, 0x0000000000000000, 0),
    ("log2", 0x4024000000000000, 0x0000000000000000, 0x3ffa934f0979a371, 0x3c97f2495fb7fa6d, 1),
    ("log2", 0x4480f0cf064dd592, 0x0000000000000000, 0x3ff245465683a05e, 0xbc6c4b6c718c1ea6, 6),
    ("log2", 0x01a56e1fc2f8f359, 0x0000000000000000, 0xbfff24a09f1a8b89, 0x3c81ea8f9f7c393d, 9),
    ("log2", 0x3fe6a09e667f3bcd, 0x0000000000000000, 0xbffffffffffffffe, 0xbc8c991a39819edf, -2),
    ("log2", 0x3ff6a09e667f3bcd, 0x0000000000000000, 0x3ff0000000000001, 0xbc7c991a39819edf, -1),
    ("log2", 0x3ff68f5c28f5c28f, 0x0000000000000000, 0x3fffb97834105ea6, 0x3c8cc7d83c952008, -2),
    ("log2", 0x3ff6b851eb851eb8, 0x0000000000000000, 0x3ff030422cd10ffb, 0xbc7cea137a96418f, -1),
    ("log2", 0x3ff0000100000000, 0x0000000000000000, 0x3ff71546ac814f86, 0x3c9f5ea66b0903c6, -20),
    ("log2", 0x3feff7ced916872b, 0x0000000000000000, 0xbff7a6210ffb7151, 0xbc7d365a602612e8, -10),
    ("log2", 0x7e37e43c8800759c, 0x0000000000000000, 0x3fff24a09f1a8b89, 0xbc81ca4ec9a5578d, 9),
    ("log2", 0x4008000000000000, 0x0000000000000000, 0x3ff95c01a39fbd68, 0x3c9e7e802c48281a, 0),
    ("log10", 0x3ff0000000000001, 0x0000000000000000, 0x3ffbcb7b1526e50d, 0x3c9512b4b307b350, -54),
    ("log10", 0x3fefffffffffffff, 0x0000000000000000, 0xbffbcb7b1526e50f, 0x3c978ada0d839d88, -55),
    ("log10", 0x3ff0000000000002, 0x0000000000000000, 0x3ffbcb7b1526e50c, 0x3c9d7bbe88b9e93a, -53),
    ("log10", 0x3feffffffffffffe, 0x0000000000000000, 0xbffbcb7b1526e50f, 0xbc7102841e8d1e21, -54),
    ("log10", 0x0000000000000001, 0x0000000000000000, 0xbff434e6420f4374, 0x3c7a0fae8e2e61be, 8),
    ("log10", 0x0010000000000000, 0x0000000000000000, 0xbff33a7146f72a42, 0x3c68cf2f9ac032a2, 8),
    ("log10", 0x000fffffffffffff, 0x0000000000000000, 0xbff33a7146f72a42, 0x3c67f0d3c216fb7a, 8),
    ("log10", 0x7fefffffffffffff, 0x0000000000000000, 0x3ff34413509f79ff, 0xbc6a4b4b95e998b5, 8),
    ("log10", 0x4000000000000000, 0x0000000000000000, 0x3ff34413509f79ff, 0xbc69dc1da994fd21, -2),
    ("log10", 0x4024000000000000, 0x0000000000000000, 0x3ff0000000000000, 0x0000000000000000, 0),
    ("log10", 0x4480f0cf064dd592, 0x0000000000000000, 0x3ff6000000000000, 0x0000000000000000, 4),
    ("log10", 0x01a56e1fc2f8f359, 0x0000000000000000, 0xbff2c00000000000, 0x3be9183449324080, 8),
    ("log10", 0x3fe6a09e667f3bcd, 0x0000000000000000, 0xbff34413509f79fe, 0x3c7ebfc7851a1808, -3),
    ("log10", 0x3ff6a09e667f3bcd, 0x0000000000000000, 0x3ff34413509f7a00, 0x3c538ea76e146b9e, -3),
    ("log10", 0x3ff68f5c28f5c28f, 0x0000000000000000, 0x3ff3199ca46580e7, 0x3c5b85dee32fa229, -3),
    ("log10", 0x3ff6b851eb851eb8, 0x0000000000000000, 0x3ff37e2f3957f697, 0xbc927b3b8f1a7a35, -3),
    ("log10", 0x3ff0000100000000, 0x0000000000000000, 0x3ffbcb7a36cb15a9, 0xbc889f9e327ac187, -22),
    ("log10", 0x3feff7ced916872b, 0x0000000000000000, 0xbffc79e5dbf022b6, 0xbc8450d383cd7136, -12),
    ("log10", 0x7e37e43c8800759c, 0x0000000000000000, 0x3ff2c00000000000, 0x3bfa4a1e46e7b5c9, 8),
    ("log10", 0x4008000000000000, 0x0000000000000000, 0x3ffe8927964fd5fd, 0x3c61860687504345, -2),
    ("atan", 0x3ff0000000000000, 0x0000000000000000, 0x3ff921fb54442d18, 0x3c91a62633145c07, -1),
    ("atan", 0x3ff0000000000001, 0x0000000000000000, 0x3ff921fb54442d19, 0x3c91a62633145c05, -1),
    ("atan", 0x3fefffffffffffff, 0x0000000000000000, 0x3ff921fb54442d18, 0xbc8cb3b399d747f3, -1),
    ("atan", 0x43b0000000000000, 0x0000000000000000, 0x3ff921fb54442d18, 0x3c91662633145c07, 0),
    ("atan", 0x43b0000000000001, 0x0000000000000000, 0x3ff921fb54442d18, 0x3c91662633145c07, 0),
    ("atan", 0x7e37e43c8800759c, 0x0000000000000000, 0x3ff921fb54442d18, 0x3c91a62633145c07, 0),
    ("atan", 0x7fefffffffffffff, 0x0000000000000000, 0x3ff921fb54442d18, 0x3c91a62633145c07, 0),
    ("atan", 0x0000000000000001, 0x0000000000000000, 0x3ff0000000000000, 0x0000000000000000, -1074),
    ("atan", 0x01a56e1fc2f8f359, 0x0000000000000000, 0x3ff56e1fc2f8f359, 0x0000000000000000, -997),
    ("atan", 0x3e40000000000000, 0x0000000000000000, 0x4000000000000000, 0xbc85555555555555, -28),
    ("atan", 0x3e3ffffffffffffe, 0x0000000000000000, 0x3ffffffffffffffe, 0xbc85555555555551, -28),
    ("atan", 0x3fe0000000000000, 0x0000000000000000, 0x3ffdac670561bb4f, 0x3c9a2b7f222f65e2, -2),
    ("atan", 0x4000000000000000, 0x0000000000000000, 0x3ff1b6e192ebbe44, 0x3c9b1b466a88828e, 0),
    ("atan", 0x4197d78400000000, 0x0000000000000000, 0x3ff921fb5194fb3c, 0x3c20e232da01e8d1, 0),
    ("atan", 0xbe45798ee2308c3a, 0x0000000000000000, 0xbff5798ee2308c3a, 0x3c89ca58cce0be35, -27),
    ("asin", 0x3ff0000000000000, 0x0000000000000000, 0x3ff921fb54442d18, 0x3c91a62633145c07, 0),
    ("asin", 0xbff0000000000000, 0x0000000000000000, 0xbff921fb54442d18, 0xbc91a62633145c07, 0),
    ("asin", 0x3fefffffffffffff, 0x0000000000000000, 0x3ff921fb50442d18, 0x3c91a6263269b15c, 0),
    ("asin", 0x3fe0000000000000, 0x0000000000000000, 0x3ff0c152382d7366, 0xbc9ee6913347c2a6, -1),
    ("asin", 0x3fdfffffffffffff, 0x0000000000000000, 0x3ff0c152382d7365, 0xbc6ecefdf34146f4, -1),
    ("asin", 0x3fe0000000000001, 0x0000000000000000, 0x3ff0c152382d7367, 0xbc94fff41d06f634, -1),
    ("asin", 0x3e40000000000000, 0x0000000000000000, 0x3ff0000000000000, 0x3c65555555555555, -27),
    ("asin", 0x01a56e1fc2f8f359, 0x0000000000000000, 0x3ff56e1fc2f8f359, 0x0000000000000000, -997),
    ("asin", 0x0000000000000001, 0x0000000000000000, 0x3ff0000000000000, 0x0000000000000000, -1074),
    ("asin", 0x3fefff2e48e8a71e, 0x0000000000000000, 0x3ff8e80e1a01556a, 0xbc941330a56b5522, 0),
    ("asin", 0xbfe8000000000000, 0x0000000000000000, 0xbffb235315c680dc, 0xbc602b07b66c1abc, -1),
    ("asin", 0x3e45798ee2308c3a, 0x0000000000000000, 0x3ff5798ee2308c3a, 0x3c79ca58cce0be35, -27),
    ("acos", 0x3ff0000000000000, 0x0000000000000000, 0x0000000000000000, 0x0000000000000000, 0),
    ("acos", 0xbff0000000000000, 0x0000000000000000, 0x3ff921fb54442d18, 0x3c91a62633145c07, 1),
    ("acos", 0x3fefffffffffffff, 0x0000000000000000, 0x3ff0000000000000, 0x3c65555555555555, -26),
    ("acos", 0xbfefffffffffffff, 0x0000000000000000, 0x3ff921fb52442d18, 0x3c91a62632bf06b2, 1),
    ("acos", 0x3fe0000000000000, 0x0000000000000000, 0x3ff0c152382d7366, 0xbc9ee6913347c2a6, 0),
    ("acos", 0x3fdfffffffffffff, 0x0000000000000000, 0x3ff0c152382d7366, 0xbc88d9d3db6f1f14, 0),
    ("acos", 0xbfe0000000000000, 0x0000000000000000, 0x3ff0c152382d7366, 0xbc9ee6913347c2a6, 1),
    ("acos", 0x3e40000000000000, 0x0000000000000000, 0x3ff921fb52442d18, 0x3c91a62632bf06b2, 0),
    ("acos", 0x01a56e1fc2f8f359, 0x0000000000000000, 0x3ff921fb54442d18, 0x3c91a62633145c07, 0),
    ("acos", 0x0000000000000000, 0x0000000000000000, 0x3ff921fb54442d18, 0x3c91a62633145c07, 0),
    ("acos", 0x3fefff2e48e8a71e, 0x0000000000000000, 0x3ffcf69d216bd74b, 0x3c9cab6c3fd89443, -7),
    ("acos", 0xbfefff2e48e8a71e, 0x0000000000000000, 0x3ff90504b722c141, 0xbc53685392b7c8d6, 1),
    ("sinh", 0x3fc0000000000000, 0x0000000000000000, 0x3ff00aaccd00d2f1, 0xbc83ea29146349de, -3),
    ("sinh", 0x3fbffffffffffffe, 0x0000000000000000, 0x3ff00aaccd00d2f0, 0xbc84ea7e751a2555, -3),
    ("sinh", 0x4036000000000000, 0x0000000000000000, 0x3ffab5adb9c43600, 0xbc9f9ac8899634dd, 30),
    ("sinh", 0x4042000000000000, 0x0000000000000000, 0x3ffea215a1d20d76, 0x3c9d120d789d7f9c, 50),
    ("sinh", 0x4041ffffffffffff, 0x0000000000000000, 0x3ffea215a1d20d39, 0x3c880e791f2da766, 50),
    ("sinh", 0x408633ce8fb9f87d, 0x0000000000000000, 0x3ffffffffffffd3b, 0xbc7a6b164256494c, 1023),
    ("sinh", 0x408633c28f5c28f6, 0x0000000000000000, 0x3fffd02278f9df9b, 0x3c92c9e9659379db, 1023),
    ("sinh", 0x4086380000000000, 0x0000000000000000, 0x3ffb062ab00769f5, 0x3c83e9e64f28ef1c, 1024),
    ("sinh", 0x01a56e1fc2f8f359, 0x0000000000000000, 0x3ff56e1fc2f8f359, 0x0000000000000000, -997),
    ("sinh", 0x3e40000000000000, 0x0000000000000000, 0x3ff0000000000000, 0x3c65555555555555, -27),
    ("sinh", 0x3e45798ee2308c3a, 0x0000000000000000, 0x3ff5798ee2308c3a, 0x3c79ca58cce0be35, -27),
    ("sinh", 0xbfe0000000000000, 0x0000000000000000, 0xbff0acd00fe63b97, 0x3c8ae543b544f28d, -1),
    ("sinh", 0x40330f5c28f5c28f, 0x0000000000000000, 0x3ff697a96c26002b, 0xbc8453ff18412d3d, 26),
    ("sinh", 0x3fe62e42fefa39ef, 0x0000000000000000, 0x3ff8000000000000, 0xbc90b5e2e503f027, -1),
    ("cosh", 0x3fc0000000000000, 0x0000000000000000, 0x3ff0200aac16db6f, 0xbc809b4f99576fc1, 0),
    ("cosh", 0x3fbffffffffffffe, 0x0000000000000000, 0x3ff0200aac16db6f, 0xbc829ca532f78a1f, 0),
    ("cosh", 0x4036000000000000, 0x0000000000000000, 0x3ffab5adb9c43600, 0xbc9f879d40d7235f, 30),
    ("cosh", 0x4042000000000000, 0x0000000000000000, 0x3ffea215a1d20d76, 0x3c9d120d789d7fad, 50),
    ("cosh", 0x4041ffffffffffff, 0x0000000000000000, 0x3ffea215a1d20d39, 0x3c880e791f2da787, 50),
    ("cosh", 0x408633ce8fb9f87d, 0x0000000000000000, 0x3ffffffffffffd3b, 0xbc7a6b164256494c, 1023),
    ("cosh", 0x408633c28f5c28f6, 0x0000000000000000, 0x3fffd02278f9df9b, 0x3c92c9e9659379db, 1023),
    ("cosh", 0x4086380000000000, 0x0000000000000000, 0x3ffb062ab00769f5, 0x3c83e9e64f28ef1c, 1024),
    ("cosh", 0x01a56e1fc2f8f359, 0x0000000000000000, 0x3ff0000000000000, 0x0000000000000000, 0),
    ("cosh", 0x3e40000000000000, 0x0000000000000000, 0x3ff0000000000000, 0x3c80000000000000, 0),
    ("cosh", 0x3e45798ee2308c3a, 0x0000000000000000, 0x3ff0000000000000, 0x3c8cd2b297d889bd, 0),
    ("cosh", 0xbfe0000000000000, 0x0000000000000000, 0x3ff20ac1862ae8d0, 0x3c991608e93c1820, 0),
    ("cosh", 0x40330f5c28f5c28f, 0x0000000000000000, 0x3ff697a96c26002b, 0x3c88ff2ec39317fb, 26),
    ("cosh", 0x3fe62e42fefa39ef, 0x0000000000000000, 0x3ff4000000000000, 0xbc740d76ac6b202f, 0),
    ("tanh", 0x3fc0000000000000, 0x0000000000000000, 0x3fffd5992bc4b835, 0xbc4fa84a9b880414, -4),
    ("tanh", 0x3fbffffffffffffe, 0x0000000000000000, 0x3fffd5992bc4b833, 0x3c4fae8a71f9f0e8, -4),
    ("tanh", 0x4036000000000000, 0x0000000000000000, 0x4000000000000000, 0xbc16f741de1748ec, -1),
    ("tanh", 0x4042000000000000, 0x0000000000000000, 0x4000000000000000, 0xb99175af0cf60ec5, -1),
    ("tanh", 0x4041ffffffffffff, 0x0000000000000000, 0x4000000000000000, 0xb99175af0cf60f0b, -1),
    ("tanh", 0x408633ce8fb9f87d, 0x0000000000000000, 0x3ff0000000000000, 0x0000000000000000, 0),
    ("tanh", 0x408633c28f5c28f6, 0x0000000000000000, 0x3ff0000000000000, 0x0000000000000000, 0),
    ("tanh", 0x4086380000000000, 0x0000000000000000, 0x3ff0000000000000, 0x0000000000000000, 0),
    ("tanh", 0x01a56e1fc2f8f359, 0x0000000000000000, 0x3ff56e1fc2f8f359, 0x0000000000000000, -997),
    ("tanh", 0x3e40000000000000, 0x0000000000000000, 0x4000000000000000, 0xbc85555555555555, -28),
    ("tanh", 0x3e45798ee2308c3a, 0x0000000000000000, 0x3ff5798ee2308c3a, 0xbc89ca58cce0be35, -27),
    ("tanh", 0xbfe0000000000000, 0x0000000000000000, 0xbffd9353d7568af3, 0xbc9944a3b887196c, -2),
    ("tanh", 0x40330f5c28f5c28f, 0x0000000000000000, 0x3fffffffffffffff, 0x3c9fe69b67ca73ff, -1),
    ("tanh", 0x3fe62e42fefa39ef, 0x0000000000000000, 0x3ff3333333333333, 0x3c70fa16be6eb801, -1),
    ("cbrt", 0x7fefffffffffffff, 0x0000000000000000, 0x3ff428a2f98d728b, 0xbc94e7753b81f564, 341),
    ("cbrt", 0x0000000000000001, 0x0000000000000000, 0x3ff0000000000000, 0x0000000000000000, -358),
    ("cbrt", 0x0010000000000000, 0x0000000000000000, 0x3ff428a2f98d728b, 0xbc7ddc22548ea41e, -341),
    ("cbrt", 0x000fffffffffffff, 0x0000000000000000, 0x3ff428a2f98d728a, 0x3c9da81e1e1fbe3f, -341),
    ("cbrt", 0xc03b000000000000, 0x0000000000000000, 0xbff8000000000000, 0x0000000000000000, 1),
    ("cbrt", 0x4000000000000000, 0x0000000000000000, 0x3ff428a2f98d728b, 0xbc7ddc22548ea41e, 0),
    ("cbrt", 0x4010000000000000, 0x0000000000000000, 0x3ff965fea53d6e3d, 0xbc9f53e999952f09, 0),
    ("cbrt", 0x401ffffffffffffe, 0x0000000000000000, 0x3fffffffffffffff, 0x3c95555555555554, 0),
    ("cbrt", 0x01a56e1fc2f8f359, 0x0000000000000000, 0x3ffbff2ee48e0530, 0xbc777ae4ca2af9e3, -333),
    ("cbrt", 0x7e37e43c8800759c, 0x0000000000000000, 0x3ff249ad2594c37d, 0x3c40da9067103799, 332),
    ("cbrt", 0x800012688b70e62b, 0x0000000000000000, 0xbffa9d1b0b5d7427, 0x3c67220ca343c9c9, -344),
    ("cbrt", 0x3f50624dd2f1a9fc, 0x0000000000000000, 0x3ff999999999999a, 0xbc96666666666666, -4),
    ("cbrt", 0x4008000000000000, 0x0000000000000000, 0x3ff7137449123ef6, 0x3c973779fc5b15b9, 0),
    ("hypot", 0x7fefffffffffffff, 0x7fefffffffffffff, 0x3ff6a09e667f3bcc, 0xbc823ce1c161b7de, 1024),
    ("hypot", 0x7fefffffffffffff, 0x3ff0000000000000, 0x3fffffffffffffff, 0x0000000000000000, 1023),
    ("hypot", 0x0000000000000001, 0x0000000000000001, 0x3ff6a09e667f3bcd, 0xbc9bdd3413b26456, -1074),
    ("hypot", 0x7e37e43c8800759c, 0x01a56e1fc2f8f359, 0x3ff7e43c8800759c, 0x2d745ecfe5bf520b, 996),
    ("hypot", 0x00000000000017b8, 0x0000000000001fa0, 0x3ff3c40000000000, 0x0000000000000000, -1061),
    ("hypot", 0x4008000000000000, 0x4010000000000000, 0x3ff4000000000000, 0x0000000000000000, 2),
    ("hypot", 0x3ff0000000000000, 0x3e40000000000000, 0x3ff0000000000000, 0x3c80000000000000, 0),
    ("hypot", 0x3ff0000000000000, 0x3c30000000000000, 0x3ff0000000000000, 0x3860000000000000, 0),
    ("hypot", 0x3ff0000000000000, 0x3c20000000000000, 0x3ff0000000000000, 0x3840000000000000, 0),
    ("hypot", 0x5c10000000000000, 0x5c00000000000000, 0x3ff1e3779b97f4a8, 0xbc8f506319fcfd19, 450),
    ("hypot", 0x5c20000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x0000000000000000, 451),
    ("hypot", 0x23c0000000000000, 0x2330000000000000, 0x3ff00001ffffe000, 0x3c4ffffb0000e000, -451),
    ("hypot", 0x0010000000000000, 0x0010000000000000, 0x3ff6a09e667f3bcd, 0xbc9bdd3413b26456, -1022),
    ("hypot", 0x5fe7dddf6b095ff1, 0x5fe7dddf6b095ff1, 0x3ff0e0551a9edea1, 0x3c89c86aa7a78963, 512),
    ("atan2", 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff921fb54442d18, 0x3c91a62633145c07, -1),
    ("atan2", 0xbff0000000000000, 0xbff0000000000000, 0xbff2d97c7f3321d2, 0xbc8a79394c9e8a0a, 1),
    ("atan2", 0x01a56e1fc2f8f359, 0x7e37e43c8800759c, 0x3ffcb40954c56aa8, 0xbc9961447d090d2a, -1994),
    ("atan2", 0x7e37e43c8800759c, 0x01a56e1fc2f8f359, 0x3ff921fb54442d18, 0x3c91a62633145c07, 0),
    ("atan2", 0x3ff0000000000000, 0xffefffffffffffff, 0x3ff921fb54442d18, 0x3c91a62633145c07, 1),
    ("atan2", 0x0000000000000001, 0x3ff0000000000000, 0x3ff0000000000000, 0x0000000000000000, -1074),
    ("atan2", 0x3ff0000000000000, 0x41a0000000000000, 0x4000000000000000, 0xbc85555555555555, -28),
    ("atan2", 0xc000000000000000, 0xbbc79ca10c924223, 0xbff921fb54442d18, 0xbc91a684a5988e50, 0),
    ("atan2", 0x7fefffffffffffff, 0x7fefffffffffffff, 0x3ff921fb54442d18, 0x3c91a62633145c07, -1),
    ("atan2", 0x4008000000000000, 0xc010000000000000, 0x3ff3fc176b7a8560, 0xbc4441a3bd3f1083, 1),
    ("atan2", 0x000012688b70e62b, 0x000012688b70e62b, 0x3ff921fb54442d18, 0x3c91a62633145c07, -1),
    ("atan2", 0x3ff0000000000000, 0x43c0000000000000, 0x4000000000000000, 0xb845555555555555, -62),
    ("atan2", 0xbadf6ce95f8cd5c8, 0x3c9a296089b08259, 0xbff33820f929c707, 0xbc9e4ba6395d147a, -28),
    ("atan2", 0x01a56e1fc2f8f359, 0x052dbd86cd6238d9, 0x3ff70ef54646d497, 0x3c662594dc5b3c04, -57),
    ("atan2", 0x20b0000000000000, 0x2460000000000000, 0x4000000000000000, 0xb885555555555555, -60),
    ("atan2", 0x4008000000000000, 0x43a0000000000000, 0x3ff8000000000000, 0xb8b2000000000000, -58),
    ("atan2", 0x00000000000007e8, 0x009c16c5c5253575, 0x3ff203a27a37512d, 0x3c82d6d834e74fe4, -50),
    ("powf", 0x4000000000000000, 0x408fffffffffffff, 0x3ffffffffffffd3a, 0x3c8bd0105c706c87, 1023),
    ("powf", 0x4000000000000000, 0xc090c80000000000, 0x3ff0000000000000, 0x2db83096e0d3316d, -1074),
    ("powf", 0x4000000000000000, 0xc090ce0000000000, 0x3ff6a09e667f3bcd, 0xbc9bdd3413b26456, -1076),
    ("powf", 0x3fe0000000000000, 0x4090ca0000000000, 0x3ff6a09e667f3bcd, 0xbc9bdd3413b26456, -1075),
    ("powf", 0x3ff0000000000001, 0x4330000000000000, 0x3ff5bf0a8b145769, 0xbc96a8963377ad96, 1),
    ("powf", 0x4024000000000000, 0x4036000000000000, 0x3ff0f0cf064dd592, 0x2da97683df2f268d, 73),
    ("powf", 0x4024000000000000, 0xc072c00000000000, 0x3ff56e1fc2f8f359, 0xbc83592429cf5b75, -997),
    ("powf", 0x3fefffffffffffff, 0x43b0000000000000, 0x3ff42eb9f39afae3, 0xbc95394412a9aab0, -185),
    ("powf", 0xc020000000000000, 0x4008000000000000, 0xc000000000000000, 0x2d745ecfe5bf520b, 8),
    ("powf", 0xc000000000000000, 0xc090c40000000000, 0xc000000000000000, 0x2dc12fff69d96d39, -1074),
    ("powf", 0x7e37e43c8800759c, 0x3ff051eb851eb852, 0x3ff6c8e5ca239078, 0xbc72394a4f20983e, 1016),
    ("powf", 0x01a56e1fc2f8f359, 0x4003d70a3d70a3d7, 0x3ff666ae37746e28, 0xbc8a66394c8aeef4, -2472),
    ("powf", 0x401c000000000000, 0x3fe0000000000000, 0x3ff52a7fa9d2f8ea, 0xbc921c62b033c079, 1),
    ("powf", 0x4000000000000000, 0x3fe0000000000000, 0x3ff6a09e667f3bcd, 0xbc9bdd3413b26456, 0),
    ("powf", 0x7fefffffffffffff, 0x3fefffffffffffff, 0x3ffffffffffffd39, 0x3c8bd0105c7077a3, 1023),
    ("powf", 0x0000000000000001, 0x3fe0000000000000, 0x3ff0000000000000, 0x2dd318e2e7635cea, -537),
    ("powi", 0x3ff0000000000001, 0x412e848000000000, 0x3ff00000000f4240, 0x3bdd1a92b9c14078, 0),
    ("powi", 0x3fefffffffffffff, 0xc12e848000000000, 0x3ff000000007a120, 0x3bbd1a968a4ca03e, 0),
    ("powi", 0x4000000000000000, 0xc090c80000000000, 0x3ff0000000000000, 0x0000000000000000, -1074),
    ("powi", 0x4000000000000000, 0x408ff80000000000, 0x3ff0000000000000, 0x0000000000000000, 1023),
    ("powi", 0x4000000000000000, 0x4090000000000000, 0x3ff0000000000000, 0x0000000000000000, 1024),
    ("powi", 0xc008000000000000, 0x4044800000000000, 0xbfffa2a1cf67b5fc, 0x3c9e740000000000, 64),
    ("powi", 0x3ff199999999999a, 0xc08f400000000000, 0x3ff6927cd8545574, 0xbc80cd8513ba71b4, -138),
    ("powi", 0x3fb999999999999a, 0x4072c00000000000, 0x3ff56e1fc2f8f3bd, 0x3c9360a666d5b90e, -997),
    ("powi", 0x3fb999999999999a, 0xc073400000000000, 0x3ff1ccf385ebc84a, 0x3c91e5bb58a22884, 1023),
    ("powi", 0x4024000000000000, 0x4036000000000000, 0x3ff0f0cf064dd592, 0x0000000000000000, 73),
    ("powi", 0x4024000000000000, 0x4037000000000000, 0x3ff52d02c7e14af6, 0x3ca0000000000000, 76),
    ("powi", 0x7fefffffffffffff, 0x4000000000000000, 0x3ffffffffffffffe, 0x3960000000000000, 2047),
    ("powi", 0x0000000000000001, 0xbff0000000000000, 0x3ff0000000000000, 0x0000000000000000, 1074),
    ("powi", 0x0000000000000001, 0x4000000000000000, 0x3ff0000000000000, 0x0000000000000000, -2148),
    ("powi", 0x3ff8000000000000, 0x409b5c0000000000, 0x3ff348b9c58eb682, 0xbc791c8e784f4003, 1024),
    ("powi", 0xbfeccccccccccccd, 0x40bb590000000000, 0xbffc5ef6e23e340f, 0x3c5534389d89c820, -1065),
    ("powi", 0x4008000000000000, 0xc085380000000000, 0x3ffc0f74426a241b, 0xbc783b22a852ed0f, -1077),
    ("powi", 0x7fefffffffffffff, 0xbff0000000000000, 0x3ff0000000000001, 0xbc9fffffffffffff, -1024),
    ("powi", 0x3ff000001ad7f29b, 0x41dfffffffc00000, 0x3ffc2d024509add1, 0xbc9f1aa9c0ec35b0, 309),
    ("powi", 0x3feffffde7210be9, 0xc1dfffffffc00000, 0x3ff1f211e443b69b, 0xbc6bf6c80a5b4af3, 3098),
];
