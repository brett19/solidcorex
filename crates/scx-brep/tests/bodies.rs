// Written for solidcorex. Apache-2.0.
//! Two bodies built by hand, a unit block and a closed cylinder, walked through every view
//! and iterator, finished and as drafts.
//!
//! The block has six planes, twelve lines and eight vertices. The cylinder has two planar
//! caps and one cylindrical face whose loop crosses a seam: the seam edge is used twice by
//! one face in opposite senses, and its fins name that face twice.

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use std::collections::BTreeMap;
use std::f64::consts::TAU;

use scx_brep::{
    Brep, Document, Draft, EdgeGeom, EdgeId, EdgeShape, EdgeUse, FaceId, FaceRecord, FaceSide, Fin,
    LoopRecord, RegionId, ShellId, ShellRecord, UseGeom, UvSide, VertexId, VertexRecord,
};
use scx_curve::{Circle, Curve, Curve2, Line, Line2};
use scx_num::extent1d::Extent1d;
use scx_num::extent3d::Extent3d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Cylinder, Plane, Surface, pcurve_of};

fn tol() -> Resolution {
    Resolution::new(Length::from_mm(2e-9)).unwrap()
}

/// The shells of a one-solid body: the outside's, then the solid's.
fn solid_shells(d: &mut Draft) -> (ShellId, ShellId) {
    let outer = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
    let solid = d.add_region(true).unwrap();
    let inner = d.add_shell(ShellRecord::new(solid)).unwrap();
    (outer, inner)
}

fn pcurve(edge: &Curve, interval: Extent1d, surface: &Surface) -> UseGeom {
    UseGeom::Pcurve {
        curve: pcurve_of(edge, surface, interval, tol()).unwrap(),
        interval,
    }
}

/// The unit block `[0, 1]^3`, each face's front outward.
fn block() -> Draft {
    let mut d = Draft::new();
    let (outer, inner) = solid_shells(&mut d);
    let corner = |i: usize| [i & 1, (i >> 1) & 1, (i >> 2) & 1].map(|c| c as f64);
    let vertices: Vec<VertexId> = (0..8)
        .map(|i| {
            let [x, y, z] = corner(i);
            d.add_vertex(VertexRecord::new(Point3::new(x, y, z)))
                .unwrap()
        })
        .collect();
    let mut edges: BTreeMap<(usize, usize), (EdgeId, Curve)> = BTreeMap::new();
    let unit = Extent1d::new(0.0, 1.0).unwrap();
    for axis in 0..3 {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        for side in 0..2 {
            let mut normal = [0.0; 3];
            normal[axis] = if side == 1 { 1.0 } else { -1.0 };
            let mut x = [0.0; 3];
            x[u] = 1.0;
            let mut origin = [0.0; 3];
            origin[axis] = side as f64;
            let frame = Frame::new(
                Point3::new(origin[0], origin[1], origin[2]),
                Vec3::new(normal[0], normal[1], normal[2]),
                Vec3::new(x[0], x[1], x[2]),
            )
            .unwrap();
            let surface = Surface::Plane(Plane::new(frame));
            // Counter-clockwise about +axis in (u, v); reversed for the -axis face.
            let mut ring: Vec<usize> = [(0, 0), (1, 0), (1, 1), (0, 1)]
                .iter()
                .map(|&(a, b)| (side << axis) | (a << u) | (b << v))
                .collect();
            if side == 0 {
                ring.reverse();
            }
            let mut uses = Vec::new();
            for k in 0..4 {
                let (a, b) = (ring[k], ring[(k + 1) % 4]);
                let key = (a.min(b), a.max(b));
                let (edge, curve) = edges
                    .entry(key)
                    .or_insert_with(|| {
                        let [p, q] = [key.0, key.1].map(corner);
                        let line = Line::new(
                            Point3::new(p[0], p[1], p[2]),
                            Vec3::new(q[0] - p[0], q[1] - p[1], q[2] - p[2]),
                        )
                        .unwrap();
                        let curve = Curve::Line(line);
                        let shape = EdgeShape::Curve {
                            curve: curve.clone(),
                            interval: unit,
                        };
                        let ends = [vertices[key.0], vertices[key.1]];
                        (d.add_edge(EdgeGeom::new(shape, ends)).unwrap(), curve)
                    })
                    .clone();
                let sense = if a < b {
                    Sense::Forward
                } else {
                    Sense::Reversed
                };
                uses.push(EdgeUse::new(edge, sense, pcurve(&curve, unit, &surface)));
            }
            d.add_face(FaceRecord::new(
                surface,
                [outer, inner],
                vec![LoopRecord::new(uses)],
            ))
            .unwrap();
        }
    }
    d
}

