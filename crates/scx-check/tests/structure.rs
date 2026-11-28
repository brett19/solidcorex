// Written for solidcorex. Apache-2.0.
//! The structure pass of `check` and `contract` (research 74 §18, CF1): vertex
//! manifoldness and the solid-or-sheet class, on testkit bodies only. Each defect is the
//! only one its body has, and the truths are constructions: the two blocks of
//! `corner_touch` share one lattice vertex and nothing else, a valid generator has no
//! pinched vertex, a sheet is a sheet.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, Draft};
use scx_check::{
    Breach, CheckError, Class, Contacts, ContractError, Finding, Findings, Scope, contract,
};
use scx_num::budget::Stop;
use scx_num::error::ErrorKind;
use scx_testkit::bodies::{Kind, corner_touch, generate};

fn finish(d: Draft) -> Brep {
    d.finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap()
        .0
}

fn kind(k: Kind, seed: u64, size: usize) -> Brep {
    finish(generate(k, seed, size).unwrap().draft)
}

fn run(b: &Brep, settings: &Settings, stop: &Stop, scope: Scope) -> Result<Findings, CheckError> {
    let doc = Document::with_resolution(b.resolution());
    let mut scratch = Scratch::default();
    let mut cx = Cx::new(&doc, settings, &mut scratch, stop, &NoTrace);
    scx_check::check(b, &mut cx, scope)
}

fn found(b: &Brep) -> Findings {
    run(b, &Settings::new(), &Stop::new(), Scope::Import).unwrap()
}

fn contract_with(b: &Brep, contacts: Contacts) -> Result<(), ContractError> {
    let doc = Document::with_resolution(b.resolution());
    let (settings, stop) = (Settings::new(), Stop::new());
    let mut scratch = Scratch::default();
    let cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    contract(b, &cx, contacts).map(|_| ())
}

/// Two blocks meeting at one corner: that vertex is the one pinch, in two pieces; no other
/// finding; each is a solid. The contract refuses it only under `Contacts::Manifold`: the
/// Boolean builds such a body on purpose (BO7), so `Refused` and `Radial` admit it.
#[test]
fn blocks_meeting_at_a_vertex_pinch_it() {
    for one in [false, true] {
        for seed in 0..4 {
            let b = finish(corner_touch(seed, one).unwrap());
            // Two blocks: 12 vertices, one of them shared (a lattice point of the cells).
            assert_eq!(b.vertices().len(), 15, "one {one} seed {seed}");
            let f = found(&b);
            let [Finding::VertexNotManifold { vertex, pieces }] = f.list[..] else {
                panic!("one {one} seed {seed}: {:?}", f.list);
            };
            assert_eq!(pieces, 2);
            // The pinch is the vertex four of the faces of each block share: 6 edges end
            // at it (3 of each block).
            assert_eq!(b.vertex(vertex).unwrap().edges().count(), 6);
            assert_eq!(f.class, Class::Solid);
            assert!(!f.is_clean());
            for contacts in [Contacts::Refused, Contacts::Radial] {
                assert_eq!(contract_with(&b, contacts), Ok(()), "one {one} seed {seed}");
            }
            assert_eq!(
                contract_with(&b, Contacts::Manifold),
                Err(ContractError::Broken(Breach::VertexNotManifold {
                    vertex,
                    pieces: 2
                })),
                "one {one} seed {seed}"
            );
        }
    }
}

/// Without the pinch the same blocks are clean: one block alone, and the checkerboard whose
/// blocks share edges (a pinched edge, not a pinched vertex: its link is connected through
/// the edge's end).
#[test]
fn bodies_that_are_manifold_at_every_vertex_have_no_finding() {
    for k in Kind::ALL {
        for seed in 0..3 {
            for size in [1, 2, 3] {
                let f = found(&kind(k, seed, size));
                let pinched = f
                    .list
                    .iter()
                    .any(|x| matches!(x, Finding::VertexNotManifold { .. }));
                assert!(!pinched, "{k:?} seed {seed} size {size}: {:?}", f.list);
            }
        }
    }
}

