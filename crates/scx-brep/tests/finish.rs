// Written for solidcorex. Apache-2.0.
//! `Draft::finish` against valid and broken bodies: valid ones finish, and each broken
//! invariant is refused with its cause, the same cause on every run and the lowest one
//! where there are several.
//!
//! The structure check is held to the plain-data oracle in `scx_testkit::topology` (research
//! 63 T7): on every body here, `finish` refuses the structure exactly when the oracle finds
//! a violation, and the cause is among the violations it finds.

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

use scx_brep::{
    Document, Draft, EdgeGeom, EdgeId, EdgeShape, EdgeUse, FaceId, FaceRecord, Fin, FinishCause,
    GeometryCause, LoopRecord, RegionId, ShellId, ShellRecord, StructureCause, UseGeom, VertexId,
    VertexRecord,
};
use scx_curve::{AngleLine2, Circle, Curve, Curve2, Line, Line2};
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Cone, Cylinder, Plane, Sphere, Surface, pcurve_of};
use scx_testkit::topology::{self, Violation};

const TOL: Length = Length::from_mm(1e-9);

fn ext(a: f64, b: f64) -> Extent1d {
    Extent1d::new(a, b).unwrap()
}

fn pcurve(edge: &Curve, interval: Extent1d, surface: &Surface) -> UseGeom {
    UseGeom::Pcurve {
        curve: pcurve_of(edge, surface, interval, TOL).unwrap(),
        interval,
    }
}

/// The same pcurve one period `du` along `u`: the other side of a seam.
fn shifted(geom: &UseGeom, du: f64) -> UseGeom {
    let UseGeom::Pcurve { curve, interval } = geom else {
        panic!("not a pcurve");
    };
    let curve = match curve {
        Curve2::Line(l) => {
            let mut o = l.origin();
            o += Vec2::new(du, 0.0);
            Curve2::Line(Line2::new(o, l.direction()).unwrap())
        }
        Curve2::AngleLine(a) => Curve2::AngleLine(
            AngleLine2::new(
                a.arcs(),
                a.axis(),
                a.start(),
                a.sense(),
                a.other() + du,
                a.rate(),
            )
            .unwrap(),
        ),
        other => panic!("no shift for {other:?}"),
    };
    UseGeom::Pcurve {
        curve,
        interval: *interval,
    }
}

/// The pcurve `(u0 + rate t, v)` over `interval`.
fn uv_line(u0: f64, v: f64, du: f64, dv: f64, interval: Extent1d) -> UseGeom {
    UseGeom::Pcurve {
        curve: Curve2::Line(Line2::new(Point2::new(u0, v), Vec2::new(du, dv)).unwrap()),
        interval,
    }
}

fn line(a: Point3, b: Point3) -> Curve {
    Curve::Line(Line::new(a, b - a).unwrap())
}

fn curve_edge(d: &mut Draft, curve: &Curve, interval: Extent1d, ends: [VertexId; 2]) -> EdgeId {
    let shape = EdgeShape::Curve {
        curve: curve.clone(),
        interval,
    };
    d.add_edge(EdgeGeom::new(shape, ends)).unwrap()
}

fn collapsed_edge(d: &mut Draft, at: VertexId) -> EdgeId {
    d.add_edge(EdgeGeom::new(EdgeShape::Collapsed, [at, at]))
        .unwrap()
}

fn vertex(d: &mut Draft, x: f64, y: f64, z: f64) -> VertexId {
    d.add_vertex(VertexRecord::new(Point3::new(x, y, z)))
        .unwrap()
}

/// The shells of a one-solid body: the outside's, then the solid's.
fn solid_shells(d: &mut Draft) -> [ShellId; 2] {
    let outer = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
    let solid = d.add_region(true).unwrap();
    let inner = d.add_shell(ShellRecord::new(solid)).unwrap();
    [outer, inner]
}