/// The cylinder of radius 1 about z from `z = 0` to `z = 2`, closed by two caps.
fn cylinder() -> Draft {
    let mut d = Draft::new();
    let (outer, inner) = solid_shells(&mut d);
    let r = Length::from_mm(1.0);
    let bottom = d
        .add_vertex(VertexRecord::new(Point3::new(1.0, 0.0, 0.0)))
        .unwrap();
    let top = d
        .add_vertex(VertexRecord::new(Point3::new(1.0, 0.0, 2.0)))
        .unwrap();
    let round = Extent1d::new(0.0, TAU).unwrap();
    let circle = |z: f64| {
        let frame = Frame::new(Point3::new(0.0, 0.0, z), Vec3::Z, Vec3::X).unwrap();
        Curve::Circle(Circle::new(frame, r, Sense::Forward).unwrap())
    };
    let (c0, c1) = (circle(0.0), circle(2.0));
    let seam = Curve::Line(Line::new(Point3::new(1.0, 0.0, 0.0), Vec3::Z).unwrap());
    let height = Extent1d::new(0.0, 2.0).unwrap();
    let mut edge = |curve: &Curve, interval, ends| {
        let shape = EdgeShape::Curve {
            curve: curve.clone(),
            interval,
        };
        d.add_edge(EdgeGeom::new(shape, ends)).unwrap()
    };
    let e0 = edge(&c0, round, [bottom, bottom]);
    let e1 = edge(&c1, round, [top, top]);
    let e2 = edge(&seam, height, [bottom, top]);

    let side = Surface::Cylinder(Cylinder::new(Frame::WORLD, r, Sense::Forward).unwrap());
    let seam_at = |u: f64| UseGeom::Pcurve {
        curve: Curve2::Line(Line2::new(Point2::new(u, 0.0), Vec2::new(0.0, 1.0)).unwrap()),
        interval: height,
    };
    // Counter-clockwise round the uv rectangle [0, 2 pi] x [0, 2], seen from outside.
    let wall = vec![
        EdgeUse::new(e0, Sense::Forward, pcurve(&c0, round, &side)),
        EdgeUse::new(e2, Sense::Forward, seam_at(TAU)),
        EdgeUse::new(e1, Sense::Reversed, pcurve(&c1, round, &side)),
        EdgeUse::new(e2, Sense::Reversed, seam_at(0.0)),
    ];
    d.add_face(FaceRecord::new(
        side,
        [outer, inner],
        vec![LoopRecord::new(wall)],
    ))
    .unwrap();
    for (z, normal, circle, e, sense) in [
        (0.0, -1.0, &c0, e0, Sense::Reversed),
        (2.0, 1.0, &c1, e1, Sense::Forward),
    ] {
        let frame = Frame::new(
            Point3::new(0.0, 0.0, z),
            Vec3::new(0.0, 0.0, normal),
            Vec3::X,
        )
        .unwrap();
        let cap = Surface::Plane(Plane::new(frame));
        let lp = LoopRecord::new(vec![EdgeUse::new(e, sense, pcurve(circle, round, &cap))]);
        d.add_face(FaceRecord::new(cap, [outer, inner], vec![lp]))
            .unwrap();
    }
    d
}

