// Written for solidcorex. Apache-2.0.
//! `pick` (ray) judged against closed forms: a block, a cylinder, a sphere, a cone and a torus
//! at known depths, rays that pass within an aperture of a surface without crossing it, ties
//! ordered by id, edges and vertices, and the refusals.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod along_common;
mod common;

use scx_bodyq::{BodyqError, BodyqRefusal, BodyqUnsupported, RayPicked, pick};
use scx_brep::{Brep, Entity, EntityKind};
use scx_num::budget::{Budget, Stop};
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};

const EPS: f64 = 1e-7;

fn pt(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

fn body(name: &str) -> Brep {
    common::primitives()
        .into_iter()
        .find(|(n, ..)| n == name)
        .unwrap()
        .1
}

fn go(
    b: &Brep,
    o: [f64; 3],
    d: [f64; 3],
    want: EntityKind,
    aperture: f64,
) -> Result<Vec<RayPicked>, BodyqError> {
    pick(
        b,
        pt(o),
        Vec3::new(d[0], d[1], d[2]),
        want,
        Length::from_mm(aperture),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
}

fn faces(b: &Brep, o: [f64; 3], d: [f64; 3], aperture: f64) -> Vec<RayPicked> {
    go(b, o, d, EntityKind::Face, aperture).unwrap()
}

fn depths(h: &[RayPicked]) -> Vec<f64> {
    h.iter().map(|h| h.depth).collect()
}

fn close(got: &[f64], want: &[f64]) {
    assert_eq!(got.len(), want.len(), "{got:?} vs {want:?}");
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() < EPS, "{got:?} vs {want:?}");
    }
}

/// The world block's lower corner and size: the origin and 3 by 2 by 1.5 (the body's box is
/// grown by its accuracy, so it is not read from there).
fn extent(_: &Brep) -> ([f64; 3], [f64; 3]) {
    ([0.0; 3], [3.0, 2.0, 1.5])
}

#[test]
fn a_ray_through_a_block_meets_two_faces_at_their_depths() {
    let b = body("block world");
    let (lo, size) = extent(&b);
    let o = [lo[0] + 1.0, lo[1] + 0.7, lo[2] - 3.0];
    let h = faces(&b, o, [0.0, 0.0, 1.0], 0.0);
    close(&depths(&h), &[3.0, 3.0 + size[2]]);
    assert!(h.iter().all(|h| h.distance == 0.0));
    assert!((h[0].at.z - lo[2]).abs() < EPS && (h[1].at.z - (lo[2] + size[2])).abs() < EPS);
    // The same ray backwards from the far side, and sideways, meet nothing behind them.
    let back = faces(&b, o, [0.0, 0.0, -1.0], 0.0);
    assert!(back.is_empty());
    // A slanted ray: the ray through the block's middle along a body diagonal meets the two
    // faces it leaves through, and the depths are the closed forms.
    let c = [
        lo[0] + size[0] / 2.0,
        lo[1] + size[1] / 2.0,
        lo[2] + size[2] / 2.0,
    ];
    let from = [c[0] - 1.0, c[1] - 0.5, c[2] - 0.25];
    let h = faces(&b, from, [4.0, 2.0, 1.0], 0.0);
    let l = (16.0f64 + 4.0 + 1.0).sqrt();
    // From the start the ray is inside; it leaves at the least t with a coordinate at a face.
    let t_out = [
        (lo[0] + size[0] - from[0]) / (4.0 / l),
        (lo[1] + size[1] - from[1]) / (2.0 / l),
        (lo[2] + size[2] - from[2]) / (1.0 / l),
    ]
    .into_iter()
    .fold(f64::INFINITY, f64::min);
    assert_eq!(h.len(), 1, "{h:?}");
    assert!((h[0].depth - t_out).abs() < EPS);
}

