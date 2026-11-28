// Written for solidcorex. Apache-2.0.
//! Sewing within a gap (research 75 §8.2, RP5), on the app's fixture: the six faces of a
//! 10 x 20 x 30 box, the five that are not the top stitched into an open box, and the lid
//! lifted by `delta` above the walls, gathered into one body of two pieces whose eight free
//! edges pair up `delta` apart (the app's `A/repair.rs` test
//! `a_gap_between_faces_is_sewn_each_sewn_edge_carrying_its_gap`).
//!
//! The truth of §8.2: within the gap the box sews to a closed solid of 6 faces, 12 edges and
//! 8 vertices whose volume is within area x `delta` of 6000; each of the four sewn edges'
//! recorded accuracy is at least the closed-form gap `delta` (and no wider than the search
//! width past it), in the report's raises, the bound its use on the dropped face carries
//! certified; the check passes; the resolution
//! stays. Past the gap each of the four pairs is `TooFar`, its bracket holding `delta`.
//!
//! The app sews a 0.05 mm gap at its 1 mm cap; here the gap must be under the ratio limit
//! (1000 times the zone), so the gap fixtures run at a zone of 1e-4 mm (limit 0.1 mm).

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;

use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, Draft, EdgeId, Entity, RegionId, ShellRecord, VertexId};
use scx_check::Scope;
use scx_mass::RelAccuracy;
use scx_num::budget::{Budget, Stop};
use scx_num::cert::{Basis, Claim, Recorded};
use scx_num::frame::Frame;
use scx_num::math::Affine;
use scx_num::units::{Length, Resolution};
use scx_num::vector3d::Vec3;
use scx_repair::{OnItemFailure, RepairCause, RepairError, SewCause, sew};
use scx_stitch::{Want, gather, open_edges, stitch};
use scx_testkit::bodies::made;

/// The zone of the gap fixtures: a 0.05 mm gap is under its ratio limit (0.1 mm).
const ZONE: f64 = 1e-4;

fn doc(zone: f64) -> Document {
    Document::with_resolution(Resolution::new(Length::from_mm(zone)).unwrap())
}

