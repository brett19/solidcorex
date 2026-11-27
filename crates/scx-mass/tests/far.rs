// Written for solidcorex. Apache-2.0.
//! An accuracy that cannot be reached is never a defect (research 72 §14.18, MA3): far from
//! the origin or at an accuracy near rounding, a measurement meets it, or says by a typed
//! cause that it cannot, and states the error it reached.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use scx_brep::op::Scratch;
use scx_brep::{Brep, Document};
use scx_mass::{MassError, MassSpec, RelAccuracy, mass, volume};
use scx_num::budget::{Budget, Stop};
use scx_num::math::Affine;
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::{Kind, generate};

/// The block kind with sides `side` at `(at, at, at)`.
fn cube(side: f64, at: f64) -> Brep {
    let doc = Document::default();
    let mut d = generate(Kind::Block, 0, 1).unwrap().draft;
    let map = Affine::from_axes(
        Vec3::new(side, 0.0, 0.0),
        Vec3::new(0.0, side, 0.0),
        Vec3::new(0.0, 0.0, side),
        Point3::new(at, at, at),
    );
    d.transform(&map, &doc).unwrap();
    d.finish(&doc).map_err(|u| u.cause).unwrap().0
}

/// `mass` and `volume` of `b` at `acc`, about `about`, in a bounded budget: neither is a
/// defect and neither runs on.
fn bounded(name: &str, b: &Brep, acc: f64, about: Option<Point3>) {
    let a = RelAccuracy::new(acc).unwrap();
    let spec = match about {
        Some(p) => MassSpec::new(a).with_about(p),
        None => MassSpec::new(a),
    };
    let mut budget = Budget::new(20_000_000);
    let m = mass(b, &spec, &mut Scratch::new(), &mut budget, &Stop::new());
    if let Err(e) = &m {
        assert!(
            !matches!(e, MassError::Defect(_)),
            "{name} {acc:e} mass: {e}"
        );
    }
    let mut budget = Budget::new(20_000_000);
    let v = volume(b, a, &mut Scratch::new(), &mut budget, &Stop::new());
    if let Err(e) = &v {
        assert!(
            !matches!(e, MassError::Defect(_)),
            "{name} {acc:e} volume: {e}"
        );
    }
}

/// MK2b's smallest case: a 2e-4 cube at 1e3.
#[test]
fn a_tiny_cube_far_out() {
    let b = cube(2e-4, 1e3);
    let at = Point3::new(1e3, 1e3, 1e3);
    for acc in [1e-9, 1e-12, 1e-13] {
        bounded("2e-4 at 1e3", &b, acc, Some(at));
    }
}

/// A 0.1 cube at 1e8.
#[test]
fn a_small_cube_very_far_out() {
    let b = cube(0.1, 1e8);
    for acc in [1e-6, 1e-9, 1e-12] {
        bounded("0.1 at 1e8", &b, acc, None);
    }
}

/// A 5e3 cube at 1e12: bounded, whatever it returns.
#[test]
fn a_large_cube_at_1e12_ends() {
    let b = cube(5e3, 1e12);
    for acc in [1e-9, 1e-12, 1e-13] {
        bounded("5e3 at 1e12", &b, acc, None);
    }
}

/// Every kind far out at tight accuracies.
#[test]
fn every_kind_far_out_is_never_a_defect() {
    for kind in Kind::ALL {
        for at in [1e6, 1e9, 1e12] {
            let doc = Document::default();
            let mut d = generate(kind, 0, 2).unwrap().draft;
            let map = Affine::from_axes(
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                Point3::new(at, at, at),
            );
            d.transform(&map, &doc).unwrap();
            // A body too far out for the resolution is refused by `finish`, not a case here.
            let Ok((b, _)) = d.finish(&doc) else { continue };
            for acc in [1e-10, 1e-13] {
                bounded(kind.name(), &b, acc, None);
            }
        }
    }
}

/// What a far cube returns holds: the volume within its stated error, and the stated error
/// at least what the coordinates allow when the accuracy asked is below that.
#[test]
fn a_far_cubes_stated_error_covers_its_miss() {
    for (side, at) in [(2e-4, 1e3), (0.1, 1e8), (5e3, 1e12)] {
        let b = cube(side, at);
        let want = side * side * side;
        for acc in [1e-9, 1e-12, 1e-13] {
            let spec =
                MassSpec::new(RelAccuracy::new(acc).unwrap()).with_about(Point3::new(at, at, at));
            let m = mass(
                &b,
                &spec,
                &mut Scratch::new(),
                &mut Budget::new(20_000_000),
                &Stop::new(),
            )
            .unwrap();
            let miss = (m.volume - want).abs();
            assert!(
                miss <= m.error.volume.max(acc * want),
                "{side:e} at {at:e}, {acc:e}: miss {miss:e}, stated {:e}",
                m.error.volume
            );
            // Where the coordinates cannot give the accuracy, the stated error says so.
            let floor = 8.0 * f64::EPSILON * at / side;
            if (side, at, acc) == (5e3, 1e12, 1e-13) {
                assert!(
                    m.error.volume >= 0.5 * floor * want,
                    "stated {:e}",
                    m.error.volume
                );
            }
        }
    }
}

/// An edge far out: the length within its stated error, never a defect.
#[test]
fn a_far_edges_length_holds_its_stated_error() {
    use scx_mass::edge_length;
    let b = cube(5e3, 1e12);
    let e = b.edges().next().unwrap();
    for acc in [1e-9, 1e-13] {
        let r = edge_length(
            e,
            RelAccuracy::new(acc).unwrap(),
            &mut Scratch::new(),
            &mut Budget::new(20_000_000),
            &Stop::new(),
        )
        .unwrap();
        assert!(
            (r.value - 5e3).abs() <= r.error.max(1e-9 * 5e3),
            "{acc:e}: {r:?}"
        );
    }
}
