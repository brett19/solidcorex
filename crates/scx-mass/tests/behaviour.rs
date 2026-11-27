// Written for solidcorex. Apache-2.0.
//! The pin's behavioural tests of `mass::integrator::properties` and its #16735 and
//! #2887 integration tests, on this crate's bodies: exact boxes, a mirror, the refusals,
//! the stop flag and the budget, determinism, far origins, and edge lengths.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::f64::consts::{PI, TAU};

use scx_brep::op::Scratch;
use scx_brep::{Brep, Document, EdgeShape};
use scx_mass::{
    Mass, MassError, MassRefusal, MassSpec, RelAccuracy, area, edge_length, mass, volume,
};
use scx_num::budget::{Budget, Stop};
use scx_num::math::Affine;
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::{Kind, generate};

fn acc(a: f64) -> RelAccuracy {
    RelAccuracy::new(a).unwrap()
}

/// The seed-0 `kind` body at size 1 mapped by `x`, `y`, `z` (columns) and `origin`.
fn mapped(kind: Kind, x: [f64; 3], y: [f64; 3], z: [f64; 3], origin: [f64; 3]) -> Brep {
    let doc = Document::default();
    let mut d = generate(kind, 0, 1).unwrap().draft;
    let v = |a: [f64; 3]| Vec3::new(a[0], a[1], a[2]);
    let map = Affine::from_axes(
        v(x),
        v(y),
        v(z),
        Point3::new(origin[0], origin[1], origin[2]),
    );
    d.transform(&map, &doc).unwrap();
    d.finish(&doc).map_err(|u| u.cause).unwrap().0
}

/// The unit block scaled to `a` x `b` x `c` with its corner at `at`.
fn cuboid(a: f64, b: f64, c: f64, at: [f64; 3]) -> Brep {
    mapped(Kind::Block, [a, 0.0, 0.0], [0.0, b, 0.0], [0.0, 0.0, c], at)
}

fn measure(b: &Brep, spec: &MassSpec) -> Mass {
    mass(
        b,
        spec,
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
}

/// `a_plate_box_is_measured_exactly`: a thin plate's volume and area, exact to rounding.
#[test]
fn a_plate_is_measured_exactly() {
    let b = cuboid(100.0, 100.0, 2.0, [0.0; 3]);
    let m = measure(&b, &MassSpec::new(acc(1e-6)));
    assert!((m.volume - 20000.0).abs() <= 1e-9 * 20000.0, "{}", m.volume);
    assert!((m.area - 20800.0).abs() <= 1e-9 * 20800.0, "{}", m.area);
    let c = m.centroid().unwrap();
    assert!(
        (c - Point3::new(50.0, 50.0, 1.0)).length() <= 1e-9 * 100.0,
        "{c:?}"
    );
}

/// `centroidal_mass_properties_of_a_box`: a 1 x 2 x 3 box's centroid and inertia, at a
/// density.
#[test]
fn a_boxs_centroidal_inertia() {
    let b = cuboid(1.0, 2.0, 3.0, [-4.0, 5.0, 0.5]);
    let m = measure(&b, &MassSpec::new(acc(1e-9)).with_density(2.5));
    let mass = 6.0 * 2.5;
    assert!((m.mass() - mass).abs() <= 1e-12 * mass);
    let c = m.centroid().unwrap();
    assert!(
        (c - Point3::new(-3.5, 6.0, 2.0)).length() <= 1e-12 * 10.0,
        "{c:?}"
    );
    let i = m.inertia().unwrap();
    let want = [
        mass * (4.0 + 9.0) / 12.0,
        mass * (1.0 + 9.0) / 12.0,
        mass * (1.0 + 4.0) / 12.0,
    ];
    for (got, want) in [i.xx, i.yy, i.zz].into_iter().zip(want) {
        assert!(
            (got - want).abs() <= 1e-11 * mass * 9.0,
            "{got} against {want}"
        );
    }
    for p in [i.xy, i.yz, i.zx] {
        assert!(p.abs() <= 1e-11 * mass * 9.0, "product {p}");
    }
}

/// `centroidal_mass_properties_of_a_sphere`: radius 2, off the origin.
#[test]
fn a_spheres_centroidal_inertia() {
    let b = mapped(
        Kind::Sphere,
        [2.0, 0.0, 0.0],
        [0.0, 2.0, 0.0],
        [0.0, 0.0, 2.0],
        [3.0, -1.0, 7.0],
    );
    let m = measure(&b, &MassSpec::new(acc(1e-10)));
    let v = 4.0 / 3.0 * PI * 8.0;
    assert!((m.volume - v).abs() <= 1e-10 * v, "{}", m.volume);
    assert!(
        (m.area - 16.0 * PI).abs() <= 1e-10 * 16.0 * PI,
        "{}",
        m.area
    );
    let c = m.centroid().unwrap();
    assert!((c - Point3::new(3.0, -1.0, 7.0)).length() <= 1e-9, "{c:?}");
    let i = m.inertia().unwrap();
    let want = 0.4 * v * 4.0;
    for got in [i.xx, i.yy, i.zz] {
        assert!((got - want).abs() <= 1e-9 * want, "{got} against {want}");
    }
}

/// A mirror swaps each face's sides: the volume stays positive, the moments mirror.
#[test]
fn a_mirrored_body_keeps_its_volume() {
    let plain = mapped(
        Kind::Cone,
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 2.0, 3.0],
    );
    let mirror = mapped(
        Kind::Cone,
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [-1.0, 2.0, 3.0],
    );
    let spec = MassSpec::new(acc(1e-10)).with_about(Point3::new(0.0, 0.0, 0.0));
    let (a, b) = (measure(&plain, &spec), measure(&mirror, &spec));
    assert!(
        a.volume > 0.0 && (a.volume - PI / 3.0).abs() <= 1e-10,
        "{}",
        a.volume
    );
    assert!(
        (a.volume - b.volume).abs() <= 1e-12,
        "{} against {}",
        a.volume,
        b.volume
    );
    assert!((a.first.x + b.first.x).abs() <= 1e-10 && (a.first.z - b.first.z).abs() <= 1e-10);
}