/// The block `[x0, x0 + 1] x [0, 1]^2` between `shells`, each face's front outward.
fn add_block(d: &mut Draft, shells: [ShellId; 2], x0: f64) -> Vec<FaceId> {
    let corner = |i: usize| {
        [
            x0 + (i & 1) as f64,
            ((i >> 1) & 1) as f64,
            ((i >> 2) & 1) as f64,
        ]
    };
    let p = |i: usize| {
        let [x, y, z] = corner(i);
        Point3::new(x, y, z)
    };
    let vertices: Vec<VertexId> = (0..8)
        .map(|i| d.add_vertex(VertexRecord::new(p(i))).unwrap())
        .collect();
    let unit = ext(0.0, 1.0);
    let mut edges: Vec<((usize, usize), EdgeId, Curve)> = Vec::new();
    let mut faces = Vec::new();
    for axis in 0..3 {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        for side in 0..2 {
            let mut n = [0.0; 3];
            n[axis] = if side == 1 { 1.0 } else { -1.0 };
            let mut x = [0.0; 3];
            x[u] = 1.0;
            let mut o = [x0, 0.0, 0.0];
            o[axis] += side as f64;
            let frame = Frame::new(
                Point3::new(o[0], o[1], o[2]),
                Vec3::new(n[0], n[1], n[2]),
                Vec3::new(x[0], x[1], x[2]),
            )
            .unwrap();
            let surface = Surface::Plane(Plane::new(frame));
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
                let found = edges.iter().find(|(k, _, _)| *k == key);
                let (edge, curve) = if let Some((_, e, c)) = found {
                    (*e, c.clone())
                } else {
                    let c = line(p(key.0), p(key.1));
                    let e = curve_edge(d, &c, unit, [vertices[key.0], vertices[key.1]]);
                    edges.push((key, e, c.clone()));
                    (e, c)
                };
                let sense = if a < b {
                    Sense::Forward
                } else {
                    Sense::Reversed
                };
                uses.push(EdgeUse::new(edge, sense, pcurve(&curve, unit, &surface)));
            }
            faces.push(
                d.add_face(FaceRecord::new(
                    surface,
                    shells,
                    vec![LoopRecord::new(uses)],
                ))
                .unwrap(),
            );
        }
    }
    faces
}

fn block() -> (Draft, Vec<FaceId>) {
    let mut d = Draft::new();
    let shells = solid_shells(&mut d);
    let faces = add_block(&mut d, shells, 0.0);
    (d, faces)
}

/// A closed cylinder of radius 1 about z from 0 to 2, its circles and side running
/// `turns` times round.
struct Cyl {
    d: Draft,
    side: FaceId,
    bottom_cap: FaceId,
    bottom: VertexId,
}

fn cylinder(turns: f64) -> Cyl {
    let mut d = Draft::new();
    let [outer, inner] = solid_shells(&mut d);
    let r = Length::from_mm(1.0);
    let bottom = vertex(&mut d, 1.0, 0.0, 0.0);
    let top = vertex(&mut d, 1.0, 0.0, 2.0);
    let round = ext(0.0, TAU * turns);
    let circle = |z: f64| {
        let frame = Frame::new(Point3::new(0.0, 0.0, z), Vec3::Z, Vec3::X).unwrap();
        Curve::Circle(Circle::new(frame, r, Sense::Forward).unwrap())
    };
    let (c0, c1) = (circle(0.0), circle(2.0));
    let e0 = curve_edge(&mut d, &c0, round, [bottom, bottom]);
    let e1 = curve_edge(&mut d, &c1, round, [top, top]);
    let seam_curve = line(Point3::new(1.0, 0.0, 0.0), Point3::new(1.0, 0.0, 2.0));
    let seam = curve_edge(&mut d, &seam_curve, ext(0.0, 1.0), [bottom, top]);
    let surface = Surface::Cylinder(Cylinder::new(Frame::WORLD, r, Sense::Forward).unwrap());
    let seam_at = |u: f64| uv_line(u, 0.0, 0.0, 2.0, ext(0.0, 1.0));
    let wall = vec![
        EdgeUse::new(e0, Sense::Forward, pcurve(&c0, round, &surface)),
        EdgeUse::new(seam, Sense::Forward, seam_at(TAU * turns)),
        EdgeUse::new(e1, Sense::Reversed, pcurve(&c1, round, &surface)),
        EdgeUse::new(seam, Sense::Reversed, seam_at(0.0)),
    ];
    let side = d
        .add_face(FaceRecord::new(
            surface,
            [outer, inner],
            vec![LoopRecord::new(wall)],
        ))
        .unwrap();
    let mut caps = Vec::new();
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
        caps.push(
            d.add_face(FaceRecord::new(cap, [outer, inner], vec![lp]))
                .unwrap(),
        );
    }
    Cyl {
        d,
        side,
        bottom_cap: caps[0],
        bottom,
    }
}

