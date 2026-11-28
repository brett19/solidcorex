// Written for solidcorex. Apache-2.0.
//! `pick_at` judged against closed forms on a unit cube and a cylinder: a point on a face,
//! on an edge, off a face past its trim, and the order and completeness of what is returned.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use scx_bodyq::{BodyqError, BodyqRefusal, Picked, pick_at};
use scx_brep::{Brep, Entity, EntityKind};
use scx_num::budget::{Budget, Stop};
use scx_num::units::Length;
use scx_num::vector3d::Point3;

fn pt(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

fn pick(body: &Brep, p: [f64; 3], want: EntityKind, aperture: f64) -> Vec<Picked> {
    pick_at(
        body,
        &[pt(p)],
        want,
        Length::from_mm(aperture),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .remove(0)
}

fn dists(h: &[Picked]) -> Vec<f64> {
    h.iter().map(|h| h.distance).collect()
}

/// The unit cube's face lying in the plane `axis = value`: its box is flat there.
fn face_in(body: &Brep, axis: usize, value: f64) -> Entity {
    let mut found = body.faces().filter(|f| {
        let b = f.bounding_box().unwrap();
        let (lo, hi) = [
            (b.x_min(), b.x_max()),
            (b.y_min(), b.y_max()),
            (b.z_min(), b.z_max()),
        ][axis];
        (lo - value).abs() < 1e-3 && (hi - value).abs() < 1e-3
    });
    let f = found.next().expect("a face in the plane");
    assert!(found.next().is_none());
    Entity::Face(f.id())
}

#[test]
fn a_point_on_a_cube_face_picks_that_face() {
    let cube = common::unit_cube();
    let hits = pick(&cube, [0.5, 0.5, 0.0], EntityKind::Face, 0.0);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].entity, face_in(&cube, 2, 0.0));
    assert!(hits[0].distance < 1e-9);
    let at = hits[0].at;
    assert!((at.x - 0.5).abs() < 1e-9 && (at.y - 0.5).abs() < 1e-9 && at.z.abs() < 1e-9);
}

#[test]
fn a_point_on_an_edge_picks_the_edge_and_both_faces() {
    let cube = common::unit_cube();
    // The middle of the edge x in [0, 1], y = 0, z = 0: it joins the faces z = 0 and y = 0.
    let p = [0.5, 0.0, 0.0];
    let edges = pick(&cube, p, EntityKind::Edge, 0.0);
    assert_eq!(edges.len(), 1);
    assert!(edges[0].distance < 1e-9);
    let faces = pick(&cube, p, EntityKind::Face, 0.0);
    let mut got: Vec<Entity> = faces.iter().map(|h| h.entity).collect();
    got.sort_unstable();
    let mut want = vec![face_in(&cube, 2, 0.0), face_in(&cube, 1, 0.0)];
    want.sort_unstable();
    assert_eq!(got, want);
    assert!(dists(&faces).iter().all(|&d| d < 1e-9));
    // Within an aperture of zero no vertex; within half an edge both ends, by distance 0.5.
    assert!(pick(&cube, p, EntityKind::Vertex, 0.0).is_empty());
    let ends = pick(&cube, p, EntityKind::Vertex, 0.5);
    assert_eq!(ends.len(), 2);
    assert!(dists(&ends).iter().all(|d| (d - 0.5).abs() < 1e-9));
    // Edges within 0.5: this one and the four that meet its ends, each at 0.5 exactly.
    let near = pick(&cube, p, EntityKind::Edge, 0.5);
    assert_eq!(near.len(), 5, "{near:?}");
    assert_eq!(near[0].entity, edges[0].entity);
}

#[test]
fn every_hit_is_sorted_by_distance_then_id() {
    let cube = common::unit_cube();
    // Distances to the faces x = 0, z = 0, y = 0: 0.05, 0.1, 0.15; the rest are over 0.5.
    let hits = pick(&cube, [0.05, 0.15, 0.1], EntityKind::Face, 0.2);
    let want = [
        (face_in(&cube, 0, 0.0), 0.05),
        (face_in(&cube, 2, 0.0), 0.1),
        (face_in(&cube, 1, 0.0), 0.15),
    ];
    assert_eq!(hits.len(), 3);
    for (h, (e, d)) in hits.iter().zip(want) {
        assert_eq!(h.entity, e);
        assert!((h.distance - d).abs() < 1e-9);
    }
    // Ties by id on every kind, wherever the aperture lands.
    for kind in [EntityKind::Face, EntityKind::Edge, EntityKind::Vertex] {
        for a in [0.3, 0.6, 1.1] {
            let h = pick(&cube, [0.5, 0.5, 0.0], kind, a);
            for w in h.windows(2) {
                let key = |p: &Picked| (p.distance, p.entity);
                let (x, y) = (key(&w[0]), key(&w[1]));
                assert!(
                    x.0 < y.0 || (x.0 == y.0 && x.1 < y.1),
                    "{kind:?} {a}: {h:?}"
                );
            }
        }
    }
}

