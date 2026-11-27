// Written for solidcorex. Apache-2.0.
//! [`eft::fma`] against Berkeley TestFloat 3e's `f64_mulAdd` test cases (BSD-3-Clause;
//! origin, commits and how the cases were taken in `tests/data/README.md`): every result
//! must be the expected one bit for bit, with any NaN matching any NaN (IEEE 754 leaves
//! its payload and sign open; TestFloat's is the x86 default). The exception flags are
//! not checked: Rust does not expose them.
//!
//! The same operands also check [`eft::two_sum`] and [`eft::two_prod`]: `a + b = s + e`
//! and `a * b = p + e` exactly, as exact rationals, wherever their docs promise it. (All of
//! level 1 found `two_sum`'s `s - a` overflowing at 316 pairs its docs then promised, now
//! named there.)
//!
//! Level 1 (every 1,533rd of its 6,133,248 cases) and the first 2,000 of level 2 run here.
//! A whole generated file (all of level 1 is 6,133,248 lines, 436 MB) runs with
//!
//! ```text
//! testfloat_gen -seed 1 -level 1 f64_mulAdd > cases.txt
//! SCX_TESTFLOAT_CASES=cases.txt cargo test --release -p scx-num --test testfloat_muladd -- --ignored
//! ```

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::panic
)]

use scx_num::eft;
use scx_testkit::rat::Rat;

/// `(a, b, c, result)` from one line `A B C RESULT FLAGS` (hex bit patterns).
fn parse(line: &str) -> Option<[f64; 4]> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let f: Vec<&str> = line.split_whitespace().collect();
    assert_eq!(f.len(), 5, "bad line {line:?}");
    let bits = |s: &str| {
        f64::from_bits(u64::from_str_radix(s, 16).unwrap_or_else(|e| panic!("{line:?}: {e}")))
    };
    Some([bits(f[0]), bits(f[1]), bits(f[2]), bits(f[3])])
}

/// `x ∘ y == r + e` exactly, `∘` being `+` or `*` (`false` for a non-finite `r` or `e`).
fn exact(x: f64, y: f64, prod: bool, r: f64, e: f64) -> bool {
    let q = |v: f64| Rat::from_f64(v);
    let (Some(x), Some(y), Some(r), Some(e)) = (q(x), q(y), q(r), q(e)) else {
        return false;
    };
    (if prod { &x * &y } else { &x + &y }) == &r + &e
}

/// Checks every case; returns `(cases, two_sum checks, two_sum overflows, two_prod checks)`.
fn check(name: &str, text: &str) -> (usize, usize, usize, usize) {
    let (mut cases, mut sums, mut overflows, mut prods) = (0, 0, 0, 0);
    let mut bad = Vec::new();
    for line in text.lines() {
        let Some([a, b, c, want]) = parse(line) else {
            continue;
        };
        cases += 1;
        let got = eft::fma(a, b, c);
        if !(got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan())) {
            bad.push(format!(
                "fma({a:e}, {b:e}, {c:e}) [{line}]: got {:016X}, want {:016X}",
                got.to_bits(),
                want.to_bits()
            ));
        }
        for (x, y) in [(a, b), (b, c), (a, c)] {
            if x.is_finite() && y.is_finite() && (x + y).is_finite() {
                // exact unless `s - x` overflows, which its docs allow only for `|x| < |y|`
                let (s, e) = eft::two_sum(x, y);
                if (s - x).is_finite() {
                    if !exact(x, y, false, s, e) {
                        bad.push(format!("two_sum({x:e}, {y:e}) [{line}]: ({s:e}, {e:e})"));
                    }
                    sums += 1;
                } else if x.abs() >= y.abs() {
                    bad.push(format!("two_sum({x:e}, {y:e}) [{line}]: s - a overflowed"));
                } else {
                    overflows += 1;
                }
            }
            // exact when nothing overflows and the error does not underflow
            let p = x * y;
            let tiny = f64::from_bits(54 << 52); // 2^-969
            if p.is_finite() && (x == 0.0 || y == 0.0 || p.abs() > tiny) {
                let (p, e) = eft::two_prod(x, y);
                if !exact(x, y, true, p, e) {
                    bad.push(format!("two_prod({x:e}, {y:e}) [{line}]: ({p:e}, {e:e})"));
                }
                prods += 1;
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{name}: {} wrong of {cases}:\n{}",
        bad.len(),
        bad.join("\n")
    );
    (cases, sums, overflows, prods)
}

#[test]
fn level_1() {
    let counts = check(
        "level 1",
        include_str!("data/testfloat/f64_mulAdd_level1.txt"),
    );
    assert_eq!(counts.0, 4001);
    eprintln!("(cases, two_sum, two_sum overflows, two_prod) = {counts:?}");
}

#[test]
fn level_2() {
    let counts = check(
        "level 2",
        include_str!("data/testfloat/f64_mulAdd_level2.txt"),
    );
    assert_eq!(counts.0, 2000);
    eprintln!("(cases, two_sum, two_sum overflows, two_prod) = {counts:?}");
}

/// A whole `testfloat_gen` output (see the module notes).
#[test]
#[ignore = "a whole TestFloat level (level 1: 20 s in release); set SCX_TESTFLOAT_CASES"]
fn generated_file() {
    let path = std::env::var("SCX_TESTFLOAT_CASES").expect("SCX_TESTFLOAT_CASES");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let counts = check(&path, &text);
    eprintln!("(cases, two_sum, two_sum overflows, two_prod) = {counts:?}");
}

/// The first pair the whole of level 1 found (its `a, b`): `s - a` overflows, as
/// [`eft::two_sum`]'s docs say it may for `|a| < |b|`, and with the operands swapped the
/// result is exact.
#[test]
fn two_sum_overflow_named_in_docs() {
    let a = f64::from_bits(0xFFD0_0004_00FF_FFFF);
    let b = f64::MAX;
    let (s, e) = eft::two_sum(a, b);
    assert!(s.is_finite() && e.is_nan(), "({s:e}, {e:e})");
    let (s, e) = eft::two_sum(b, a);
    assert!(exact(b, a, false, s, e), "({s:e}, {e:e})");
}
