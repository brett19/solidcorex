// Written for solidcorex. Apache-2.0.
//! `Draft::transform` on every generated body (research 63 §6.5 BR6): the identity, rigid
//! motions, uniform scales and mirrors finish, move every point as the map does, keep
//! their boxes around the result and report every record as replaced in its geometry; a
//! map back undoes them, to the bit where the edits are exact; a surface of revolution under
//! a map that is not a similarity is refused by name.

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use std::collections::BTreeMap;

use scx_brep::{
    Brep, Carry, Changes, Document, EdgeShape, FaceSide, Touch, TransformError, UseGeom, carry,
};
use scx_num::error::ErrorKind;
use scx_num::extent3d::Extent3d;
use scx_num::math::Affine;
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::{Kind, generate};

fn doc() -> Document {
    Document::default()
}

fn body(kind: Kind, seed: u64, size: usize) -> Brep {
    let g = generate(kind, seed, size).unwrap();
    g.draft.finish(&doc()).unwrap().0
}

fn moved(b: &Brep, a: &Affine) -> (Brep, Changes) {
    let mut d = b.edit();
    d.transform(a, &doc()).unwrap();
    d.finish(&doc())
        .unwrap_or_else(|u| panic!("finish after the map: {}", u.cause))
}

fn rigid() -> Affine {
    let turn =
        Affine::rotation(Point3::new(0.3, -1.2, 0.7), Vec3::new(1.0, 2.0, -0.5), 0.9).unwrap();
    Affine::translation(Vec3::new(4.0, -3.5, 12.25)) * turn
}

fn scale(k: f64) -> Affine {
    Affine::uniform_scaling(Point3::new(1.5, -0.25, 2.0), k).unwrap()
}

fn mirror() -> Affine {
    Affine::reflection(Point3::new(0.5, 1.0, -2.0), Vec3::new(0.3, -1.0, 0.8)).unwrap()
}

/// Every point of the body to compare: vertices, and nine samples along each edge.
fn samples(b: &Brep) -> Vec<Point3> {
    let mut out: Vec<Point3> = b.vertices().map(scx_brep::Vertex::point).collect();
    for e in b.edges() {
        if let EdgeShape::Curve { curve, interval } = e.shape() {
            for i in 0..=8 {
                let t = interval.min() + (interval.max() - interval.min()) * f64::from(i) / 8.0;
                out.push(curve.point(t));
            }
        }
    }
    out
}

fn holds(b: Extent3d, p: Point3) -> bool {
    let (lo, hi) = (b.min(), b.max());
    (lo.x..=hi.x).contains(&p.x) && (lo.y..=hi.y).contains(&p.y) && (lo.z..=hi.z).contains(&p.z)
}

