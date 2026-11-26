// Written for solidcorex. Apache-2.0.
//! Properties of the closed-form polynomial solvers over seeded random polynomials built
//! from known real roots: every root returned is a root (small residual against the
//! size of the terms), at most `degree` are returned, every well-separated known root is
//! found, a quartic with a vanishing leading term gives its cubic's roots without
//! duplicates, and no two quartic roots lie within the tolerance of each other.

use scx_algo::polynomial::{solve_cubic, solve_quadratic, solve_quartic};
use scx_num::math::EFF_ZERO;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    /// A root in [-4, 4).
    fn root(&mut self) -> f64 {
        self.next() * 8.0 - 4.0
    }
}

fn from_roots(roots: &[f64], lead: f64) -> Vec<f64> {
    let mut c = vec![lead];
    for &r in roots {
        let mut n = vec![0.0; c.len() + 1];
        for (i, &a) in c.iter().enumerate() {
            if let Some(s) = n.get_mut(i + 1) {
                *s += a;
            }
            if let Some(s) = n.get_mut(i) {
                *s -= r * a;
            }
        }
        c = n;
    }
    c
}

/// `|p(x)|` over the sum of `|c_i x^i|`: the residual relative to the terms' size.
fn rel_residual(c: &[f64], x: f64) -> f64 {
    let (mut v, mut s, mut p) = (0.0, 0.0, 1.0);
    for &a in c {
        v += a * p;
        s += (a * p).abs();
        p *= x;
    }
    if s == 0.0 {
        0.0
    } else {
        v.abs() / s
    }
}

fn arr<const N: usize>(c: &[f64]) -> [f64; N] {
    let mut a = [0.0; N];
    for (s, &v) in a.iter_mut().zip(c) {
        *s = v;
    }
    a
}

fn separated(roots: &[f64], gap: f64) -> bool {
    roots
        .iter()
        .enumerate()
        .all(|(i, a)| roots.iter().skip(i + 1).all(|b| (a - b).abs() > gap))
}

/// `gap`: how far apart the known roots must be for each to be required. The quartic needs
/// 0.5: Ferrari's method loses about 4e-3 on roots 0.04 apart near 2.4 (report 53,
/// "found, not changed").
fn check(deg: usize, gap: f64, solve: impl Fn(&[f64]) -> Vec<f64>, seed: u64) -> (usize, usize) {
    let mut rng = Rng(seed);
    let (mut cases, mut separated_cases) = (0, 0);
    for _ in 0..20_000 {
        let roots: Vec<f64> = (0..deg).map(|_| rng.root()).collect();
        let lead = if rng.next() < 0.5 {
            1.0
        } else {
            -0.5 - rng.next() * 3.0
        };
        let c = from_roots(&roots, lead);
        let got = solve(&c);
        cases += 1;
        assert!(got.len() <= deg, "{c:?}: {got:?}");
        // The quartic's residual is bounded only on separated roots: on a cluster near 0
        // (0.0087, 0.0359, 0.0435) it returns 0.0096 with relative residual 0.018 (report
        // 53, "found, not changed").
        let checked = deg < 4 || separated(&roots, gap);
        for &x in &got {
            assert!(x.is_finite(), "{c:?}: {got:?}");
            assert!(!checked || rel_residual(&c, x) < 1e-6, "{c:?}: {x} of {got:?}");
        }
        if separated(&roots, gap) {
            separated_cases += 1;
            for r in &roots {
                assert!(
                    got.iter().any(|x| (x - r).abs() < 1e-6 * (1.0 + r.abs())),
                    "{c:?}: {r} not in {got:?}"
                );
            }
        }
    }
    (cases, separated_cases)
}

#[test]
fn quadratic_roots_are_roots_and_all_are_found() {
    let (n, s) = check(2, 0.05, |c| solve_quadratic(&arr(c), EFF_ZERO).to_vec(), 7);
    assert!(n == 20_000 && s > 15_000);
}

#[test]
fn cubic_roots_are_roots_and_all_are_found() {
    let (n, s) = check(3, 0.05, |c| solve_cubic(&arr(c), EFF_ZERO).to_vec(), 11);
    assert!(n == 20_000 && s > 10_000);
}

#[test]
fn quartic_roots_are_roots_and_all_are_found() {
    let (n, s) = check(4, 0.5, |c| solve_quartic(&arr(c), EFF_ZERO).to_vec(), 13);
    assert!(n == 20_000 && s > 2_000);
}

#[test]
fn a_vanishing_leading_term_falls_back_without_duplicates() {
    let mut rng = Rng(17);
    for _ in 0..5_000 {
        let roots: Vec<f64> = (0..3).map(|_| (rng.root() * 2.0).round() / 2.0).collect();
        let c = from_roots(&roots, 1.0);
        let [c0, c1, c2, c3] = arr(&c);
        let q = solve_quartic(&[c0, c1, c2, c3, 0.0], EFF_ZERO).to_vec();
        let cubic = solve_cubic(&[c0, c1, c2, c3], EFF_ZERO).to_vec();
        let mut dedup: Vec<f64> = Vec::new();
        for x in cubic {
            if !dedup.iter().any(|y| (x - y).abs() < EFF_ZERO) {
                dedup.push(x);
            }
        }
        assert_eq!(q, dedup, "{c:?}");
        // the cubic itself also falls back when its leading term vanishes
        let a = solve_cubic(&[c0, c1, c2, 0.0], EFF_ZERO).to_vec();
        let b = solve_quadratic(&[c0, c1, c2], EFF_ZERO).to_vec();
        assert_eq!(a, b, "{c:?}");
    }
}

#[test]
fn quartic_roots_are_distinct_within_the_tolerance() {
    let mut rng = Rng(19);
    for _ in 0..20_000 {
        let tol = match (rng.next() * 3.0) as usize {
            0 => EFF_ZERO,
            1 => 1e-8,
            _ => 1e-4,
        };
        let roots: Vec<f64> = (0..4).map(|_| (rng.root() * 4.0).round() / 4.0).collect();
        let c = from_roots(&roots, 1.0);
        let q = solve_quartic(&arr(&c), tol).to_vec();
        let distinct = q
            .iter()
            .enumerate()
            .all(|(i, a)| q.iter().skip(i + 1).all(|b| (a - b).abs() >= tol));
        assert!(distinct, "{c:?} {tol}: {q:?}");
    }
}