/// What a walk of a body found, in a form a draft's walk and a finished body's walk can
/// be compared by.
#[derive(Debug, PartialEq)]
struct Walk {
    counts: [usize; 5],
    /// Per face: its uses as (edge, sense, start, end).
    loops: Vec<Vec<(EdgeId, Sense, VertexId, VertexId)>>,
    /// Per edge: its fins.
    fins: Vec<Vec<Fin>>,
    /// Per vertex: its edges.
    stars: Vec<Vec<EdgeId>>,
    /// Per shell: its face sides.
    sides: Vec<Vec<(FaceId, FaceSide)>>,
    /// Per region: its shells.
    shells: Vec<Vec<ShellId>>,
}

/// Walks every view and iterator of a finished body, checking what holds of any closed
/// solid of one shell pair.
fn walk(body: &Brep) -> Walk {
    let counts = [
        body.regions().len(),
        body.shells().len(),
        body.faces().len(),
        body.edges().len(),
        body.vertices().len(),
    ];
    // Ids come in order, both ways.
    let ids: Vec<_> = body.faces().map(scx_brep::Face::id).collect();
    let mut back: Vec<_> = body.faces().rev().map(scx_brep::Face::id).collect();
    back.reverse();
    assert_eq!(ids, back);
    assert!(ids.windows(2).all(|w| w[0] < w[1]));

    // Regions and shells: the outside's shell sees every face's front, the solid's every back.
    let outside = body.outside();
    assert!(outside.is_outside() && !outside.is_solid());
    let solid = body.regions().find(|r| r.is_solid()).unwrap();
    let outer = outside.shells().next().unwrap();
    let inner = solid.shells().next().unwrap();
    assert_eq!(outside.shells().count(), 1);
    assert_eq!(solid.shells().count(), 1);
    assert_eq!(outer.region().id(), RegionId::OUTSIDE);
    assert_eq!(inner.region().id(), solid.id());
    assert!(outer.faces().all(|(_, s)| s == FaceSide::Front));
    assert!(inner.faces().all(|(_, s)| s == FaceSide::Back));
    assert_eq!(outer.faces().count(), counts[2]);
    assert_eq!(inner.faces().count(), counts[2]);
    assert_eq!(outer.wires().len() + outer.lone_vertices().len(), 0);

    let mut loops = Vec::new();
    for face in body.faces() {
        assert_eq!(face.shell(FaceSide::Front).id(), outer.id());
        assert_eq!(face.shell(FaceSide::Back).id(), inner.id());
        assert_eq!(body.face(face.id()).unwrap().id(), face.id());
        assert!(matches!(
            face.surface(),
            Surface::Plane(_) | Surface::Cylinder(_)
        ));
        let mut found = Vec::new();
        for lp in face.loops() {
            assert_eq!(lp.face().id(), face.id());
            assert!(!lp.is_empty());
            assert_eq!(lp.uses().len(), lp.len());
            for u in lp.uses() {
                // The loop closes: each use ends where the next starts.
                assert_eq!(u.end().id(), u.next().start().id());
                assert_eq!(u.next().prev().position(), u.position());
                assert_eq!(u.lp().index(), lp.index());
                assert_eq!(u.face().id(), face.id());
                assert!(u.bound().is_none());
                let edge = u.edge();
                assert!(edge.fins().any(|f| f == u.fin()));
                // The pcurve runs over the edge's own interval.
                let (UseGeom::Pcurve { interval, .. }, EdgeShape::Curve { interval: own, .. }) =
                    (u.geom(), edge.shape())
                else {
                    panic!("a use without a pcurve on {}", edge.id());
                };
                assert_eq!(interval, own);
                found.push((edge.id(), u.sense(), u.start().id(), u.end().id()));
            }
        }
        assert_eq!(face.uses().count(), found.len());
        loops.push(found);
    }

    let mut fins = Vec::new();
    for edge in body.edges() {
        assert!(!edge.is_wire());
        assert!(edge.accuracy().is_none());
        assert_eq!([edge.start().id(), edge.end().id()], edge.ends());
        assert_eq!(edge.fins().len(), 2, "a manifold edge has two fins");
        for (fin, u) in edge.fins().zip(edge.uses()) {
            assert_eq!(u.face().id(), fin.face);
            assert_eq!(u.sense(), fin.sense);
            assert_eq!(u.edge().id(), edge.id());
        }
        // The two passes run opposite ways.
        let senses: Vec<_> = edge.fins().map(|f| f.sense).collect();
        assert_eq!(senses[0], senses[1].reversed());
        fins.push(edge.fins().collect());
    }

    let mut stars = Vec::new();
    for vertex in body.vertices() {
        assert!(vertex.accuracy().is_none());
        let star: Vec<_> = vertex.edges().map(scx_brep::Edge::id).collect();
        assert!(
            star.iter()
                .all(|&e| body.edge(e).unwrap().ends().contains(&vertex.id()))
        );
        assert_eq!(body.vertex(vertex.id()).unwrap().point(), vertex.point());
        stars.push(star);
    }

    // Euler-Poincare for one closed shell of genus 0 with one loop per face.
    let [_, _, f, e, v] = counts;
    assert_eq!(v + f, e + 2);
    Walk {
        counts,
        loops,
        fins,
        stars,
        sides: body
            .shells()
            .map(|s| s.faces().map(|(f, side)| (f.id(), side)).collect())
            .collect(),
        shells: body
            .regions()
            .map(|r| r.shells().map(scx_brep::Shell::id).collect())
            .collect(),
    }
}