/// #16735's `a_box_has_no_leak`: a 4 x 5 x 6 box measured about points near and far from
/// it, and moved far from the origin, keeps its volume to rounding.
#[test]
fn a_box_has_no_leak() {
    for at in [[1.0, -2.0, 3.0], [1e3, -2e3, 5e2], [1e6, 1e6, -1e6]] {
        let b = cuboid(4.0, 5.0, 6.0, at);
        for about in [
            None,
            Some(Point3::new(0.0, 0.0, 0.0)),
            Some(Point3::new(at[0] + 2.0, at[1], at[2])),
        ] {
            let spec = MassSpec::new(acc(1e-9));
            let spec = match about {
                Some(p) => spec.with_about(p),
                None => spec,
            };
            let m = measure(&b, &spec);
            let far = at.iter().map(|x| x.abs()).fold(1.0, f64::max);
            let rounding =
                if about.is_some_and(|p| (p - Point3::new(at[0], at[1], at[2])).length() > 10.0) {
                    1e-15 * far * 148.0 * 8.0
                } else {
                    1e-12
                };
            assert!(
                (m.volume - 120.0).abs() <= 1e-12 * 120.0 + rounding,
                "{at:?} {about:?}: {}",
                m.volume
            );
            assert!(
                (m.area - 148.0).abs() <= 1e-12 * 148.0,
                "{at:?}: {}",
                m.area
            );
        }
        let v = volume(
            &b,
            acc(1e-9),
            &mut Scratch::new(),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap();
        assert!((v.value - 120.0).abs() <= 1e-12 * 120.0, "{at:?}: {v:?}");
    }
}

/// A sheet encloses nothing and has the area of its faces.
#[test]
fn a_sheet_has_area_and_no_volume() {
    for kind in [Kind::QuadSheet, Kind::TriSheet] {
        let b = mapped(
            kind,
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0; 3],
        );
        let m = measure(&b, &MassSpec::new(acc(1e-9)));
        assert_eq!(m.volume, 0.0);
        assert!(m.centroid().is_none() && m.inertia().is_none());
        assert!((m.area - 1.0).abs() <= 1e-12, "{}", m.area);
        assert!(m.area_centroid().is_some());
    }
}

/// `an_estimate_refuses_or_clamps_its_accuracy`, and the spec's other refusals.
#[test]
fn bad_inputs_are_refused_by_name() {
    for a in [0.0, -1e-3, 1.5, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            RelAccuracy::new(a),
            Err(MassError::Refused(MassRefusal::BadAccuracy { .. }))
        ));
    }
    let b = cuboid(1.0, 1.0, 1.0, [0.0; 3]);
    let run = |spec: MassSpec| {
        mass(
            &b,
            &spec,
            &mut Scratch::new(),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
    };
    for d in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            run(MassSpec::new(acc(1e-6)).with_density(d)),
            Err(MassError::Refused(MassRefusal::BadDensity { .. }))
        ));
    }
    assert!(matches!(
        run(MassSpec::new(acc(1e-6)).with_about(Point3::new(f64::NAN, 0.0, 0.0))),
        Err(MassError::Refused(MassRefusal::BadReference))
    ));
}