/// The unit sphere about the origin: one face, bounded by a meridian seam used twice and
/// the two poles' collapsed edges.
struct Ball {
    d: Draft,
    shells: [ShellId; 2],
    seam: EdgeId,
    south: EdgeId,
    north: EdgeId,
}

/// The sphere's loop, counter-clockwise round `[0, 1]^2` in uv seen from outside: the
/// south pole's line, up the seam at `u = 1`, back along the north pole's line, down the
/// seam at `u = 0`.
fn ball_loop(ball: &Ball, seam_geom: &UseGeom) -> Vec<EdgeUse> {
    let unit = ext(0.0, 1.0);
    vec![
        EdgeUse::new(
            ball.south,
            Sense::Forward,
            uv_line(0.0, 0.0, 1.0, 0.0, unit),
        ),
        EdgeUse::new(ball.seam, Sense::Forward, shifted(seam_geom, 1.0)),
        EdgeUse::new(
            ball.north,
            Sense::Reversed,
            uv_line(0.0, 1.0, 1.0, 0.0, unit),
        ),
        EdgeUse::new(ball.seam, Sense::Reversed, seam_geom.clone()),
    ]
}

fn sphere_surface() -> Surface {
    Surface::Sphere(Sphere::new(Frame::WORLD, Length::from_mm(1.0), Sense::Forward).unwrap())
}

/// The seam, `x = cos t, z = sin t` for `t` in `[-pi/2, pi/2]`, and its pcurve at `u = 0`.
fn meridian() -> (Curve, Extent1d, UseGeom) {
    let frame = Frame::new(Point3::new(0.0, 0.0, 0.0), -Vec3::Y, Vec3::X).unwrap();
    let c = Curve::Circle(Circle::new(frame, Length::from_mm(1.0), Sense::Forward).unwrap());
    let over = ext(-FRAC_PI_2, FRAC_PI_2);
    let g = pcurve(&c, over, &sphere_surface());
    (c, over, g)
}

fn sphere_parts() -> Ball {
    let mut d = Draft::new();
    let shells = solid_shells(&mut d);
    let s = vertex(&mut d, 0.0, 0.0, -1.0);
    let n = vertex(&mut d, 0.0, 0.0, 1.0);
    let (c, over, _) = meridian();
    let seam = curve_edge(&mut d, &c, over, [s, n]);
    let south = collapsed_edge(&mut d, s);
    let north = collapsed_edge(&mut d, n);
    Ball {
        d,
        shells,
        seam,
        south,
        north,
    }
}

fn sphere() -> (Ball, FaceId) {
    let mut ball = sphere_parts();
    let (_, _, g) = meridian();
    let lp = ball_loop(&ball, &g);
    let face = ball
        .d
        .add_face(FaceRecord::new(
            sphere_surface(),
            ball.shells,
            vec![LoopRecord::new(lp)],
        ))
        .unwrap();
    (ball, face)
}

