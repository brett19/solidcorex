// Written for solidcorex. Apache-2.0.
//! Planar faces (research 72 TS2): MK1 blocks and MK3 extrusions with polygonal holes give
//! closed meshes, each directed side once and its reverse once, wound outward; each cap of
//! an extrusion with circular holes covers its boundary polylines exactly; each face's
//! triangles count `V - 2 + 2h` with no point added and sum to the face's area from
//! `scx_mass::face_area`; non-planar faces, unknown ids, budget and stop are refused.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use std::collections::HashMap;

use scx_brep::op::Scratch;
use scx_brep::{Brep, Document, FaceId};
use scx_make::{Span, block, cylinder, extrude};
use scx_mass::{RelAccuracy, face_area};
use scx_num::budget::{Budget, Stop};
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_planar::{Region, Segment};
use scx_surface::SurfaceKind;
use scx_tess::{
    Mesh, TessError, TessRefusal, TessSpec, TessUnsupported, tessellate, tessellate_faces,
};

fn mm(v: f64) -> Length {
    Length::from_mm(v)
}

fn at() -> Frame {
    Frame::new(
        Point3::ORIGIN,
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap()
}

/// Far from the origin, its axes along no coordinate direction.
fn tilted() -> Frame {
    Frame::new(
        Point3::new(1250.0, -2125.0, 600.0),
        Vec3::new(1.0, 2.0, 3.0),
        Vec3::new(-3.0, 0.5, 1.0),
    )
    .unwrap()
}

fn spec(chord: f64) -> TessSpec {
    TessSpec::new(mm(chord), Resolution::DEFAULT).unwrap()
}

fn mesh_of(body: &Brep, spec: &TessSpec) -> Mesh {
    tessellate(body, spec, &mut Budget::unlimited(), &Stop::new()).unwrap()
}

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn polygon(pts: &[Point2]) -> Vec<Segment> {
    (0..pts.len())
        .map(|k| Segment::line(pts[k], pts[(k + 1) % pts.len()]))
        .collect()
}

fn square(c: Point2, h: f64) -> Vec<Point2> {
    vec![
        p(c.x - h, c.y - h),
        p(c.x + h, c.y - h),
        p(c.x + h, c.y + h),
        p(c.x - h, c.y + h),
    ]
}

/// A 20 mm square with a square, a triangular and an L-shaped hole, 5 mm deep.
fn holed(frame: &Frame) -> Brep {
    let outer = polygon(&square(p(0.0, 0.0), 10.0));
    let holes = vec![
        polygon(&square(p(-5.0, -5.0), 2.0)),
        polygon(&[p(3.0, -7.0), p(8.0, -7.0), p(5.0, -2.0)]),
        polygon(&[
            p(-7.0, 2.0),
            p(7.0, 2.0),
            p(7.0, 4.0),
            p(-5.0, 4.0),
            p(-5.0, 8.0),
            p(-7.0, 8.0),
        ]),
    ];
    let d = Document::default();
    let r = Region::new(outer, holes, d.resolution()).unwrap();
    extrude(&r, frame, Span::new(mm(0.0), mm(5.0)), &d)
        .unwrap()
        .body
}

/// Each directed triangle side once and its reverse once.
fn assert_closed(mesh: &Mesh, name: &str) {
    let mut sides: HashMap<(u32, u32), usize> = HashMap::new();
    for t in &mesh.triangles {
        for k in 0..3 {
            *sides.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
        }
    }
    for (&(a, b), &n) in &sides {
        assert_eq!(n, 1, "{name}: side {a} {b} {n} times");
        assert_eq!(sides.get(&(b, a)), Some(&1), "{name}: side {a} {b} open");
    }
}

fn corner(mesh: &Mesh, i: u32) -> Point3 {
    mesh.positions[i as usize]
}

/// The volume the triangles enclose, by the divergence theorem: positive when wound
/// outward. About the first point, so a far frame loses nothing to cancellation.
fn enclosed(mesh: &Mesh) -> f64 {
    let o = mesh.positions[0];
    mesh.triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| corner(mesh, i) - o);
            a.dot(b.cross(c)) / 6.0
        })
        .sum()
}

fn tri_area(mesh: &Mesh, t: [u32; 3]) -> f64 {
    let [a, b, c] = t.map(|i| corner(mesh, i));
    (b - a).cross(c - a).length() / 2.0
}

