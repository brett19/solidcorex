// Written for solidcorex. Apache-2.0.
//! The generators: deterministic, valid, and as ill-conditioned as they say.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::needless_range_loop,
    clippy::unwrap_used,
    clippy::float_cmp
)]

use scx_num::hash::Rng;
use scx_testkit::gen::{self, Knots, Spec, Weights};

fn spec(degree: usize, count: usize, knots: Knots) -> Spec {
    Spec {
        degree,
        count,
        knots,
    }
}

#[test]
fn equal_seeds_give_equal_data() {
    let make = |seed| {
        let mut rng = Rng::new(seed);
        let c = gen::curve::<3>(&mut rng, spec(3, 9, Knots::Clustered), Weights::NearZero);
        let s = gen::surface::<4>(
            &mut rng,
            (spec(2, 5, Knots::Multiple), spec(1, 3, Knots::Random)),
            Weights::Moderate,
        );
        let u = gen::parameter(&mut rng, &c.knots, (0.0, 1.0));
        (c, s, u, gen::frame(&mut rng, 1.0))
    };
    assert_eq!(make(7), make(7));
    assert_ne!(make(7), make(8));
    // The stream itself, pinned: a change to a generator changes every test's data.
    let (c, ..) = make(7);
    assert_eq!(c.knots.len(), 13);
}

#[test]
fn generated_data_is_valid_and_ill_conditioned_where_asked() {
    let mut rng = Rng::new(1);
    let (mut clustered, mut repeated, mut tiny) = (0, 0, 0);
    for i in 0..400 {
        let degree = 1 + i % 4;
        let count = degree + 1 + i % 7;
        for knots in [
            Knots::Uniform,
            Knots::Random,
            Knots::Clustered,
            Knots::Multiple,
        ] {
            for weights in [Weights::Unit, Weights::Moderate, Weights::NearZero] {
                let c = gen::curve::<2>(&mut rng, spec(degree, count, knots), weights);
                assert!(c.exact().is_ok(), "{c:?}");
                assert_eq!(c.knots.first(), Some(&0.0));
                assert_eq!(c.knots.last(), Some(&1.0));
                for w in c.knots.windows(2) {
                    let gap = w[1] - w[0];
                    if gap > 0.0 && gap < 1e-14 {
                        clustered += 1;
                    }
                }
                let inner = &c.knots[degree + 1..count];
                repeated += inner.windows(2).filter(|w| w[0] == w[1]).count();
                tiny += c.weights.iter().flatten().filter(|&&w| w < 1e-6).count();
                assert_eq!(c.weights.is_none(), weights == Weights::Unit);
                if knots == Knots::Uniform || knots == Knots::Random {
                    assert!(inner.windows(2).all(|w| w[0] < w[1]) || knots == Knots::Random);
                }
            }
        }
    }
    assert!(clustered > 100, "{clustered}");
    assert!(repeated > 100, "{repeated}");
    assert!(tiny > 100, "{tiny}");
}

#[test]
fn parameters_land_on_and_beside_the_knots() {
    let mut rng = Rng::new(2);
    let knots = [0.0, 0.0, 0.25, 0.5, 0.5, 1.0, 1.0];
    let (mut on, mut beside) = (0, 0);
    for _ in 0..1000 {
        let u = gen::parameter(&mut rng, &knots, (0.0, 1.0));
        assert!((0.0..=1.0).contains(&u));
        if knots.contains(&u) {
            on += 1;
        } else if knots.iter().any(|&k| (u - k).abs() < 1e-15) {
            beside += 1;
        }
    }
    assert!(on > 50 && beside > 200, "{on} {beside}");
}
