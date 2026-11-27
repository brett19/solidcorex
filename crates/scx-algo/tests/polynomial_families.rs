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
//! (`EFF_ZERO`). Their claim (module docs) is that a polished simple root is the true root
//! correctly rounded. Each case's `(returned, within half an ulp, true)` is pinned in
//! [`CLOSED`], which marks the cases that meet the claim and the known bugs.
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
    match c.len() {
        4 => solve_cubic(&[c[0], c[1], c[2], c[3]], EFF_ZERO).to_vec(),
        5 => solve_quartic(&[c[0], c[1], c[2], c[3], c[4]], EFF_ZERO).to_vec(),
        n => panic!("degree {}", n - 1),
    }
}

/// What a closed-form case gave: per returned root, ulps from the nearest true root, and
/// that root's gap to the next true root (relative), with the counts.
struct Measured {
    ulps: Vec<(f64, f64)>,
    returned: usize,
    true_roots: usize,
}

fn measure(k: &Case) -> Measured {
    let t = truth(&exact(&k.c));
    let got = solve(&k.c);
    let mut ulps = Vec::new();
    for &x in &got {
        assert!(x.is_finite(), "{}: {x}", k.name);
        let Some((d, r, _)) = nearest(x, &t) else {
            panic!("{}: root {x:e} but no true real root", k.name)
        };
        let gap = t
            .iter()
            .map(|(s, _)| (s.to_f64() - r).abs())
            .filter(|g| *g > 0.0)
            .fold(f64::INFINITY, f64::min)
            / r.abs().max(f64::MIN_POSITIVE);
        ulps.push((d / ulp(r), gap));
    }
    if printing() {
        let tv: Vec<f64> = t.iter().map(|(r, _)| r.to_f64()).collect();
        println!("{}: true {tv:?}", k.name);
        println!("    got {got:?}");
        println!("    ulps/gap {ulps:.3?}");
    }
    Measured {
        ulps,
        returned: got.len(),
        true_roots: t.len(),
    }
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

fn closed_cases() -> (Vec<Case>, Vec<Case>) {
    let mut separated = Vec::new();
    let mut close = Vec::new();
    for n in [3, 4] {
        separated.push(case(format!("wilkinson {n}"), &wilkinson(n)));
        separated.push(case(format!("chebyshev {n}"), &chebyshev(n as usize)));
        // Wilkinson's second: roots 2^-1 .. 2^-n, exact
        let r: Vec<Rat> = (1..=n).map(|k| Rat::one().scale(-k)).collect();
        separated.push(case(format!("geometric {n}"), &from_roots(&r, &Rat::one())));
        // roots k/n scaled to 10^s
        for s in [-6, 0, 6] {
            let r: Vec<Rat> = (1..=n).map(|k| &q(k, n) * &rat(ten(s))).collect();
            separated.push(case(
                format!("wilkinson {n} at 1e{s}"),
                &from_roots(&r, &Rat::one()),
            ));
        }
        for a in [10, 100, 1000] {
            close.push(case(
                format!("mignotte {n}, a = {a}"),
                &mignotte(n as usize, &int(a)),
            ));
        }
        for j in [2, 4, 6] {
            let d = rat(ten(-j));
            let r: Vec<Rat> = (0..n).map(|k| &int(1) + &(&int(k) * &d)).collect();
            close.push(case(
                format!("cluster {n} at 1, step 1e-{j}"),
                &from_roots(&r, &Rat::one()),
            ));
        }
        for k in [10, 20, 30, 40] {
            let r0 = q(7, 10);
            let r1 = &r0 * &(&Rat::one() + &Rat::one().scale(-k));
            let mut r = vec![r0, r1];
            r.extend((3..=n).map(|i| int(-i)));
            close.push(case(
                format!("near-double {n}, 2^-{k}"),
                &from_roots(&r, &Rat::one()),
            ));
        }
    }
    separated.push(Case {
        name: "research 53 quartic".into(),
        c: R53_QUARTIC.to_vec(),
    });
    (separated, close)
}

/// What each closed-form case gives: `(roots returned, of them within half an ulp of a
/// true root, true real roots)`. A row marked `ok` meets the module's claim (every root
/// returned, each the true one correctly rounded); every other row is a known bug, pinned
/// (see the notes above `closed_forms_on_the_families`). A fix that brings a row to
/// `(n, n, n)` marks it `ok`; any other change fails.
const CLOSED: &[(&str, (usize, usize, usize), bool)] = &[
    ("wilkinson 3", (3, 3, 3), OK),
    ("chebyshev 3", (3, 3, 3), OK),
    ("geometric 3", (3, 3, 3), OK),
    ("wilkinson 3 at 1e-6", (1, 1, 3), BUG),
    ("wilkinson 3 at 1e0", (3, 3, 3), OK),
    ("wilkinson 3 at 1e6", (3, 3, 3), OK),
    ("mignotte 3, a = 10", (3, 3, 3), OK),
    ("mignotte 3, a = 100", (1, 1, 3), BUG),
    ("mignotte 3, a = 1000", (1, 1, 3), BUG),
    ("cluster 3 at 1, step 1e-2", (1, 1, 3), BUG),
    ("cluster 3 at 1, step 1e-4", (1, 1, 3), BUG),
    ("cluster 3 at 1, step 1e-6", (1, 0, 1), BUG),
    ("near-double 3, 2^-10", (3, 3, 3), OK),
    ("near-double 3, 2^-20", (2, 1, 3), BUG),
    ("near-double 3, 2^-30", (2, 1, 3), BUG),
    // one real root: the pair is complex once rounded, reported as a grazing double root
    ("near-double 3, 2^-40", (2, 1, 1), OK),
    ("wilkinson 4", (4, 4, 4), OK),
    ("chebyshev 4", (4, 4, 4), OK),
    ("geometric 4", (4, 0, 4), BUG),
    ("wilkinson 4 at 1e-6", (1, 0, 4), BUG),
    ("wilkinson 4 at 1e0", (4, 4, 4), OK),
    ("wilkinson 4 at 1e6", (0, 0, 4), BUG),
    ("mignotte 4, a = 10", (4, 4, 4), OK),
    ("mignotte 4, a = 100", (4, 2, 4), BUG),
    ("mignotte 4, a = 1000", (3, 2, 4), BUG),
    ("cluster 4 at 1, step 1e-2", (0, 0, 4), BUG),
    ("cluster 4 at 1, step 1e-4", (1, 0, 2), BUG),
    ("cluster 4 at 1, step 1e-6", (1, 0, 1), BUG),
    ("near-double 4, 2^-10", (4, 4, 4), OK),
    ("near-double 4, 2^-20", (3, 2, 4), BUG),
    ("near-double 4, 2^-30", (3, 2, 4), BUG),
    ("near-double 4, 2^-40", (3, 2, 4), BUG),
    ("research 53 quartic", (4, 1, 4), BUG),
];

const OK: bool = true;
const BUG: bool = false;

// Known bug (solve_cubic, solve_quartic; found 11-27 by this test): the module docs say a
// polished simple root is within half an ulp of the true one, but the closed forms decide
// on absolute tolerances. Callers pass `zero_tol = EFF_ZERO = 1e-12`. It is compared with
// the cubic's discriminant `q^2 + p^3` (of the sixth power of the roots' scale) and with
// Ferrari's `u = z^2 - r` and `v = 2 z - p`. Where they fall below it, roots merge,
// vanish or move. Examples: the cubic and quartic with roots 1, 1.01, 1.02(, 1.03) return
// 1 and 0 roots; the quartic with roots 1/16, 1/8, 1/4, 1/2 (exact coefficients) returns
// 0.1524 for 1/8; research 53's quartic (roots 0.00874, 0.0359, 0.0435, -0.689) still
// returns 0.0326 for 0.0359 after polishing (error 3.3e-3; its other small roots are off
// by 4e-5 and 2e-7).
// Polishing cannot repair these: it keeps the count and moves no root past a quarter of
// the gap to another. With `zero_tol = 0` the cubic clusters, the geometric quartic and
// research 53's quartic come out right (within 0.5 ulp), but Wilkinson's quartic and the
// quartic with roots 1 to 1.03 then return no roots. At either tolerance, the quartic
// with roots 250000 to 10^6 returns none (an early `return` when `u` or `v` is below 0),
// and the cubic `x^3 - 2 (100 x - 1)^2` (exact coefficients) loses its two roots near
// 0.01, 1.4e-3 apart relative. A fix needs scale-aware decisions or another method: not
// small.
#[test]
fn closed_forms_on_the_families() {
    let (separated, close) = closed_cases();
    let cases: Vec<Case> = separated.into_iter().chain(close).collect();
    assert_eq!(cases.len(), CLOSED.len());
    let mut changed = Vec::new();
    for k in &cases {
        let m = measure(k);
        let half = m.ulps.iter().filter(|u| u.0 <= 0.5).count();
        let got = (m.returned, half, m.true_roots);
        let Some(&(_, want, ok)) = CLOSED.iter().find(|r| r.0 == k.name) else {
            panic!("{}: no row", k.name)
        };
        if ok && k.name != "near-double 3, 2^-40" {
            assert!(
                got.0 == got.2 && got.1 == got.2,
                "{}: an ok row {got:?}",
                k.name
            );
        }
        if got != want {
            changed.push(format!("{}: {got:?}, pinned {want:?}", k.name));
        }
    }
    assert!(changed.is_empty(), "rows changed:\n{}", changed.join("\n"));
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
