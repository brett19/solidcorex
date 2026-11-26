// Written for solidcorex. Apache-2.0.
//! `scx-algo`: `polynomial.rs`.
//!
//! The pools give random and edge coefficients; `poly_from_roots` adds every polynomial
//! built from a small set of real roots (with repeats, so double and triple roots), with
//! a complex pair, with the constant term moved by about each threshold, and with the
//! leading coefficient at zero or near it, so each branch of the three solvers runs (the
//! grazing discriminant, the triple root, three real roots, the resolvent's `u` or `v`
//! below zero, the duplicate removal).

use crate::diff;

const R: &[f64] = &[-3.0, -2.0, -1.0, -0.5, 0.0, 1e-9, 0.5, 1.0, 2.0, 3.0];
const NUDGE: &[f64] = &[0.0, 1e-13, -1e-13, 1e-10, -1e-10, 1e-7, -1e-7];
const LEAD: &[f64] = &[1.0, -2.0, 1e-13, 0.0];

/// Coefficients (ascending) of `lead * prod (x - r)`.
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

/// Every multiset of `k` roots from `R`.
fn multisets(k: usize) -> Vec<Vec<f64>> {
    fn go(k: usize, from: usize, cur: &mut Vec<f64>, out: &mut Vec<Vec<f64>>) {
        if cur.len() == k {
            out.push(cur.clone());
            return;
        }
        for i in from..R.len() {
            if let Some(&r) = R.get(i) {
                cur.push(r);
                go(k, i, cur, out);
                cur.pop();
            }
        }
    }
    let mut out = Vec::new();
    go(k, 0, &mut Vec::new(), &mut out);
    out
}

/// The structured cases of degree `deg` (ascending coefficients, `deg + 1` of them).
fn cases(deg: usize) -> Vec<Vec<f64>> {
    let mut out = Vec::new();
    for lead in LEAD {
        for roots in multisets(deg) {
            for nudge in NUDGE {
                let mut c = from_roots(&roots, *lead);
                if let Some(c0) = c.first_mut() {
                    *c0 += nudge;
                }
                out.push(c);
            }
        }
        // a complex pair `x^2 + 1` times real roots
        if deg >= 2 {
            for roots in multisets(deg - 2) {
                let real = from_roots(&roots, *lead);
                let mut c = vec![0.0; deg + 1];
                for (i, &a) in real.iter().enumerate() {
                    if let Some(s) = c.get_mut(i) {
                        *s += a;
                    }
                    if let Some(s) = c.get_mut(i + 2) {
                        *s += a;
                    }
                }
                out.push(c);
            }
        }
    }
    out
}

fn arr<const N: usize>(c: &[f64]) -> [f64; N] {
    let mut a = [0.0; N];
    for (s, &v) in a.iter_mut().zip(c) {
        *s = v;
    }
    a
}

diff!(poly_quadratic, |c: [f64; 3], tol: f64| m::polynomial::solve_quadratic(&c, tol).to_vec());
diff!(poly_cubic, |c: [f64; 4], tol: f64| m::polynomial::solve_cubic(&c, tol).to_vec());
diff!(poly_quartic, |c: [f64; 4], e: f64, tol: f64| {
    let [a, b, d, f] = c;
    m::polynomial::solve_quartic(&[a, b, d, f, e], tol).to_vec()
});
diff!(poly_roots_type, || {
    let r = m::polynomial::Roots::new();
    let d = m::polynomial::Roots::default();
    (r.as_slice().to_vec(), r.len(), r == d, format!("{r:?}") == format!("{d:?}"))
});
diff!(poly_from_roots, |tol: f64| {
    let mut out: Vec<Vec<f64>> = Vec::new();
    for c in cases(2) {
        out.push(m::polynomial::solve_quadratic(&arr::<3>(&c), tol).to_vec());
    }
    for c in cases(3) {
        out.push(m::polynomial::solve_cubic(&arr::<4>(&c), tol).to_vec());
    }
    for c in cases(4) {
        out.push(m::polynomial::solve_quartic(&arr::<5>(&c), tol).to_vec());
    }
    out
});