/// Each triangle's normals match its winding, and each face's triangles count
/// `V - 2 + 2h` from its loops' polylines.
fn assert_faces(body: &Brep, mesh: &Mesh, name: &str) {
    for (k, t) in mesh.triangles.iter().enumerate() {
        let [a, b, c] = t.map(|i| corner(mesh, i));
        let w = (b - a).cross(c - a);
        for n in mesh.normals[k] {
            assert!(w.dot(n) > 0.0, "{name}: triangle {k} against its normal");
        }
    }
    let lines: HashMap<_, _> = mesh.edges.iter().map(|l| (l.edge, l)).collect();
    for (face, _) in &mesh.chord_bound {
        let f = body.face(*face).unwrap();
        let v: usize = f
            .uses()
            .map(|u| lines[&u.edge().id()].points.len() - 1)
            .sum();
        let h = f.loops().len() - 1;
        let count = mesh.face_of.iter().filter(|&&g| g == *face).count();
        assert_eq!(count, v - 2 + 2 * h, "{name} {face}");
    }
}

/// Each face's triangles sum to `face_area` within the chord's sweep of its boundary.
fn assert_areas(body: &Brep, mesh: &Mesh, name: &str) {
    let lines: HashMap<_, _> = mesh.edges.iter().map(|l| (l.edge, l)).collect();
    for &(face, chord) in &mesh.chord_bound {
        let f = body.face(face).unwrap();
        let truth = face_area(
            f,
            RelAccuracy::new(1e-12).unwrap(),
            &mut Scratch::new(),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap();
        let sum: f64 = mesh
            .triangles
            .iter()
            .zip(&mesh.face_of)
            .filter(|&(_, &g)| g == face)
            .map(|(&t, _)| tri_area(mesh, t))
            .sum();
        let perimeter: f64 = f
            .uses()
            .flat_map(|u| {
                let pts = &lines[&u.edge().id()].points;
                pts.windows(2)
                    .map(|w| (corner(mesh, w[1]) - corner(mesh, w[0])).length())
                    .collect::<Vec<_>>()
            })
            .sum();
        let room = perimeter * chord.mm() + 1e-9 * truth.value + truth.error;
        assert!(
            (sum - truth.value).abs() <= room,
            "{name} {face}: {sum} against {} (room {room})",
            truth.value
        );
    }
}

#[test]
fn blocks_mesh_closed_with_two_triangles_a_face() {
    let d = Document::default();
    for (name, f) in [("at", at()), ("tilted", tilted())] {
        let body = block(&f, [mm(10.0), mm(20.0), mm(30.0)], &d).unwrap().body;
        let mesh = mesh_of(&body, &spec(0.01));
        assert_eq!(mesh.triangles.len(), 12, "{name}");
        assert_eq!(mesh.positions.len(), 8, "{name}");
        assert_closed(&mesh, name);
        assert_faces(&body, &mesh, name);
        assert_areas(&body, &mesh, name);
        assert!(
            (enclosed(&mesh) - 6000.0).abs() < 1e-8,
            "{name}: {}",
            enclosed(&mesh)
        );
        assert!(mesh.chord_bound.iter().all(|&(_, c)| c == Length::ZERO));
    }
}

#[test]
fn extrusions_with_polygonal_holes_mesh_closed() {
    let holes = 2.0 * 2.0 * 4.0 + 0.5 * 5.0 * 5.0 + (14.0 * 2.0 + 2.0 * 4.0);
    for (name, f) in [("at", at()), ("tilted", tilted())] {
        let body = holed(&f);
        let mesh = mesh_of(&body, &spec(0.01));
        assert_eq!(mesh.chord_bound.len(), body.faces().len(), "{name}");
        assert_closed(&mesh, name);
        assert_faces(&body, &mesh, name);
        assert_areas(&body, &mesh, name);
        let v = 5.0 * (400.0 - holes);
        assert!(
            (enclosed(&mesh) - v).abs() < 1e-6 * v,
            "{name}: {}",
            enclosed(&mesh)
        );
    }
}

#[test]
fn the_mesh_is_a_function_of_the_body() {
    let body = holed(&tilted());
    let s = spec(0.01);
    assert_eq!(mesh_of(&body, &s), mesh_of(&body, &s));
    let faces: Vec<FaceId> = body.faces().map(scx_brep::Face::id).collect();
    let mut shuffled: Vec<FaceId> = faces.iter().rev().copied().collect();
    shuffled.extend_from_slice(&faces);
    let again =
        tessellate_faces(&body, &s, &shuffled, &mut Budget::unlimited(), &Stop::new()).unwrap();
    assert_eq!(again, mesh_of(&body, &s));
}

/// A 30 mm square with two circular holes and a slot of two lines and two arcs, 4 mm deep.
fn round_holes(frame: &Frame) -> (Brep, [FaceId; 2]) {
    let outer = polygon(&square(p(0.0, 0.0), 15.0));
    let holes = vec![
        vec![Segment::circle(p(-7.0, -7.0), 3.0, Sense::Reversed)],
        vec![Segment::circle(p(7.0, -7.0), 0.5, Sense::Forward)],
        vec![
            Segment::line(p(-6.0, 5.0), p(6.0, 5.0)),
            Segment::arc(p(6.0, 7.0), p(6.0, 5.0), p(6.0, 9.0), Sense::Forward),
            Segment::line(p(6.0, 9.0), p(-6.0, 9.0)),
            Segment::arc(p(-6.0, 7.0), p(-6.0, 9.0), p(-6.0, 5.0), Sense::Forward),
        ],
    ];
    let d = Document::default();
    let r = Region::new(outer, holes, d.resolution()).unwrap();
    let m = extrude(&r, frame, Span::new(mm(-1.0), mm(3.0)), &d).unwrap();
    (m.body, [m.faces.start, m.faces.end])
}

#[test]
fn caps_with_circular_holes_cover_their_boundary_exactly() {
    for (name, f) in [("at", at()), ("tilted", tilted())] {
        let (body, caps) = round_holes(&f);
        // The walls of the circles and arcs are cylinders: not before TS3.
        let e = tessellate(&body, &spec(0.01), &mut Budget::unlimited(), &Stop::new());
        assert!(
            matches!(
                e,
                Err(TessError::Unsupported(TessUnsupported::FaceKind {
                    kind: SurfaceKind::Cylinder,
                    ..
                }))
            ),
            "{name}: {e:?}"
        );
        for chord in [0.1, 0.01, 0.001] {
            let s = spec(chord);
            let mesh =
                tessellate_faces(&body, &s, &caps, &mut Budget::unlimited(), &Stop::new()).unwrap();
            assert_faces(&body, &mesh, name);
            assert_areas(&body, &mesh, name);
            // Within a cap, each boundary polyline segment is one triangle side the loop's
            // way, and each other side is used once each way.
            for cap in caps {
                let mut sides: HashMap<(u32, u32), i32> = HashMap::new();
                for (t, _) in mesh
                    .triangles
                    .iter()
                    .zip(&mesh.face_of)
                    .filter(|(_, g)| **g == cap)
                {
                    for k in 0..3 {
                        *sides.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
                    }
                }
                let lines: HashMap<_, _> = mesh.edges.iter().map(|l| (l.edge, l)).collect();
                let mut boundary = 0;
                for u in body.face(cap).unwrap().uses() {
                    let pts = &lines[&u.edge().id()].points;
                    for w in pts.windows(2) {
                        let (a, b) = (w[0], w[1]);
                        let one = sides.get(&(a, b)).copied().unwrap_or(0)
                            + sides.get(&(b, a)).copied().unwrap_or(0);
                        assert_eq!(one, 1, "{name} {cap}: boundary side {a} {b}");
                        boundary += 1;
                    }
                }
                let inner = sides
                    .keys()
                    .filter(|&&(a, b)| sides.contains_key(&(b, a)))
                    .count();
                assert_eq!(
                    inner + boundary,
                    sides.len(),
                    "{name} {cap}: an open inner side"
                );
            }
        }
    }
}

#[test]
fn faces_that_are_not_planes_are_unsupported_and_unknown_ids_refused() {
    let d = Document::default();
    let body = cylinder(&at(), mm(5.0), mm(10.0), &d).unwrap().body;
    let e = tessellate(&body, &spec(0.01), &mut Budget::unlimited(), &Stop::new()).unwrap_err();
    assert!(matches!(
        e,
        TessError::Unsupported(TessUnsupported::FaceKind {
            kind: SurfaceKind::Cylinder,
            ..
        })
    ));
    assert_eq!(e.name(), "face_kind");
    // A face id the block does not have: the holed extrusion's last.
    let block = block(&at(), [mm(1.0); 3], &d).unwrap().body;
    let far = holed(&at()).faces().last().unwrap().id();
    let e = tessellate_faces(
        &block,
        &spec(0.01),
        &[far],
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap_err();
    assert_eq!(e, TessError::Refused(TessRefusal::NoFace { face: far }));
    assert_eq!(e.name(), "no_face");
}

#[test]
fn budget_and_stop_end_the_work() {
    let body = holed(&at());
    let s = spec(0.01);
    let mut b = Budget::unlimited();
    tessellate(&body, &s, &mut b, &Stop::new()).unwrap();
    let used = b.used();
    let e = tessellate(&body, &s, &mut Budget::new(used - 1), &Stop::new()).unwrap_err();
    assert!(matches!(e, TessError::Exhausted { .. }));
    assert!(tessellate(&body, &s, &mut Budget::new(used), &Stop::new()).is_ok());
    let stop = Stop::new();
    stop.set();
    assert_eq!(
        tessellate(&body, &s, &mut Budget::unlimited(), &stop),
        Err(TessError::Cancelled)
    );
}
