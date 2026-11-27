// Written for solidcorex. Apache-2.0.
//! The face memo (research 72 MA5) on this crate's bodies: a warm `Scratch` gives the bits a
//! cold one gives, values and stated errors, and charges the same work; another accuracy,
//! or another recorded bound on a trim, is another entry. The memo across a Boolean that
//! keeps faces is tested in `scx-boolean`'s `mass_memo`.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use scx_brep::op::Scratch;
use scx_brep::{Brep, Document};
use scx_mass::{Mass, MassError, MassSpec, RelAccuracy, area, mass, volume};
use scx_num::budget::{Budget, Stop};
use scx_testkit::bodies::{Kind, generate, loose};

fn acc(a: f64) -> RelAccuracy {
    RelAccuracy::new(a).unwrap()
}

fn body(kind: Kind, seed: u64, size: usize) -> Brep {
    let d = generate(kind, seed, size).unwrap().draft;
    d.finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap()
        .0
}

fn loose_body(kind: Kind, offset: f64) -> Brep {
    let d = loose(kind, 0, 1, offset).unwrap();
    d.finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap()
        .0
}

/// Every number of a [`Mass`], as bits.
fn bits(m: &Mass) -> Vec<u64> {
    let v = |p: [f64; 3]| p.map(f64::to_bits);
    let s = |s: &scx_mass::Sym3| [s.xx, s.yy, s.zz, s.yz, s.zx, s.xy].map(f64::to_bits);
    let e = &m.error;
    let mut out = vec![m.density.to_bits(), m.volume.to_bits(), m.area.to_bits()];
    out.extend(v([m.about.x, m.about.y, m.about.z]));
    out.extend(v([m.first.x, m.first.y, m.first.z]));
    out.extend(s(&m.second));
    out.extend(v([m.area_first.x, m.area_first.y, m.area_first.z]));
    out.extend([e.volume.to_bits(), e.area.to_bits()]);
    out.extend(v([e.first.x, e.first.y, e.first.z]));
    out.extend(s(&e.second));
    out.extend(v([e.area_first.x, e.area_first.y, e.area_first.z]));
    out
}

/// `mass` on `scratch`: its result's bits and the work it charged.
fn measure(b: &Brep, a: f64, scratch: &mut Scratch) -> (Vec<u64>, u64) {
    let mut budget = Budget::unlimited();
    let m = mass(
        b,
        &MassSpec::new(acc(a)),
        scratch,
        &mut budget,
        &Stop::new(),
    )
    .unwrap();
    (bits(&m), budget.used())
}

#[test]
fn a_warm_memo_gives_the_cold_bits_and_work() {
    let bodies = [
        body(Kind::Block, 0, 2),
        body(Kind::Holed, 1, 2),
        body(Kind::Cylinder, 0, 2),
        body(Kind::Cone, 0, 1),
        body(Kind::Sphere, 0, 1),
        body(Kind::Torus, 0, 1),
        loose_body(Kind::Cylinder, 1e-4),
    ];
    for (i, b) in bodies.iter().enumerate() {
        let cold = measure(b, 1e-9, &mut Scratch::new());
        let mut warm = Scratch::new();
        assert_eq!(
            measure(b, 1e-9, &mut warm),
            cold,
            "body {i}: the filling call"
        );
        let filled = warm.len();
        assert_eq!(filled, b.faces().count(), "body {i}: one entry a face");
        assert_eq!(measure(b, 1e-9, &mut warm), cold, "body {i}: all hits");
        assert_eq!(warm.len(), filled, "body {i}: a hit adds nothing");

        // `volume` and `area` are their own sets: their entries are new, their bits the cold.
        let one = |f: fn(
            &Brep,
            RelAccuracy,
            &mut Scratch,
            &mut Budget,
            &Stop,
        ) -> Result<scx_mass::Estimate, MassError>,
                   s: &mut Scratch| {
            let mut budget = Budget::unlimited();
            let e = f(b, acc(1e-9), s, &mut budget, &Stop::new()).unwrap();
            (e.value.to_bits(), e.error.to_bits(), budget.used())
        };
        for f in [volume, area] {
            let cold = one(f, &mut Scratch::new());
            assert_eq!(one(f, &mut warm), cold, "body {i}: a fill");
            assert_eq!(one(f, &mut warm), cold, "body {i}: a hit");
        }
    }
}

#[test]
fn another_accuracy_or_bound_misses() {
    let b = body(Kind::Cylinder, 0, 1);
    let faces = b.faces().count();
    let mut s = Scratch::new();
    measure(&b, 1e-9, &mut s);
    measure(&b, 1e-10, &mut s);
    assert_eq!(
        s.len(),
        2 * faces,
        "another accuracy is another entry for every face"
    );
    let next = f64::from_bits(1e-9f64.to_bits() + 1);
    let cold = measure(&b, next, &mut Scratch::new());
    assert_eq!(measure(&b, next, &mut s), cold);
    assert_eq!(s.len(), 3 * faces, "one bit of accuracy is another entry");

    // A loose body's moved trims carry their bound: another offset is another face.
    let (a, c) = (loose_body(Kind::Block, 1e-4), loose_body(Kind::Block, 2e-4));
    let mut s = Scratch::new();
    measure(&a, 1e-9, &mut s);
    let n = s.len();
    let cold = measure(&c, 1e-9, &mut Scratch::new());
    assert_eq!(measure(&c, 1e-9, &mut s), cold);
    assert_eq!(s.len(), 2 * n, "every face of the block has moved trims");
}

#[test]
fn a_hit_runs_out_of_budget_where_a_recompute_does() {
    let b = body(Kind::Sphere, 0, 1);
    let spec = MassSpec::new(acc(1e-9));
    let (_, total) = measure(&b, 1e-9, &mut Scratch::new());
    let mut warm = Scratch::new();
    measure(&b, 1e-9, &mut warm);
    for limit in [0, 1, total / 3, total / 2, total - 1, total] {
        let run = |s: &mut Scratch| {
            mass(&b, &spec, s, &mut Budget::new(limit), &Stop::new()).map(|m| bits(&m))
        };
        let cold = run(&mut Scratch::new());
        assert_eq!(run(&mut warm), cold, "limit {limit}");
        assert_eq!(cold.is_ok(), limit >= total, "limit {limit} of {total}");
    }
    // A call cut by the budget keeps only the faces it finished.
    let mut cut = Scratch::new();
    let _ = mass(
        &b,
        &spec,
        &mut cut,
        &mut Budget::new(total / 2),
        &Stop::new(),
    );
    assert!(cut.len() < b.faces().count(), "{} entries", cut.len());
    assert_eq!(
        measure(&b, 1e-9, &mut cut).0,
        measure(&b, 1e-9, &mut Scratch::new()).0
    );
}
