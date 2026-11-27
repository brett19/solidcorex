// Written for solidcorex. Apache-2.0.
//! The contract (research 72 §7, CK1) on every body this workspace makes, and on bodies set
//! wrong in each way the contract names: sheets and partial shells, stacked and touching
//! solids, bodies and cavities set inside out, a contradiction smaller than the volume
//! estimate's error, and the pin's far thin shell with each face's loop reversed
//! (`the_far_thin_shells_loops_reversed_are_caught` of its #2972 tests).
//! CK2's pairing (research 72 §14.34) is checked against `scx_mass::outward` on the same
//! cases, passing and failing.
//!
//! Not ported, each by intent: the pin's `SolidTest` cases for a loose vertex tolerance
//! (`PointersInvalid`) and a negated plane (`Invalid`), which this crate's records cannot
//! express (a vertex has no tolerance to loosen; a face's sense is its sides, so a negated
//! plane is the inside-out case). The pin's thin shell at (10, 1e-6), `VolumeUndecided`
//! there, is proved here.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, Draft, FaceId, FaceSide, FinishCause, ShellId, StructureCause};
use scx_check::{Breach, ContractError, Proved, contract};
use scx_make::{Span, block, cone, cylinder, extrude, sphere, torus};
use scx_mass::{MassError, MassRefusal, RelAccuracy, Sign, outward, volume, volume_sign};
use scx_num::budget::{Budget, Stop};
use scx_num::error::ErrorKind;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_planar::{Region, Segment};
use scx_testkit::bodies::{Kind, build_resolution, generate, quarter_shell};
use scx_testkit::profile::{Seg, Verdict, fixtures};

fn mm(v: f64) -> Length {
    Length::from_mm(v)
}

fn check_with(b: &Brep, settings: &Settings, stop: &Stop) -> Result<Proved, ContractError> {
    let doc = Document::with_resolution(b.resolution());
    let mut scratch = Scratch::default();
    let cx = Cx::new(&doc, settings, &mut scratch, stop, &NoTrace);
    contract(b, &cx)
}

fn check(b: &Brep) -> Result<Proved, ContractError> {
    check_with(b, &Settings::new(), &Stop::new())
}

fn proved(b: &Brep, what: &str) {
    match check(b) {
        Ok(p) => assert_eq!(p.body().faces().len(), b.faces().len(), "{what}"),
        Err(e) => panic!("{what}: {e} ({e:?})"),
    }
}

fn broken(b: &Brep) -> Breach {
    match check(b) {
        Err(ContractError::Broken(breach)) => breach,
        other => panic!("not broken: {other:?}"),
    }
}

fn finish_in(d: Draft, doc: &Document) -> Brep {
    d.finish(doc).map_err(|u| u.cause).unwrap().0
}

fn finish(d: Draft) -> Brep {
    finish_in(d, &Document::default())
}

fn kind(k: Kind, seed: u64, size: usize) -> Brep {
    finish(generate(k, seed, size).unwrap().draft)
}

/// The draft with the given faces' sides swapped: the same geometry, set inside out.
fn swapped(mut d: Draft, faces: &[FaceId]) -> Draft {
    for &id in faces {
        d.face_mut(id).unwrap().sides.swap(0, 1);
    }
    d
}

fn all_faces(b: &Brep) -> Vec<FaceId> {
    b.faces().map(|f| f.id()).collect()
}

