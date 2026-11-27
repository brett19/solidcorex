// Written for solidcorex. Apache-2.0.
//! `scx-mass` (research 72 MA1) against the pin's `mass::integrator::properties`
//! (`brep_volume_default`, `brep_area`, `brep_mass_properties_default`) on the seed-0
//! primitives the pin also builds (`t_bodies`): the volume, the area and the centroid.
//! Each side is judged against the closed form (`scx_testkit::solid`), not against the
//! other: both must hold the accuracy asked; the misses are printed side by side.

use scx_brep::Document;
use scx_brep::op::Scratch;
use scx_mass::{MassSpec, RelAccuracy, area, mass, volume};
use scx_num::budget::{Budget, Stop};
use scx_num::vector3d::Point3;
use scx_testkit::bodies::{Kind, generate};
use scx_testkit::solid;
use solidcore::Kernel;
use solidcore::mass::integrator::properties as pin;
use solidcore::primitive::api_primitives as ap;

const ACCURACIES: [f64; 3] = [1e-6, 1e-9, 1e-11];

fn cases() -> [(Kind, solid::Solid); 5] {
    [
        (Kind::Block, solid::block(1.0, 1.0, 1.0).unwrap()),
        (Kind::Cylinder, solid::cylinder(1.0, 2.0).unwrap()),
        (Kind::Cone, solid::cone(1.0, 0.0, 1.0).unwrap()),
        (Kind::Sphere, solid::sphere(1.0).unwrap()),
        (Kind::Torus, solid::torus(2.0, 1.0).unwrap()),
    ]
}

fn pin_body(kind: Kind, k: &mut Kernel) -> solidcore::Brep {
    let o = solidcore::Point3::new(0.0, 0.0, 0.0);
    match kind {
        Kind::Block => ap::create_box(k, o, 1.0, 1.0, 1.0),
        Kind::Cylinder => ap::create_cylinder(k, o, 1.0, 2.0),
        Kind::Cone => ap::create_cone(k, o, 1.0, 0.0, 1.0),
        Kind::Sphere => ap::create_sphere(k, o, 1.0),
        _ => ap::create_torus(k, o, 2.0, 1.0),
    }
    .unwrap()
}

#[test]
fn mass_against_pin_and_truth() {
    let doc = Document::default();
    for (kind, want) in cases() {
        let b = generate(kind, 0, 1).unwrap().draft;
        let b = b.finish(&doc).map_err(|u| u.cause).unwrap().0;
        let mut k = Kernel::default();
        let mut p = pin_body(kind, &mut k);
        let (wv, wa) = (want.volume(), want.area);
        let wc = want.moments.centroid();
        for acc in ACCURACIES {
            let r = RelAccuracy::new(acc).unwrap();
            let ours_v = volume(
                &b,
                r,
                &mut Scratch::new(),
                &mut Budget::unlimited(),
                &Stop::new(),
            )
            .unwrap()
            .value;
            let ours_a = area(
                &b,
                r,
                &mut Scratch::new(),
                &mut Budget::unlimited(),
                &Stop::new(),
            )
            .unwrap()
            .value;
            let spec = MassSpec::new(r).with_about(Point3::new(0.0, 0.0, 0.0));
            let m = mass(
                &b,
                &spec,
                &mut Scratch::new(),
                &mut Budget::unlimited(),
                &Stop::new(),
            )
            .unwrap();
            let ours_c = m.centroid().unwrap();
            let pin_v = pin::brep_volume_default(&k, &mut p, acc).unwrap();
            let pin_a = pin::brep_area(&k, &mut p, acc).unwrap();
            let pm = pin::brep_mass_properties_default(
                &k,
                &mut p,
                acc,
                1.0,
                solidcore::Point3::new(0.0, 0.0, 0.0),
            )
            .unwrap();
            let pin_c = [pm.centroid.x, pm.centroid.y, pm.centroid.z];
            let ours_c = [ours_c.x, ours_c.y, ours_c.z];
            let miss_c = |c: [f64; 3]| (0..3).map(|i| (c[i] - wc[i]).abs()).fold(0.0, f64::max);
            let name = kind.name();
            eprintln!(
                "{name} {acc:e}: volume miss ours {:e} pin {:e}; area ours {:e} pin {:e}; centroid ours {:e} pin {:e}",
                (ours_v - wv).abs(),
                (pin_v - wv).abs(),
                (ours_a - wa).abs(),
                (pin_a - wa).abs(),
                miss_c(ours_c),
                miss_c(pin_c),
            );
            let scale = wa.max(wv);
            assert!(
                (ours_v - wv).abs() <= acc * scale,
                "{name} {acc:e}: ours volume {ours_v}"
            );
            assert!(
                (ours_a - wa).abs() <= acc * wa,
                "{name} {acc:e}: ours area {ours_a}"
            );
            assert!(
                miss_c(ours_c) <= acc * 4.0,
                "{name} {acc:e}: ours centroid {ours_c:?}"
            );
            // The pin is held to the same bar; it is the comparison, not the reference.
            assert!(
                (pin_v - wv).abs() <= acc * scale,
                "{name} {acc:e}: pin volume {pin_v}"
            );
            assert!(
                (pin_a - wa).abs() <= acc * wa,
                "{name} {acc:e}: pin area {pin_a}"
            );
            assert!(
                miss_c(pin_c) <= acc * 4.0,
                "{name} {acc:e}: pin centroid {pin_c:?}"
            );
        }
    }
}
