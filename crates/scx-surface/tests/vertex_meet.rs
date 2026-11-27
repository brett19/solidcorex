// Written for solidcorex. Apache-2.0.
//! `points_meet` (research 74 §30, SU8): the pin's two tests on the typed result, with the
//! exact ties the enclosure cannot decide named, and closed forms (a 3-4-5 triangle, the
//! joined accuracy holding the other ball) on a spread of magnitudes.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic,
    clippy::disallowed_methods
)]

use scx_num::units::{Accuracy, Length};
use scx_num::vector3d::Point3;
use scx_surface::{Meet, MeetError, points_meet};

fn acc(x: f64) -> Accuracy {
    Accuracy::new(Length::from_mm(x)).unwrap()
}

const O: Point3 = Point3::new(0.0, 0.0, 0.0);

/// The joined accuracy of a meeting pair, or a panic.
fn one(a: Point3, a_acc: f64, b: Point3, b_acc: f64) -> f64 {
    match points_meet(a, acc(a_acc), b, acc(b_acc)).unwrap() {
        Meet::One { accuracy } => accuracy.length().mm(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn points_meet_up_to_the_sum_of_their_accuracies_and_not_past_it() {
    // 3-4-5: the distance is 5. Tolerances 2 and 4 overlap by one: the joined radius about
    // `a` is max(2, 4 + 5), within the enclosure's rounding up.
    let b = Point3::new(3.0, 4.0, 0.0);
    let t = one(O, 2.0, b, 4.0);
    assert!((9.0..9.0 + 1e-14).contains(&t), "{t}");
    let t = one(O, 6.0, b, 0.5);
    assert!((6.0..6.0 + 1e-14).contains(&t), "{t}");
    // Apart.
    assert_eq!(points_meet(O, acc(2.0), b, acc(2.5)).unwrap(), Meet::Apart);
    assert_eq!(points_meet(O, acc(0.0), b, acc(4.9)).unwrap(), Meet::Apart);
}

#[test]
fn a_tie_to_the_last_bit_is_undecided_with_the_distance_bracketed() {
    let b = Point3::new(3.0, 4.0, 0.0);
    // Touching exactly: the pin says one; the enclosure cannot prove it.
    let Meet::Undecided { distance } = points_meet(O, acc(2.0), b, acc(3.0)).unwrap() else {
        panic!("decided")
    };
    assert!(distance.lower() <= 5.0 && distance.upper() >= 5.0);
    assert!(distance.width() < 1e-14);
    // One ulp short of touching, and one past, are also within rounding.
    let below = f64::from_bits(5.0f64.to_bits() - 1);
    assert!(matches!(
        points_meet(O, acc(below), b, acc(0.0)).unwrap(),
        Meet::Undecided { .. }
    ));
    // But a part in 1e12 either side is decided.
    assert!(matches!(
        points_meet(O, acc(5.0 * (1.0 + 1e-12)), b, acc(0.0)).unwrap(),
        Meet::One { .. }
    ));
    assert_eq!(
        points_meet(O, acc(5.0 * (1.0 - 1e-12)), b, acc(0.0)).unwrap(),
        Meet::Apart
    );
}

#[test]
fn one_point_takes_the_larger_accuracy_and_a_ball_that_reaches_past() {
    let b = Point3::new(3.0, 4.0, 0.0);
    // The same point twice: the larger accuracy, exactly.
    assert_eq!(one(b, 0.0, b, 0.0), 0.0);
    assert_eq!(one(b, 1e-7, b, 2e-7), 2e-7);
    assert_eq!(one(b, 2e-7, b, 1e-7), 2e-7);
    // 1.5e-7 apart at 1e-7 each: one point, its ball reaching past the other's, to
    // 1e-7 + 1.5e-7 = 2.5e-7 (up to the rounding of the distance).
    let near = Point3::new(1.5e-7, 0.0, 0.0);
    let t = one(O, 1e-7, near, 1e-7);
    assert!((2.5e-7..2.5e-7 * (1.0 + 1e-12)).contains(&t), "{t}");
    // And never less: the joined ball holds b's ball, whatever the rounding.
    assert!(t >= 1e-7 + 1.5e-7);
    // 2.1e-7 apart at 1e-7 each: apart.
    assert_eq!(
        points_meet(O, acc(1e-7), Point3::new(2.1e-7, 0.0, 0.0), acc(1e-7)).unwrap(),
        Meet::Apart
    );
}

#[test]
fn the_joined_ball_holds_the_other_at_every_scale() {
    for scale in [1e-9, 1e-3, 1.0, 1e3, 1e6] {
        let a = Point3::new(1.0 * scale, -2.0 * scale, 3.0 * scale);
        let b = Point3::new(a.x + 0.3e-3 * scale, a.y + 0.4e-3 * scale, a.z);
        // |a - b| = 5e-4 scale; the accuracies sum to 6e-4 scale.
        let (ta, tb) = (3e-4 * scale, 3e-4 * scale);
        let t = one(a, ta, b, tb);
        assert!(t >= tb + a.distance(b) * (1.0 - 1e-12), "scale {scale}");
        assert!(t >= ta);
        assert!(t <= (tb + a.distance(b)) * (1.0 + 1e-12), "scale {scale}");
    }
}

#[test]
fn a_point_that_is_not_a_number_is_a_typed_cause() {
    let nan = Point3::new(f64::NAN, 0.0, 0.0);
    assert_eq!(
        points_meet(nan, acc(1.0), O, acc(1.0)),
        Err(MeetError::NotFinite)
    );
    let inf = Point3::new(f64::INFINITY, 0.0, 0.0);
    assert_eq!(
        points_meet(O, acc(1.0), inf, acc(1.0)),
        Err(MeetError::NotFinite)
    );
}

#[test]
fn a_run_twice_is_the_same_to_the_bit() {
    let b = Point3::new(1.5e-7, 0.0, 0.0);
    assert_eq!(
        points_meet(O, acc(1e-7), b, acc(1e-7)),
        points_meet(O, acc(1e-7), b, acc(1e-7))
    );
}
