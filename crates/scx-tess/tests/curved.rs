// Written for solidcorex. Apache-2.0.
//! Curved faces (research 72 TS3): every MK2 primitive (cylinder, frustum, apex cone either
//! way up, sphere, torus) and an MK3 extrusion with circular holes give meshes that the T13
//! oracle (`scx_testkit::mesh`) passes, closed, manifold, outward and unfolded; every vertex
//! on its surface; every triangle's sampled points within its face's stated chord bound,
//! measured against the closed-form surface; each face's stated bound within the spec's
//! chord; each enclosed volume within chord times area of the closed form; each edge
//! polyline segment a triangle side on both its faces (the crack statement); the same body
//! twice the same mesh; budget and stop end the work.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use std::collections::HashMap;
use std::f64::consts::PI;

use scx_brep::{Brep, Document, FaceId};
use scx_num::budget::{Budget, Stop};
use scx_num::fmath::tan;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3, sqrt};
use scx_planar::{Region, Segment};
use scx_surface::Surface;
use scx_tess::{Mesh, TessError, TessSpec, tessellate, tessellate_faces};
use scx_testkit::bodies::made::{Span, cone, cylinder, extrude, sphere, torus};
use scx_testkit::mesh as oracle;

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

/// A body, its closed-form volume and area.
struct Case {
    name: String,
    body: Brep,
    volume: f64,
    area: f64,
}

/// Every MK2 primitive on `frame`.
fn primitives(frame: &Frame, tag: &str) -> Vec<Case> {
    let d = Document::default();
    let (r, h) = (5.0, 10.0);
    let frustum = |r0: f64, r1: f64| {
        let s = sqrt(h * h + (r1 - r0) * (r1 - r0));
        (
            PI * h / 3.0 * (r0 * r0 + r0 * r1 + r1 * r1),
            PI * (r0 + r1) * s + PI * (r0 * r0 + r1 * r1),
        )
    };
    let mut out = Vec::new();
    let mut add = |name: &str, body: Brep, (volume, area): (f64, f64)| {
        out.push(Case {
            name: format!("{tag} {name}"),
            body,
            volume,
            area,
        });
    };
    add(
        "cylinder",
        cylinder(frame, mm(r), mm(h), &d).unwrap().body,
        (PI * r * r * h, 2.0 * PI * r * h + 2.0 * PI * r * r),
    );
    add(
        "frustum",
        cone(frame, [mm(4.0), mm(1.5)], mm(h), &d).unwrap().body,
        frustum(4.0, 1.5),
    );
    add(
        "apex up",
        cone(frame, [mm(4.0), mm(0.0)], mm(h), &d).unwrap().body,
        frustum(4.0, 0.0),
    );
    add(
        "apex down",
        cone(frame, [mm(0.0), mm(3.0)], mm(h), &d).unwrap().body,
        frustum(0.0, 3.0),
    );
    add(
        "sphere",
        sphere(frame, mm(7.0), &d).unwrap().body,
        (4.0 / 3.0 * PI * 343.0, 4.0 * PI * 49.0),
    );
    let (big, small) = (10.0, 3.0);
    add(
        "torus",
        torus(frame, mm(big), mm(small), &d).unwrap().body,
        (
            2.0 * PI * PI * big * small * small,
            4.0 * PI * PI * big * small,
        ),
    );
    // The horn torus (MK4b): the tube meets the axis at a pole.
    let horn = 6.0;
    add(
        "horn torus",
        torus(frame, mm(horn), mm(horn), &d).unwrap().body,
        (
            2.0 * PI * PI * horn * horn * horn,
            4.0 * PI * PI * horn * horn,
        ),
    );
    out
}

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn square(h: f64) -> Vec<Segment> {
    let pts = [p(-h, -h), p(h, -h), p(h, h), p(-h, h)];
    (0..4)
        .map(|k| Segment::line(pts[k], pts[(k + 1) % 4]))
        .collect()
}

/// A 30 mm square with two circular holes and a slot of two lines and two arcs, 4 mm deep
/// (face.rs's `round_holes`), with its closed-form volume and area.
fn round_holes(frame: &Frame, tag: &str) -> Case {
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
    let r = Region::new(square(15.0), holes, d.resolution()).unwrap();
    let body = extrude(&r, frame, Span::new(mm(-1.0), mm(3.0)), &d)
        .unwrap()
        .body;
    let (h, depth) = (4.0, 4.0);
    let cut = PI * 9.0 + PI * 0.25 + (12.0 * 4.0 + PI * 4.0);
    let rims = 2.0 * PI * 3.0 + 2.0 * PI * 0.5 + (24.0 + 2.0 * PI * 2.0) + 120.0;
    Case {
        name: format!("{tag} round holes"),
        body,
        volume: (900.0 - cut) * h,
        area: 2.0 * (900.0 - cut) + rims * depth,
    }
}

