// Written for solidcorex. Apache-2.0.
//! The cubic and quartic roots against the exact truth: every coefficient is a double, so
//! the polynomial is exact over the rationals, and its real roots are isolated with
//! certainty by `scx_testkit::poly` (Sturm sequences) to within `2^-100`. Each root a solver
//! returns is measured against the nearest true real root, in units in the last place of
//! that root.
//!
//! The seeded families: separated real roots; near-double roots (two roots `10^-k` apart, `k`
//! from 2 to 10); double roots as rounded to doubles (so the truth is a close pair or a
//! complex pair); roots and leading coefficients of very different sizes; and random
//! coefficients. A returned root with no true real root within `1e-4` relative (a complex
//! pair near the axis, reported as a grazing double root) is counted apart, not measured.
//!
//! `SCX_PRINT_POLY_TRUTH=1 cargo test -p scx-algo --test polynomial_truth -- --nocapture`
//! prints the table (roots measured, apart, median and max ulps per degree and family).

#![allow(missing_docs, clippy::pedantic, clippy::indexing_slicing)]

use scx_algo::polynomial::{solve_cubic, solve_quartic};
use scx_num::hash::Rng;
use scx_num::math::EFF_ZERO;
use scx_testkit::Rat;
use scx_testkit::poly::Poly;