/// A cone of base radius 1 at `z = 0` narrowing to its apex at `(0, 0, 1)`, closed by a
/// base disc: the apex a collapsed edge, a generator the seam.
fn cone() -> Draft {
    let mut d = Draft::new();
    let [outer, inner] = solid_shells(&mut d);
    let b = vertex(&mut d, 1.0, 0.0, 0.0);
    let a = vertex(&mut d, 0.0, 0.0, 1.0);
    let r = Length::from_mm(1.0);
    let round = ext(0.0, TAU);
    let base = Curve::Circle(Circle::new(Frame::WORLD, r, Sense::Forward).unwrap());
    let generator = line(Point3::new(1.0, 0.0, 0.0), Point3::new(0.0, 0.0, 1.0));
    let unit = ext(0.0, 1.0);
    let e_base = curve_edge(&mut d, &base, round, [b, b]);
    let e_gen = curve_edge(&mut d, &generator, unit, [b, a]);
    let apex = collapsed_edge(&mut d, a);
    let surface = Surface::Cone(Cone::new(Frame::WORLD, r, -FRAC_PI_4, Sense::Forward).unwrap());
    let seam = pcurve(&generator, unit, &surface);
    let wall = vec![
        EdgeUse::new(e_base, Sense::Forward, pcurve(&base, round, &surface)),
        EdgeUse::new(e_gen, Sense::Forward, shifted(&seam, 1.0)),
        EdgeUse::new(apex, Sense::Reversed, uv_line(0.0, 1.0, 1.0, 0.0, unit)),
        EdgeUse::new(e_gen, Sense::Reversed, seam),
    ];
    d.add_face(FaceRecord::new(
        surface,
        [outer, inner],
        vec![LoopRecord::new(wall)],
    ))
    .unwrap();
    let frame = Frame::new(Point3::new(0.0, 0.0, 0.0), -Vec3::Z, Vec3::X).unwrap();
    let cap = Surface::Plane(Plane::new(frame));
    let lp = LoopRecord::new(vec![EdgeUse::new(
        e_base,
        Sense::Reversed,
        pcurve(&base, round, &cap),
    )]);
    d.add_face(FaceRecord::new(cap, [outer, inner], vec![lp]))
        .unwrap();
    d
}

/// The draft's incidence as the oracle's plain data.
fn incidence(d: &Draft) -> topology::Body {
    fn put<T>(table: &mut Vec<Option<T>>, i: usize, x: T) {
        if table.len() <= i {
            table.resize_with(i + 1, || None);
        }
        table[i] = Some(x);
    }
    let mut b = topology::Body::default();
    for r in d.regions() {
        put(&mut b.regions, r.id().index(), r.is_solid());
    }
    for s in d.shells() {
        let rec = s.record();
        let shell = topology::Shell {
            region: rec.region.index(),
            wires: rec.wires.iter().map(|e| e.index()).collect(),
            lone: rec.lone_vertices.iter().map(|v| v.index()).collect(),
        };
        put(&mut b.shells, s.id().index(), shell);
    }
    for f in d.faces() {
        let rec = f.record();
        let face = topology::Face {
            sides: rec.sides.map(ShellId::index),
            loops: rec
                .loops
                .iter()
                .map(|l| {
                    l.uses
                        .iter()
                        .map(|u| topology::Use {
                            edge: u.edge.index(),
                            forward: u.sense == Sense::Forward,
                        })
                        .collect()
                })
                .collect(),
        };
        put(&mut b.faces, f.id().index(), face);
    }
    for e in d.edges() {
        let g = e.geometry();
        let edge = topology::Edge {
            ends: g.ends.map(VertexId::index),
            collapsed: g.shape == EdgeShape::Collapsed,
            fins: e
                .adjacency()
                .fins
                .iter()
                .map(|f| (f.face.index(), f.sense == Sense::Forward))
                .collect(),
        };
        put(&mut b.edges, e.id().index(), edge);
    }
    for v in d.vertices() {
        let i = v.id().index();
        if b.vertices.len() <= i {
            b.vertices.resize(i + 1, false);
        }
        b.vertices[i] = true;
    }
    b
}

/// The oracle's violations a structure cause stands for.
fn stands_for(cause: &StructureCause, v: &Violation) -> bool {
    let want = match cause {
        StructureCause::Outside => "Outside",
        StructureCause::Dangling { .. } => "Dangling",
        StructureCause::FinWithoutUse { .. }
        | StructureCause::UseWithoutFin { .. }
        | StructureCause::RepeatedFin { .. } => "Radial",
        StructureCause::WireWithFins { .. } => "WireWithFins",
        StructureCause::FaceWithoutLoop { .. } => "FaceWithoutLoop",
        StructureCause::EmptyLoop { .. } => "EmptyLoop",
        StructureCause::LoopNotConnected { .. } => "LoopNotConnected",
        StructureCause::RepeatedUse { .. } => "RepeatedUse",
        StructureCause::VertexOnTwoLoops { .. } => "VertexOnTwoLoops",
        StructureCause::CollapsedEnds { .. } => "CollapsedEnds",
        StructureCause::LooseEdge { .. } => "LooseEdge",
        StructureCause::SidesDisagree { .. } => "SidesDisagree",
        StructureCause::LooseVertex { .. } => "LooseVertex",
        StructureCause::EmptyShell { .. } => "EmptyShell",
        StructureCause::ShellNotConnected { .. } => "ShellNotConnected",
        StructureCause::RegionWithoutShell { .. } => "RegionWithoutShell",
        // A use's geometry of the wrong kind or interval: geometry the oracle does not see.
        _ => return false,
    };
    format!("{v:?}").split([' ', '{']).next() == Some(want)
}

