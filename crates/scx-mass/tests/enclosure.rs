// Written for solidcorex. Apache-2.0.
//! The pin's #2972 (`issue_2972_volume_sign_enclosure`) and the outcomes of its enclosure
//! module's tests, on this crate's bodies: certified volume enclosures that hold their
//! closed forms, the sign of every shell, the inside-out body refused by name, thin shells
//! far wider than thick, the gap a moved face leaves closed by strips, and the budget, the
//! stop flag and the width floor.
//!
//! The pin's reversed-loop case is ported to `scx-check`'s contract tests, where finish or
//! the contract refuses each variant. Not ported, each by intent: the fillet case (no
//! fillet here), the face-area enclosures (this crate encloses volumes only), the
//! relative stop (the width asked is absolute), and the gap as large as the volume (this
//! crate's pcurves are exact on the generated thin shell, so its sign decides).

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::f64::consts::PI;

use scx_brep::op::Scratch;
use scx_brep::{Brep, Document, Draft, FaceSide, ShellId};
use scx_mass::{
    MassError, MassRefusal, MassSpec, RelAccuracy, Settle, Sign, enclose_shell, enclose_volume,
    mass, outward, volume, volume_sign,
};
use scx_num::budget::{Budget, Stop};
use scx_num::math::Affine;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::Surface;
use scx_testkit::bodies::{Kind, generate, quarter_shell};

fn v3(a: [f64; 3]) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}

fn finish(d: Draft) -> Brep {
    d.finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap()
        .0
}

/// The seed-0 `kind` draft at `size` mapped by the columns `x`, `y`, `z` and `origin`.
fn mapped_draft(kind: Kind, size: usize, cols: [[f64; 3]; 3], origin: [f64; 3]) -> Draft {
    let mut d = generate(kind, 0, size).unwrap().draft;
    let map = Affine::from_axes(
        v3(cols[0]),
        v3(cols[1]),
        v3(cols[2]),
        Point3::new(origin[0], origin[1], origin[2]),
    );
    d.transform(&map, &Document::default()).unwrap();
    d
}

const EYE: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn scaled(s: f64) -> [[f64; 3]; 3] {
    [[s, 0.0, 0.0], [0.0, s, 0.0], [0.0, 0.0, s]]
}

/// The pin's box: 4 x 5 x 6 with its corner at (1, 2, 3), volume 120.
fn box_draft() -> Draft {
    let cols = [[4.0, 0.0, 0.0], [0.0, 5.0, 0.0], [0.0, 0.0, 6.0]];
    mapped_draft(Kind::Block, 1, cols, [1.0, 2.0, 3.0])
}

/// The draft with every face's sides swapped: the same geometry, set inside out.
fn inside_out(mut d: Draft) -> Draft {
    let ids: Vec<_> = d.faces().map(|f| f.id()).collect();
    for id in ids {
        let rec = d.face_mut(id).unwrap();
        rec.sides.swap(0, 1);
    }
    d
}

fn run() -> (Budget, Stop) {
    (Budget::unlimited(), Stop::new())
}

fn enclose(b: &Brep, width: f64) -> scx_mass::VolumeEnclosure {
    let (mut budget, stop) = run();
    enclose_volume(b, width, &mut budget, &stop).unwrap()
}

/// Every shell's certified sign is the one its region gives it: each region but the
/// outside has one positive (outer) shell, every other shell is negative (the outside's
/// round each solid, a solid's round each void inside it).
fn assert_outward(b: &Brep, what: &str) {
    let (mut budget, stop) = run();
    for r in b.regions() {
        let mut positive: Vec<ShellId> = Vec::new();
        for s in r.shells() {
            match volume_sign(b, s.id(), &mut budget, &stop).unwrap() {
                Sign::Positive => positive.push(s.id()),
                Sign::Negative => {}
                Sign::Zero => panic!("{what}: {} is zero", s.id()),
            }
        }
        let want = usize::from(!r.is_outside());
        assert_eq!(positive.len(), want, "{what}: {positive:?}");
    }
    outward(b, &mut budget, &stop).unwrap();
}

