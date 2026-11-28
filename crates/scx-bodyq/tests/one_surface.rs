// Written for solidcorex. Apache-2.0.
//! `faces_on_one_surface` (research 74 §46) judged against closed forms: a subdivided block
//! (each face on its own frame) grows to its six walls, a cylinder split into faces to its
//! wall, near misses by the resolution, and the typed reasons an edge stops the growth.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeSet;

use scx_bodyq::{
    BodyqError, BodyqRefusal, NotCrossed, NotOneSurface, OneSurface, faces_on_one_surface,
};
use scx_brep::{Brep, Document, EdgeId, Entity, FaceId};
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Frame;
use scx_num::vector3d::Vec3;
use scx_surface::{Cylinder, Plane, Surface};
use scx_testkit::bodies::{Kind, generate};

fn solid(kind: Kind, seed: u64, n: usize) -> Brep {
    generate(kind, seed, n)
        .unwrap()
        .draft
        .finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap()
        .0
}

fn ids(b: &Brep) -> Vec<FaceId> {
    b.faces().map(scx_brep::Face::id).collect()
}

fn grow(b: &Brep, seeds: &[FaceId], keep: &[EdgeId]) -> (OneSurface, u64) {
    let mut budget = Budget::unlimited();
    let r = faces_on_one_surface(b, seeds, keep, &mut budget, &Stop::new()).unwrap();
    (r, budget.used())
}

/// A face's plane: its unit normal and its offset along it.
fn offset_of(b: &Brep, f: FaceId) -> ([f64; 3], f64) {
    match b.face(f).unwrap().surface() {
        Surface::Plane(p) => {
            let z = Vec3::from(p.frame().z());
            let o = p.frame().origin();
            ([z.x, z.y, z.z], z.x * o.x + z.y * o.y + z.z * o.z)
        }
        s => panic!("not a plane: {s:?}"),
    }
}

/// The unit block of `n` steps a side: growing from each unassigned face splits its `6 n^2`
/// faces into six walls of `n^2`, each on one plane (normal and offset equal to the bit
/// after the frame is dropped to 1e-9), by growth across edges only.
#[test]
fn a_subdivided_block_grows_to_its_six_walls() {
    for (seed, n) in [(0, 2), (0, 3), (0, 5), (1, 3), (2, 3), (3, 2)] {
        let b = solid(Kind::Block, seed, n);
        let mut left: BTreeSet<FaceId> = ids(&b).into_iter().collect();
        assert_eq!(left.len(), 6 * n * n);
        let mut walls = 0;
        while let Some(&f) = left.iter().next() {
            let (r, used) = grow(&b, &[f], &[]);
            assert_eq!(r.faces.len(), n * n, "seed {seed} steps {n}");
            assert_eq!(r.faces[0], f);
            let (nz, d) = offset_of(&b, f);
            for &g in &r.faces {
                assert!(left.remove(&g), "a face reached twice");
                let (gz, gd) = offset_of(&b, g);
                for i in 0..3 {
                    assert!((gz[i] - nz[i]).abs() < 1e-9);
                }
                assert!((gd - d).abs() < 1e-9);
            }
            // Every edge of the wall is read once and proved once: 2 n (n + 1) edges.
            assert_eq!(used, (4 * n * (n + 1)) as u64, "seed {seed} steps {n}");
            // The 4 n edges round the wall go to the faces of the next walls, at a crease.
            assert_eq!(r.stopped.len(), 4 * n);
            assert!(
                r.stopped
                    .iter()
                    .all(|s| s.why == NotCrossed::Surface(NotOneSurface::Crease))
            );
            walls += 1;
        }
        assert_eq!(walls, 6);
    }
}

/// Seeds come first in the order given, once each; two seeds on one wall give the wall once.
#[test]
fn seeds_come_first_and_are_not_repeated() {
    let b = solid(Kind::Block, 0, 2);
    let all = ids(&b);
    let (one, _) = grow(&b, &[all[5]], &[]);
    let wall: Vec<FaceId> = one.faces.clone();
    let other = wall[2];
    let (two, _) = grow(&b, &[other, all[5], other], &[]);
    assert_eq!(two.faces[0], other);
    assert_eq!(two.faces[1], all[5]);
    assert_eq!(
        two.faces.iter().collect::<BTreeSet<_>>(),
        wall.iter().collect::<BTreeSet<_>>()
    );
    assert_eq!(two.faces.len(), 4);
    // The same call twice is the same answer and the same work.
    assert_eq!(
        grow(&b, &[other, all[5]], &[]),
        grow(&b, &[other, all[5]], &[])
    );
    // No seed, no growth.
    assert_eq!(grow(&b, &[], &[]).0, OneSurface::default());
}