/// The class by construction: solids, sheets, and the stack (a face between two solids).
#[test]
fn the_class_is_what_the_regions_say() {
    let solid = [
        Kind::Block,
        Kind::Prism,
        Kind::Cylinder,
        Kind::Cone,
        Kind::Sphere,
        Kind::Torus,
        Kind::Holed,
        Kind::Cavity,
        Kind::Checker,
        Kind::Touching,
    ];
    for k in solid {
        let f = found(&kind(k, 1, 2));
        assert_eq!((f.class, f.list.is_empty()), (Class::Solid, true), "{k:?}");
        assert!(f.is_clean());
    }
    for k in [Kind::QuadSheet, Kind::TriSheet] {
        let f = found(&kind(k, 1, 2));
        assert_eq!((f.class, f.list.is_empty()), (Class::Sheet, true), "{k:?}");
    }
    // `n + 1` stacked blocks share `n` faces, each with material on both sides: the shells
    // of the solid regions that hold them are not solid.
    for n in 1..4 {
        let b = kind(Kind::Stack, 0, n);
        let f = found(&b);
        assert_eq!(f.class, Class::Mixed);
        let shells: Vec<_> = f
            .list
            .iter()
            .map(|x| match x {
                Finding::NotSolid { shell } => *shell,
                other => panic!("{other:?}"),
            })
            .collect();
        // Every block but the outermost two has a shared face above and below; each block
        // holds one shell, and all of them touch a shared face when there are two or more.
        assert_eq!(shells.len(), n + 1, "stack of {}", n + 1);
        assert!(shells.windows(2).all(|w| w[0] < w[1]));
    }
}

/// A check reads only, and answers the same each time: the body is as it was (its debug
/// form, which lists every record), and two runs give equal findings.
#[test]
fn a_repeated_check_leaves_the_body_as_it_was() {
    for b in [
        finish(corner_touch(2, false).unwrap()),
        kind(Kind::Stack, 1, 2),
        kind(Kind::Cone, 0, 3),
    ] {
        let before = format!("{b:?}");
        let first = found(&b);
        let second = found(&b);
        assert_eq!(first, second);
        assert_eq!(format!("{b:?}"), before);
    }
}

/// Every scope runs the structure part, and on a clean body every scope gives the same answer.
#[test]
fn every_scope_runs_the_structure_part() {
    let b = finish(corner_touch(0, false).unwrap());
    let all = [Scope::Import, Scope::Stitched, Scope::Full]
        .map(|s| run(&b, &Settings::new(), &Stop::new(), s).unwrap());
    assert!(all.windows(2).all(|w| w[0] == w[1]));
    assert_eq!(Scope::default(), Scope::Import);
}

/// Work is charged by use. The structure part takes 4 units a use (24 uses here), so a budget
/// under that is `Exhausted`, with the limit; a budget that covers the structure but not the
/// gaps leaves gaps undecided, not an error (research 74 §28); a set stop is `Cancelled`;
/// neither is a finding.
#[test]
fn a_budget_or_a_stop_ends_it_typed() {
    let b = kind(Kind::Block, 0, 2);
    let structure = 24 * 4;
    let work = found(&b).spent.work;
    assert!(work > structure);
    let full = |budget| {
        run(
            &b,
            &Settings::new().with_budget(budget),
            &Stop::new(),
            Scope::Full,
        )
    };
    assert!(full(work).unwrap().is_clean());
    let short = full(structure + 1).unwrap();
    assert!(short.list.is_empty() && !short.undecided.is_empty());
    let e = full(structure - 1).unwrap_err();
    assert_eq!(
        e,
        CheckError::Exhausted {
            limit: structure - 1
        }
    );
    assert_eq!(e.kind(), ErrorKind::Exhausted);
    let stop = Stop::new();
    stop.set();
    let e = run(&b, &Settings::new(), &stop, Scope::Full).unwrap_err();
    assert_eq!((e, e.kind()), (CheckError::Cancelled, ErrorKind::Cancelled));
}