/// The pin's `box_and_its_inside_out_twin` (positive part) and `the_box_*`: the box's
/// enclosure holds 120 to the width asked, and its shells are outward.
#[test]
fn a_box_encloses_its_volume_to_the_width_asked() {
    let b = finish(box_draft());
    for width in [1e-3, 1e-9] {
        let e = enclose(&b, width);
        assert!(e.holds(120.0), "{e:?}");
        assert!(e.width() <= width && e.settle == Settle::Reached, "{e:?}");
    }
    assert_outward(&b, "box");
}

/// The pin's inside-out twin: every face's sides swapped. Its solid's shell is negative,
/// the certificate refuses it by name, and the volume and the mass, which read the
/// regions, refuse it through the guard rather than return -120.
#[test]
fn the_inside_out_box_is_refused_by_name() {
    let b = finish(inside_out(box_draft()));
    let solid = b
        .regions()
        .find(|r| r.is_solid())
        .unwrap()
        .shells()
        .next()
        .unwrap()
        .id();
    let (mut budget, stop) = run();
    assert_eq!(
        volume_sign(&b, solid, &mut budget, &stop),
        Ok(Sign::Negative)
    );
    let refused = |e: &MassError| matches!(e, MassError::Refused(MassRefusal::NotOutward { .. }));
    let e = outward(&b, &mut budget, &stop).unwrap_err();
    assert!(refused(&e), "{e:?}");
    let acc = RelAccuracy::new(1e-9).unwrap();
    let e = volume(&b, acc, &mut Scratch::new(), &mut budget, &stop).unwrap_err();
    assert!(refused(&e), "{e:?}");
    let spec = MassSpec::new(acc);
    let e = mass(&b, &spec, &mut Scratch::new(), &mut budget, &stop).unwrap_err();
    assert!(refused(&e), "{e:?}");
}

/// The pin's sphere seam and poles, and the curved kinds: each enclosure holds its closed
/// form at a relative width of 1e-6, and every shell is outward (the torus's patch crosses
/// both seams, the cone and the sphere have poles; the cavity's void shell faces in; the
/// touching blocks meet along an edge, volume 2).
#[test]
fn the_curved_kinds_hold_their_closed_forms() {
    let cases = [
        (Kind::Cylinder, PI * 2.0),
        (Kind::Cone, PI / 3.0),
        (Kind::Sphere, 4.0 * PI / 3.0),
        (Kind::Torus, 2.0 * PI * PI * 2.0),
        (Kind::Cavity, 26.0),
        (Kind::Touching, 2.0),
    ];
    for (kind, want) in cases {
        let size = if kind == Kind::Touching { 1 } else { 2 };
        let b = finish(mapped_draft(kind, size, EYE, [0.0; 3]));
        let width = 1e-6 * want;
        let e = enclose(&b, width);
        assert!(e.holds(want) && e.width() <= width, "{kind:?}: {e:?}");
        assert_outward(&b, &format!("{kind:?}"));
    }
}

/// The pin's thin shells: a quarter cylindrical shell far wider than thick is positive
/// and its enclosure holds `pi / 4 ((r + t)^2 - r^2) L`. The pin's (10, 1e-6) case,
/// undecided there (its gap as large as the volume), decides here: its pcurves are exact.
/// A thickness below the default resolution needs a finer one.
#[test]
fn thin_shells_are_positive() {
    let fine = Document::with_resolution(scx_testkit::bodies::build_resolution());
    for (r, t) in [(1e4, 1e-3), (10.0, 1e-4), (10.0, 1e-6)] {
        let d = quarter_shell(r, t, 1.0).unwrap();
        let b = d.finish(&fine).map_err(|u| u.cause).unwrap().0;
        let want = PI / 4.0 * (2.0 * r * t + t * t);
        assert_outward(&b, &format!("r {r} t {t}"));
        let e = enclose(&b, 1e-3 * want);
        assert!(e.holds(want), "r {r} t {t}: {e:?}");
    }
}