#[test]
fn a_ray_through_a_cylinder_has_two_hits_on_its_wall() {
    let b = body("cylinder world");
    // Along x at y = 0.3, z = 1.0: the wall at x = -/+ sqrt(1.2^2 - 0.3^2).
    let s = (1.44f64 - 0.09).sqrt();
    let h = faces(&b, [-5.0, 0.3, 1.0], [1.0, 0.0, 0.0], 0.0);
    close(&depths(&h), &[5.0 - s, 5.0 + s]);
    assert_eq!(h[0].entity, h[1].entity);
    // Along the axis from below: the two caps, and not the wall (it is 1.2 away).
    let h = faces(&b, [0.0, 0.0, -2.0], [0.0, 0.0, 1.0], 0.0);
    close(&depths(&h), &[2.0, 4.5]);
    assert_ne!(h[0].entity, h[1].entity);
    // The wall within an aperture of 1.2 joins them, at its nearest approach to the ray: the
    // rim of the cap the ray starts under (at the first cap's depth).
    let h = faces(&b, [0.0, 0.0, -2.0], [0.0, 0.0, 1.0], 1.25);
    assert_eq!(h.len(), 3, "{h:?}");
}

#[test]
fn a_ray_through_a_sphere_has_two_hits() {
    let b = body("sphere world");
    let s = (1.69f64 - 0.25 - 0.04).sqrt();
    let h = faces(&b, [-5.0, 0.5, 0.2], [1.0, 0.0, 0.0], 0.0);
    close(&depths(&h), &[5.0 - s, 5.0 + s]);
    // A slanted ray through the centre: hits at 5 -/+ the radius.
    let u = Vec3::new(1.0, 2.0, 2.0).normalized().unwrap().as_vec();
    let h = faces(
        &b,
        [-5.0 * u.x, -5.0 * u.y, -5.0 * u.z],
        [1.0, 2.0, 2.0],
        0.0,
    );
    close(&depths(&h), &[3.7, 6.3]);
}

