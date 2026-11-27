// Written for solidcorex. Apache-2.0.
//! The check that keeps each Ziv fast path (`cos`, `acos`, `cbrt`) from changing a result:
//! the function against its double-double path alone, bit for bit, on seeded random
//! inputs dense at the hard places, and on published hard cases when given.
//!
//! The default run is short. The long one is `#[ignore]`d, for release builds:
//! `FMATH_FAST_N` sets the random inputs per function (default 10^8), and
//! `FMATH_HARD_DIR` names a directory holding CORE-MATH's worst-case lists (`cos.wc`,
//! `acos.wc`, `cbrt.wc`, MIT licence, from its `src/binary64/<function>/`), one hex
//! float per line, which are then run too.

use super::dd;
use crate::hash::Rng;

/// Counts for one function: inputs, how many the fast path's test accepted, and the
/// largest `|fast - dd| / |dd|` seen between the two unrounded values.
#[derive(Default)]
pub(super) struct Stats {
    pub(super) n: u64,
    pub(super) fast: u64,
    pub(super) max_rel: f64,
}

impl Stats {
    /// Records one input: the fast pair, whether its test passed, and the double-double
    /// pair.
    pub(super) fn add(&mut self, fast: (f64, f64), passed: bool, slow: (f64, f64)) {
        self.n += 1;
        self.fast += u64::from(passed);
        let d = ((fast.0 - slow.0) + (fast.1 - slow.1)).abs() / slow.0.abs();
        if d > self.max_rel {
            self.max_rel = d;
        }
    }

    /// One line for the log.
    pub(super) fn report(&self, name: &str) {
        let rate = self.fast as f64 / self.n.max(1) as f64;
        println!(
            "{name}: {} inputs, fast path accepted {:.4}%, largest fast-vs-dd relative \
             difference 2^{:.1}",
            self.n,
            100.0 * rate,
            self.max_rel.log2()
        );
    }
}

/// Random inputs per function: `FMATH_FAST_N`, else `default`.
pub(super) fn count(default: u64) -> u64 {
    std::env::var("FMATH_FAST_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// The hard cases in `$FMATH_HARD_DIR/<name>.wc`, if the variable is set.
pub(super) fn hard_cases(name: &str) -> Vec<f64> {
    let Ok(dir) = std::env::var("FMATH_HARD_DIR") else {
        return Vec::new();
    };
    let path = std::path::Path::new(&dir).join(format!("{name}.wc"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| parse_hex(l).unwrap_or_else(|| panic!("{path:?}: {l}")))
        .collect()
}

/// A C99 hex float (`-0x1.8p-3`, `0x.fp-1022`), a small integer (`+0`, `-2`), `inf`,
/// `nan` or `snan`, each with an optional sign.
fn parse_hex(s: &str) -> Option<f64> {
    let (neg, s) = match s.as_bytes().first()? {
        b'-' => (true, &s[1..]),
        b'+' => (false, &s[1..]),
        _ => (false, s),
    };
    let v = match s {
        "inf" => f64::INFINITY,
        // Signalling NaNs become quiet ones; no function here tells them apart.
        "nan" | "snan" => f64::NAN,
        _ if !s.starts_with("0x") => f64::from(s.parse::<u8>().ok()?),
        _ => {
        let s = s.strip_prefix("0x")?;
        let (mant, exp) = s.split_once('p')?;
        let (int, frac) = mant.split_once('.').unwrap_or((mant, ""));
        let digits = format!("{int}{frac}");
        let m = u64::from_str_radix(&digits, 16).ok()?;
        let e: i32 = exp.parse().ok()?;
        // m below 2^56 here; every list's values have 53 bits or fewer, so m is exact.
        dd::scale(m as f64, e - 4 * frac.len() as i32)
        }
    };
    Some(if neg { -v } else { v })
}

/// A uniform draw in `[lo, hi)`.
pub(super) fn uniform(r: &mut Rng, lo: f64, hi: f64) -> f64 {
    lo + (hi - lo) * r.next_f64()
}

/// `2^e` times a uniform significand, `e` uniform in `[lo, hi)`, random sign.
pub(super) fn log_uniform(r: &mut Rng, lo: f64, hi: f64) -> f64 {
    let e = uniform(r, lo, hi).floor() as i32;
    let v = dd::scale(1.0 + r.next_f64(), e);
    if r.next_u64() & 1 == 0 { v } else { -v }
}

/// `x` moved by `k` units in the last place.
pub(super) fn ulps(x: f64, k: i64) -> f64 {
    f64::from_bits((x.to_bits() as i64 + k) as u64)
}

#[test]
fn hex_floats_parse() {
    assert_eq!(parse_hex("0x1.8p-1"), Some(0.75));
    assert_eq!(parse_hex("-0x1p+0"), Some(-1.0));
    assert_eq!(parse_hex("+0x1p-1074"), Some(f64::from_bits(1)));
    assert_eq!(parse_hex("+0x.fffffffffffffp-1022"), Some(f64::from_bits((1 << 52) - 1)));
    assert_eq!(parse_hex("+0x1.fffffffffffffp+1023"), Some(f64::MAX));
    assert_eq!(parse_hex("+0").map(f64::to_bits), Some(0));
    assert_eq!(parse_hex("-0").map(f64::to_bits), Some((-0.0f64).to_bits()));
    assert_eq!(parse_hex("-2"), Some(-2.0));
    assert_eq!(parse_hex("-inf"), Some(f64::NEG_INFINITY));
    assert!(parse_hex("+snan").is_some_and(f64::is_nan));
}
