// Written for solidcorex. Apache-2.0.
//! The polynomial root solvers on the classic hard families, against the exact truth.
//!
//! - Wilkinson's `prod (x - k)`, also scaled to `10^-6` and `10^6`, and his second,
//!   `prod (x - 2^-k)`.
//! - Mignotte's `x^n - 2 (a x - 1)^2`, with two roots within about `2 a^(-(n+2)/2)` of
//!   each other.
//! - Chebyshev's `T_n`.
//! - Clusters of roots, including research 53's quartic.
//! - Near-double roots `r` and `r (1 + 2^-k)`.
//!
//! Each polynomial is built exactly in rationals and rounded once to doubles, as a caller
//! would hold it. The truth is the real roots of the rounded polynomial, isolated with
//! certainty by `scx_testkit::poly` (Sturm sequences) to `2^-100`.
//!
//! `solve_cubic` and `solve_quartic` take the power basis, at the `zero_tol` callers pass
//! (`EFF_ZERO`). Every case must meet their contract (module docs): each true root has a
//! returned root within half an ulp plus its pseudo-zero radius, and each returned root is
//! that near a true root or is a pseudo-zero. A separated simple root must be correctly
//! rounded, and `2^a p(2^b x)` must give the same roots scaled, bit for bit.
//!
//! `bernstein_roots` takes the Bernstein coefficients on `[0, 1]`, of degrees up to 20.
//! Its claim is a place certified by a sign change of de Casteljau's value, so each true
//! root in `[0, 1]` must have a place within de Casteljau's error over the slope (see
//! [`bound`]), each place must be that near a true root, and the distinct places must
//! number the true roots.
//!
//! `SCX_PRINT_POLY_FAMILIES=1 cargo test -p scx-algo --test polynomial_families -- --nocapture`
//! prints each case.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::panic
)]

use scx_algo::bernstein::{RootLimits, bernstein_roots};
use scx_algo::polynomial::{solve_cubic, solve_quartic};
use scx_num::math::EFF_ZERO;
use scx_testkit::Rat;
use scx_testkit::poly::Poly;

fn rat(x: f64) -> Rat {
    Rat::from_f64(x).unwrap_or_else(|| panic!("{x} is not finite"))
}

/// `10^k`, exact for `k >= 0` (to 22) and one correctly rounded division below.
fn ten(k: i32) -> f64 {
    let p = (0..k.unsigned_abs()).fold(1.0, |p, _| p * 10.0);
    if k < 0 { 1.0 / p } else { p }
}

fn int(n: i64) -> Rat {
    Rat::from(n)
}

fn q(n: i64, d: i64) -> Rat {
    int(n)
        .checked_div(&int(d))
        .unwrap_or_else(|| panic!("{n}/{d}"))
}

/// `lead * prod (x - r)`, exact.
fn from_roots(roots: &[Rat], lead: &Rat) -> Poly {
    roots.iter().fold(Poly::constant(lead.clone()), |p, r| {
        p.mul(&Poly::linear(-r, Rat::one()))
    })
}

/// The coefficients of `p`, each rounded to the nearest double.
fn rounded(p: &Poly) -> Vec<f64> {
    p.coeffs().iter().map(Rat::to_f64).collect()
}

/// The power-basis polynomial with these double coefficients, exactly.
fn exact(c: &[f64]) -> Poly {
    Poly::new(c.iter().map(|&x| rat(x)).collect())
}

/// The true distinct real roots of `p`, ascending, each the midpoint of an enclosure of
/// width at most `2^-100`, with its multiplicity.
fn truth(p: &Poly) -> Vec<(Rat, u32)> {
    let width = Rat::one().scale(-100);
    p.real_roots(&width)
        .into_iter()
        .map(|r| (r.at.mid(), r.multiplicity))
        .collect()
}

fn ulp(x: f64) -> f64 {
    let a = x.abs();
    a.next_up() - a
}