/// The same walk over a draft's views: every reference resolves, so every option is `Some`.
fn walk_draft(d: &Draft) -> Walk {
    let counts = [
        d.regions().len(),
        d.shells().len(),
        d.faces().len(),
        d.edges().len(),
        d.vertices().len(),
    ];
    let loops = d
        .faces()
        .map(|face| {
            face.uses()
                .map(|u| {
                    let edge = u.edge().unwrap();
                    assert!(edge.shape().is_some());
                    (
                        edge.id(),
                        u.sense(),
                        u.start().unwrap().id(),
                        u.end().unwrap().id(),
                    )
                })
                .collect()
        })
        .collect();
    let fins = d
        .edges()
        .map(|edge| {
            assert!(edge.uses().all(|u| u.is_some()));
            edge.fins().collect()
        })
        .collect();
    let stars = d
        .vertices()
        .map(|v| v.edges().map(scx_brep::Edge::id).collect())
        .collect();
    Walk {
        counts,
        loops,
        fins,
        stars,
        sides: d
            .shells()
            .map(|s| s.faces().map(|(f, side)| (f.id(), side)).collect())
            .collect(),
        shells: d
            .regions()
            .map(|r| r.shells().map(scx_brep::Shell::id).collect())
            .collect(),
    }
}

fn check_body(d: Draft) -> (Brep, Walk) {
    let drafted = walk_draft(&d);
    let (body, _) = d.finish(&Document::default()).unwrap();
    let walked = walk(&body);
    assert_eq!(walked, drafted);
    // An edit that writes nothing finishes to the same walk, and compaction keeps every id.
    let (again, _) = body.edit().finish(&Document::default()).unwrap();
    assert_eq!(walk(&again), walked);
    let (compact, map) = body.compact();
    assert!(map.is_identity());
    assert_eq!(walk(&compact), walked);
    (body, walked)
}

#[test]
fn block_walks() {
    let (body, w) = check_body(block());
    assert_eq!(w.counts, [2, 2, 6, 12, 8]);
    assert!(w.loops.iter().all(|l| l.len() == 4));
    assert!(w.stars.iter().all(|s| s.len() == 3));
    // Every fin pair names two different faces.
    assert!(w.fins.iter().all(|f| f[0].face != f[1].face));
    // A body crosses threads.
    let shared = body.clone();
    let faces = std::thread::spawn(move || shared.faces().len())
        .join()
        .unwrap();
    assert_eq!(faces, 6);
}