/// `integration_failures_are_typed_by_cause`: the stop flag and the budget end the work by
/// name.
#[test]
fn stop_and_budget_end_the_work() {
    let b = mapped(
        Kind::Torus,
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0; 3],
    );
    let stop = Stop::new();
    stop.set();
    let r = volume(
        &b,
        acc(1e-9),
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &stop,
    );
    assert!(matches!(r, Err(MassError::Cancelled)), "{r:?}");
    let mut budget = Budget::new(100);
    let r = mass(
        &b,
        &MassSpec::new(acc(1e-9)),
        &mut Scratch::new(),
        &mut budget,
        &Stop::new(),
    );
    assert!(matches!(r, Err(MassError::Exhausted { .. })), "{r:?}");
    let mut budget = Budget::unlimited();
    volume(
        &b,
        acc(1e-9),
        &mut Scratch::new(),
        &mut budget,
        &Stop::new(),
    )
    .unwrap();
    assert!(budget.used() > 0, "the work is charged");
}

/// The same body measured twice gives the same bits.
#[test]
fn results_are_deterministic() {
    for kind in [Kind::Torus, Kind::Cavity, Kind::Holed] {
        let b = mapped(
            kind,
            [0.6, 0.8, 0.0],
            [-0.8, 0.6, 0.0],
            [0.0, 0.0, 1.0],
            [3.0, 1.0, -2.0],
        );
        let spec = MassSpec::new(acc(1e-8));
        assert_eq!(measure(&b, &spec), measure(&b, &spec));
        let a = |b: &Brep| {
            area(
                b,
                acc(1e-8),
                &mut Scratch::new(),
                &mut Budget::unlimited(),
                &Stop::new(),
            )
            .unwrap()
        };
        assert_eq!(a(&b), a(&b));
    }
}

/// `edge_properties_hold_their_accuracy`'s lengths: a circle's arcs near and far from the
/// origin and a block's segments, each within its accuracy, the estimate covering the miss.
#[test]
fn edge_lengths_hold_their_accuracy() {
    for at in [[0.0; 3], [0.37, -0.29, 0.41], [1e6, -1e6, 1e6]] {
        let cyl = mapped(
            Kind::Cylinder,
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            at,
        );
        let blk = cuboid(2.0, 3.0, 4.0, at);
        for e in [4, 7, 9, 11] {
            let a = scx_num::fmath::powi(10.0, -e);
            let len = |b: &Brep| -> (f64, f64, f64) {
                let mut sum = (0.0, 0.0, 0.0);
                for edge in b.edges() {
                    let EdgeShape::Curve { .. } = edge.shape() else {
                        continue;
                    };
                    let l = edge_length(
                        edge,
                        acc(a),
                        &mut Scratch::new(),
                        &mut Budget::unlimited(),
                        &Stop::new(),
                    )
                    .unwrap();
                    sum.0 += l.value;
                    sum.1 += l.error;
                    sum.2 = f64::max(sum.2, l.value);
                }
                sum
            };
            // The circles' arcs add to two turns of radius 1; the edges along the axis to
            // the height 2 each.
            let (total, err, _) = len(&cyl);
            let (circles, lines) = (
                2.0 * TAU,
                cyl.edges().filter(|e| is_line(e.shape())).count() as f64 * 2.0,
            );
            let want = circles + lines;
            let miss = (total - want).abs();
            assert!(miss <= a * want, "{at:?} 1e-{e}: {total} against {want}");
            assert!(
                miss <= err.max(1e-15 * want.max(at[0].abs())),
                "{at:?} 1e-{e}: miss {miss:e} est {err:e}"
            );
            let (total, _, _) = len(&blk);
            let want = 4.0 * (2.0 + 3.0 + 4.0);
            assert!(
                (total - want).abs() <= 1e-12 * want.max(at[0].abs()),
                "{at:?}: {total}"
            );
        }
    }
}

fn is_line(shape: &EdgeShape) -> bool {
    matches!(
        shape,
        EdgeShape::Curve {
            curve: scx_curve::Curve::Line(_),
            ..
        }
    )
}