/// Finishes `d`, holding the structure check to the oracle, and gives the cause.
fn outcome(d: Draft) -> Result<(), FinishCause> {
    let found = topology::violations(&incidence(&d));
    let result = d
        .finish(&Document::default())
        .map(|_| ())
        .map_err(|u| u.cause);
    match &result {
        Err(FinishCause::Structure(
            c @ (StructureCause::PointUseOfCurve { .. } | StructureCause::PcurveInterval { .. }),
        )) => assert!(found.is_empty(), "{c}: oracle found {found:?}"),
        Err(FinishCause::Structure(c)) => assert!(
            found.iter().any(|v| stands_for(c, v)),
            "refused for {c}, oracle found {found:?}"
        ),
        _ => assert!(
            found.is_empty(),
            "finish gave {result:?}, oracle found {found:?}"
        ),
    }
    result
}

fn refused(d: Draft) -> &'static str {
    outcome(d).unwrap_err().name()
}

#[test]
fn valid_bodies_finish() {
    for (name, d) in [
        ("block", block().0),
        ("cylinder", cylinder(1.0).d),
        ("sphere", sphere().0.d),
        ("cone", cone()),
    ] {
        let b = incidence(&d);
        for shell in 0..2 {
            assert_eq!(topology::genus(&b, shell), Some(0), "{name}");
        }
        assert_eq!(outcome(d), Ok(()), "{name}");
    }
}

