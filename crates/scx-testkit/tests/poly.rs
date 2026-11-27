// Written for solidcorex. Apache-2.0.
//! The certified real roots against polynomials built from known roots and multiplicities,
//! and the integer working form's square-free factorisation against the rational one.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp
)]

use scx_num::hash::Rng;
use scx_testkit::poly::Poly;
use scx_testkit::Rat;

fn r(x: f64) -> Rat {
    Rat::from_f64(x).unwrap()
}

/// `prod (x - root)^m`.
fn from_roots(roots: &[(Rat, u32)]) -> Poly {
    roots.iter().fold(Poly::constant(Rat::one()), |p, (x, m)| {
        p.mul(&Poly::linear(-x, Rat::one()).pow(*m))
    })
}

#[test]
fn known_roots_come_back_exactly_with_their_multiplicities() {
    let mut rng = Rng::new(0x7401);
    for _ in 0..60 {
        let n = 1 + (rng.next_u64() % 4) as usize;
        let mut want: Vec<(Rat, u32)> = Vec::new();
        while want.len() < n {
            // Dyadic roots, met exactly by the bisection or not: both are allowed.
            let x = r((rng.next_signed_unit() * 64.0).round() / 16.0);
            if want.iter().all(|(y, _)| *y != x) {
                want.push((x, 1 + (rng.next_u64() % 3) as u32));
            }
        }
        // A positive quadratic with no real root, and a scale, change nothing.
        let p = from_roots(&want)
            .mul(&Poly::new(vec![r(1.5), r(0.25), r(1.0)]))
            .scale(&r(-0.375));
        want.sort();
        let got = p.real_roots();
        assert_eq!(got.len(), want.len());
        for (g, (x, m)) in got.iter().zip(&want) {
            assert_eq!(g.multiplicity, *m);
            assert!(g.lo <= *x && *x <= g.hi);
        }
    }
}

#[test]
fn irrational_roots_are_isolated_and_refined() {
    // x^2 - 2: +-sqrt2; (x^2 - 2)^2 (x - 1): each sqrt2 double.
    let q = Poly::new(vec![r(-2.0), Rat::zero(), Rat::one()]);
    let s2 = 2f64.sqrt();
    for (p, m) in [
        (q.clone(), 1),
        (q.mul(&q).mul(&Poly::linear(r(-1.0), Rat::one())), 2),
    ] {
        let roots = p.real_roots();
        let sq: Vec<_> = roots.iter().filter(|x| x.multiplicity == m).collect();
        assert_eq!(sq.len(), 2);
        assert!((sq[0].value().to_f64() + s2).abs() <= 2.3e-16);
        assert!((sq[1].value().to_f64() - s2).abs() <= 2.3e-16);
        for x in &sq {
            assert!(x.radius() < 1e-32);
            // The interval holds a sign change of x^2 - 2.
            assert!(q.eval(&x.lo).signum() * q.eval(&x.hi).signum() <= 0);
        }
    }
}

/// The integer form behind `real_roots` against the rational Yun factorisation.
#[test]
fn the_multiplicities_agree_with_the_rational_factorisation() {
    let mut rng = Rng::new(0x7402);
    for _ in 0..30 {
        // Random rational factors, some repeated.
        let mut p = Poly::constant(r(1.0 + rng.next_f64()));
        for _ in 0..3 {
            let f = Poly::new(vec![
                r(rng.next_signed_unit()),
                r(rng.next_signed_unit()),
                r(1.0),
            ]);
            p = p.mul(&f.pow(1 + (rng.next_u64() % 2) as u32));
        }
        let by_rat: u32 = p
            .square_free()
            .iter()
            .map(|(f, m)| f.real_roots().len() as u32 * m)
            .sum();
        let by_int: u32 = p.real_roots().iter().map(|x| x.multiplicity).sum();
        assert_eq!(by_rat, by_int);
    }
}
