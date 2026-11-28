// Written for solidcorex. Apache-2.0.
//! Vertex clearance in `check` (research 74 §3.3, RP1): two vertices of one body within the
//! larger of their reaches are found. The recipe gives one block vertex the accuracy
//! `acc`, set against the closest other vertex's distance `d`: at `10 d` every vertex
//! within `10 d` of it is found, at `d` the closest pair is the allowance (found or
//! undecided, never anything else), at `d / 10` the block is clean. The truths are the
//! block's own distances.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, VertexId};
use scx_check::{CheckError, Finding, Findings, Scope, Undecided, Why};
use scx_num::budget::Stop;
use scx_num::cert::{Basis, Recorded};
use scx_num::units::{Accuracy, Length};
use scx_testkit::bodies::{Kind, generate};

fn run(b: &Brep, settings: &Settings, stop: &Stop) -> Result<Findings, CheckError> {
    let doc = Document::with_resolution(b.resolution());
    let mut scratch = Scratch::default();
    let mut cx = Cx::new(&doc, settings, &mut scratch, stop, &NoTrace);
    scx_check::check(b, &mut cx, Scope::Import)
}

/// A block (seed `seed`) with its first vertex given the accuracy `acc`, and that vertex.
fn widened(seed: u64, acc: f64) -> (Brep, VertexId) {
    let mut d = generate(Kind::Block, seed, 1).unwrap().draft;
    let v = d.vertices().next().unwrap().id();
    d.vertex_mut(v).unwrap().accuracy = Some(Arc::new(Recorded {
        acc: Accuracy::new(Length::from_mm(acc)).unwrap(),
        basis: Basis::Measured,
    }));
    let b = d
        .finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap()
        .0;
    (b, v)
}

/// Every other vertex of `b` with its distance from `v`, closest first.
fn others(b: &Brep, v: VertexId) -> Vec<(VertexId, f64)> {
    let p = b.vertex(v).unwrap().point();
    let mut out: Vec<(VertexId, f64)> = b
        .vertices()
        .filter(|w| w.id() != v)
        .map(|w| (w.id(), (w.point() - p).length()))
        .collect();
    out.sort_by(|a, b| a.1.total_cmp(&b.1));
    out
}

fn pairs(f: &Findings) -> Vec<(VertexId, VertexId)> {
    f.list
        .iter()
        .filter_map(|x| match x {
            Finding::VerticesWithinCoincidence { a, b, .. } => Some((*a, *b)),
            _ => None,
        })
        .collect()
}

fn ordered(a: VertexId, b: VertexId) -> (VertexId, VertexId) {
    if a < b { (a, b) } else { (b, a) }
}

#[test]
fn a_block_clean_before_widening() {
    for seed in 0..3 {
        let b = generate(Kind::Block, seed, 1).unwrap().draft;
        let b = b
            .finish(&Document::default())
            .map_err(|u| u.cause)
            .unwrap()
            .0;
        assert!(run(&b, &Settings::new(), &Stop::new()).unwrap().is_clean());
    }
}

#[test]
fn vertices_within_a_widened_reach_are_found_and_only_those() {
    for seed in 0..3 {
        let (plain, v) = widened(seed, 1e-9);
        let near = others(&plain, v);
        let d = near[0].1;

        // 10 d: every vertex well within 10 d is found, with its bracket holding the
        // distance; none past it.
        let (b, _) = widened(seed, 10.0 * d);
        let f = run(&b, &Settings::new(), &Stop::new()).unwrap();
        let mut want: Vec<_> = near
            .iter()
            .filter(|(_, x)| *x <= 10.0 * d * (1.0 - 1e-9))
            .map(|(w, _)| ordered(v, *w))
            .collect();
        want.sort();
        assert!(!want.is_empty());
        assert_eq!(pairs(&f), want, "seed {seed}");
        for x in &f.list {
            if let Finding::VerticesWithinCoincidence {
                a,
                b: w,
                distance,
                allowed,
            } = x
            {
                let other = if *a == v { *w } else { *a };
                let truth = near.iter().find(|(id, _)| *id == other).unwrap().1;
                assert!(distance.lower() <= truth && truth <= distance.upper());
                assert_eq!(allowed.mm(), 10.0 * d);
            }
        }
        assert!(
            f.list
                .iter()
                .all(|x| matches!(x, Finding::VerticesWithinCoincidence { .. }))
        );

        // d: the closest pair at its allowance, found or undecided; nothing else.
        let (b, _) = widened(seed, d);
        let f = run(&b, &Settings::new(), &Stop::new()).unwrap();
        let undecided = f
            .undecided
            .iter()
            .filter(|u| matches!(u, Undecided::VerticesWithinCoincidence { .. }))
            .count();
        assert!(pairs(&f).len() + undecided >= 1, "seed {seed}");
        assert!(pairs(&f).len() + undecided <= near.iter().filter(|x| x.1 <= d).count());

        // d / 10: clean.
        let (b, _) = widened(seed, d / 10.0);
        assert!(
            run(&b, &Settings::new(), &Stop::new()).unwrap().is_clean(),
            "seed {seed}"
        );
    }
}

#[test]
fn a_budget_too_small_leaves_the_pairs_undecided() {
    let (plain, v) = widened(0, 1e-9);
    let d = others(&plain, v)[0].1;
    let (b, _) = widened(0, 10.0 * d);
    let whole = run(&b, &Settings::new(), &Stop::new()).unwrap();
    let found = pairs(&whole);
    assert!(!found.is_empty());
    // The exact work answers as the unlimited budget does.
    let exact = Settings::new().with_budget(whole.spent.work);
    assert_eq!(run(&b, &exact, &Stop::new()).unwrap(), whole);
    // Past the structure part only: the gaps and the pairs are left undecided, never found
    // and never an error.
    let uses: u64 = b
        .faces()
        .map(|f| f.loops().map(|l| l.uses().len() as u64).sum::<u64>())
        .sum();
    let tight = run(&b, &Settings::new().with_budget(uses + 1), &Stop::new()).unwrap();
    assert!(pairs(&tight).is_empty());
    let starved: Vec<(VertexId, VertexId)> = tight
        .undecided
        .iter()
        .filter_map(|u| match u {
            Undecided::VerticesWithinCoincidence {
                a,
                b,
                why: Why::Exhausted,
                ..
            } => Some((*a, *b)),
            _ => None,
        })
        .collect();
    for p in &found {
        assert!(starved.contains(p), "{p:?}");
    }
    let stop = Stop::new();
    stop.set();
    assert!(matches!(
        run(&b, &Settings::new(), &stop),
        Err(CheckError::Cancelled)
    ));
}