#[test]
fn each_broken_structure_is_refused_with_its_cause() {
    type Edit = fn(&mut Draft, &[FaceId]);
    let cases: [(&str, Edit); 15] = [
        ("outside_region", |d, _| {
            d.region_mut(RegionId::OUTSIDE).unwrap().solid = true;
        }),
        ("dangling_reference", |d, _| {
            let v = d.vertices().next().unwrap().id();
            d.remove_vertex(v);
        }),
        ("fin_without_use", |d, f| {
            let e = d.face(f[1]).unwrap().record().loops[0].uses[0].edge;
            d.adjacency_mut(e)
                .unwrap()
                .fins
                .push(Fin::new(f[5], Sense::Forward));
        }),
        ("wire_with_fins", |d, _| {
            let e = d.edges().next().unwrap().id();
            let s = d.shells().next().unwrap().id();
            d.shell_mut(s).unwrap().wires.push(e);
        }),
        ("face_without_loop", |d, f| {
            d.face_mut(f[0]).unwrap().loops.clear();
            let edges: Vec<EdgeId> = d.edges().map(scx_brep::Edge::id).collect();
            for e in edges {
                d.adjacency_mut(e)
                    .unwrap()
                    .fins
                    .retain(|fin| fin.face != f[0]);
            }
        }),
        ("empty_loop", |d, f| {
            d.face_mut(f[0])
                .unwrap()
                .loops
                .push(LoopRecord::new(vec![]));
        }),
        ("loop_not_connected", |d, f| {
            d.face_mut(f[0]).unwrap().loops[0].uses.swap(0, 1);
        }),
        ("point_use_of_curve", |d, f| {
            d.face_mut(f[0]).unwrap().loops[0].uses[0].geom = UseGeom::Point(Point2::new(0.0, 0.0));
        }),
        ("pcurve_interval", |d, f| {
            if let UseGeom::Pcurve { interval, .. } =
                &mut d.face_mut(f[0]).unwrap().loops[0].uses[0].geom
            {
                *interval = ext(0.0, 0.5);
            }
        }),
        ("use_without_fin", |d, f| {
            let e = d.face(f[0]).unwrap().record().loops[0].uses[0].edge;
            d.adjacency_mut(e).unwrap().fins.clear();
        }),
        ("repeated_fin", |d, f| {
            let e = d.face(f[0]).unwrap().record().loops[0].uses[0].edge;
            let fins = &mut d.adjacency_mut(e).unwrap().fins;
            fins.push(fins[0]);
        }),
        ("collapsed_edge_ends", |d, _| {
            let e = d.edges().next().unwrap().id();
            d.edge_mut(e).unwrap().shape = EdgeShape::Collapsed;
        }),
        ("loose_edge", |d, _| {
            let [a, b] = d.edges().next().unwrap().geometry().ends;
            let c = line(Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 0.0, 0.0));
            curve_edge(d, &c, ext(0.0, 1.0), [a, b]);
        }),
        ("sides_disagree", |d, f| {
            d.face_mut(f[2]).unwrap().sides.swap(0, 1);
        }),
        ("loose_vertex", |d, _| {
            vertex(d, 5.0, 5.0, 5.0);
        }),
    ];
    for (name, edit) in cases {
        let (mut d, faces) = block();
        edit(&mut d, &faces);
        assert_eq!(refused(d), name);
    }

    let (mut d, _) = block();
    d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
    assert_eq!(refused(d), "empty_shell");

    let (mut d, _) = block();
    d.add_region(true).unwrap();
    assert_eq!(refused(d), "region_without_shell");

    // Two blocks between one pair of shells: each shell is two pieces.
    let mut d = Draft::new();
    let shells = solid_shells(&mut d);
    add_block(&mut d, shells, 0.0);
    add_block(&mut d, shells, 3.0);
    let cause = outcome(d).unwrap_err();
    assert_eq!(
        cause,
        FinishCause::Structure(StructureCause::ShellNotConnected { shell: shells[0] })
    );

    // A second loop repeating the cap's circle.
    let mut c = cylinder(1.0);
    let lp = c.d.face(c.bottom_cap).unwrap().record().loops[0].clone();
    c.d.face_mut(c.bottom_cap).unwrap().loops.push(lp);
    assert_eq!(refused(c.d), "repeated_use");

    // A second loop on the cap at a vertex the first loop has: a point use of a collapsed
    // edge there.
    let mut c = cylinder(1.0);
    let e = collapsed_edge(&mut c.d, c.bottom);
    let lp = LoopRecord::new(vec![EdgeUse::new(
        e,
        Sense::Forward,
        UseGeom::Point(Point2::new(1.0, 0.0)),
    )]);
    c.d.face_mut(c.bottom_cap).unwrap().loops.push(lp);
    c.d.adjacency_mut(e)
        .unwrap()
        .fins
        .push(Fin::new(c.bottom_cap, Sense::Forward));
    assert_eq!(refused(c.d), "vertex_on_two_loops");
}