/// A split line the caller keeps is not crossed (and is not reported): the wall stays a half.
#[test]
fn an_edge_kept_is_not_crossed() {
    let b = solid(Kind::Block, 0, 2);
    let f = ids(&b)[0];
    let (r, _) = grow(&b, &[f], &[]);
    assert_eq!(r.faces.len(), 4);
    // Keep both edges of the cross that touch the seed: it stays in its quadrant.
    let kept: Vec<EdgeId> = b
        .face(f)
        .unwrap()
        .uses()
        .map(|u| u.edge().id())
        .filter(|&e| b.edge(e).unwrap().fins().all(|x| r.faces.contains(&x.face)))
        .collect();
    assert_eq!(kept.len(), 2);
    let (k, _) = grow(&b, &[f], &kept);
    assert_eq!(k.faces, vec![f]);
    assert!(k.stopped.iter().all(|s| !kept.contains(&s.edge)));
}

/// A cylinder of `n` wall faces on one cylinder: the walls are one surface (`==`), the caps
/// are planes that cross it at a right angle, so each wall grows to the `n` and no further.
#[test]
fn a_cylinder_split_into_faces_grows_to_its_wall() {
    for n in [2, 3, 7] {
        let b = solid(Kind::Cylinder, 0, n);
        let walls: Vec<FaceId> = b
            .faces()
            .filter(|f| matches!(f.surface(), Surface::Cylinder(_)))
            .map(scx_brep::Face::id)
            .collect();
        assert_eq!(walls.len(), n);
        let (r, _) = grow(&b, &[walls[0]], &[]);
        assert_eq!(r.faces.len(), n);
        assert_eq!(
            r.faces.iter().collect::<BTreeSet<_>>(),
            walls.iter().collect::<BTreeSet<_>>()
        );
        // The two arcs of each wall on the caps (a cylinder-to-plane edge each) stop it: a
        // cylinder and a plane are not one surface.
        let caps: Vec<_> = r
            .stopped
            .iter()
            .filter(|s| s.why == NotCrossed::Surface(NotOneSurface::Kind))
            .collect();
        assert_eq!(caps.len(), 2 * n, "{:?}", r.stopped);
        assert_eq!(r.stopped.len(), 2 * n);
    }
}

/// The same cylinder on another frame (moved along its axis): the same surface in space,
/// not `==`, and not proved here: typed `Kind`, the cylinder wall splits in two.
#[test]
fn a_cylinder_on_another_frame_is_not_proved() {
    let b = solid(Kind::Cylinder, 0, 3);
    let walls: Vec<FaceId> = b
        .faces()
        .filter(|f| matches!(f.surface(), Surface::Cylinder(_)))
        .map(scx_brep::Face::id)
        .collect();
    let Surface::Cylinder(c) = b.face(walls[1]).unwrap().surface().clone() else {
        panic!()
    };
    let f = c.frame();
    let z = Vec3::from(f.z());
    let moved = Frame::new(f.origin() + z * 0.25, z, Vec3::from(f.x())).unwrap();
    let c2 = Cylinder::new(moved, c.radius(), c.sense()).unwrap();
    let doc = Document::default();
    let mut d = b.edit();
    d.edit_face(walls[1], |rec| rec.surface = Surface::Cylinder(c2));
    d.write_pcurves(walls[1], &doc).unwrap();
    let b2 = d.finish(&doc).map_err(|u| u.cause).unwrap().0;
    let (r, _) = grow(&b2, &[walls[0]], &[]);
    assert_eq!(
        r.faces.iter().collect::<BTreeSet<_>>(),
        BTreeSet::from([&walls[0], &walls[2]]),
        "walls 0 and 2 are `==`; wall 1 is on another frame"
    );
    let kind = r
        .stopped
        .iter()
        .filter(|s| s.why == NotCrossed::Surface(NotOneSurface::Kind))
        .count();
    // Two cap arcs on each of the two walls reached, and the two edges into wall 1.
    assert_eq!(kind, 6, "{:?}", r.stopped);
}