#[test]
fn cylinder_walks_its_seam() {
    let (body, w) = check_body(cylinder());
    assert_eq!(w.counts, [2, 2, 3, 3, 2]);
    assert_eq!(w.loops.iter().map(Vec::len).collect::<Vec<_>>(), [4, 1, 1]);
    // The seam: both fins on the wall, opposite senses, each resolving to its own use.
    let seam = body.edges().nth(2).unwrap();
    let wall = body.faces().next().unwrap().id();
    assert_eq!(
        seam.fins().collect::<Vec<_>>(),
        [
            Fin::new(wall, Sense::Forward),
            Fin::new(wall, Sense::Reversed)
        ]
    );
    let positions: Vec<_> = seam.uses().map(scx_brep::Use::position).collect();
    assert_eq!(positions, [1, 3]);
    // A closed edge starts and ends at its one vertex.
    let rim = body.edges().next().unwrap();
    assert_eq!(rim.start().id(), rim.end().id());
    assert_eq!(rim.start().edges().count(), 2);
}

#[test]
fn a_rewritten_face_leaves_the_original_body_alone() {
    let (body, before) = check_body(block());
    let mut d = body.edit();
    let first = body.faces().next().unwrap().id();
    let face = d.face_mut(first).unwrap();
    face.loops[0].uses.reverse();
    for u in &mut face.loops[0].uses {
        u.sense = u.sense.reversed();
    }
    // The fins follow by hand.
    let edges: Vec<_> = d
        .face(first)
        .unwrap()
        .uses()
        .map(|u| u.record().edge)
        .collect();
    for e in edges {
        for fin in &mut d.adjacency_mut(e).unwrap().fins {
            if fin.face == first {
                fin.sense = fin.sense.reversed();
            }
        }
    }
    assert_eq!(walk(&body), before);
    // Turned over with its sides and surface left as they were, the face disagrees with
    // its neighbours about which shell lies between them.
    let refused = d.finish(&Document::default()).unwrap_err();
    assert_eq!(refused.name(), "sides_disagree");
    // A rewrite that keeps the face a face: its loop starts at another use.
    let mut d = body.edit();
    d.face_mut(first).unwrap().loops[0].uses.rotate_left(1);
    assert_eq!(walk(&body), before);
    let (edited, _) = d.finish(&Document::default()).unwrap();
    let after = edited.face(first).unwrap().uses().next().unwrap();
    assert_ne!(
        after.record(),
        body.face(first).unwrap().uses().next().unwrap().record()
    );
}

fn holds(b: Extent3d, p: Point3) -> bool {
    let (lo, hi) = (b.min(), b.max());
    (lo.x..=hi.x).contains(&p.x) && (lo.y..=hi.y).contains(&p.y) && (lo.z..=hi.z).contains(&p.z)
}

/// Points of each face, from a grid over its uv box kept where the face holds them, each
/// checked to lie in the face's box; how many.
fn sample_faces(body: &Brep) -> usize {
    let mut n = 0;
    for face in body.faces() {
        let bx = face.bounding_box().unwrap();
        let uv = face.uv_box().unwrap();
        let (lo, hi) = (uv.min(), uv.max());
        for i in 0..=20 {
            for j in 0..=20 {
                let p = Point2::new(
                    lo.x + (hi.x - lo.x) * f64::from(i) / 20.0,
                    lo.y + (hi.y - lo.y) * f64::from(j) / 20.0,
                );
                match face.contains_uv(p) {
                    Some(UvSide::Inside | UvSide::Near) => {
                        let q = face.surface().point(p);
                        assert!(holds(bx, q), "{q:?} of {} outside {bx:?}", face.id());
                        n += 1;
                    }
                    Some(UvSide::Outside) => {}
                    None => panic!("{} cannot tell {p:?}", face.id()),
                }
            }
        }
    }
    n
}

/// Every face's box, every edge's box and the body's, as bits.
fn box_bits(body: &Brep) -> Vec<[u64; 6]> {
    let bits = |b: Option<Extent3d>| {
        let b = b.unwrap();
        let (lo, hi) = (b.min(), b.max());
        [lo.x, lo.y, lo.z, hi.x, hi.y, hi.z].map(f64::to_bits)
    };
    body.faces()
        .map(|f| bits(f.bounding_box()))
        .chain(body.edges().map(|e| bits(e.bounding_box())))
        .chain([bits(body.bounding_box())])
        .collect()
}

