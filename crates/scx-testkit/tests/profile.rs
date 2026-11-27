// Written for solidcorex. Apache-2.0.
//! The profile fixtures are consistent with closed forms, and cover every kind.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use scx_testkit::profile::{Cause, Verdict, fixtures, loop_area, reversed, self_check};
use std::f64::consts::PI;

#[test]
fn every_fixture_is_self_consistent() {
    for f in fixtures() {
        assert!(
            self_check(&f).is_empty(),
            "{}: {:?}",
            f.name,
            self_check(&f)
        );
    }
}

#[test]
fn names_are_unique_and_every_kind_is_covered() {
    let all = fixtures();
    let mut names: Vec<_> = all.iter().map(|f| f.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), all.len());
    let has = |p: fn(&Verdict) -> bool| all.iter().any(|f| p(&f.verdict));
    assert!(has(|v| matches!(v, Verdict::Refused(Cause::Open { .. }))));
    assert!(has(|v| matches!(v, Verdict::Refused(Cause::Crossing))));
    assert!(has(|v| matches!(v, Verdict::Refused(Cause::Touching))));
    assert!(has(|v| matches!(
        v,
        Verdict::Refused(Cause::HoleOutside { .. })
    )));
    assert!(has(|v| matches!(
        v,
        Verdict::Refused(Cause::HoleInHole { .. })
    )));
    assert!(has(|v| matches!(
        v,
        Verdict::Accepted {
            outer_reversed: true,
            ..
        }
    )));
    assert!(has(
        |v| matches!(v, Verdict::Accepted { holes_reversed, .. } if holes_reversed.contains(&true))
    ));
    assert!(has(
        |v| matches!(v, Verdict::Accepted { holes_reversed, .. } if !holes_reversed.is_empty())
    ));
}

#[test]
fn areas_match_hand_values() {
    let by = |n: &str| fixtures().into_iter().find(|f| f.name == n).unwrap();
    assert!((loop_area(&by("closed square").outer) - 16.0).abs() < 1e-14);
    assert!((loop_area(&by("closed circle of two arcs").outer) - 25.0 * PI).abs() < 1e-12);
    assert!((loop_area(&by("closed D of a line and an arc").outer) - 12.5 * PI).abs() < 1e-12);
    // The rounded top: the rectangle plus the segment, computed here from the sector minus the
    // triangle: sector 25 * theta / 2 with theta = 2 asin(3/5), triangle 6 * 4 / 2 = 12, so the
    // segment is 25 theta / 2 - 12 ... measured from the origin the cap is sector - triangle.
    let theta = 2.0 * scx_num::fmath::atan2(3.0, 4.0);
    let cap = 12.5 * theta - 12.0;
    assert!((loop_area(&by("closed rounded top").outer) - (24.0 + cap)).abs() < 1e-12);
}

#[test]
fn reversing_negates_the_area() {
    for f in fixtures() {
        let a = loop_area(&f.outer);
        let b = loop_area(&reversed(&f.outer));
        assert!((a + b).abs() <= 1e-12 * a.abs().max(1.0), "{}", f.name);
    }
}