/// `(distance to the nearest true root, that root)` for a returned `x`.
fn nearest(x: f64, roots: &[(Rat, u32)]) -> Option<(f64, f64, u32)> {
    let rx = rat(x);
    roots
        .iter()
        .map(|(r, m)| ((&rx - r).abs(), r, *m))
        .min_by(|a, b| a.0.cmp(&b.0))
        .map(|(d, r, m)| (d.to_f64(), r.to_f64(), m))
}

fn printing() -> bool {
    std::env::var_os("SCX_PRINT_POLY_FAMILIES").is_some()
}

// ---- power basis: solve_cubic and solve_quartic ----

/// One case for the closed forms: the double coefficients, lowest first.
struct Case {
    name: String,
    c: Vec<f64>,
}

fn case(name: impl Into<String>, p: &Poly) -> Case {
    Case {
        name: name.into(),
        c: rounded(p),
    }
}

fn solve(c: &[f64]) -> Vec<f64> {
    solve_at(c, EFF_ZERO)
}

fn solve_at(c: &[f64], zero_tol: f64) -> Vec<f64> {
    match c.len() {
        4 => solve_cubic(&[c[0], c[1], c[2], c[3]], zero_tol).to_vec(),
        5 => solve_quartic(&[c[0], c[1], c[2], c[3], c[4]], zero_tol).to_vec(),
        n => panic!("degree {}", n - 1),
    }
}

/// The contract's `η` (module docs of `scx_algo::polynomial`).
const ETA: f64 = 8.077935669463161e-28; // 2^-90

/// `Σ |c_i| |x|^i`.
fn size(c: &[f64], x: f64) -> f64 {
    c.iter().rev().fold(0.0, |s, a| s * x.abs() + a.abs())
}

/// `(k! η S(r) / |p^(k)(r)|)^(1/k)`, least over `k`: the pseudo-zero radius at `r`.
fn radius(p: &Poly, c: &[f64], r: &Rat) -> f64 {
    let s = size(c, r.to_f64());
    let (mut d, mut fact, mut best) = (p.clone(), 1.0, f64::INFINITY);
    for k in 1..c.len() {
        d = d.derivative();
        fact *= k as f64;
        let v = d.eval(r).to_f64().abs();
        if v > 0.0 {
            let x = fact * ETA * s / v;
            let root = match k {
                1 => x,
                2 => x.sqrt(),
                3 => scx_num::fmath::cbrt(x),
                _ => x.sqrt().sqrt(),
            };
            best = best.min(root);
        }
    }
    best
}

/// The contract's bound at the true root `r`: half an ulp plus the pseudo-zero radius.
fn root_bound(p: &Poly, c: &[f64], r: &Rat) -> f64 {
    0.5 * ulp(r.to_f64()) + radius(p, c, r)
}

/// Every way `solve` breaks the contract on `c`, and the returned roots.
fn breaches(name: &str, c: &[f64]) -> (Vec<String>, Vec<f64>) {
    let p = exact(c);
    let t = truth(&p);
    let got = solve(c);
    let mut out = Vec::new();
    if got.len() > c.len() - 1
        || got
            .windows(2)
            .any(|w| w[0].partial_cmp(&w[1]) != Some(core::cmp::Ordering::Less))
    {
        out.push(format!("{name}: not distinct and ascending: {got:?}"));
    }
    let dist = |x: f64, r: &Rat| (&rat(x) - r).abs().to_f64();
    for (r, m) in &t {
        let b = root_bound(&p, c, r);
        let near = got
            .iter()
            .map(|&x| dist(x, r))
            .fold(f64::INFINITY, f64::min);
        if near > b {
            out.push(format!(
                "{name}: true root {:e} (multiplicity {m}) has none within {b:.3e} (nearest {near:.3e})",
                r.to_f64()
            ));
        }
    }
    for &x in &got {
        if !x.is_finite() {
            out.push(format!("{name}: returned {x}"));
            continue;
        }
        let near = t.iter().any(|(r, _)| dist(x, r) <= root_bound(&p, c, r));
        let small = p.eval(&rat(x)).to_f64().abs() <= ETA * size(c, x);
        if !(near || small) {
            out.push(format!("{name}: returned {x:e} is no root"));
        }
    }
    (out, got)
}