/// What every map but the identity must give: the same topology, each point where the map
/// takes it, boxes around the moved body, and every record replaced in its geometry.
fn check_moved(kind: Kind, b: &Brep, a: &Affine, tol: f64) {
    let (m, changes) = moved(b, a);
    let name = kind.name();
    assert_eq!(
        (m.faces().len(), m.edges().len(), m.vertices().len()),
        (b.faces().len(), b.edges().len(), b.vertices().len()),
        "{name}"
    );
    // Each point (vertices, edge samples) is the old one's image.
    let before = samples(b);
    let after = samples(&m);
    let reach = tol
        * (1.0
            + before
                .iter()
                .map(|p| p.to_vec().length())
                .fold(0.0, f64::max));
    for (p, q) in before.iter().zip(&after) {
        assert!(
            (a.apply(*p) - *q).length() <= reach,
            "{name}: {p:?} -> {q:?}"
        );
    }
    // Each use's pcurve lies on its moved edge: S'(p(t)) = C'(t).
    for f in m.faces() {
        for l in f.loops() {
            for u in l.uses() {
                let rec = u.record();
                if let (UseGeom::Pcurve { curve, interval }, EdgeShape::Curve { curve: c, .. }) =
                    (&rec.geom, u.edge().shape())
                {
                    for i in 0..=4 {
                        let t =
                            interval.min() + (interval.max() - interval.min()) * f64::from(i) / 4.0;
                        let on = f.surface().point(curve.point(t));
                        assert!((on - c.point(t)).length() <= reach, "{name}: {:?}", f.id());
                    }
                }
            }
        }
    }
    // The boxes after the map still hold the body: the body's box every sample, each
    // edge's box its own samples, each face's box its edges' samples.
    let bbox = m.bounding_box().unwrap();
    for p in &after {
        assert!(holds(bbox, *p), "{name}: body box");
    }
    for e in m.edges() {
        let eb = e.bounding_box().unwrap();
        if let EdgeShape::Curve { curve, interval } = e.shape() {
            for i in 0..=16 {
                let t = interval.min() + (interval.max() - interval.min()) * f64::from(i) / 16.0;
                assert!(holds(eb, curve.point(t)), "{name}: edge box");
            }
        }
        for v in [e.start(), e.end()] {
            assert!(holds(eb, v.point()), "{name}: edge box at its vertex");
        }
    }
    for f in m.faces() {
        let fb = f.bounding_box().unwrap();
        for l in f.loops() {
            for u in l.uses() {
                if let UseGeom::Pcurve { curve, interval } = &u.record().geom {
                    for i in 0..=8 {
                        let t =
                            interval.min() + (interval.max() - interval.min()) * f64::from(i) / 8.0;
                        assert!(
                            holds(fb, f.surface().point(curve.point(t))),
                            "{name}: face box"
                        );
                    }
                }
            }
        }
    }
    // Under a mirror each face's sides swap and each edge's fins reverse.
    let mirrored = {
        let (x, y, z) = (a.x_axis(), a.y_axis(), a.z_axis());
        x.dot(y.cross(z)) < 0.0
    };
    for (f, g) in b.faces().zip(m.faces()) {
        let sides =
            |f: scx_brep::Face<'_>| [FaceSide::Front, FaceSide::Back].map(|s| f.shell(s).id());
        let mut want = sides(f);
        if mirrored {
            want.reverse();
        }
        assert_eq!(sides(g), want, "{name}: sides");
    }
    for (e, g) in b.edges().zip(m.edges()) {
        let mut want: Vec<_> = e.fins().collect();
        if mirrored {
            want.reverse();
        }
        assert_eq!(g.fins().collect::<Vec<_>>(), want, "{name}: fins");
    }
    // Every record is replaced in its geometry; nothing else happened.
    assert_eq!(changes.faces.replaced.len(), b.faces().len(), "{name}");
    // A collapsed edge has no geometry to move: it is replaced only where its bound moves.
    let curved = b
        .edges()
        .filter(|e| matches!(e.shape(), EdgeShape::Curve { .. }))
        .count();
    assert!(changes.edges.replaced.len() >= curved, "{name}");
    assert_eq!(
        changes.vertices.replaced.len(),
        b.vertices().len(),
        "{name}"
    );
    assert!(all_geometry(&changes.faces.replaced), "{name}");
    assert!(all_geometry(&changes.edges.replaced), "{name}");
    assert!(all_geometry(&changes.vertices.replaced), "{name}");
    assert!(changes.faces.created.is_empty() && changes.faces.removed.is_empty());
    // Carry keeps every value across it.
    let own: BTreeMap<_, u32> = b.faces().map(|f| (f.id(), f.id().index() as u32)).collect();
    assert_eq!(
        carry(&changes.faces, &own, &[], &Keep),
        own,
        "{name}: carry"
    );
}

fn all_geometry<I>(r: &[(I, Touch)]) -> bool {
    r.iter().all(|(_, t)| *t == Touch::Geometry)
}

struct Keep;

impl Carry<u32> for Keep {
    fn split(&self, from: &u32) -> Option<u32> {
        Some(*from)
    }
    fn merge(&self, values: &[&u32]) -> Option<u32> {
        values.first().map(|v| **v)
    }
    fn coincident(&self, onto: Option<&u32>, _: &u32) -> Option<u32> {
        onto.copied()
    }
    fn offset(&self, from: &u32) -> Option<u32> {
        Some(*from)
    }
}

#[test]
fn every_body_moves_under_rigid_scale_and_mirror() {
    for kind in Kind::ALL {
        for seed in [0, 7] {
            let b = body(kind, seed, 2);
            for (a, tol) in [
                (rigid(), 1e-12),
                (scale(2.5), 1e-12),
                (scale(0.375), 1e-12),
                (mirror(), 1e-12),
                (mirror() * rigid(), 1e-12),
            ] {
                check_moved(kind, &b, &a, tol);
            }
        }
    }
}

#[test]
fn the_identity_writes_nothing() {
    for kind in Kind::ALL {
        let b = body(kind, 3, 2);
        let (m, changes) = moved(&b, &Affine::IDENTITY);
        assert!(changes.is_empty(), "{}", kind.name());
        for (f, g) in b.faces().zip(m.faces()) {
            assert_eq!(f.record().surface, g.record().surface);
            assert_eq!(f.record().loops, g.record().loops);
        }
    }
}

/// The records of two bodies, compared field by field: surfaces, sides, loops, edge
/// shapes and accuracies, fins, points and vertex accuracies.
fn same_records(b: &Brep, c: &Brep) -> bool {
    b.faces().zip(c.faces()).all(|(f, g)| {
        let (f, g) = (f.record(), g.record());
        f.surface == g.surface && f.sides == g.sides && f.loops == g.loops
    }) && b.edges().zip(c.edges()).all(|(e, g)| {
        e.shape() == g.shape() && e.accuracy() == g.accuracy() && e.fins().eq(g.fins())
    }) && b
        .vertices()
        .zip(c.vertices())
        .all(|(v, w)| v.point() == w.point() && v.accuracy() == w.accuracy())
}

