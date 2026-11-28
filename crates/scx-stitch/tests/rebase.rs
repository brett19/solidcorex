// Written for solidcorex. Apache-2.0.
//! `rebase` (74 §4.3 step 8, ST6): topology and ids kept, accuracies past the new
//! resolution warned and recorded, vertices within the new coincidence refused, and a
//! round trip from coarse to fine leaving the geometry as it was.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use scx_brep::op::{Cx, NoTrace, Scratch, Settings, WarningKind};
use scx_brep::{Brep, Document, Entity};
use scx_num::budget::Stop;
use scx_num::cert::{Basis, Recorded};
use scx_num::units::{Accuracy, Length, Resolution};
use scx_stitch::{Rebased, StitchError, StitchRefusal, rebase};
use scx_testkit::bodies::{Kind, generate};

fn res(mm: f64) -> Resolution {
    Resolution::new(Length::from_mm(mm)).unwrap()
}

fn block() -> Brep {
    generate(Kind::Block, 0, 1)
        .unwrap()
        .draft
        .finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap()
        .0
}

fn run(b: &Brep, to: f64) -> Result<Rebased, StitchError> {
    let doc = Document::default();
    let mut scratch = Scratch::new();
    let settings = Settings::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    rebase(b, res(to), &mut cx)
}

fn points(b: &Brep) -> Vec<[f64; 3]> {
    b.vertices()
        .map(|v| [v.point().x, v.point().y, v.point().z])
        .collect()
}

fn count(r: &Rebased, kind: fn(&Entity) -> bool) -> usize {
    r.done
        .report
        .warnings
        .iter()
        .filter(|w| w.kind == WarningKind::AccuracyPastResolution && kind(&w.entity))
        .count()
}

#[test]
fn coarser_keeps_topology_ids_and_says_nothing() {
    let b = block();
    let r = run(&b, 1.0e-3).unwrap();
    let out = &r.done.body;
    assert_eq!(out.resolution(), res(1.0e-3));
    assert_eq!(points(out), points(&b));
    assert_eq!(out.faces().count(), b.faces().count());
    assert_eq!(out.edges().count(), b.edges().count());
    assert!(r.done.report.warnings.is_empty());
    assert!(r.report.past.is_empty() && r.report.recorded.is_empty());
    assert!(r.done.changes.is_empty());
}

#[test]
fn same_resolution_is_the_body_again() {
    let b = block();
    let r = run(&b, Document::default().resolution().zone().mm()).unwrap();
    assert!(r.done.report.is_clean());
    assert_eq!(points(&r.done.body), points(&b));
}

#[test]
fn vertices_within_the_new_coincidence_are_refused_at_the_boundary() {
    let b = block();
    // The unit block's nearest vertices are 1 apart: within at 1.0, not at 0.999.
    assert!(run(&b, 0.999).is_ok());
    match run(&b, 1.0) {
        Err(StitchError::Refused(StitchRefusal::VerticesWithinCoincidence {
            vertices: [a, c],
            ..
        })) => assert!(a < c),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        run(&b, 5.0),
        Err(StitchError::Refused(
            StitchRefusal::VerticesWithinCoincidence { .. }
        ))
    ));
}

#[test]
fn finer_warns_every_entity_that_was_only_within_the_old_resolution() {
    let b = block();
    let r = run(&b, 1.0e-6).unwrap();
    let v = count(&r, |e| matches!(e, Entity::Vertex(_)));
    let e = count(&r, |e| matches!(e, Entity::Edge(_)));
    let f = count(&r, |e| matches!(e, Entity::Face(_)));
    assert_eq!((v, e, f), (8, 12, 6));
    // The old resolution is recorded as the accuracy, not claimed as "within".
    for vx in r.done.body.vertices() {
        let a = vx.accuracy().expect("recorded");
        assert_eq!(a.basis, Basis::Stated);
        assert_eq!(a.acc.length().mm(), 1.0e-5);
    }
    assert_eq!(r.report.past.len(), 26);
    assert_eq!(r.report.recorded.len(), 26);
    assert_eq!(points(&r.done.body), points(&b));
}

#[test]
fn a_recorded_accuracy_past_the_new_resolution_is_kept_and_warned() {
    let mut d = block().edit();
    let v = d.vertices().next().unwrap().id();
    d.vertex_mut(v).unwrap().accuracy = Some(Arc::new(Recorded {
        acc: Accuracy::new(Length::from_mm(1.0e-4)).unwrap(),
        basis: Basis::Measured,
    }));
    let b = d
        .finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap()
        .0;
    // Coarser than the accuracy: within the resolution, kept as recorded, no warning.
    let r = run(&b, 1.0e-3).unwrap();
    assert!(r.done.report.warnings.is_empty());
    assert_eq!(
        r.done
            .body
            .vertices()
            .next()
            .unwrap()
            .accuracy()
            .unwrap()
            .basis,
        Basis::Measured
    );
    // Finer than the accuracy: kept, warned, basis untouched.
    let r = run(&b, 1.0e-5).unwrap();
    assert_eq!(r.report.past.len(), 1);
    assert_eq!(r.report.past[0].entity, Entity::Vertex(v));
    assert_eq!(r.report.past[0].accuracy.length().mm(), 1.0e-4);
    assert!(r.report.recorded.is_empty());
    let a = r.done.body.vertices().next().unwrap().accuracy().unwrap();
    assert_eq!((a.basis, a.acc.length().mm()), (Basis::Measured, 1.0e-4));
}

#[test]
fn a_round_trip_coarse_to_fine_leaves_the_geometry() {
    let b = block();
    let coarse = run(&b, 1.0e-3).unwrap();
    let fine = run(&coarse.done.body, 1.0e-5).unwrap();
    let out = &fine.done.body;
    assert_eq!(out.resolution(), b.resolution());
    assert_eq!(points(out), points(&b));
    for (x, y) in out.faces().zip(b.faces()) {
        assert_eq!(x.record().surface, y.record().surface);
    }
    for (x, y) in out.edges().zip(b.edges()) {
        assert_eq!(x.shape(), y.shape());
        assert_eq!(x.ends(), y.ends());
    }
    // Each entity now says honestly that it is within 1e-3, past 1e-5.
    assert_eq!(fine.report.past.len(), 26);
    let a = out.vertices().next().unwrap().accuracy().unwrap();
    assert_eq!(a.acc.length().mm(), 1.0e-3);
}

#[test]
fn rebase_is_deterministic() {
    let b = block();
    let (x, y) = (run(&b, 1.0e-7).unwrap(), run(&b, 1.0e-7).unwrap());
    assert_eq!(x.report, y.report);
    assert_eq!(x.done.report.warnings, y.done.report.warnings);
    assert_eq!(x.done.report.work, y.done.report.work);
}