fn adapt(mesh: &Mesh) -> oracle::Mesh {
    oracle::Mesh {
        vertices: mesh.positions.iter().map(|q| [q.x, q.y, q.z]).collect(),
        triangles: mesh.triangles.clone(),
    }
}

/// The distance from `q` to the face's surface, in closed form in its own frame.
fn distance(surface: &Surface, q: Point3) -> f64 {
    let local = |f: &Frame| {
        let d = q - f.origin();
        (
            d.dot(f.x().as_vec()),
            d.dot(f.y().as_vec()),
            d.dot(f.z().as_vec()),
        )
    };
    match surface {
        Surface::Plane(s) => local(s.frame()).2.abs(),
        Surface::Cylinder(s) => {
            let (x, y, _) = local(s.frame());
            (sqrt(x * x + y * y) - s.radius().mm()).abs()
        }
        Surface::Cone(s) => {
            let (x, y, z) = local(s.frame());
            let k = tan(s.half_angle());
            // The meridian line `rho = r + k z`; near the surface, its distance.
            (sqrt(x * x + y * y) - s.radius().mm() - k * z).abs() / sqrt(1.0 + k * k)
        }
        Surface::Sphere(s) => {
            let (x, y, z) = local(s.frame());
            (sqrt(x * x + y * y + z * z) - s.radius().mm()).abs()
        }
        Surface::Torus(s) => {
            let (x, y, z) = local(s.frame());
            let w = sqrt(x * x + y * y) - s.major().mm();
            (sqrt(w * w + z * z) - s.minor().mm()).abs()
        }
        _ => panic!("no closed form"),
    }
}

/// Points of triangle `abc` at fixed barycentric weights: its centroid, side midpoints and
/// the centroids of its corner sub-triangles.
fn samples(a: Point3, b: Point3, c: Point3) -> Vec<Point3> {
    let w = [
        [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0],
        [0.5, 0.5, 0.0],
        [0.0, 0.5, 0.5],
        [0.5, 0.0, 0.5],
        [2.0 / 3.0, 1.0 / 6.0, 1.0 / 6.0],
        [1.0 / 6.0, 2.0 / 3.0, 1.0 / 6.0],
        [1.0 / 6.0, 1.0 / 6.0, 2.0 / 3.0],
    ];
    w.iter()
        .map(|[x, y, z]| {
            Point3::new(
                x * a.x + y * b.x + z * c.x,
                x * a.y + y * b.y + z * c.y,
                x * a.z + y * b.z + z * c.z,
            )
        })
        .collect()
}

/// Every check of the module doc, on one body at one chord.
fn check(case: &Case, chord: f64) {
    let name = format!("{} at {chord}", case.name);
    let s = spec(chord);
    let mesh = mesh_of(&case.body, &s);
    let m = adapt(&mesh);
    let mut v = oracle::violations(&m, oracle::Limits::default());
    // A horn torus's pole is a pinch: two lobes meet at one vertex, the one violation it has.
    if case.name.ends_with("horn torus") {
        assert!(
            matches!(v[..], [oracle::Violation::NonManifoldVertex { .. }]),
            "{name}: {v:?}"
        );
        v.clear();
    }
    assert!(v.is_empty(), "{name}: {v:?}");
    let gap = oracle::volume_gap(&m, case.volume, oracle::chord_bound(chord, case.area));
    assert!(gap.holds(), "{name}: {gap:?}");
    assert_eq!(mesh.triangles.len(), mesh.face_of.len());
    assert_eq!(mesh.triangles.len(), mesh.normals.len());

    let bound: HashMap<FaceId, f64> = mesh.chord_bound.iter().map(|&(f, b)| (f, b.mm())).collect();
    for face in case.body.faces() {
        let b = bound[&face.id()];
        assert!(b <= chord * (1.0 + 1e-9), "{name} {}: bound {b}", face.id());
    }
    // A vertex is on its surface to the frame's rounding; a triangle's points within its
    // face's bound, to the same.
    let scale = 1e-9 * (1.0 + Point3::ORIGIN.distance(mesh.positions[0]));
    for ((t, f), normals) in mesh.triangles.iter().zip(&mesh.face_of).zip(&mesh.normals) {
        let face = case.body.face(*f).unwrap();
        let surface = face.surface();
        let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
        for q in [a, b, c] {
            let d = distance(surface, q);
            assert!(d <= scale, "{name} {f}: vertex {d} off");
        }
        let limit = bound[f] + scale;
        for q in samples(a, b, c) {
            let d = distance(surface, q);
            assert!(d <= limit, "{name} {f}: {d} past {limit}");
        }
        // The triangle faces the way its corner normals do.
        let n = (b - a).cross(c - a);
        for nk in normals {
            assert!(
                n.dot(*nk) > 0.0,
                "{name} {f}: a triangle against its normals {:?} {n:?} {normals:?}",
                [a, b, c]
            );
        }
    }
    // The crack statement: each polyline segment is a triangle side on every face of its
    // edge, once, so both faces meet on it.
    let mut sides: HashMap<(u32, u32), Vec<FaceId>> = HashMap::new();
    for (t, f) in mesh.triangles.iter().zip(&mesh.face_of) {
        for k in 0..3 {
            let (x, y) = (t[k], t[(k + 1) % 3]);
            sides.entry((x.min(y), x.max(y))).or_default().push(*f);
        }
    }
    for line in &mesh.edges {
        let edge = case.body.edge(line.edge).unwrap();
        let mut faces: Vec<FaceId> = edge.uses().map(|u| u.face().id()).collect();
        faces.sort_unstable();
        for w in line.points.windows(2) {
            if w[0] == w[1] {
                continue;
            }
            let mut on = sides
                .get(&(w[0].min(w[1]), w[0].max(w[1])))
                .cloned()
                .unwrap_or_default();
            on.sort_unstable();
            assert_eq!(on, faces, "{name}: segment {w:?} of {}", line.edge);
        }
    }
}