#[test]
fn a_block_s_boxes_hold_it_closely() {
    let (body, _) = check_body(block());
    let zone = body.resolution().zone().mm();
    let b = body.bounding_box().unwrap();
    // Each side grown by the resolution and no more than a rounding past it.
    for off in [
        -b.min().x,
        -b.min().y,
        -b.min().z,
        b.max().x - 1.0,
        b.max().y - 1.0,
        b.max().z - 1.0,
    ] {
        assert!(off >= zone && off < zone + 1e-12, "{off} against {zone}");
    }
    assert!(sample_faces(&body) > 6 * 400);
    for face in body.faces() {
        assert!(face.accuracy().is_none());
        let uv = face.uv_box().unwrap();
        assert_eq!(face.contains_uv(uv.mid()), Some(UvSide::Inside));
    }
    // The faces near a corner are the three that meet there, in id order.
    let corner = Extent3d::from_bounds(-0.1, -0.1, -0.1, 0.1, 0.1, 0.1).unwrap();
    let near: Vec<_> = body.faces_meeting(corner).map(scx_brep::Face::id).collect();
    let brute: Vec<_> = body
        .faces()
        .filter(|f| f.bounding_box().unwrap().intersect(corner).is_some())
        .map(scx_brep::Face::id)
        .collect();
    assert_eq!(near, brute);
    assert_eq!(near.len(), 3);
    let far = Extent3d::from_bounds(5.0, 5.0, 5.0, 6.0, 6.0, 6.0).unwrap();
    assert_eq!(body.faces_meeting(far).len(), 0);
}

#[test]
fn a_cylinder_s_boxes_hold_it() {
    let (body, _) = check_body(cylinder());
    let b = body.bounding_box().unwrap();
    assert!(b.min().x <= -1.0 && b.max().y >= 1.0 && b.min().z <= 0.0 && b.max().z >= 2.0);
    // Certified, not exact: the round sides stand off by under a quarter of the radius.
    assert!(b.max().x < 1.25 && b.min().y > -1.25 && b.max().z < 2.0 + 1e-4);
    assert!(sample_faces(&body) > 400);
    let wall = body.faces().next().unwrap();
    let bx = wall.bounding_box().unwrap();
    for i in 0..64 {
        let t = f64::from(i) * TAU / 64.0;
        for z in [0.0, 0.7, 2.0] {
            assert!(holds(bx, Point3::new(fmath::cos(t), fmath::sin(t), z)));
        }
    }
    // Round the seam: every lift of a point of the wall is inside it.
    let p = Point2::new(1.0 + 2.0 * TAU, 1.0);
    assert_eq!(wall.contains_uv(p), Some(UvSide::Inside));
    assert_eq!(
        wall.contains_uv(Point2::new(1.0, 2.5)),
        Some(UvSide::Outside)
    );
    for e in body.edges() {
        assert!(e.bounding_box().is_some());
    }
}

#[test]
fn cells_fill_the_same_bits_on_every_thread() {
    for d in [block(), cylinder()] {
        let (body, _) = d.finish(&Document::default()).unwrap();
        let want = box_bits(&body);
        // Fresh cells on two threads at once, and one body's cells raced by two threads.
        let fresh: Vec<_> = (0..2)
            .map(|_| {
                let b = body.edit().finish(&Document::default()).unwrap().0;
                std::thread::spawn(move || box_bits(&b))
            })
            .collect();
        let (shared, _) = body.edit().finish(&Document::default()).unwrap();
        std::thread::scope(|s| {
            let racers: Vec<_> = (0..2).map(|_| s.spawn(|| box_bits(&shared))).collect();
            for r in racers {
                assert_eq!(r.join().unwrap(), want);
            }
        });
        for f in fresh {
            assert_eq!(f.join().unwrap(), want);
        }
        // Shed and filled again, the same.
        let mut again = body.clone();
        again.shed_cells();
        assert_eq!(box_bits(&again), want);
    }
}
