// Written for solidcorex. Apache-2.0.
//! The stated error bounds the rounding where the terms cancel (research 72 §14.31, MA6):
//! bodies whose faces' fluxes are far larger than what they enclose (a wide thin shell, a
//! small cube far from the origin, a thin slab), each against its closed form. Before, the
//! stated error was the rules' difference alone, which is zero where they are exact, and
//! these stated 0 against a real miss (CK1: the shell (1e4, 1e-3), 0 against 9.3e-9).

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::f64::consts::PI;

use scx_brep::op::Scratch;
use scx_brep::{Brep, Document};
use scx_mass::{MassSpec, RelAccuracy, area, mass, volume};
use scx_num::budget::{Budget, Stop};
use scx_num::math::Affine;
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::{Kind, generate, quarter_shell};

/// A generated block of `size`, scaled per axis by `s` and moved to `at` on each axis.
fn block(size: usize, s: [f64; 3], at: f64) -> Brep {
    let doc = Document::with_resolution(scx_testkit::bodies::build_resolution());
    let mut d = generate(Kind::Block, 0, size).unwrap().draft;
    let map = Affine::from_axes(
        Vec3::new(s[0], 0.0, 0.0),
        Vec3::new(0.0, s[1], 0.0),
        Vec3::new(0.0, 0.0, s[2]),
        Point3::new(at, at, at),
    );
    d.transform(&map, &doc).unwrap();
    d.finish(&doc).map_err(|u| u.cause).unwrap().0
}

/// The quarter cylindrical shell of radius `r`, thickness `t` and length 1.
fn shell(r: f64, t: f64) -> Brep {
    let fine = Document::with_resolution(scx_testkit::bodies::build_resolution());
    let d = quarter_shell(r, t, 1.0).unwrap();
    d.finish(&fine).map_err(|u| u.cause).unwrap().0
}

/// `volume`, and `mass`'s volume, each within its stated error of `want`; `area` within its
/// error of `surface` where given. Returns the volume's stated error.
fn holds(name: &str, b: &Brep, acc: f64, want: f64, surface: Option<f64>) -> f64 {
    let a = RelAccuracy::new(acc).unwrap();
    let run = || (Scratch::new(), Budget::unlimited(), Stop::new());
    let (mut s, mut g, stop) = run();
    let v = volume(b, a, &mut s, &mut g, &stop).unwrap();
    let miss = (v.value - want).abs();
    assert!(
        miss <= v.error,
        "{name} at {acc:e}: volume {v:?}, miss {miss:e}"
    );
    let (mut s, mut g, stop) = run();
    let m = mass(b, &MassSpec::new(a), &mut s, &mut g, &stop).unwrap();
    let miss = (m.volume - want).abs();
    assert!(
        miss <= m.error.volume,
        "{name} at {acc:e}: mass volume {} error {}, miss {miss:e}",
        m.volume,
        m.error.volume
    );
    if let Some(want) = surface {
        let (mut s, mut g, stop) = run();
        let ar = area(b, a, &mut s, &mut g, &stop).unwrap();
        let miss = (ar.value - want).abs();
        assert!(
            miss <= ar.error,
            "{name} at {acc:e}: area {ar:?}, miss {miss:e}"
        );
    }
    v.error
}

/// CK1's case: the quarter shell (1e4, 1e-3), its fluxes cancelling from about 1e8 to 15.7.
/// Its stated error was 0 against a miss of 9.3e-9; it now covers it, and is not wildly
/// above what the cancellation costs (`eps` of the fluxes, about 2e-8, times a few hundred).
#[test]
fn ck1s_shell_states_its_rounding() {
    let b = shell(1e4, 1e-3);
    let want = PI / 4.0 * (2.0 * 1e4 * 1e-3 + 1e-6);
    for acc in [1e-6, 1e-9, 1e-12] {
        let e = holds("shell (1e4, 1e-3)", &b, acc, want, None);
        assert!(e <= 1e-5, "{acc:e}: {e:e}");
    }
}

/// Thinner and wider shells: (1e4, 1e-5) at every accuracy, (1e3, 1e-6) at 1e-6 (at finer
/// accuracies its `mass` is the open `StepControl` of 72 §14.31, before and after).
#[test]
fn thinner_shells_state_their_rounding() {
    let b = shell(1e4, 1e-5);
    let want = PI / 4.0 * (2.0 * 1e4 * 1e-5 + 1e-10);
    for acc in [1e-6, 1e-9, 1e-12] {
        holds("shell (1e4, 1e-5)", &b, acc, want, None);
    }
    let b = shell(1e3, 1e-6);
    let want = PI / 4.0 * (2.0 * 1e3 * 1e-6 + 1e-12);
    holds("shell (1e3, 1e-6)", &b, 1e-6, want, None);
}

/// Small cubes far from the origin: each face's flux is its distance times its area, the
/// volume their difference. Both stated 0 against misses of 2e-21 and 2e-19.
#[test]
fn far_small_cubes_state_their_rounding() {
    for (side, at) in [(2e-4, 1e3), (1e-3, 1e4)] {
        let b = block(1, [side; 3], at);
        for acc in [1e-9, 1e-12] {
            let name = format!("cube {side:e} at {at:e}");
            let e = holds(&name, &b, acc, side * side * side, Some(6.0 * side * side));
            // Within a few hundred `eps` of the fluxes (`at` times the face area).
            assert!(e <= 1e3 * f64::EPSILON * at * side * side, "{name}: {e:e}");
        }
    }
}

/// A slab 1 x 1 x 1e-6 at 1e4, each face cut in nine: many faces, each cancelling.
#[test]
fn a_far_thin_slab_states_its_rounding() {
    let b = block(3, [1.0, 1.0, 1e-6], 1e4);
    holds("slab", &b, 1e-9, 1e-6, Some(2.0 + 4.0 * 1e-6));
}

/// An ordinary body keeps an error near its own rounding: the unit cube at the origin, whose
/// rules are exact, states tens of `eps`, not more.
#[test]
fn an_ordinary_cube_is_not_loosened() {
    let b = block(1, [1.0; 3], 0.0);
    let e = holds("unit cube", &b, 1e-9, 1.0, Some(6.0));
    assert!(e > 0.0 && e <= 100.0 * f64::EPSILON, "{e:e}");
}