#[test]
fn a_ray_through_a_torus_has_four_hits() {
    let b = body("torus world");
    // Along x through the ring's plane at the centre: the tube at 2 -/+ 0.6 on both sides.
    let h = faces(&b, [-5.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.0);
    close(&depths(&h), &[2.4, 3.6, 6.4, 7.6]);
    // Higher, at z = 0.3: the tube's section is the chord sqrt(0.36 - 0.09) either side of 2.
    let w = (0.36f64 - 0.09).sqrt();
    let h = faces(&b, [-5.0, 0.0, 0.3], [1.0, 0.0, 0.0], 0.0);
    close(&depths(&h), &[3.0 - w, 3.0 + w, 7.0 - w, 7.0 + w]);
    // Through the hole along the axis: no hit.
    assert!(faces(&b, [0.0, 0.0, -5.0], [0.0, 0.0, 1.0], 0.0).is_empty());
}

#[test]
fn a_grazing_ray_within_the_aperture_is_picked_at_its_nearest_approach() {
    // A cylinder: along x at y = 1.2 + 0.05 misses its wall by 0.05, nearest at x = 0.
    let b = body("cylinder world");
    let miss = [-5.0, 1.25, 1.0];
    assert!(faces(&b, miss, [1.0, 0.0, 0.0], 0.04).is_empty());
    let h = faces(&b, miss, [1.0, 0.0, 0.0], 0.06);
    assert_eq!(h.len(), 1, "{h:?}");
    assert!((h[0].depth - 5.0).abs() < 1e-6 && (h[0].distance - 0.05).abs() < 1e-9);
    assert!((h[0].at.y - 1.2).abs() < 1e-9 && h[0].at.x.abs() < 1e-6);
    // A sphere of radius 1.3: along x at y = 1.35.
    let b = body("sphere world");
    assert!(faces(&b, [-5.0, 1.35, 0.0], [1.0, 0.0, 0.0], 0.04).is_empty());
    let h = faces(&b, [-5.0, 1.35, 0.0], [1.0, 0.0, 0.0], 0.06);
    assert_eq!(h.len(), 1, "{h:?}");
    assert!((h[0].depth - 5.0).abs() < 1e-6 && (h[0].distance - 0.05).abs() < 1e-9);
    // A torus: along x at height 0.65 passes over the tube's top (0.6) at x = -2 and x = 2.
    let b = body("torus world");
    assert!(faces(&b, [-5.0, 0.0, 0.65], [1.0, 0.0, 0.0], 0.04).is_empty());
    let h = faces(&b, [-5.0, 0.0, 0.65], [1.0, 0.0, 0.0], 0.06);
    assert_eq!(h.len(), 2, "{h:?}");
    close(&depths(&h), &[3.0, 7.0]);
    assert!(h.iter().all(|h| (h.distance - 0.05).abs() < 1e-8));
    // A cone (radius 1.5 at the base falling by 0.45 a mm): along x at z = 1, y = 1.1 is
    // 0.05 / sqrt(1 + 0.45^2) from the slant.
    let b = body("cone 1.5 0.6 world");
    let want = 0.05 / (1.0f64 + 0.45 * 0.45).sqrt();
    assert!(faces(&b, [-5.0, 1.1, 1.0], [1.0, 0.0, 0.0], want - 1e-3).is_empty());
    let h = faces(&b, [-5.0, 1.1, 1.0], [1.0, 0.0, 0.0], want + 1e-3);
    assert_eq!(h.len(), 1, "{h:?}");
    assert!((h[0].distance - want).abs() < 1e-8, "{h:?}");
    assert!((h[0].depth - 5.0).abs() < 0.1);
}

#[test]
fn a_ray_just_past_a_rim_is_picked_by_its_edge() {
    let b = body("block world");
    let (lo, size) = extent(&b);
    // Along z, 0.03 outside the block's x = lo face: the ray passes 0.03 from the bottom,
    // the top and the face's own plane edge.
    let o = [lo[0] - 0.03, lo[1] + 0.7, lo[2] - 3.0];
    assert!(faces(&b, o, [0.0, 0.0, 1.0], 0.02).is_empty());
    let h = faces(&b, o, [0.0, 0.0, 1.0], 0.05);
    // The x = lo face, then the bottom and the top, each at 0.03.
    assert_eq!(h.len(), 3, "{h:?}");
    assert!(h.iter().all(|h| (h.distance - 0.03).abs() < 1e-8), "{h:?}");
    assert!((h[1].depth - 3.0).abs() < 1e-6 || (h[0].depth - 3.0).abs() < 1e-6);
    let _ = size;
}

#[test]
fn ties_in_depth_are_ordered_by_id() {
    let b = body("block world");
    let (lo, size) = extent(&b);
    // Along z up the block's x = lo face: it meets the bottom and the top where the face
    // does, and the face's own stretch starts at the bottom's depth.
    let o = [lo[0], lo[1] + 0.7, lo[2] - 3.0];
    let h = faces(&b, o, [0.0, 0.0, 1.0], 0.0);
    assert!(h.len() >= 3, "{h:?}");
    close(&depths(&h)[..2], &[3.0, 3.0]);
    for w in h.windows(2) {
        let (x, y) = ((w[0].depth, w[0].entity), (w[1].depth, w[1].entity));
        assert!(x.0 < y.0 || (x.0 == y.0 && x.1 < y.1), "{h:?}");
    }
    let _ = size;
}

#[test]
fn edges_and_vertices_are_picked_by_their_nearest_approach() {
    let b = body("block world");
    let (lo, size) = extent(&b);
    // Along z over the middle of the bottom's edge y = lo, and the top's: two edges, at the
    // bottom and top depths.
    let o = [lo[0] + 1.0, lo[1], lo[2] - 3.0];
    let h = go(&b, o, [0.0, 0.0, 1.0], EntityKind::Edge, 0.0).unwrap();
    close(&depths(&h), &[3.0, 3.0 + size[2]]);
    assert!(h.iter().all(|h| h.distance < 1e-9));
    // Along z up the corner's vertical edge: its two ends are vertices at the same depths.
    let o = [lo[0], lo[1], lo[2] - 3.0];
    let v = go(&b, o, [0.0, 0.0, 1.0], EntityKind::Vertex, 0.0).unwrap();
    close(&depths(&v), &[3.0, 3.0 + size[2]]);
    // A wider aperture reaches the neighbouring corners, by the distance the box gives.
    let v = go(&b, o, [0.0, 0.0, 1.0], EntityKind::Vertex, 2.5).unwrap();
    let want = size[0].min(size[1]) <= 2.5;
    assert_eq!(v.len() > 2, want);
    for w in v.windows(2) {
        assert!(w[0].depth <= w[1].depth);
    }
}

#[test]
fn a_circular_edge_is_picked_at_the_ray_that_crosses_its_circle() {
    // The cylinder's rims are circles of radius 1.2 about z at z = 0 and 2.5. A ray along y
    // in the plane z = 0 crosses the bottom rim at y = -/+ 1.2 (x = 0 there).
    let b = body("cylinder world");
    let h = go(&b, [0.0, -5.0, 0.0], [0.0, 1.0, 0.0], EntityKind::Edge, 0.0).unwrap();
    let rims: Vec<&RayPicked> = h.iter().collect();
    assert!(rims.len() >= 2, "{h:?}");
    close(&depths(&h)[..2], &[3.8, 6.2]);
    assert!(h.iter().take(2).all(|h| h.distance < 1e-9));
}

#[test]
fn a_nurbs_face_is_refused_where_only_step_5_can_answer() {
    let (sheet, _, patch) = along_common::nurbs_meets_plane();
    // A ray that crosses the patch has its crossing from the surface intersection.
    let h = go(
        &sheet,
        [2.0, 5.0, 1.0],
        [0.0, -1.0, 0.0],
        EntityKind::Face,
        0.0,
    )
    .unwrap();
    assert!(
        h.iter()
            .any(|h| h.entity == Entity::Face(patch) && h.distance == 0.0)
    );
    // A ray that passes beside it, near enough to be within the aperture of its surface, has
    // no crossing and no offset surface to meet: refused, naming the face, never sampled.
    let r = go(
        &sheet,
        [-5.0, -1.0, 1.5],
        [1.0, 0.0, 0.0],
        EntityKind::Face,
        0.4,
    );
    assert_eq!(
        r,
        Err(BodyqError::Unsupported(BodyqUnsupported::Nurbs {
            at: Entity::Face(patch)
        }))
    );
}

#[test]
fn bad_requests_are_refused_and_the_work_is_repeatable() {
    let b = body("block world");
    let ok = |o: [f64; 3], d: [f64; 3], k, a| go(&b, o, d, k, a);
    assert_eq!(
        ok([f64::NAN, 0.0, 0.0], [1.0, 0.0, 0.0], EntityKind::Face, 0.0),
        Err(BodyqError::Refused(BodyqRefusal::NotFinite))
    );
    assert_eq!(
        ok([0.0; 3], [0.0; 3], EntityKind::Face, 0.0),
        Err(BodyqError::Refused(BodyqRefusal::NoDirection))
    );
    for a in [-1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            ok([0.0; 3], [1.0, 0.0, 0.0], EntityKind::Face, a),
            Err(BodyqError::Refused(BodyqRefusal::Tolerance))
        );
    }
    assert_eq!(
        ok([0.0; 3], [1.0, 0.0, 0.0], EntityKind::Region, 1.0),
        Err(BodyqError::Refused(BodyqRefusal::NotPickable {
            kind: EntityKind::Region
        }))
    );
    let run = || {
        let mut w = Budget::unlimited();
        let h = pick(
            &b,
            pt([0.3, 0.2, -2.0]),
            Vec3::new(0.1, 0.05, 1.0),
            EntityKind::Edge,
            Length::from_mm(0.7),
            &mut w,
            &Stop::new(),
        )
        .unwrap();
        (h, w.used())
    };
    assert_eq!(run(), run());
    let r = pick(
        &b,
        pt([0.3, 0.2, -2.0]),
        Vec3::new(0.0, 0.0, 1.0),
        EntityKind::Face,
        Length::from_mm(0.0),
        &mut Budget::new(1),
        &Stop::new(),
    );
    assert!(
        matches!(r, Err(BodyqError::Exhausted { limit: 1 })),
        "{r:?}"
    );
}