/// The estimate reads the thin shells too: each cap's region is proved (its arcs' areas in
/// closed form, 72 §14.22), so every face has a box and `volume` holds the closed form
/// within its stated error (it was `NoBox` on the caps before).
#[test]
fn a_thin_shells_estimate_holds_its_closed_form() {
    let fine = Document::with_resolution(scx_testkit::bodies::build_resolution());
    for (r, t) in [(1e4, 1e-3), (10.0, 1e-4), (10.0, 1e-6)] {
        let b = quarter_shell(r, t, 1.0)
            .unwrap()
            .finish(&fine)
            .map_err(|u| u.cause)
            .unwrap()
            .0;
        let want = PI / 4.0 * (2.0 * r * t + t * t);
        let acc = RelAccuracy::new(1e-9).unwrap();
        let e = volume(
            &b,
            acc,
            &mut Scratch::new(),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap();
        let miss = (e.value - want).abs();
        // Within the stated error: the faces' fluxes cancel from 1e8 on the widest shell,
        // and their rounding is in the error (72 §14.31; it was 0 against a miss of 9e-9).
        assert!(miss <= e.error, "r {r} t {t}: {e:?}, miss {miss:e}");
    }
}

/// The pin's far thin shell: the same shell carried 1e4 away stays outward.
#[test]
fn a_far_thin_shell_is_outward() {
    let mut d = quarter_shell(10.0, 1e-4, 1.0).unwrap();
    let map = Affine::from_axes(
        v3(EYE[0]),
        v3(EYE[1]),
        v3(EYE[2]),
        Point3::new(1e4, -1e4, 1e4),
    );
    d.transform(&map, &Document::default()).unwrap();
    let b = finish(d);
    assert_outward(&b, "far shell");
    let want = PI / 4.0 * (2.0 * 10.0 * 1e-4 + 1e-8);
    let e = enclose(&b, 1e-2 * want);
    assert!(e.holds(want), "{e:?}");
}

/// The pin's `gap_closed_by_strips`: the box's face at x = 5 moved 5e-5 outward (inside
/// the gap a finished body allows its uses), its edges left. The strips between its trims
/// and the edges close the gap, so the enclosure holds `120 + 30 * 5e-5` (the slab the
/// face moved over) and the sign is positive. A strip is bounded, not integrated (its
/// largest `|z - z0|` times its area), so the width cannot fall below twice the strips'
/// bounds, here about 7e-3: asked for 1e-2, it is reached.
#[test]
fn a_gap_is_closed_by_strips() {
    const D: f64 = 5e-5;
    let found = finish(box_draft());
    let id = found
        .faces()
        .find(|f| f.bounding_box().is_some_and(|b| b.x_min() > 4.5))
        .unwrap()
        .id();
    let mut d = box_draft();
    let rec = d.face_mut(id).unwrap();
    let Surface::Plane(p) = rec.surface.clone() else {
        panic!("a box face is a plane");
    };
    let shift = Affine::from_axes(v3(EYE[0]), v3(EYE[1]), v3(EYE[2]), Point3::new(D, 0.0, 0.0));
    rec.surface = Surface::Plane(p.transformed(&shift).unwrap().0);
    let b = finish(d);
    let e = enclose(&b, 1e-2);
    assert!(
        e.holds(120.0 + 30.0 * D) && e.settle == Settle::Reached,
        "{e:?}"
    );
    assert_outward(&b, "gapped box");
}

/// A sheet's shell has no face counted once: its sign is exactly zero.
#[test]
fn a_sheet_shell_is_zero() {
    let b = finish(mapped_draft(Kind::QuadSheet, 1, EYE, [0.0; 3]));
    let (mut budget, stop) = run();
    for s in b.shells() {
        assert_eq!(volume_sign(&b, s.id(), &mut budget, &stop), Ok(Sign::Zero));
    }
}

/// The pin's `a_budget_of_zero_refines_nothing` and `a_spent_budget_leaves_the_sign_undecided`:
/// a budget that cannot pay is exhausted, never a sign; a set stop flag cancels.
#[test]
fn the_budget_and_the_stop_flag_end_it() {
    let b = finish(mapped_draft(Kind::Sphere, 2, EYE, [0.0; 3]));
    let shell = b
        .regions()
        .find(|r| r.is_solid())
        .unwrap()
        .shells()
        .next()
        .unwrap()
        .id();
    for limit in [0, 10] {
        let mut budget = Budget::new(limit);
        let e = volume_sign(&b, shell, &mut budget, &Stop::new());
        assert_eq!(e, Err(MassError::Exhausted { limit }), "{limit}");
    }
    let stop = Stop::new();
    stop.set();
    let e = enclose_volume(&b, 1e-6, &mut Budget::unlimited(), &stop);
    assert_eq!(e, Err(MassError::Cancelled));
}

/// The pin's `spending_never_widens`: asking past the floor costs cells but never widens
/// the enclosure, and both hold 120.
#[test]
fn spending_never_widens() {
    let b = finish(box_draft());
    let coarse = enclose(&b, 1e-9);
    let fine = enclose(&b, f64::MIN_POSITIVE);
    assert!(
        coarse.holds(120.0) && fine.holds(120.0),
        "{coarse:?} {fine:?}"
    );
    assert!(fine.width() <= coarse.width(), "{coarse:?} {fine:?}");
    assert!(fine.cells >= coarse.cells);
    assert_ne!(fine.settle, Settle::Reached, "{fine:?}");
}

/// The pin's `width_below_the_floor`: a sphere of radius 2 asked ever narrower: the widths
/// fall, each is reached exactly when it is at most the width asked, and the last is
/// within 1e-9.
///
/// One test per width, so the runner checks them in parallel (the narrowest alone takes
/// about a minute, a call that runs to the cell cap, the others a few seconds). Each
/// asks the next wider width again for the one it must not exceed, as the loop's
/// `last` carried it; it returns the width it reached.
fn width_falls(width: f64, wider: Option<f64>) -> f64 {
    let b = finish(mapped_draft(Kind::Sphere, 2, scaled(2.0), [0.0; 3]));
    let want = 32.0 * PI / 3.0;
    // The wider width's own call runs beside this one, so it adds no time to the narrowest.
    let (last, e) = std::thread::scope(|s| {
        let last = s.spawn(|| wider.map_or(f64::INFINITY, |w| enclose(&b, w).width()));
        let e = enclose(&b, width);
        (last.join().unwrap(), e)
    });
    assert!(e.holds(want), "{width}: {e:?}");
    assert!(e.width() <= last, "{width}: {e:?}");
    assert_eq!(
        e.settle == Settle::Reached,
        e.width() <= width,
        "{width}: {e:?}"
    );
    e.width()
}

#[test]
fn width_falls_to_1e_6() {
    width_falls(1e-6, None);
}

#[test]
fn width_falls_to_1e_8() {
    width_falls(1e-8, Some(1e-6));
}

#[test]
fn widths_fall_to_the_floor() {
    let last = width_falls(1e-11, Some(1e-8));
    assert!(last <= 1e-9, "{last}");
}

/// A width that is not finite and at least zero, and a shell the body does not have, are
/// refused by name.
#[test]
fn bad_requests_are_refused() {
    let b = finish(box_draft());
    let (mut budget, stop) = run();
    for w in [-1.0, f64::NAN, f64::INFINITY] {
        match enclose_volume(&b, w, &mut budget, &stop) {
            Err(MassError::Refused(MassRefusal::BadWidth { .. })) => {}
            other => panic!("{w}: {other:?}"),
        }
    }
    let cavity = finish(mapped_draft(Kind::Cavity, 1, EYE, [0.0; 3]));
    let foreign = cavity.shells().map(|s| s.id()).max().unwrap();
    assert!(b.shells().all(|s| s.id() != foreign));
    let e = enclose_shell(&b, foreign, 1.0, &mut budget, &stop);
    assert_eq!(
        e,
        Err(MassError::Refused(MassRefusal::NoShell { shell: foreign }))
    );
}

/// The sign is read from each shell's side: the box's outer shell is positive from the
/// solid and the outside's shell round it negative, by the same magnitude.
#[test]
fn a_shell_is_counted_from_its_own_side() {
    let b = finish(box_draft());
    let (mut budget, stop) = run();
    let f = b.faces().next().unwrap();
    let (back, front) = (f.shell(FaceSide::Back), f.shell(FaceSide::Front));
    let inner = enclose_shell(&b, back.id(), 1e-9, &mut budget, &stop).unwrap();
    let outer = enclose_shell(&b, front.id(), 1e-9, &mut budget, &stop).unwrap();
    assert!(
        inner.holds(120.0) && outer.holds(-120.0),
        "{inner:?} {outer:?}"
    );
}
