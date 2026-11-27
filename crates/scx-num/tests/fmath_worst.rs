// Written for solidcorex. Apache-2.0.
//! `fmath` at CORE-MATH's worst cases: the inputs whose true results lie closest to a
//! halfway point between two doubles, found by the CORE-MATH project's searches (and
//! inputs that broke earlier implementations), from its `.wc` files (MIT; origin,
//! commit and how the subsets were taken in `tests/data/README.md`). Each result is
//! compared, bit for bit, with the correctly rounded one from [`scx_testkit::mp`], which
//! decides it by interval arithmetic at up to 4096 bits (double-double is not precise
//! enough here). A result must be that one or its neighbour, and the neighbours per file
//! are counted and pinned in the table at the end (at 0). Inputs the reference leaves to
//! C99's special values (zeros, infinities, NaNs, `pow` of a negative base) are counted
//! and left to `tests/fmath.rs`.
//!
//! The vendored subsets are run here. The full files (millions of inputs, hours) run with
//!
//! ```text
//! SCX_CORE_MATH_DIR=<core-math checkout>/src/binary64 \
//!     cargo test --release -p scx-num --test fmath_worst -- --ignored
//! ```

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::panic
)]

use scx_num::fmath;
use scx_testkit::mp;

/// One input line: `x` or `x,y`. Each number ends at the first blank, as CORE-MATH's
/// own reader (`sscanf` with `%la`) takes it (a few lines of the full files carry more).
fn parse(line: &str) -> Option<(f64, f64)> {
    let d = line.split('#').next()?.trim();
    if d.is_empty() {
        return None;
    }
    let num = |s: &str| {
        let s = s.split_whitespace().next().unwrap_or("");
        mp::parse_f64(s).unwrap_or_else(|| panic!("bad input {line:?}"))
    };
    let mut it = d.split(',');
    let x = num(it.next()?);
    let y = it.next().map_or(0.0, num);
    Some((x, y))
}

/// `(function, fmath's result, the reference)` for the `.wc` file `name` at `(x, y)`.
fn eval(name: &str, x: f64, y: f64) -> Vec<(&'static str, f64, Option<f64>)> {
    let one = |f: &'static str, got: f64, r: Option<f64>| vec![(f, got, r)];
    match name {
        "exp" => one("exp", fmath::exp(x), mp::exp(x)),
        "exp2" => one("exp2", fmath::exp2(x), mp::exp2(x)),
        "log" => one("ln", fmath::ln(x), mp::ln(x)),
        "log2" => one("log2", fmath::log2(x), mp::log2(x)),
        "log10" => one("log10", fmath::log10(x), mp::log10(x)),
        "sin" | "cos" => {
            // each file also checks the matching half of `sin_cos`
            let (s, c) = fmath::sin_cos(x);
            if name == "sin" {
                vec![
                    ("sin", fmath::sin(x), mp::sin(x)),
                    ("sin_cos.sin", s, mp::sin(x)),
                ]
            } else {
                vec![
                    ("cos", fmath::cos(x), mp::cos(x)),
                    ("sin_cos.cos", c, mp::cos(x)),
                ]
            }
        }
        "tan" => one("tan", fmath::tan(x), mp::tan(x)),
        "asin" => one("asin", fmath::asin(x), mp::asin(x)),
        "acos" => one("acos", fmath::acos(x), mp::acos(x)),
        "atan" => one("atan", fmath::atan(x), mp::atan(x)),
        // the file's lines are `y,x`, as C's `atan2(y, x)`
        "atan2" => one("atan2", fmath::atan2(x, y), mp::atan2(x, y)),
        "sinh" => one("sinh", fmath::sinh(x), mp::sinh(x)),
        "cosh" => one("cosh", fmath::cosh(x), mp::cosh(x)),
        "tanh" => one("tanh", fmath::tanh(x), mp::tanh(x)),
        "cbrt" => one("cbrt", fmath::cbrt(x), mp::cbrt(x)),
        "hypot" => one("hypot", fmath::hypot(x, y), mp::hypot(x, y)),
        "pow" => one("powf", fmath::powf(x, y), mp::pow(x, y)),
        _ => panic!("no fmath function for {name}.wc"),
    }
}

fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

/// What one `.wc` file gave.
#[derive(Debug, Default)]
struct Outcome {
    /// Input lines.
    inputs: usize,
    /// Results the reference decided (two per `sin`/`cos` line, with `sin_cos`).
    decided: usize,
    /// Results left to C99's special values, which `tests/fmath.rs` checks.
    special: usize,
    /// Results one ulp from the correctly rounded one: the known `fmath` bug (see the
    /// table at the end).
    misrounded: usize,
}

/// Checks every input of a `.wc` file's text. A result that is neither the correctly
/// rounded one nor its neighbour fails at once; neighbours are counted.
fn check(name: &str, text: &str) -> Outcome {
    let mut o = Outcome::default();
    let mut bad = Vec::new();
    for line in text.lines() {
        let Some((x, y)) = parse(line) else { continue };
        o.inputs += 1;
        for (f, got, reference) in eval(name, x, y) {
            let Some(want) = reference else {
                o.special += 1;
                continue;
            };
            o.decided += 1;
            if same(got, want) {
                continue;
            }
            let neighbour = got.is_finite()
                && (got.next_up().to_bits() == want.to_bits()
                    || got.next_down().to_bits() == want.to_bits());
            if neighbour {
                o.misrounded += 1;
            } else {
                bad.push(format!(
                    "{f}({x:e}, {y:e}) [{line}]: got {got:e} ({:#018x}), want {want:e} ({:#018x})",
                    got.to_bits(),
                    want.to_bits()
                ));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{name}.wc: {} wrong of {}:\n{}",
        bad.len(),
        o.inputs,
        bad.join("\n")
    );
    assert!(o.inputs > 0, "{name}.wc: no inputs");
    o
}

macro_rules! worst {
    ($($test:ident => $name:literal, $known:expr;)*) => {
        $(
            #[test]
            fn $test() {
                let o = check($name, include_str!(concat!("data/core-math/", $name, ".wc")));
                assert_eq!(
                    o.misrounded, $known,
                    "{}.wc: misrounded results changed (see the table): {o:?}", $name
                );
            }
        )*

        /// The full files from a CORE-MATH checkout (see the module notes).
        #[test]
        #[ignore = "hours: the full CORE-MATH files; set SCX_CORE_MATH_DIR"]
        fn full_core_math_files() {
            let dir = std::env::var("SCX_CORE_MATH_DIR").expect("SCX_CORE_MATH_DIR");
            let mut failed = Vec::new();
            for name in [$($name),*] {
                let text = std::fs::read_to_string(format!("{dir}/{name}/{name}.wc"))
                    .unwrap_or_else(|e| panic!("{name}.wc: {e}"));
                // every file runs; the failures are listed at the end
                match std::panic::catch_unwind(|| check(name, &text)) {
                    Ok(o) => eprintln!("{name}: {o:?}"),
                    Err(_) => failed.push(name),
                }
            }
            assert!(failed.is_empty(), "failed: {failed:?}");
        }
    };
}

// The second number is how many results in the subset misround (per result, so two per
// `sin`/`cos` input): 0 for every function, and a rise is a regression. Before each
// function had its rounding test with a more precise fallback (research note 70), it
// rounded once a result carrying 68 to 104 bits, which erred where the true value lies
// that close to a halfway point, and `powf` also misrounded exact halfway points
// (`powf(194701.5625, 2.5)`, `powf(1781, 5)`): 3174 results here, each the neighbour of
// the correct one.
worst! {
    exp => "exp", 0;
    exp2 => "exp2", 0;
    ln => "log", 0;
    log2 => "log2", 0;
    log10 => "log10", 0;
    sin_and_sin_cos => "sin", 0;
    cos_and_sin_cos => "cos", 0;
    tan => "tan", 0;
    asin => "asin", 0;
    acos => "acos", 0;
    atan => "atan", 0;
    atan2 => "atan2", 0;
    sinh => "sinh", 0;
    cosh => "cosh", 0;
    tanh => "tanh", 0;
    cbrt => "cbrt", 0;
    hypot => "hypot", 0;
    powf => "pow", 0;
}