#[test]
fn every_primitive_is_closed_on_its_surfaces_and_within_the_chord() {
    for (tag, f) in [("at", at()), ("tilted", tilted())] {
        for case in primitives(&f, tag) {
            for chord in [0.1, 0.01, 0.001] {
                check(&case, chord);
            }
        }
    }
}

#[test]
fn an_extrusion_with_circular_holes_is_closed() {
    for (tag, f) in [("at", at()), ("tilted", tilted())] {
        let case = round_holes(&f, tag);
        for chord in [0.1, 0.01, 0.001] {
            check(&case, chord);
        }
    }
}

#[test]
fn coarse_and_bounded_specs_hold_their_bounds() {
    // A chord past the radius, an angle bound and an edge-length bound.
    let d = Document::default();
    let bodies = [
        ("sphere", sphere(&at(), mm(2.0), &d).unwrap().body),
        ("torus", torus(&at(), mm(4.0), mm(1.0), &d).unwrap().body),
        (
            "cylinder",
            cylinder(&at(), mm(1.0), mm(6.0), &d).unwrap().body,
        ),
        (
            "cone",
            cone(&at(), [mm(2.0), mm(0.0)], mm(5.0), &d).unwrap().body,
        ),
    ];
    for (name, body) in &bodies {
        for s in [
            spec(5.0),
            spec(0.05).with_angle(0.2).unwrap(),
            spec(0.05).with_edge_length(mm(0.4)).unwrap(),
        ] {
            let mesh = mesh_of(body, &s);
            let v = oracle::violations(&adapt(&mesh), oracle::Limits::default());
            assert!(v.is_empty(), "{name} {s:?}: {v:?}");
            // The curved faces' sides; a planar face adds no point (TS2), so its sides are
            // its boundary's.
            if let Some(e) = s.edge_length() {
                let curved = mesh.triangles.iter().zip(&mesh.face_of).filter(|(_, f)| {
                    !matches!(body.face(**f).unwrap().surface(), Surface::Plane(_))
                });
                for (t, _) in curved {
                    let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
                    for (x, y) in [(a, b), (b, c), (c, a)] {
                        assert!(
                            x.distance(y) <= e.mm() * (1.0 + 1e-9),
                            "{name}: a long side"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn the_same_body_gives_the_same_mesh() {
    for case in primitives(&tilted(), "tilted") {
        let s = spec(0.01);
        assert_eq!(
            mesh_of(&case.body, &s),
            mesh_of(&case.body, &s),
            "{}",
            case.name
        );
    }
}

#[test]
fn one_curved_face_alone_meshes_on_the_whole_wireframe() {
    let d = Document::default();
    let made = cylinder(&at(), mm(5.0), mm(10.0), &d).unwrap();
    let s = spec(0.01);
    let wall = made.faces.wall;
    let mesh = tessellate_faces(
        &made.body,
        &s,
        &[wall],
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap();
    assert!(!mesh.triangles.is_empty());
    assert!(mesh.face_of.iter().all(|f| *f == wall));
    assert_eq!(mesh.chord_bound.len(), 1);
}

#[test]
fn budget_and_stop_end_the_work() {
    let d = Document::default();
    let body = sphere(&at(), mm(7.0), &d).unwrap().body;
    let s = spec(0.001);
    let mut b = Budget::unlimited();
    tessellate(&body, &s, &mut b, &Stop::new()).unwrap();
    let used = b.used();
    let e = tessellate(&body, &s, &mut Budget::new(used - 1), &Stop::new()).unwrap_err();
    assert!(matches!(e, TessError::Exhausted { .. }), "{e:?}");
    assert!(tessellate(&body, &s, &mut Budget::new(used), &Stop::new()).is_ok());
    let stop = Stop::new();
    stop.set();
    let e = tessellate(&body, &s, &mut Budget::unlimited(), &stop).unwrap_err();
    assert_eq!(e, TessError::Cancelled);
}