/// `lead * prod (x - r)`, lowest power first, in doubles (so rounded).
fn from_roots(roots: &[f64], lead: f64) -> Vec<f64> {
    let mut c = vec![lead];
    for &r in roots {
        let mut n = vec![0.0; c.len() + 1];
        for (i, &a) in c.iter().enumerate() {
            n[i + 1] += a;
            n[i] -= r * a;
        }
        c = n;
    }
    c
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Family {
    Separated,
    NearDouble,
    Double,
    Scales,
    Random,
}

const FAMILIES: [Family; 5] = [
    Family::Separated,
    Family::NearDouble,
    Family::Double,
    Family::Scales,
    Family::Random,
];

fn sign(r: &mut Rng) -> f64 {
    if r.next_u64() % 2 == 0 { 1.0 } else { -1.0 }
}

/// A root in `[-3, -0.1] u [0.1, 3]`, never exactly zero (a zero root has no ulp scale).
fn root(r: &mut Rng) -> f64 {
    sign(r) * (0.1 + 2.9 * r.next_f64())
}

fn lead(r: &mut Rng) -> f64 {
    sign(r) * (0.5 + 1.5 * r.next_f64())
}

/// One polynomial of degree `deg` from `family`.
fn draw(family: Family, deg: usize, r: &mut Rng) -> Vec<f64> {
    let mut roots: Vec<f64> = (0..deg).map(|_| root(r)).collect();
    match family {
        Family::Separated => {
            // redrawn until every two are at least 0.2 apart
            while roots
                .iter()
                .enumerate()
                .any(|(i, a)| roots[i + 1..].iter().any(|b| (a - b).abs() < 0.2))
            {
                roots = (0..deg).map(|_| root(r)).collect();
            }
        }
        Family::Random => {}
        Family::NearDouble => {
            let k = 2 + (r.next_u64() % 9) as i32;
            roots[1] = roots[0] + sign(r) * 10f64.powi(-k);
        }
        Family::Double => roots[1] = roots[0],
        Family::Scales => {
            let k = 1 + (r.next_u64() % 3) as i32;
            roots[0] *= 10f64.powi(-k);
            roots[deg - 1] *= 10f64.powi(k);
            let l = lead(r) * 10f64.powi(((r.next_u64() % 13) as i32) - 6);
            return from_roots(&roots, l);
        }
    }
    if family == Family::Random {
        let mut c: Vec<f64> = (0..=deg).map(|_| r.next_signed_unit()).collect();
        c[deg] = lead(r);
        return c;
    }
    from_roots(&roots, lead(r))
}

/// The true real roots of `c` (exact rational coefficients), each to within `2^-100`.
fn truth(c: &[f64]) -> Vec<Rat> {
    let rc: Vec<Rat> = c.iter().map(|&x| Rat::from_f64(x).unwrap()).collect();
    let width = Rat::one().scale(-100);
    Poly::new(rc)
        .real_roots(&width)
        .iter()
        .map(|t| t.at.mid())
        .collect()
}

/// The spacing of doubles at `|x|`.
fn ulp(x: f64) -> f64 {
    let a = x.abs();
    a.next_up() - a
}

/// How a returned root measures against the truth: ulps from the nearest true real root,
/// or `None` when no true root lies within `1e-4` relative.
fn measure(x: f64, roots: &[Rat]) -> Option<f64> {
    let rx = Rat::from_f64(x)?;
    let near = roots
        .iter()
        .min_by(|a, b| (&rx - a).abs().cmp(&(&rx - b).abs()))?;
    let t = near.to_f64();
    let err = (&rx - near).abs().to_f64();
    (err <= 1e-4 * t.abs().max(1.0)).then(|| err / ulp(t))
}

#[derive(Default)]
struct Stats {
    ulps: Vec<f64>,
    apart: usize,
}

impl Stats {
    fn median(&mut self) -> f64 {
        self.ulps.sort_by(f64::total_cmp);
        self.ulps.get(self.ulps.len() / 2).copied().unwrap_or(0.0)
    }
    fn max(&self) -> f64 {
        self.ulps.iter().copied().fold(0.0, f64::max)
    }
}

const PER_FAMILY: usize = 400;

fn run(deg: usize, seed: u64, solve: impl Fn(&[f64]) -> Vec<f64>) -> Vec<(Family, Stats)> {
    let mut r = Rng::new(seed);
    let mut out = Vec::new();
    for family in FAMILIES {
        let mut s = Stats::default();
        for _ in 0..PER_FAMILY {
            let c = draw(family, deg, &mut r);
            let t = truth(&c);
            for x in solve(&c) {
                assert!(x.is_finite(), "{c:?}: {x}");
                match measure(x, &t) {
                    Some(u) => s.ulps.push(u),
                    None => s.apart += 1,
                }
            }
        }
        out.push((family, s));
    }
    out
}

fn print(name: &str, table: &mut [(Family, Stats)]) {
    if std::env::var_os("SCX_PRINT_POLY_TRUTH").is_none() {
        return;
    }
    let mut all = Stats::default();
    for (family, s) in table.iter_mut() {
        let median = s.median();
        println!(
            "{name} {family:?}: {} roots, {} apart, median {median:.3e} ulps, max {:.3e} ulps",
            s.ulps.len(),
            s.apart,
            s.max()
        );
        all.ulps.extend_from_slice(&s.ulps);
        all.apart += s.apart;
    }
    let median = all.median();
    println!(
        "{name} all: {} roots, {} apart, median {median:.3e} ulps, max {:.3e} ulps",
        all.ulps.len(),
        all.apart,
        all.max()
    );
}

/// The bounds the polished roots meet. A root not near another (separated, of mixed
/// scales, or of random coefficients) is the true root correctly rounded: within half a
/// unit in the last place (the Newton steps run on a residual as accurate as twice the
/// working precision). Near-double and double roots are not bounded one by one: where the
/// closed form reports one root for a close pair, that root lies between the two, half
/// their distance from each, and polishing keeps the count. Half of them must still be
/// within one unit.
fn assert_bounds(name: &str, table: &mut [(Family, Stats)]) {
    for (family, s) in table.iter_mut() {
        let median = s.median();
        assert!(
            median <= 1.0,
            "{name} {family:?}: the median root is {median} ulps off"
        );
        if matches!(family, Family::Separated | Family::Scales | Family::Random) {
            assert!(
                s.max() <= 0.5,
                "{name} {family:?}: a root is {} ulps from the truth",
                s.max()
            );
        }
    }
}

#[test]
fn cubic_roots_against_the_truth() {
    let mut t = run(3, 101, |c| {
        solve_cubic(&[c[0], c[1], c[2], c[3]], EFF_ZERO).to_vec()
    });
    print("cubic", &mut t);
    assert_bounds("cubic", &mut t);
}

#[test]
fn quartic_roots_against_the_truth() {
    let mut t = run(4, 103, |c| {
        solve_quartic(&[c[0], c[1], c[2], c[3], c[4]], EFF_ZERO).to_vec()
    });
    print("quartic", &mut t);
    assert_bounds("quartic", &mut t);
}