fn frames() -> [Frame; 3] {
    [
        Frame::new(
            Point3::ORIGIN,
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap(),
        Frame::new(
            Point3::new(1250.0, -2125.0, 600.0),
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(-3.0, 0.5, 1.0),
        )
        .unwrap(),
        Frame::new(
            Point3::new(1e5, -1e5, 1e5),
            Vec3::new(0.0, 1.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap(),
    ]
}

/// Every primitive `scx-make` builds is proved, at the origin, tilted and far.
#[test]
fn every_primitive_is_proved() {
    let doc = Document::default();
    for (i, f) in frames().iter().enumerate() {
        proved(
            &block(f, [mm(2.0), mm(3.0), mm(5.0)], &doc).unwrap().body,
            &format!("block {i}"),
        );
        proved(
            &cylinder(f, mm(2.0), mm(3.0), &doc).unwrap().body,
            &format!("cylinder {i}"),
        );
        proved(
            &cone(f, [mm(2.0), mm(1.0)], mm(3.0), &doc).unwrap().body,
            &format!("frustum {i}"),
        );
        proved(
            &cone(f, [mm(2.0), mm(0.0)], mm(3.0), &doc).unwrap().body,
            &format!("cone {i}"),
        );
        proved(
            &sphere(f, mm(2.0), &doc).unwrap().body,
            &format!("sphere {i}"),
        );
        proved(
            &torus(f, mm(3.0), mm(1.0), &doc).unwrap().body,
            &format!("torus {i}"),
        );
    }
}

fn segment(s: &Seg) -> Segment {
    match *s {
        Seg::Line { start, end } => Segment::line(Point2::from(start), Point2::from(end)),
        Seg::Arc {
            centre,
            start,
            end,
            ccw,
        } => {
            let sense = if ccw { Sense::Forward } else { Sense::Reversed };
            if start == end {
                let r = fmath::hypot(start[0] - centre[0], start[1] - centre[1]);
                Segment::circle(Point2::from(centre), r, sense)
            } else {
                Segment::arc(
                    Point2::from(centre),
                    Point2::from(start),
                    Point2::from(end),
                    sense,
                )
            }
        }
    }
}

/// Every profile fixture `extrude` accepts gives a proved prism, at each frame.
#[test]
fn every_extrusion_is_proved() {
    let mut n = 0;
    for fx in fixtures() {
        if !matches!(fx.verdict, Verdict::Accepted { .. }) {
            continue;
        }
        let doc = Document::with_resolution(Resolution::new(mm(fx.resolution)).unwrap());
        let Ok(region) = Region::new(
            fx.outer.iter().map(segment).collect(),
            fx.holes
                .iter()
                .map(|h| h.iter().map(segment).collect())
                .collect(),
            doc.resolution(),
        ) else {
            continue;
        };
        for (i, f) in frames().iter().enumerate() {
            if let Ok(m) = extrude(&region, f, Span::new(mm(0.0), mm(1.5)), &doc) {
                proved(&m.body, &format!("{} at frame {i}", fx.name));
                n += 1;
            }
        }
    }
    assert!(n > 30, "{n} extrusions");
}

/// Every generated solid kind is proved, over seeds and sizes.
#[test]
fn every_generated_solid_is_proved() {
    let solids = [
        Kind::Block,
        Kind::Prism,
        Kind::Cylinder,
        Kind::Cone,
        Kind::Sphere,
        Kind::Torus,
        Kind::Holed,
        Kind::Cavity,
    ];
    for k in solids {
        for seed in 0..4 {
            for size in [1, 3] {
                proved(
                    &kind(k, seed, size),
                    &format!("{k:?} seed {seed} size {size}"),
                );
            }
        }
    }
}

/// The empty body, sheets, a block with a face taken out (a partial shell), stacked and
/// touching solids are refused by name.
#[test]
fn what_is_not_one_closed_manifold_solid_is_refused() {
    assert_eq!(broken(&finish(Draft::new())), Breach::Empty);
    for k in [Kind::QuadSheet, Kind::TriSheet] {
        let b = kind(k, 0, 2);
        let first = b.faces().next().unwrap().id();
        assert_eq!(broken(&b), Breach::FaceNotBounding { face: first }, "{k:?}");
    }
    let block = kind(Kind::Block, 0, 1);
    let mut d = block.edit();
    let gone = block.faces().next().unwrap();
    let open = gone.uses().map(|u| u.edge().id()).min().unwrap();
    d.remove_face(gone.id()).unwrap();
    // Finish refuses the open block itself (its edges' radial order checks the sides);
    // were it to pass, the contract refuses the open edge.
    if let Ok((b, _)) = d.finish(&Document::default()) {
        assert_eq!(
            broken(&b),
            Breach::NonManifoldEdge {
                edge: open,
                fins: 1
            }
        );
    }
    for k in [Kind::Stack, Kind::Checker, Kind::Touching] {
        let breach = broken(&kind(k, 0, 2));
        assert!(
            matches!(
                breach,
                Breach::FaceNotBounding { .. } | Breach::NonManifoldEdge { .. }
            ),
            "{k:?}: {breach:?}"
        );
    }
}

/// The block and the cavity set inside out, and the cavity alone inverted (its faces facing
/// into the solid), are refused as not outward.
#[test]
fn inside_out_bodies_and_inverted_voids_are_refused() {
    for k in [
        Kind::Block,
        Kind::Cylinder,
        Kind::Sphere,
        Kind::Torus,
        Kind::Cavity,
    ] {
        let g = kind(k, 0, 1);
        let b = finish(swapped(g.edit(), &all_faces(&g)));
        assert!(matches!(broken(&b), Breach::NotOutward { .. }), "{k:?}");
    }
    let g = kind(Kind::Cavity, 0, 1);
    let void = g
        .regions()
        .find(|r| !r.is_solid() && !r.is_outside())
        .unwrap();
    let lining: Vec<FaceId> = void
        .shells()
        .flat_map(|s| s.faces().map(|(f, _)| f.id()).collect::<Vec<_>>())
        .collect();
    let b = finish(swapped(g.edit(), &lining));
    let shell = void.shells().next().unwrap().id();
    assert_eq!(broken(&b), Breach::NotOutward { shell });
}

/// A cavity labelled the outside (its shell moved to the outside region, its own region
/// removed): finish passes it, since a region's shells need not meet. The outside then
/// holds a positive shell, and the contract refuses it by that shell.
#[test]
fn a_cavity_labelled_the_outside_is_refused() {
    let g = kind(Kind::Cavity, 0, 1);
    let void = g
        .regions()
        .find(|r| !r.is_solid() && !r.is_outside())
        .unwrap();
    let shell = void.shells().next().unwrap().id();
    let mut d = g.edit();
    d.shell_mut(shell).unwrap().region = g.outside().id();
    d.remove_region(void.id()).unwrap();
    assert_eq!(broken(&finish(d)), Breach::NotOutward { shell });
}

/// The pin's `SolidTest::VolumeNegative` case, the solid and void labels swapped: here the
/// outside may not be solid, so finish refuses it before the contract is asked.
#[test]
fn a_solid_outside_is_refused_by_finish() {
    let g = kind(Kind::Block, 0, 1);
    let mut d = g.edit();
    for r in g.regions() {
        d.region_mut(r.id()).unwrap().solid = !r.is_solid();
    }
    let cause = d
        .finish(&Document::default())
        .map(|_| ())
        .unwrap_err()
        .cause;
    assert!(
        matches!(cause, FinishCause::Structure(StructureCause::Outside)),
        "{cause:?}"
    );
}

/// A contradiction smaller than the estimate's error: the pin's thinnest shell (10, 1e-6)
/// set inside out has a volume (-1.6e-5) below its error at a loose accuracy (8e-4 at
/// 1e-3), so the volume's guard, which reads the estimate, lets it through. The certified
/// sign refuses it.
#[test]
fn a_contradiction_smaller_than_the_error_is_refused() {
    let doc = Document::with_resolution(build_resolution());
    let g = finish_in(quarter_shell(10.0, 1e-6, 1.0).unwrap(), &doc);
    let b = finish_in(swapped(g.edit(), &all_faces(&g)), &doc);
    let e = volume(
        &b,
        RelAccuracy::new(1e-3).unwrap(),
        &mut Scratch::default(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap();
    assert!(e.error > e.value.abs(), "{e:?}");
    assert!(matches!(broken(&b), Breach::NotOutward { .. }));
}

/// The pin's thin quarter shells, (10, 1e-6) included (`VolumeUndecided` there), are
/// proved.
#[test]
fn thin_shells_are_proved() {
    let doc = Document::with_resolution(build_resolution());
    for (r, t) in [(1e4, 1e-3), (10.0, 1e-4), (10.0, 1e-6)] {
        proved(
            &finish_in(quarter_shell(r, t, 1.0).unwrap(), &doc),
            &format!("({r}, {t})"),
        );
    }
}

/// Reverses the face's first loop: the uses in the other order, each against its edge,
/// and the face's fins on those edges flipped to match.
fn reverse_first_loop(d: &mut Draft, face: FaceId) {
    let rec = d.face_mut(face).unwrap();
    let lp = &mut rec.loops[0];
    lp.uses.reverse();
    for u in &mut lp.uses {
        u.sense = u.sense.reversed();
    }
    let edges: Vec<_> = lp.uses.iter().map(|u| u.edge).collect();
    for e in edges {
        for fin in &mut d.adjacency_mut(e).unwrap().fins {
            if fin.face == face {
                fin.sense = fin.sense.reversed();
            }
        }
    }
}

/// The pin's `the_far_thin_shells_loops_reversed_are_caught`: on each face of the far
/// thin shell, the first loop reversed. Alone, finish refuses it (its fins' senses no
/// longer agree with the face's sides). With the face's sides swapped to match, finish
/// passes it and the contract refuses it: the reversed loop leaves the face the unbounded
/// rest of its surface. Never proved.
#[test]
fn the_far_thin_shells_loops_reversed_are_caught() {
    let doc = Document::with_resolution(build_resolution());
    let g = finish_in(quarter_shell(1e4, 1e-3, 1.0).unwrap(), &doc);
    assert_eq!(g.faces().len(), 6);
    for face in all_faces(&g) {
        let mut d = g.edit();
        reverse_first_loop(&mut d, face);
        match d.clone().finish(&doc) {
            Err(u) => assert!(
                matches!(
                    u.cause,
                    FinishCause::Structure(StructureCause::SidesDisagree { .. })
                ),
                "{face}: {:?}",
                u.cause
            ),
            Ok(_) => panic!("{face}: finish passed the reversed loop"),
        }
        let b = finish_in(swapped(d, &[face]), &doc);
        assert_eq!(broken(&b), Breach::FaceUnbounded { face });
    }
}

/// The far thin shell's faces stay on one side each: a face's own sides swapped (its loops
/// kept) leaves its shell not outward.
#[test]
fn one_face_set_inside_out_is_refused() {
    let doc = Document::with_resolution(build_resolution());
    let g = finish_in(quarter_shell(1e4, 1e-3, 1.0).unwrap(), &doc);
    for face in all_faces(&g) {
        match swapped(g.edit(), &[face]).finish(&doc) {
            Err(_) => {}
            Ok((b, _)) => assert!(check(&b).is_err(), "{face}"),
        }
    }
}

/// A zero budget ends the certified sign as exhausted, a set stop flag as cancelled, both
/// typed by kind; the structure tests come first and charge nothing.
#[test]
fn the_budget_and_the_stop_flag_end_it() {
    let b = kind(Kind::Sphere, 0, 1);
    match check_with(&b, &Settings::new().with_budget(0), &Stop::new()) {
        Err(e @ ContractError::Mass(MassError::Exhausted { .. })) => {
            assert_eq!(e.kind(), ErrorKind::Exhausted);
        }
        other => panic!("{other:?}"),
    }
    let stop = Stop::new();
    stop.set();
    match check_with(&b, &Settings::new(), &stop) {
        Err(e @ ContractError::Mass(MassError::Cancelled)) => {
            assert_eq!(e.kind(), ErrorKind::Cancelled);
        }
        other => panic!("{other:?}"),
    }
    let sheet = kind(Kind::QuadSheet, 0, 1);
    let got = check_with(&sheet, &Settings::new().with_budget(0), &stop);
    assert!(matches!(got, Err(ContractError::Broken(_))), "{got:?}");
    assert_eq!(got.unwrap_err().kind(), ErrorKind::Refused);
}

/// Bodies that pass the structure tests, both proved and set wrong in each outward way:
/// generated solids and primitives, set inside out, a cavity's lining inverted, a cavity
/// labelled the outside, the thin shells and the thinnest set inside out.
fn outward_cases() -> Vec<(String, Brep)> {
    let mut out = Vec::new();
    let doc = Document::default();
    for k in [
        Kind::Block,
        Kind::Cylinder,
        Kind::Cone,
        Kind::Sphere,
        Kind::Torus,
        Kind::Holed,
        Kind::Cavity,
    ] {
        for (seed, size) in [(0, 1), (1, 3)] {
            let g = kind(k, seed, size);
            let inv = finish(swapped(g.edit(), &all_faces(&g)));
            out.push((format!("{k:?} {seed} {size}"), g));
            out.push((format!("{k:?} {seed} {size} inside out"), inv));
        }
    }
    let f = &frames()[1];
    out.push((
        "torus primitive".into(),
        torus(f, mm(3.0), mm(1.0), &doc).unwrap().body,
    ));
    out.push((
        "sphere primitive".into(),
        sphere(f, mm(2.0), &doc).unwrap().body,
    ));
    let g = kind(Kind::Cavity, 0, 3);
    for void in g.regions().filter(|r| !r.is_solid() && !r.is_outside()) {
        let lining: Vec<FaceId> = void
            .shells()
            .flat_map(|s| s.faces().map(|(f, _)| f.id()).collect::<Vec<_>>())
            .collect();
        out.push((
            format!("cavity {} lining inverted", void.id()),
            finish(swapped(g.edit(), &lining)),
        ));
        let shell = void.shells().next().unwrap().id();
        let mut d = g.edit();
        d.shell_mut(shell).unwrap().region = g.outside().id();
        d.remove_region(void.id()).unwrap();
        out.push((format!("cavity {} labelled outside", void.id()), finish(d)));
    }
    let doc = Document::with_resolution(build_resolution());
    for (r, t) in [(1e4, 1e-3), (10.0, 1e-6)] {
        let g = finish_in(quarter_shell(r, t, 1.0).unwrap(), &doc);
        let inv = finish_in(swapped(g.edit(), &all_faces(&g)), &doc);
        out.push((format!("thin ({r}, {t})"), g));
        out.push((format!("thin ({r}, {t}) inside out"), inv));
    }
    out
}

/// CK2: the contract's outward test, which certifies one shell of each mirror pair, gives
/// `scx_mass::outward`'s answer on every case: proved where it passes, and the same shell
/// named where it refuses. At least a third of the cases are refused.
#[test]
fn outward_agrees_with_scx_mass_on_every_case() {
    let cases = outward_cases();
    let mut refused = 0;
    for (what, b) in &cases {
        let want = outward(b, &mut Budget::unlimited(), &Stop::new());
        match (check(b), want) {
            (Ok(_), Ok(())) => {}
            (
                Err(ContractError::Broken(Breach::NotOutward { shell })),
                Err(MassError::Refused(MassRefusal::NotOutward { shell: s })),
            ) => {
                assert_eq!(shell, s, "{what}");
                refused += 1;
            }
            (got, want) => panic!("{what}: {got:?} against {want:?}"),
        }
    }
    assert!(3 * refused >= cases.len(), "{refused} of {}", cases.len());
}

/// The shell's faces with their weights: +1 for a back side, -1 for a front side, both
/// sides cancelling.
fn weights(b: &Brep, shell: ShellId) -> Vec<(FaceId, i32)> {
    let mut w = std::collections::BTreeMap::new();
    for (f, side) in b.shell(shell).unwrap().faces() {
        *w.entry(f.id()).or_insert(0) += if side == FaceSide::Back { 1 } else { -1 };
    }
    w.into_iter().filter(|&(_, x)| x != 0).collect()
}

/// CK2's premise, from the certified sign itself: two shells whose weighted faces are
/// negatives of each other have opposite signs, each certified on its own. Every case's
/// shells pair this way, so each pair is certified once.
#[test]
fn mirror_shells_have_opposite_signs() {
    let mut pairs = 0;
    for (what, b) in outward_cases() {
        let shells: Vec<ShellId> = b.shells().map(|s| s.id()).collect();
        for &a in &shells {
            let wa = weights(&b, a);
            let neg: Vec<(FaceId, i32)> = wa.iter().map(|&(f, x)| (f, -x)).collect();
            let mirrors: Vec<ShellId> = shells
                .iter()
                .copied()
                .filter(|&m| m != a && weights(&b, m) == neg)
                .collect();
            assert_eq!(mirrors.len(), 1, "{what}: shell {a}");
            let m = mirrors[0];
            if m < a {
                continue;
            }
            let sa = volume_sign(&b, a, &mut Budget::unlimited(), &Stop::new()).unwrap();
            let sm = volume_sign(&b, m, &mut Budget::unlimited(), &Stop::new()).unwrap();
            assert_ne!(sa, Sign::Zero, "{what}: shell {a}");
            let flipped = if sa == Sign::Positive {
                Sign::Negative
            } else {
                Sign::Positive
            };
            assert_eq!(sm, flipped, "{what}: shells {a}, {m}");
            pairs += 1;
        }
    }
    assert!(pairs > 30, "{pairs} pairs");
}

/// CK2's saving, in budget units: the contract proves the torus on half the work
/// `scx_mass::outward` spends, which is exhausted on that budget.
#[test]
fn the_contract_spends_half_the_work() {
    let b = torus(&frames()[0], mm(3.0), mm(1.0), &Document::default())
        .unwrap()
        .body;
    let mut full = Budget::unlimited();
    outward(&b, &mut full, &Stop::new()).unwrap();
    let half = full.used().div_ceil(2);
    let got = check_with(&b, &Settings::new().with_budget(half), &Stop::new());
    assert!(got.is_ok(), "{got:?}");
    let mut budget = Budget::new(half);
    let old = outward(&b, &mut budget, &Stop::new());
    assert!(matches!(old, Err(MassError::Exhausted { .. })), "{old:?}");
}

/// The proved body is the body: the same tables, shared.
#[test]
fn proved_holds_the_body() {
    let b = kind(Kind::Torus, 1, 2);
    let p = check(&b).unwrap();
    assert_eq!(p.body().faces().len(), b.faces().len());
    assert_eq!(p.as_ref().edges().len(), b.edges().len());
    let back = p.into_body();
    assert_eq!(back.bounding_box(), b.bounding_box());
    assert!(b.faces().zip(back.faces()).all(|(x, y)| x.id() == y.id()));
}