fn print_case(k: &Case) {
    let t = truth(&exact(&k.c));
    let got = solve(&k.c);
    let tv: Vec<(f64, u32)> = t.iter().map(|(r, m)| (r.to_f64(), *m)).collect();
    let ulps: Vec<f64> = got
        .iter()
        .map(|&x| match nearest(x, &t) {
            Some((d, r, _)) => d / ulp(r),
            None => f64::NAN,
        })
        .collect();
    println!("{}: true {tv:?}", k.name);
    println!("    got {got:?}");
    println!("    ulps {ulps:.3?}");
}

fn wilkinson(n: i64) -> Poly {
    let r: Vec<Rat> = (1..=n).map(int).collect();
    from_roots(&r, &Rat::one())
}

/// `x^n - 2 (a x - 1)^2`.
fn mignotte(n: usize, a: &Rat) -> Poly {
    let mut c = vec![Rat::zero(); n + 1];
    c[n] = Rat::one();
    let sq = Poly::linear(-&Rat::one(), a.clone()).pow(2).scale(&int(2));
    Poly::new(c).sub(&sq)
}

/// Chebyshev's `T_n` (integer coefficients).
fn chebyshev(n: usize) -> Poly {
    let x = Poly::linear(Rat::zero(), Rat::one());
    let (mut a, mut b) = (Poly::constant(Rat::one()), x.clone());
    if n == 0 {
        return a;
    }
    for _ in 1..n {
        let c = x.scale(&int(2)).mul(&b).sub(&a);
        a = b;
        b = c;
    }
    b
}

/// Research 53 (section 9, item 2): Ferrari's method gave forward errors up to `3e-3`.
const R53_QUARTIC: [f64; 5] = [
    -9.389680767756892e-6,
    0.0015388801638414014,
    -0.05843609829791384,
    0.6006785459030644,
    1.0,
];

fn closed_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for n in [3, 4] {
        cases.push(case(format!("wilkinson {n}"), &wilkinson(n)));
        cases.push(case(format!("chebyshev {n}"), &chebyshev(n as usize)));
        // Wilkinson's second: roots 2^-1 .. 2^-n, exact
        let r: Vec<Rat> = (1..=n).map(|k| Rat::one().scale(-k)).collect();
        cases.push(case(format!("geometric {n}"), &from_roots(&r, &Rat::one())));
        // roots k/n scaled to 10^s
        for s in [-6, 0, 6] {
            let r: Vec<Rat> = (1..=n).map(|k| &q(k, n) * &rat(ten(s))).collect();
            cases.push(case(
                format!("wilkinson {n} at 1e{s}"),
                &from_roots(&r, &Rat::one()),
            ));
        }
        for a in [10, 100, 1000] {
            cases.push(case(
                format!("mignotte {n}, a = {a}"),
                &mignotte(n as usize, &int(a)),
            ));
        }
        for j in [2, 4, 6] {
            let d = rat(ten(-j));
            let r: Vec<Rat> = (0..n).map(|k| &int(1) + &(&int(k) * &d)).collect();
            cases.push(case(
                format!("cluster {n} at 1, step 1e-{j}"),
                &from_roots(&r, &Rat::one()),
            ));
        }
        for k in [10, 20, 30, 40] {
            let r0 = q(7, 10);
            let r1 = &r0 * &(&Rat::one() + &Rat::one().scale(-k));
            let mut r = vec![r0, r1];
            r.extend((3..=n).map(|i| int(-i)));
            cases.push(case(
                format!("near-double {n}, 2^-{k}"),
                &from_roots(&r, &Rat::one()),
            ));
        }
    }
    cases.push(Case {
        name: "research 53 quartic".into(),
        c: R53_QUARTIC.to_vec(),
    });
    // exact multiple roots, and complex pairs near the axis
    let x2 = |r: Rat| Poly::linear(-r, Rat::one()).pow(2);
    let quad = |a: Rat, b2: Rat| {
        // (x - a)^2 + b2
        x2(a).add(&Poly::constant(b2))
    };
    let lin = |r: Rat| Poly::linear(-r, Rat::one());
    let multiple: [(&str, Poly); 12] = [
        ("(x - 1)^2 (x + 2)", x2(int(1)).mul(&lin(int(-2)))),
        ("(x - 1)^3", lin(int(1)).pow(3)),
        ("(x - 0.1)^3", lin(q(1, 10)).pow(3)),
        ("(x - 1/3)^2 (x - 3)", x2(q(1, 3)).mul(&lin(int(3)))),
        ("(x - 1)^4", lin(int(1)).pow(4)),
        ("(x - 1)^2 (x + 2)^2", x2(int(1)).mul(&x2(int(-2)))),
        (
            "(x - 0.5)^3 (x + 3)",
            lin(q(1, 2)).pow(3).mul(&lin(int(-3))),
        ),
        (
            "(x - 0.7)^2 (x - 2) (x + 1)",
            x2(q(7, 10)).mul(&lin(int(2))).mul(&lin(int(-1))),
        ),
        (
            "((x - 1)^2 + 2^-60) (x + 2)",
            quad(int(1), Rat::one().scale(-60)).mul(&lin(int(-2))),
        ),
        (
            "((x - 1)^2 + 2^-60) (x - 3)^2",
            quad(int(1), Rat::one().scale(-60)).mul(&x2(int(3))),
        ),
        (
            "((x - 1)^2 + 2^-30) ((x + 1)^2 + 2^-30)",
            quad(int(1), Rat::one().scale(-30)).mul(&quad(int(-1), Rat::one().scale(-30))),
        ),
        // no real root, and a critical point where the derivative has a triple root, so
        // its curvature tells nothing of how near the critical point is (the difftest's
        // `(x + 3)^4 + 1e-13`)
        (
            "(x + 3)^4 + 2^-43",
            lin(int(-3))
                .pow(4)
                .add(&Poly::constant(Rat::one().scale(-43))),
        ),
    ];
    for (name, p) in multiple {
        cases.push(case(name, &p));
    }
    cases
}