#[test]
fn each_broken_geometry_is_refused_with_its_cause() {
    // A corner moved off its edges' ends.
    let (mut d, _) = block();
    let v = d.vertices().next().unwrap().id();
    d.vertex_mut(v).unwrap().point = Point3::new(1e-3, 0.0, 0.0);
    assert_eq!(refused(d), "edge_end_off_vertex");

    // A wire shorter than its accuracy.
    let mut d = Draft::new();
    let s = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
    let a = vertex(&mut d, 0.0, 0.0, 0.0);
    let b = vertex(&mut d, 1e-6, 0.0, 0.0);
    let c = Curve::Line(Line::new(Point3::new(0.0, 0.0, 0.0), Vec3::X).unwrap());
    let e = curve_edge(&mut d, &c, ext(0.0, 1e-6), [a, b]);
    d.shell_mut(s).unwrap().wires.push(e);
    assert_eq!(refused(d), "edge_too_short");

    // The seam's far copy a millimetre's thousandth along: its ends miss their vertices.
    let mut c = cylinder(1.0);
    c.d.face_mut(c.side).unwrap().loops[0].uses[1].geom =
        uv_line(TAU + 1e-3, 0.0, 0.0, 2.0, ext(0.0, 1.0));
    assert_eq!(refused(c.d), "use_end_off_vertex");

    // The bottom circle's pcurve at twice its speed: right ends, wrong middle.
    let mut c = cylinder(1.0);
    c.d.face_mut(c.side).unwrap().loops[0].uses[0].geom =
        uv_line(0.0, 0.0, 2.0, 0.0, ext(0.0, TAU));
    assert_eq!(refused(c.d), "pcurve_off_edge");

    // At nine times its speed every sample lifts onto the circle, a turn and more apart.
    let mut c = cylinder(1.0);
    c.d.face_mut(c.side).unwrap().loops[0].uses[0].geom =
        uv_line(0.0, 0.0, 9.0, 0.0, ext(0.0, TAU));
    assert_eq!(refused(c.d), "period_jump");

    // A side going twice round: closed and turning the right way, but covering the
    // cylinder twice.
    assert_eq!(refused(cylinder(2.0).d), "face_wraps");

    // The north pole's use as a point past the pole.
    let (mut ball, face) = sphere();
    ball.d.face_mut(face).unwrap().loops[0].uses[2].geom = UseGeom::Point(Point2::new(0.5, 1.5));
    assert_eq!(refused(ball.d), "outside_domain");

    // Both of the seam's uses at u = 0: the loop does not close across the seam.
    let (mut ball, face) = sphere();
    let (_, _, g) = meridian();
    ball.d.face_mut(face).unwrap().loops[0].uses[1].geom = g;
    assert_eq!(refused(ball.d), "loop_open_in_uv");

    // No pole edges: up the seam and straight back down, the poles' lines not in the loop.
    let mut ball = sphere_parts();
    for e in [ball.south, ball.north] {
        ball.d.remove_edge(e);
    }
    let (_, _, g) = meridian();
    let lp = vec![
        EdgeUse::new(ball.seam, Sense::Forward, shifted(&g, 1.0)),
        EdgeUse::new(ball.seam, Sense::Reversed, g),
    ];
    ball.d
        .add_face(FaceRecord::new(
            sphere_surface(),
            ball.shells,
            vec![LoopRecord::new(lp)],
        ))
        .unwrap();
    assert_eq!(refused(ball.d), "loop_open_in_uv");

    // The sphere turned inside out in its records only: sides swapped and the loop run
    // backwards, the surface's normal still outward, so the loop turns clockwise from the
    // front.
    let mut ball = sphere_parts();
    let (_, _, g) = meridian();
    let lp: Vec<EdgeUse> = ball_loop(&ball, &g)
        .into_iter()
        .rev()
        .map(|u| EdgeUse::new(u.edge, u.sense.reversed(), u.geom))
        .collect();
    let [outer, inner] = ball.shells;
    ball.d
        .add_face(FaceRecord::new(
            sphere_surface(),
            [inner, outer],
            vec![LoopRecord::new(lp)],
        ))
        .unwrap();
    assert_eq!(refused(ball.d), "loop_orientation");
}

#[test]
fn the_cause_is_the_lowest_and_the_same_every_run() {
    // Structure before geometry: a loose vertex outranks a moved corner.
    let (mut d, _) = block();
    let v = d.vertices().next().unwrap().id();
    d.vertex_mut(v).unwrap().point = Point3::new(1e-3, 0.0, 0.0);
    vertex(&mut d, 5.0, 5.0, 5.0);
    assert_eq!(refused(d), "loose_vertex");

    // Two loose vertices: the lower is named, on every run.
    let (mut d, _) = block();
    let low = vertex(&mut d, 5.0, 5.0, 5.0);
    vertex(&mut d, 6.0, 6.0, 6.0);
    let first = d.finish(&Document::default()).unwrap_err();
    let want = FinishCause::Structure(StructureCause::LooseVertex { vertex: low });
    assert_eq!(first.cause, want);
    let again = first.draft.finish(&Document::default()).unwrap_err();
    assert_eq!(again.cause, want);

    // Two faces with bad pcurves: the lower face is named.
    let (mut d, faces) = block();
    for f in [faces[4], faces[1]] {
        if let UseGeom::Pcurve { curve, interval } =
            &mut d.face_mut(f).unwrap().loops[0].uses[0].geom
        {
            let start = curve.point(interval.min());
            *curve = Curve2::Line(Line2::new(start, Vec2::new(0.5, 0.5)).unwrap());
        }
    }
    let Err(FinishCause::Geometry(GeometryCause::UseEndOffVertex { face, .. })) = outcome(d) else {
        panic!("not a use's end off its vertex");
    };
    assert_eq!(face, faces[1]);
}