/// A plane of face `f` of `b` moved along its normal by `d` and tilted by `tilt` about the
/// face's x axis, written back with its pcurves.
fn nudged(b: &Brep, f: FaceId, d: f64, tilt: f64) -> Brep {
    let Surface::Plane(p) = b.face(f).unwrap().surface().clone() else {
        panic!()
    };
    let fr = p.frame();
    let (x, z) = (Vec3::from(fr.x()), Vec3::from(fr.z()));
    let y = z.cross(x);
    let z2 = z * scx_num::fmath::cos(tilt) + y * scx_num::fmath::sin(tilt);
    let moved = Frame::new(fr.origin() + z * d, z2, x).unwrap();
    let doc = Document::default();
    let mut dr = b.edit();
    dr.edit_face(f, |rec| rec.surface = Surface::Plane(Plane::new(moved)));
    // The pcurve writer places a curve within `approx` of its surface and no further: a
    // larger move keeps the pcurves as they were (the body is then off its recorded surface,
    // which is what a near miss that `finish` still accepts is).
    if d.abs() < 0.9 * b.resolution().approx().mm() {
        dr.write_pcurves(f, &doc).unwrap();
    }
    dr.finish(&doc).map_err(|u| u.cause).unwrap().0
}

/// Near misses, on the unit block of 2 steps (4 faces a wall): a face moved by under `approx`
/// along its normal is still one surface with its neighbours; moved by more (but under the
/// zone, which `finish` still accepts), it is `Apart` by the proved bound.
#[test]
fn near_misses_are_measured_against_the_resolution() {
    let b = solid(Kind::Block, 0, 2);
    let approx = b.resolution().approx().mm();
    let f = ids(&b)[0];
    let wall = grow(&b, &[f], &[]).0.faces;
    assert_eq!(wall.len(), 4);
    // Within approx: still the wall of four.
    let near = nudged(&b, f, 0.4 * approx, 0.0);
    let r = grow(&near, &[f], &[]).0;
    assert_eq!(r.faces.len(), 4, "{:?}", r.stopped);
    // A tilt that moves the far corner by 0.3 approx over a quarter-wall box.
    let tilt = nudged(&b, f, 0.0, 0.3 * approx);
    assert_eq!(grow(&tilt, &[f], &[]).0.faces.len(), 4);
    // Past approx: the face is alone; its two inner edges say how far apart (a pure offset
    // is its own bound).
    let far = nudged(&b, f, 1.6 * approx, 0.0);
    let (r, _) = grow(&far, &[f], &[]);
    assert_eq!(r.faces, vec![f]);
    let apart: Vec<f64> = r
        .stopped
        .iter()
        .filter_map(|s| match s.why {
            NotCrossed::Surface(NotOneSurface::Apart(l)) => Some(l.mm()),
            _ => None,
        })
        .collect();
    assert_eq!(apart.len(), 2, "{:?}", r.stopped);
    for a in apart {
        assert!(a >= 1.6 * approx && a < 1.6 * approx * 1.01, "{a}");
    }
}

/// Faces on both sides of an edge of three fins (stacked blocks share a wall) are not one
/// surface across it: the edge is `Fins(3)`.
#[test]
fn an_edge_of_three_faces_is_not_crossed() {
    let b = solid(Kind::Stack, 0, 1);
    let mut seen = 0;
    for f in ids(&b) {
        let (r, _) = grow(&b, &[f], &[]);
        for s in &r.stopped {
            if let NotCrossed::Fins(k) = s.why {
                assert_eq!(k, 3);
                assert_eq!(s.to, None);
                seen += 1;
            }
        }
    }
    assert!(seen > 0);
}

#[test]
fn a_seed_the_body_does_not_hold_is_refused() {
    let b = solid(Kind::Block, 0, 1);
    let gone = FaceId::from_raw(999);
    let err =
        faces_on_one_surface(&b, &[gone], &[], &mut Budget::unlimited(), &Stop::new()).unwrap_err();
    assert_eq!(
        err,
        BodyqError::Refused(BodyqRefusal::NoEntity {
            entity: Entity::Face(gone)
        })
    );
}

#[test]
fn a_small_budget_is_exhausted_and_leaves_nothing() {
    let b = solid(Kind::Block, 0, 3);
    let f = ids(&b)[0];
    let err = faces_on_one_surface(&b, &[f], &[], &mut Budget::new(5), &Stop::new()).unwrap_err();
    assert!(matches!(err, BodyqError::Exhausted { .. }));
}