/// A face's distance is to the face as trimmed: past the rim the nearest point is the edge,
/// though the plane is at distance zero.
#[test]
fn a_face_is_picked_by_its_trim_not_its_surface() {
    let cube = common::unit_cube();
    let p = [2.0, 0.5, 0.0];
    let bottom = face_in(&cube, 2, 0.0);
    assert!(pick(&cube, p, EntityKind::Face, 0.9).is_empty());
    let hits = pick(&cube, p, EntityKind::Face, 1.0);
    // Only the bottom is within one of it (x = 1 is at distance 1.0 as well).
    let on_bottom = hits.iter().find(|h| h.entity == bottom).unwrap();
    assert!((on_bottom.distance - 1.0).abs() < 1e-9);
    assert!((on_bottom.at.x - 1.0).abs() < 1e-9 && on_bottom.at.z.abs() < 1e-9);
    assert_eq!(hits.len(), 2);
}

#[test]
fn a_point_that_picks_nothing_is_an_empty_answer_and_the_batch_goes_on() {
    let cube = common::unit_cube();
    let out = pick_at(
        &cube,
        &[
            pt([50.0, 0.0, 0.0]),
            pt([0.5, 0.5, 0.0]),
            pt([0.0, 50.0, 0.0]),
        ],
        EntityKind::Face,
        Length::from_mm(0.1),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap();
    assert_eq!(out.len(), 3);
    assert!(out[0].is_empty() && out[2].is_empty());
    assert_eq!(out[1].len(), 1);
}

#[test]
fn the_same_picks_give_the_same_hits_and_work() {
    let cube = common::unit_cube();
    let run = || {
        let mut b = Budget::unlimited();
        let h = pick_at(
            &cube,
            &[pt([0.5, 0.0, 0.0]), pt([0.2, 0.9, 0.4])],
            EntityKind::Edge,
            Length::from_mm(0.6),
            &mut b,
            &Stop::new(),
        )
        .unwrap();
        (h, b.used())
    };
    assert_eq!(run(), run());
}

#[test]
fn a_budget_that_runs_out_is_an_error_not_a_partial_answer() {
    let cube = common::unit_cube();
    let r = pick_at(
        &cube,
        &[pt([0.5, 0.5, 0.0])],
        EntityKind::Face,
        Length::from_mm(2.0),
        &mut Budget::new(1),
        &Stop::new(),
    );
    assert!(
        matches!(r, Err(BodyqError::Exhausted { limit: 1 })),
        "{r:?}"
    );
}

#[test]
fn bad_requests_are_refused() {
    let cube = common::unit_cube();
    let go = |p: [f64; 3], k: EntityKind, a: f64| {
        pick_at(
            &cube,
            &[pt(p)],
            k,
            Length::from_mm(a),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
    };
    assert_eq!(
        go([f64::NAN, 0.0, 0.0], EntityKind::Face, 1.0),
        Err(BodyqError::Refused(BodyqRefusal::NotFinite))
    );
    for a in [-1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            go([0.0; 3], EntityKind::Face, a),
            Err(BodyqError::Refused(BodyqRefusal::Tolerance))
        );
    }
    assert_eq!(
        go([0.0; 3], EntityKind::Shell, 1.0),
        Err(BodyqError::Refused(BodyqRefusal::NotPickable {
            kind: EntityKind::Shell
        }))
    );
}

/// The world cylinder of radius 1.2 and height 2.5 about the z axis from z = 0: a point 0.3
/// off its side picks the side at 0.3; a point 0.3 past the rim above the cap picks the cap
/// and the side, each at 0.3, the nearest point of each being the rim.
#[test]
fn a_cylinder_is_picked_at_closed_form_distances() {
    let (_, body, _) = common::primitives()
        .into_iter()
        .find(|(n, ..)| n == "cylinder world")
        .unwrap();
    let side = pick(&body, [0.0, 1.5, 1.0], EntityKind::Face, 0.4);
    assert_eq!(side.len(), 1);
    assert!((side[0].distance - 0.3).abs() < 1e-9);
    let rim = pick(&body, [0.0, 1.5, 2.5], EntityKind::Face, 0.4);
    assert_eq!(rim.len(), 2, "{rim:?}");
    assert!(dists(&rim).iter().all(|d| (d - 0.3).abs() < 1e-9));
}