#[test]
fn a_map_and_its_inverse_return_the_body() {
    for kind in Kind::ALL {
        let b = body(kind, 5, 2);
        for a in [rigid(), scale(2.5), mirror()] {
            let back = a.inverse().unwrap();
            let (m, _) = moved(&b, &a);
            let (r, _) = moved(&m, &back);
            let reach = 1e-12 * 100.0;
            for (p, q) in samples(&b).iter().zip(&samples(&r)) {
                assert!(
                    (*p - *q).length() <= reach,
                    "{}: {p:?} vs {q:?}",
                    kind.name()
                );
            }
        }
    }
}

#[test]
fn exact_maps_return_the_body_to_the_bit() {
    // On the canonical bodies (round sizes; the planes of the prism, holed and stack kinds
    // on no world axis) a mirror in a coordinate plane through the origin, a scale by a power
    // of two about the origin and a quarter turn about a coordinate axis are exact, and so
    // is every record after the map and its inverse, planes' normals included (FU1).
    let o = Point3::new(0.0, 0.0, 0.0);
    let quadruple = Affine::uniform_scaling(o, 4.0).unwrap();
    let flip = Affine::reflection(o, Vec3::new(0.0, 1.0, 0.0)).unwrap();
    let quarter = Affine::from_axes(Vec3::Y, -Vec3::X, Vec3::Z, o);
    // Up first: a scale down takes a `None` bound to half the zone, which stays `None`, and
    // the way back up then widens it past the zone (`bounds_scale_with_the_map`).
    let double_flip =
        Affine::uniform_scaling(o, 2.0).unwrap() * Affine::reflection(o, Vec3::Z).unwrap();
    for kind in Kind::ALL {
        let b = body(kind, 0, 2);
        for a in [flip, quadruple, quarter, double_flip] {
            let (m, _) = moved(&b, &a);
            let (r, _) = moved(&m, &a.inverse().unwrap());
            assert!(same_records(&b, &r), "{}: {a:?} and back", kind.name());
        }
    }
}

#[test]
fn bounds_scale_with_the_map() {
    let zone = doc().resolution().zone().mm();
    let b = body(Kind::Block, 1, 1);
    assert!(b.edges().all(|e| e.accuracy().is_none()));
    let (m, _) = moved(&b, &scale(8.0));
    for e in m.edges() {
        assert_eq!(e.accuracy().map(|r| r.acc.length().mm()), Some(8.0 * zone));
    }
    for v in m.vertices() {
        assert_eq!(v.accuracy().map(|r| r.acc.length().mm()), Some(8.0 * zone));
    }
    // Back down to the resolution: within it again.
    let (r, _) = moved(&m, &scale(8.0).inverse().unwrap());
    assert!(r.edges().all(|e| e.accuracy().is_none()));
    assert!(r.vertices().all(|v| v.accuracy().is_none()));
    // A rigid motion leaves every bound as it was.
    let (r, _) = moved(&b, &rigid());
    assert!(r.edges().all(|e| e.accuracy().is_none()));
}

#[test]
fn a_planar_body_takes_any_regular_map() {
    let shear = Affine::from_axes(
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.6, 1.3, 0.0),
        Vec3::new(0.2, -0.4, 0.7),
        Point3::new(1.0, 2.0, 3.0),
    );
    for kind in [
        Kind::Block,
        Kind::Prism,
        Kind::Holed,
        Kind::Cavity,
        Kind::Stack,
        Kind::QuadSheet,
    ] {
        let b = body(kind, 2, 2);
        check_moved(kind, &b, &shear, 1e-12);
    }
}

#[test]
fn a_surface_of_revolution_refuses_a_map_not_similar() {
    let stretch = Affine::from_axes(
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Point3::new(0.0, 0.0, 0.0),
    );
    for kind in [Kind::Cylinder, Kind::Cone, Kind::Sphere, Kind::Torus] {
        let b = body(kind, 4, 2);
        let mut d = b.edit();
        let e = d.transform(&stretch, &doc()).unwrap_err();
        assert!(
            matches!(e, TransformError::Surface { .. }),
            "{}: {e}",
            kind.name()
        );
        assert_eq!(
            (e.kind(), e.name()),
            (ErrorKind::Unsupported, "map_not_similar")
        );
        // The draft is unchanged.
        let (_, changes) = d.finish(&doc()).unwrap();
        assert!(changes.is_empty(), "{}", kind.name());
    }
}

#[test]
fn a_map_not_regular_or_not_finite_is_refused() {
    let b = body(Kind::Block, 1, 1);
    let flat = Affine::from_axes(Vec3::X, Vec3::X, Vec3::Z, Point3::new(0.0, 0.0, 0.0));
    let nan = Affine::translation(Vec3::new(f64::NAN, 0.0, 0.0));
    let mut d = b.edit();
    assert_eq!(d.transform(&flat, &doc()), Err(TransformError::MapSingular));
    assert_eq!(d.transform(&nan, &doc()), Err(TransformError::MapNotFinite));
    assert_eq!(TransformError::MapSingular.kind(), ErrorKind::Refused);
    assert!(d.finish(&doc()).unwrap().1.is_empty());
}
