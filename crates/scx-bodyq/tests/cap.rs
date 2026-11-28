// Written for solidcorex. Apache-2.0.
//! Whether a curve lies on a face, and whether a prism cut at a face is kept (research 74
//! §34, BW11), on a block and a cylinder whose edges and faces are known. The pin's
//! `every_edge_of_a_cylinder_lies_on_a_face_of_it_and_the_circles_on_two`,
//! `an_edge_of_another_body_is_on_none_of_a_boxes_faces` and
//! `a_part_is_kept_dropped_or_reaching_past_by_its_caps_and_section`, with the trim cases the
//! pin sampled made proofs.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod along_common;

use along_common::{between, doc};
use scx_bodyq::{
    BodyqError, BodyqRefusal, BodyqUnsupported, Lies, Near, PlaneAt, PrismCut, PrismPart, Reach,
    curve_lies_on_face, prism_part,
};
use scx_brep::{Brep, EdgeId, EdgeShape, Entity, FaceId};
use scx_curve::{Curve, Line};
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::frame::Frame;
use scx_num::units::{Accuracy, Length};
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::made::{block, cylinder};

const TOL: f64 = 1e-6;

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn lies(body: &Brep, face: FaceId, edge: EdgeId, from: &Brep) -> Lies {
    let e = from.edge(edge).unwrap();
    let EdgeShape::Curve { curve, interval } = e.shape() else {
        panic!()
    };
    curve_lies_on_face(
        body,
        face,
        curve,
        *interval,
        mm(TOL),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
}

fn segment(a: [f64; 3], b: [f64; 3]) -> (Curve, Extent1d) {
    let (a, b) = (Point3::new(a[0], a[1], a[2]), Point3::new(b[0], b[1], b[2]));
    (
        Curve::Line(Line::through(a, b).unwrap()),
        Extent1d::new(0.0, 1.0).unwrap(),
    )
}

fn on(body: &Brep, face: FaceId, c: &(Curve, Extent1d)) -> Lies {
    curve_lies_on_face(
        body,
        face,
        &c.0,
        c.1,
        mm(TOL),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
}

#[test]
fn every_edge_of_a_cylinder_lies_on_a_face_of_it_and_the_circles_on_two() {
    // The circles are where a plane meets the cylinder; the seam lies on the wall alone.
    let m = cylinder(&Frame::WORLD, mm(5.0), mm(12.0), &doc()).unwrap();
    let faces: Vec<FaceId> = m.body.faces().map(scx_brep::Face::id).collect();
    let mut circles = 0;
    let mut seams = 0;
    for e in m.body.edges() {
        let held = faces
            .iter()
            .filter(|&&f| lies(&m.body, f, e.id(), &m.body) == Lies::Yes)
            .count();
        assert!(held >= 1, "{:?} lies on no face", e.id());
        match held {
            2 => circles += 1,
            1 => seams += 1,
            n => panic!("{n}"),
        }
    }
    assert_eq!((circles, seams), (2, 1));
}

#[test]
fn every_edge_of_a_block_lies_on_exactly_its_two_faces() {
    let m = block(&Frame::WORLD, [mm(10.0), mm(20.0), mm(30.0)], &doc()).unwrap();
    let faces: Vec<FaceId> = m.body.faces().map(scx_brep::Face::id).collect();
    for e in m.body.edges() {
        let held: Vec<FaceId> = faces
            .iter()
            .copied()
            .filter(|&f| lies(&m.body, f, e.id(), &m.body) == Lies::Yes)
            .collect();
        let want: Vec<FaceId> = e.fins().map(|f| f.face).collect();
        assert_eq!(held.len(), 2, "{:?}", e.id());
        assert!(held.iter().all(|f| want.contains(f)), "{held:?} {want:?}");
    }
}

#[test]
fn an_edge_of_another_body_is_on_none_of_a_blocks_faces() {
    let boxed = block(&Frame::WORLD, [mm(10.0), mm(20.0), mm(30.0)], &doc()).unwrap();
    let far = Frame::new(
        Point3::new(100.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    let c = cylinder(&far, mm(5.0), mm(12.0), &doc()).unwrap();
    for e in c.body.edges() {
        for f in boxed.body.faces() {
            // A rim circle lies in the plane of the block's bottom (off its trim); the others
            // are off the plane.
            assert!(
                matches!(
                    lies(&boxed.body, f.id(), e.id(), &c.body),
                    Lies::OffSurface { .. } | Lies::OutsideTrim
                ),
                "{:?} {:?}",
                e.id(),
                f.id()
            );
        }
    }
}

#[test]
fn a_curve_in_a_faces_plane_is_on_it_only_inside_its_trim() {
    // The top of a 10 x 6 block at z = 4.
    let m = block(&Frame::WORLD, [mm(10.0), mm(6.0), mm(4.0)], &doc()).unwrap();
    let top = m.faces.top;
    // A diagonal inside the face lies on it (no edge of the face is near it).
    assert_eq!(
        on(&m.body, top, &segment([2.0, 1.0, 4.0], [8.0, 5.0, 4.0])),
        Lies::Yes
    );
    // One beyond the block's edge, in the same plane, is on the plane and off the face.
    assert_eq!(
        on(&m.body, top, &segment([20.0, 1.0, 4.0], [30.0, 1.0, 4.0])),
        Lies::OutsideTrim
    );
    // One that runs out across the boundary has a stretch outside it.
    assert_eq!(
        on(&m.body, top, &segment([5.0, 1.0, 4.0], [15.0, 1.0, 4.0])),
        Lies::OutsideTrim
    );
    // A curve off the plane is refuted by its gap, with a lower bound on it.
    match on(&m.body, top, &segment([2.0, 1.0, 4.5], [8.0, 5.0, 4.5])) {
        Lies::OffSurface { by } => assert!(by.mm() > 0.4 && by.mm() <= 0.5 + 1e-9, "{by:?}"),
        other => panic!("{other:?}"),
    }
    // Along an edge of the face, within the tolerance, on its boundary: on it.
    assert_eq!(
        on(&m.body, top, &segment([0.0, 0.0, 4.0], [10.0, 0.0, 4.0])),
        Lies::Yes
    );
    // A boundary stretch only part of an edge is on it too.
    assert_eq!(
        on(&m.body, top, &segment([2.0, 0.0, 4.0], [7.0, 0.0, 4.0])),
        Lies::Yes
    );
}

#[test]
fn a_budget_and_a_stop_end_the_proof() {
    let m = block(&Frame::WORLD, [mm(10.0), mm(6.0), mm(4.0)], &doc()).unwrap();
    let (c, i) = segment([5.0, 1.0, 4.0], [15.0, 1.0, 4.0]);
    let run = |budget: &mut Budget, stop: &Stop| {
        curve_lies_on_face(&m.body, m.faces.top, &c, i, mm(TOL), budget, stop)
    };
    assert!(matches!(
        run(&mut Budget::new(1), &Stop::new()),
        Err(BodyqError::Exhausted { .. })
    ));
    let stop = Stop::new();
    stop.set();
    assert_eq!(
        run(&mut Budget::unlimited(), &stop),
        Err(BodyqError::Cancelled)
    );
}

#[test]
fn refusals_and_a_nurbs_target_face() {
    let m = block(&Frame::WORLD, [mm(10.0), mm(6.0), mm(4.0)], &doc()).unwrap();
    let (c, i) = segment([2.0, 1.0, 4.0], [8.0, 5.0, 4.0]);
    let go = |tol: f64, face: FaceId| {
        curve_lies_on_face(
            &m.body,
            face,
            &c,
            i,
            mm(tol),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
    };
    assert_eq!(
        go(-1.0, m.faces.top),
        Err(BodyqError::Refused(BodyqRefusal::Tolerance))
    );
    assert_eq!(
        go(f64::NAN, m.faces.top),
        Err(BodyqError::Refused(BodyqRefusal::NotFinite))
    );
    let nothing = FaceId::from_raw(999);
    assert_eq!(
        go(1e-6, nothing),
        Err(BodyqError::Refused(BodyqRefusal::NoEntity {
            entity: Entity::Face(nothing)
        }))
    );
    let (sheet, _, patch) = along_common::nurbs_meets_plane();
    assert_eq!(
        curve_lies_on_face(
            &sheet,
            patch,
            &c,
            i,
            mm(1e-6),
            &mut Budget::unlimited(),
            &Stop::new()
        ),
        Err(BodyqError::Unsupported(BodyqUnsupported::Nurbs {
            at: Entity::Face(patch)
        }))
    );
}

#[test]
fn a_part_is_kept_dropped_or_reaching_past_by_its_caps_and_section() {
    let m = block(&Frame::WORLD, [mm(10.0), mm(20.0), mm(30.0)], &doc()).unwrap();
    let body = &m.body;
    let near = Near::of(
        doc().resolution(),
        Accuracy::ZERO,
        Accuracy::ZERO,
        mm(100.0),
    )
    .unwrap();
    // The bottom's front edge (y = 0, z = 0) stands for the section; it lies on the bottom
    // and the y = 0 face.
    let edge = between(body, m.faces.bottom, m.faces.sides[0])[0];
    let (on_face, other) = (m.faces.sides[0], m.faces.sides[2]);
    let cut = PrismCut {
        origin: Point3::new(0.0, 0.0, 0.0),
        axis: Vec3::new(0.0, 0.0, 1.0),
        start: mm(0.0),
        far: mm(100.0),
    };
    let cap = |z: f64| PlaneAt {
        origin: Point3::new(1.0, 1.0, z),
        normal: Vec3::new(0.0, 0.0, 1.0),
    };
    let judge = |face: FaceId, planar: &[PlaneAt], section: &[EdgeId]| {
        prism_part(
            body,
            face,
            body,
            planar,
            section,
            &cut,
            &near,
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap()
    };
    assert_eq!(judge(on_face, &[cap(0.0)], &[edge]), PrismPart::Kept);
    assert_eq!(judge(on_face, &[cap(5.0)], &[edge]), PrismPart::Dropped);
    assert_eq!(
        judge(on_face, &[cap(0.0), cap(100.0)], &[edge]),
        PrismPart::ReachesPast(Reach::FarCap)
    );
    assert_eq!(
        judge(other, &[cap(0.0)], &[edge]),
        PrismPart::ReachesPast(Reach::SectionOff { edge })
    );
    assert_eq!(
        judge(on_face, &[cap(0.0)], &[]),
        PrismPart::ReachesPast(Reach::EmptySection)
    );
}
