// Written for solidcorex. Apache-2.0.
//! `still_names` judged on constructed truth: a cube moved by a translation keeps every id;
//! a lump removed by an edit takes its entities with it; renumbering follows; and the rules
//! for a split and a merge on lineage written out by hand.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use scx_bodyq::{BodyqError, BodyqRefusal, Standing, Step, Stored, still_names};
use scx_brep::{Brep, Document, EdgeId, Entity, FaceId, IdMap, Merge, Origin, RegionId, VertexId};
use scx_num::budget::{Budget, Stop};
use scx_num::math::Affine;
use scx_num::vector3d::{Point3, Vec3};

fn pt(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

fn names(body: &Brep, s: &[Stored], h: &[Step<'_>]) -> Vec<Standing> {
    still_names(body, s, h, &mut Budget::unlimited(), &Stop::new()).unwrap()
}

/// The cube's faces by the plane they lie in, and its first and last of each other kind.
fn face_at(body: &Brep, p: [f64; 3]) -> FaceId {
    let hits = scx_bodyq::pick_at(
        body,
        &[pt(p)],
        scx_brep::EntityKind::Face,
        scx_num::units::Length::from_mm(0.0),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap();
    match hits[0][..] {
        [h] => match h.entity {
            Entity::Face(f) => f,
            e => panic!("{e:?}"),
        },
        ref h => panic!("{p:?}: {h:?}"),
    }
}

#[test]
fn a_reference_against_its_own_body_names_the_entity_the_point_lies_on() {
    let cube = common::unit_cube();
    let bottom = face_at(&cube, [0.5, 0.5, 0.0]);
    let top = face_at(&cube, [0.5, 0.5, 1.0]);
    let r = |f: FaceId, p| Stored {
        entity: Entity::Face(f),
        at: pt(p),
    };
    let got = names(
        &cube,
        &[
            r(bottom, [0.3, 0.6, 0.0]),
            // on the bottom's plane but past its edge, and on the other face: neither is on it
            r(bottom, [1.5, 0.5, 0.0]),
            r(bottom, [0.5, 0.5, 1.0]),
            // an id the body never had
            r(FaceId::from_raw(99), [0.5, 0.5, 0.0]),
            r(top, [0.5, 0.5, 1.0]),
        ],
        &[],
    );
    assert_eq!(
        got,
        vec![
            Standing::Names(vec![Entity::Face(bottom)]),
            Standing::Moved(vec![Entity::Face(bottom)]),
            Standing::Moved(vec![Entity::Face(bottom)]),
            Standing::Gone,
            Standing::Names(vec![Entity::Face(top)]),
        ]
    );
}

/// An edge and a vertex are named by the point on them; a stored face at an edge's point is
/// named too (the point is on the face's boundary).
#[test]
fn edges_and_vertices_are_named_by_their_points() {
    let cube = common::unit_cube();
    let hit = |kind, p| {
        scx_bodyq::pick_at(
            &cube,
            &[pt(p)],
            kind,
            scx_num::units::Length::from_mm(0.0),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap()[0][0]
            .entity
    };
    let e = hit(scx_brep::EntityKind::Edge, [0.5, 0.0, 0.0]);
    let v = hit(scx_brep::EntityKind::Vertex, [1.0, 1.0, 1.0]);
    let got = names(
        &cube,
        &[
            Stored {
                entity: e,
                at: pt([0.25, 0.0, 0.0]),
            },
            Stored {
                entity: e,
                at: pt([0.25, 0.1, 0.0]),
            },
            Stored {
                entity: v,
                at: pt([1.0, 1.0, 1.0]),
            },
            Stored {
                entity: v,
                at: pt([1.0, 1.0, 0.9]),
            },
        ],
        &[],
    );
    assert_eq!(
        got,
        vec![
            Standing::Names(vec![e]),
            Standing::Moved(vec![e]),
            Standing::Names(vec![v]),
            Standing::Moved(vec![v]),
        ]
    );
}

/// A translation keeps every id, replacing every record: references survive when their
/// points move with the body, and are `Moved` when they do not.
#[test]
fn references_survive_a_translate() {
    let cube = common::unit_cube();
    let by = Vec3::new(2.0, -3.0, 4.0);
    let mut draft = cube.edit();
    draft
        .transform(&Affine::translation(by), &Document::default())
        .unwrap();
    let (moved, changes) = draft
        .finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap();
    let probes: Vec<(Entity, [f64; 3])> = vec![
        (
            Entity::Face(face_at(&cube, [0.5, 0.5, 0.0])),
            [0.5, 0.5, 0.0],
        ),
        (
            Entity::Face(face_at(&cube, [1.0, 0.5, 0.5])),
            [1.0, 0.5, 0.5],
        ),
        (
            scx_bodyq::pick_at(
                &cube,
                &[pt([0.5, 0.0, 1.0])],
                scx_brep::EntityKind::Edge,
                scx_num::units::Length::from_mm(0.0),
                &mut Budget::unlimited(),
                &Stop::new(),
            )
            .unwrap()[0][0]
                .entity,
            [0.5, 0.0, 1.0],
        ),
        (Entity::Vertex(VertexId::from_raw(3)), [0.0, 1.0, 0.0]),
    ];
    let history = [Step::Edit(&changes)];
    let carried: Vec<Stored> = probes
        .iter()
        .map(|&(entity, p)| Stored {
            entity,
            at: pt(p) + by,
        })
        .collect();
    let stale: Vec<Stored> = probes
        .iter()
        .map(|&(entity, p)| Stored { entity, at: pt(p) })
        .collect();
    let got = names(&moved, &carried, &history);
    for (g, &(e, _)) in got.iter().zip(&probes) {
        assert_eq!(g, &Standing::Names(vec![e]));
    }
    for (g, &(e, _)) in names(&moved, &stale, &history).iter().zip(&probes) {
        assert_eq!(g, &Standing::Moved(vec![e]));
    }
}

/// Two cubes in one body, then an edit that removes the second whole (what a Boolean that
/// consumes a lump reports): references to its faces, edges and vertices are `Gone`, those
/// of the first are named; after `compact` the first lump's ids need no renumbering, and
/// after removing the first instead, the second's follow the renumbering.
#[test]
fn references_to_a_removed_lump_are_gone_and_renumbering_is_followed() {
    let (two, a, b) = common::two_cubes();
    for (keep, drop) in [(&a, &b), (&b, &a)] {
        let mut draft = two.edit();
        for &f in &drop.faces {
            draft.remove_face(f).unwrap();
        }
        for &e in &drop.edges {
            draft.remove_edge(e).unwrap();
        }
        for &v in &drop.vertices {
            draft.remove_vertex(v).unwrap();
        }
        draft.remove_shell(drop.shell).unwrap();
        draft.remove_shell(drop.outside).unwrap();
        draft.remove_region(drop.region).unwrap();
        let (after, changes) = draft
            .finish(&Document::default())
            .map_err(|u| u.cause)
            .unwrap();
        let (compact, map): (Brep, IdMap) = after.compact();
        let stored = |s: &common::Lump, k: usize| -> Vec<Stored> {
            vec![
                Stored {
                    entity: Entity::Face(s.faces[k]),
                    at: s.face_points[k],
                },
                Stored {
                    entity: Entity::Edge(s.edges[0]),
                    at: s.edge_point,
                },
                Stored {
                    entity: Entity::Vertex(s.vertices[0]),
                    at: s.vertex_point,
                },
            ]
        };
        // Un-compacted: the kept lump's references are named, the dropped one's are gone.
        let h = [Step::Edit(&changes)];
        for k in 0..6 {
            let got = names(&after, &stored(keep, k), &h);
            assert_eq!(got[0], Standing::Names(vec![Entity::Face(keep.faces[k])]));
            assert_eq!(got[1], Standing::Names(vec![Entity::Edge(keep.edges[0])]));
            assert_eq!(
                got[2],
                Standing::Names(vec![Entity::Vertex(keep.vertices[0])])
            );
            assert!(
                names(&after, &stored(drop, k), &h)
                    .iter()
                    .all(|s| *s == Standing::Gone)
            );
        }
        // Compacted: the history carries on through the renumbering.
        let h = [Step::Edit(&changes), Step::Compact(&map)];
        let got = names(&compact, &stored(keep, 2), &h);
        let renumbered = map.face(keep.faces[2]).unwrap();
        assert_eq!(got[0], Standing::Names(vec![Entity::Face(renumbered)]));
        assert_eq!(
            got[1],
            Standing::Names(vec![Entity::Edge(map.edge(keep.edges[0]).unwrap())])
        );
        assert!(
            names(&compact, &stored(drop, 2), &h)
                .iter()
                .all(|s| *s == Standing::Gone)
        );
        // Without the compaction step the same references are about other entities or none.
        if renumbered != keep.faces[2] {
            let wrong = names(&compact, &stored(keep, 2), &[Step::Edit(&changes)]);
            assert_ne!(wrong[0], Standing::Names(vec![Entity::Face(renumbered)]));
        }
    }
}

/// A lump rebuilt by an edit that declares where its faces came from (what a Boolean that
/// re-makes a lump reports): two new faces made from one old face, and a survivor of a merge.
/// The point says which piece a reference names, and on the pieces' shared edge both.
#[test]
fn the_pieces_made_from_a_face_are_told_apart_by_the_point() {
    let mut b = common::Builder::new();
    let old = b.cube([0.0; 3]);
    let before = b.finish();
    let mut b = common::Builder::extend(&before);
    for &f in &old.faces {
        b.draft.remove_face(f).unwrap();
    }
    for &e in &old.edges {
        b.draft.remove_edge(e).unwrap();
    }
    for &v in &old.vertices {
        b.draft.remove_vertex(v).unwrap();
    }
    let new = b.cube([0.0; 3]);
    // The old bottom is now the new bottom and the new front; every other face is its twin.
    let (bottom, front) = (new.faces[0], new.faces[2]);
    for &f in &[bottom, front] {
        b.draft.derive(f, Origin::Face(old.faces[0])).unwrap();
    }
    for k in 1..6 {
        if k != 2 {
            b.draft
                .derive(new.faces[k], Origin::Face(old.faces[k]))
                .unwrap();
        }
    }
    let (after, mut changes) = b
        .draft
        .finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap();
    let h = [Step::Edit(&changes)];
    let at = |f: FaceId, p: Point3| Stored {
        entity: Entity::Face(f),
        at: p,
    };
    let ask = |f: FaceId, p| names(&after, &[at(f, p)], &h).remove(0);
    let both = {
        let mut v = vec![Entity::Face(bottom), Entity::Face(front)];
        v.sort_unstable();
        v
    };
    assert_eq!(
        ask(old.faces[0], old.face_points[0]),
        Standing::Names(vec![Entity::Face(bottom)])
    );
    assert_eq!(
        ask(old.faces[0], old.face_points[2]),
        Standing::Names(vec![Entity::Face(front)])
    );
    assert_eq!(
        ask(old.faces[0], pt([0.5, 0.0, 0.0])),
        Standing::Names(both.clone())
    );
    assert_eq!(
        ask(old.faces[0], pt([0.5, 0.5, 0.5])),
        Standing::Moved(both)
    );
    // The twin faces are one each, and a face's point on another face is not its name.
    assert_eq!(
        ask(old.faces[1], old.face_points[1]),
        Standing::Names(vec![Entity::Face(new.faces[1])])
    );
    assert_eq!(
        ask(old.faces[1], old.face_points[0]),
        Standing::Moved(vec![Entity::Face(new.faces[1])])
    );
    // Edges and vertices were not derived: nothing of the new body descends from them.
    let lost = names(
        &after,
        &[
            Stored {
                entity: Entity::Edge(old.edges[0]),
                at: old.edge_point,
            },
            Stored {
                entity: Entity::Vertex(old.vertices[0]),
                at: old.vertex_point,
            },
        ],
        &h,
    );
    assert_eq!(lost, vec![Standing::Gone, Standing::Gone]);
    // A merge written into the lineage: face 90 of an earlier body went into the top face.
    changes.faces.merged.push(Merge {
        survivor: new.faces[1],
        absorbed: vec![FaceId::from_raw(90)],
        taken: Vec::new(),
    });
    let h = [Step::Edit(&changes)];
    assert_eq!(
        names(&after, &[at(FaceId::from_raw(90), new.face_points[1])], &h),
        vec![Standing::Names(vec![Entity::Face(new.faces[1])])]
    );
}

#[test]
fn bad_requests_are_refused() {
    let cube = common::unit_cube();
    let f = |e, p| {
        still_names(
            &cube,
            &[Stored { entity: e, at: p }],
            &[],
            &mut Budget::unlimited(),
            &Stop::new(),
        )
    };
    assert_eq!(
        f(Entity::Region(RegionId::OUTSIDE), pt([0.0; 3])),
        Err(BodyqError::Refused(BodyqRefusal::NotPickable {
            kind: scx_brep::EntityKind::Region
        }))
    );
    assert_eq!(
        f(Entity::Face(FaceId::from_raw(0)), pt([f64::NAN, 0.0, 0.0])),
        Err(BodyqError::Refused(BodyqRefusal::NotFinite))
    );
    let _ = (EdgeId::from_raw(0),);
}