// Every case meets the contract of `scx_algo::polynomial` (module docs) against the exact
// truth: each true root has a returned root within half an ulp plus its pseudo-zero radius,
// each returned root is that near a true root or a pseudo-zero, and the roots are
// distinct and ascending. Before research 71 the closed forms broke it on 20 of the 33
// family cases (absolute tolerances on Cardano's discriminant and Ferrari's `u`, `v`).
#[test]
fn closed_forms_meet_the_contract() {
    let mut failed = Vec::new();
    for k in &closed_cases() {
        if printing() {
            print_case(k);
        }
        failed.extend(breaches(&k.name, &k.c).0);
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

// A simple, separated root is the true root correctly rounded (within half an ulp), not
// only within the contract's bound.
#[test]
fn separated_roots_are_correctly_rounded() {
    let names = [
        "wilkinson 3",
        "chebyshev 3",
        "geometric 3",
        "wilkinson 3 at 1e-6",
        "wilkinson 3 at 1e0",
        "wilkinson 3 at 1e6",
        "wilkinson 4",
        "chebyshev 4",
        "geometric 4",
        "wilkinson 4 at 1e-6",
        "wilkinson 4 at 1e0",
        "wilkinson 4 at 1e6",
        "research 53 quartic",
        "mignotte 3, a = 10",
        "mignotte 4, a = 10",
    ];
    for k in closed_cases()
        .iter()
        .filter(|k| names.contains(&k.name.as_str()))
    {
        let t = truth(&exact(&k.c));
        let got = solve(&k.c);
        assert_eq!(got.len(), t.len(), "{}", k.name);
        for (&x, (r, _)) in got.iter().zip(&t) {
            let d = (&rat(x) - r).abs().to_f64();
            assert!(
                d <= 0.5 * ulp(r.to_f64()),
                "{}: {x:e} is {d:e} from {:e}",
                k.name,
                r.to_f64()
            );
        }
    }
}

/// `2^k`, for `k` in the normal range.
fn pow2(k: i32) -> f64 {
    f64::from_bits(((1023 + k) as u64) << 52)
}

// No absolute threshold past the degree drop: `2^a p(x / 2^b)` has the roots of `p` times
// `2^b`, bit for bit (at `zero_tol = 0`, so that the degree stays).
#[test]
fn closed_forms_scale_exactly() {
    for k in &closed_cases() {
        let base = solve(&k.c);
        for (a, b) in [(-40, 3), (25, -7), (0, 20), (300, -60), (-500, 40)] {
            let c: Vec<f64> =
                k.c.iter()
                    .enumerate()
                    .map(|(i, &v)| v * pow2(a - b * i as i32))
                    .collect();
            let scaled: Vec<f64> = solve_at(&c, 0.0).iter().map(|&x| x * pow2(-b)).collect();
            assert_eq!(scaled, base, "{} at 2^{a}, 2^{b}", k.name);
        }
    }
}

// ---- Bernstein basis: bernstein_roots on [0, 1] ----

fn binom(n: usize, k: usize) -> Rat {
    let mut c = Rat::one();
    for i in 0..k {
        c = &(&c * &int((n - i) as i64)) * &q(1, (i + 1) as i64);
    }
    c
}

/// The Bernstein coefficients of degree `n` of the power-basis `p` (degree at most `n`) on
/// `[0, 1]`, exactly: `b_j = sum_(i <= j) C(j, i) / C(n, i) a_i`.
fn to_bernstein(p: &Poly, n: usize) -> Vec<Rat> {
    let a = p.coeffs();
    (0..=n)
        .map(|j| {
            (0..=j.min(a.len().saturating_sub(1))).fold(Rat::zero(), |s, i| {
                let w = binom(j, i).checked_div(&binom(n, i)).unwrap_or_default();
                &s + &(&w * &a[i])
            })
        })
        .collect()
}

/// The power-basis polynomial whose Bernstein coefficients on `[0, 1]` are `b`, exactly.
fn from_bernstein(b: &[f64]) -> Poly {
    let n = b.len() - 1;
    let t = Poly::linear(Rat::zero(), Rat::one());
    let s = Poly::linear(Rat::one(), -&Rat::one());
    b.iter()
        .enumerate()
        .fold(Poly::new(vec![]), |acc, (j, &bj)| {
            let term = t
                .pow(j as u32)
                .mul(&s.pow((n - j) as u32))
                .scale(&(&binom(n, j) * &rat(bj)));
            acc.add(&term)
        })
}

struct BCase {
    name: String,
    b: Vec<f64>,
}

fn bcase(name: impl Into<String>, p: &Poly, n: usize) -> BCase {
    BCase {
        name: name.into(),
        b: to_bernstein(p, n).iter().map(Rat::to_f64).collect(),
    }
}

fn bernstein_cases() -> Vec<BCase> {
    let mut v = Vec::new();
    for n in [5usize, 10, 15, 20] {
        // Wilkinson on [0, 1]: roots k / (n + 1)
        let r: Vec<Rat> = (1..=n as i64).map(|k| q(k, n as i64 + 1)).collect();
        v.push(bcase(
            format!("wilkinson {n}"),
            &from_roots(&r, &Rat::one()),
            n,
        ));
        // Chebyshev T_n on [0, 1]: x = 2 t - 1
        let x = Poly::linear(-&Rat::one(), int(2));
        let tn = chebyshev(n);
        let comp = tn
            .coeffs()
            .iter()
            .enumerate()
            .fold(Poly::new(vec![]), |acc, (i, c)| {
                acc.add(&x.pow(i as u32).scale(c))
            });
        v.push(bcase(format!("chebyshev {n}"), &comp, n));
        // Mignotte, a = 10: roots near 0.1, very close
        v.push(bcase(
            format!("mignotte {n}, a = 10"),
            &mignotte(n, &int(10)),
            n,
        ));
    }
    for (m, j) in [(2, 3), (3, 4), (4, 6), (2, 8), (3, 8)] {
        let d = rat(ten(-j));
        let r: Vec<Rat> = (0..m).map(|k| &q(3, 10) + &(&int(k) * &d)).collect();
        let others = [q(1, 10), q(7, 10)];
        let all: Vec<Rat> = r.iter().chain(others.iter()).cloned().collect();
        v.push(bcase(
            format!("cluster {m} at 0.3, step 1e-{j}"),
            &from_roots(&all, &Rat::one()),
            all.len(),
        ));
    }
    for k in [10, 20, 30, 40, 50] {
        let r0 = q(4, 10);
        let r1 = &r0 * &(&Rat::one() + &Rat::one().scale(-k));
        let all = vec![q(1, 10), r0, r1, q(8, 10)];
        v.push(bcase(
            format!("near-double 2^-{k}"),
            &from_roots(&all, &Rat::one()),
            4,
        ));
    }
    v
}

/// `sum |b_i| B_i(t)`: the scale of de Casteljau's rounding error at `t`.
fn abs_scale(b: &[f64], t: f64) -> f64 {
    let n = b.len() - 1;
    b.iter()
        .enumerate()
        .map(|(i, x)| {
            let c = binom(n, i).to_f64();
            x.abs()
                * c
                * (0..i).fold(1.0, |p, _| p * t)
                * (0..n - i).fold(1.0, |p, _| p * (1.0 - t))
        })
        .sum()
}

/// The bound a place meets: the true root is within `2 ulp + 4 n u sum |b_i| B_i(r) /
/// |p'(r)|` of it. `simple_root` certifies a place by de Casteljau's signs at two
/// adjacent doubles, so the true root is within the evaluation error over the slope.
/// De Casteljau's error is at most about `2 n u sum |b_i| B_i`, doubled here for the
/// subdivisions before it.
fn bound(b: &[f64], dp: &Poly, r: &Rat) -> f64 {
    let rf = r.to_f64();
    let n = (b.len() - 1) as f64;
    2.0 * ulp(rf) + 4.0 * n * (f64::EPSILON / 2.0) * abs_scale(b, rf) / dp.eval(r).to_f64().abs()
}

#[test]
fn bernstein_roots_on_the_families() {
    let mut cases = 0;
    for k in bernstein_cases() {
        cases += 1;
        let p = from_bernstein(&k.b);
        let dp = p.derivative();
        let t: Vec<Rat> = truth(&p)
            .into_iter()
            .map(|(r, _)| r)
            .filter(|r| r.signum() >= 0 && *r <= Rat::one())
            .collect();
        let got = bernstein_roots(&k.b, 0.0, 1.0, RootLimits::DEFAULT);
        if printing() {
            let tv: Vec<f64> = t.iter().map(Rat::to_f64).collect();
            println!(
                "{}: true {tv:?}\n    got {:?} ({})",
                k.name, got.places, got.isolated
            );
        }
        assert!(got.isolated, "{}: not isolated", k.name);
        let dist = |x: f64, r: &Rat| (&rat(x) - r).abs().to_f64();
        // every true root has a place within its bound
        for r in &t {
            let d = got
                .places
                .iter()
                .map(|&x| dist(x, r))
                .fold(f64::INFINITY, f64::min);
            let b = bound(&k.b, &dp, r);
            assert!(
                d <= b,
                "{}: root {:e} is {d:e} from a place, bound {b:e}",
                k.name,
                r.to_f64()
            );
        }
        // every place is within the bound of a true root
        for &x in &got.places {
            let ok = t.iter().any(|r| dist(x, r) <= bound(&k.b, &dp, r));
            assert!(ok, "{}: place {x:e} is near no true root", k.name);
        }
        let mut distinct = got.places.clone();
        distinct.sort_by(f64::total_cmp);
        distinct.dedup();
        assert_eq!(distinct.len(), t.len(), "{}: distinct places", k.name);
        assert_eq!(got.places.len(), t.len(), "{}: each root once", k.name);
        assert!(
            got.places.windows(2).all(|w| w[0] < w[1]),
            "{}: increasing order",
            k.name
        );
    }
    assert_eq!(cases, 22);
}