fn with_cx<T>(d: &Document, f: impl FnOnce(&mut Cx<'_>) -> T) -> T {
    let settings = Settings::new();
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let mut cx = Cx::new(d, &settings, &mut scratch, &stop, &NoTrace);
    f(&mut cx)
}

fn block(d: &Document) -> Brep {
    made::block(&Frame::WORLD, [10.0, 20.0, 30.0].map(Length::from_mm), d)
        .unwrap()
        .body
}

/// `body`'s faces, each its own sheet, the top (`top`) moved by `lift` in z.
fn sheets(body: &Brep, top: scx_brep::FaceId, lift: f64, d: &Document) -> Vec<(bool, Brep)> {
    body.faces()
        .map(|f| {
            let mut dr = Draft::new();
            let shell = dr.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
            let mut vs: BTreeMap<VertexId, VertexId> = BTreeMap::new();
            let mut es: BTreeMap<EdgeId, EdgeId> = BTreeMap::new();
            let mut rec = f.record().clone();
            for u in rec.loops.iter_mut().flat_map(|l| l.uses.iter_mut()) {
                let e = u.edge;
                if let Some(&x) = es.get(&e) {
                    u.edge = x;
                    continue;
                }
                let mut g = body.edge(e).unwrap().geometry().clone();
                for end in &mut g.ends {
                    let v = *end;
                    *end = *vs.entry(v).or_insert_with(|| {
                        dr.add_vertex(body.vertex(v).unwrap().record().clone())
                            .unwrap()
                    });
                }
                let x = dr.add_edge(g).unwrap();
                es.insert(e, x);
                u.edge = x;
            }
            rec.sides = [shell, shell];
            dr.add_face(rec).unwrap();
            let is_top = f.id() == top;
            if is_top {
                dr.transform(&Affine::translation(Vec3::new(0.0, 0.0, lift)), d)
                    .unwrap();
            }
            (is_top, dr.finish(d).map_err(|u| u.cause).unwrap().0)
        })
        .collect()
}

/// The open box and the lid `lift` above it, one body of two pieces.
fn lidded(lift: f64, d: &Document) -> Brep {
    let made = made::block(&Frame::WORLD, [10.0, 20.0, 30.0].map(Length::from_mm), d).unwrap();
    let all = sheets(&made.body, made.faces.top, lift, d);
    let walls: Vec<&Brep> = all.iter().filter(|(t, _)| !t).map(|(_, b)| b).collect();
    let lid = &all.iter().find(|(t, _)| *t).unwrap().1;
    let open = with_cx(d, |cx| stitch(&walls, Want::Sheets, cx)).unwrap();
    let open = open.done.body;
    with_cx(d, |cx| gather(&[&open, lid], cx)).unwrap().body
}

fn run(
    b: &Brep,
    gap: f64,
    mode: OnItemFailure,
) -> Result<scx_brep::op::Done<SewCause>, RepairError> {
    let d = Document::with_resolution(b.resolution());
    with_cx(&d, |cx| sew(b, Length::from_mm(gap), mode, cx))
}

fn volume(b: &Brep) -> f64 {
    scx_mass::volume(
        b,
        RelAccuracy::new(1e-12).unwrap(),
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .value
}

fn area(b: &Brep) -> f64 {
    scx_mass::area(
        b,
        RelAccuracy::new(1e-12).unwrap(),
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .value
}

fn counts(b: &Brep) -> (usize, usize, usize) {
    (b.faces().count(), b.edges().count(), b.vertices().count())
}

#[test]
fn the_fixture_is_two_pieces_with_eight_free_edges() {
    let d = doc(ZONE);
    let b = lidded(0.05, &d);
    assert_eq!(counts(&b), (6, 16, 12));
    assert_eq!(open_edges(&b).len(), 8);
}

#[test]
fn a_gap_within_the_asked_one_is_sewn_each_sewn_edge_carrying_its_gap() {
    let d = doc(ZONE);
    for (delta, gap) in [(0.05, 0.08), (0.01, 0.08), (0.002, 0.005)] {
        let b = lidded(delta, &d);
        let done = run(&b, gap, OnItemFailure::Stop).unwrap();
        let s = &done.body;
        assert_eq!(counts(s), (6, 12, 8), "{delta}");
        assert!(open_edges(s).is_empty(), "{delta}");
        assert!(done.report.skipped.is_empty(), "{delta}");
        assert_eq!(s.resolution(), b.resolution());

        let v = volume(s);
        let bound = area(s) * delta;
        assert!(
            (v - 6000.0).abs() <= bound,
            "{delta}: volume {v}, bound {bound}"
        );

        // The four sewn edges, raised in the report to their certified gap.
        let raised: Vec<_> = done
            .report
            .raised
            .iter()
            .filter_map(|r| match r.entity {
                Entity::Edge(e) => Some((e, r.after.length().mm())),
                _ => None,
            })
            .collect();
        assert_eq!(raised.len(), 4, "{delta}: {raised:?}");
        let width = 1e-3 * gap + ZONE;
        for &(e, after) in &raised {
            assert!(after >= delta && after <= delta + width, "{delta}: {after}");
            let rec: &Recorded = s.edge(e).unwrap().accuracy().unwrap();
            assert_eq!(rec.acc.length().mm(), after);
            // The edge's use on the dropped face carries the certificate: its lift's gap
            // from the kept curve, `PcurveOnSurface`, the edge's accuracy.
            let certs: Vec<f64> = s
                .faces()
                .flat_map(|f| f.uses().collect::<Vec<_>>())
                .filter(|u| u.edge().id() == e)
                .filter_map(|u| u.bound())
                .map(|b| {
                    let Basis::Certified(c) = &b.basis else {
                        panic!("{delta}: {b:?}")
                    };
                    assert_eq!(c.claim, Claim::PcurveOnSurface);
                    c.bound.mm()
                })
                .collect();
            assert_eq!(certs, vec![after], "{delta}");
        }
        // Every other edge keeps the zone: only the sewn ones carry the gap.
        let loose = s
            .edges()
            .filter(|e| e.accuracy().is_some_and(|r| r.acc.length().mm() > ZONE))
            .count();
        assert_eq!(loose, 4, "{delta}");

        let findings = with_cx(&d, |cx| scx_check::check(s, cx, Scope::Import)).unwrap();
        assert!(findings.is_clean(), "{delta}: {:?}", findings.list);
        // Closed: the result is a solid.
        assert_eq!(s.regions().filter(|r| r.is_solid()).count(), 1, "{delta}");
    }
}

#[test]
fn a_gap_wider_than_asked_is_too_far_with_its_bracket() {
    let d = doc(ZONE);
    let (delta, gap) = (0.05, 0.01);
    let b = lidded(delta, &d);

    let done = run(&b, gap, OnItemFailure::Skip).unwrap();
    assert_eq!(done.report.skipped.len(), 4, "{:?}", done.report.skipped);
    assert_eq!(open_edges(&done.body).len(), 8);
    for (k, s) in done.report.skipped.iter().enumerate() {
        assert_eq!((s.item, s.round), (k, 1));
        let SewCause::TooFar {
            gap: bracket,
            allowed,
            ..
        } = &s.cause
        else {
            panic!("{:?}", s.cause)
        };
        assert_eq!(allowed.mm(), gap);
        assert!(
            bracket.lower() <= delta && delta <= bracket.upper(),
            "{bracket:?}"
        );
        assert!(bracket.upper() - bracket.lower() <= 1e-3 * gap + 1e-12);
        let resize = s.cause.resize().unwrap().mm();
        assert!(resize > gap && resize >= delta, "{resize}");
    }
    // In order of the lowest input edge, each pair once.
    let lows: Vec<EdgeId> = done
        .report
        .skipped
        .iter()
        .map(|s| match &s.cause {
            SewCause::TooFar { edges, .. } => edges[0],
            c => panic!("{c:?}"),
        })
        .collect();
    assert!(lows.windows(2).all(|w| w[0] < w[1]), "{lows:?}");

    // The resize sews it.
    let resize = done.report.skipped[0].cause.resize().unwrap().mm();
    let again = run(&b, resize.max(0.06), OnItemFailure::Stop).unwrap();
    assert!(open_edges(&again.body).is_empty());

    let first = done.report.skipped[0].cause.clone();
    assert_eq!(
        run(&b, gap, OnItemFailure::Stop).unwrap_err(),
        RepairError::Cause(RepairCause::Sew(first))
    );
    match run(&b, gap, OnItemFailure::Collect).unwrap_err() {
        RepairError::Items(items) => {
            assert_eq!(items.len(), 4);
            for (i, s) in items.iter().zip(&done.report.skipped) {
                assert_eq!(i.cause, RepairCause::Sew(s.cause.clone()));
            }
        }
        e => panic!("{e:?}"),
    }
}

#[test]
fn a_closed_part_is_returned_as_it_is() {
    let d = Document::default();
    let b = block(&d);
    let done = run(&b, 0.005, OnItemFailure::Stop).unwrap();
    assert_eq!(counts(&done.body), (6, 12, 8));
    assert!(done.report.skipped.is_empty() && done.report.raised.is_empty());
    assert!((volume(&done.body) - 6000.0).abs() < 1e-6);
}

#[test]
fn a_gap_not_finite_not_above_zero_or_at_the_limit_is_refused() {
    let d = Document::default();
    let b = block(&d);
    let limit = 1000.0 * d.resolution().zone().mm();
    for gap in [0.0, -1.0, limit, 2.0, f64::NAN, f64::INFINITY] {
        match run(&b, gap, OnItemFailure::Stop).unwrap_err() {
            RepairError::Cause(RepairCause::Gap { limit: l, .. }) => assert_eq!(l.mm(), limit),
            e => panic!("{gap}: {e:?}"),
        }
    }
}

#[test]
fn a_body_of_another_document_is_refused() {
    let b = lidded(0.05, &doc(ZONE));
    let other = Document::default();
    assert_eq!(
        with_cx(&other, |cx| sew(
            &b,
            Length::from_mm(0.08),
            OnItemFailure::Stop,
            cx
        ))
        .unwrap_err(),
        RepairError::Cause(RepairCause::OtherDocument)
    );
}

#[test]
fn sew_is_deterministic() {
    let d = doc(ZONE);
    let b = lidded(0.05, &d);
    let one = run(&b, 0.08, OnItemFailure::Stop).unwrap();
    let two = run(&b, 0.08, OnItemFailure::Stop).unwrap();
    assert_eq!(one.report.raised, two.report.raised);
    assert_eq!(one.report.work, two.report.work);
    let ids = |b: &Brep| b.edges().map(|e| e.id()).collect::<Vec<_>>();
    assert_eq!(ids(&one.body), ids(&two.body));
    let far1 = run(&b, 0.01, OnItemFailure::Skip).unwrap();
    let far2 = run(&b, 0.01, OnItemFailure::Skip).unwrap();
    assert_eq!(far1.report.skipped, far2.report.skipped);
}

#[test]
fn a_cancelled_sew_is_cancelled() {
    let d = doc(ZONE);
    let b = lidded(0.05, &d);
    let settings = Settings::new();
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    stop.set();
    let mut cx = Cx::new(&d, &settings, &mut scratch, &stop, &NoTrace);
    assert_eq!(
        sew(&b, Length::from_mm(0.08), OnItemFailure::Stop, &mut cx).unwrap_err(),
        RepairError::Cancelled
    );
}
